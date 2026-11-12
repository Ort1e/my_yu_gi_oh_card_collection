//! "+ Missing card" (a card by its name, looked up on ygoprodeck when the
//! app doesn't know it) and a built version's "Take free copies".

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{card_tiles, ApiCard};
use crate::store::card_data::get_or_fetch;
use crate::store::db::{begin_write, exists};
use crate::store::lists;
use crate::web::error::{bad_request, required, AppResult, OrNotFound};
use crate::AppState;
use ts_rs::TS;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct LookupInput {
    name: String,
}

/// `POST /api/deck-versions/:id/lookup`: a tile of the card named `name`
/// (its English name, or the name of a card not in ygoprodeck: then a
/// name-only card, never owned). Nothing is saved: the builder adds the tile
/// to a zone, saved with the others.
pub async fn lookup_card(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<LookupInput>,
) -> AppResult<Json<ApiCard>> {
    let name = required(&input.name, "The card name can't be empty.")?;
    exists(&mut *state.pool.acquire().await?, "deck_version", id).await?;
    let card_data_id = get_or_fetch(&state.pool, &state.http, name).await?;
    let mut conn = state.pool.acquire().await?;
    let name =
        match card_data_id {
            Some(card_data_id) => sqlx::query_scalar(
                "SELECT coalesce(json_extract(json_data, '$.name'), ?) FROM card_data WHERE id = ?",
            )
            .bind(name)
            .bind(card_data_id)
            .fetch_one(&mut *conn)
            .await?,
            None => name.to_string(),
        };
    let tile = card_tiles(&mut conn, id, &[(card_data_id, name)])
        .await?
        .pop()
        .expect("one card asked");
    Ok(Json(tile))
}

/// What "Take free copies" did.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Taken {
    /// Copies put in the version's empty slots.
    taken: usize,
    /// Slots still empty: to buy.
    missing: usize,
}

/// `POST /api/deck-versions/:id/take-free`: a built version's empty slots
/// take the free copies of their cards (bought since it was built). Its
/// other copies stay; other versions are not touched.
pub async fn take_free(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Taken>> {
    let mut tx = begin_write(&state.pool).await?;
    let is_built: bool = sqlx::query_scalar("SELECT is_built FROM deck_version WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .or_404()?;
    if !is_built {
        return bad_request("Only a built version holds copies.");
    }
    let before: i64 = sqlx::query_scalar(
        "SELECT count(card_id) FROM deck_version_copy WHERE deck_version_id = ?",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let done = lists::assign_copies(&mut tx, id).await?;
    tx.commit().await?;
    let taken = done.copies.saturating_sub(before as usize);
    tracing::info!(
        version_id = id,
        taken,
        missing = done.missing,
        "free copies taken"
    );
    Ok(Json(Taken {
        taken,
        missing: done.missing,
    }))
}
