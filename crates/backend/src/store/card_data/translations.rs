//! Card names in other languages, from ygoprodeck, remembered in
//! `card_translation` (migration 0005), and the cards whose name isn't in
//! the language of their code yet.

use std::collections::{HashMap, HashSet};

use yugioh_domain::card::name_to_translate;
use yugioh_domain::rarity::split_cardmarket_name;

use crate::store::db::{today, Pool};
use crate::web::error::AppResult;

use super::fetch;

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
    let today = today();
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
pub(super) async fn remember_translation(
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
    .bind(today().to_string())
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

/// A card whose name isn't in the language of its code yet.
pub struct Translation {
    pub card_id: i64,
    pub code: String,
    pub from: String,
    pub to: String,
}

/// A card with a code and its card data, for [`plan_translations`].
#[derive(sqlx::FromRow)]
struct CodedCard {
    id: i64,
    name: String,
    code: String,
    ygopro_id: i64,
    /// ygoprodeck's English name.
    english: Option<String>,
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
    let cards: Vec<CodedCard> = sqlx::query_as(
        "SELECT card.id, card.name, card.code, card_data.ygopro_id,
                json_extract(card_data.json_data, '$.name') AS english
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
    for card in cards {
        let CodedCard {
            id: card_id,
            name,
            code,
            ygopro_id,
            english,
        } = card;
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
