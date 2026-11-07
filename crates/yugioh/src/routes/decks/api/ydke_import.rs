//! New deck version from a YDKE URL.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use sqlx::SqliteConnection;
use yugioh_domain::card::normalize_card_name;
use yugioh_domain::ydke::YdkeDeck;

use super::Created;
use crate::store::card_data::get_or_fetch_by_ygopro_id;
use crate::store::cards::{locked_elsewhere, new_proxy, Zone, SOLD_EXISTS};
use crate::store::db::{begin_write, exists};
use crate::store::snapshots;
use crate::web::error::{bad_request, non_blank, AppResult};
use crate::AppState;
use ts_rs::TS;

#[derive(Debug, Deserialize, TS)]
#[ts(export, optional_fields = nullable)]
pub struct YdkeInput {
    ydke: String,
    name: Option<String>,
}

/// New version of `deck_id` from a YDKE URL. Each card uses an owned copy
/// not already used by this import nor locked by a built version when there
/// is one, else a new proxy.
pub async fn import_ydke(
    State(state): State<AppState>,
    Path(deck_id): Path<i64>,
    Json(input): Json<YdkeInput>,
) -> AppResult<(StatusCode, Json<Created>)> {
    let name = non_blank(input.name).unwrap_or_else(|| "Imported".to_string());
    let version_id = import_into_deck(&state, deck_id, &input.ydke, &name).await?;
    Ok((StatusCode::CREATED, Json(Created { id: version_id })))
}

/// The import itself (also the deck page's "new version" form): the id of
/// the new version `name` of `deck_id`.
pub(crate) async fn import_into_deck(
    state: &AppState,
    deck_id: i64,
    ydke: &str,
    name: &str,
) -> AppResult<i64> {
    let deck = match YdkeDeck::parse(ydke.trim()) {
        Ok(d) => d,
        Err(e) => return bad_request(e.to_string()),
    };
    exists(&mut *state.pool.acquire().await?, "deck", deck_id).await?;

    // Resolve card data first (network), outside the transaction.
    let mut data: HashMap<u32, Option<(i64, String)>> = HashMap::new();
    for &ygopro_id in deck.main.iter().chain(&deck.extra).chain(&deck.side) {
        if let Entry::Vacant(entry) = data.entry(ygopro_id) {
            entry.insert(
                get_or_fetch_by_ygopro_id(&state.pool, &state.http, ygopro_id.into()).await?,
            );
        }
    }

    let card = |ygopro_id: &u32| match &data[ygopro_id] {
        Some((data_id, en_name)) => ListCard {
            card_data: Some((*data_id, en_name.clone())),
            name: en_name.clone(),
        },
        None => ListCard {
            card_data: None,
            name: format!("Unknown-{ygopro_id}"),
        },
    };
    let zones = [
        (Zone::Main, deck.main.iter().map(card).collect()),
        (Zone::Extra, deck.extra.iter().map(card).collect()),
        (Zone::Side, deck.side.iter().map(card).collect()),
    ];

    let mut tx = begin_write(&state.pool).await?;
    let version_id = create_version_from_list(&mut tx, deck_id, name, zones).await?;
    snapshots::record(&mut tx, version_id).await?;
    tx.commit().await?;
    tracing::info!(deck_id, version_id, "deck version imported from YDKE");
    Ok(version_id)
}

/// A card of a list to make a version of: its card data (id, English
/// name), or only a name for a card unknown to ygoprodeck.
pub(crate) struct ListCard {
    pub card_data: Option<(i64, String)>,
    pub name: String,
}

/// New version `name` of `deck_id` holding these cards: each one an owned
/// copy not already used by this list nor locked by a built version when
/// there is one, else a new proxy. Its id; call inside a write transaction.
pub(crate) async fn create_version_from_list(
    tx: &mut SqliteConnection,
    deck_id: i64,
    name: &str,
    zones: [(Zone, Vec<ListCard>); 3],
) -> sqlx::Result<i64> {
    let version_id: i64 =
        sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (?, ?)")
            .bind(deck_id)
            .bind(name)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();

    let mut used: HashSet<i64> = HashSet::new();
    for (zone, cards) in zones {
        for card in cards {
            let card_id = match &card.card_data {
                Some((data_id, en_name)) => {
                    let owned: Vec<i64> = sqlx::query_scalar(&format!(
                        "SELECT id FROM card
                         WHERE is_proxy = 0 AND (card_data_id = ? OR en_name = ?)
                           AND NOT {SOLD_EXISTS} AND id NOT IN ({}) ORDER BY id",
                        locked_elsewhere(0)
                    ))
                    .bind(data_id)
                    .bind(normalize_card_name(en_name))
                    .fetch_all(&mut *tx)
                    .await?;
                    match owned.into_iter().find(|id| !used.contains(id)) {
                        Some(id) => id,
                        None => new_proxy(&mut *tx, en_name, Some(*data_id)).await?,
                    }
                }
                None => new_proxy(&mut *tx, &card.name, None).await?,
            };
            used.insert(card_id);
            zone.insert(&mut *tx, version_id, card_id).await?;
        }
    }
    Ok(version_id)
}
