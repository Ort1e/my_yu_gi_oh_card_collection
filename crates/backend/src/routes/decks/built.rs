//! `/decks/built`: the built deck versions (`domain::built_deck`), where
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
}

/// A card missing from the built versions: its name, how many, and where.
pub struct ShoppingLine {
    name: String,
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
    is_proxy: bool,
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
                card.is_proxy
         FROM (SELECT deck_version_id, card_id FROM deck_version_main_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_extra_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_side_deck) AS zone
         JOIN deck_version ON deck_version.id = zone.deck_version_id
         JOIN card ON card.id = zone.card_id
         WHERE deck_version.is_built = 1
         ORDER BY card.name",
    )
    .fetch_all(pool)
    .await?;

    // key -> (name, count, versions' full names)
    let mut shopping: BTreeMap<String, (String, usize, Vec<String>)> = BTreeMap::new();
    let mut versions = Vec::new();
    for version in rows {
        let in_version = cards.iter().filter(|c| c.deck_version_id == version.id);
        let mut missing: BTreeMap<&str, (&str, usize)> = BTreeMap::new();
        let (mut count, mut proxies) = (0, 0);
        for card in in_version {
            count += 1;
            if card.is_proxy {
                proxies += 1;
                missing.entry(&card.key).or_insert((&card.name, 0)).1 += 1;
            }
        }

        for (key, (name, n)) in &missing {
            let line = shopping
                .entry(key.to_string())
                .or_insert_with(|| (name.to_string(), 0, Vec::new()));
            line.1 += n;
            line.2.push(version.full_name());
        }
        versions.push(BuiltVersion {
            cards: count,
            proxies,
            missing: missing
                .into_values()
                .map(|(name, n)| (name.to_string(), n))
                .collect(),
            version,
        });
    }

    let mut shopping: Vec<ShoppingLine> = shopping
        .into_values()
        .map(|(name, count, decks)| ShoppingLine {
            name,
            count,
            decks: decks.join(", "),
        })
        .collect();
    shopping.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(BuiltPage { versions, shopping })
}
