//! Adding, editing and deleting books (behind the login).
//!
//! The add page is made for adding books in a row: type or scan the ISBN,
//! "Look up" fills the fields (`static/js/book_form.js` calls
//! `/books/api/isbn/:isbn`), check, save, and the empty form comes back with
//! "Added: …". Books can be deleted (unlike cards: no purchase history to
//! keep), except while behind an advent calendar door.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use domain::book::{clean_name, split_authors, BookFormat};
use domain::isbn::Isbn;
use serde::Deserialize;

use crate::store::books::{self, BookFields};
use crate::store::db::{begin_write, Pool};
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// Book fields as typed, to fill the form again.
#[derive(Debug, Default)]
pub struct BookValues {
    pub isbn: String,
    pub title: String,
    /// Comma-separated.
    pub authors: String,
    pub publisher: String,
    pub year: String,
    pub language: String,
    pub pages: String,
    pub series: String,
    pub volume: String,
    pub format: String,
    pub notes: String,
}

impl BookValues {
    fn from_form(form: &FormData) -> BookValues {
        BookValues {
            isbn: form.text("isbn"),
            title: form.text("title"),
            authors: form.text("authors"),
            publisher: form.text("publisher"),
            year: form.text("year"),
            language: form.text("language"),
            pages: form.text("pages"),
            series: form.text("series"),
            volume: form.text("volume"),
            format: form.text("format"),
            notes: form.text("notes"),
        }
    }

    fn format_is(&self, label: &str) -> bool {
        self.format == label
    }

    fn formats(&self) -> [BookFormat; 4] {
        BookFormat::ALL
    }
}

/// Validate the form; errors are added to `form.errors`.
fn read_book(form: &mut FormData) -> Option<BookFields> {
    let isbn = match form.get("isbn").map(Isbn::parse) {
        None => None,
        Some(Ok(isbn)) => Some(isbn),
        Some(Err(why)) => {
            form.errors.push(format!("ISBN: {why}."));
            None
        }
    };
    let title = form.required("title", "Title").map(|t| clean_name(&t));
    let year = form.number("year", "Year", 1000..=2100);
    let pages = form.number("pages", "Pages", 1..=100_000);
    let volume = form.number("volume", "Volume", 1..=999);
    let series = form.get("series").map(clean_name);
    if volume.is_some() && series.is_none() {
        form.errors
            .push("Volume: give the series it is a volume of.".into());
    }
    let format = match form.get("format") {
        None => None,
        Some(label) => {
            let format = BookFormat::from_label(label);
            if format.is_none() {
                form.errors.push("Format: invalid choice.".into());
            }
            format
        }
    };

    if !form.errors.is_empty() {
        return None;
    }

    Some(BookFields {
        isbn,
        title: title?,
        authors: split_authors(&form.text("authors")),
        publisher: form.get("publisher").map(clean_name),
        year,
        language: form.get("language").map(str::to_lowercase),
        pages,
        series,
        volume,
        format,
        notes: form.get("notes").map(str::to_string),
    })
}

/// An error when another book already has this ISBN.
async fn check_isbn_free(
    pool: &Pool,
    book: &BookFields,
    except: Option<i64>,
    errors: &mut Vec<String>,
) -> sqlx::Result<()> {
    let Some(isbn) = &book.isbn else {
        return Ok(());
    };

    let other: Option<(i64, String)> =
        sqlx::query_as("SELECT id, title FROM book WHERE isbn = ? AND id != ?")
            .bind(isbn.as_str())
            .bind(except.unwrap_or(0))
            .fetch_optional(pool)
            .await?;
    if let Some((id, title)) = other {
        errors.push(format!(
            "This ISBN is already in the list: \"{title}\" (book #{id}). \
             Another edition or format has its own ISBN."
        ));
    }
    Ok(())
}

/// The names suggested in the fields.
async fn suggestions(pool: &Pool) -> sqlx::Result<(Vec<String>, Vec<String>)> {
    let series = sqlx::query_scalar("SELECT name FROM book_series ORDER BY name COLLATE NOCASE")
        .fetch_all(pool)
        .await?;
    let authors = sqlx::query_scalar("SELECT name FROM book_author ORDER BY sort_name")
        .fetch_all(pool)
        .await?;
    Ok((series, authors))
}

// ---------- add ----------

