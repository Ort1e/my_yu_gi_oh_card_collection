//! JSON API of the deck builder (Svelte app in `frontend/`).
//! Django: `rest_api/deck_builder_api.py`, `DeckVersion` model methods.
//!
//! Deliberate differences from Django:
//! - deleting a version only removes orphan proxies that are not in a lot
//!   (Django's signal also deleted proxies bought in lots, with their unites);
//! - saving the zones drops the category assignments of cards that left the
//!   deck (Django needed the `clean_card_categories` command);
//! - YDKE import never reuses sold cards, and one owned copy is used once
//!   across all zones;
//! - clone keeps the ban list (or format) but not the tournament (the copy wasn't
//!   played there: it would count twice in the tournament stats);
//! - conditional categories are created here (Django: in the admin only).
//!
//! One file per group of routes; the card JSON shared by all of them is here.

mod categories;
mod history;
mod proxies;
mod reserve;
mod simulation;
mod tournament;
mod version;
mod ydke_import;

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;
use sqlx::{QueryBuilder, SqliteConnection};

use crate::store::db::push_ids;
use crate::store::formats;

use crate::store::db::Money;
use crate::store::models::picture_url;
use crate::web::error::AppResult;
use ts_rs::TS;

pub use categories::{add_category, assign_category, card_categories, categories, delete_category};
pub use history::{compare, history, name_snapshot};
pub use proxies::{add_proxy, delete_proxy, replace_proxy};
pub use reserve::reserve;
pub(crate) use simulation::inputs as simulation_inputs;
pub use simulation::monte_carlo;
pub(crate) use version::copy_version;
pub use version::{check_version, clone_version, delete_version, get_version, update_version};
pub use ydke_import::import_ydke;
pub(crate) use ydke_import::{create_version_from_list, import_into_deck, ListCard};

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ApiCard {
    id: i64,
    name: String,
    en_name: Option<String>,
    /// The English name as ygoprodeck writes it ("Rainbow Bridge Bifrost").
    name_en: Option<String>,
    code: Option<String>,
    status: Option<String>,
    rarity: Option<String>,
    is_proxy: bool,
    /// Cheapest price paid for this card (Django's deck price used the min).
    #[ts(type = "string | null")]
    price: Option<Money>,
    card_data_id: Option<i64>,
    /// Its picture's address (`store::models::picture_url`).
    image: Option<String>,
    card_type: Option<String>,
    ygopro_id: Option<i64>,
    /// ban list id (`"12"`) or format (`"f3"`) -> "Banned" | "Limited" |
    /// "Semi-Limited"
    ban: BTreeMap<String, String>,
    /// Its first TCG release (`YYYY-MM-DD`), for the formats' card pools;
    /// `None`: unknown (an OCG card, or not linked yet).
    first_release: Option<String>,
    /// Its Cardmarket product (`card.cardmarket_product_id`), for the
    /// product page; `None` for a proxy or a copy not linked.
    cardmarket_product_id: Option<i64>,
    /// Cardmarket's trend price in EUR: the linked product's, else the
    /// cheapest product named like the card (what a proxy would cost);
    /// `None`: no price known.
    #[ts(type = "string | null")]
    cardmarket_trend: Option<Money>,
}

#[derive(sqlx::FromRow)]
struct CardRecord {
    id: i64,
    name: String,
    en_name: Option<String>,
    name_en: Option<String>,
    code: Option<String>,
    last_known_status: Option<String>,
    rarity: Option<String>,
    is_proxy: bool,
    card_data_id: Option<i64>,
    picture_file: Option<String>,
    card_type: Option<String>,
    ygopro_id: Option<i64>,
    cardmarket_product_id: Option<i64>,
    linked_trend: Option<Money>,
}

