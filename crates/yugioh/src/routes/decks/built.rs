//! `/decks/built`: the built deck versions (`yugioh_domain::built_deck`), where
//! they live, and the cards still missing to play them for real (their
//! proxies), then all those missing cards together: the shopping list.

use std::collections::BTreeMap;

use askama::Template;
use axum::extract::State;

use crate::routes::collection::{deck_values, DeckLine, ValueData};
use crate::store::errata::{CardRef, Context, ErrataWarning, MissingCard, Verdicts};
use crate::store::models::DeckVersionRow;
use crate::web::error::AppResult;
use crate::web::filters;
use crate::AppState;

pub struct BuiltVersion {
    version: DeckVersionRow,
    cards: usize,
    proxies: usize,
    /// Real copies with the wrong text for the version's format or ban list.
    wrong_text: usize,
    /// By name.
    missing: Vec<MissingCard>,
}

/// A card missing from the built versions: its name, how many, and where.
pub struct ShoppingLine {
    name: String,
    /// The name Cardmarket knows the card by.
    english_name: String,
    count: usize,
    /// How many of them replace an owned copy with the wrong text.
    wrong_text: usize,
    decks: String,
    /// One per format or ban list the card is errata'd in.
    warnings: Vec<ErrataWarning>,
}

#[derive(Template)]
#[template(path = "decks/built.html")]
pub struct BuiltPage {
    versions: Vec<BuiltVersion>,
    shopping: Vec<ShoppingLine>,
    /// Cards whose errata are being looked up on Yugipedia.
    errata_pending: usize,
    /// Each deck's latest version: its value and what its proxies would
    /// cost (was on `/collection/value`).
    decks: Vec<DeckLine>,
}

#[derive(sqlx::FromRow)]
struct ZoneCard {
    deck_version_id: i64,
    #[sqlx(flatten)]
    card: CardRef,
}

/// What to do with a wants list's text, shown with every wants list (this
/// page's and the deck builder's To buy tab, in `Check::wants_note`).
pub const WANTS_NOTE: &str =
    "Paste it into a Cardmarket Yu-Gi-Oh! wants list (Wants → import a decklist). \
Language and condition can't be in the text: after the import, set Language: English, French \
and Min. condition: Good on the list's cards.";

/// A Cardmarket wants list's text: one `"{count} {name}"` line per card.
/// No expansion or version: any printing will do for a proxy. Language and
/// condition can't go in the text; they are set on the list after the import.
pub(super) fn wants_list<'a>(lines: impl Iterator<Item = (usize, &'a str)>) -> String {
    lines
        .map(|(count, name)| format!("{count} {name}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The Cardmarket wants list of any version's proxies (built or not), by
/// English name, sorted.
pub(super) async fn version_wants_list(pool: &sqlx::SqlitePool, id: i64) -> AppResult<String> {
    let names: Vec<(String, String)> = sqlx::query_as(
        "SELECT coalesce(card.en_name, card.name),
                coalesce(json_extract(card_data.json_data, '$.name'), card.en_name, card.name)
         FROM (SELECT card_id FROM deck_version_main_deck WHERE deck_version_id = ?1
               UNION ALL SELECT card_id FROM deck_version_extra_deck WHERE deck_version_id = ?1
               UNION ALL SELECT card_id FROM deck_version_side_deck WHERE deck_version_id = ?1) AS zone
         JOIN card ON card.id = zone.card_id
         LEFT JOIN card_data ON card_data.id = card.card_data_id
         WHERE card.is_proxy = 1",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    // key -> (English name, count)
    let mut missing: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for (key, english) in names {
        missing.entry(key).or_insert((english, 0)).1 += 1;
    }

    Ok(wants_list(
        missing
            .values()
            .map(|(name, count)| (*count, name.as_str())),
    ))
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

impl BuiltPage {
    pub fn wants_note(&self) -> &'static str {
        WANTS_NOTE
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
    let cards: Vec<ZoneCard> = sqlx::query_as(&format!(
        "SELECT zone.deck_version_id, {}
         FROM (SELECT deck_version_id, card_id FROM deck_version_main_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_extra_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_side_deck) AS zone
         JOIN deck_version ON deck_version.id = zone.deck_version_id
         JOIN card ON card.id = zone.card_id
         LEFT JOIN card_data ON card_data.id = card.card_data_id
         WHERE deck_version.is_built = 1
         ORDER BY card.name",
        CardRef::COLUMNS
    ))
    .fetch_all(pool)
    .await?;

    let ids: Vec<i64> = rows.iter().map(|v| v.id).collect();
    let contexts = crate::store::errata::contexts(pool, &ids).await?;
    let probes: Vec<(&Context, &CardRef)> = cards
        .iter()
        .filter_map(|c| Some((contexts.get(&c.deck_version_id)?, &c.card)))
        .collect();
    let verdicts = Verdicts::load(&state, &probes).await?;

    // key -> the line, with the versions' full names
    let mut shopping: BTreeMap<String, (ShoppingLine, Vec<String>)> = BTreeMap::new();
    let mut versions = Vec::new();
    for version in rows {
        let in_version: Vec<&CardRef> = cards
            .iter()
            .filter(|c| c.deck_version_id == version.id)
            .map(|c| &c.card)
            .collect();
        let proxies = in_version.iter().filter(|c| c.is_proxy).count();
        let missing = verdicts.missing(contexts.get(&version.id), in_version.iter().copied());

        for (key, card) in &missing.lines {
            let (line, decks) = shopping.entry(key.clone()).or_insert_with(|| {
                (
                    ShoppingLine {
                        name: card.name.clone(),
                        english_name: card.english_name.clone(),
                        count: 0,
                        wrong_text: 0,
                        decks: String::new(),
                        warnings: Vec::new(),
                    },
                    Vec::new(),
                )
            });
            line.count += card.count;
            line.wrong_text += card.wrong_text.len();
            decks.push(version.full_name());
            if let Some(warning) = &card.warning {
                if !line.warnings.iter().any(|w| w.format == warning.format) {
                    line.warnings.push(warning.clone());
                }
            }
        }
        versions.push(BuiltVersion {
            cards: in_version.len(),
            proxies,
            wrong_text: missing.wrong_text,
            missing: missing.lines.into_values().collect(),
            version,
        });
    }

    let mut shopping: Vec<ShoppingLine> = shopping
        .into_values()
        .map(|(mut line, decks)| {
            line.decks = decks.join(", ");
            line
        })
        .collect();
    shopping.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let values = ValueData::load(pool).await?;

    Ok(BuiltPage {
        versions,
        shopping,
        errata_pending: verdicts.pending,
        decks: deck_values(pool, &values.latest).await?,
    })
}
