//! `/books/authors`: the authors (surname order) and the books of each.
//! Authors are created and deleted with their books (`store::books`).

use askama::Template;
use axum::extract::{Path, Query, State};

use super::BookRow;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::pagination::{Page, PageParam, PAGE_SIZE};
use crate::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct AuthorRow {
    id: i64,
    name: String,
    books: i64,
}

#[derive(Template)]
#[template(path = "books/authors.html")]
pub struct AuthorsList {
    page_title: &'static str,
    page: Page,
    total: i64,
    authors: Vec<AuthorRow>,
}

/// `/books/authors`.
pub async fn list(
    State(state): State<AppState>,
    Query(param): Query<PageParam>,
) -> AppResult<AuthorsList> {
    let pool = &state.pool;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM book_author")
        .fetch_one(pool)
        .await?;
    let page = Page::new(param.page, total, String::new());
    let authors = sqlx::query_as(
        "SELECT a.id, a.name,
                (SELECT count(*) FROM book_book_author ba WHERE ba.author_id = a.id) AS books
         FROM book_author a ORDER BY a.sort_name LIMIT ? OFFSET ?",
    )
    .bind(PAGE_SIZE)
    .bind(page.offset())
    .fetch_all(pool)
    .await?;

    Ok(AuthorsList {
        page_title: "Authors",
        page,
        total,
        authors,
    })
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