#[derive(Template)]
#[template(path = "books/add.html")]
pub struct AddBookPage {
    errors: Vec<String>,
    values: BookValues,
    series_names: Vec<String>,
    author_names: Vec<String>,
    /// The book just added.
    added: Option<AddedBook>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AddedBook {
    id: i64,
    title: String,
}

impl AddBookPage {
    async fn new(pool: &Pool, errors: Vec<String>, values: BookValues) -> AppResult<AddBookPage> {
        let (series_names, author_names) = suggestions(pool).await?;
        Ok(AddBookPage {
            errors,
            values,
            series_names,
            author_names,
            added: None,
        })
    }
}

/// `/books/add?added=<id>` (after a save), `?series=…&volume=…` (from a
/// series page's missing volume).
#[derive(Debug, Deserialize)]
pub struct AddParams {
    added: Option<i64>,
    series: Option<String>,
    volume: Option<String>,
}

/// `GET /books/add`.
pub async fn add_page(
    State(state): State<AppState>,
    Query(params): Query<AddParams>,
) -> AppResult<AddBookPage> {
    let values = BookValues {
        series: params.series.unwrap_or_default(),
        volume: params.volume.unwrap_or_default(),
        ..BookValues::default()
    };
    let mut page = AddBookPage::new(&state.pool, Vec::new(), values).await?;
    if let Some(id) = params.added {
        page.added = sqlx::query_as("SELECT id, title FROM book WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    }
    Ok(page)
}

/// `POST /books/add`: save, then the empty form again for the next book.
pub async fn create(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let book = read_book(&mut form);
    if let Some(book) = &book {
        check_isbn_free(&state.pool, book, None, &mut form.errors).await?;
    }

    let (Some(book), true) = (book, form.errors.is_empty()) else {
        let values = BookValues::from_form(&form);
        let page = AddBookPage::new(&state.pool, form.errors, values).await?;
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    let id = books::insert(&mut tx, &book).await?;
    tx.commit().await?;
    tracing::info!(book_id = id, "book added");

    Ok(Redirect::to(&format!("/books/add?added={id}")).into_response())
}

// ---------- edit, delete ----------

#[derive(Template)]
#[template(path = "books/edit.html")]
pub struct EditBookPage {
    id: i64,
    errors: Vec<String>,
    values: BookValues,
    series_names: Vec<String>,
    author_names: Vec<String>,
}

impl EditBookPage {
    async fn new(
        pool: &Pool,
        id: i64,
        errors: Vec<String>,
        values: BookValues,
    ) -> AppResult<EditBookPage> {
        let (series_names, author_names) = suggestions(pool).await?;
        Ok(EditBookPage {
            id,
            errors,
            values,
            series_names,
            author_names,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredBook {
    isbn: Option<String>,
    title: String,
    publisher: Option<String>,
    published_year: Option<i64>,
    language: Option<String>,
    page_count: Option<i64>,
    series: Option<String>,
    volume: Option<i64>,
    format: Option<String>,
    notes: Option<String>,
}

async fn stored_values(pool: &Pool, id: i64) -> AppResult<BookValues> {
    let book: StoredBook = sqlx::query_as(
        "SELECT book.isbn, book.title, book.publisher, book.published_year, book.language,
                book.page_count, book_series.name AS series, book.volume, book.format, book.notes
         FROM book LEFT JOIN book_series ON book_series.id = book.series_id WHERE book.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .or_404()?;
    let authors: Vec<String> = super::book_authors(pool, id)
        .await?
        .into_iter()
        .map(|(_, name)| name)
        .collect();

    let text = |n: Option<i64>| n.map(|n| n.to_string()).unwrap_or_default();
    Ok(BookValues {
        isbn: book.isbn.unwrap_or_default(),
        title: book.title,
        authors: authors.join(", "),
        publisher: book.publisher.unwrap_or_default(),
        year: text(book.published_year),
        language: book.language.unwrap_or_default(),
        pages: text(book.page_count),
        series: book.series.unwrap_or_default(),
        volume: text(book.volume),
        format: book.format.unwrap_or_default(),
        notes: book.notes.unwrap_or_default(),
    })
}

/// `GET /books/:id/edit`.
pub async fn edit_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<EditBookPage> {
    let values = stored_values(&state.pool, id).await?;
    EditBookPage::new(&state.pool, id, Vec::new(), values).await
}

/// `POST /books/:id/edit`.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let book = read_book(&mut form);
    if let Some(book) = &book {
        check_isbn_free(&state.pool, book, Some(id), &mut form.errors).await?;
    }

    let (Some(book), true) = (book, form.errors.is_empty()) else {
        let values = BookValues::from_form(&form);
        let page = EditBookPage::new(&state.pool, id, form.errors, values).await?;
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    if !books::update(&mut tx, id, &book).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    tracing::info!(book_id = id, "book edited");

    Ok(Redirect::to(&format!("/books/{id}")).into_response())
}

/// `POST /books/:id/delete`: refused while the book is behind an advent
/// calendar door (the edit page says which).
pub async fn delete(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Response> {
    let mut tx = begin_write(&state.pool).await?;
    let days = books::calendar_days(&mut tx, id).await?;
    if let Some((year, day)) = days.first() {
        drop(tx);
        let error = format!(
            "This book is behind door {day} of the {year} advent calendar: \
             empty that day first (its edit page), then delete the book."
        );
        let values = stored_values(&state.pool, id).await?;
        let page = EditBookPage::new(&state.pool, id, vec![error], values).await?;
        return Ok(page.into_response());
    }

    if !books::delete(&mut tx, id).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    tracing::info!(book_id = id, "book deleted");

    Ok(Redirect::to("/books").into_response())
}
