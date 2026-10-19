//! Cardmarket's public catalogue: its daily price guide, a price for each
//! **printing** (a "product": one card in one expansion, one per version or
//! rarity), where ygoprodeck only has one price per card name.
//!
//! Two files, published by Cardmarket for download (game 3 = Yu-Gi-Oh):
//! - [`PRODUCTS_URL`]: `{"products": [{"idProduct", "name", "idExpansion",
//!   "dateAdded", …}]}`, names without set code nor rarity, and `""` for a
//!   quote (`""A"" Cell Breeding Device`);
//! - [`PRICES_URL`]: `{"createdAt", "priceGuides": [{"idProduct", "trend",
//!   "avg30", "low", …}]}`, in EUR, over every language.
//!
//! Neither file says which set code an expansion is, nor the rarity of a
//! product, so an owned copy is matched to its product in two steps:
//! 1. its **set code → expansion** ([`pick_expansion`]): the expansion that
//!    holds the most of the set's card names (from ygoprodeck), the one added
//!    nearest to the set's release among close ones (Cardmarket has twins: the
//!    OCG version of a set, a reprint);
//! 2. its **product** in that expansion ([`pick_product`]): the only one with
//!    its name, or, when the card was printed in several rarities there, the
//!    rarities ranked by rarity against the products ranked by price.
//!
//! Each step gives up (`None`) rather than guess when it's unclear; the owner
//! then picks the product on the card's edit page.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::card::clean_card_name;
use crate::rarity::Rarity;

pub const PRODUCTS_URL: &str =
    "https://downloads.s3.cardmarket.com/productCatalog/productList/products_singles_3.json";
pub const PRICES_URL: &str =
    "https://downloads.s3.cardmarket.com/productCatalog/priceGuide/price_guide_3.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub id: i64,
    /// As Cardmarket writes it, `""` turned back into `"`.
    pub name: String,
    pub expansion: i64,
    pub added: NaiveDate,
}

impl Product {
    /// The name as card data names are stored (`clean_card_name`).
    pub fn key(&self) -> String {
        clean_card_name(&self.name)
    }
}

/// A product's prices (EUR).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProductPrices {
    pub trend: Option<Decimal>,
    pub avg30: Option<Decimal>,
    pub low: Option<Decimal>,
}

impl ProductPrices {
    /// The price a copy is valued at: the trend, else the 30-day average
    /// (cards rarely sold have no trend).
    pub fn value(&self) -> Option<Decimal> {
        self.trend.or(self.avg30)
    }
}

