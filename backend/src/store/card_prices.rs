//! Market prices of the cards (`card_price`, migration 0008), refreshed from
//! ygoprodeck weekly, and the owned copies they value (`domain::valuation`).
//!
//! ygoprodeck gives one Cardmarket price (EUR) per card name, the cheapest
//! printing's. The `card_prices` copy in `card_data.json_data` is never used:
//! it dates from when the card was linked.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use domain::card::{CardStatus, Language};
use domain::valuation::{Copy, Lot, Prices};
use rust_decimal::Decimal;
use serde_json::Value;

use crate::store::card_data::{throttle, API_URL};
use crate::store::db::{begin_write, Money, Pool};
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
    if dry_run {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }

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

/// The date of the last refresh, `None` before the first one.
pub async fn latest_refresh(pool: &Pool) -> sqlx::Result<Option<NaiveDate>> {
    let date: Option<String> = sqlx::query_scalar("SELECT max(date) FROM card_price")
        .fetch_one(pool)
        .await?;
    Ok(date.and_then(|d| d.parse().ok()))
}

/// Every price, by refresh date: ygoprodeck's by card data, Cardmarket's
/// by product (`store::cardmarket`).
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

/// An owned (or once owned) copy and the card it is.
#[derive(Debug, Clone)]
pub struct OwnedCopy {
    pub card_id: i64,
    /// Its purchase lot.
    pub lot_id: i64,
    pub name: String,
    pub code: Option<String>,
    pub copy: Copy,
}

#[derive(sqlx::FromRow)]
struct CopyRow {
    card_id: i64,
    lot_id: i64,
    name: String,
    code: Option<String>,
    card_data_id: Option<i64>,
    cardmarket_product_id: Option<i64>,
    status: Option<String>,
    bought: String,
    paid: Option<Money>,
    sold: Option<String>,
    sold_for: Option<Money>,
}

/// Every non-proxy card bought in a purchase lot that isn't cancelled, sold
/// or not.
pub async fn owned_copies(pool: &Pool) -> sqlx::Result<Vec<OwnedCopy>> {
    let rows: Vec<CopyRow> = sqlx::query_as(
        "SELECT card.id AS card_id, card.name, card.code, card.card_data_id,
                card.cardmarket_product_id, card.last_known_status AS status, lot.buy_date AS bought, unite.price AS paid,
                unite.lot_id,
                (SELECT min(sale.buy_date) FROM unite AS sold_unite
                   JOIN lot AS sale ON sale.id = sold_unite.lot_id
                  WHERE sold_unite.card_id = card.id AND sale.lot_type = 'sale'
                    AND sale.is_cancelled = 0) AS sold,
                (SELECT sold_unite.price FROM unite AS sold_unite
                   JOIN lot AS sale ON sale.id = sold_unite.lot_id
                  WHERE sold_unite.card_id = card.id AND sale.lot_type = 'sale'
                    AND sale.is_cancelled = 0
                  ORDER BY sale.buy_date LIMIT 1) AS sold_for
         FROM unite
         JOIN lot ON lot.id = unite.lot_id
         JOIN card ON card.id = unite.card_id
         WHERE lot.lot_type = 'purchase' AND lot.is_cancelled = 0 AND card.is_proxy = 0
         ORDER BY card.id",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let copy = Copy {
                card_data_id: row.card_data_id,
                cardmarket_product_id: row.cardmarket_product_id,
                status: row.status.as_deref().and_then(CardStatus::from_code),
                language: row.code.as_deref().and_then(Language::of_code),
                bought: row.bought.parse().ok()?,
                sold: row.sold.as_deref().and_then(|d| d.parse().ok()),
                paid: row.paid.map(|p| p.0),
                sold_for: row.sold_for.map(|p| p.0),
            };
            Some(OwnedCopy {
                card_id: row.card_id,
                lot_id: row.lot_id,
                name: row.name,
                code: row.code,
                copy,
            })
        })
        .collect())
}

/// A purchase lot that isn't cancelled, for the gain by lot.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PurchaseLot {
    pub id: i64,
    pub date: String,
    pub price: Money,
    pub seller: Option<String>,
}

/// The purchase lots that aren't cancelled, newest first.
pub async fn purchase_lots(pool: &Pool) -> sqlx::Result<Vec<PurchaseLot>> {
    sqlx::query_as(
        "SELECT lot.id, lot.buy_date AS date, lot.price, seller.name AS seller
         FROM lot LEFT JOIN seller ON seller.id = lot.seller_id
         WHERE lot.lot_type = 'purchase' AND lot.is_cancelled = 0
         ORDER BY lot.buy_date DESC, lot.id DESC",
    )
    .fetch_all(pool)
    .await
}

/// The lots that aren't cancelled: their date, kind and price.
pub async fn lots(pool: &Pool) -> sqlx::Result<Vec<Lot>> {
    let rows: Vec<(String, String, Money)> =
        sqlx::query_as("SELECT buy_date, lot_type, price FROM lot WHERE is_cancelled = 0")
            .fetch_all(pool)
            .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(date, lot_type, price)| {
            Some(Lot {
                date: date.parse().ok()?,
                is_sale: lot_type == "sale",
                price: price.0,
            })
        })
        .collect())
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
