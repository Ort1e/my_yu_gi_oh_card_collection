//! `/collection/value`: what the collection is worth at market prices
//! (`yugioh_domain::valuation`; prices from `store::prices`, copies and lots from
//! `store::valuation`). [`data`] loads, [`sections`] builds each table.
//! [`stats`]: `/collection/stats`, the public part of these numbers.

mod data;
mod sections;
pub mod stats;

pub use data::ValueData;

use askama::Template;
use axum::extract::State;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use yugioh_domain::valuation::{unit_totals_on, Totals, UnitTotals};

use sections::{
    chart_json, copy_lines, deck_values, lot_lines, moves, percent, top_n, CopyLine, DeckLine,
    LotLine, Moves, TOP_COPIES, TOP_LOTS,
};

use crate::store::db::today;
use crate::store::valuation::purchase_lots;
use crate::web::error::AppResult;
use crate::web::filters;
use crate::AppState;

#[derive(Template)]
#[template(path = "collection/value.html")]
pub struct ValuePage {
    last_refresh: Option<NaiveDate>,
    now: Totals,
    /// `now.gain()` (askama filters take references).
    gain: Decimal,
    units: UnitTotals,
    unit_gain: Decimal,
    /// The most valuable copies, and the ones that lost the most against
    /// their unite price.
    top: Vec<CopyLine>,
    losses: Vec<CopyLine>,
    moves: Vec<Moves>,
    decks: Vec<DeckLine>,
    /// The purchase lots (with cards) against their full price: the
    /// [`TOP_LOTS`] whose cards are worth the most today, with the biggest
    /// gains, with the worst losses.
    lots_by_value: Vec<LotLine>,
    lots_by_gain: Vec<LotLine>,
    lots_by_loss: Vec<LotLine>,
    /// Chart.js data, serialised as JSON.
    chart: String,
}

/// Getters for the table partials (`{% let %}` would move the fields).
impl ValuePage {
    fn top_lines(&self) -> &[CopyLine] {
        &self.top
    }

    fn loss_lines(&self) -> &[CopyLine] {
        &self.losses
    }

    fn lots_by_value(&self) -> &[LotLine] {
        &self.lots_by_value
    }

    fn lots_by_gain(&self) -> &[LotLine] {
        &self.lots_by_gain
    }

    fn lots_by_loss(&self) -> &[LotLine] {
        &self.lots_by_loss
    }

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

pub async fn page(State(state): State<AppState>) -> AppResult<ValuePage> {
    let pool = &state.pool;
    let data = ValueData::load(pool).await?;
    let today = today();

    let copies = copy_lines(&data, today);
    let by_card = |line: &CopyLine| line.card_id;
    let top = top_n(
        &copies,
        TOP_COPIES,
        |_| true,
        |a, b| b.value.cmp(&a.value),
        by_card,
    );
    let losses = top_n(
        &copies,
        TOP_COPIES,
        |c| c.gain.is_some_and(|gain| gain.is_sign_negative()),
        |a, b| a.gain.cmp(&b.gain),
        by_card,
    );

    let lots = lot_lines(&data, purchase_lots(pool).await?, today);
    let by_lot = |line: &LotLine| line.id;
    let lots_by_value = top_n(
        &lots,
        TOP_LOTS,
        |_| true,
        |a, b| b.value.cmp(&a.value),
        by_lot,
    );
    let lots_by_gain = top_n(
        &lots,
        TOP_LOTS,
        |l| l.gain > Decimal::ZERO,
        |a, b| b.gain.cmp(&a.gain),
        by_lot,
    );
    let lots_by_loss = top_n(
        &lots,
        TOP_LOTS,
        |l| l.gain < Decimal::ZERO,
        |a, b| a.gain.cmp(&b.gain),
        by_lot,
    );

    let now = data.now();
    let units = unit_totals_on(&data.copies, today, &data.latest);
    Ok(ValuePage {
        last_refresh: data.last_refresh(),
        gain: now.gain(),
        now,
        unit_gain: units.gain(),
        units,
        top,
        losses,
        moves: moves(&data),
        decks: deck_values(pool, &data.latest).await?,
        lots_by_value,
        lots_by_gain,
        lots_by_loss,
        chart: chart_json(&data),
    })
}
