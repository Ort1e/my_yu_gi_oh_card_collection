//! Proxy cards added to (or removed from) a deck version, and replaced by
//! an owned copy by hand (the only way for a built version: the automatic
//! `replace_proxies` leaves built versions alone).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{load_cards, ApiCard};
use crate::store::card_data::get_or_fetch;
use crate::store::cards::{
    delete_orphan_proxies, free_copy, new_proxy, proxy_copy, swap_card, version_card_ids, Zone,
};
use crate::store::db::{begin_write, exists};
use crate::web::error::{bad_request, required, AppResult, OrNotFound};
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

#[derive(Debug, Serialize)]
pub struct ReplacedProxy {
    /// The owned copy now in the version, in place of the proxy.
    card: ApiCard,
    /// "Deck - version" of each deck the copy was taken from (built version
    /// only), which now has a proxy of it.
    proxied_in: Vec<String>,
}

/// Put an owned copy (`free_copy`) in place of the proxy `card_id` of
/// version `id`. For a built version, a copy used in another deck is taken
/// from it like building does: that deck gets a proxy instead.
pub async fn replace_proxy(
    State(state): State<AppState>,
    Path((id, card_id)): Path<(i64, i64)>,
) -> AppResult<Json<ReplacedProxy>> {
    let mut tx = begin_write(&state.pool).await?;
    let is_built: bool = sqlx::query_scalar("SELECT is_built FROM deck_version WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .or_404()?;
    let (name, is_proxy, card_data_id): (String, bool, Option<i64>) =
        sqlx::query_as("SELECT name, is_proxy, card_data_id FROM card WHERE id = ?")
            .bind(card_id)
            .fetch_optional(&mut *tx)
            .await?
            .or_404()?;

    if !is_proxy || !version_card_ids(&mut tx, id).await?.contains(&card_id) {
        return bad_request("This card is not a proxy of this deck version.");
    }

    let copy = match card_data_id {
        Some(card_data_id) => free_copy(&mut tx, id, card_data_id, is_built).await?,
        None => None,
    };
    let Some(copy) = copy else {
        return bad_request(format!("No free copy of {name} in your collection."));
    };

    let mut proxied_in = Vec::new();
    if is_built {
        // the copy can only be in non-built versions: `free_copy` skips locked cards
        let others: Vec<(i64, String)> = sqlx::query_as(
            "SELECT deck_version.id,
                    coalesce(deck.name, 'No Deck') || ' - ' || deck_version.version_name
             FROM deck_version LEFT JOIN deck ON deck.id = deck_version.deck_id
             WHERE deck_version.id != ?1 AND deck_version.id IN (
                 SELECT deck_version_id FROM deck_version_main_deck WHERE card_id = ?2
                 UNION SELECT deck_version_id FROM deck_version_extra_deck WHERE card_id = ?2
                 UNION SELECT deck_version_id FROM deck_version_side_deck WHERE card_id = ?2)
             ORDER BY deck_version.id",
        )
        .bind(id)
        .bind(copy)
        .fetch_all(&mut *tx)
        .await?;
        for (other, version_name) in others {
            let proxy = proxy_copy(&mut tx, copy).await?;
            swap_card(&mut tx, other, copy, proxy).await?;
            proxied_in.push(version_name);
        }
    }

    swap_card(&mut tx, id, card_id, copy).await?;
    delete_orphan_proxies(&mut tx).await?;
    let card = load_cards(&mut tx, &[copy])
        .await?
        .pop()
        .expect("the copy exists");
    tx.commit().await?;

    tracing::info!(
        version_id = id,
        proxy_id = card_id,
        card_id = copy,
        proxied_in = proxied_in.len(),
        "proxy replaced by an owned copy"
    );
    Ok(Json(ReplacedProxy { card, proxied_in }))
}
