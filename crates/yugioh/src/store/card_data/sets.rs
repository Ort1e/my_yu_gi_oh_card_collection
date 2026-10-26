//! The cards of a set: for the quick add (`GET /api/sets/:code`, cached in
//! memory) and for finding a set's Cardmarket expansion.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::Value;
use tokio::sync::Mutex;

use crate::store::db::Pool;
use crate::web::error::{AppError, AppResult};

use super::translations::remember_translation;
use super::{throttle, API_URL, SETS_URL};

/// The ygoprodeck answers for one set prefix (`"RA05"`), for
/// `yugioh_domain::card_set::set_cards`. Several sets can share a prefix: their
/// cards are put together.
#[derive(Debug, Clone)]
pub struct FetchedSet {
    /// Name of the biggest set with that prefix (`"Rarity Collection 5"`).
    pub name: String,
    /// English cards: the `data` arrays of `cardinfo.php?cardset=…`.
    pub en: Vec<Value>,
    /// French cards (`&language=fr`), empty when ygoprodeck has none.
    pub fr: Vec<Value>,
}

/// Answers of [`fetch_set`] by prefix (upper case). A released set doesn't
/// change: no expiry, a restart empties it. Only successful answers are kept,
/// so a set released today is found today.
static SET_CACHE: Mutex<BTreeMap<String, FetchedSet>> = Mutex::const_new(BTreeMap::new());

/// Fetch every card of the sets whose code is `prefix`, `None` for an
/// unknown prefix. Calls `cardsets.php`, then `cardinfo.php` in English and
/// French for each matching set; `throttle()` before each request. Answers
/// are cached ([`SET_CACHE`]), and the French names of a new fetch are
/// remembered as translations (`card_translation`) for free.
pub async fn fetch_set(
    pool: &Pool,
    http: &reqwest::Client,
    prefix: &str,
) -> AppResult<Option<FetchedSet>> {
    let prefix = prefix.trim().to_uppercase();
    // not held during the requests: two loads of a new set at once fetch it twice
    if let Some(set) = SET_CACHE.lock().await.get(&prefix) {
        return Ok(Some(set.clone()));
    }

    let internal = |e: reqwest::Error| AppError::Internal(format!("ygoprodeck: {e}"));
    throttle().await;
    let sets: Value = http
        .get(SETS_URL)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(internal)?
        .json()
        .await
        .map_err(internal)?;

    // the sets with that code (several for ABPF), the biggest first
    let mut matching: Vec<(&str, i64)> = sets
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| {
            s["set_code"]
                .as_str()
                .is_some_and(|code| code.eq_ignore_ascii_case(&prefix))
        })
        .filter_map(|s| {
            Some((
                s["set_name"].as_str()?,
                s["num_of_cards"].as_i64().unwrap_or(0),
            ))
        })
        .collect();
    if matching.is_empty() {
        return Ok(None);
    }
    matching.sort_by_key(|&(_, cards)| std::cmp::Reverse(cards));

    let mut en = Vec::new();
    let mut fr = Vec::new();
    for &(set_name, _) in &matching {
        en.extend(fetch_set_cards(http, set_name, None).await?);
        fr.extend(fetch_set_cards(http, set_name, Some("fr")).await?);
    }
    let set = FetchedSet {
        name: matching[0].0.to_string(),
        en,
        fr,
    };
    tracing::info!(
        prefix = %prefix,
        sets = matching.len(),
        cards = set.en.len(),
        "set fetched from ygoprodeck"
    );

    remember_set_translations(pool, &set).await?;
    SET_CACHE.lock().await.insert(prefix, set.clone());
    Ok(Some(set))
}

/// The French names of a fetched set as translations; a card of the English
/// answer missing from the French one has no translation (yet).
async fn remember_set_translations(pool: &Pool, set: &FetchedSet) -> sqlx::Result<()> {
    let french: HashMap<i64, &str> = set
        .fr
        .iter()
        .filter_map(|c| Some((c["id"].as_i64()?, c["name"].as_str()?)))
        .collect();
    let ids: HashSet<i64> = set.en.iter().filter_map(|c| c["id"].as_i64()).collect();
    for id in ids {
        remember_translation(pool, id, "fr", french.get(&id).copied()).await?;
    }
    Ok(())
}

/// The cards of one set (`cardinfo.php?cardset=<name>`, in `language` when
/// given). Empty when ygoprodeck has none: it answers 400 for "no result",
/// e.g. a set without French names.
async fn fetch_set_cards(
    http: &reqwest::Client,
    set_name: &str,
    language: Option<&str>,
) -> AppResult<Vec<Value>> {
    let internal = |e: reqwest::Error| AppError::Internal(format!("ygoprodeck: {e}"));
    let mut query = vec![("cardset", set_name)];
    if let Some(language) = language {
        query.push(("language", language));
    }
    throttle().await;
    let response = http
        .get(API_URL)
        .query(&query)
        .send()
        .await
        .map_err(internal)?;
    if !response.status().is_success() {
        return Ok(Vec::new());
    }
    let mut body: Value = response.json().await.map_err(internal)?;
    Ok(match body["data"].take() {
        Value::Array(cards) => cards,
        _ => Vec::new(),
    })
}

/// What ygoprodeck says about a set code, to find its Cardmarket expansion
/// (`store::prices::cardmarket`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetInfo {
    /// Its cards' names (`clean_card_name`), empty for an unknown code.
    pub names: HashSet<String>,
    pub release: Option<chrono::NaiveDate>,
}

/// [`SetInfo`] of each set code of `prefixes` (`"SUDA"`): `cardsets.php`
/// once, then the English cards of each set with that code.
pub async fn fetch_set_infos(
    http: &reqwest::Client,
    prefixes: &[String],
) -> AppResult<HashMap<String, SetInfo>> {
    let mut infos = HashMap::new();
    if prefixes.is_empty() {
        return Ok(infos);
    }

    let internal = |e: reqwest::Error| AppError::Internal(format!("ygoprodeck: {e}"));
    throttle().await;
    let sets: Value = http
        .get(SETS_URL)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(internal)?
        .json()
        .await
        .map_err(internal)?;

    for prefix in prefixes {
        let (set_names, release) = yugioh_domain::card_set::sets_with_code(&sets, prefix);
        let mut cards = Vec::new();
        for set_name in &set_names {
            cards.extend(fetch_set_cards(http, set_name, None).await?);
        }
        let names = yugioh_domain::card_set::set_card_names(prefix, &cards);
        infos.insert(prefix.clone(), SetInfo { names, release });
    }
    Ok(infos)
}
