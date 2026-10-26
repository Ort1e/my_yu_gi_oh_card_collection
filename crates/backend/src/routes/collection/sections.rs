//! The sections of `/collection/value`, each built from [`ValueData`]: the
//! lines its tables show.

use std::cmp::Ordering;
use std::collections::HashMap;

use chrono::NaiveDate;
use domain::card::{CardStatus, Language};
use domain::valuation::{copy_value, history, lot_gain, Prices};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::json;

use super::data::ValueData;
use crate::store::db::Pool;
use crate::store::valuation::PurchaseLot;

/// Copies in each copy list, lots in each lot list, moves in each period.
pub const TOP_COPIES: usize = 20;
pub const TOP_LOTS: usize = 10;
const MOVES: usize = 10;
/// A price move smaller than this (€) isn't shown, whatever its %.
const MOVE_FLOOR: Decimal = Decimal::from_parts(50, 0, 0, false, 2);
const QUARTER_DAYS: i64 = 90;

/// `+12.5%`, `—` without a percentage.
pub fn percent(value: Option<Decimal>) -> String {
    value.map_or("—".to_string(), |p| format!("{p:+.1}%"))
}

/// At most `n` of `items` that `keep`, sorted by `order`, then by `id`.
pub fn top_n<T: Clone>(
    items: &[T],
    n: usize,
    keep: impl Fn(&T) -> bool,
    order: impl Fn(&T, &T) -> Ordering,
    id: impl Fn(&T) -> i64,
) -> Vec<T> {
    let mut chosen: Vec<T> = items.iter().filter(|i| keep(i)).cloned().collect();
    chosen.sort_by(|a, b| order(a, b).then(id(a).cmp(&id(b))));
    chosen.truncate(n);
    chosen
}

#[derive(Clone)]
pub struct CopyLine {
    pub card_id: i64,
    pub name: String,
    pub code: String,
    pub status: &'static str,
    pub value: Decimal,
    pub paid: Option<Decimal>,
    pub gain: Option<Decimal>,
}

/// Every copy owned on `today` with a price, valued.
pub fn copy_lines(data: &ValueData, today: NaiveDate) -> Vec<CopyLine> {
    data.owned
        .iter()
        .filter(|owned| owned.copy.is_owned_on(today))
        .filter_map(|owned| {
            let value = owned.copy.value(&data.latest)?.round_dp(2);
            Some(CopyLine {
                card_id: owned.card_id,
                name: owned.name.clone(),
                code: owned.code.clone().unwrap_or_default(),
                status: owned.copy.status.map_or("", CardStatus::code),
                value,
                paid: owned.copy.paid,
                gain: owned.copy.paid.map(|paid| value - paid),
            })
        })
        .collect()
}

pub struct MoveLine {
    pub card_id: i64,
    pub name: String,
    pub before: Decimal,
    pub after: Decimal,
    pub change: Decimal,
    /// `+50.0%`.
    pub percent: String,
}

/// The moves over one period, for the template.
pub struct Moves {
    pub title: String,
    pub lines: Vec<MoveLine>,
}

/// The moves since the previous refresh and over 3 months (when there are
/// two refreshes to compare).
pub fn moves(data: &ValueData) -> Vec<Moves> {
    let since_last = data.compared_dates(1).map(|(before, after)| {
        let title = format!("Since the previous refresh ({before})");
        period(data, before, after, title)
    });
    let quarter = data.compared_dates(QUARTER_DAYS).map(|(before, after)| {
        let title = format!("Over 3 months (since {before})");
        period(data, before, after, title)
    });
    since_last.into_iter().chain(quarter).collect()
}

fn period(data: &ValueData, before: NaiveDate, after: NaiveDate, title: String) -> Moves {
    let lines = data
        .price_moves(before, after, MOVE_FLOOR, MOVES)
        .into_iter()
        .map(|(price_move, owned)| MoveLine {
            card_id: owned.card_id,
            name: owned.name.clone(),
            before: price_move.before,
            after: price_move.after,
            change: price_move.change(),
            percent: percent(Some(price_move.percent())),
        })
        .collect();
    Moves { title, lines }
}

/// A purchase lot against its full price (`domain::valuation::lot_gain`).
#[derive(Clone)]
pub struct LotLine {
    pub id: i64,
    pub date: String,
    pub seller: String,
    pub price: Decimal,
    pub owned: usize,
    pub sold: usize,
    pub without_price: usize,
    pub value: Decimal,
    pub sold_for: Decimal,
    pub gain: Decimal,
    /// `+12.5%`, `—` for a free lot.
    pub percent: String,
}

/// Each purchase lot with cards against its full price (the lots without
/// cards, sleeves or boxes, are left out).
pub fn lot_lines(data: &ValueData, lots: Vec<PurchaseLot>, today: NaiveDate) -> Vec<LotLine> {
    let mut by_lot: HashMap<i64, Vec<&domain::valuation::Copy>> = HashMap::new();
    for owned in &data.owned {
        by_lot.entry(owned.lot_id).or_default().push(&owned.copy);
    }

    lots.into_iter()
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
        .collect()
}

/// The chart's data: the value and the net spending at each refresh, as
/// JSON safe to embed in a `<script>`.
pub fn chart_json(data: &ValueData) -> String {
    let points = history(&data.copies, &data.lots, &data.by_date);
    let to_f64 = |d: Decimal| d.to_f64().unwrap_or_default();
    let chart = json!({
        "labels": points.iter().map(|t| t.date.to_string()).collect::<Vec<_>>(),
        "value": points.iter().map(|t| to_f64(t.value)).collect::<Vec<_>>(),
        "net_spent": points.iter().map(|t| to_f64(t.spent - t.sold)).collect::<Vec<_>>(),
    });
    chart.to_string().replace("</", "<\\/")
}

pub struct DeckLine {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub cards: usize,
    pub value: Decimal,
    pub proxies: usize,
    /// What the proxies' cards would cost (at market price).
    pub proxies_value: Decimal,
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

/// The value of each deck's latest version: its real cards, and its proxies
/// (what the missing cards would cost), most valuable first.
pub async fn deck_values(pool: &Pool, prices: &Prices) -> sqlx::Result<Vec<DeckLine>> {
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
        let Some(deck) = decks.last_mut() else {
            continue;
        };

        let value = prices
            .of(row.card_data_id, row.cardmarket_product_id)
            .map(|price| {
                copy_value(
                    price,
                    row.status.as_deref().and_then(CardStatus::from_code),
                    row.code.as_deref().and_then(Language::of_code),
                )
            })
            .unwrap_or_default();
        if row.is_proxy {
            deck.proxies += 1;
            deck.proxies_value += value;
        } else {
            deck.cards += 1;
            deck.value += value;
        }
    }

    for deck in &mut decks {
        deck.value = deck.value.round_dp(2);
        deck.proxies_value = deck.proxies_value.round_dp(2);
    }
    decks.sort_by(|a, b| b.value.cmp(&a.value).then(a.id.cmp(&b.id)));
    Ok(decks)
}
