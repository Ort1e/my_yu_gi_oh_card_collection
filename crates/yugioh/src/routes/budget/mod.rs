//! `/budget`: the months' budget, spending and sales (most recent first),
//! and two charts: spending and shipping per month with the cards bought,
//! and how many cards were bought at each unit price.

pub mod form;

use std::collections::BTreeMap;

use askama::Template;
use axum::extract::State;
use chrono::NaiveDate;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::json;
use yugioh_domain::budget::{
    self, first_of_month, BudgetSummary, BudgetTotals, LotSummary, RecapEntry,
};
use yugioh_domain::lot::{shipping_cost, LotType};

use crate::store::db::today;
use crate::store::db::{Money, Pool};
use crate::web::error::{AppError, AppResult};
use crate::web::filters;
use crate::AppState;

/// Everything the budget computations need, loaded in three queries.
pub struct BudgetData {
    pub lots: Vec<LotSummary>,
    pub budgets: Vec<BudgetSummary>,
    /// (lot id, number of cards, unite prices) of each lot, same order as `lots`.
    pub lot_unites: Vec<(i64, Vec<Option<Decimal>>)>,
}

#[derive(sqlx::FromRow)]
struct LotRecord {
    id: i64,
    lot_type: String,
    is_cancelled: bool,
    buy_date: String,
    price: Money,
    no_card_price: Money,
    refund: Money,
}

fn parse_date(s: &str) -> AppResult<NaiveDate> {
    s.parse()
        .map_err(|e| AppError::Internal(format!("invalid date {s:?}: {e}")))
}

impl BudgetData {
    pub async fn load(pool: &Pool) -> AppResult<BudgetData> {
        let lot_records: Vec<LotRecord> = sqlx::query_as(
            "SELECT id, lot_type, is_cancelled, buy_date, price, no_card_price, refund
             FROM lot ORDER BY id",
        )
        .fetch_all(pool)
        .await?;
        let unites: Vec<(i64, Option<Money>)> =
            sqlx::query_as("SELECT lot_id, price FROM unite ORDER BY lot_id, id")
                .fetch_all(pool)
                .await?;
        let budget_rows: Vec<(i64, String, Money, Option<String>)> = sqlx::query_as(
            "SELECT b.id, b.month, b.amount,
                    (SELECT group_concat(a.amount, ' ') FROM apport a WHERE a.budget_id = b.id)
             FROM monthly_budget b ORDER BY b.month",
        )
        .fetch_all(pool)
        .await?;

        let mut lots = Vec::with_capacity(lot_records.len());
        let mut lot_unites = Vec::with_capacity(lot_records.len());
        for lot in lot_records {
            let prices: Vec<Option<Decimal>> = unites
                .iter()
                .filter(|(lot_id, _)| *lot_id == lot.id)
                .map(|(_, p)| p.map(|p| p.0))
                .collect();
            let lot_type = LotType::from_code(&lot.lot_type)
                .ok_or_else(|| AppError::Internal(format!("lot {}: bad lot_type", lot.id)))?;
            // what the lot cost: the price paid, less what the seller gave
            // back, in the lot's month
            let cost = lot.price.0 - lot.refund.0;
            lots.push(LotSummary {
                lot_type,
                is_cancelled: lot.is_cancelled,
                buy_date: parse_date(&lot.buy_date)?,
                price: cost,
                shipping_cost: shipping_cost(cost, lot.no_card_price.0, prices.clone()),
            });
            lot_unites.push((lot.id, prices));
        }

        let mut budgets = Vec::with_capacity(budget_rows.len());
        for (id, month, amount, apports) in budget_rows {
            // Summing in Rust keeps decimal precision (SQL SUM on TEXT is REAL).
            let apports_total = apports
                .unwrap_or_default()
                .split_whitespace()
                .map(str::parse::<Decimal>)
                .sum::<Result<Decimal, _>>()
                .map_err(|e| AppError::Internal(format!("budget {id}: bad apport: {e}")))?;
            budgets.push(BudgetSummary {
                id,
                month: parse_date(&month)?,
                amount: amount.0,
                apports_total,
            });
        }

        Ok(BudgetData {
            lots,
            budgets,
            lot_unites,
        })
    }
}
/// What is left to spend this month (the month's budget and one-off
/// contributions less what lots cost), negative when over it. `None` when no
/// budget was ever set. Shown in the deck builder's header.
pub async fn left_this_month(pool: &Pool) -> AppResult<Option<Money>> {
    let data = BudgetData::load(pool).await?;

    if data.budgets.is_empty() {
        return Ok(None);
    }

    // most recent month first
    let current = budget::recap(&data.lots, &data.budgets, today(), false);

    Ok(current
        .first()
        .map(|month| Money(month.budget - month.spent)))
}

