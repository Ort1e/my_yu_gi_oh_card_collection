//! The advent calendar: the 24 days of a December, each with the card
//! received that day and a photo of it (Django had the models, no page).
//!
//! Behind the login (`routes::private_router`, and the photos in
//! `main::app`): the photos show handwritten notes.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::Redirect;

use crate::store::models::other_name;
use crate::web::error::{AppResult, OrNotFound};
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
    /// Every calendar, for the year switcher.
    years: Vec<i64>,
    doors: Vec<Door>,
}

impl CalendarPage {
    fn is_year(&self, year: &i64) -> bool {
        *year == self.year
    }

    fn card_count(&self) -> usize {
        self.doors.iter().filter(|d| d.entry.is_some()).count()
    }
}

/// `/advent-calendar`: the latest calendar.
pub async fn latest(State(state): State<AppState>) -> AppResult<Redirect> {
    let year: i64 = sqlx::query_scalar::<_, Option<i64>>("SELECT max(year) FROM advent_calendar")
        .fetch_one(&state.pool)
        .await?
        .or_404()?;
    Ok(Redirect::to(&format!("/advent-calendar/{year}")))
}

/// `/advent-calendar/:year`: the doors 1 to 24 (25 when there is a day 25).
pub async fn calendar(
    State(state): State<AppState>,
    Path(year): Path<i64>,
) -> AppResult<CalendarPage> {
    let calendar_id: i64 = sqlx::query_scalar("SELECT id FROM advent_calendar WHERE year = ?")
        .bind(year)
        .fetch_optional(&state.pool)
        .await?
        .or_404()?;
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
    .fetch_all(&state.pool)
    .await?;

    let last_day = if entries.iter().any(|e| e.day == 25) {
        25
    } else {
        24
    };
    let doors = (1..=last_day)
        .map(|day| Door {
            day,
            entry: entries
                .iter()
                .position(|e| e.day == day)
                .map(|i| entries.swap_remove(i)),
        })
        .collect();

    Ok(CalendarPage {
        year,
        years: sqlx::query_scalar("SELECT year FROM advent_calendar ORDER BY year DESC")
            .fetch_all(&state.pool)
            .await?,
        doors,
    })
}
