//! The calendars' list and a calendar's page: its doors, opened one by one
//! (`static/js/advent_calendar.js`).

use askama::Template;
use axum::extract::{Path, Query, State};
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, Row};

use crate::{calendar_id, Calendar, Spec};
use common::db::Pool;
use common::web::error::AppResult;
use common::web::pagination::{Page, PageParam, PAGE_SIZE};
use common::AppState;

/// A day with its item, as read by [`Calendar::ENTRIES`].
struct Entry<I> {
    day: i64,
    photo: Option<String>,
    item: I,
}

impl<'r, I: FromRow<'r, SqliteRow>> FromRow<'r, SqliteRow> for Entry<I> {
    fn from_row(row: &'r SqliteRow) -> sqlx::Result<Self> {
        Ok(Entry {
            day: row.try_get("day")?,
            photo: row.try_get("photo")?,
            item: I::from_row(row)?,
        })
    }
}

/// A filled day, ready to show.
pub struct DayView {
    /// Relative to the media dir: `advent_calendar_photos/2025/01-….jpg`.
    pub photo: Option<String>,
    pub id: i64,
    pub href: String,
    pub name: String,
    pub detail: String,
    /// The opened door: the item's template.
    pub door: String,
}

/// A door of the calendar: its day, and what is behind it (if anything).
pub struct Door {
    pub day: i64,
    pub entry: Option<DayView>,
}

/// The doors 1 to 24 (25 when there is a day 25) of a calendar.
pub(crate) async fn load_doors<C: Calendar>(pool: &Pool, calendar_id: i64) -> AppResult<Vec<Door>> {
    let entries: Vec<Entry<C::Item>> = sqlx::query_as(C::ENTRIES)
        .bind(calendar_id)
        .fetch_all(pool)
        .await?;
    let mut entries = entries
        .into_iter()
        .map(|e| {
            Ok((
                e.day,
                DayView {
                    door: e.item.render()?,
                    id: C::id(&e.item),
                    href: C::href(&e.item),
                    name: C::name(&e.item).to_string(),
                    detail: C::detail(&e.item),
                    photo: e.photo,
                },
            ))
        })
        .collect::<AppResult<Vec<_>>>()?;

    let last_day = if entries.iter().any(|(day, _)| *day == 25) {
        25
    } else {
        24
    };
    Ok((1..=last_day)
        .map(|day| Door {
            day,
            entry: entries
                .iter()
                .position(|(d, _)| *d == day)
                .map(|i| entries.swap_remove(i).1),
        })
        .collect())
}

#[derive(Template)]
#[template(path = "advent/calendar.html")]
pub struct CalendarPage {
    spec: &'static Spec,
    year: i64,
    doors: Vec<Door>,
    /// More toolbar buttons (HTML).
    toolbar: String,
}

impl CalendarPage {
    fn item_count(&self) -> usize {
        self.doors.iter().filter(|d| d.entry.is_some()).count()
    }

    /// Where the page remembers its opened doors: `advent-calendar-2025-opened`.
    fn storage_key(&self) -> String {
        format!("{}-{}-opened", self.spec.table.replace('_', "-"), self.year)
    }
}

/// `<base>/:year`.
pub async fn calendar<C: Calendar>(
    State(state): State<AppState>,
    Path(year): Path<i64>,
) -> AppResult<CalendarPage> {
    let calendar = calendar_id::<C>(&state.pool, year).await?;
    Ok(CalendarPage {
        spec: C::SPEC,
        year,
        doors: load_doors::<C>(&state.pool, calendar).await?,
        toolbar: C::toolbar(&state.pool, calendar).await?,
    })
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
    spec: &'static Spec,
    page_title: String,
    page: Page,
    total: i64,
    calendars: Vec<CalendarRow>,
}

/// `<base>`: every calendar, the latest first.
pub async fn list<C: Calendar>(
    State(state): State<AppState>,
    Query(param): Query<PageParam>,
) -> AppResult<CalendarsList> {
    let pool = &state.pool;
    let table = C::SPEC.table;
    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await?;
    let page = Page::new(param.page, total, String::new());
    let calendars = sqlx::query_as(&format!(
        "SELECT c.year,
                (SELECT count(*) FROM {table}_entry e WHERE e.calendar_id = c.id) AS days,
                (SELECT e.photo FROM {table}_entry e
                 WHERE e.calendar_id = c.id AND e.photo IS NOT NULL ORDER BY e.day LIMIT 1) AS photo
         FROM {table} c ORDER BY c.year DESC LIMIT ? OFFSET ?"
    ))
    .bind(PAGE_SIZE)
    .bind(page.offset())
    .fetch_all(pool)
    .await?;

    Ok(CalendarsList {
        spec: C::SPEC,
        page_title: format!("{}s", C::SPEC.name),
        page,
        total,
        calendars,
    })
}
