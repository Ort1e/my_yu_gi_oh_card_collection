use std::collections::BTreeMap;

use askama::Template;
use axum::extract::State;
use domain::budget::{self, first_of_month, BudgetTotals, RecapEntry};
use domain::lot::LotType;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::json;

use super::budget::{today, BudgetData};
use crate::error::AppResult;
use crate::filters;
use crate::AppState;

const MONTHS_TO_DISPLAY: usize = 5;

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct Dashboard {
    /// The last months (up to next month), oldest first.
    recap: Vec<RecapEntry>,
    /// Totals over every month.
    totals: BudgetTotals,
    available_debt: Decimal,
    /// Chart.js data, serialised as JSON.
    charts: String,
}

pub async fn index(State(state): State<AppState>) -> AppResult<Dashboard> {
    let data = BudgetData::load(&state.pool).await?;
    let mut recap = budget::recap(&data.lots, &data.budgets, today(), true);
    recap.reverse(); // oldest first

    // Cards bought per month and number of cards per unit price, over the
    // non-cancelled purchases.
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
    let charts = json!({
        "labels": recap.iter().map(RecapEntry::month_str).collect::<Vec<_>>(),
        "card_spending": recap.iter().map(|e| to_f64(e.spent - e.shipping_cost)).collect::<Vec<_>>(),
        "shipping_costs": recap.iter().map(|e| to_f64(e.shipping_cost)).collect::<Vec<_>>(),
        "card_counts": recap
            .iter()
            .map(|e| cards_per_month.get(&e.month).copied().unwrap_or(0))
            .collect::<Vec<_>>(),
        "cards_by_cost": cards_per_price
            .iter()
            .map(|(price, n)| json!({ "x": to_f64(*price), "y": n }))
            .collect::<Vec<_>>(),
    });

    let totals = BudgetTotals::compute(&recap);
    let shown = recap.len().saturating_sub(MONTHS_TO_DISPLAY);
    recap.drain(..shown);

    Ok(Dashboard {
        recap,
        totals,
        available_debt: budget::available_debt(&data.budgets),
        // `</` can't appear in JSON numbers/our labels, but escape it anyway
        // since this is embedded in a <script>.
        charts: charts.to_string().replace("</", "<\\/"),
    })
}
