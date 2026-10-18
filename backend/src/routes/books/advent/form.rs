//! Creating a book advent calendar and filling its days, one at a time
//! (behind the login). A day is a book, given by its ISBN or `#id`, and a
//! photo. A book not in the list yet is looked up by its ISBN and added on
//! the way, so filling the calendar fills the book list too.
//!
//! Photos: `store::calendar_photos`, like the card calendars.

use askama::Template;
use axum::extract::{Multipart, Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::Datelike;
use domain::isbn::Isbn;

use super::{calendar_id, load_doors, Door, PHOTO_FOLDER};
use crate::store::book_data;
use crate::store::books::{self, BookFields};
use crate::store::calendar_photos;
use crate::store::db::{begin_write, Pool};
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

#[derive(Template)]
#[template(path = "books/advent/edit.html")]
pub struct EditPage {
    year: i64,
    doors: Vec<Door>,
    errors: Vec<String>,
    /// The books not behind a door of any calendar, suggested in the fields:
    /// `(id, title)`.
    free_books: Vec<(i64, String)>,
}

async fn edit_page_with(pool: &Pool, year: i64, errors: Vec<String>) -> AppResult<EditPage> {
    let calendar = calendar_id(pool, year).await?;
    let free_books = sqlx::query_as(
        "SELECT id, title FROM book
         WHERE id NOT IN (SELECT book_id FROM book_advent_calendar_entry)
         ORDER BY added_on DESC, id DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(EditPage {
        year,
        doors: load_doors(pool, calendar).await?,
        errors,
        free_books,
    })
}

/// `GET /books/advent-calendar/:year/edit`.
pub async fn edit_page(
    State(state): State<AppState>,
    Path(year): Path<i64>,
) -> AppResult<EditPage> {
    edit_page_with(&state.pool, year, Vec::new()).await
}

#[derive(Template)]
#[template(path = "books/advent/add.html")]
pub struct AddPage {
    errors: Vec<String>,
    year: String,
}

/// `GET /books/advent-calendar/add`: the year of the new calendar
/// (suggested: the year after the latest calendar, else this year).
pub async fn add_page(State(state): State<AppState>) -> AppResult<AddPage> {
    let latest: Option<i64> = sqlx::query_scalar("SELECT max(year) FROM book_advent_calendar")
        .fetch_one(&state.pool)
        .await?;
    let year = latest.map_or_else(|| i64::from(chrono::Local::now().year()), |y| y + 1);
    Ok(AddPage {
        errors: Vec::new(),
        year: year.to_string(),
    })
}

/// `POST /books/advent-calendar/add` (field `year`): the new calendar, then
/// its edit page.
pub async fn create(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let form = FormData::new(pairs);
    let typed = form.text("year");
    let refuse = |error: String| {
        Ok(AddPage {
            errors: vec![error],
            year: typed.clone(),
        }
        .into_response())
    };

    let Some(year) = typed
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|y| (2000..=2100).contains(y))
    else {
        return refuse("The year must be a year like 2026.".into());
    };
    let inserted = sqlx::query("INSERT OR IGNORE INTO book_advent_calendar (year) VALUES (?)")
        .bind(year)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if inserted == 0 {
        return refuse(format!(
            "There is already a calendar for {year}: open it from the list to fill its days."
        ));
    }

    tracing::info!(year, "book advent calendar created");
    Ok(Redirect::to(&format!("/books/advent-calendar/{year}/edit")).into_response())
}

fn back_to(year: i64, day: i64) -> Response {
    Redirect::to(&format!("/books/advent-calendar/{year}/edit#day-{day}")).into_response()
}

fn check_day(day: i64) -> AppResult<()> {
    if (1..=25).contains(&day) {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// The book a day field names.
enum DayBook {
    /// In the list already.
    Existing(i64),
    /// Not in the list: to add, from the lookup.
    New(Box<BookFields>),
}

/// The book a day field names: `#12` (an id) or an ISBN. An ISBN not in the
/// list is looked up (network: before any write transaction). `Err` is the
/// message to show.
async fn find_book(state: &AppState, text: &str) -> AppResult<Result<DayBook, String>> {
    if let Some(id) = text.strip_prefix('#') {
        let Ok(id) = id.trim().parse::<i64>() else {
            return Ok(Err(format!("\"{text}\" is not a book id (#12).")));
        };
        let found: Option<i64> = sqlx::query_scalar("SELECT id FROM book WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
        return Ok(found
            .map(DayBook::Existing)
            .ok_or(format!("there is no book #{id} in the list.")));
    }

    let isbn = match Isbn::parse(text) {
        Ok(isbn) => isbn,
        Err(why) => return Ok(Err(format!("\"{text}\": {why}."))),
    };
    let mut conn = state.pool.acquire().await?;
    if let Some(id) = books::with_isbn(&mut conn, &isbn).await? {
        return Ok(Ok(DayBook::Existing(id)));
    }
    drop(conn);

    let not_found = || {
        format!("no catalogue knows the ISBN {isbn}: add the book on the Add page first, then use its #id.")
    };
    match book_data::lookup(&state.pool, &state.http, &isbn).await {
        Ok(Some(info)) => Ok(BookFields::from_info(isbn.clone(), info)
            .map(|book| DayBook::New(Box::new(book)))
            .ok_or_else(not_found)),
        Ok(None) => Ok(Err(not_found())),
        Err(e) => {
            tracing::warn!(isbn = %isbn, "book lookup failed: {e}");
            Ok(Err(format!(
                "the lookup of {isbn} failed (no internet?): try again, or add the book on the Add page first."
            )))
        }
    }
}

async fn current_photo(pool: &Pool, calendar_id: i64, day: i64) -> sqlx::Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(
        "SELECT photo FROM book_advent_calendar_entry WHERE calendar_id = ? AND day = ?",
    )
    .bind(calendar_id)
    .bind(day)
    .fetch_optional(pool)
    .await?
    .flatten())
}

/// `POST /books/advent-calendar/:year/days/:day` (multipart): the book
/// (`book`: ISBN or `#id`; empty keeps the current one) and/or a photo
/// (`photo`).
pub async fn save_day(
    State(state): State<AppState>,
    Path((year, day)): Path<(i64, i64)>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    check_day(day)?;
    let calendar = calendar_id(&state.pool, year).await?;

    let read = |e: axum::extract::multipart::MultipartError| {
        AppError::Internal(format!("reading the form: {e}"))
    };
    let (mut book_text, mut photo) = (String::new(), Vec::new());
    while let Some(field) = multipart.next_field().await.map_err(read)? {
        match field.name() {
            Some("book") => book_text = field.text().await.map_err(read)?.trim().to_string(),
            Some("photo") => photo = field.bytes().await.map_err(read)?.to_vec(),
            _ => {}
        }
    }

    let mut errors = Vec::new();
    let current_book: Option<i64> = sqlx::query_scalar(
        "SELECT book_id FROM book_advent_calendar_entry WHERE calendar_id = ? AND day = ?",
    )
    .bind(calendar)
    .bind(day)
    .fetch_optional(&state.pool)
    .await?;
    let book = if book_text.is_empty() {
        if current_book.is_none() {
            errors.push(format!("Day {day}: choose its book (its ISBN, or #id)."));
        }
        current_book.map(DayBook::Existing)
    } else {
        match find_book(&state, &book_text).await? {
            Ok(book) => Some(book),
            Err(why) => {
                errors.push(format!("Day {day}: {why}"));
                None
            }
        }
    };

    let processed = if photo.is_empty() {
        None
    } else {
        match calendar_photos::process(photo).await? {
            Ok(processed) => Some(processed),
            Err(why) => {
                errors.push(format!("Day {day}: {why}."));
                None
            }
        }
    };

    let (Some(book), true) = (book, errors.is_empty()) else {
        return Ok(edit_page_with(&state.pool, year, errors)
            .await?
            .into_response());
    };

    let old_photo = current_photo(&state.pool, calendar, day).await?;
    let new_photo = match &processed {
        Some(photo) => {
            Some(calendar_photos::write(&state.media_dir, PHOTO_FOLDER, year, day, photo).await?)
        }
        None => None,
    };

    let saved = async {
        let mut tx = begin_write(&state.pool).await?;
        let (book_id, added) = match &book {
            DayBook::Existing(id) => (*id, false),
            DayBook::New(fields) => (books::insert(&mut tx, fields).await?, true),
        };
        sqlx::query(
            "INSERT INTO book_advent_calendar_entry (calendar_id, day, book_id, photo)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (calendar_id, day) DO UPDATE
                 SET book_id = excluded.book_id,
                     photo = coalesce(excluded.photo, book_advent_calendar_entry.photo)",
        )
        .bind(calendar)
        .bind(day)
        .bind(book_id)
        .bind(new_photo.as_ref().map(|p| &p.relative))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>((book_id, added))
    }
    .await;
    let (book_id, added) = match saved {
        Ok(saved) => saved,
        Err(e) => {
            if let Some(new_photo) = &new_photo {
                calendar_photos::remove(&new_photo.files).await;
            }
            return Err(e.into());
        }
    };
    if let (Some(old), Some(_)) = (&old_photo, &new_photo) {
        calendar_photos::remove(&calendar_photos::files(&state.media_dir, old)).await;
    }

    tracing::info!(
        year,
        day,
        book_id,
        book_added = added,
        new_photo = new_photo.is_some(),
        "book advent day saved"
    );
    Ok(back_to(year, day))
}

/// `POST /books/advent-calendar/:year/days/:day/rotate` (field `direction`:
/// `left` or `right`): turn the day's photo a quarter turn.
pub async fn rotate(
    State(state): State<AppState>,
    Path((year, day)): Path<(i64, i64)>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    check_day(day)?;
    let calendar = calendar_id(&state.pool, year).await?;
    let clockwise = FormData::new(pairs).get("direction") == Some("right");
    let old = current_photo(&state.pool, calendar, day).await?.or_404()?;

    let new =
        calendar_photos::rotate(&state.media_dir, PHOTO_FOLDER, year, day, &old, clockwise).await?;
    sqlx::query(
        "UPDATE book_advent_calendar_entry SET photo = ? WHERE calendar_id = ? AND day = ?",
    )
    .bind(&new)
    .bind(calendar)
    .bind(day)
    .execute(&state.pool)
    .await?;
    calendar_photos::remove(&[state.media_dir.join(&old)]).await;

    tracing::info!(year, day, clockwise, "book advent photo rotated");
    Ok(back_to(year, day))
}

/// `POST /books/advent-calendar/:year/days/:day/delete`: empty the day (its
/// photo files are deleted; the book stays in the list).
pub async fn remove_day(
    State(state): State<AppState>,
    Path((year, day)): Path<(i64, i64)>,
) -> AppResult<Response> {
    check_day(day)?;
    let calendar = calendar_id(&state.pool, year).await?;
    let photo = current_photo(&state.pool, calendar, day).await?;

    sqlx::query("DELETE FROM book_advent_calendar_entry WHERE calendar_id = ? AND day = ?")
        .bind(calendar)
        .bind(day)
        .execute(&state.pool)
        .await?;
    if let Some(photo) = photo {
        calendar_photos::remove(&calendar_photos::files(&state.media_dir, &photo)).await;
    }

    tracing::info!(year, day, "book advent day emptied");
    Ok(back_to(year, day))
}
