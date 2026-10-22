//! ygoprodeck's market prices (`card_price`, migration 0008), refreshed
//! weekly: one Cardmarket price (EUR) per card **name**, the cheapest
//! printing's, and TCGplayer's (USD, kept for later). The `card_prices` copy
//! in `card_data.json_data` is never used: it dates from when the card was
//! linked.

use std::collections::HashMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::Value;

use crate::store::card_data::{throttle, API_URL};
use crate::store::db::{begin_write, commit_unless_dry_run, Money, Pool};
use crate::web::error::{AppError, AppResult};

/// Ids per `cardinfo.php?id=…` request.
const BATCH_SIZE: usize = 100;

/// The prices of one card on ygoprodeck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YgoprodeckPrices {
    pub cardmarket: Option<Decimal>,
    pub tcgplayer: Option<Decimal>,
}

/// What a refresh did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub date: Option<NaiveDate>,
    /// Card data asked for.
    pub asked: usize,
    /// Rows written (with or without a price).
    pub written: usize,
    /// Rows without a Cardmarket price.
    pub without_price: usize,
    /// Ids ygoprodeck didn't return (removed or renamed cards).
    pub missing: usize,
}

impl Report {
    pub fn log(&self) {
        tracing::info!(
            "price refresh: {} cards asked, {} prices written ({} without a Cardmarket price), \
             {} unknown to ygoprodeck",
            self.asked,
            self.written,
            self.without_price,
            self.missing
        );
    }
}

/// Fetch today's prices of every card data with a ygopro id and no price row
/// today, and write them in one transaction (rolled back with `dry_run`).
/// Requests come first: the write transaction is opened after them.
pub async fn refresh_prices(
    pool: &Pool,
    http: &reqwest::Client,
    today: NaiveDate,
    dry_run: bool,
) -> AppResult<Report> {
    let cards: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT id, ygopro_id FROM card_data
         WHERE ygopro_id IS NOT NULL
           AND id NOT IN (SELECT card_data_id FROM card_price WHERE date = ?)
         ORDER BY id",
    )
    .bind(today.to_string())
    .fetch_all(pool)
    .await?;

    let mut found: HashMap<i64, YgoprodeckPrices> = HashMap::new();
    for batch in cards.chunks(BATCH_SIZE) {
        let ids: Vec<i64> = batch.iter().map(|(_, ygopro_id)| *ygopro_id).collect();
        found.extend(parse_prices(&fetch_batch(http, &ids).await?));
    }

    let mut report = Report {
        date: Some(today),
        asked: cards.len(),
        ..Report::default()
    };
    let mut tx = begin_write(pool).await?;
    for (card_data_id, ygopro_id) in &cards {
        let Some(prices) = found.get(ygopro_id) else {
            report.missing += 1;
            continue;
        };

        sqlx::query(
            "INSERT OR REPLACE INTO card_price (card_data_id, date, cardmarket_price, tcgplayer_price)
             VALUES (?, ?, ?, ?)",
        )
        .bind(card_data_id)
        .bind(today.to_string())
        .bind(prices.cardmarket.map(Money))
        .bind(prices.tcgplayer.map(Money))
        .execute(&mut *tx)
        .await?;
        report.written += 1;
        report.without_price += usize::from(prices.cardmarket.is_none());
    }
    commit_unless_dry_run(tx, dry_run).await?;

    Ok(report)
}

/// `cardinfo.php?id=1,2,3`: the cards ygoprodeck knows among `ids` (the
/// unknown ones are left out; an answer 400 means none is known).
async fn fetch_batch(http: &reqwest::Client, ids: &[i64]) -> AppResult<Value> {
    let internal = |e: reqwest::Error| AppError::Internal(format!("ygoprodeck prices: {e}"));
    let ids: Vec<String> = ids.iter().map(i64::to_string).collect();

    throttle().await;
    let response = http
        .get(API_URL)
        .query(&[("id", ids.join(","))])
        .send()
        .await
        .map_err(internal)?;
    if response.status() == reqwest::StatusCode::BAD_REQUEST {
        return Ok(Value::Null);
    }

    response
        .error_for_status()
        .map_err(internal)?
        .json()
        .await
        .map_err(internal)
}

/// The prices in a `cardinfo.php` answer, by ygopro id. A card is also
/// listed under its other artworks' ids (`card_images[].id`), in case a
/// card data row has one of them.
pub fn parse_prices(body: &Value) -> HashMap<i64, YgoprodeckPrices> {
    let mut prices = HashMap::new();
    let cards = body["data"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();

    for card in cards {
        let card_prices = &card["card_prices"][0];
        let found = YgoprodeckPrices {
            cardmarket: price(&card_prices["cardmarket_price"]),
            tcgplayer: price(&card_prices["tcgplayer_price"]),
        };
        let artworks = card["card_images"].as_array().into_iter().flatten();
        let ids = std::iter::once(&card["id"]).chain(artworks.map(|image| &image["id"]));
        for id in ids.filter_map(Value::as_i64) {
            prices.insert(id, found);
        }
    }

    prices
}

/// A ygoprodeck price (`"0.25"`); `"0"`, missing or invalid: no price.
fn price(value: &Value) -> Option<Decimal> {
    let price: Decimal = value.as_str()?.trim().parse().ok()?;
    (price > Decimal::ZERO).then_some(price)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use serde_json::json;

    #[test]
    fn parses_prices_by_id_and_artwork() {
        let body = json!({"data": [
            {
                "id": 46986414,
                "card_images": [{"id": 46986414}, {"id": 46986415}],
                "card_prices": [{"cardmarket_price": "0.25", "tcgplayer_price": "0.54"}]
            },
            {
                "id": 40640057,
                "card_images": [],
                "card_prices": [{"cardmarket_price": "0", "tcgplayer_price": "1.10"}]
            },
            {"id": 1, "card_prices": []}
        ]});
        let prices = parse_prices(&body);

        let dark_magician = YgoprodeckPrices {
            cardmarket: Some(dec!(0.25)),
            tcgplayer: Some(dec!(0.54)),
        };
        assert_eq!(prices[&46986414], dark_magician);
        assert_eq!(prices[&46986415], dark_magician);
        assert_eq!(
            prices[&40640057],
            YgoprodeckPrices {
                cardmarket: None,
                tcgplayer: Some(dec!(1.10)),
            }
        );
        assert_eq!(
            prices[&1],
            YgoprodeckPrices {
                cardmarket: None,
                tcgplayer: None,
            }
        );
        assert!(parse_prices(&Value::Null).is_empty());
    }
}
