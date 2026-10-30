//! `/books/authors`: the authors (surname order) in one table, sorted and
//! searched in the page like the books (`static/js/tables.js`), and the
//! books of each.
//! Authors are created and deleted with their books (`store::books`).

use askama::Template;
use axum::extract::{Path, State};

use super::BookRow;
use common::web::error::{AppResult, OrNotFound};
use common::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct AuthorRow {
    id: i64,
    name: String,
    /// `hobb robin`, to sort by surname.
    sort_name: String,
    books: i64,
}

#[derive(Template)]
#[template(path = "books/authors.html")]
pub struct AuthorsList {
    authors: Vec<AuthorRow>,
}

/// `/books/authors`.
pub async fn list(State(state): State<AppState>) -> AppResult<AuthorsList> {
    let authors = sqlx::query_as(
        "SELECT a.id, a.name, a.sort_name,
                (SELECT count(*) FROM book_book_author ba WHERE ba.author_id = a.id) AS books
         FROM book_author a ORDER BY a.sort_name COLLATE NOCASE",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(AuthorsList { authors })
}

#[derive(Template)]
#[template(path = "books/author.html")]
pub struct AuthorPage {
    name: String,
    books: Vec<BookRow>,
}

/// `/books/authors/:id`: every book of the author, by series then year.
pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<AuthorPage> {
    let pool = &state.pool;
    let name: String = sqlx::query_scalar("SELECT name FROM book_author WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()?;
    let books = sqlx::query_as(&format!(
        "{} WHERE book.id IN (SELECT book_id FROM book_book_author WHERE author_id = ?)
         ORDER BY book_series.name IS NULL, book_series.name COLLATE NOCASE, book.volume,
                  book.published_year, book.title COLLATE NOCASE",
        BookRow::SELECT
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;

    Ok(AuthorPage { name, books })
}
