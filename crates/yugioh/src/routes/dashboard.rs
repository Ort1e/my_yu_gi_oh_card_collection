//! `/`: the dashboard, the collection first (cards owned, its market value
//! and its move, the last lots), then one budget tile linking to `/budget`,
//! which has the monthly table and the spending charts.

use askama::Template;
use axum::extract::State;
use rust_decimal::Decimal;

use super::budget::left_this_month;
use super::collection::ValueData;
use crate::store::cards::SOLD_EXISTS;
use crate::store::db::Money;
use crate::store::models::LotRow;
use crate::web::error::AppResult;
use crate::web::filters;
use crate::AppState;

/// Lots in the "Last lots" table.
const LAST_LOTS: i64 = 5;

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct Dashboard {
    /// Real copies owned (not sold, not proxies).
    owned: i64,
    /// `None` before the first price refresh.
    value: Option<ValueTile>,
    /// Left to spend this month, `None` when no budget was ever set.
    budget_left: Option<Money>,
    /// The last lots bought or sold (not cancelled), newest first.
    lots: Vec<LotRow>,
}

/// The collection value tile, linking to `/collection/value`.
pub struct ValueTile {
    value: Decimal,
    /// Over the last 4 weeks of refreshes, `None` with a single refresh.
    change: Option<Decimal>,
}

const VALUE_CHANGE_DAYS: i64 = 28;

pub async fn index(State(state): State<AppState>) -> AppResult<Dashboard> {
    let pool = &state.pool;
    let owned = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM card WHERE card.is_proxy = 0 AND NOT {SOLD_EXISTS}"
    ))
    .fetch_one(pool)
    .await?;

    let value_data = ValueData::load(pool).await?;
    let value = value_data.last_refresh().map(|_| ValueTile {
        value: value_data.now().value,
        change: value_data.change_over(VALUE_CHANGE_DAYS),
    });

    let lots = sqlx::query_as(&format!(
        "{} WHERE lot.is_cancelled = 0 ORDER BY lot.buy_date DESC, lot.id DESC LIMIT ?",
        LotRow::SELECT
    ))
    .bind(LAST_LOTS)
    .fetch_all(pool)
    .await?;

    Ok(Dashboard {
        owned,
        value,
        budget_left: left_this_month(pool).await?,
        lots,
    })
}
