//! The copies flagged "to sell" (`card.for_sale`, migration 0032).
//!
//! - `POST /cards/:id/for-sale`: the card page's switch;
//! - `POST /lots/:id/for-sale`: the lot page's checkboxes (mark / unmark);
//! - `GET /cards/to-sell`: the flagged copies, and for Cardmarket's bulk
//!   sell (which asks for the set first, then its cards) one folded section
//!   per set, a row per card + rarity + language + condition;
//!
//! No "sell" button: a sale is validated only by creating its lot from a
//! Cardmarket shipment (`/lots/add`); `store::cards::insert_lot_cards` makes
//! each line of a sale lot take a matching flagged copy (same code, rarity,
//! the same condition first) instead of a new `card`, clears its flag and
//! empties its built slot.
//!
//! A flagged copy still counts as owned; a deck takes it last
//! (`yugioh_domain::ownership::take_order`). Private: not in the public copy.

use std::collections::{BTreeMap, HashMap};

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use yugioh_domain::card::{CardStatus, Language};
use yugioh_domain::card_set::{card_number, code_without_language};

use crate::store::cards::{set_for_sale, SOLD_EXISTS};
use crate::store::db::{begin_write, Money};
use crate::store::valuation::MARKET_PRICE;
use crate::web::error::{bad_request, AppResult};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// The section of the codes whose set is not known.
const OTHER: &str = "Other";

/// `for_sale` = `1` or `0`; back to the card.
pub async fn set_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let form = FormData::new(pairs);
    let mut tx = begin_write(&state.pool).await?;
    crate::store::db::exists(&mut tx, "card", id).await?;
    set_for_sale(&mut tx, &[id], form.get("for_sale") == Some("1")).await?;
    tx.commit().await?;
    Ok(Redirect::to(&format!("/cards/{id}")).into_response())
}

/// The lot page's form: `group` = the ids of a row's copies, comma-separated
/// (one per ticked row), `action` = `mark` or `unmark`; back to the lot.
pub async fn set_many(
    State(state): State<AppState>,
    Path(lot_id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mark = match pairs
        .iter()
        .find(|(k, _)| k == "action")
        .map(|(_, v)| v.as_str())
    {
        Some("mark") => true,
        Some("unmark") => false,
        _ => return bad_request("Unknown action."),
    };
    let wanted: Vec<i64> = pairs
        .iter()
        .filter(|(k, _)| k == "group")
        .flat_map(|(_, v)| v.split(','))
        .filter_map(|id| id.trim().parse().ok())
        .collect();

    let mut tx = begin_write(&state.pool).await?;
    crate::store::db::exists(&mut tx, "lot", lot_id).await?;
    // only the copies of this lot
    let ids: Vec<i64> = sqlx::query_scalar("SELECT card_id FROM unite WHERE lot_id = ?")
        .bind(lot_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .filter(|id| wanted.contains(id))
        .collect();
    set_for_sale(&mut tx, &ids, mark).await?;
    tx.commit().await?;
    Ok(Redirect::to(&format!("/lots/{lot_id}")).into_response())
}

/// A flagged copy, one row of the first table.
#[derive(sqlx::FromRow)]
struct FlaggedRow {
    id: i64,
    name: String,
    code: Option<String>,
    rarity: Option<String>,
    status: Option<String>,
    name_en: Option<String>,
    market_price: Option<Money>,
    deck_id: Option<i64>,
    deck_name: Option<String>,
    version_name: Option<String>,
}

impl FlaggedRow {
    fn english_name(&self) -> &str {
        self.name_en.as_deref().unwrap_or(&self.name)
    }

    fn status_label(&self) -> &'static str {
        self.status
            .as_deref()
            .and_then(CardStatus::from_code)
            .map_or("", CardStatus::label)
    }
}

/// A row of a set's section: what Cardmarket asks to pick.
pub struct ListRow {
    name: String,
    number: String,
    rarity: String,
    language: String,
    status: String,
    status_label: &'static str,
    count: usize,
}

/// One set's section.
pub struct SetSection {
    name: String,
    copies: usize,
    rows: Vec<ListRow>,
}

#[derive(Template)]
#[template(path = "cards/to_sell.html")]
pub struct ToSellPage {
    copies: Vec<FlaggedRow>,
    sets: Vec<SetSection>,
}

impl ToSellPage {
    fn total_trend(&self) -> Money {
        Money(
            self.copies
                .iter()
                .filter_map(|c| c.market_price)
                .map(|p| p.0)
                .sum(),
        )
    }
}

/// The flagged copies still owned, oldest first, with the built version that
/// holds each.
async fn flagged(state: &AppState) -> sqlx::Result<Vec<FlaggedRow>> {
    sqlx::query_as(&format!(
        "SELECT card.id, card.name, card.code, card.rarity, card.last_known_status AS status,
                json_extract(card_data.json_data, '$.name') AS name_en,
                {MARKET_PRICE} AS market_price,
                deck.id AS deck_id, deck.name AS deck_name, version.version_name
         FROM card
         LEFT JOIN card_data ON card_data.id = card.card_data_id
         LEFT JOIN deck_version_copy AS slot ON slot.card_id = card.id
         LEFT JOIN deck_version AS version ON version.id = slot.deck_version_id
         LEFT JOIN deck ON deck.id = version.deck_id
         WHERE card.for_sale = 1 AND NOT {SOLD_EXISTS}
         ORDER BY card.name, card.id"
    ))
    .fetch_all(&state.pool)
    .await
}

/// The sets' names by code (upper case), the earliest release winning.
async fn set_names(state: &AppState) -> sqlx::Result<HashMap<String, String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT code, name FROM ygo_set WHERE code IS NOT NULL ORDER BY tcg_date IS NOT NULL, tcg_date DESC, name DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    // the last one inserted wins: the earliest
    Ok(rows
        .into_iter()
        .map(|(code, name)| {
            (
                code.to_ascii_uppercase(),
                yugioh_domain::card_set::set_name(&name),
            )
        })
        .collect())
}

