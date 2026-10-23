//! Pokémon sets by the code printed on their cards (`FNE` in French, `SFA`
//! in English, `SV6a` in Japanese, where the code is the set's id), with
//! their card lists: the booster form and the card finder reach a card by
//! set + number. Remembered in `pokemon_set` per language, asked again
//! after [`RECHECK_DAYS`] (promo sets grow).

use domain::pokemon::card_ref::same_number;
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

/// The set with this printed code (or TCGdex id) in `language`; `None` when
/// TCGdex knows none. Asks TCGdex only when `pokemon_set` has no fresh
/// answer: call it before a write transaction.
pub async fn by_code(
    pool: &Pool,
    http: &reqwest::Client,
    language: &str,
    code: &str,
) -> AppResult<Option<SetInfo>> {
    let code = code.trim();
    if let Some(set) = remembered(pool, language, code).await? {
        return Ok(Some(set));
    }

    let Some(json) = find(http, language, code).await? else {
        return Ok(None);
    };
    let set = from_tcgdex(language, &json)
        .ok_or_else(|| AppError::Internal(format!("TCGdex: unreadable set {code}")))?;
    remember(pool, &set).await?;
    Ok(Some(set))
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

async fn remembered(pool: &Pool, language: &str, code: &str) -> sqlx::Result<Option<SetInfo>> {
    let row: Option<SetRow> = sqlx::query_as(
        "SELECT language, id, name, code, official, cards, fetched_at FROM pokemon_set
         WHERE language = ?1 AND (lower(code) = lower(?2) OR lower(id) = lower(?2))
         ORDER BY lower(code) = lower(?2) DESC LIMIT 1",
    )
    .bind(language)
    .bind(code)
    .fetch_optional(pool)
    .await?;

    Ok(row
        .filter(|row| is_fresh(&row.fetched_at, RECHECK_DAYS))
        .map(|row| SetInfo {
            language: row.language,
            id: row.id,
            name: row.name,
            code: row.code,
            official: row.official,
            cards: serde_json::from_str(&row.cards).unwrap_or_default(),
        }))
}

/// Ask TCGdex: the code as printed in this language, as printed in English
/// (the `official` one), then as a set id.
async fn find(http: &reqwest::Client, language: &str, code: &str) -> AppResult<Option<Value>> {
    if language == "ja" {
        return tcgdex::set(http, "ja", code).await;
    }

    for field in ["abbreviation.localized", "abbreviation.official"] {
        let candidates = tcgdex::sets_where(http, language, field, code).await?;
        for brief in candidates.iter().take(5) {
            let Some(id) = brief["id"].as_str() else {
                continue;
            };
            let Some(set) = tcgdex::set(http, language, id).await? else {
                continue;
            };
            let printed = printed_code(&set);
            if printed.is_some_and(|p| p.eq_ignore_ascii_case(code))
                || set["abbreviation"]["official"]
                    .as_str()
                    .is_some_and(|p| p.eq_ignore_ascii_case(code))
            {
                return Ok(Some(set));
            }
        }
    }

    tcgdex::set(http, language, code).await
}

/// The code printed in the set's language: its localized abbreviation,
/// else the official (English) one.
fn printed_code(set: &Value) -> Option<&str> {
    let abbreviation = &set["abbreviation"];
    abbreviation["localized"]
        .as_str()
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

        let japanese = json!({"id": "SV6a", "name": "ナイトワンダラー",
                              "cardCount": {"official": 64}, "cards": []});
        assert_eq!(
            from_tcgdex("ja", &japanese).unwrap().code.as_deref(),
            Some("SV6a")
        );
    }
}
