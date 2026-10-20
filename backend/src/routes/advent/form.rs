//! Creating an advent calendar and filling its days, one at a time (behind
//! the login). A day is a card of the collection, given by its code
//! (`MAMO-FR023`) or `#id`, and a photo.
//!
//! Photos (`store::calendar_photos`): the copy shown on the page is upright,
//! at most 1600 px and without metadata; the original is kept in
//! `originals/`.
//!
//! A calendar can have its own lot (a 0 € gift lot, created here): its cards
//! are added on the usual lot page, then suggested for the days.

use askama::Template;
use axum::extract::{Multipart, Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::Datelike;
use sqlx::SqliteConnection;

use super::{calendar_id, calendar_lot, load_doors, Door};
use crate::store::calendar_photos;
use crate::store::db::{begin_write, Pool};
use crate::web::day_form::{back_to, check_day, DayForm};
use crate::web::error::{AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// Where the day forms go back to (`day_form::back_to`).
const EDIT_BASE: &str = "/advent-calendar";

/// Photos live under `<media>/advent_calendar_photos/<year>/`.
const PHOTO_FOLDER: &str = "advent_calendar_photos";

#[derive(Template)]
#[template(path = "advent/edit.html")]
pub struct EditPage {
    year: i64,
    doors: Vec<Door>,
    errors: Vec<String>,
    /// The calendar's lot, if it has one.
    lot_id: Option<i64>,
    /// Its cards not yet behind a door, suggested for the days.
    lot_cards: Vec<LotCard>,
}

/// A card of the calendar's lot, for the suggestions of the card fields.
#[derive(Debug, sqlx::FromRow)]
pub struct LotCard {
    id: i64,
    name: String,
    code: Option<String>,
}

impl LotCard {
    fn code_text(&self) -> &str {
        self.code.as_deref().unwrap_or("")
    }
}

async fn edit_page_with(pool: &Pool, year: i64, errors: Vec<String>) -> AppResult<EditPage> {
    let calendar = calendar_id(pool, year).await?;
    let lot_id = calendar_lot(pool, calendar).await?;
    let lot_cards = match lot_id {
        Some(lot) => sqlx::query_as(
            "SELECT card.id, card.name, card.code FROM unite JOIN card ON card.id = unite.card_id
                 WHERE unite.lot_id = ? AND card.is_proxy = 0
                   AND card.id NOT IN (SELECT card_id FROM advent_calendar_entry)
                 ORDER BY card.code, card.id",
        )
        .bind(lot)
        .fetch_all(pool)
        .await?,
        None => Vec::new(),
    };
    Ok(EditPage {
        year,
        doors: load_doors(pool, calendar).await?,
        errors,
        lot_id,
        lot_cards,
    })
}

/// The seller of the lots created for calendars.
const CALENDAR_SELLER: &str = "Advent calendar";

/// `POST /advent-calendar/:year/lot`: the calendar's own lot (a gift: 0 €,
/// dated December 1st, seller "Advent calendar"), then its page to add the
/// cards. A calendar that already has one keeps it.
pub async fn create_lot(
    State(state): State<AppState>,
    Path(year): Path<i64>,
) -> AppResult<Response> {
    let calendar = calendar_id(&state.pool, year).await?;
    let mut tx = begin_write(&state.pool).await?;
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT lot_id FROM advent_calendar WHERE id = ?")
            .bind(calendar)
            .fetch_one(&mut *tx)
            .await?;
    if let Some(lot) = existing {
        return Ok(Redirect::to(&format!("/lots/{lot}")).into_response());
    }

    sqlx::query("INSERT OR IGNORE INTO seller (name, is_person) VALUES (?, 0)")
        .bind(CALENDAR_SELLER)
        .execute(&mut *tx)
        .await?;
    let seller: i64 = sqlx::query_scalar("SELECT id FROM seller WHERE name = ?")
        .bind(CALENDAR_SELLER)
        .fetch_one(&mut *tx)
        .await?;
    let date = format!("{year}-12-01");
    let lot = sqlx::query(
        "INSERT INTO lot (lot_type, price, buy_date, received_date, seller_id, no_card_price)
         VALUES ('purchase', '0.00', ?, ?, ?, '0.00')",
    )
    .bind(&date)
    .bind(&date)
    .bind(seller)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    sqlx::query("UPDATE advent_calendar SET lot_id = ? WHERE id = ?")
        .bind(lot)
        .bind(calendar)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    tracing::info!(year, lot_id = lot, "advent calendar lot created");
    Ok(Redirect::to(&format!("/lots/{lot}")).into_response())
}

/// `GET /advent-calendar/:year/edit`.
pub async fn edit_page(
    State(state): State<AppState>,
    Path(year): Path<i64>,
) -> AppResult<EditPage> {
    edit_page_with(&state.pool, year, Vec::new()).await
}

#[derive(Template)]
#[template(path = "advent/add.html")]
pub struct AddPage {
    errors: Vec<String>,
    year: String,
}

/// `GET /advent-calendar/add`: the year of the new calendar (suggested: the
/// first year after the latest calendar, else this year).
pub async fn add_page(State(state): State<AppState>) -> AppResult<AddPage> {
    let latest: Option<i64> = sqlx::query_scalar("SELECT max(year) FROM advent_calendar")
        .fetch_one(&state.pool)
        .await?;
    let year = latest.map_or_else(|| i64::from(chrono::Local::now().year()), |y| y + 1);
    Ok(AddPage {
        errors: Vec::new(),
        year: year.to_string(),
    })
}

/// `POST /advent-calendar/add` (field `year`): the new calendar, then its edit
/// page to fill the days.
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
    let inserted = sqlx::query("INSERT OR IGNORE INTO advent_calendar (year) VALUES (?)")
        .bind(year)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if inserted == 0 {
        return refuse(format!(
            "There is already a calendar for {year}: open it from the list to fill its days."
        ));
    }

    tracing::info!(year, "advent calendar created");
    Ok(Redirect::to(&format!("/advent-calendar/{year}/edit")).into_response())
}

/// The card a day field names: `#1794` (an id) or a code (`MAMO-FR023`,
/// case-insensitive), preferring a copy not already in a calendar. Only real
/// cards (no proxy). `Err` is the message to show.
async fn find_card(conn: &mut SqliteConnection, text: &str) -> AppResult<Result<i64, String>> {
    if let Some(id) = text.strip_prefix('#') {
        let Ok(id) = id.trim().parse::<i64>() else {
            return Ok(Err(format!("\"{text}\" is not a card id (#1794).")));
        };
        let found: Option<i64> =
            sqlx::query_scalar("SELECT id FROM card WHERE id = ? AND is_proxy = 0")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
        return Ok(found.ok_or(format!("there is no card #{id} in the collection.")));
    }
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM card WHERE upper(code) = upper(?) AND is_proxy = 0
         ORDER BY id IN (SELECT card_id FROM advent_calendar_entry), id LIMIT 1",
    )
    .bind(text)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(found.ok_or(format!(
        "there is no card {text} in the collection: add it to a lot first."
    )))
}

