//! What `yugioh_domain::valuation` values, read from the collection: the owned
//! copies, the lots, and the estimates of the list pages (a card's latest
//! market price, `MARKET_PRICE`, with its condition and language).

use std::collections::HashMap;

use rust_decimal::Decimal;
use sqlx::QueryBuilder;
use yugioh_domain::card::{CardStatus, Language};
use yugioh_domain::valuation::{copy_value, Copy, Lot};

use crate::store::cards::SOLD_EXISTS;
use crate::store::db::{Money, Pool};
use crate::store::models::LotRow;

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
        "WITH sale AS (
             -- each card's first sale (a card is sold once, but who knows)
             SELECT unite.card_id, lot.buy_date, unite.price,
                    row_number() OVER (PARTITION BY unite.card_id ORDER BY lot.buy_date) AS nth
             FROM unite JOIN lot ON lot.id = unite.lot_id
             WHERE lot.lot_type = 'sale' AND lot.is_cancelled = 0
         )
         SELECT card.id AS card_id, unite.lot_id, card.name, card.code, card.card_data_id,
                card.cardmarket_product_id, card.last_known_status AS status,
                lot.buy_date AS bought, unite.price AS paid,
                sale.buy_date AS sold, sale.price AS sold_for
         FROM unite
         JOIN lot ON lot.id = unite.lot_id
         JOIN card ON card.id = unite.card_id
         LEFT JOIN sale ON sale.card_id = card.id AND sale.nth = 1
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

/// SQL expression on `card`: its latest market price (TEXT, Money), its
/// Cardmarket product's when it has one, else its card name's (ygoprodeck);
/// NULL without any. Like `latest_prices`, for the list pages.
pub const MARKET_PRICE: &str = "coalesce(
    (SELECT price FROM cardmarket_price
      WHERE product_id = card.cardmarket_product_id ORDER BY date DESC LIMIT 1),
    (SELECT cardmarket_price FROM card_price
      WHERE card_data_id = card.card_data_id AND cardmarket_price IS NOT NULL
      ORDER BY date DESC LIMIT 1))";

/// A copy's estimated value from its [`MARKET_PRICE`], condition
/// (`last_known_status`) and code (its language), rounded to the cent.
pub fn estimate(
    market_price: Option<Money>,
    status: Option<&str>,
    code: Option<&str>,
) -> Option<Money> {
    let value = copy_value(
        market_price?.0,
        status.and_then(CardStatus::from_code),
        code.and_then(Language::of_code),
    );
    Some(Money(value.round_dp(2)))
}

/// A card still owned of a lot, for [`fill_lot_estimates`].
#[derive(sqlx::FromRow)]
struct LotCard {
    lot_id: i64,
    status: Option<String>,
    code: Option<String>,
    market_price: Option<Money>,
}

/// Fill `estimate` of the purchase lots of `lots`: the estimated value of
/// their cards still owned (proxies and sold cards aside), `None` when none
/// of them has a market price.
pub async fn fill_lot_estimates(pool: &Pool, lots: &mut [LotRow]) -> sqlx::Result<()> {
    let ids: Vec<i64> = lots
        .iter()
        .filter(|lot| lot.lot_type == "purchase")
        .map(|lot| lot.id)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }

    let mut query = QueryBuilder::new(format!(
        "SELECT unite.lot_id, card.last_known_status AS status, card.code,
                {MARKET_PRICE} AS market_price
         FROM unite JOIN card ON card.id = unite.card_id
         WHERE card.is_proxy = 0 AND NOT {SOLD_EXISTS} AND unite.lot_id IN ("
    ));
    let mut ids_list = query.separated(", ");
    for id in ids {
        ids_list.push_bind(id);
    }
    query.push(")");
    let cards: Vec<LotCard> = query.build_query_as().fetch_all(pool).await?;

    let mut totals: HashMap<i64, Decimal> = HashMap::new();
    for card in cards {
        let value = estimate(
            card.market_price,
            card.status.as_deref(),
            card.code.as_deref(),
        );
        if let Some(value) = value {
            *totals.entry(card.lot_id).or_default() += value.0;
        }
    }
    for lot in lots {
        lot.estimate = totals.get(&lot.id).map(|total| Money(*total));
    }
    Ok(())
}

/// A purchase lot that isn't cancelled, for the gain by lot.
#[derive(Debug, Clone)]
pub struct PurchaseLot {
    pub id: i64,
    pub date: String,
    /// What it cost: the price paid less the refund.
    pub price: Money,
    pub seller: Option<String>,
}

/// The purchase lots that aren't cancelled, newest first.
pub async fn purchase_lots(pool: &Pool) -> sqlx::Result<Vec<PurchaseLot>> {
    let rows: Vec<(i64, String, Money, Money, Option<String>)> = sqlx::query_as(
        "SELECT lot.id, lot.buy_date, lot.price, lot.refund, seller.name
         FROM lot LEFT JOIN seller ON seller.id = lot.seller_id
         WHERE lot.lot_type = 'purchase' AND lot.is_cancelled = 0
         ORDER BY lot.buy_date DESC, lot.id DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, date, price, refund, seller)| PurchaseLot {
            id,
            date,
            price: Money(price.0 - refund.0),
            seller,
        })
        .collect())
}

/// The lots that aren't cancelled: their date, kind and cost (the price
/// less the refund, on the lot's date).
pub async fn lots(pool: &Pool) -> sqlx::Result<Vec<Lot>> {
    let rows: Vec<(String, String, Money, Money)> =
        sqlx::query_as("SELECT buy_date, lot_type, price, refund FROM lot WHERE is_cancelled = 0")
            .fetch_all(pool)
            .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(date, lot_type, price, refund)| {
            Some(Lot {
                date: date.parse().ok()?,
                is_sale: lot_type == "sale",
                price: price.0 - refund.0,
            })
        })
        .collect())
}
