//! `card_data` rows: reference data from the ygoprodeck API
//! (Django: `CardData.get_or_fetch`, `ImageDownloader.get_card_data`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use domain::card::{clean_card_name, name_to_translate};
use domain::rarity::split_cardmarket_name;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::store::cards::{delete_orphan_proxies, replace_proxies};
use crate::store::db::{begin_write, Pool};
use crate::web::error::{AppError, AppResult};
use crate::AppState;

const API_URL: &str = "https://db.ygoprodeck.com/api/v7/cardinfo.php";
const SETS_URL: &str = "https://db.ygoprodeck.com/api/v7/cardsets.php";

/// ygoprodeck allows 20 requests per second; stay well below.
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(60);
static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::const_new(None);

/// Wait so that two API requests are at least `MIN_REQUEST_INTERVAL` apart.
async fn throttle() {
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

/// How long "ygoprodeck has no translation" is believed before asking again
/// (recent sets get their translations after a while).
const RECHECK_MISSING_TRANSLATION_DAYS: i64 = 7;

/// The name of a card in another language (ygoprodeck's `language`
/// parameter: `"fr"`), `None` when ygoprodeck has no translation. Answers are
/// remembered in `card_translation` (migration 0005): a name found is never
/// asked again, a missing one only after
/// [`RECHECK_MISSING_TRANSLATION_DAYS`].
pub async fn translated_name(
    pool: &Pool,
    http: &reqwest::Client,
    ygopro_id: i64,
    language: &str,
) -> AppResult<Option<String>> {
    let known: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT name, checked_at FROM card_translation WHERE ygopro_id = ? AND language = ?",
    )
    .bind(ygopro_id)
    .bind(language)
    .fetch_optional(pool)
    .await?;
    let today = chrono::Local::now().date_naive();
    if let Some((name, checked_at)) = known {
        let recent = checked_at
            .parse::<chrono::NaiveDate>()
            .is_ok_and(|d| (today - d).num_days() < RECHECK_MISSING_TRANSLATION_DAYS);
        if name.is_some() || recent {
            return Ok(name);
        }
    }

    let name = fetch_translated_name(http, ygopro_id, language).await?;
    remember_translation(pool, ygopro_id, language, name.as_deref()).await?;
    Ok(name)
}

