//! New deck version from a YDKE URL.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use sqlx::SqliteConnection;
use yugioh_domain::deck_history::SnapshotList;
use yugioh_domain::ydke::YdkeDeck;

use super::Created;
use crate::store::card_data::get_or_fetch_by_ygopro_id;
use crate::store::cards::Zone;
use crate::store::db::{begin_write, exists};
use crate::store::lists;
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

/// New version of `deck_id` from a YDKE URL: its cards and counts.
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
struct ListCard {
    card_data: Option<(i64, String)>,
    name: String,
}

/// New version `name` of `deck_id` listing these cards (a plain list: the
/// collection says what is owned). Its id; call inside a write transaction.
async fn create_version_from_list(
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

    let list = SnapshotList::from_cards(zones.iter().flat_map(|(zone, cards)| {
        cards.iter().map(|c| {
            (
                zone.deck_zone(),
                c.card_data.as_ref().map(|(id, _)| *id),
                c.name.as_str(),
            )
        })
    }));
    lists::write_lines(tx, version_id, &list).await?;
    Ok(version_id)
}
