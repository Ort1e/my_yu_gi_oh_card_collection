//! Books: a second collection (novels), with nothing in common with the
//! cards but the app. Every route starts with `/books`. Public reads like
//! the cards; the writes and the ISBN lookup behind the login; the book
//! advent calendars private (`advent/`).
//!
//! `form.rs` adds, edits and deletes books (with the ISBN lookup and the
//! barcode scan of `static/js/book_form.js`), `authors.rs` and `series.rs`
//! list them, `api.rs` is the lookup's JSON. Book writes are in
//! `store::books`, the lookup in `store::book_data`, the pure rules in
//! `domain::book` / `domain::isbn`.

pub mod advent;
pub mod api;
pub mod authors;
pub mod form;
pub mod series;

use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};

use crate::routes::cards::like_pattern;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::pagination::{self, Page, PAGE_SIZE};
use crate::AppState;

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
}

impl BookRow {
    /// Selects every column, `FROM book` (join `book_series`): add `WHERE` /
    /// `ORDER BY`.
    pub const SELECT: &'static str = "SELECT book.id, book.title,
            (SELECT group_concat(name, ', ') FROM
                (SELECT a.name FROM book_book_author ba JOIN book_author a ON a.id = ba.author_id
                 WHERE ba.book_id = book.id ORDER BY ba.position)) AS authors,
            book_series.name AS series, book.volume, book.published_year,
            book.format
         FROM book LEFT JOIN book_series ON book_series.id = book.series_id";

    /// `Discworld #3`, `Discworld`, or nothing.
    pub fn series_text(&self) -> String {
        series_text(self.series.as_deref(), self.volume)
    }

    pub fn authors_text(&self) -> &str {
        self.authors.as_deref().unwrap_or("")
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

    fn querystring(&self) -> String {
        pagination::querystring([
            ("title", self.title.as_deref()),
            ("author", self.author.as_deref()),
            ("series", self.series.as_deref()),
            ("sort", self.sort.as_deref()),
        ])
    }

    fn value(value: &Option<String>) -> &str {
        value.as_deref().unwrap_or("")
    }
}

#[derive(Template)]
#[template(path = "books/list.html")]
pub struct BooksList {
    page_title: &'static str,
    page: Page,
    total: i64,
    books: Vec<BookRow>,
    filter: BookFilter,
}

impl BooksList {
    fn sort_is(&self, option: &str) -> bool {
        self.filter.sort.as_deref().unwrap_or("title") == option
    }

    fn title_value(&self) -> &str {
        BookFilter::value(&self.filter.title)
    }

    fn author_value(&self) -> &str {
        BookFilter::value(&self.filter.author)
    }

    fn series_value(&self) -> &str {
        BookFilter::value(&self.filter.series)
    }
}

/// `/books`.
pub async fn list(
    State(state): State<AppState>,
    Query(page): Query<pagination::PageParam>,
    Query(filter): Query<BookFilter>,
) -> AppResult<BooksList> {
    let pool = &state.pool;

    let mut count = QueryBuilder::new(
        "SELECT count(*) FROM book LEFT JOIN book_series ON book_series.id = book.series_id",
    );
    filter.push_where(&mut count);
    let count: i64 = count.build_query_scalar().fetch_one(pool).await?;
    let page = Page::new(page.page, count, filter.querystring());

    let mut select = QueryBuilder::new(BookRow::SELECT);
    filter.push_where(&mut select);
    select
        .push(filter.order_by())
        .push(" LIMIT ")
        .push_bind(PAGE_SIZE)
        .push(" OFFSET ")
        .push_bind(page.offset());
    let books = select.build_query_as().fetch_all(pool).await?;

    Ok(BooksList {
        page_title: "Books",
        page,
        total: sqlx::query_scalar("SELECT count(*) FROM book")
            .fetch_one(pool)
            .await?,
        books,
        filter,
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
}

impl BookDetail {
    fn series_text(&self) -> String {
        series_text(self.book.series.as_deref(), self.book.volume)
    }
}

/// The authors of a book, in order: `(id, name)`.
pub async fn book_authors(
    pool: &crate::store::db::Pool,
    id: i64,
) -> sqlx::Result<Vec<(i64, String)>> {
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
    let book = sqlx::query_as(
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

    let mut conn = pool.acquire().await?;
    Ok(BookDetail {
        book,
        authors: book_authors(pool, id).await?,
        calendar_days: crate::store::books::calendar_days(&mut conn, id).await?,
    })
}
