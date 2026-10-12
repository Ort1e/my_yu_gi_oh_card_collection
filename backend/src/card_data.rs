//! `card_data` rows: reference data from the ygoprodeck API
//! (Django: `CardData.get_or_fetch`, `ImageDownloader.get_card_data`).

use std::collections::BTreeMap;

use domain::card::clean_card_name;
use serde_json::Value;
use sqlx::{Sqlite, Transaction};

use crate::db::{Money, Pool};
use crate::error::{AppError, AppResult};
use crate::forms::NewCards;
use crate::AppState;

const API_URL: &str = "https://db.ygoprodeck.com/api/v7/cardinfo.php";

/// Find the `card_data` of a card by its English name, fetching and storing
/// it from ygoprodeck when unknown. `Ok(None)` when the API doesn't know it.
pub async fn get_or_fetch(
    pool: &Pool,
    http: &reqwest::Client,
    en_name: &str,
) -> AppResult<Option<i64>> {
    let name = clean_card_name(en_name);
    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM card_data WHERE en_name = ?")
        .bind(&name)
        .fetch_optional(pool)
        .await?;
    if existing.is_some() {
        return Ok(existing);
    }

    match fetch(http, "name", &name).await? {
        Some(data) => store(pool, &name, &data).await.map(Some),
        None => Ok(None),
    }
}

/// Same as [`get_or_fetch`], by ygopro id (used by YDKE import). Returns the
/// `card_data` id and English name.
pub async fn get_or_fetch_by_ygopro_id(
    pool: &Pool,
    http: &reqwest::Client,
    ygopro_id: i64,
) -> AppResult<Option<(i64, String)>> {
    let existing: Option<(i64, String)> =
        sqlx::query_as("SELECT id, en_name FROM card_data WHERE ygopro_id = ?")
            .bind(ygopro_id)
            .fetch_optional(pool)
            .await?;
    if existing.is_some() {
        return Ok(existing);
    }
    let Some(data) = fetch(http, "id", &ygopro_id.to_string()).await? else {
        return Ok(None);
    };
    let name = clean_card_name(data["name"].as_str().unwrap_or_default());
    let id = store(pool, &name, &data).await?;
    Ok(Some((id, name)))
}

/// First card of a ygoprodeck search, `None` when unknown.
async fn fetch(http: &reqwest::Client, param: &str, value: &str) -> AppResult<Option<Value>> {
    let internal = |e: reqwest::Error| AppError::Internal(format!("ygoprodeck: {e}"));
    let response = http
        .get(API_URL)
        .query(&[(param, value)])
        .send()
        .await
        .map_err(internal)?;
    // ygoprodeck answers 400 for an unknown card
    if !response.status().is_success() {
        return Ok(None);
    }
    let body: Value = response.json().await.map_err(internal)?;
    Ok(body["data"].get(0).cloned())
}

/// Insert API data as a `card_data` row (or find the row it duplicates).
async fn store(pool: &Pool, name: &str, data: &Value) -> AppResult<i64> {
    let ygopro_id = data["id"].as_i64();

    // The same card may already be stored under another spelling.
    if let Some(ygopro_id) = ygopro_id {
        let by_id: Option<i64> = sqlx::query_scalar("SELECT id FROM card_data WHERE ygopro_id = ?")
            .bind(ygopro_id)
            .fetch_optional(pool)
            .await?;
        if let Some(id) = by_id {
            return Ok(id);
        }
    }

    // `INSERT ... RETURNING` is avoided everywhere: with sqlx 0.7 its
    // autocommit isn't committed when `fetch_one` returns (see db.rs).
    sqlx::query(
        "INSERT INTO card_data (en_name, ygopro_id, card_type, json_data) VALUES (?, ?, ?, ?)
         ON CONFLICT (en_name) DO NOTHING",
    )
    .bind(name)
    .bind(ygopro_id)
    .bind(data["type"].as_str())
    .bind(data.to_string())
    .execute(pool)
    .await?;
    let id = sqlx::query_scalar("SELECT id FROM card_data WHERE en_name = ?")
        .bind(name)
        .fetch_one(pool)
        .await?;
    tracing::info!("fetched card data for {name:?}");
    Ok(id)
}

/// Link the given cards to their `card_data` (fetching it if needed) in the
/// background, so form submissions don't wait on the API.
pub fn spawn_link_cards(state: AppState, card_ids: Vec<i64>) {
    if card_ids.is_empty() {
        return;
    }
    tokio::spawn(async move {
        if let Err(e) = link_cards(&state, &card_ids).await {
            tracing::warn!("linking card data: {e:?}");
        }
    });
}

async fn link_cards(state: &AppState, card_ids: &[i64]) -> AppResult<()> {
    let mut by_name: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for &id in card_ids {
        let en_name: Option<String> =
            sqlx::query_scalar("SELECT en_name FROM card WHERE id = ? AND card_data_id IS NULL")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?
                .flatten();
        if let Some(en_name) = en_name {
            by_name.entry(en_name).or_default().push(id);
        }
    }
    for (en_name, ids) in by_name {
        match get_or_fetch(&state.pool, &state.http, &en_name).await? {
            Some(data_id) => {
                for id in ids {
                    sqlx::query("UPDATE card SET card_data_id = ? WHERE id = ?")
                        .bind(data_id)
                        .bind(id)
                        .execute(&state.pool)
                        .await?;
                }
            }
            None => tracing::warn!("no card data found for {en_name:?}"),
        }
    }
    Ok(())
}

/// Create the cards of `rows` (each `quantity` times) with one unite each in
/// `lot_id`. Returns the new card ids.
pub async fn insert_lot_cards(
    tx: &mut Transaction<'_, Sqlite>,
    lot_id: i64,
    rows: &[NewCards],
) -> sqlx::Result<Vec<i64>> {
    let mut ids = Vec::new();
    for row in rows {
        for _ in 0..row.quantity {
            let card_id: i64 = sqlx::query(
                "INSERT INTO card (name, en_name, code, last_known_status, is_proxy)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&row.name)
            .bind(&row.en_name)
            .bind(&row.code)
            .bind(row.status.map(|s| s.code()))
            .bind(row.is_proxy)
            .execute(&mut **tx)
            .await?
            .last_insert_rowid();
            sqlx::query("INSERT INTO unite (price, lot_id, card_id) VALUES (?, ?, ?)")
                .bind(row.price.map(Money))
                .bind(lot_id)
                .bind(card_id)
                .execute(&mut **tx)
                .await?;
            ids.push(card_id);
        }
    }
    Ok(ids)
}
