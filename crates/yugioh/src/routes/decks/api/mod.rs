//! JSON API of the deck builder (Svelte app in `frontend/`).
//! Django: `rest_api/deck_builder_api.py`, `DeckVersion` model methods.
//!
//! Deliberate differences from Django:
//! - a version is a list of cards and counts (`store::lists`), not of
//!   copies: no proxy card rows, only a built version holds copies;
//! - categories are given to a card of a version, not to a copy;
//! - saving the zones drops the category assignments of cards that left the
//!   deck (Django needed the `clean_card_categories` command);
//! - clone keeps the ban list (or format) but not the tournament (the copy wasn't
//!   played there: it would count twice in the tournament stats);
//! - conditional categories are created here (Django: in the admin only).
//!
//! One file per group of routes; the card JSON shared by all of them is here.

mod categories;
mod history;
mod lookup;
mod reserve;
mod simulation;
mod tournament;
mod version;
mod ydke_import;

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, SqliteConnection};
use yugioh_domain::deck_history::{DeckZone, SnapshotList};
use yugioh_domain::ownership::LineRef;

use crate::store::cards::SOLD_EXISTS;
use crate::store::db::{push_ids, Money};
use crate::store::formats;
use crate::store::lists;
use crate::store::models::picture_url;
use crate::web::error::{AppError, AppResult};
use ts_rs::TS;

pub(crate) use self::version_tiles as zone_tiles;
pub use categories::{add_category, assign_category, card_categories, categories, delete_category};
pub use history::{compare, history, name_snapshot};
pub use lookup::{lookup_card, take_free};
pub use reserve::reserve;
pub(crate) use simulation::inputs as simulation_inputs;
pub use simulation::monte_carlo;
pub(crate) use version::copy_version;
pub use version::{check_version, clone_version, delete_version, get_version, update_version};
pub(crate) use ydke_import::import_into_deck;
pub use ydke_import::import_ydke;

/// One copy of a line, as a tile of the deck builder (a line of 3 is 3
/// tiles; the builder groups them back by card when it saves). Also a result
/// of the card search (`reserve`).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ApiCard {
    /// `None`: a card known by its name only (not linked to card data yet):
    /// never owned.
    pub(crate) card_data_id: Option<i64>,
    /// The line's name (a copy's: French for a French card).
    pub(crate) name: String,
    /// The English name as ygoprodeck writes it ("Rainbow Bridge Bifrost").
    pub(crate) name_en: Option<String>,
    /// Its picture's address (`store::models::picture_url`): the copy's
    /// printing for a built deck's copy, else the card's.
    pub(crate) image: Option<String>,
    pub(crate) card_type: Option<String>,
    pub(crate) ygopro_id: Option<i64>,
    /// ban list id (`"12"`) or format (`"f3"`) -> "Banned" | "Limited" |
    /// "Semi-Limited"
    pub(crate) ban: BTreeMap<String, String>,
    /// Its first TCG release (`YYYY-MM-DD`), for the formats' card pools;
    /// `None`: unknown (an OCG card, or not linked yet).
    pub(crate) first_release: Option<String>,
    /// Cardmarket's trend price in EUR: the copy's linked product, else the
    /// cheapest product named like the card (what buying it would cost);
    /// `None`: no price known.
    #[ts(type = "string | null")]
    pub(crate) cardmarket_trend: Option<Money>,
    /// What was paid: the copy's price, else the cheapest paid for the card
    /// (an owned tile); `None` for a missing one.
    #[ts(type = "string | null")]
    pub(crate) price: Option<Money>,
    /// The collection's copies of the card (not sold), the ones free (held
    /// by no built version) and the ones held by this version; the same on
    /// every tile of the card. The builder works out `missing` from them:
    /// the tiles of a card beyond `free + here`, in zone order.
    pub(crate) owned: u32,
    pub(crate) free: u32,
    pub(crate) here: u32,
    /// Not owned: to buy (`ownership::missing`).
    pub(crate) missing: bool,
    /// A built version's copy in this slot; `None` in a plain version, or a
    /// slot without one yet (missing, or a free copy to take).
    pub(crate) copy: Option<ApiCopy>,
    /// An owned copy of the card (unsold, the oldest; the slot's copy for a
    /// built deck), for a link to its page; `None`: none owned.
    pub(crate) sample_copy: Option<i64>,
}

/// The copy a built version holds in a slot.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ApiCopy {
    pub(crate) id: i64,
    pub(crate) code: Option<String>,
    pub(crate) rarity: Option<String>,
    pub(crate) status: Option<String>,
    /// Its Cardmarket product (`card.cardmarket_product_id`), `None` when
    /// not linked.
    pub(crate) cardmarket_product_id: Option<i64>,
}

/// One tile to show: a card, maybe the copy in its slot, and whether it is
/// missing.
pub(crate) struct TileSpec {
    pub card_data_id: Option<i64>,
    pub name: String,
    pub copy: Option<i64>,
    pub missing: bool,
}

