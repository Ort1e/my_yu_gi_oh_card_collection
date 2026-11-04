//! `/decks/built`: the built deck versions (`yugioh_domain::built_deck`), where
//! they live, and the cards still missing to play them for real (their
//! proxies), then all those missing cards together: the shopping list.

use std::collections::BTreeMap;

use askama::Template;
use axum::extract::State;

use crate::store::models::DeckVersionRow;
use crate::web::error::AppResult;
use crate::AppState;

pub struct BuiltVersion {
    version: DeckVersionRow,
    cards: usize,
    proxies: usize,
    /// (card name, count), by name.
    missing: Vec<(String, usize)>,
    /// (English name, count), for the Cardmarket wants list.
    missing_english: Vec<(String, usize)>,
}

/// A card missing from the built versions: its name, how many, and where.
pub struct ShoppingLine {
    name: String,
    /// The name Cardmarket knows the card by.
    english_name: String,
    count: usize,
    decks: String,
}

#[derive(Template)]
#[template(path = "decks/built.html")]
pub struct BuiltPage {
    versions: Vec<BuiltVersion>,
    shopping: Vec<ShoppingLine>,
}

#[derive(sqlx::FromRow)]
struct ZoneCard {
    deck_version_id: i64,
    name: String,
    /// Groups the copies of a card (`en_name`, else the name).
    key: String,
    /// The English name as written on the card data (`en_name` is lowercase).
    english_name: String,
    is_proxy: bool,
}

/// A Cardmarket wants list's text: one `"{count} {name}"` line per card.
/// No expansion or version: any printing will do for a proxy. Language and
/// condition can't go in the text; they are set on the list after the import.
fn wants_list<'a>(lines: impl Iterator<Item = (usize, &'a str)>) -> String {
    lines
        .map(|(count, name)| format!("{count} {name}"))
        .collect::<Vec<_>>()
        .join("\n")
}

impl BuiltPage {
    /// The whole shopping list as a Cardmarket wants list.
    pub fn wants_list(&self) -> String {
        wants_list(
            self.shopping
                .iter()
                .map(|line| (line.count, line.english_name.as_str())),
        )
    }
}

impl BuiltVersion {
    /// This version's missing cards as a Cardmarket wants list.
    pub fn wants_list(&self) -> String {
        wants_list(
            self.missing_english
                .iter()
                .map(|(name, count)| (*count, name.as_str())),
        )
    }
}

pub async fn page(State(state): State<AppState>) -> AppResult<BuiltPage> {
    let pool = &state.pool;
    let rows: Vec<DeckVersionRow> = sqlx::query_as(&format!(
        "{} WHERE deck_version.is_built = 1 ORDER BY deck.name, deck_version.version_name",
        DeckVersionRow::SELECT
    ))
    .fetch_all(pool)
    .await?;
    let cards: Vec<ZoneCard> = sqlx::query_as(
        "SELECT zone.deck_version_id, card.name, coalesce(card.en_name, card.name) AS key,
                coalesce(json_extract(card_data.json_data, '$.name'), card.en_name, card.name)
                    AS english_name,
                card.is_proxy
         FROM (SELECT deck_version_id, card_id FROM deck_version_main_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_extra_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_side_deck) AS zone
         JOIN deck_version ON deck_version.id = zone.deck_version_id
         JOIN card ON card.id = zone.card_id
         LEFT JOIN card_data ON card_data.id = card.card_data_id
         WHERE deck_version.is_built = 1
         ORDER BY card.name",
    )
    .fetch_all(pool)
    .await?;

    // key -> (name, English name, count, versions' full names)
    let mut shopping: BTreeMap<String, (String, String, usize, Vec<String>)> = BTreeMap::new();
    let mut versions = Vec::new();
    for version in rows {
        let in_version = cards.iter().filter(|c| c.deck_version_id == version.id);
        let mut missing: BTreeMap<&str, (&str, &str, usize)> = BTreeMap::new();
        let (mut count, mut proxies) = (0, 0);
        for card in in_version {
            count += 1;
            if card.is_proxy {
                proxies += 1;
                missing
                    .entry(&card.key)
                    .or_insert((&card.name, &card.english_name, 0))
                    .2 += 1;
            }
        }

        for (key, (name, english, n)) in &missing {
            let line = shopping
                .entry(key.to_string())
                .or_insert_with(|| (name.to_string(), english.to_string(), 0, Vec::new()));
            line.2 += n;
            line.3.push(version.full_name());
        }
        versions.push(BuiltVersion {
            cards: count,
            proxies,
            missing: missing
                .values()
                .map(|(name, _, n)| (name.to_string(), *n))
                .collect(),
            missing_english: missing
                .values()
                .map(|(_, english, n)| (english.to_string(), *n))
                .collect(),
            version,
        });
    }

    let mut shopping: Vec<ShoppingLine> = shopping
        .into_values()
        .map(|(name, english_name, count, decks)| ShoppingLine {
            name,
            english_name,
            count,
            decks: decks.join(", "),
        })
        .collect();
    shopping.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(BuiltPage { versions, shopping })
}
