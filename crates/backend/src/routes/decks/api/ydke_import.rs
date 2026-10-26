//! New deck version from a YDKE URL.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use yugioh_domain::card::normalize_card_name;
use yugioh_domain::ydke::YdkeDeck;
use serde::Deserialize;

use super::Created;
use crate::store::card_data::get_or_fetch_by_ygopro_id;
use crate::store::cards::{locked_elsewhere, new_proxy, Zone, SOLD_EXISTS};
use crate::store::db::{begin_write, exists};
use crate::web::error::{bad_request, non_blank, AppResult};
use crate::AppState;

#[derive(Debug, Deserialize)]
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
    let deck = match YdkeDeck::parse(input.ydke.trim()) {
        Ok(d) => d,
        Err(e) => return bad_request(e.to_string()),
    };
    exists(&mut *state.pool.acquire().await?, "deck", deck_id).await?;
    let name = non_blank(input.name).unwrap_or_else(|| "Imported".to_string());

    // Resolve card data first (network), outside the transaction.
    let mut data: HashMap<u32, Option<(i64, String)>> = HashMap::new();
    for &ygopro_id in deck.main.iter().chain(&deck.extra).chain(&deck.side) {
        if let Entry::Vacant(entry) = data.entry(ygopro_id) {
            entry.insert(
                get_or_fetch_by_ygopro_id(&state.pool, &state.http, ygopro_id.into()).await?,
            );
        }
    }

    let mut tx = begin_write(&state.pool).await?;
    let version_id: i64 =
        sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (?, ?)")
            .bind(deck_id)
            .bind(&name)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();

    let mut used: HashSet<i64> = HashSet::new();
    for (zone, ids) in [
        (Zone::Main, &deck.main),
        (Zone::Extra, &deck.extra),
        (Zone::Side, &deck.side),
    ] {
        for ygopro_id in ids {
            let card_id = match &data[ygopro_id] {
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
                        None => new_proxy(&mut tx, en_name, Some(*data_id)).await?,
                    }
                }
                None => {
                    let name = format!("Unknown-{ygopro_id}");
                    new_proxy(&mut tx, &name, None).await?
                }
            };
            used.insert(card_id);
            zone.insert(&mut tx, version_id, card_id).await?;
        }
    }
    tx.commit().await?;
    tracing::info!(deck_id, version_id, "deck version imported from YDKE");
    Ok((StatusCode::CREATED, Json(Created { id: version_id })))
}
