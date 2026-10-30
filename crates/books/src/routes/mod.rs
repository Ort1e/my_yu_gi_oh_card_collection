//! Books: a second collection (novels), with nothing in common with the
//! cards but the app. Every route starts with `/books`. Public reads like
//! the cards; the writes and the ISBN lookup behind the login; the book
//! advent calendars private (`advent/`).
//!
//! `form.rs` adds, edits and deletes books (with the ISBN lookup and the
//! barcode scan of `static/js/book_form.js`), `authors.rs` and `series.rs`
//! list them, `api.rs` is the lookup's JSON. Book writes are in
//! `store::books`, the lookup in `store::book_data`, the pure rules in
//! `books_domain::book` / `books_domain::isbn`.

pub mod advent;
pub mod api;
pub mod authors;
pub mod form;
pub mod series;

use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};

use common::sql::like_pattern;
use common::web::error::{AppResult, OrNotFound};
use common::AppState;

/// A book in a list: its authors joined, its series.
#[derive(Debug, sqlx::FromRow)]
pub struct BookRow {
    pub id: i64,
    pub title: String,
    /// `Neil Gaiman, Terry Pratchett`, in order.
    pub authors: Option<String>,
    pub series: Option<String>,
    pub volume: Option<i64>,
    pub published_year: Option<i64>,
    pub format: Option<String>,
    /// The first author's `sort_name` (`hobb robin`), to sort by surname.
    pub author_sort: Option<String>,
}

impl BookRow {
    /// Selects every column, `FROM book` (join `book_series`): add `WHERE` /
    /// `ORDER BY`.
    pub const SELECT: &'static str = "SELECT book.id, book.title,
            (SELECT group_concat(name, ', ') FROM
                (SELECT a.name FROM book_book_author ba JOIN book_author a ON a.id = ba.author_id
                 WHERE ba.book_id = book.id ORDER BY ba.position)) AS authors,
            book_series.name AS series, book.volume, book.published_year,
            book.format,
            (SELECT a.sort_name FROM book_book_author ba JOIN book_author a ON a.id = ba.author_id
             WHERE ba.book_id = book.id ORDER BY ba.position LIMIT 1) AS author_sort
         FROM book LEFT JOIN book_series ON book_series.id = book.series_id";

    /// `Discworld #3`, `Discworld`, or nothing.
    pub fn series_text(&self) -> String {
        series_text(self.series.as_deref(), self.volume)
    }

    pub fn authors_text(&self) -> &str {
        self.authors.as_deref().unwrap_or("")
    }

    /// To sort by surname, then series and volume, in the list's table.
    pub fn author_sort_value(&self) -> String {
        match &self.author_sort {
            Some(name) => format!("{name} {}", self.series_sort_value()),
            None => String::new(),
        }
    }

    /// To sort by series then volume (`Farseer 00002`), empty without one.
    pub fn series_sort_value(&self) -> String {
        match &self.series {
            Some(name) => format!("{name} {:05}", self.volume.unwrap_or(0)),
            None => String::new(),
        }
    }
}

pub fn series_text(series: Option<&str>, volume: Option<i64>) -> String {
    match (series, volume) {
        (Some(name), Some(volume)) => format!("{name} #{volume}"),
        (Some(name), None) => name.to_string(),
        (None, _) => String::new(),
    }
}

/// The list's filters (all "contains", case-insensitive) and sort.
#[derive(Debug, Default, Deserialize)]
pub struct BookFilter {
    pub title: Option<String>,
    pub author: Option<String>,
    pub series: Option<String>,
    /// `title` (default), `author`, `added` (latest first).
    pub sort: Option<String>,
}

impl BookFilter {
    fn text(value: &Option<String>) -> Option<&str> {
        value.as_deref().map(str::trim).filter(|v| !v.is_empty())
    }

    fn push_where(&self, qb: &mut QueryBuilder<'_, Sqlite>) {
        qb.push(" WHERE 1");
        if let Some(title) = Self::text(&self.title) {
            qb.push(" AND book.title LIKE ")
                .push_bind(like_pattern(title))
                .push(" ESCAPE '\\'");
        }
        if let Some(author) = Self::text(&self.author) {
            qb.push(
                " AND book.id IN (SELECT ba.book_id FROM book_book_author ba
                    JOIN book_author a ON a.id = ba.author_id WHERE a.name LIKE ",
            )
            .push_bind(like_pattern(author))
            .push(" ESCAPE '\\')");
        }
        if let Some(series) = Self::text(&self.series) {
            qb.push(" AND book_series.name LIKE ")
                .push_bind(like_pattern(series))
                .push(" ESCAPE '\\'");
        }
    }

    fn order_by(&self) -> &'static str {
        match self.sort.as_deref() {
            Some("author") => {
                " ORDER BY (SELECT a.sort_name FROM book_book_author ba
                    JOIN book_author a ON a.id = ba.author_id
                    WHERE ba.book_id = book.id ORDER BY ba.position LIMIT 1) IS NULL,
                 (SELECT a.sort_name FROM book_book_author ba
                    JOIN book_author a ON a.id = ba.author_id
                    WHERE ba.book_id = book.id ORDER BY ba.position LIMIT 1),
                 book_series.name, book.volume, book.published_year, book.title"
            }
            Some("added") => " ORDER BY book.added_on DESC, book.id DESC",
            _ => " ORDER BY book.title COLLATE NOCASE, book.id",
        }
    }
}