async fn current_photo(pool: &Pool, calendar_id: i64, day: i64) -> sqlx::Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(
        "SELECT photo FROM advent_calendar_entry WHERE calendar_id = ? AND day = ?",
    )
    .bind(calendar_id)
    .bind(day)
    .fetch_optional(pool)
    .await?
    .flatten())
}

/// `POST /advent-calendar/:year/days/:day` (multipart): the card (`card`,
/// code or `#id`; empty keeps the current one) and/or a photo (`photo`).
pub async fn save_day(
    State(state): State<AppState>,
    Path((year, day)): Path<(i64, i64)>,
    multipart: Multipart,
) -> AppResult<Response> {
    check_day(day)?;
    let calendar = calendar_id(&state.pool, year).await?;

    let mut form = DayForm::read(multipart, "card").await?;

    let mut errors = Vec::new();
    let mut conn = state.pool.acquire().await?;
    let current_card: Option<i64> = sqlx::query_scalar(
        "SELECT card_id FROM advent_calendar_entry WHERE calendar_id = ? AND day = ?",
    )
    .bind(calendar)
    .bind(day)
    .fetch_optional(&mut *conn)
    .await?;
    let card = if form.item.is_empty() {
        if current_card.is_none() {
            errors.push(format!(
                "Day {day}: choose its card (a code like MAMO-FR023, or #id)."
            ));
        }
        current_card
    } else {
        match find_card(&mut conn, &form.item).await? {
            Ok(id) => Some(id),
            Err(why) => {
                errors.push(format!("Day {day}: {why}"));
                None
            }
        }
    };
    drop(conn);

    let processed = form.photo(day, &mut errors).await?;

    let (Some(card), true) = (card, errors.is_empty()) else {
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
        sqlx::query(
            "INSERT INTO advent_calendar_entry (calendar_id, day, card_id, photo)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (calendar_id, day) DO UPDATE
                 SET card_id = excluded.card_id,
                     photo = coalesce(excluded.photo, advent_calendar_entry.photo)",
        )
        .bind(calendar)
        .bind(day)
        .bind(card)
        .bind(new_photo.as_ref().map(|p| &p.relative))
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }
    .await;
    if let Err(e) = saved {
        if let Some(new_photo) = &new_photo {
            calendar_photos::remove(&new_photo.files).await;
        }
        return Err(e.into());
    }
    if let (Some(old), Some(_)) = (&old_photo, &new_photo) {
        calendar_photos::remove(&calendar_photos::files(&state.media_dir, old)).await;
    }

    tracing::info!(
        year,
        day,
        card_id = card,
        new_photo = new_photo.is_some(),
        "advent day saved"
    );
    Ok(back_to(EDIT_BASE, year, day))
}

/// `POST /advent-calendar/:year/days/:day/rotate` (field `direction`:
/// `left` or `right`): turn the day's photo a quarter turn. The original
/// stays as uploaded.
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
    sqlx::query("UPDATE advent_calendar_entry SET photo = ? WHERE calendar_id = ? AND day = ?")
        .bind(&new)
        .bind(calendar)
        .bind(day)
        .execute(&state.pool)
        .await?;
    calendar_photos::remove(&[state.media_dir.join(&old)]).await;

    tracing::info!(year, day, clockwise, "advent photo rotated");
    Ok(back_to(EDIT_BASE, year, day))
}

/// `POST /advent-calendar/:year/days/:day/delete`: empty the day (its photo
/// files are deleted; the card stays in the collection).
pub async fn remove_day(
    State(state): State<AppState>,
    Path((year, day)): Path<(i64, i64)>,
) -> AppResult<Response> {
    check_day(day)?;
    let calendar = calendar_id(&state.pool, year).await?;
    let photo = current_photo(&state.pool, calendar, day).await?;

    sqlx::query("DELETE FROM advent_calendar_entry WHERE calendar_id = ? AND day = ?")
        .bind(calendar)
        .bind(day)
        .execute(&state.pool)
        .await?;
    if let Some(photo) = photo {
        calendar_photos::remove(&calendar_photos::files(&state.media_dir, &photo)).await;
    }

    tracing::info!(year, day, "advent day emptied");
    Ok(back_to(EDIT_BASE, year, day))
}
