//! The advent calendar: the 24 days of a December, each with the card
//! received that day and a photo of it (Django had the models, no page).
//!
//! Behind the login (`routes::private_router`, and the photos in
//! `main::app`): the photos show handwritten notes. `form.rs` creates the
//! calendars and fills their days.

pub mod form;

use askama::Template;
use axum::extract::{Path, Query, State};

use crate::store::db::Pool;
use crate::store::models::other_name;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::pagination::{Page, PageParam, PAGE_SIZE};
use crate::AppState;

/// One day of the calendar, with its card.
#[derive(Debug, sqlx::FromRow)]
pub struct DayRow {
    day: i64,
    /// Relative to the media dir: `advent_calendar_photos/2025/01.jpg`.
    photo: Option<String>,
    card_id: i64,
    name: String,
    name_en: Option<String>,
    code: Option<String>,
    rarity: Option<String>,
    card_data_id: Option<i64>,
}

impl DayRow {
    fn english_name(&self) -> Option<&str> {
        other_name(&self.name, self.name_en.as_deref())
    }
}

/// A door of the calendar: its day, and what is behind it (if anything).
pub struct Door {
    day: i64,
    entry: Option<DayRow>,
}

#[derive(Template)]
#[template(path = "advent/calendar.html")]
pub struct CalendarPage {
    year: i64,
    /// The calendar's lot, if it has one.
    lot_id: Option<i64>,
    doors: Vec<Door>,
}

impl CalendarPage {
    fn card_count(&self) -> usize {
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
#[template(path = "advent/list.html")]
pub struct CalendarsList {
    page_title: &'static str,
    page: Page,
    total: i64,
    calendars: Vec<CalendarRow>,
}

/// `/advent-calendar`: every calendar, the latest first.
pub async fn list(
    State(state): State<AppState>,
    Query(param): Query<PageParam>,
) -> AppResult<CalendarsList> {
    let pool = &state.pool;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM advent_calendar")
        .fetch_one(pool)
        .await?;
    let page = Page::new(param.page, total, String::new());
    let calendars = sqlx::query_as(
        "SELECT c.year,
                (SELECT count(*) FROM advent_calendar_entry e WHERE e.calendar_id = c.id) AS days,
                (SELECT e.photo FROM advent_calendar_entry e
                 WHERE e.calendar_id = c.id AND e.photo IS NOT NULL ORDER BY e.day LIMIT 1) AS photo
         FROM advent_calendar c ORDER BY c.year DESC LIMIT ? OFFSET ?",
    )
    .bind(PAGE_SIZE)
    .bind(page.offset())
    .fetch_all(pool)
    .await?;

    Ok(CalendarsList {
        page_title: "Advent calendars",
        page,
        total,
        calendars,
    })
}

/// The id of the calendar of `year`, a 404 when there is none.
async fn calendar_id(pool: &Pool, year: i64) -> AppResult<i64> {
    sqlx::query_scalar("SELECT id FROM advent_calendar WHERE year = ?")
        .bind(year)
        .fetch_optional(pool)
        .await?
        .or_404()
}

/// The lot of a calendar, if it has one.
async fn calendar_lot(pool: &Pool, calendar_id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT lot_id FROM advent_calendar WHERE id = ?")
        .bind(calendar_id)
        .fetch_one(pool)
        .await
}

/// The doors 1 to 24 (25 when there is a day 25) of a calendar.
async fn load_doors(pool: &Pool, calendar_id: i64) -> sqlx::Result<Vec<Door>> {
    let mut entries: Vec<DayRow> = sqlx::query_as(
        "SELECT e.day, e.photo, card.id AS card_id, card.name,
                json_extract(card_data.json_data, '$.name') AS name_en,
                card.code, card.rarity, card.card_data_id
         FROM advent_calendar_entry e
         JOIN card ON card.id = e.card_id
         LEFT JOIN card_data ON card_data.id = card.card_data_id
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

/// `/advent-calendar/:year`.
pub async fn calendar(
    State(state): State<AppState>,
    Path(year): Path<i64>,
) -> AppResult<CalendarPage> {
    let calendar = calendar_id(&state.pool, year).await?;
    let doors = load_doors(&state.pool, calendar).await?;
    Ok(CalendarPage {
        year,
        lot_id: calendar_lot(&state.pool, calendar).await?,
        doors,
    })
}
