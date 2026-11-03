//! `/books/authors`: the authors (surname order) in one table, sorted and
//! searched in the page like the books (`static/js/tables.js`), and the
//! books of each.
//! Authors are created and deleted with their books (`store::books`).
//! In the public copy, only their books not private, and only the authors
//! who have one (`super::shown_books`).

use askama::Template;
use axum::extract::{Path, State};

use super::{shown_books, BookRow};
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
    let authors = sqlx::query_as(&format!(
        "SELECT * FROM
             (SELECT a.id, a.name, a.sort_name,
                     (SELECT count(*) FROM book_book_author ba JOIN book ON book.id = ba.book_id
                      WHERE ba.author_id = a.id AND {}) AS books
              FROM book_author a)
         WHERE books > 0 ORDER BY sort_name COLLATE NOCASE",
        shown_books()
    ))
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
    let books: Vec<BookRow> = sqlx::query_as(&format!(
        "{} WHERE book.id IN (SELECT book_id FROM book_book_author WHERE author_id = ?) AND {}
         ORDER BY book_series.name IS NULL, book_series.name COLLATE NOCASE, book.volume,
                  book.published_year, book.title COLLATE NOCASE",
        BookRow::SELECT,
        shown_books()
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;
    // an author has books (deleted with the last one): none = all private
    if books.is_empty() {
        return Err(common::web::error::AppError::NotFound);
    }

    Ok(AuthorPage { name, books })
}
