//! `/decks/shopping`: what is missing to play the built and the planned
//! versions (`deck_version.is_planned`) for real, deck by deck, with the
//! errata warnings and each card's Cardmarket price, then all of it as one
//! Cardmarket wants list. The versions share the collection: they take their
//! copies in turn (built first, then planned, each by deck name), so a copy
//! owned once is counted once (`ownership::missing_for_all`).

use std::collections::{BTreeMap, HashMap};

use askama::Template;
use axum::extract::State;
use rust_decimal::Decimal;

use sqlx::QueryBuilder;
use yugioh_domain::ownership::{missing_for_all, Line};

use crate::routes::collection::ValueData;
use crate::store::cards::SOLD_EXISTS;
use crate::store::db::{push_ids, Money};
use crate::store::errata::{self, CardRef, Context, MissingCard, Verdicts};
use crate::store::lists;
use crate::store::models::DeckVersionRow;
use crate::web::error::AppResult;
use crate::web::filters;
use crate::AppState;

pub struct BuiltVersion {
    version: DeckVersionRow,
    cards: usize,
    /// Real copies with the wrong text for the version's format or ban list.
    wrong_text: usize,
    /// What is missing, by name.
    missing: Vec<WantLine>,
}

impl BuiltVersion {
    /// How many missing cards (with their copies) the deck lacks.
    pub fn missing_copies(&self) -> usize {
        self.missing.iter().map(|line| line.card.count).sum()
    }

    /// How many missing cards carry an errata warning.
    pub fn errata_cards(&self) -> usize {
        self.missing
            .iter()
            .filter(|line| line.card.warning.is_some())
            .count()
    }

    /// What the deck's missing cards cost together, when their prices are known.
    pub fn cost(&self) -> String {
        let cost: Decimal = self.missing.iter().filter_map(WantLine::amount).sum();
        format!("{:.2}", cost.round_dp(2))
    }
}

/// A card missing from a built version, with its Cardmarket price.
pub struct WantLine {
    card: MissingCard,
    /// One copy, ygoprodeck's Cardmarket price: None = no price known.
    unit_price: Option<Decimal>,
}

impl WantLine {
    fn amount(&self) -> Option<Decimal> {
        self.unit_price
            .map(|price| price * Decimal::from(self.card.count))
    }

    /// All the copies together.
    pub fn total(&self) -> Option<String> {
        self.unit_price.map(|price| {
            format!(
                "{:.2}",
                (price * Decimal::from(self.card.count)).round_dp(2)
            )
        })
    }

    /// The price as the page's data attribute: empty when unknown.
    pub fn price_attr(&self) -> String {
        self.unit_price
            .map(|price| price.round_dp(2).to_string())
            .unwrap_or_default()
    }
}

#[derive(Template)]
#[template(path = "decks/shopping.html")]
pub struct ShoppingPage {
    versions: Vec<BuiltVersion>,
    /// Every listed deck's missing cards together, by English name:
    /// `(count, name)`, for the wants list (all picked at first).
    shopping: Vec<(usize, String)>,
    /// What the shopping list costs, and how many of its cards have no price.
    estimate: Decimal,
    unpriced: usize,
    /// Cards whose errata are being looked up on Yugipedia.
    errata_pending: usize,
    /// What is left of this month's budget (`None`: no budget ever set).
    budget_left: Option<Money>,
}

/// What to do with a wants list's text, shown with every wants list (this
/// page's and the deck builder's To buy tab, in `Check::wants_note`).
pub const WANTS_NOTE: &str =
    "Paste it into a Cardmarket Yu-Gi-Oh! wants list (Wants → import a decklist). \
Language and condition can't be in the text: after the import, set Language: English, French \
and Min. condition: Good on the list's cards.";

