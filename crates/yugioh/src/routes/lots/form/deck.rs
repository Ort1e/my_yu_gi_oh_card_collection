//! The deck received in a lot: `/lots/:id/deck`. Its cards are entered like
//! the lot's (card rows, quick add), go into the lot **and** into a built
//! version holding exactly those copies; a snapshot linked to the lot
//! (`deck_snapshot.lot_id`, kept like a tournament's list) is the deck as
//! received. A lot holds any number of decks: `/lots/:id/deck` always makes
//! a new one (which deck, which version), `/lots/:id/deck/:snapshot_id` adds
//! cards to one of them.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use sqlx::FromRow;
use yugioh_domain::deck_history::DeckZone;
use yugioh_domain::stats::{kind, CardKind};

use crate::store::card_data::{get_or_fetch, spawn_link_cards};
use crate::store::cards::insert_lot_cards;
use crate::store::db::{begin_write, exists, Pool};
use crate::store::models::LotRow;
use crate::store::{lists, snapshots};
use crate::web::card_rows::{CardRowValues, CardRows};
use crate::web::error::{AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// The deck received in a lot, as the lot page and this page name it.
#[derive(Debug, Clone, FromRow)]
pub struct LotDeck {
    pub snapshot_id: i64,
    pub deck_id: i64,
    pub deck_name: String,
    pub version_id: Option<i64>,
    pub version_name: Option<String>,
}

const LOT_DECK_SELECT: &str = "SELECT s.id AS snapshot_id, s.deck_id, deck.name AS deck_name,
        s.version_id, v.version_name
     FROM deck_snapshot s
     JOIN deck ON deck.id = s.deck_id
     LEFT JOIN deck_version v ON v.id = s.version_id";

impl LotDeck {
    /// Every deck received in the lot, oldest first.
    pub async fn all_of_lot(pool: &Pool, lot_id: i64) -> sqlx::Result<Vec<LotDeck>> {
        sqlx::query_as(&format!(
            "{LOT_DECK_SELECT} WHERE s.lot_id = ? ORDER BY s.id"
        ))
        .bind(lot_id)
        .fetch_all(pool)
        .await
    }

    /// One of the lot's decks (`None` when the snapshot is another lot's).
    pub async fn one(pool: &Pool, lot_id: i64, snapshot_id: i64) -> sqlx::Result<Option<LotDeck>> {
        sqlx::query_as(&format!(
            "{LOT_DECK_SELECT} WHERE s.lot_id = ? AND s.id = ?"
        ))
        .bind(lot_id)
        .bind(snapshot_id)
        .fetch_optional(pool)
        .await
    }
}

/// The version name proposed for a new deck: `From lot 7`, then
/// `From lot 7 (2)`… when the lot already has decks.
fn default_version_name(lot_id: i64, existing: usize) -> String {
    if existing == 0 {
        format!("From lot {lot_id}")
    } else {
        format!("From lot {lot_id} ({})", existing + 1)
    }
}

#[derive(Template)]
#[template(path = "lots/deck.html")]
pub struct LotDeckPage {
    lot: LotRow,
    /// The deck cards are added to; `None` = a new deck (its fields are shown).
    lot_deck: Option<LotDeck>,
    errors: Vec<String>,
    rows: Vec<CardRowValues>,
    /// The decks to choose from (the first time).
    decks: Vec<(i64, String)>,
    deck_id: String,
    deck_name: String,
    version_name: String,
}

impl LotDeckPage {
    async fn load(
        pool: &Pool,
        lot_id: i64,
        errors: Vec<String>,
        mut rows: Vec<CardRowValues>,
        fields: Option<&FormData>,
        snapshot_id: Option<i64>,
    ) -> AppResult<LotDeckPage> {
        if rows.is_empty() {
            rows.push(CardRowValues::default());
        }
        let lot: LotRow = sqlx::query_as(&format!("{} WHERE lot.id = ?", LotRow::SELECT))
            .bind(lot_id)
            .fetch_optional(pool)
            .await?
            .or_404()?;
        let (lot_deck, existing) = match snapshot_id {
            Some(id) => (Some(LotDeck::one(pool, lot_id, id).await?.or_404()?), 0),
            None => (None, LotDeck::all_of_lot(pool, lot_id).await?.len()),
        };
        let decks = sqlx::query_as("SELECT id, name FROM deck ORDER BY name COLLATE NOCASE")
            .fetch_all(pool)
            .await?;
        let text = |name: &str| fields.map(|f| f.text(name)).unwrap_or_default();
        Ok(LotDeckPage {
            version_name: fields
                .map(|f| f.text("version_name"))
                .unwrap_or_else(|| default_version_name(lot_id, existing)),
            deck_id: text("deck_id"),
            deck_name: text("deck_name"),
            lot,
            lot_deck,
            errors,
            rows,
            decks,
        })
    }

    /// Where the form posts: a new deck, or more cards for one.
    fn form_action(&self) -> String {
        match &self.lot_deck {
            Some(d) => format!("/lots/{}/deck/{}", self.lot.id, d.snapshot_id),
            None => format!("/lots/{}/deck", self.lot.id),
        }
    }

    fn blank_row(&self) -> CardRowValues {
        CardRowValues::default()
    }

    fn statuses(&self) -> [yugioh_domain::card::CardStatus; 5] {
        yugioh_domain::card::CardStatus::ALL
    }

    fn rarities(&self) -> [yugioh_domain::rarity::Rarity; 34] {
        yugioh_domain::rarity::Rarity::ALL
    }

    fn deck_selected(&self, id: &i64) -> bool {
        self.deck_id == id.to_string()
    }
}

pub async fn deck_page(
    State(state): State<AppState>,
    Path(lot_id): Path<i64>,
) -> AppResult<LotDeckPage> {
    LotDeckPage::load(&state.pool, lot_id, Vec::new(), Vec::new(), None, None).await
}

pub async fn deck_cards_page(
    State(state): State<AppState>,
    Path((lot_id, snapshot_id)): Path<(i64, i64)>,
) -> AppResult<LotDeckPage> {
    LotDeckPage::load(
        &state.pool,
        lot_id,
        Vec::new(),
        Vec::new(),
        None,
        Some(snapshot_id),
    )
    .await
}

/// Where a copy goes: the side deck when asked, else the extra deck for the
/// types that live there, else the main deck.
fn zone_of(card_type: &str, side: bool) -> DeckZone {
    if side {
        DeckZone::Side
    } else if kind(card_type) == CardKind::ExtraDeck {
        DeckZone::Extra
    } else {
        DeckZone::Main
    }
}

/// A new deck received in the lot.
pub async fn new_deck(
    State(state): State<AppState>,
    Path(lot_id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    save(state, lot_id, None, pairs).await
}

/// More cards for a deck already received in the lot.
pub async fn add_to_deck(
    State(state): State<AppState>,
    Path((lot_id, snapshot_id)): Path<(i64, i64)>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    save(state, lot_id, Some(snapshot_id), pairs).await
}

async fn save(
    state: AppState,
    lot_id: i64,
    snapshot_id: Option<i64>,
    pairs: Pairs,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let pool = &state.pool;
    exists(&mut *pool.acquire().await?, "lot", lot_id).await?;
    let first_time = snapshot_id.is_none();
    if let Some(id) = snapshot_id {
        LotDeck::one(pool, lot_id, id).await?.or_404()?;
    }

    let mut cards = form.new_cards();
    if cards.is_empty() && form.errors.is_empty() {
        form.errors.push("Card 1: name is required.".into());
    }
    let mut deck_id: Option<i64> = None;
    let mut deck_name: Option<String> = None;
    let mut version_name = String::new();
    if first_time {
        deck_id = form.id("deck_id", "Deck");
        if deck_id.is_none() {
            deck_name = form.required("deck_name", "The new deck's name");
        }
        let existing = LotDeck::all_of_lot(pool, lot_id).await?.len();
        version_name = form
            .get("version_name")
            .map(str::to_string)
            .unwrap_or_else(|| default_version_name(lot_id, existing));
    }

    // network first: every card needs its card data to fill a slot
    let mut types: Vec<String> = Vec::new();
    if form.errors.is_empty() {
        for (n, card) in cards.iter_mut().enumerate() {
            card.card_data_id = get_or_fetch(pool, &state.http, &card.en_name).await?;
            match card.card_data_id {
                Some(id) => types.push(
                    sqlx::query_scalar(
                        "SELECT COALESCE(card_type, json_extract(json_data, '$.type'), '')
                         FROM card_data WHERE id = ?",
                    )
                    .bind(id)
                    .fetch_one(pool)
                    .await?,
                ),
                None => form
                    .errors
                    .push(format!("Card {}: unknown to ygoprodeck.", n + 1)),
            }
        }
    }
    if !form.errors.is_empty() {
        let rows = form.card_rows();
        let page = LotDeckPage::load(
            pool,
            lot_id,
            form.errors.clone(),
            rows,
            Some(&form),
            snapshot_id,
        )
        .await?;
        return Ok(page.into_response());
    }

    let mut tx = begin_write(pool).await?;
    let snapshot = match snapshot_id {
        Some(id) => Some(snapshots::lot_deck(&mut tx, lot_id, id).await?.or_404()?),
        None => None,
    };

    // the version holding the copies, built: made now, or the lot's still there
    let mut version_id: Option<i64> = None;
    if let Some(s) = &snapshot {
        if let Some(v) = s.version_id {
            let built: Option<bool> =
                sqlx::query_scalar("SELECT is_built FROM deck_version WHERE id = ?")
                    .bind(v)
                    .fetch_optional(&mut *tx)
                    .await?;
            if built == Some(true) {
                version_id = Some(v);
            }
        }
    }
    let version_id = match (version_id, &snapshot) {
        (Some(v), _) => v,
        (None, Some(s)) => {
            // the lot's version is gone (or unbuilt): a new one from the
            // list received, before the new copies exist so that it can't
            // take them
            let list = snapshots::cards(&mut tx, s.id).await?;
            let v = new_built_version(&mut tx, s.deck_id, &format!("From lot {lot_id} (again)"))
                .await?;
            lists::write_lines(&mut tx, v, &list).await?;
            lists::assign_copies(&mut tx, v).await?;
            sqlx::query("UPDATE deck_snapshot SET version_id = ? WHERE id = ?")
                .bind(v)
                .bind(s.id)
                .execute(&mut *tx)
                .await?;
            v
        }
        (None, None) => {
            let deck = match (deck_id, &deck_name) {
                (Some(id), _) => {
                    exists(&mut tx, "deck", id).await?;
                    id
                }
                (None, name) => sqlx::query("INSERT INTO deck (name, description) VALUES (?, ?)")
                    .bind(name.as_deref().unwrap_or_default())
                    .bind(format!("Received in lot {lot_id}"))
                    .execute(&mut *tx)
                    .await?
                    .last_insert_rowid(),
            };
            new_built_version(&mut tx, deck, &version_name).await?
        }
    };

    let card_ids = insert_lot_cards(&mut tx, lot_id, &cards).await?;
    // `insert_lot_cards` returns the ids row by row, `quantity` each
    let mut ids = card_ids.iter().copied();
    let mut copies = Vec::with_capacity(card_ids.len());
    for (card, card_type) in cards.iter().zip(&types) {
        let zone = zone_of(card_type, card.side);
        for _ in 0..card.quantity {
            if let (Some(card_data_id), Some(card_id)) = (card.card_data_id, ids.next()) {
                copies.push((zone, card_data_id, card_id));
            }
        }
    }
    let added = lists::add_copies(&mut tx, version_id, &copies).await?;

    match snapshot {
        Some(s) => snapshots::append(&mut tx, s.id, &added).await?,
        None => {
            snapshots::lot_freeze(&mut tx, version_id, lot_id).await?;
        }
    }
    tx.commit().await?;
    tracing::info!(
        lot_id,
        version_id,
        cards = card_ids.len(),
        "lot deck cards added"
    );

    spawn_link_cards(state, card_ids);
    Ok(Redirect::to(&format!("/lots/{lot_id}")).into_response())
}

/// A version of `deck_id` that holds copies, shown on the public site
/// (building turns that on).
async fn new_built_version(
    conn: &mut sqlx::SqliteConnection,
    deck_id: i64,
    name: &str,
) -> AppResult<i64> {
    Ok(sqlx::query(
        "INSERT INTO deck_version (deck_id, version_name, is_built, is_public)
         VALUES (?, ?, 1, 1)",
    )
    .bind(deck_id)
    .bind(name)
    .execute(&mut *conn)
    .await?
    .last_insert_rowid())
}
