//! The book advent calendars (the `advent` crate's engine): behind each door
//! a book, given by its ISBN or `#id`. A book not in the list yet is looked
//! up by its ISBN and added on the way, so filling the calendar fills the
//! book list too. Own tables (migration 0007) and photo folder.

use advent::{Calendar, EditExtras, Pick, Spec};
use askama::Template;
use books_domain::isbn::Isbn;
use sqlx::SqliteConnection;

use super::series_text;
use crate::store::book_data::{self, Found};
use crate::store::books::{self, BookFields};
use common::db::Pool;
use common::web::error::AppResult;
use common::AppState;

/// The book calendars.
pub struct Books;

/// A book behind a door: the opened door shows it (`books/advent/door.html`).
#[derive(Debug, sqlx::FromRow, Template)]
#[template(path = "books/advent/door.html")]
pub struct DoorBook {
    book_id: i64,
    title: String,
    authors: Option<String>,
    series: Option<String>,
    volume: Option<i64>,
}

impl DoorBook {
    fn authors_text(&self) -> &str {
        self.authors.as_deref().unwrap_or("")
    }

    fn series_text(&self) -> String {
        series_text(self.series.as_deref(), self.volume)
    }
}

impl Calendar for Books {
    const SPEC: &'static Spec = &Spec {
        base: "/books/advent-calendar",
        table: "book_advent_calendar",
        item_column: "book_id",
        photo_folder: "book_advent_calendar_photos",
        section: "books",
        name: "Book advent calendar",
        item: "book",
        items: "books",
        field_label: "Book (ISBN or #id)",
        example: "e.g. 9780553573398",
        choose: "its ISBN, or #id",
        add_help: "its book (by ISBN: a book not in the list yet is added)",
        edit_help: "For each day: its book, by its <strong>ISBN</strong> (the 13 digits under the barcode, on the back),
    or <code>#id</code> for a book already in the list (they are suggested in the field); and its photo
    (JPEG, PNG or WebP). A book not in the list yet is looked up and added.",
        stays: "the book stays in the list",
    };

    const ENTRIES: &'static str = "SELECT e.day, e.photo, book.id AS book_id, book.title,
                (SELECT group_concat(name, ', ') FROM
                    (SELECT a.name FROM book_book_author ba
                     JOIN book_author a ON a.id = ba.author_id
                     WHERE ba.book_id = book.id ORDER BY ba.position)) AS authors,
                book_series.name AS series, book.volume
         FROM book_advent_calendar_entry e
         JOIN book ON book.id = e.book_id
         LEFT JOIN book_series ON book_series.id = book.series_id
         WHERE e.calendar_id = ? ORDER BY e.day";

    type Item = DoorBook;
    type New = Box<BookFields>;

    fn id(book: &DoorBook) -> i64 {
        book.book_id
    }

    fn href(book: &DoorBook) -> String {
        format!("/books/{}", book.book_id)
    }

    fn name(book: &DoorBook) -> &str {
        &book.title
    }

    fn detail(book: &DoorBook) -> String {
        book.authors_text().to_string()
    }

    /// `#12` (an id) or an ISBN. An ISBN not in the list is looked up
    /// (network: before any write transaction).
    async fn find(
        state: &AppState,
        text: &str,
    ) -> AppResult<Result<Pick<Box<BookFields>>, String>> {
        if let Some(id) = text.strip_prefix('#') {
            let Ok(id) = id.trim().parse::<i64>() else {
                return Ok(Err(format!("\"{text}\" is not a book id (#12).")));
            };
            let found: Option<i64> = sqlx::query_scalar("SELECT id FROM book WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?;
            return Ok(found
                .map(Pick::Existing)
                .ok_or(format!("there is no book #{id} in the list.")));
        }

        let isbn = match Isbn::parse(text) {
            Ok(isbn) => isbn,
            Err(why) => return Ok(Err(format!("\"{text}\": {why}."))),
        };
        let mut conn = state.pool.acquire().await?;
        if let Some(id) = books::with_isbn(&mut conn, &isbn).await? {
            return Ok(Ok(Pick::Existing(id)));
        }
        drop(conn);

        let not_found = || {
            format!("no catalogue knows the ISBN {isbn}: add the book on the Add page first, then use its #id.")
        };
        match book_data::lookup(&state.pool, &state.http, &isbn).await {
            Ok(Found::Book(info)) => Ok(BookFields::from_info(isbn.clone(), info)
                .map(|book| Pick::New(Box::new(book)))
                .ok_or_else(not_found)),
            Ok(Found::Unknown) => Ok(Err(not_found())),
            Ok(Found::Unavailable) => Ok(Err(format!(
                "Google Books is refusing requests for now and Open Library doesn't know {isbn}: \
                 try again later, or add the book on the Add page first."
            ))),
            Err(e) => {
                tracing::warn!(isbn = %isbn, "book lookup failed: {e}");
                Ok(Err(format!(
                    "the lookup of {isbn} failed (no internet?): try again, or add the book on the Add page first."
                )))
            }
        }
    }

    async fn add(conn: &mut SqliteConnection, book: &Box<BookFields>) -> sqlx::Result<i64> {
        books::insert(conn, book).await
    }

    /// The books not behind a door of any calendar, suggested in the fields.
    async fn edit_extras(pool: &Pool, _calendar_id: i64, _year: i64) -> AppResult<EditExtras> {
        let suggestions = sqlx::query_as(
            "SELECT id, title FROM book
             WHERE id NOT IN (SELECT book_id FROM book_advent_calendar_entry)
             ORDER BY added_on DESC, id DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(EditExtras {
            above: String::new(),
            suggestions,
        })
    }
}