/// A Cardmarket wants list's text: one `"{count} {name}"` line per card.
/// No expansion or version: any printing will do. Language and
/// condition can't go in the text; they are set on the list after the import.
pub(super) fn wants_list<'a>(lines: impl Iterator<Item = (usize, &'a str)>) -> String {
    lines
        .map(|(count, name)| format!("{count} {name}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The Cardmarket wants list of any version's missing cards (built or not),
/// by English name, sorted.
pub(super) async fn version_wants_list(pool: &sqlx::SqlitePool, id: i64) -> AppResult<String> {
    let cards = errata::version_cards(&mut *pool.acquire().await?, id).await?;

    // key -> (English name, count)
    let mut missing: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for card in cards.into_iter().filter(|c| c.missing) {
        missing.entry(card.key).or_insert((card.english_name, 0)).1 += 1;
    }

    Ok(wants_list(
        missing
            .values()
            .map(|(name, count)| (*count, name.as_str())),
    ))
}

impl ShoppingPage {
    /// The whole shopping list as a Cardmarket wants list.
    pub fn wants_list(&self) -> String {
        wants_list(
            self.shopping
                .iter()
                .map(|(count, name)| (*count, name.as_str())),
        )
    }
}

impl ShoppingPage {
    pub fn wants_note(&self) -> &'static str {
        WANTS_NOTE
    }
}

pub async fn page(State(state): State<AppState>) -> AppResult<ShoppingPage> {
    let pool = &state.pool;
    let rows: Vec<DeckVersionRow> = sqlx::query_as(&format!(
        "{} WHERE deck_version.is_built = 1 OR deck_version.is_planned = 1
         ORDER BY deck_version.is_built DESC, deck.name, deck_version.version_name",
        DeckVersionRow::SELECT
    ))
    .fetch_all(pool)
    .await?;

    // what each version lacks, the copies shared between them
    let mut conn = pool.acquire().await?;
    let mut all: Vec<(i64, Vec<Line>)> = Vec::new();
    for version in &rows {
        all.push((version.id, lists::lines(&mut conn, version.id).await?));
    }
    let data_ids: Vec<i64> = all
        .iter()
        .flat_map(|(_, lines)| lines.iter().filter_map(|l| l.card_data_id))
        .collect();
    let mut owned: HashMap<i64, usize> = HashMap::new();
    if !data_ids.is_empty() {
        let mut qb = QueryBuilder::new(format!(
            "SELECT card_data_id, count(*) FROM card WHERE NOT {SOLD_EXISTS} AND card_data_id IN "
        ));
        push_ids(&mut qb, data_ids);
        qb.push(" GROUP BY card_data_id");
        let rows: Vec<(i64, i64)> = qb.build_query_as().fetch_all(&mut *conn).await?;
        owned.extend(rows.into_iter().map(|(id, n)| (id, n as usize)));
    }
    let mut short = missing_for_all(&all, &owned);

    // each version's copies and missing cards, and its size
    let mut cards: Vec<(i64, Vec<CardRef>, usize)> = Vec::new();
    for (id, lines) in &all {
        let size = lines.iter().map(|l| l.count as usize).sum();
        let missing = short.remove(id).unwrap_or_default();
        cards.push((
            *id,
            errata::version_cards_missing(&mut conn, *id, missing).await?,
            size,
        ));
    }
    drop(conn);

    let ids: Vec<i64> = rows.iter().map(|v| v.id).collect();
    let contexts = crate::store::errata::contexts(pool, &ids).await?;
    let probes: Vec<(&Context, &CardRef)> = cards
        .iter()
        .filter_map(|(id, cards, _)| Some((contexts.get(id)?, cards)))
        .flat_map(|(context, cards)| cards.iter().map(move |c| (context, c)))
        .collect();
    let verdicts = Verdicts::load(&state, &probes).await?;

    let values = ValueData::load(pool).await?;

    // key -> (name on the card, English name, count, unit price)
    let mut shopping: BTreeMap<String, (String, String, usize, Option<Decimal>)> = BTreeMap::new();
    let mut versions = Vec::new();
    for version in rows {
        let (in_version, size) = cards
            .iter()
            .find(|(id, _, _)| *id == version.id)
            .map(|(_, cards, size)| (cards.iter().collect::<Vec<_>>(), *size))
            .unwrap_or_default();
        let missing = verdicts.missing(contexts.get(&version.id), in_version.iter().copied());

        let mut lines = Vec::new();
        for (key, card) in missing.lines {
            let card_data_id = in_version
                .iter()
                .find(|c| c.key == key)
                .and_then(|c| c.card_data_id);
            let unit_price = values.latest.of(card_data_id, None);
            let entry = shopping
                .entry(key)
                .or_insert_with(|| (card.name.clone(), card.english_name.clone(), 0, unit_price));
            entry.2 += card.count;
            lines.push(WantLine { card, unit_price });
        }
        versions.push(BuiltVersion {
            cards: size,
            wrong_text: missing.wrong_text,
            missing: lines,
            version,
        });
    }

    let estimate = shopping
        .values()
        .filter_map(|(_, _, count, price)| price.map(|p| p * Decimal::from(*count)))
        .sum();
    let unpriced = shopping.values().filter(|(_, _, _, p)| p.is_none()).count();
    let mut shopping: Vec<(String, String, usize)> = shopping
        .into_values()
        .map(|(name, english, count, _)| (name, english, count))
        .collect();
    shopping.sort_by_key(|(name, _, _)| name.to_lowercase());
    let shopping = shopping
        .into_iter()
        .map(|(_, english, count)| (count, english))
        .collect();

    Ok(ShoppingPage {
        versions,
        shopping,
        estimate,
        unpriced,
        errata_pending: verdicts.pending,
        budget_left: crate::routes::budget::left_this_month(pool).await?,
    })
}
