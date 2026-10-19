//! `/collection/value`: what the collection is worth at market prices
//! (`domain::valuation`, prices from `store::card_prices`).

use std::collections::{BTreeMap, HashMap};

use askama::Template;
use axum::extract::State;
use chrono::{Duration, NaiveDate};
use domain::card::{CardStatus, Language};
use domain::valuation::{
    biggest_moves, copy_value, history, totals_on, unit_totals_on, Lot, Totals, UnitTotals,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::json;

use crate::routes::budget::today;
use crate::store::card_prices::{latest_prices, lots, owned_copies, prices_by_date, OwnedCopy};
use crate::store::db::Pool;
use crate::web::error::AppResult;
use crate::web::filters;
use crate::AppState;

const TOP_COPIES: usize = 20;
const MOVES: usize = 10;
/// A price move smaller than this (€) isn't shown, whatever its %.
const MOVE_FLOOR: Decimal = Decimal::from_parts(50, 0, 0, false, 2);
const QUARTER_DAYS: i64 = 90;

/// Everything the valuations are computed from.
pub struct ValueData {
    pub copies: Vec<OwnedCopy>,
    pub lots: Vec<Lot>,
    pub by_date: BTreeMap<NaiveDate, HashMap<i64, Decimal>>,
    /// Each card's latest price.
    pub latest: HashMap<i64, Decimal>,
}

impl ValueData {
    pub async fn load(pool: &Pool) -> sqlx::Result<ValueData> {
        let by_date = prices_by_date(pool).await?;
        Ok(ValueData {
            copies: owned_copies(pool).await?,
            lots: lots(pool).await?,
            latest: latest_prices(&by_date),
            by_date,
        })
    }

    fn copies(&self) -> Vec<domain::valuation::Copy> {
        self.copies.iter().map(|c| c.copy.clone()).collect()
    }

    pub fn last_refresh(&self) -> Option<NaiveDate> {
        self.by_date.keys().next_back().copied()
    }

    /// The collection today, at each card's latest price.
    pub fn now(&self) -> Totals {
        totals_on(&self.copies(), &self.lots, today(), &self.latest)
    }

    /// The value change since the last refresh at least `days` before the
    /// latest one (or the first refresh), `None` with fewer than two.
    pub fn change_over(&self, days: i64) -> Option<Decimal> {
        let (before, latest) = self.compared_dates(days)?;
        let copies = self.copies();
        let value = |date| totals_on(&copies, &self.lots, date, &self.by_date[&date]).value;
        Some(value(latest) - value(before))
    }

    /// (refresh at least `days` before the latest, latest).
    fn compared_dates(&self, days: i64) -> Option<(NaiveDate, NaiveDate)> {
        let latest = self.last_refresh()?;
        let first = *self.by_date.keys().next()?;
        let before = self
            .by_date
            .range(..=latest - Duration::days(days))
            .next_back()
            .map_or(first, |(date, _)| *date);
        (before < latest).then_some((before, latest))
    }

    /// The price moves of the cards owned now between two refreshes.
    fn moves(&self, before: NaiveDate, after: NaiveDate) -> Vec<MoveLine> {
        let today = today();
        let owned: HashMap<i64, &OwnedCopy> = self
            .copies
            .iter()
            .filter(|c| c.copy.is_owned_on(today))
            .filter_map(|c| Some((c.copy.card_data_id?, c)))
            .collect();
        let only_owned = |prices: &HashMap<i64, Decimal>| -> HashMap<i64, Decimal> {
            prices
                .iter()
                .filter(|(id, _)| owned.contains_key(id))
                .map(|(id, price)| (*id, *price))
                .collect()
        };

        biggest_moves(
            &only_owned(&self.by_date[&before]),
            &only_owned(&self.by_date[&after]),
            MOVE_FLOOR,
            MOVES,
        )
        .into_iter()
        .map(|price_move| {
            let card = owned[&price_move.card_data_id];
            MoveLine {
                card_id: card.card_id,
                name: card.name.clone(),
                before: price_move.before,
                after: price_move.after,
                change: price_move.change(),
                percent: percent(Some(price_move.percent())),
            }
        })
        .collect()
    }
}

pub struct CopyLine {
    card_id: i64,
    name: String,
    code: String,
    status: &'static str,
    value: Decimal,
    paid: Option<Decimal>,
    gain: Option<Decimal>,
}

pub struct MoveLine {
    card_id: i64,
    name: String,
    before: Decimal,
    after: Decimal,
    change: Decimal,
    /// `+50.0%`.
    percent: String,
}

pub struct DeckLine {
    id: i64,
    name: String,
    version: String,
    cards: usize,
    value: Decimal,
    proxies: usize,
    /// What the proxies' cards would cost (at market price).
    proxies_value: Decimal,
}

/// The moves over one period, for the template.
pub struct Moves {
    title: String,
    lines: Vec<MoveLine>,
}

#[derive(Template)]
#[template(path = "collection/value.html")]
pub struct ValuePage {
    last_refresh: Option<NaiveDate>,
    now: Totals,
    /// `now.gain()` (askama filters take references).
    gain: Decimal,
    units: UnitTotals,
    unit_gain: Decimal,
    top: Vec<CopyLine>,
    moves: Vec<Moves>,
    decks: Vec<DeckLine>,
    /// Chart.js data, serialised as JSON.
    chart: String,
}

impl ValuePage {
    fn gain_percent(&self) -> String {
        percent(self.now.gain_percent())
    }

    fn unit_gain_percent(&self) -> String {
        percent(self.units.gain_percent())
    }

    fn last_refresh_str(&self) -> String {
        self.last_refresh.map(|d| d.to_string()).unwrap_or_default()
    }
}

fn percent(value: Option<Decimal>) -> String {
    value.map_or("—".to_string(), |p| format!("{p:+.1}%"))
}

pub async fn page(State(state): State<AppState>) -> AppResult<ValuePage> {
    let pool = &state.pool;
    let data = ValueData::load(pool).await?;
    let today = today();
    let copies = data.copies();

    let mut top: Vec<CopyLine> = data
        .copies
        .iter()
        .filter(|c| c.copy.is_owned_on(today))
        .filter_map(|c| {
            let value = c.copy.value(&data.latest)?.round_dp(2);
            Some(CopyLine {
                card_id: c.card_id,
                name: c.name.clone(),
                code: c.code.clone().unwrap_or_default(),
                status: c.copy.status.map_or("", CardStatus::code),
                value,
                paid: c.copy.paid,
                gain: c.copy.paid.map(|paid| value - paid),
            })
        })
        .collect();
    top.sort_by(|a, b| b.value.cmp(&a.value).then(a.card_id.cmp(&b.card_id)));
    top.truncate(TOP_COPIES);

    let mut moves = Vec::new();
    if let Some((before, after)) = data.compared_dates(1) {
        moves.push(Moves {
            title: format!("Since the previous refresh ({before})"),
            lines: data.moves(before, after),
        });
    }
    if let Some((before, after)) = data.compared_dates(QUARTER_DAYS) {
        moves.push(Moves {
            title: format!("Over 3 months (since {before})"),
            lines: data.moves(before, after),
        });
    }

    let points = history(&copies, &data.lots, &data.by_date);
    let to_f64 = |d: Decimal| d.to_f64().unwrap_or_default();
    let chart = json!({
        "labels": points.iter().map(|t| t.date.to_string()).collect::<Vec<_>>(),
        "value": points.iter().map(|t| to_f64(t.value)).collect::<Vec<_>>(),
        "net_spent": points.iter().map(|t| to_f64(t.spent - t.sold)).collect::<Vec<_>>(),
    });

    let now = data.now();
    let units = unit_totals_on(&copies, today, &data.latest);
    Ok(ValuePage {
        last_refresh: data.last_refresh(),
        gain: now.gain(),
        now,
        unit_gain: units.gain(),
        units,
        top,
        moves,
        decks: deck_values(pool, &data.latest).await?,
        // embedded in a <script>
        chart: chart.to_string().replace("</", "<\\/"),
    })
}

#[derive(sqlx::FromRow)]
struct DeckCardRow {
    id: i64,
    name: String,
    version: String,
    is_proxy: bool,
    card_data_id: Option<i64>,
    status: Option<String>,
    code: Option<String>,
}

/// The value of each deck's latest version: its real cards, and its proxies
/// (what the missing cards would cost).
async fn deck_values(pool: &Pool, prices: &HashMap<i64, Decimal>) -> sqlx::Result<Vec<DeckLine>> {
    let rows: Vec<DeckCardRow> = sqlx::query_as(
        "SELECT deck.id, deck.name, deck_version.version_name AS version, card.is_proxy,
                    card.card_data_id, card.last_known_status AS status, card.code
             FROM deck_version
             JOIN deck ON deck.id = deck_version.deck_id
             JOIN (SELECT deck_version_id, card_id FROM deck_version_main_deck
                   UNION ALL SELECT deck_version_id, card_id FROM deck_version_extra_deck
                   UNION ALL SELECT deck_version_id, card_id FROM deck_version_side_deck) AS zone
               ON zone.deck_version_id = deck_version.id
             JOIN card ON card.id = zone.card_id
             WHERE deck_version.id IN (SELECT max(id) FROM deck_version GROUP BY deck_id)
             ORDER BY deck.name, deck.id",
    )
    .fetch_all(pool)
    .await?;

    let mut decks: Vec<DeckLine> = Vec::new();
    for row in rows {
        if decks.last().is_none_or(|deck| deck.id != row.id) {
            decks.push(DeckLine {
                id: row.id,
                name: row.name,
                version: row.version,
                cards: 0,
                value: Decimal::ZERO,
                proxies: 0,
                proxies_value: Decimal::ZERO,
            });
        }
        let deck = decks.last_mut().expect("just pushed");
        let value = row
            .card_data_id
            .and_then(|id| prices.get(&id))
            .map(|price| {
                copy_value(
                    *price,
                    row.status.as_deref().and_then(CardStatus::from_code),
                    row.code.as_deref().and_then(Language::of_code),
                )
            });

        if row.is_proxy {
            deck.proxies += 1;
            deck.proxies_value += value.unwrap_or_default();
        } else {
            deck.cards += 1;
            deck.value += value.unwrap_or_default();
        }
    }

    for deck in &mut decks {
        deck.value = deck.value.round_dp(2);
        deck.proxies_value = deck.proxies_value.round_dp(2);
    }
    decks.sort_by(|a, b| b.value.cmp(&a.value).then(a.id.cmp(&b.id)));
    Ok(decks)
}
