pub mod form;

use askama::Template;
use axum::extract::State;
use chrono::NaiveDate;
use yugioh_domain::budget::{self, BudgetSummary, BudgetTotals, LotSummary, RecapEntry};
use yugioh_domain::lot::{shipping_cost, LotType};
use rust_decimal::Decimal;

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
}

fn parse_date(s: &str) -> AppResult<NaiveDate> {
    s.parse()
        .map_err(|e| AppError::Internal(format!("invalid date {s:?}: {e}")))
}

impl BudgetData {
    pub async fn load(pool: &Pool) -> AppResult<BudgetData> {
        let lot_records: Vec<LotRecord> = sqlx::query_as(
            "SELECT id, lot_type, is_cancelled, buy_date, price, no_card_price FROM lot ORDER BY id",
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
            lots.push(LotSummary {
                lot_type,
                is_cancelled: lot.is_cancelled,
                buy_date: parse_date(&lot.buy_date)?,
                price: lot.price.0,
                shipping_cost: shipping_cost(lot.price.0, lot.no_card_price.0, prices.clone()),
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
        totals_prev: BudgetTotals::compute(&recap),
        totals: BudgetTotals::compute(&current),
        recap,
        available_debt: budget::available_debt(&data.budgets),
    })
}
