//! `/collection/stats`: the Yu-Gi-Oh! collection in numbers, also in the
//! public copy, so nothing about lots or prices paid: the cards owned (no
//! proxy, not sold) at their market estimate, grouped by
//! `yugioh_domain::stats`; the value over time and the price moves of the
//! value page's data.

use std::collections::{HashMap, HashSet};

use askama::Template;
use axum::extract::State;
use chrono::NaiveDate;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::json;
use yugioh_domain::card::{CardStatus, Language};
use yugioh_domain::rarity::Rarity;
use yugioh_domain::stats::{breakdown, kind, set_prefix, CardKind, Group};
use yugioh_domain::valuation::history;

use super::sections::{moves, Moves};
use super::ValueData;
use crate::store::cards::SOLD_EXISTS;
use crate::store::models::CardRow;
use crate::web::error::AppResult;
use crate::web::filters;
use crate::AppState;
use common::web::tiles::CardTile;

/// The cards shown as the most valuable and the latest added.
const TILES: usize = 12;
/// The sets listed, those with most copies.
const SETS: usize = 15;

/// One row of a breakdown table.
pub struct Line {
    label: String,
    copies: usize,
    value: Decimal,
    /// Its share of the value: `12.5 %`.
    share: String,
}

#[derive(Template)]
#[template(path = "collection/stats.html")]
pub struct StatsPage {
    last_refresh: Option<NaiveDate>,
    copies: usize,
    distinct: usize,
    value: Decimal,
    without_price: usize,
    tables: Vec<(&'static str, Vec<Line>)>,
    top: Vec<CardTile>,
    recent: Vec<CardTile>,
    moves: Vec<Moves>,
    /// Chart.js data (the value over time only), serialised as JSON.
    chart: String,
}

impl StatsPage {
    fn top_tiles(&self) -> &[CardTile] {
        &self.top
    }

    fn recent_tiles(&self) -> &[CardTile] {
        &self.recent
    }
}

pub async fn page(State(state): State<AppState>) -> AppResult<StatsPage> {
    let pool = &state.pool;
    let data = ValueData::load(pool).await?;
    let cards: Vec<CardRow> = sqlx::query_as(&format!(
        "SELECT {} FROM card WHERE card.is_proxy = 0 AND NOT {SOLD_EXISTS}
         ORDER BY card.en_name, card.id",
        CardRow::columns()
    ))
    .fetch_all(pool)
    .await?;
    let types: HashMap<i64, String> = sqlx::query_as::<_, (i64, Option<String>)>(
        "SELECT id, json_extract(json_data, '$.type') FROM card_data",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(|(id, card_type)| Some((id, card_type?)))
    .collect();
    let added: HashMap<i64, NaiveDate> = data
        .owned
        .iter()
        .map(|owned| (owned.card_id, owned.copy.bought))
        .collect();

    let value_of = |card: &CardRow| card.estimate().map(|m| m.0);
    let value: Decimal = cards.iter().filter_map(value_of).sum();
    let lines = |groups: Vec<Group<String>>| -> Vec<Line> {
        groups
            .into_iter()
            .map(|group| Line {
                share: share(group.value, value),
                label: group.key,
                copies: group.copies,
                value: group.value,
            })
            .collect()
    };

    // in the guide's order, unknown last
    let rarity_rank = |card: &CardRow| {
        let rarity = card.rarity.as_deref().and_then(Rarity::from_label);
        rarity.map_or(Rarity::ALL.len(), |r| {
            Rarity::ALL.iter().position(|x| *x == r).unwrap_or_default()
        })
    };
    let mut rarities = breakdown(
        cards
            .iter()
            .map(|c| ((rarity_rank(c), label(c.rarity.as_deref())), value_of(c))),
    );
    rarities.sort_by_key(|g| g.key.0);
    let rarities = rarities
        .into_iter()
        .map(|g| Group {
            key: g.key.1,
            copies: g.copies,
            value: g.value,
        })
        .collect();

    let card_kind = |card: &CardRow| {
        card.card_data_id
            .and_then(|id| types.get(&id))
            .map_or(CardKind::Other, |t| kind(t))
    };
    let kinds = breakdown(cards.iter().map(|c| (card_kind(c), value_of(c))))
        .into_iter()
        .map(|g| Group {
            key: g.key.label().to_string(),
            copies: g.copies,
            value: g.value,
        })
        .collect();

    let mut sets = breakdown(cards.iter().map(|c| {
        let set = c.code.as_deref().and_then(set_prefix);
        (label(set), value_of(c))
    }));
    sets.sort_by(|a, b| b.copies.cmp(&a.copies).then(b.value.cmp(&a.value)));
    sets.truncate(SETS);

    let languages = breakdown(cards.iter().map(|c| {
        let language = c.code.as_deref().and_then(Language::from_code);
        (label(language.map(Language::code)), value_of(c))
    }));

    let status_rank = |card: &CardRow| {
        card.status()
            .and_then(|s| CardStatus::ALL.iter().position(|x| *x == s))
            .unwrap_or(CardStatus::ALL.len())
    };
    let mut conditions = breakdown(cards.iter().map(|c| {
        (
            (status_rank(c), label(c.status().map(CardStatus::label))),
            value_of(c),
        )
    }));
    conditions.sort_by_key(|g| g.key.0);
    let conditions = conditions
        .into_iter()
        .map(|g| Group {
            key: g.key.1,
            copies: g.copies,
            value: g.value,
        })
        .collect();

    let mut by_value: Vec<&CardRow> = cards.iter().filter(|c| value_of(c).is_some()).collect();
    by_value.sort_by_key(|c| std::cmp::Reverse(value_of(c)));
    let mut by_date: Vec<&CardRow> = cards.iter().filter(|c| added.contains_key(&c.id)).collect();
    by_date.sort_by(|a, b| added[&b.id].cmp(&added[&a.id]).then(b.id.cmp(&a.id)));
    let tiles = |cards: Vec<&CardRow>| cards.into_iter().take(TILES).map(CardRow::tile).collect();

    let distinct: HashSet<String> = cards.iter().map(|c| c.name.to_lowercase()).collect();
    Ok(StatsPage {
        last_refresh: data.last_refresh(),
        copies: cards.len(),
        distinct: distinct.len(),
        without_price: cards.iter().filter(|c| value_of(c).is_none()).count(),
        value,
        tables: vec![
            ("By rarity", lines(rarities)),
            ("By kind", lines(kinds)),
            ("By set (the 15 with most copies)", lines(sets)),
            ("By language", lines(languages)),
            ("By condition", lines(conditions)),
        ],
        top: tiles(by_value),
        recent: tiles(by_date),
        moves: moves(&data),
        chart: value_chart(&data),
    })
}

/// A group's name, `Unknown` when not noted.
fn label(name: Option<&str>) -> String {
    name.unwrap_or("Unknown").to_string()
}

/// `part` of `whole`, as `12.5 %`.
fn share(part: Decimal, whole: Decimal) -> String {
    if whole.is_zero() {
        return String::new();
    }
    format!("{:.1} %", (part / whole * Decimal::ONE_HUNDRED).round_dp(1))
}

/// The value of the cards owned at each price refresh (the value page's
/// chart without what was spent: that's the prices paid).
fn value_chart(data: &ValueData) -> String {
    let points = history(&data.copies, &data.lots, &data.by_date);
    let chart = json!({
        "labels": points.iter().map(|t| t.date.to_string()).collect::<Vec<_>>(),
        "value": points.iter().map(|t| t.value.to_f64().unwrap_or_default()).collect::<Vec<_>>(),
    });
    crate::web::filters::json_for_script(&chart)
}
