//! The stored prices, ygoprodeck's (`card_price`) and Cardmarket's
//! (`cardmarket_price`), as `domain::valuation::Prices` by refresh date.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use domain::valuation::Prices;

use crate::store::db::{Money, Pool};

/// The date of the last refresh, `None` before the first one.
pub async fn latest_refresh(pool: &Pool) -> sqlx::Result<Option<NaiveDate>> {
    let date: Option<String> = sqlx::query_scalar("SELECT max(date) FROM card_price")
        .fetch_one(pool)
        .await?;
    Ok(date.and_then(|d| d.parse().ok()))
}

/// Every price, by refresh date: ygoprodeck's by card data, Cardmarket's
/// by product (`store::prices::cardmarket`).
pub async fn prices_by_date(pool: &Pool) -> sqlx::Result<BTreeMap<NaiveDate, Prices>> {
    let cards: Vec<(i64, String, Money)> = sqlx::query_as(
        "SELECT card_data_id, date, cardmarket_price FROM card_price
         WHERE cardmarket_price IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    let products: Vec<(i64, String, Money)> =
        sqlx::query_as("SELECT product_id, date, price FROM cardmarket_price")
            .fetch_all(pool)
            .await?;

    let mut by_date: BTreeMap<NaiveDate, Prices> = BTreeMap::new();
    for (card_data_id, date, price) in cards {
        if let Ok(date) = date.parse() {
            let prices = by_date.entry(date).or_default();
            prices.cards.insert(card_data_id, price.0);
        }
    }
    for (product_id, date, price) in products {
        if let Ok(date) = date.parse() {
            let prices = by_date.entry(date).or_default();
            prices.products.insert(product_id, price.0);
        }
    }
    Ok(by_date)
}

/// Each card's and product's most recent price.
pub fn latest_prices(by_date: &BTreeMap<NaiveDate, Prices>) -> Prices {
    let mut latest = Prices::default();
    for prices in by_date.values() {
        latest.cards.extend(&prices.cards);
        latest.products.extend(&prices.products);
    }
    latest
}

/// The price history of a card: of its Cardmarket product when it has one
/// with prices, else of its card data (ygoprodeck). (date, price), oldest
/// first, and whether they are Cardmarket's.
pub async fn card_history(
    pool: &Pool,
    card_data_id: i64,
    product_id: Option<i64>,
) -> sqlx::Result<(Vec<(String, Money)>, bool)> {
    if let Some(product_id) = product_id {
        let history: Vec<(String, Money)> = sqlx::query_as(
            "SELECT date, price FROM cardmarket_price WHERE product_id = ? ORDER BY date",
        )
        .bind(product_id)
        .fetch_all(pool)
        .await?;
        if !history.is_empty() {
            return Ok((history, true));
        }
    }

    let history = sqlx::query_as(
        "SELECT date, cardmarket_price FROM card_price
         WHERE card_data_id = ? AND cardmarket_price IS NOT NULL ORDER BY date",
    )
    .bind(card_data_id)
    .fetch_all(pool)
    .await?;
    Ok((history, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    #[test]
    fn latest_prices_keep_each_ones_last_price() {
        let d = |day| NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        let prices = |cards: &[(i64, Decimal)], products: &[(i64, Decimal)]| Prices {
            cards: cards.iter().copied().collect(),
            products: products.iter().copied().collect(),
        };
        let by_date = BTreeMap::from([
            (
                d(5),
                prices(&[(1, dec!(1)), (2, dec!(2))], &[(9, dec!(40))]),
            ),
            (d(12), prices(&[(1, dec!(3))], &[])),
        ]);
        assert_eq!(
            latest_prices(&by_date),
            prices(&[(1, dec!(3)), (2, dec!(2))], &[(9, dec!(40))])
        );
    }
}