#[derive(Template)]
#[template(path = "budget/overview.html")]
pub struct BudgetPage {
    /// Up to next month, most recent first.
    recap: Vec<RecapEntry>,
    /// Totals including next month ("previsional").
    totals_prev: BudgetTotals,
    /// Totals up to the current month.
    totals: BudgetTotals,
    available_debt: Decimal,
    /// Chart.js data, serialised as JSON.
    charts: String,
}

impl BudgetPage {
    fn totals_prev(&self) -> &BudgetTotals {
        &self.totals_prev
    }

    fn totals(&self) -> &BudgetTotals {
        &self.totals
    }
}

pub async fn overview(State(state): State<AppState>) -> AppResult<BudgetPage> {
    let data = BudgetData::load(&state.pool).await?;
    let today = today();
    let recap = budget::recap(&data.lots, &data.budgets, today, true);
    let current = budget::recap(&data.lots, &data.budgets, today, false);

    Ok(BudgetPage {
        charts: charts(&data, &recap),
        totals_prev: BudgetTotals::compute(&recap),
        totals: BudgetTotals::compute(&current),
        recap,
        available_debt: budget::available_debt(&data.budgets),
    })
}

/// The charts' data (`static/js/budget_charts.js`), months oldest first:
/// cards bought per month and number of cards per unit price, over the
/// purchases not cancelled.
fn charts(data: &BudgetData, recap: &[RecapEntry]) -> String {
    let mut cards_per_month: BTreeMap<_, u32> = BTreeMap::new();
    let mut cards_per_price: BTreeMap<Decimal, u32> = BTreeMap::new();
    for (lot, (_, prices)) in data.lots.iter().zip(&data.lot_unites) {
        if lot.is_cancelled || lot.lot_type != LotType::Purchase {
            continue;
        }
        *cards_per_month
            .entry(first_of_month(lot.buy_date))
            .or_default() += prices.len() as u32;
        for price in prices.iter().flatten() {
            *cards_per_price.entry(price.normalize()).or_default() += 1;
        }
    }

    let to_f64 = |d: Decimal| d.to_f64().unwrap_or_default();
    let months = || recap.iter().rev();
    let charts = json!({
        "labels": months().map(RecapEntry::month_str).collect::<Vec<_>>(),
        "card_spending": months().map(|e| to_f64(e.spent - e.shipping_cost)).collect::<Vec<_>>(),
        "shipping_costs": months().map(|e| to_f64(e.shipping_cost)).collect::<Vec<_>>(),
        "card_counts": months()
            .map(|e| cards_per_month.get(&e.month).copied().unwrap_or(0))
            .collect::<Vec<_>>(),
        "cards_by_cost": cards_per_price
            .iter()
            .map(|(price, n)| json!({ "x": to_f64(*price), "y": n }))
            .collect::<Vec<_>>(),
    });
    // `</` can't appear in JSON numbers/our labels, but escape it anyway
    // since this is embedded in a <script>.
    filters::json_for_script(&charts)
}
