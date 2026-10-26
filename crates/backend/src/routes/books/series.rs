//! `/books/series`: the series, and the volumes of each with the missing
//! ones (the gaps up to the last volume owned, `books_domain::book::missing_volumes`).
//! Series are created and deleted with their books (`store::books`).

use askama::Template;
use axum::extract::{Path, Query, State};
use books_domain::book::missing_volumes;

use super::BookRow;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::pagination::{Page, PageParam, PAGE_SIZE};
use crate::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct SeriesRow {
    id: i64,
    name: String,
    books: i64,
    /// The volume numbers owned, comma-separated (`1,2,4`).
    volumes: Option<String>,
    /// The first author of its books.
    author: Option<String>,
}

impl SeriesRow {
    fn missing(&self) -> Vec<i64> {
        missing_volumes(&parse_volumes(self.volumes.as_deref()))
    }

    fn missing_text(&self) -> String {
        join(&self.missing())
    }
}

fn parse_volumes(text: Option<&str>) -> Vec<i64> {
    text.unwrap_or_default()
        .split(',')
        .filter_map(|v| v.parse().ok())
        .collect()
}

fn join(volumes: &[i64]) -> String {
    volumes
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Template)]
#[template(path = "books/series_list.html")]
pub struct SeriesList {
    page_title: &'static str,
    page: Page,
    total: i64,
    series: Vec<SeriesRow>,
}

/// `/books/series`.
pub async fn list(
    State(state): State<AppState>,
    Query(param): Query<PageParam>,
) -> AppResult<SeriesList> {
    let pool = &state.pool;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM book_series")
        .fetch_one(pool)
        .await?;
    let page = Page::new(param.page, total, String::new());
    let series = sqlx::query_as(
        "SELECT s.id, s.name,
                (SELECT count(*) FROM book WHERE book.series_id = s.id) AS books,
                (SELECT group_concat(volume) FROM book
                 WHERE book.series_id = s.id AND volume IS NOT NULL) AS volumes,
                (SELECT a.name FROM book
                 JOIN book_book_author ba ON ba.book_id = book.id AND ba.position = 0
                 JOIN book_author a ON a.id = ba.author_id
                 WHERE book.series_id = s.id ORDER BY book.volume LIMIT 1) AS author
         FROM book_series s ORDER BY s.name COLLATE NOCASE LIMIT ? OFFSET ?",
    )
    .bind(PAGE_SIZE)
    .bind(page.offset())
    .fetch_all(pool)
    .await?;

    Ok(SeriesList {
        page_title: "Series",
        page,
        total,
        series,
    })
}

#[derive(Template)]
#[template(path = "books/series.html")]
pub struct SeriesPage {
    name: String,
    /// By volume, the books without a number last.
    books: Vec<BookRow>,
    missing: Vec<i64>,
}

impl SeriesPage {
    fn missing_text(&self) -> String {
        join(&self.missing)
    }

    /// The add page, ready for a missing volume.
    fn add_href(&self, volume: &i64) -> String {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("series", &self.name)
            .append_pair("volume", &volume.to_string())
            .finish();
        format!("/books/add?{query}")
    }
}

/// `/books/series/:id`.
pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<SeriesPage> {
    let pool = &state.pool;
    let name: String = sqlx::query_scalar("SELECT name FROM book_series WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()?;
    let books: Vec<BookRow> = sqlx::query_as(&format!(
        "{} WHERE book.series_id = ?
         ORDER BY book.volume IS NULL, book.volume, book.published_year, book.title",
        BookRow::SELECT
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;
    let volumes: Vec<i64> = books.iter().filter_map(|b| b.volume).collect();

    Ok(SeriesPage {
        name,
        missing: missing_volumes(&volumes),
        books,
    })
}