#[derive(Deserialize)]
struct RawProducts {
    products: Vec<RawProduct>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProduct {
    id_product: i64,
    name: String,
    id_expansion: i64,
    date_added: String,
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

/// The products file. Products with a date that can't be read are skipped.
pub fn parse_products(json: &[u8]) -> Result<Vec<Product>, serde_json::Error> {
    let raw: RawProducts = serde_json::from_slice(json)?;
    Ok(raw
        .products
        .into_iter()
        .filter_map(|p| {
            // "2024-10-28 10:03:56"
            let added = NaiveDate::parse_from_str(p.date_added.get(..10)?, "%Y-%m-%d").ok()?;
            Some(Product {
                id: p.id_product,
                name: p.name.replace("\"\"", "\""),
                expansion: p.id_expansion,
                added,
            })
        })
        .collect())
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

/// What an expansion holds, for [`pick_expansion`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expansion {
    /// Its products' names ([`Product::key`]).
    pub names: HashSet<String>,
    /// When its first product was added.
    pub first_added: Option<NaiveDate>,
}

/// Every expansion of `products`.
pub fn expansions(products: &[Product]) -> HashMap<i64, Expansion> {
    let mut expansions: HashMap<i64, Expansion> = HashMap::new();
    for product in products {
        let expansion = expansions.entry(product.expansion).or_default();
        expansion.names.insert(product.key());
        expansion.first_added = Some(
            expansion
                .first_added
                .map_or(product.added, |first| first.min(product.added)),
        );
    }
    expansions
}

/// Below this share of the set's names, an expansion isn't the set.
const MIN_SHARE: f64 = 0.5;
/// Expansions within this share of the best one are told apart by date.
const CLOSE_SHARE: f64 = 0.1;
/// The nearest-dated close expansion must be nearer by this many days.
const DATE_MARGIN_DAYS: i64 = 30;

/// The expansion of a set (ygoprodeck): the one holding the largest share of
/// `set_names` (the set's card names, `clean_card_name`), at least half.
/// When others come within 10% of it (a set and its OCG version, a reprint),
/// the one whose products were added nearest to `release` wins, if it is
/// nearer by a month; otherwise `None` (as without `release` then).
pub fn pick_expansion(
    set_names: &HashSet<String>,
    expansions: &HashMap<i64, Expansion>,
    release: Option<NaiveDate>,
) -> Option<i64> {
    if set_names.is_empty() {
        return None;
    }

    let share = |expansion: &Expansion| {
        expansion.names.intersection(set_names).count() as f64 / set_names.len() as f64
    };
    let shares: Vec<(i64, f64)> = expansions
        .iter()
        .map(|(id, expansion)| (*id, share(expansion)))
        .filter(|(_, share)| *share > 0.0)
        .collect();
    let best = shares.iter().map(|(_, share)| *share).fold(0.0, f64::max);
    if best < MIN_SHARE {
        return None;
    }

    let close: Vec<i64> = shares
        .iter()
        .filter(|(_, share)| *share >= best - CLOSE_SHARE)
        .map(|(id, _)| *id)
        .collect();
    if let [only] = close.as_slice() {
        return Some(*only);
    }

    // the nearest to the release, by a clear margin
    let release = release?;
    let mut by_distance: Vec<(i64, i64)> = close
        .iter()
        .map(|id| {
            let distance = expansions[id]
                .first_added
                .map_or(i64::MAX, |added| (added - release).num_days().abs());
            (distance, *id)
        })
        .collect();
    by_distance.sort();
    let (nearest, id) = by_distance[0];
    let (second, _) = by_distance[1];
    (second.saturating_sub(nearest) >= DATE_MARGIN_DAYS).then_some(id)
}

/// The product of a copy among `products`, the products with its name in its
/// set's expansion, each with its price ([`ProductPrices::value`]):
/// - one product: that one;
/// - several: `set_rarities` (the rarities ygoprodeck lists for the copy's
///   code) must be as many, all known and of different ranks, the copy's
///   `rarity` one of them, and every product priced; the rarities sorted from
///   the most common, the products from the cheapest, the copy's rarity takes
///   the product at its place.
///
/// `None` otherwise: the owner picks it.
pub fn pick_product(
    products: &[(i64, Option<Decimal>)],
    set_rarities: &[Rarity],
    rarity: Option<Rarity>,
) -> Option<i64> {
    if let [(only, _)] = products {
        return Some(*only);
    }

    let rarity = rarity?;
    let mut rarities: Vec<Rarity> = set_rarities.to_vec();
    rarities.sort_by_key(|r| rank(*r));
    rarities.dedup();
    if products.len() < 2 || rarities.len() != products.len() || !rarities.contains(&rarity) {
        return None;
    }

    let ranks: Option<Vec<u8>> = rarities.iter().map(|r| rank(*r)).collect();
    let ranks = ranks?;
    if ranks.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }

    let mut priced: Vec<(Decimal, i64)> = products
        .iter()
        .map(|(id, price)| Some(((*price)?, *id)))
        .collect::<Option<_>>()?;
    priced.sort();
    if priced.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return None;
    }

    let place = rarities.iter().position(|r| *r == rarity)?;
    Some(priced[place].1)
}