/// The sections of the Cardmarket list: by set (sorted by name, "Other" last),
/// a row per card + rarity + language + condition.
fn sections(copies: &[FlaggedRow], names: &HashMap<String, String>) -> Vec<SetSection> {
    type Key = (String, String, String, String, String);
    let mut by_set: BTreeMap<String, BTreeMap<Key, usize>> = BTreeMap::new();
    for copy in copies {
        let code = copy.code.as_deref().unwrap_or_default();
        let parts = code_without_language(code);
        let set = parts
            .as_ref()
            .and_then(|(set, _)| names.get(set))
            .cloned()
            .unwrap_or_else(|| OTHER.to_string());
        let number = card_number(code).unwrap_or_default();
        let language = Language::of_code(code).map_or("—", Language::code);
        let key = (
            copy.english_name().to_string(),
            number,
            copy.rarity.clone().unwrap_or_default(),
            language.to_string(),
            copy.status.clone().unwrap_or_default(),
        );
        *by_set.entry(set).or_default().entry(key).or_insert(0) += 1;
    }

    let mut sets: Vec<SetSection> = by_set
        .into_iter()
        .map(|(name, rows)| {
            let rows: Vec<ListRow> = rows
                .into_iter()
                .map(
                    |((name, number, rarity, language, status), count)| ListRow {
                        status_label: CardStatus::from_code(&status).map_or("", CardStatus::label),
                        name,
                        number,
                        rarity,
                        language,
                        status,
                        count,
                    },
                )
                .collect();
            SetSection {
                name,
                copies: rows.iter().map(|r| r.count).sum(),
                rows,
            }
        })
        .collect();
    // "Other" last; the others stay in name order
    sets.sort_by_key(|s| s.name == OTHER);
    sets
}

pub async fn page(State(state): State<AppState>) -> AppResult<ToSellPage> {
    let copies = flagged(&state).await?;
    let sets = sections(&copies, &set_names(&state).await?);
    Ok(ToSellPage { copies, sets })
}
