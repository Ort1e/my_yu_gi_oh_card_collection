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
//! - clone keeps the ban list but not the tournament (the copy wasn't
//!   played there: it would count twice in the tournament stats);
//! - conditional categories are created here (Django: in the admin only).
//!
//! One file per group of routes; the card JSON shared by all of them is here.

mod categories;
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

use crate::store::db::Money;
use crate::store::models::picture_url;
use crate::web::error::AppResult;

pub use categories::{add_category, assign_category, card_categories, categories, delete_category};
pub use proxies::{add_proxy, delete_proxy, replace_proxy};
pub use reserve::reserve;
pub(crate) use simulation::inputs as simulation_inputs;
pub use simulation::monte_carlo;
pub use version::{clone_version, delete_version, get_version, update_version};
pub use ydke_import::import_ydke;

#[derive(Debug, Serialize)]
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
    price: Option<Money>,
    card_data_id: Option<i64>,
    /// Its picture's address (`store::models::picture_url`).
    image: Option<String>,
    card_type: Option<String>,
    ygopro_id: Option<i64>,
    /// ban list id -> "Banned" | "Limited" | "Semi-Limited"
    ban: BTreeMap<i64, String>,
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
                card_data.card_type, card_data.ygopro_id
         FROM card LEFT JOIN card_data ON card_data.id = card.card_data_id WHERE card.id IN ",
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
    let mut bans: HashMap<i64, BTreeMap<i64, String>> = HashMap::new();
    if !data_ids.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT card_data_id, ban_list_id, status FROM ban_list_entry WHERE card_data_id IN ",
        );
        push_ids(&mut qb, data_ids);
        for (data_id, ban_list_id, status) in qb
            .build_query_as::<(i64, i64, String)>()
            .fetch_all(&mut *conn)
            .await?
        {
            bans.entry(data_id).or_default().insert(ban_list_id, status);
        }
    }

    let mut by_id: HashMap<i64, ApiCard> = records
        .into_iter()
        .map(|r| {
            let card = ApiCard {
                id: r.id,
                price: prices.get(&r.id).copied(),
                ban: r
                    .card_data_id
                    .and_then(|d| bans.get(&d).cloned())
                    .unwrap_or_default(),
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

#[derive(Debug, Serialize)]
pub struct Created {
    id: i64,
}
