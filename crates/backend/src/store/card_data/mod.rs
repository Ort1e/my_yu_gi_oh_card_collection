//! `card_data` rows: reference data from the ygoprodeck API
//! (Django: `CardData.get_or_fetch`, `ImageDownloader.get_card_data`).
//!
//! Every call to ygoprodeck goes through this module, `throttle()`d:
//! - here: the client, and a card's data by name or ygopro id;
//! - [`translations`]: card names in other languages (`card_translation`);
//! - [`sets`]: the cards of a set (quick add, Cardmarket expansions);
//! - [`linking`]: new cards linked to their data in the background.

mod linking;
mod sets;
mod translations;

pub use linking::spawn_link_cards;
pub use sets::{fetch_set, fetch_set_infos, SetInfo};
pub use translations::{plan_translations, Translation};

use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::Mutex;
use yugioh_domain::card::clean_card_name;

use crate::store::db::Pool;
use crate::web::error::{AppError, AppResult};

pub(crate) const API_URL: &str = "https://db.ygoprodeck.com/api/v7/cardinfo.php";
pub(super) const SETS_URL: &str = "https://db.ygoprodeck.com/api/v7/cardsets.php";

/// ygoprodeck allows 20 requests per second; stay well below.
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(60);
static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::const_new(None);

/// Wait so that two API requests are at least `MIN_REQUEST_INTERVAL` apart.
pub(crate) async fn throttle() {
    let mut last = LAST_REQUEST.lock().await;
    if let Some(at) = *last {
        tokio::time::sleep_until((at + MIN_REQUEST_INTERVAL).into()).await;
    }
    *last = Some(Instant::now());
}

/// Find the `card_data` of a card by its English name, fetching and storing
/// it from ygoprodeck when unknown. `Ok(None)` when the API doesn't know it.
pub async fn get_or_fetch(
    pool: &Pool,
    http: &reqwest::Client,
    en_name: &str,
) -> AppResult<Option<i64>> {
    if let Some(id) = find(pool, en_name).await? {
        return Ok(Some(id));
    }
    let name = clean_card_name(en_name);

    match fetch(http, &[("name", &name)]).await? {
        Some(data) => store(pool, &name, &data).await.map(Some),
        None => Ok(None),
    }
}

/// The stored `card_data` of a card name, without calling the API.
pub async fn find(pool: &Pool, en_name: &str) -> AppResult<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM card_data WHERE en_name = ?")
            .bind(clean_card_name(en_name))
            .fetch_optional(pool)
            .await?,
    )
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
    let Some(data) = fetch(http, &[("id", &ygopro_id.to_string())]).await? else {
        return Ok(None);
    };
    let name = clean_card_name(data["name"].as_str().unwrap_or_default());
    let id = store(pool, &name, &data).await?;
    Ok(Some((id, name)))
}

/// First card of a ygoprodeck search, `None` when unknown.
pub(super) async fn fetch(
    http: &reqwest::Client,
    query: &[(&str, &str)],
) -> AppResult<Option<Value>> {
    let internal = |e: reqwest::Error| AppError::Internal(format!("ygoprodeck: {e}"));
    throttle().await;
    let response = http
        .get(API_URL)
        .query(query)
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
pub(super) async fn store(pool: &Pool, name: &str, data: &Value) -> AppResult<i64> {
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
