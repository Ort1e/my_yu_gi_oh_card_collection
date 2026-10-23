//! The [TCGdex API](https://tcgdex.dev): free, open source, no key. Every
//! call of the app to TCGdex goes through here, one at a time with
//! [`MIN_REQUEST_INTERVAL`] between them (it publishes no limit: be polite).
//!
//! Its filters (`?name=`, `?abbreviation.official=`) match "contains,
//! ignoring the case" (`eq:` would be exact but case-sensitive): callers
//! keep the exact matches. Answers are reshaped in `domain::pokemon`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::Value;
use tokio::sync::{Mutex, OnceCell};

use crate::web::error::AppError;

const BASE_URL: &str = "https://api.tcgdex.net/v2";
const TIMEOUT: Duration = Duration::from_secs(15);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(100);
/// Name searches: the first results only.
pub const SEARCH_LIMIT: usize = 60;

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::const_new(None);

async fn throttle() {
    let mut last = LAST_REQUEST.lock().await;
    if let Some(at) = *last {
        tokio::time::sleep_until((at + MIN_REQUEST_INTERVAL).into()).await;
    }
    *last = Some(Instant::now());
}

fn error(what: &str, why: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("TCGdex {what}: {why}"))
}

/// `GET {BASE_URL}/{path}`; `None` for a 404.
async fn get(
    http: &reqwest::Client,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Option<Value>, AppError> {
    throttle().await;
    let response = http
        .get(format!("{BASE_URL}/{path}"))
        .query(query)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| error(path, e))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(error(path, format!("HTTP {}", response.status())));
    }

    response.json().await.map(Some).map_err(|e| error(path, e))
}

fn segment(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

/// A card, in a language (`en`, `fr`, `ja`…); `None` when TCGdex has no
/// such card in it.
pub async fn card(
    http: &reqwest::Client,
    language: &str,
    id: &str,
) -> Result<Option<Value>, AppError> {
    get(http, &format!("{language}/cards/{}", segment(id)), &[]).await
}

/// A set and its card list; `None` when unknown in this language. Set ids
/// are not case-sensitive here (`sv6a` is `SV6a`).
pub async fn set(
    http: &reqwest::Client,
    language: &str,
    id: &str,
) -> Result<Option<Value>, AppError> {
    get(http, &format!("{language}/sets/{}", segment(id)), &[]).await
}

/// The sets whose `field` (`abbreviation.official`,
/// `abbreviation.localized`) contains `value`, ignoring the case (brief:
/// id, name, cardCount).
pub async fn sets_where(
    http: &reqwest::Client,
    language: &str,
    field: &str,
    value: &str,
) -> Result<Vec<Value>, AppError> {
    let answer = get(http, &format!("{language}/sets"), &[(field, value)]).await?;
    Ok(list(answer))
}

/// Cards whose name contains `name` (brief: id, localId, name, image),
/// Pocket cards left out.
pub async fn search(
    http: &reqwest::Client,
    language: &str,
    name: &str,
) -> Result<Vec<Value>, AppError> {
    let limit = SEARCH_LIMIT.to_string();
    let answer = get(
        http,
        &format!("{language}/cards"),
        &[("name", name), ("pagination:itemsPerPage", &limit)],
    )
    .await?;
    let pocket = pocket_sets(http).await;
    Ok(list(answer)
        .into_iter()
        .filter(|card| !is_pocket(card, &pocket))
        .collect())
}

/// The cards printed with this number in any set (`TG29`: one per Trainer
/// Gallery), Pocket cards left out (brief).
pub async fn by_number(
    http: &reqwest::Client,
    language: &str,
    number: &str,
) -> Result<Vec<Value>, AppError> {
    let limit = SEARCH_LIMIT.to_string();
    let answer = get(
        http,
        &format!("{language}/cards"),
        &[
            ("localId", &format!("eq:{number}")),
            ("pagination:itemsPerPage", &limit),
        ],
    )
    .await?;
    let pocket = pocket_sets(http).await;
    Ok(list(answer)
        .into_iter()
        .filter(|card| !is_pocket(card, &pocket))
        .collect())
}

/// Every set's name in a language, by id: the finder shows them under the
/// results. Asked once per language and run; empty when TCGdex can't
/// answer (the ids are shown then).
pub async fn set_names(http: &reqwest::Client, language: &str) -> HashMap<String, String> {
    static NAMES: Mutex<BTreeMap<String, HashMap<String, String>>> =
        Mutex::const_new(BTreeMap::new());
    let mut names = NAMES.lock().await;
    if let Some(known) = names.get(language) {
        return known.clone();
    }

    let found: HashMap<String, String> = match get(http, &format!("{language}/sets"), &[]).await {
        Ok(answer) => list(answer)
            .into_iter()
            .filter_map(|set| {
                Some((
                    set["id"].as_str()?.to_string(),
                    set["name"].as_str()?.to_string(),
                ))
            })
            .collect(),
        Err(e) => {
            tracing::warn!("{e}");
            return HashMap::new();
        }
    };
    names.insert(language.to_string(), found.clone());
    found
}

/// The cards of a Pokémon (its Pokédex number), in a language (brief).
pub async fn species_cards(
    http: &reqwest::Client,
    language: &str,
    dex_id: i64,
) -> Result<Vec<Value>, AppError> {
    let limit = SEARCH_LIMIT.to_string();
    let answer = get(
        http,
        &format!("{language}/cards"),
        &[
            ("dexId", &format!("eq:{dex_id}")),
            ("pagination:itemsPerPage", &limit),
        ],
    )
    .await?;
    Ok(list(answer))
}

fn list(answer: Option<Value>) -> Vec<Value> {
    match answer {
        Some(Value::Array(items)) => items,
        _ => Vec::new(),
    }
}

/// The sets of Pokémon TCG Pocket (the mobile game, series `tcgp`): their
/// cards are not physical cards. Asked once per run; empty when TCGdex
/// can't answer (then nothing is left out).
async fn pocket_sets(http: &reqwest::Client) -> HashSet<String> {
    static POCKET: OnceCell<HashSet<String>> = OnceCell::const_new();
    POCKET
        .get_or_init(|| async {
            match get(http, "en/series/tcgp", &[]).await {
                Ok(Some(serie)) => serie["sets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|set| set["id"].as_str().map(str::to_string))
                    .collect(),
                Ok(None) => HashSet::new(),
                Err(e) => {
                    tracing::warn!("{e}");
                    HashSet::new()
                }
            }
        })
        .await
        .clone()
}

/// A brief card's set is the part of its id before the last `-`.
fn is_pocket(card: &Value, pocket: &HashSet<String>) -> bool {
    card["id"]
        .as_str()
        .and_then(|id| id.rsplit_once('-'))
        .is_some_and(|(set, _)| pocket.contains(set))
}

/// A card picture (`{image}/high.webp`).
pub async fn image(http: &reqwest::Client, base_url: &str) -> Result<Vec<u8>, AppError> {
    let url = format!("{base_url}/high.webp");
    let fail = |e: reqwest::Error| error("image", e);
    let bytes = http
        .get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(fail)?
        .bytes()
        .await
        .map_err(fail)?;
    Ok(bytes.to_vec())
}