/// Every book in one table (no pages): the search field and the column
/// headings filter and sort it in the page (`static/js/tables.js`). The
/// filters of the address (`?author=hobb`, `?sort=author`) still apply.
#[derive(Template)]
#[template(path = "books/list.html")]
pub struct BooksList {
    total: i64,
    books: Vec<BookRow>,
}

impl BooksList {
    /// Fewer books than in the collection (filters in the address).
    fn filtered(&self) -> bool {
        self.books.len() as i64 != self.total
    }
}

/// `/books`.
pub async fn list(
    State(state): State<AppState>,
    Query(filter): Query<BookFilter>,
) -> AppResult<BooksList> {
    let pool = &state.pool;
    let mut select = QueryBuilder::new(BookRow::SELECT);
    filter.push_where(&mut select);
    select.push(filter.order_by());
    let books = select.build_query_as().fetch_all(pool).await?;

    Ok(BooksList {
        total: sqlx::query_scalar("SELECT count(*) FROM book")
            .fetch_one(pool)
            .await?,
        books,
    })
}

/// Everything about one book, for its page.
#[derive(Debug, sqlx::FromRow)]
pub struct BookDetailRow {
    id: i64,
    isbn: Option<String>,
    title: String,
    publisher: Option<String>,
    published_year: Option<i64>,
    language: Option<String>,
    page_count: Option<i64>,
    series_id: Option<i64>,
    series: Option<String>,
    volume: Option<i64>,
    format: Option<String>,
    notes: Option<String>,
    added_on: String,
}

#[derive(Template)]
#[template(path = "books/detail.html")]
pub struct BookDetail {
    book: BookDetailRow,
    /// `(id, name)`, in order.
    authors: Vec<(i64, String)>,
    /// The advent calendar days it was behind: `(year, day)`.
    calendar_days: Vec<(i64, i64)>,
    /// The volumes of its series, this one included, by volume.
    series_books: Vec<BookRow>,
    /// Its series' missing volumes (`books_domain::book::missing_volumes`).
    missing: Vec<i64>,
    /// The other books of its authors, not of its series, latest first.
    other_books: Vec<BookRow>,
}

impl BookDetail {
    /// The language in words (`English`), or as stored.
    fn language_text(&self) -> String {
        let code = self.book.language.as_deref().unwrap_or("");
        books_domain::book::language_name(code)
            .map(str::to_string)
            .unwrap_or_else(|| code.to_string())
    }

    /// Its cover at Open Library (by ISBN, shown from there: never stored);
    /// none in the public copy, where ISBNs aren't shown.
    fn cover_url(&self) -> Option<String> {
        if common::export::public_copy() {
            return None;
        }
        let isbn = self.book.isbn.as_deref()?;
        Some(format!(
            "https://covers.openlibrary.org/b/isbn/{isbn}-M.jpg?default=false"
        ))
    }

    /// "Origin", "Dan Brown": the authors on one line.
    fn authors_text(&self) -> String {
        self.authors
            .iter()
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn missing_text(&self) -> String {
        self.missing
            .iter()
            .map(|v| format!("#{v}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The authors of a book, in order: `(id, name)`.
pub async fn book_authors(pool: &common::db::Pool, id: i64) -> sqlx::Result<Vec<(i64, String)>> {
    sqlx::query_as(
        "SELECT a.id, a.name FROM book_book_author ba JOIN book_author a ON a.id = ba.author_id
         WHERE ba.book_id = ? ORDER BY ba.position",
    )
    .bind(id)
    .fetch_all(pool)
    .await
}

/// `/books/:id`.
pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<BookDetail> {
    let pool = &state.pool;
    let book: BookDetailRow = sqlx::query_as(
        "SELECT book.id, book.isbn, book.title, book.publisher, book.published_year,
                book.language, book.page_count, book.series_id, book_series.name AS series,
                book.volume, book.format, book.notes, book.added_on
         FROM book LEFT JOIN book_series ON book_series.id = book.series_id
         WHERE book.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .or_404()?;

    let series_books: Vec<BookRow> = match book.series_id {
        Some(series_id) => {
            sqlx::query_as(&format!(
                "{} WHERE book.series_id = ?
                 ORDER BY book.volume IS NULL, book.volume, book.published_year, book.title",
                BookRow::SELECT
            ))
            .bind(series_id)
            .fetch_all(pool)
            .await?
        }
        None => Vec::new(),
    };
    let volumes: Vec<i64> = series_books.iter().filter_map(|b| b.volume).collect();
    let other_books = sqlx::query_as(&format!(
        "{} WHERE book.id <> ?1
             AND book.id IN (SELECT ba.book_id FROM book_book_author ba WHERE ba.author_id IN
                 (SELECT author_id FROM book_book_author WHERE book_id = ?1))
             AND ((SELECT series_id FROM book WHERE id = ?1) IS NULL
                  OR book.series_id IS NOT (SELECT series_id FROM book WHERE id = ?1))
         ORDER BY book.published_year IS NULL, book.published_year DESC, book.title",
        BookRow::SELECT
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;

    let mut conn = pool.acquire().await?;
    Ok(BookDetail {
        book,
        authors: book_authors(pool, id).await?,
        calendar_days: crate::store::books::calendar_days(&mut conn, id).await?,
        missing: books_domain::book::missing_volumes(&volumes),
        series_books,
        other_books,
    })
}