#[derive(sqlx::FromRow)]
struct DataRecord {
    id: i64,
    name_en: Option<String>,
    card_type: Option<String>,
    ygopro_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct CopyRecord {
    id: i64,
    code: Option<String>,
    rarity: Option<String>,
    last_known_status: Option<String>,
    picture: Option<String>,
    cardmarket_product_id: Option<i64>,
    linked_trend: Option<Money>,
}

/// The tiles of `specs`, in the same order, seen from version `version_id`
/// (what it holds counts as `here`).
async fn load_tiles(
    conn: &mut SqliteConnection,
    version_id: i64,
    specs: &[TileSpec],
) -> AppResult<Vec<ApiCard>> {
    let data_ids: HashSet<i64> = specs.iter().filter_map(|s| s.card_data_id).collect();
    let copy_ids: HashSet<i64> = specs.iter().filter_map(|s| s.copy).collect();

    let mut data: HashMap<i64, DataRecord> = HashMap::new();
    if !data_ids.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT id, json_extract(json_data, '$.name') AS name_en, card_type, ygopro_id
             FROM card_data WHERE id IN ",
        );
        push_ids(&mut qb, data_ids.iter().copied());
        let rows: Vec<DataRecord> = qb.build_query_as().fetch_all(&mut *conn).await?;
        data.extend(rows.into_iter().map(|r| (r.id, r)));
    }

    let mut copies: HashMap<i64, CopyRecord> = HashMap::new();
    if !copy_ids.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT card.id, card.code, card.rarity, card.last_known_status, card.picture,
                    card.cardmarket_product_id, cardmarket_product.trend AS linked_trend
             FROM card
             LEFT JOIN cardmarket_product ON cardmarket_product.id = card.cardmarket_product_id
             WHERE card.id IN ",
        );
        push_ids(&mut qb, copy_ids.iter().copied());
        let rows: Vec<CopyRecord> = qb.build_query_as().fetch_all(&mut *conn).await?;
        copies.extend(rows.into_iter().map(|r| (r.id, r)));
    }

    // an owned copy of each card, for a link
    let mut sample: HashMap<i64, i64> = HashMap::new();
    if !data_ids.is_empty() {
        let mut qb = QueryBuilder::new(format!(
            "SELECT card.card_data_id, min(card.id) FROM card
             WHERE NOT {SOLD_EXISTS} AND card.card_data_id IN "
        ));
        push_ids(&mut qb, data_ids.iter().copied());
        qb.push(" GROUP BY card.card_data_id");
        sample.extend(
            qb.build_query_as::<(i64, i64)>()
                .fetch_all(&mut *conn)
                .await?,
        );
    }

    // what was paid: per copy, and the cheapest per card (unsold copies)
    let mut paid: HashMap<i64, Money> = HashMap::new();
    let mut cheapest: HashMap<i64, Money> = HashMap::new();
    if !data_ids.is_empty() {
        let mut qb = QueryBuilder::new(format!(
            "SELECT card.id, card.card_data_id, unite.price, {SOLD_EXISTS}
             FROM card JOIN unite ON unite.card_id = card.id
             JOIN lot ON lot.id = unite.lot_id AND lot.lot_type != 'sale'
             WHERE unite.price IS NOT NULL AND card.card_data_id IN "
        ));
        push_ids(&mut qb, data_ids.iter().copied());
        for (card_id, data_id, price, sold) in qb
            .build_query_as::<(i64, i64, Money, bool)>()
            .fetch_all(&mut *conn)
            .await?
        {
            let best = paid.entry(card_id).or_insert(price);
            *best = (*best).min(price);
            if !sold {
                let best = cheapest.entry(data_id).or_insert(price);
                *best = (*best).min(price);
            }
        }
    }

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
    // the cheapest product named like the card (compared here, a price is text)
    let mut named_trends: HashMap<i64, Money> = HashMap::new();
    if !data_ids.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT card_data.id, cardmarket_product.trend
             FROM card_data JOIN cardmarket_product ON cardmarket_product.name_key = card_data.en_name
             WHERE cardmarket_product.trend IS NOT NULL AND card_data.id IN ",
        );
        push_ids(&mut qb, data_ids.iter().copied());
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
    let available = lists::availability(&mut *conn, version_id, data_ids.iter().copied()).await?;

    Ok(specs
        .iter()
        .map(|spec| {
            let d = spec.card_data_id.and_then(|id| data.get(&id));
            let c = spec.copy.and_then(|id| copies.get(&id));
            let a = spec
                .card_data_id
                .and_then(|id| available.get(&id))
                .copied()
                .unwrap_or_default();
            ApiCard {
                card_data_id: spec.card_data_id,
                name: spec.name.clone(),
                name_en: d.and_then(|d| d.name_en.clone()),
                image: picture_url(c.and_then(|c| c.picture.as_deref()), spec.card_data_id),
                card_type: d.and_then(|d| d.card_type.clone()),
                ygopro_id: d.and_then(|d| d.ygopro_id),
                ban: spec
                    .card_data_id
                    .and_then(|id| bans.get(&id).cloned())
                    .unwrap_or_default(),
                first_release: spec
                    .card_data_id
                    .and_then(|id| releases.get(&id).copied().flatten())
                    .map(|d| d.to_string()),
                cardmarket_trend: c.and_then(|c| c.linked_trend).or_else(|| {
                    spec.card_data_id
                        .and_then(|id| named_trends.get(&id).copied())
                }),
                price: match (c, spec.missing) {
                    (Some(c), _) => paid.get(&c.id).copied(),
                    (None, true) => None,
                    (None, false) => spec.card_data_id.and_then(|id| cheapest.get(&id).copied()),
                },
                owned: a.owned as u32,
                free: a.free as u32,
                here: a.here as u32,
                missing: spec.missing,
                sample_copy: spec
                    .copy
                    .or_else(|| spec.card_data_id.and_then(|id| sample.get(&id).copied())),
                copy: c.map(|c| ApiCopy {
                    id: c.id,
                    code: c.code.clone(),
                    rarity: c.rarity.clone(),
                    status: c.last_known_status.clone(),
                    cardmarket_product_id: c.cardmarket_product_id,
                }),
            }
        })
        .collect())
}

