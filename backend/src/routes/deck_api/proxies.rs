//! Proxy cards added to (or removed from) a deck version.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use super::{load_cards, ApiCard};
use crate::card_data::get_or_fetch;
use crate::cards::{delete_orphan_proxies, new_proxy, Zone};
use crate::db::{begin_write, exists};
use crate::error::{bad_request, required, AppResult, OrNotFound};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ProxyInput {
    name: String,
    zone: Zone,
}

pub async fn add_proxy(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<ProxyInput>,
) -> AppResult<(StatusCode, Json<ApiCard>)> {
    let name = required(&input.name, "The card name can't be empty.")?;
    exists(&mut *state.pool.acquire().await?, "deck_version", id).await?;
    // network call, before the transaction
    let card_data_id = get_or_fetch(&state.pool, &state.http, name).await?;

    let mut tx = begin_write(&state.pool).await?;
    let card_id = new_proxy(&mut tx, name, card_data_id).await?;
    input.zone.insert(&mut tx, id, card_id).await?;
    let card = load_cards(&mut tx, &[card_id])
        .await?
        .pop()
        .expect("just inserted");
    tx.commit().await?;
    tracing::info!(version_id = id, card_id, zone = ?input.zone, "proxy added");
    Ok((StatusCode::CREATED, Json(card)))
}

pub async fn delete_proxy(
    State(state): State<AppState>,
    Path((id, card_id)): Path<(i64, i64)>,
) -> AppResult<StatusCode> {
    let mut tx = begin_write(&state.pool).await?;
    let is_proxy: bool = sqlx::query_scalar("SELECT is_proxy FROM card WHERE id = ?")
        .bind(card_id)
        .fetch_optional(&mut *tx)
        .await?
        .or_404()?;
    if !is_proxy {
        return bad_request("Only proxy cards can be deleted here.");
    }
    for zone in Zone::ALL {
        sqlx::query(&format!(
            "DELETE FROM {} WHERE deck_version_id = ? AND card_id = ?",
            zone.table()
        ))
        .bind(id)
        .bind(card_id)
        .execute(&mut *tx)
        .await?;
    }
    delete_orphan_proxies(&mut tx).await?;
    tx.commit().await?;
    tracing::info!(version_id = id, card_id, "proxy removed");
    Ok(StatusCode::NO_CONTENT)
}
