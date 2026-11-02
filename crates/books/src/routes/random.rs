//! `/books/random`: what to read next, a book picked at random among the
//! ones the filters keep: unread, "can start it now" (no unread earlier
//! volume of its series: the volumes not owned don't count), a language, a
//! format. The page carries every book as JSON and `static/js/book_random.js`
//! filters and picks in the browser, so the public copy (static files) has
//! it too. "Another one" never shows the same book twice in a row unless
//! it's the only one left.

use std::collections::HashMap;

use askama::Template;
use axum::extract::State;
use books_domain::book::{language_name, BookFormat};
use serde_json::json;

use super::{cover_url, BookRow};
use common::web::error::AppResult;
use common::web::filters::json_for_script;
use common::AppState;

/// A language of the collection, for the filter.
pub struct LanguageChoice {
    code: String,
    name: String,
}

#[derive(Template)]
#[template(path = "books/random.html")]
pub struct RandomPage {
    languages: Vec<LanguageChoice>,
    /// Every book (`json_for_script`): `{id, href, title, authors, series,
    /// series_id, volume, language, format, pages, read, cover}`.
    books_json: String,
}

impl RandomPage {
    fn formats(&self) -> [BookFormat; 4] {
        BookFormat::ALL
    }
}

#[derive(sqlx::FromRow)]
struct Extra {
    id: i64,
    series_id: Option<i64>,
    language: Option<String>,
    page_count: Option<i64>,
    isbn: Option<String>,
}

/// `GET /books/random`.
pub async fn page(State(state): State<AppState>) -> AppResult<RandomPage> {
    let pool = &state.pool;
    let books: Vec<BookRow> = sqlx::query_as(&format!("{} ORDER BY book.id", BookRow::SELECT))
        .fetch_all(pool)
        .await?;
    let extras: HashMap<i64, Extra> =
        sqlx::query_as::<_, Extra>("SELECT id, series_id, language, page_count, isbn FROM book")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|extra| (extra.id, extra))
            .collect();

    let books: Vec<_> = books
        .iter()
        .filter_map(|book| Some((book, extras.get(&book.id)?)))
        .map(|(book, extra)| {
            json!({
                "id": book.id,
                "href": common::export::href(&format!("/books/{}", book.id)),
                "title": book.title,
                "authors": book.authors_text(),
                "series": book.series_text(),
                "series_id": extra.series_id,
                "volume": book.volume,
                "language": extra.language,
                "format": book.format,
                "pages": extra.page_count,
                "read": book.is_read,
                "cover": cover_url(extra.isbn.as_deref()),
            })
        })
        .collect();

    let codes: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT language FROM book WHERE language IS NOT NULL ORDER BY language",
    )
    .fetch_all(pool)
    .await?;
    let languages = codes
        .into_iter()
        .map(|code| LanguageChoice {
            name: language_name(&code).unwrap_or(&code).to_string(),
            code,
        })
        .collect();

    Ok(RandomPage {
        languages,
        books_json: json_for_script(&serde_json::Value::Array(books)),
    })
}
