//! Cardmarket's price guide, the same file for every game: one price per
//! product (a card printing, a booster…) over every language, in EUR.
//! `{"createdAt", "priceGuides": [{"idProduct", "trend", "avg30", "low", …}]}`.
//! Its products files differ by game: each collection reads its own.

use std::collections::HashMap;

use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use serde::Deserialize;

/// The price guide's address for a Cardmarket game (3 = Yu-Gi-Oh!, 6 = Pokémon).
pub fn price_guide_url(game: u32) -> String {
    format!("https://downloads.s3.cardmarket.com/productCatalog/priceGuide/price_guide_{game}.json")
}

/// A products list's address: `kind` is `singles` or `nonsingles`.
pub fn products_url(kind: &str, game: u32) -> String {
    format!("https://downloads.s3.cardmarket.com/productCatalog/productList/products_{kind}_{game}.json")
}

/// A product's prices (EUR).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProductPrices {
    pub trend: Option<Decimal>,
    pub avg30: Option<Decimal>,
    pub low: Option<Decimal>,
}

impl ProductPrices {
    /// The price a product is valued at: the trend, else the 30-day average
    /// (products rarely sold have no trend).
    pub fn value(&self) -> Option<Decimal> {
        self.trend.or(self.avg30)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPrices {
    price_guides: Vec<RawPrice>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPrice {
    id_product: i64,
    trend: Option<f64>,
    avg30: Option<f64>,
    low: Option<f64>,
}

/// The price guide file, by product id. Zero or negative prices count as none.
pub fn parse_prices(json: &[u8]) -> Result<HashMap<i64, ProductPrices>, serde_json::Error> {
    let raw: RawPrices = serde_json::from_slice(json)?;
    let euros = |price: Option<f64>| {
        Decimal::from_f64(price?)
            .map(|d| d.round_dp(2))
            .filter(|d| *d > Decimal::ZERO)
    };
    Ok(raw
        .price_guides
        .into_iter()
        .map(|p| {
            let prices = ProductPrices {
                trend: euros(p.trend),
                avg30: euros(p.avg30),
                low: euros(p.low),
            };
            (p.id_product, prices)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parses_the_price_guide() {
        let prices = br#"{"version": 1, "createdAt": "2026-10-19T02:42:54+0200", "priceGuides": [
            {"idProduct": 1, "idCategory": 5, "avg": 0.08, "low": 0.02, "trend": 0.08, "avg30": 0.12},
            {"idProduct": 2, "low": 38.99, "trend": null, "avg30": null},
            {"idProduct": 3, "low": 0, "trend": 12.346, "avg30": 13.71}
        ]}"#;
        let prices = parse_prices(prices).unwrap();
        assert_eq!(prices[&1].value(), Some(dec!(0.08)));
        assert_eq!(prices[&2].value(), None);
        assert_eq!(prices[&2].low, Some(dec!(38.99)));
        assert_eq!(prices[&3].trend, Some(dec!(12.35)));
        assert_eq!(prices[&3].low, None);
    }
}