/// Store an answer of ygoprodeck in `card_translation`. A name already known
/// is never replaced by "no translation".
async fn remember_translation(
    pool: &Pool,
    ygopro_id: i64,
    language: &str,
    name: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO card_translation (ygopro_id, language, name, checked_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT (ygopro_id, language) DO UPDATE
             SET name = coalesce(excluded.name, card_translation.name),
                 checked_at = excluded.checked_at",
    )
    .bind(ygopro_id)
    .bind(language)
    .bind(name)
    .bind(chrono::Local::now().date_naive().to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// The name of a card in another language, asked to ygoprodeck (use
/// [`translated_name`], which remembers the answers).
async fn fetch_translated_name(
    http: &reqwest::Client,
    ygopro_id: i64,
    language: &str,
) -> AppResult<Option<String>> {
    let card = fetch(
        http,
        &[("id", &ygopro_id.to_string()), ("language", language)],
    )
    .await?;
    Ok(card.and_then(|c| c["name"].as_str().map(str::to_string)))
}

/// First card of a ygoprodeck search, `None` when unknown.
async fn fetch(http: &reqwest::Client, query: &[(&str, &str)]) -> AppResult<Option<Value>> {
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

/// The ygoprodeck answers for one set prefix (`"RA05"`), for
/// `domain::card_set::set_cards`. Several sets can share a prefix: their
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

/// A card whose name isn't in the language of its code yet.
pub struct Translation {
    pub card_id: i64,
    pub code: String,
    pub from: String,
    pub to: String,
}

/// The cards (all, or `only` these) whose code is in another language than
/// English (`MAMO-FR023`) but whose name is still the English one, with their
/// name in that language from ygoprodeck (the code is the truth). Also
/// returns the cards for which ygoprodeck has no translation ("Name (code)").
///
/// Only those cards are looked up, through [`translated_name`] (answers
/// remembered in `card_translation`: even a dry run of `clean-cards` fills
/// it). Cards must be linked to their card data (for the English name and
/// the ygopro id).
pub async fn plan_translations(
    pool: &Pool,
    http: &reqwest::Client,
    only: Option<&[i64]>,
) -> AppResult<(Vec<Translation>, Vec<String>)> {
    let cards: Vec<(i64, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT card.id, card.name, card.code, card_data.ygopro_id,
                json_extract(card_data.json_data, '$.name')
         FROM card JOIN card_data ON card_data.id = card.card_data_id
         WHERE card.code IS NOT NULL AND card_data.ygopro_id IS NOT NULL
         ORDER BY card.id",
    )
    .fetch_all(pool)
    .await?;
    let only: Option<HashSet<i64>> = only.map(|ids| ids.iter().copied().collect());

    let mut translations = Vec::new();
    let mut untranslated = Vec::new();
    let mut fetched: HashMap<(i64, &'static str), Option<String>> = HashMap::new();
    for (card_id, name, code, ygopro_id, english) in cards {
        if only.as_ref().is_some_and(|only| !only.contains(&card_id)) {
            continue;
        }
        let Some(english) = english else { continue };
        // compared without a "(V.x - …)" suffix (clean-cards removes it)
        let (short_name, _) = split_cardmarket_name(&name);
        let Some(language) = name_to_translate(short_name, &code, &english) else {
            continue;
        };
        // Spanish: ygoprodeck has no Spanish names
        let Some(param) = language.ygoprodeck() else {
            continue;
        };

        let to = match fetched.get(&(ygopro_id, param)) {
            Some(to) => to.clone(),
            None => {
                let to = translated_name(pool, http, ygopro_id, param).await?;
                fetched.insert((ygopro_id, param), to.clone());
                to
            }
        };
        match to {
            // the same name in both languages ("Kuriboh")
            Some(to) if to.eq_ignore_ascii_case(short_name) => {}
            Some(to) => translations.push(Translation {
                card_id,
                code,
                from: name,
                to,
            }),
            None => untranslated.push(format!("{name} ({code})")),
        }
    }
    Ok((translations, untranslated))
}

/// In the background, so form submissions don't wait on the API: link the
/// given cards to their `card_data` (fetching it if needed), put their names
/// in the language of their code, then replace deck proxies by the new owned
/// copies and delete the proxies left unused.
pub fn spawn_link_cards(state: AppState, card_ids: Vec<i64>) {
    if card_ids.is_empty() {
        return;
    }
    tokio::spawn(async move {
        if let Err(e) = link_cards(&state, &card_ids).await {
            tracing::warn!("linking card data: {e:?}");
        }
        if let Err(e) = translate_names(&state, &card_ids).await {
            tracing::warn!("translating card names: {e:?}");
        }
        if let Err(e) = tidy_proxies(&state.pool).await {
            tracing::warn!("replacing proxies: {e:?}");
        }
    });
}

/// Replace deck proxies by owned copies (some may just have arrived), then
/// delete the proxies left unused.
async fn tidy_proxies(pool: &Pool) -> AppResult<()> {
    let mut tx = begin_write(pool).await?;
    let replaced = replace_proxies(&mut tx).await?;
    let deleted = delete_orphan_proxies(&mut tx).await?;
    tx.commit().await?;
    if replaced > 0 || deleted > 0 {
        tracing::info!(replaced, deleted, "deck proxies replaced by owned cards");
    }
    Ok(())
}

/// Put the names of these cards in the language of their code.
async fn translate_names(state: &AppState, card_ids: &[i64]) -> AppResult<()> {
    // network first, then the writes (each one on its own: no transaction
    // held while waiting on ygoprodeck)
    let (translations, untranslated) =
        plan_translations(&state.pool, &state.http, Some(card_ids)).await?;
    for t in &translations {
        sqlx::query("UPDATE card SET name = ? WHERE id = ?")
            .bind(&t.to)
            .bind(t.card_id)
            .execute(&state.pool)
            .await?;
    }
    if !translations.is_empty() || !untranslated.is_empty() {
        // a recent set may have no translation at ygoprodeck yet: clean-cards
        // tries again later
        tracing::info!(
            cards = translations.len(),
            without_translation = untranslated.len(),
            "names put in the language of their code"
        );
    }
    Ok(())
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
