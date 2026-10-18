//! The book advent calendars: the 24 days of a December, each with the book
//! received that day and a photo of it. The twin of `routes::advent` (the
//! card calendars), with its own tables (migration 0007) and photo folder
//! (`book_advent_calendar_photos`).
//!
//! Private (`routes::private_router`, and the photos in `main::app`), like
//! the card calendars: the photos may show handwritten notes. `form.rs`
//! creates the calendars and fills their days.

pub mod form;

use askama::Template;
use axum::extract::{Path, Query, State};

use super::series_text;
use crate::store::db::Pool;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::pagination::{Page, PageParam, PAGE_SIZE};
use crate::AppState;

/// Photos live under `<media>/book_advent_calendar_photos/<year>/`.
pub const PHOTO_FOLDER: &str = "book_advent_calendar_photos";

/// One day of the calendar, with its book.
#[derive(Debug, sqlx::FromRow)]
pub struct DayRow {
    day: i64,
    /// Relative to the media dir: `book_advent_calendar_photos/2025/01-….jpg`.
    photo: Option<String>,
    book_id: i64,
    title: String,
    authors: Option<String>,
    series: Option<String>,
    volume: Option<i64>,
}

impl DayRow {
    fn authors_text(&self) -> &str {
        self.authors.as_deref().unwrap_or("")
    }

    fn series_text(&self) -> String {
        series_text(self.series.as_deref(), self.volume)
    }
}

/// A door of the calendar: its day, and what is behind it (if anything).
pub struct Door {
    day: i64,
    entry: Option<DayRow>,
}

#[derive(Template)]
#[template(path = "books/advent/calendar.html")]
pub struct CalendarPage {
    year: i64,
    doors: Vec<Door>,
}

impl CalendarPage {
    fn book_count(&self) -> usize {
        self.doors.iter().filter(|d| d.entry.is_some()).count()
    }
}

/// A calendar in the list.
#[derive(Debug, sqlx::FromRow)]
pub struct CalendarRow {
    year: i64,
    days: i64,
    /// The photo of its first filled day, for the list.
    photo: Option<String>,
}

#[derive(Template)]
#[template(path = "books/advent/list.html")]
pub struct CalendarsList {
    page_title: &'static str,
    page: Page,
    total: i64,
    calendars: Vec<CalendarRow>,
}

/// `/books/advent-calendar`: every calendar, the latest first.
pub async fn list(
    State(state): State<AppState>,
    Query(param): Query<PageParam>,
) -> AppResult<CalendarsList> {
    let pool = &state.pool;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM book_advent_calendar")
        .fetch_one(pool)
        .await?;
    let page = Page::new(param.page, total, String::new());
    let calendars = sqlx::query_as(
        "SELECT c.year,
                (SELECT count(*) FROM book_advent_calendar_entry e WHERE e.calendar_id = c.id) AS days,
                (SELECT e.photo FROM book_advent_calendar_entry e
                 WHERE e.calendar_id = c.id AND e.photo IS NOT NULL ORDER BY e.day LIMIT 1) AS photo
         FROM book_advent_calendar c ORDER BY c.year DESC LIMIT ? OFFSET ?",
    )
    .bind(PAGE_SIZE)
    .bind(page.offset())
    .fetch_all(pool)
    .await?;

    Ok(CalendarsList {
        page_title: "Book advent calendars",
        page,
        total,
        calendars,
    })
}

/// The id of the calendar of `year`, a 404 when there is none.
async fn calendar_id(pool: &Pool, year: i64) -> AppResult<i64> {
    sqlx::query_scalar("SELECT id FROM book_advent_calendar WHERE year = ?")
        .bind(year)
        .fetch_optional(pool)
        .await?
        .or_404()
}

/// The doors 1 to 24 (25 when there is a day 25) of a calendar.
async fn load_doors(pool: &Pool, calendar_id: i64) -> sqlx::Result<Vec<Door>> {
    let mut entries: Vec<DayRow> = sqlx::query_as(
        "SELECT e.day, e.photo, book.id AS book_id, book.title,
                (SELECT group_concat(name, ', ') FROM
                    (SELECT a.name FROM book_book_author ba
                     JOIN book_author a ON a.id = ba.author_id
                     WHERE ba.book_id = book.id ORDER BY ba.position)) AS authors,
                book_series.name AS series, book.volume
         FROM book_advent_calendar_entry e
         JOIN book ON book.id = e.book_id
         LEFT JOIN book_series ON book_series.id = book.series_id
         WHERE e.calendar_id = ? ORDER BY e.day",
    )
    .bind(calendar_id)
    .fetch_all(pool)
    .await?;

    let last_day = if entries.iter().any(|e| e.day == 25) {
        25
    } else {
        24
    };
    Ok((1..=last_day)
        .map(|day| Door {
            day,
            entry: entries
                .iter()
                .position(|e| e.day == day)
                .map(|i| entries.swap_remove(i)),
        })
        .collect())
}

/// `/books/advent-calendar/:year`.
pub async fn calendar(
    State(state): State<AppState>,
    Path(year): Path<i64>,
) -> AppResult<CalendarPage> {
    let calendar = calendar_id(&state.pool, year).await?;
    Ok(CalendarPage {
        year,
        doors: load_doors(&state.pool, calendar).await?,
    })
}
