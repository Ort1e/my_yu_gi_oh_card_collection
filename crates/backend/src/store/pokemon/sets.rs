//! Pokémon sets by the code printed on their cards (`FNE` in French, `SFA`
//! in English, `SV6a` in Japanese, where the code is the set's id), with
//! their card lists: the booster form and the card finder reach a card by
//! set + number. Remembered in `pokemon_set` per language, asked again
//! after [`RECHECK_DAYS`] (promo sets grow).
//!
//! Several sets can share a code: 30th Celebration (`30th`, 158 cards and
//! the Mew cards R, G, B) and its Classic Collection (`30th-c`, 001 to 030)
//! are both `30C`. A code also covers its family: TCGdex keeps a Trainer
//! Gallery apart, coded `ASR:TG`, while its cards print `ASR` (TG29/TG30). A number found in both is told apart by the set size
//! printed after it (`/128`, `/030`); else both are offered ([`cards`]).

use pokemon_domain::card_ref::{is_set_size, same_number};
use pokemon_domain::name_matches;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{is_fresh, tcgdex};
use crate::store::db::{begin_write, today, Pool};
use crate::web::error::{AppError, AppResult};

const RECHECK_DAYS: i64 = 7;

/// A card of a set's list (TCGdex's brief card).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetCard {
    pub id: String,
    #[serde(rename = "localId")]
    pub local_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SetInfo {
    /// The language it was asked in (its names are in it).
    pub language: String,
    pub id: String,
    pub name: String,
    /// The printed code in that language, when TCGdex knows it.
    pub code: Option<String>,
    /// The set size printed after the numbers (`/064`).
    pub official: Option<i64>,
    pub cards: Vec<SetCard>,
    /// When TCGdex was asked (`YYYY-MM-DD`).
    pub fetched_at: String,
}

/// Whether some of these sets were asked on an earlier day: a number not
/// found in them is worth asking TCGdex again ([`forget`]).
pub fn are_old(sets: &[SetInfo]) -> bool {
    let today = today().to_string();
    sets.iter().any(|set| set.fetched_at != today)
}

impl SetInfo {
    /// The card printed with this number (`67` finds `067`).
    pub fn card(&self, number: &str) -> Option<&SetCard> {
        self.cards.iter().find(|c| same_number(&c.local_id, number))
    }

    /// Whether a number is above the set size (`067/064`): a chase card
    /// (illustration rares, gold cards…).
    pub fn is_secret(&self, local_id: &str) -> bool {
        let Some(official) = self.official.filter(|n| *n > 0) else {
            return false;
        };
        local_id.parse::<i64>().is_ok_and(|n| n > official)
    }
}

/// The cards printed with this number in these sets (`/total`: the set
/// size printed after it, when typed). Several when sets share a code and
/// the size doesn't settle it: the main set's first.
pub fn cards<'a>(
    sets: &'a [SetInfo],
    number: &str,
    total: Option<&str>,
) -> Vec<(&'a SetInfo, &'a SetCard)> {
    // a set size that is none of these sets' while a reprint collection
    // shares the code: a reprint printed with its old number (33/181)
    let foreign_size = total
        .and_then(|t| t.parse::<i64>().ok())
        .is_some_and(|t| sets.iter().all(|set| set.official != Some(t)));
    if foreign_size && !subsets(sets).is_empty() {
        return Vec::new();
    }

    let found: Vec<_> = sets
        .iter()
        .filter_map(|set| Some((set, set.card(number)?)))
        .collect();
    let sized: Vec<_> = found
        .iter()
        .filter(|(set, _)| total.is_some_and(|t| is_set_size(t, set.official)))
        .copied()
        .collect();
    if sized.is_empty() {
        found
    } else {
        sized
    }
}

/// The cards of these sets whose name contains `typed` (`30C Zekrom`).
pub fn cards_named<'a>(sets: &'a [SetInfo], typed: &str) -> Vec<(&'a SetInfo, &'a SetCard)> {
    sets.iter()
        .flat_map(|set| set.cards.iter().map(move |card| (set, card)))
        .filter(|(_, card)| name_matches(&card.name, typed))
        .collect()
}

/// The smaller sets sharing the main set's code (a Classic Collection),
/// whose cards may keep the number of the card they reprint: when a number
/// isn't found, their cards are offered.
pub fn subsets(sets: &[SetInfo]) -> Vec<&SetInfo> {
    let Some(main) = sets.first() else {
        return Vec::new();
    };
    // the same code exactly: a Trainer Gallery (`ASR:TG`) has its own numbers
    let same_code = |set: &&SetInfo| match (&set.code, &main.code) {
        (Some(code), Some(main)) => code.eq_ignore_ascii_case(main),
        _ => false,
    };
    sets.iter()
        .skip(1)
        .filter(same_code)
        .filter(|set| set.cards.len() <= 40)
        .collect()
}

