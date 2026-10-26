//! Everything the collection value is computed from, loaded once per page
//! (`/collection/value` and the dashboard's tile).

use std::collections::{BTreeMap, HashMap};

use chrono::{Duration, NaiveDate};
use rust_decimal::Decimal;
use yugioh_domain::valuation::{
    biggest_moves, totals_on, Copy, Lot, PriceMove, PriceSource, Prices, Totals,
};

use crate::store::db::{today, Pool};
use crate::store::prices::history::{latest_prices, prices_by_date};
use crate::store::valuation::{lots, owned_copies, OwnedCopy};

pub struct ValueData {
    /// The copies with their card (name, code, lot), in the order of `copies`.
    pub owned: Vec<OwnedCopy>,
    /// The same copies, as `yugioh_domain::valuation` takes them.
    pub copies: Vec<Copy>,
    pub lots: Vec<Lot>,
    pub by_date: BTreeMap<NaiveDate, Prices>,
    /// Each card's and product's latest price.
    pub latest: Prices,
}

impl ValueData {
    pub async fn load(pool: &Pool) -> sqlx::Result<ValueData> {
        let owned = owned_copies(pool).await?;
        let by_date = prices_by_date(pool).await?;
        Ok(ValueData {
            copies: owned.iter().map(|c| c.copy.clone()).collect(),
            owned,
            lots: lots(pool).await?,
            latest: latest_prices(&by_date),
            by_date,
        })
    }

    pub fn last_refresh(&self) -> Option<NaiveDate> {
        self.by_date.keys().next_back().copied()
    }

    /// The collection today, at each card's latest price.
    pub fn now(&self) -> Totals {
        totals_on(&self.copies, &self.lots, today(), &self.latest)
    }

    /// The value change since the last refresh at least `days` before the
    /// latest one (or the first refresh), `None` with fewer than two.
    pub fn change_over(&self, days: i64) -> Option<Decimal> {
        let (before, latest) = self.compared_dates(days)?;
        let value = |date| totals_on(&self.copies, &self.lots, date, &self.by_date[&date]).value;
        Some(value(latest) - value(before))
    }

    /// (the refresh at least `days` before the latest one, else the first;
    /// the latest), `None` with fewer than two refreshes.
    pub fn compared_dates(&self, days: i64) -> Option<(NaiveDate, NaiveDate)> {
        let latest = self.last_refresh()?;
        let first = *self.by_date.keys().next()?;
        let before = self
            .by_date
            .range(..=latest - Duration::days(days))
            .next_back()
            .map_or(first, |(date, _)| *date);
        (before < latest).then_some((before, latest))
    }

    /// The biggest price moves between two refreshes, of the cards owned
    /// now: one per price (a Cardmarket product, else a card name, the same
    /// source on both dates), with a copy of that card.
    pub fn price_moves(
        &self,
        before: NaiveDate,
        after: NaiveDate,
        floor: Decimal,
        limit: usize,
    ) -> Vec<(PriceMove, &OwnedCopy)> {
        let today = today();
        let (before, after) = (&self.by_date[&before], &self.by_date[&after]);

        let mut sources: HashMap<PriceSource, &OwnedCopy> = HashMap::new();
        for owned in self.owned.iter().filter(|c| c.copy.is_owned_on(today)) {
            let product = owned
                .copy
                .cardmarket_product_id
                .map(PriceSource::Product)
                .filter(|p| before.get(*p).is_some() && after.get(*p).is_some());
            let Some(source) = product.or(owned.copy.card_data_id.map(PriceSource::Card)) else {
                continue;
            };
            sources.entry(source).or_insert(owned);
        }

        // keyed by the id of the copy standing for each source
        let prices_on = |prices: &Prices| -> HashMap<i64, Decimal> {
            sources
                .iter()
                .filter_map(|(source, owned)| Some((owned.card_id, prices.get(*source)?)))
                .collect()
        };
        let by_card: HashMap<i64, &OwnedCopy> = sources
            .values()
            .map(|owned| (owned.card_id, *owned))
            .collect();

        biggest_moves(&prices_on(before), &prices_on(after), floor, limit)
            .into_iter()
            .map(|price_move| {
                let owned = by_card[&price_move.id];
                (price_move, owned)
            })
            .collect()
    }
}