/// How rare (and dear) a rarity usually is, for [`pick_product`]: `None` for
/// the rarities too odd to rank.
fn rank(rarity: Rarity) -> Option<u8> {
    use Rarity::*;
    Some(match rarity {
        Common => 0,
        ShortPrint | SuperShortPrint => 1,
        Rare => 2,
        SuperRare => 3,
        UltraRare | UltraRarePharaohs | GoldRare => 4,
        SecretRare | PremiumGoldRare | GoldSecretRare => 5,
        UltimateRare | PlatinumSecretRare | PrismaticSecretRare | UltraSecretRare => 6,
        GhostRare | CollectorsRare | GhostGoldRare | PlatinumRare => 7,
        QuarterCenturySecretRare => 8,
        StarlightRare => 9,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn parses_the_two_files() {
        let products = br#"{"version": 1, "createdAt": "2026-10-19T02:42:54+0200", "products": [
            {"idProduct": 101788, "name": "\"\"A\"\" Cell Breeding Device", "idCategory": 5,
             "idExpansion": 1011, "idMetacard": 101779, "dateAdded": "2007-01-01 00:00:00"},
            {"idProduct": 2, "name": "Broken", "idExpansion": 1, "dateAdded": ""}
        ]}"#;
        let products = parse_products(products).unwrap();
        assert_eq!(
            products,
            vec![Product {
                id: 101788,
                name: "\"A\" Cell Breeding Device".into(),
                expansion: 1011,
                added: date(2007, 1, 1),
            }]
        );
        assert_eq!(products[0].key(), "\"a\" cell breeding device");

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

    fn names(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn expansion(set_names: &[&str], first_added: NaiveDate) -> Expansion {
        Expansion {
            names: names(set_names),
            first_added: Some(first_added),
        }
    }

    #[test]
    fn groups_products_by_expansion() {
        let product = |id, name: &str, expansion, added| Product {
            id,
            name: name.into(),
            expansion,
            added,
        };
        let found = expansions(&[
            product(1, "Kuriboh", 7, date(2025, 2, 1)),
            product(2, "Kuriboh", 7, date(2025, 1, 15)),
            product(3, "Raigeki", 7, date(2025, 3, 1)),
            product(4, "Raigeki", 8, date(2020, 1, 1)),
        ]);
        assert_eq!(
            found[&7],
            expansion(&["kuriboh", "raigeki"], date(2025, 1, 15))
        );
        assert_eq!(found[&8].names, names(&["raigeki"]));
    }

    #[test]
    fn the_expansion_with_most_of_the_set() {
        let set = names(&["a", "b", "c", "d"]);
        let expansions = HashMap::from([
            (1, expansion(&["a", "b", "c", "d", "e"], date(2025, 1, 2))),
            // the OCG version: most of the cards, months earlier
            (2, expansion(&["a", "b", "c"], date(2024, 10, 1))),
            (3, expansion(&["a"], date(2025, 1, 2))),
        ]);
        assert_eq!(pick_expansion(&set, &expansions, None), Some(1));
        assert_eq!(pick_expansion(&HashSet::new(), &expansions, None), None);
        // under half the set: not it
        assert_eq!(
            pick_expansion(&names(&["a", "x", "y"]), &expansions, None),
            None
        );
    }

    #[test]
    fn twin_expansions_are_told_apart_by_date() {
        let set = names(&["a", "b"]);
        let release = Some(date(2025, 2, 13));
        let mut expansions = HashMap::from([
            (1, expansion(&["a", "b"], date(2025, 2, 4))),
            (2, expansion(&["a", "b"], date(2024, 9, 9))),
        ]);
        assert_eq!(pick_expansion(&set, &expansions, release), Some(1));
        // without a release date, or too close to each other: unknown
        assert_eq!(pick_expansion(&set, &expansions, None), None);
        expansions.insert(2, expansion(&["a", "b"], date(2025, 2, 24)));
        assert_eq!(pick_expansion(&set, &expansions, release), None);
    }

    #[test]
    fn a_single_product_is_the_one() {
        assert_eq!(pick_product(&[(5, None)], &[], None), Some(5));
        assert_eq!(
            pick_product(&[], &[Rarity::Common], Some(Rarity::Common)),
            None
        );
    }

    #[test]
    fn rarities_take_the_products_by_price() {
        let products = [(10, Some(dec!(48.18))), (11, Some(dec!(7.81)))];
        let rarities = [Rarity::QuarterCenturySecretRare, Rarity::SecretRare];
        assert_eq!(
            pick_product(&products, &rarities, Some(Rarity::QuarterCenturySecretRare)),
            Some(10)
        );
        assert_eq!(
            pick_product(&products, &rarities, Some(Rarity::SecretRare)),
            Some(11)
        );
    }

    #[test]
    fn unclear_products_are_left_to_the_owner() {
        let two = [(10, Some(dec!(5))), (11, Some(dec!(1)))];
        let rarities = [Rarity::UltraRare, Rarity::SecretRare];
        // the copy's rarity unknown, or not printed there
        assert_eq!(pick_product(&two, &rarities, None), None);
        assert_eq!(pick_product(&two, &rarities, Some(Rarity::Common)), None);
        // as many rarities as products needed
        assert_eq!(
            pick_product(&two, &[Rarity::UltraRare], Some(Rarity::UltraRare)),
            None
        );
        // two rarities of the same rank
        assert_eq!(
            pick_product(
                &two,
                &[Rarity::UltraRare, Rarity::GoldRare],
                Some(Rarity::UltraRare)
            ),
            None
        );
        // a product without a price, or two at the same price
        let unpriced = [(10, Some(dec!(5))), (11, None)];
        assert_eq!(
            pick_product(&unpriced, &rarities, Some(Rarity::UltraRare)),
            None
        );
        let same = [(10, Some(dec!(5))), (11, Some(dec!(5)))];
        assert_eq!(
            pick_product(&same, &rarities, Some(Rarity::UltraRare)),
            None
        );
    }
}