/// Whether a set's code is `code` or one of its family (`ASR:TG` for `ASR`).
fn in_family(set_code: &str, code: &str) -> bool {
    set_code.eq_ignore_ascii_case(code)
        || set_code
            .to_ascii_lowercase()
            .starts_with(&format!("{}:", code.to_ascii_lowercase()))
}

/// Forget the sets remembered with this code (or id), so the next lookup
/// asks TCGdex again: a number not found may be a card added since.
pub async fn forget(pool: &Pool, language: &str, code: &str) -> AppResult<()> {
    let code = code.trim();
    let mut tx = begin_write(pool).await?;
    sqlx::query(
        "DELETE FROM pokemon_set WHERE language = ?1
             AND (lower(code) = lower(?2) OR lower(code) LIKE lower(?2) || ':%'
                  OR lower(id) = lower(?2))",
    )
    .bind(language)
    .bind(code)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The sets with this printed code (or TCGdex id: then with the code of
/// that set, so `30th` also finds `30th-c`) in `language`, the biggest
/// first; empty when TCGdex knows none. Asks TCGdex only when
/// `pokemon_set` has no fresh answer: call it before a write transaction.
/// The sets of `size` cards (the size printed after the numbers, `/189`) in
/// `language`: a Sword & Shield card prints no set code (`F 034/189`).
pub async fn by_size(
    pool: &Pool,
    http: &reqwest::Client,
    language: &str,
    size: i64,
) -> AppResult<Vec<SetInfo>> {
    let briefs =
        tcgdex::sets_where(http, language, "cardCount.official", &format!("eq:{size}")).await?;
    let mut found: Vec<SetInfo> = Vec::new();
    for brief in briefs.iter().take(8) {
        let Some(id) = brief["id"].as_str() else {
            continue;
        };
        for set in by_code(pool, http, language, id).await? {
            if set.official == Some(size) && !found.iter().any(|known| known.id == set.id) {
                found.push(set);
            }
        }
    }
    Ok(found)
}

pub async fn by_code(
    pool: &Pool,
    http: &reqwest::Client,
    language: &str,
    code: &str,
) -> AppResult<Vec<SetInfo>> {
    let mut sets = by_code_once(pool, http, language, code).await?;
    let printed = sets
        .first()
        .and_then(|set| set.code.clone())
        // `ASR:TG` (a set found by its id) belongs to the family `ASR`
        .map(|printed| printed.split(':').next().unwrap_or_default().to_string())
        .filter(|printed| !printed.is_empty() && !printed.eq_ignore_ascii_case(code.trim()));
    if let Some(printed) = printed {
        for set in by_code_once(pool, http, language, &printed).await? {
            if !sets.iter().any(|known| known.id == set.id) {
                sets.push(set);
            }
        }
        sort(&mut sets);
    }
    Ok(sets)
}

fn sort(sets: &mut [SetInfo]) {
    sets.sort_by(|a, b| b.cards.len().cmp(&a.cards.len()).then(a.id.cmp(&b.id)));
}

async fn by_code_once(
    pool: &Pool,
    http: &reqwest::Client,
    language: &str,
    code: &str,
) -> AppResult<Vec<SetInfo>> {
    let code = code.trim();
    let mut sets = remembered(pool, language, code).await?;
    if sets.is_empty() {
        for json in find(http, language, code).await? {
            let set = from_tcgdex(language, &json)
                .ok_or_else(|| AppError::Internal(format!("TCGdex: unreadable set {code}")))?;
            remember(pool, &set).await?;
            sets.push(set);
        }
    }

    sort(&mut sets);
    Ok(sets)
}

#[derive(sqlx::FromRow)]
struct SetRow {
    language: String,
    id: String,
    name: String,
    code: Option<String>,
    official: Option<i64>,
    cards: String,
    fetched_at: String,
}

/// The sets remembered with this code (else with this id); none when one
/// of them is old (they are asked again together).
async fn remembered(pool: &Pool, language: &str, code: &str) -> sqlx::Result<Vec<SetInfo>> {
    let mut rows: Vec<SetRow> = sqlx::query_as(
        "SELECT language, id, name, code, official, cards, fetched_at FROM pokemon_set
         WHERE language = ?1
             AND (lower(code) = lower(?2) OR lower(code) LIKE lower(?2) || ':%')",
    )
    .bind(language)
    .bind(code)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        rows = sqlx::query_as(
            "SELECT language, id, name, code, official, cards, fetched_at FROM pokemon_set
             WHERE language = ?1 AND lower(id) = lower(?2)",
        )
        .bind(language)
        .bind(code)
        .fetch_all(pool)
        .await?;
    }
    if rows
        .iter()
        .any(|row| !is_fresh(&row.fetched_at, RECHECK_DAYS))
    {
        return Ok(Vec::new());
    }

    Ok(rows
        .into_iter()
        .map(|row| SetInfo {
            language: row.language,
            id: row.id,
            name: row.name,
            code: row.code,
            official: row.official,
            cards: serde_json::from_str(&row.cards).unwrap_or_default(),
            fetched_at: row.fetched_at,
        })
        .collect())
}

/// Ask TCGdex: every set with the code as printed in this language or as
/// printed in English (the `official` one); none: the code as a set id.
async fn find(http: &reqwest::Client, language: &str, code: &str) -> AppResult<Vec<Value>> {
    if language == "ja" {
        return Ok(tcgdex::set(http, "ja", code).await?.into_iter().collect());
    }

    let mut found: Vec<Value> = Vec::new();
    for field in ["abbreviation.localized", "abbreviation.official"] {
        // "contains" matches: keep the sets whose code is exactly this one
        let candidates = tcgdex::sets_where(http, language, field, code).await?;
        for brief in candidates.iter().take(8) {
            let Some(id) = brief["id"].as_str() else {
                continue;
            };
            if found.iter().any(|set| set["id"].as_str() == Some(id)) {
                continue;
            }
            let Some(set) = tcgdex::set(http, language, id).await? else {
                continue;
            };
            let abbreviation = &set["abbreviation"];
            let same = |field: &str| {
                abbreviation[field]
                    .as_str()
                    .is_some_and(|c| in_family(c, code))
            };
            if same("localized") || same("official") {
                found.push(set);
            }
        }
    }
    if found.is_empty() {
        found.extend(tcgdex::set(http, language, code).await?);
    }
    Ok(found)
}

/// The code printed in the set's language: its localized abbreviation,
/// else the official (English) one.
fn printed_code(set: &Value) -> Option<&str> {
    let abbreviation = &set["abbreviation"];
    abbreviation["localized"]
        .as_str()
        // TCGdex gives some sets their name as French "code" (`151` for
        // 151, whose cards print MEW): a code has a letter
        .filter(|code| code.chars().any(|c| c.is_ascii_alphabetic()))
        .or_else(|| abbreviation["official"].as_str())
}

fn from_tcgdex(language: &str, set: &Value) -> Option<SetInfo> {
    let cards = serde_json::from_value(set["cards"].clone()).ok()?;
    Some(SetInfo {
        language: language.to_string(),
        id: set["id"].as_str()?.to_string(),
        name: set["name"].as_str()?.to_string(),
        code: printed_code(set)
            .map(str::to_string)
            .or_else(|| (language == "ja").then(|| set["id"].as_str().map(str::to_string))?),
        official: set["cardCount"]["official"].as_i64(),
        cards,
        fetched_at: today().to_string(),
    })
}

async fn remember(pool: &Pool, set: &SetInfo) -> AppResult<()> {
    let cards = serde_json::to_string(&set.cards)
        .map_err(|e| AppError::Internal(format!("set cards: {e}")))?;
    let mut tx = begin_write(pool).await?;
    sqlx::query(
        "INSERT INTO pokemon_set (language, id, name, code, official, cards, fetched_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (language, id) DO UPDATE SET name = excluded.name, code = excluded.code,
             official = excluded.official, cards = excluded.cards,
             fetched_at = excluded.fetched_at",
    )
    .bind(&set.language)
    .bind(&set.id)
    .bind(&set.name)
    .bind(&set.code)
    .bind(set.official)
    .bind(cards)
    .bind(today().to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn codes_have_families() {
        assert!(in_family("ASR", "asr") && in_family("ASR:TG", "ASR"));
        assert!(!in_family("ASRX", "ASR") && !in_family("ASR", "ASR:TG"));
    }

    #[test]
    fn reads_a_set() {
        let set = json!({
            "id": "sv06.5", "name": "Fable Nébuleuse",
            "abbreviation": {"official": "SFA", "localized": "FNE"},
            "cardCount": {"official": 64, "total": 99},
            "cards": [
                {"id": "sv06.5-010", "localId": "010", "name": "Hypotrempe",
                 "image": "https://assets.tcgdex.net/fr/sv/sv06.5/010"},
                {"id": "sv06.5-067", "localId": "067", "name": "Hypotrempe"}
            ]
        });
        let set = from_tcgdex("fr", &set).unwrap();
        assert_eq!(set.code.as_deref(), Some("FNE"));
        assert_eq!(set.card("10").unwrap().id, "sv06.5-010");
        assert_eq!(set.card("67").unwrap().image, None);
        assert!(set.card("68").is_none());
        assert!(set.is_secret("067") && !set.is_secret("010") && !set.is_secret("TG05"));

        let mew = json!({"id": "sv03.5", "name": "151",
                         "abbreviation": {"official": "MEW", "localized": "151"},
                         "cardCount": {"official": 165}, "cards": []});
        assert_eq!(
            from_tcgdex("fr", &mew).unwrap().code.as_deref(),
            Some("MEW")
        );

        let japanese = json!({"id": "SV6a", "name": "ナイトワンダラー",
                              "cardCount": {"official": 64}, "cards": []});
        assert_eq!(
            from_tcgdex("ja", &japanese).unwrap().code.as_deref(),
            Some("SV6a")
        );
    }
}
