//! The card advent calendars (the `advent` crate's engine): behind each door
//! a card of the collection, given by its code (`MAMO-FR023`) or `#id`
//! (Django had the models, no page).
//!
//! A calendar can have its own lot (a 0 € gift lot, created here): its cards
//! are added on the usual lot page, then suggested for the days.

use std::convert::Infallible;

use advent::{calendar_id, Calendar, EditExtras, Pick, Spec};
use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use sqlx::SqliteConnection;

use crate::store::db::{begin_write, Pool};
use crate::store::models::other_name;
use crate::web::error::AppResult;
use crate::AppState;

/// The card calendars.
pub struct Cards;

/// A card behind a door: the opened door shows it (`advent/door.html`).
#[derive(Debug, sqlx::FromRow, Template)]
#[template(path = "advent/door.html")]
pub struct DoorCard {
    card_id: i64,
    name: String,
    name_en: Option<String>,
    code: Option<String>,
    rarity: Option<String>,
    card_data_id: Option<i64>,
}

impl DoorCard {
    fn english_name(&self) -> Option<&str> {
        other_name(&self.name, self.name_en.as_deref())
    }
}

/// The calendar's lot on its edit page (`advent/lot.html`).
#[derive(Template)]
#[template(path = "advent/lot.html")]
struct LotBlock {
    year: i64,
    lot_id: Option<i64>,
    /// Its cards not behind a door yet.
    free_cards: usize,
}

impl Calendar for Cards {
    const SPEC: &'static Spec = &Spec {
        base: "/advent-calendar",
        table: "advent_calendar",
        item_column: "card_id",
        photo_folder: "advent_calendar_photos",
        section: "",
        name: "Advent calendar",
        item: "card",
        items: "cards",
        field_label: "Card (code or #id)",
        example: "e.g. MAMO-FR023",
        choose: "a code like MAMO-FR023, or #id",
        add_help: "its card (it must be in the collection)",
        edit_help: "For each day: its card, by its code as printed (<code>MAMO-FR023</code>), or <code>#id</code>
    to choose one copy (the number in the card's page address); and its photo (JPEG, PNG or WebP).
    The card must be in the collection: <a href=\"/lots/add\">add it to a lot</a> first if needed.",
        stays: "the card stays in the collection",
    };

    const ENTRIES: &'static str = "SELECT e.day, e.photo, card.id AS card_id, card.name,
                json_extract(card_data.json_data, '$.name') AS name_en,
                card.code, card.rarity, card.card_data_id
         FROM advent_calendar_entry e
         JOIN card ON card.id = e.card_id
         LEFT JOIN card_data ON card_data.id = card.card_data_id
         WHERE e.calendar_id = ? ORDER BY e.day";

    type Item = DoorCard;
    type New = Infallible;

    fn id(card: &DoorCard) -> i64 {
        card.card_id
    }

    fn href(card: &DoorCard) -> String {
        format!("/cards/{}", card.card_id)
    }

    fn name(card: &DoorCard) -> &str {
        &card.name
    }

    fn detail(card: &DoorCard) -> String {
        card.code.clone().unwrap_or_default()
    }

    async fn find(state: &AppState, text: &str) -> AppResult<Result<Pick<Infallible>, String>> {
        let mut conn = state.pool.acquire().await?;
        Ok(find_card(&mut conn, text).await?.map(Pick::Existing))
    }

    async fn add(_: &mut SqliteConnection, new: &Infallible) -> sqlx::Result<i64> {
        match *new {}
    }

    /// The lot (or the button creating it), and its cards not behind a door
    /// yet as suggestions.
    async fn edit_extras(pool: &Pool, calendar_id: i64, year: i64) -> AppResult<EditExtras> {
        let lot_id = calendar_lot(pool, calendar_id).await?;
        let lot_cards: Vec<(i64, String, Option<String>)> = match lot_id {
            Some(lot) => {
                sqlx::query_as(
                    "SELECT card.id, card.name, card.code FROM unite JOIN card ON card.id = unite.card_id
                     WHERE unite.lot_id = ? AND card.is_proxy = 0
                       AND card.id NOT IN (SELECT card_id FROM advent_calendar_entry)
                     ORDER BY card.code, card.id",
                )
                .bind(lot)
                .fetch_all(pool)
                .await?
            }
            None => Vec::new(),
        };
        let above = LotBlock {
            year,
            lot_id,
            free_cards: lot_cards.len(),
        }
        .render()?;
        Ok(EditExtras {
            above,
            suggestions: lot_cards
                .into_iter()
                .map(|(id, name, code)| (id, format!("{name} {}", code.unwrap_or_default())))
                .collect(),
        })
    }

    /// A link to the calendar's lot.
    async fn toolbar(pool: &Pool, calendar_id: i64) -> AppResult<String> {
        Ok(match calendar_lot(pool, calendar_id).await? {
            Some(lot) => format!(r#"<a href="/lots/{lot}" class="btn btn-secondary">Its lot</a>"#),
            None => String::new(),
        })
    }
}

async fn calendar_lot(pool: &Pool, calendar_id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT lot_id FROM advent_calendar WHERE id = ?")
        .bind(calendar_id)
        .fetch_one(pool)
        .await
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

/// The seller of the lots created for calendars.
const CALENDAR_SELLER: &str = "Advent calendar";

/// `POST /advent-calendar/:year/lot`: the calendar's own lot (a gift: 0 €,
/// dated December 1st, seller "Advent calendar"), then its page to add the
/// cards. A calendar that already has one keeps it.
pub async fn create_lot(
    State(state): State<AppState>,
    Path(year): Path<i64>,
) -> AppResult<Response> {
    let calendar = calendar_id::<Cards>(&state.pool, year).await?;
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
