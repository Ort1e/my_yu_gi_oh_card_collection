//! `/collection/value`: what the collection is worth at market prices
//! (`domain::valuation`, prices from `store::card_prices` and
//! `store::cardmarket`).

use std::collections::{BTreeMap, HashMap};

use askama::Template;
use axum::extract::State;
use chrono::{Duration, NaiveDate};
use domain::card::{CardStatus, Language};
use domain::valuation::{
    biggest_moves, copy_value, history, lot_gain, totals_on, unit_totals_on, Lot, Prices, Totals,
    UnitTotals,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::json;

use crate::routes::budget::today;
use crate::store::card_prices::{
    latest_prices, lots, owned_copies, prices_by_date, purchase_lots, OwnedCopy,
};
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
    pub by_date: BTreeMap<NaiveDate, Prices>,
    /// Each card's and product's latest price.
    pub latest: Prices,
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

    /// The price moves of the cards owned now between two refreshes: one
    /// line per price (a Cardmarket product, else a card name), compared with
    /// the same source on both dates.
    fn moves(&self, before: NaiveDate, after: NaiveDate) -> Vec<MoveLine> {
        let today = today();
        let (before, after) = (&self.by_date[&before], &self.by_date[&after]);

        // one copy per price source, and its prices on both dates
        let mut sources: HashMap<(bool, i64), &OwnedCopy> = HashMap::new();
        for c in self.copies.iter().filter(|c| c.copy.is_owned_on(today)) {
            let product = c
                .copy
                .cardmarket_product_id
                .filter(|id| before.products.contains_key(id) && after.products.contains_key(id));
            let key = match (product, c.copy.card_data_id) {
                (Some(id), _) => (true, id),
                (None, Some(id)) => (false, id),
                (None, None) => continue,
            };
            sources.entry(key).or_insert(c);
        }
        let prices_on = |prices: &Prices| -> HashMap<i64, Decimal> {
            sources
                .iter()
                .filter_map(|(&(is_product, id), c)| {
                    let price = if is_product {
                        prices.products.get(&id)
                    } else {
                        prices.cards.get(&id)
                    };
                    Some((c.card_id, *price?))
                })
                .collect()
        };
        let cards: HashMap<i64, &OwnedCopy> = sources.values().map(|c| (c.card_id, *c)).collect();

        biggest_moves(&prices_on(before), &prices_on(after), MOVE_FLOOR, MOVES)
            .into_iter()
            .map(|price_move| MoveLine {
                card_id: price_move.id,
                name: cards[&price_move.id].name.clone(),
                before: price_move.before,
                after: price_move.after,
                change: price_move.change(),
                percent: percent(Some(price_move.percent())),
            })
            .collect()
    }
}

#[derive(Clone)]
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

/// A purchase lot against its full price (`domain::valuation::lot_gain`).
pub struct LotLine {
    id: i64,
    date: String,
    seller: String,
    price: Decimal,
    owned: usize,
    sold: usize,
    without_price: usize,
    value: Decimal,
    sold_for: Decimal,
    gain: Decimal,
    /// `+12.5%`, `—` for a free lot.
    percent: String,
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
    /// The owned copies that lost the most against their unite price.
    losses: Vec<CopyLine>,
    moves: Vec<Moves>,
    decks: Vec<DeckLine>,
    /// Each purchase lot with cards against its full price, newest first.
    lot_lines: Vec<LotLine>,
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

    /// For `partials/copy_values_table.html` (`{% let %}` would move the field).
    fn top_lines(&self) -> &[CopyLine] {
        &self.top
    }

    fn loss_lines(&self) -> &[CopyLine] {
        &self.losses
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
    let losses = biggest_losses(&top);
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

    let mut by_lot: HashMap<i64, Vec<&domain::valuation::Copy>> = HashMap::new();
    for c in &data.copies {
        by_lot.entry(c.lot_id).or_default().push(&c.copy);
    }
    // lots without cards (sleeves, boxes…) are left out
    let lot_lines = purchase_lots(pool)
        .await?
        .into_iter()
        .filter_map(|lot| {
            let copies = by_lot.get(&lot.id)?;
            let gain = lot_gain(lot.price.0, copies, today, &data.latest);
            Some(LotLine {
                id: lot.id,
                date: lot.date,
                seller: lot.seller.unwrap_or_default(),
                price: gain.price,
                owned: gain.owned,
                sold: gain.sold,
                without_price: gain.without_price,
                value: gain.value,
                sold_for: gain.sold_for,
                gain: gain.gain(),
                percent: percent(gain.gain_percent()),
            })
        })
        .collect();

    let now = data.now();
    let units = unit_totals_on(&copies, today, &data.latest);
    Ok(ValuePage {
        last_refresh: data.last_refresh(),
        gain: now.gain(),
        now,
        unit_gain: units.gain(),
        units,
        top,
        losses,
        moves,
        decks: deck_values(pool, &data.latest).await?,
        lot_lines,
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
    cardmarket_product_id: Option<i64>,
    status: Option<String>,
    code: Option<String>,
}

/// The copies with a negative gain (value − unite price), biggest loss
/// first (then by card id), at most [`TOP_COPIES`].
fn biggest_losses(copies: &[CopyLine]) -> Vec<CopyLine> {
    let mut losses: Vec<CopyLine> = copies
        .iter()
        .filter(|c| c.gain.is_some_and(|gain| gain.is_sign_negative()))
        .cloned()
        .collect();
    losses.sort_by(|a, b| a.gain.cmp(&b.gain).then(a.card_id.cmp(&b.card_id)));
    losses.truncate(TOP_COPIES);
    losses
}

/// The value of each deck's latest version: its real cards, and its proxies
/// (what the missing cards would cost).
async fn deck_values(pool: &Pool, prices: &Prices) -> sqlx::Result<Vec<DeckLine>> {
    let rows: Vec<DeckCardRow> = sqlx::query_as(
        "SELECT deck.id, deck.name, deck_version.version_name AS version, card.is_proxy,
                    card.card_data_id, card.cardmarket_product_id,
                    card.last_known_status AS status, card.code
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
        let price = prices.of(row.card_data_id, row.cardmarket_product_id);
        let value = price.map(|price| {
            copy_value(
                price,
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