/// The tiles of a version's zones: each line's copies (a built version's
/// in its slots), the last ones of a line missing when the collection lacks
/// them (`ownership::missing`).
pub(crate) async fn version_tiles(
    conn: &mut SqliteConnection,
    version_id: i64,
) -> AppResult<[Vec<ApiCard>; 3]> {
    let lines = lists::lines(&mut *conn, version_id).await?;
    let slots = lists::copies(&mut *conn, version_id).await?;
    let short: HashMap<LineRef, u32> = lists::missing(&mut *conn, version_id, &lines)
        .await?
        .into_iter()
        .map(|m| (m.line, m.count))
        .collect();

    let mut zones: [Vec<TileSpec>; 3] = Default::default();
    for line in &lines {
        let at = line.line_ref();
        let held = slots.get(&at);
        let lacking = short.get(&at).copied().unwrap_or(0);
        for i in 0..line.count {
            zones[line.zone as usize].push(TileSpec {
                card_data_id: line.card_data_id,
                name: line.name.clone(),
                copy: held.and_then(|s| s.get(i as usize).copied().flatten()),
                missing: i >= line.count - lacking,
            });
        }
    }
    let [main, extra, side] = zones;
    Ok([
        load_tiles(conn, version_id, &main).await?,
        load_tiles(conn, version_id, &extra).await?,
        load_tiles(conn, version_id, &side).await?,
    ])
}

/// One tile of a card, for the search and "+ Missing card": missing when
/// no copy of it is free for this version.
pub(crate) async fn card_tiles(
    conn: &mut SqliteConnection,
    version_id: i64,
    cards: &[(Option<i64>, String)],
) -> AppResult<Vec<ApiCard>> {
    let available = lists::availability(
        &mut *conn,
        version_id,
        cards.iter().filter_map(|(id, _)| *id),
    )
    .await?;
    let specs: Vec<TileSpec> = cards
        .iter()
        .map(|(card_data_id, name)| TileSpec {
            card_data_id: *card_data_id,
            name: name.clone(),
            copy: None,
            missing: card_data_id
                .and_then(|id| available.get(&id))
                .is_none_or(|a| a.free + a.here == 0),
        })
        .collect();
    load_tiles(conn, version_id, &specs).await
}

/// One tile as the builder sends it back: a copy of a card (the save, the
/// test draw, the comparison with a snapshot). The tiles of a zone are
/// grouped by card into lines (`SnapshotList::from_cards`).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct CardInput {
    card_data_id: Option<i64>,
    name: String,
}

impl CardInput {
    pub(crate) fn new(card_data_id: Option<i64>, name: &str) -> CardInput {
        CardInput {
            card_data_id,
            name: name.to_string(),
        }
    }
}

/// The zones the builder sent, as a list; refused when a card data is
/// unknown or a name-only card has no name.
pub(crate) async fn input_list(
    conn: &mut SqliteConnection,
    zones: [(DeckZone, &[CardInput]); 3],
) -> AppResult<SnapshotList> {
    let ids: HashSet<i64> = zones
        .iter()
        .flat_map(|(_, cards)| cards.iter().filter_map(|c| c.card_data_id))
        .collect();
    if !ids.is_empty() {
        let mut qb = QueryBuilder::new("SELECT count(*) FROM card_data WHERE id IN ");
        push_ids(&mut qb, ids.iter().copied());
        let found: i64 = qb.build_query_scalar().fetch_one(&mut *conn).await?;
        if found != ids.len() as i64 {
            return Err(AppError::BadRequest(
                "Some cards don't exist any more, reload the page.".into(),
            ));
        }
    }
    if zones
        .iter()
        .any(|(_, cards)| cards.iter().any(|c| c.name.trim().is_empty()))
    {
        return Err(AppError::BadRequest("A card has no name.".into()));
    }
    Ok(SnapshotList::from_cards(zones.iter().flat_map(
        |(zone, cards)| cards.iter().map(|c| (*zone, c.card_data_id, c.name.trim())),
    )))
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Created {
    id: i64,
}