/// The cards with these ids, in the same order (unknown ids are skipped).
async fn load_cards(conn: &mut SqliteConnection, ids: &[i64]) -> AppResult<Vec<ApiCard>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let unique: Vec<i64> = ids
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut qb = QueryBuilder::new(
        "SELECT card.id, card.name, card.en_name,
                json_extract(card_data.json_data, '$.name') AS name_en,
                card.code, card.last_known_status, card.rarity,
                card.is_proxy, card.card_data_id, card.picture AS picture_file,
                card_data.card_type, card_data.ygopro_id,
                card.cardmarket_product_id, cardmarket_product.trend AS linked_trend
         FROM card LEFT JOIN card_data ON card_data.id = card.card_data_id
         LEFT JOIN cardmarket_product ON cardmarket_product.id = card.cardmarket_product_id
         WHERE card.id IN ",
    );
    push_ids(&mut qb, unique.iter().copied());
    let records: Vec<CardRecord> = qb.build_query_as().fetch_all(&mut *conn).await?;

    let mut qb = QueryBuilder::new(
        "SELECT card_id, price FROM unite WHERE price IS NOT NULL AND card_id IN ",
    );
    push_ids(&mut qb, unique.iter().copied());
    let mut prices: HashMap<i64, Money> = HashMap::new();
    for (card_id, price) in qb
        .build_query_as::<(i64, Money)>()
        .fetch_all(&mut *conn)
        .await?
    {
        let best = prices.entry(card_id).or_insert(price);
        *best = (*best).min(price);
    }

    let data_ids: HashSet<i64> = records.iter().filter_map(|r| r.card_data_id).collect();
    // the formats' statuses, then the Konami lists'
    let mut bans = formats::statuses(&mut *conn, data_ids.iter().copied()).await?;
    if !data_ids.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT card_data_id, ban_list_id, status FROM ban_list_entry WHERE card_data_id IN ",
        );
        push_ids(&mut qb, data_ids.iter().copied());
        for (data_id, ban_list_id, status) in qb
            .build_query_as::<(i64, i64, String)>()
            .fetch_all(&mut *conn)
            .await?
        {
            bans.entry(data_id)
                .or_default()
                .insert(ban_list_id.to_string(), status);
        }
    }
    // a card without a linked product: the cheapest product named like it
    // (compared here, a price is text)
    let mut named_trends: HashMap<i64, Money> = HashMap::new();
    let unlinked: Vec<i64> = records
        .iter()
        .filter(|r| r.linked_trend.is_none())
        .filter_map(|r| r.card_data_id)
        .collect();
    if !unlinked.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT card_data.id, cardmarket_product.trend
             FROM card_data JOIN cardmarket_product ON cardmarket_product.name_key = card_data.en_name
             WHERE cardmarket_product.trend IS NOT NULL AND card_data.id IN ",
        );
        push_ids(&mut qb, unlinked.iter().copied());
        for (data_id, trend) in qb
            .build_query_as::<(i64, Money)>()
            .fetch_all(&mut *conn)
            .await?
        {
            let best = named_trends.entry(data_id).or_insert(trend);
            *best = (*best).min(trend);
        }
    }
    let releases = formats::first_releases(&mut *conn, data_ids.iter().copied()).await?;

    let mut by_id: HashMap<i64, ApiCard> = records
        .into_iter()
        .map(|r| {
            let card = ApiCard {
                id: r.id,
                price: prices.get(&r.id).copied(),
                cardmarket_product_id: r.cardmarket_product_id,
                cardmarket_trend: r
                    .linked_trend
                    .or_else(|| r.card_data_id.and_then(|d| named_trends.get(&d).copied())),
                ban: r
                    .card_data_id
                    .and_then(|d| bans.get(&d).cloned())
                    .unwrap_or_default(),
                first_release: r
                    .card_data_id
                    .and_then(|d| releases.get(&d).copied().flatten())
                    .map(|d| d.to_string()),
                name: r.name,
                en_name: r.en_name,
                name_en: r.name_en,
                code: r.code,
                status: r.last_known_status,
                rarity: r.rarity,
                is_proxy: r.is_proxy,
                image: picture_url(r.picture_file.as_deref(), r.card_data_id),
                card_data_id: r.card_data_id,
                card_type: r.card_type,
                ygopro_id: r.ygopro_id,
            };
            (card.id, card)
        })
        .collect();
    // keep the requested order; a duplicated id gets a single entry
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Created {
    id: i64,
}
