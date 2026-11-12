//! The sections of `/collection/value`, each built from [`ValueData`]: the
//! lines its tables show.

use std::cmp::Ordering;
use std::collections::HashMap;

use chrono::NaiveDate;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::json;
use sqlx::QueryBuilder;
use yugioh_domain::card::{CardStatus, Language};
use yugioh_domain::deck_history::DeckZone;
use yugioh_domain::valuation::{copy_value, history, lot_gain, Prices};

use super::data::ValueData;
use crate::store::db::{push_ids, Pool};
use crate::store::lists;
use crate::store::valuation::PurchaseLot;
use crate::web::filters::french_date;

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
        let title = format!(
            "Since the previous refresh ({})",
            french_date(&before.to_string())
        );
        period(data, before, after, title)
    });
    let quarter = data.compared_dates(QUARTER_DAYS).map(|(before, after)| {
        let title = format!("Over 3 months (since {})", french_date(&before.to_string()));
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

/// A purchase lot against its full price (`yugioh_domain::valuation::lot_gain`).
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
    let mut by_lot: HashMap<i64, Vec<&yugioh_domain::valuation::Copy>> = HashMap::new();
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
        "labels": points.iter().map(|t| french_date(&t.date.to_string())).collect::<Vec<_>>(),
        "value": points.iter().map(|t| to_f64(t.value)).collect::<Vec<_>>(),
        "net_spent": points.iter().map(|t| to_f64(t.spent - t.sold)).collect::<Vec<_>>(),
    });
    crate::web::filters::json_for_script(&chart)
}

pub struct DeckLine {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub cards: usize,
    pub value: Decimal,
    /// The cards the collection lacks.
    pub proxies: usize,
    /// What they would cost (at market price).
    pub proxies_value: Decimal,
}

/// A copy a deck plays, for its value.
#[derive(sqlx::FromRow)]
struct PlayedCopy {
    card_data_id: Option<i64>,
    cardmarket_product_id: Option<i64>,
    last_known_status: Option<String>,
    code: Option<String>,
}

/// The value of each deck's latest version, most valuable first: the
/// copies it plays (a built version's, else the ones building it would take:
/// `lists::playing_copies`) with their condition and language, the other
/// owned cards at market price, and what the missing ones
/// (`lists::missing`) would cost.
pub async fn deck_values(pool: &Pool, prices: &Prices) -> sqlx::Result<Vec<DeckLine>> {
    let latest: Vec<(i64, bool, i64, String, String)> = sqlx::query_as(
        "SELECT deck_version.id, deck_version.is_built, deck.id, deck.name, deck_version.version_name
         FROM deck_version JOIN deck ON deck.id = deck_version.deck_id
         WHERE deck_version.id IN (SELECT max(id) FROM deck_version GROUP BY deck_id)
         ORDER BY deck.name, deck.id",
    )
    .fetch_all(pool)
    .await?;

    let mut conn = pool.acquire().await?;
    let mut decks: Vec<DeckLine> = Vec::new();
    for (version_id, is_built, id, name, version) in latest {
        let lines = lists::lines(&mut conn, version_id).await?;
        if lines.is_empty() {
            continue;
        }
        let mut short: HashMap<(DeckZone, u32), u32> =
            lists::missing(&mut conn, version_id, &lines)
                .await?
                .into_iter()
                .map(|m| ((m.line.zone, m.line.position), m.count))
                .collect();
        let playing = lists::playing_copies(&mut conn, version_id, is_built).await?;
        let mut copies: Vec<PlayedCopy> = Vec::new();
        if !playing.is_empty() {
            let mut qb = QueryBuilder::new(
                "SELECT card_data_id, cardmarket_product_id, last_known_status, code
                 FROM card WHERE id IN ",
            );
            push_ids(&mut qb, playing);
            copies = qb.build_query_as().fetch_all(&mut *conn).await?;
        }

        let mut deck = DeckLine {
            id,
            name,
            version,
            cards: 0,
            value: Decimal::ZERO,
            proxies: short.values().map(|n| *n as usize).sum(),
            proxies_value: Decimal::ZERO,
        };
        let mut played: HashMap<Option<i64>, usize> = HashMap::new();
        for copy in &copies {
            *played.entry(copy.card_data_id).or_default() += 1;
            deck.cards += 1;
            deck.value += prices
                .of(copy.card_data_id, copy.cardmarket_product_id)
                .map(|price| {
                    copy_value(
                        price,
                        copy.last_known_status
                            .as_deref()
                            .and_then(CardStatus::from_code),
                        copy.code.as_deref().and_then(Language::of_code),
                    )
                })
                .unwrap_or_default();
        }
        // the rest of each card: owned (a built deck's copy not taken yet)
        // at market price, else missing
        for line in &lines {
            let price = prices.of(line.card_data_id, None).unwrap_or_default();
            let lacking = short.remove(&(line.zone, line.position)).unwrap_or(0) as usize;
            let in_copies = played.entry(line.card_data_id).or_default();
            let taken = (*in_copies).min(line.count as usize);
            *in_copies -= taken;
            let other = (line.count as usize - taken).saturating_sub(lacking);
            deck.cards += other;
            deck.value += price * Decimal::from(other);
            deck.proxies_value += price * Decimal::from(lacking);
        }
        decks.push(deck);
    }
    drop(conn);

    for deck in &mut decks {
        deck.value = deck.value.round_dp(2);
        deck.proxies_value = deck.proxies_value.round_dp(2);
    }
    decks.sort_by(|a, b| b.value.cmp(&a.value).then(a.id.cmp(&b.id)));
    Ok(decks)
}
