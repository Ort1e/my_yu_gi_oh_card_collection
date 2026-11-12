//! The boosters suggested by the wishlist: which booster to open for the
//! wished cards, from Cardmarket's catalogue (game [`GAME`]).
//!
//! A wished card's TCGdex data names its Cardmarket product
//! (`pricing.cardmarket.idProduct`); the singles list gives that product's
//! expansion, the non-singles list the expansion's booster
//! ([`pick_booster`]), the price guide (`common_domain::cardmarket`) its
//! price: one price over every language (the files don't split them).
//! A Trainer Gallery card's expansion is its main set's, so it counts for
//! that set's booster; promos have no booster and count for none.
//!
//! [`suggest`] gives the boosters with the most wished cards, and the
//! cheapest boosters with at least one.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::Deserialize;

/// Cardmarket's number for the Pokémon game.
pub const GAME: u32 = 6;

/// The non-singles' category of the boosters.
const BOOSTER_CATEGORY: &str = "Pokémon Booster";

/// How many boosters each list suggests.
pub const SUGGESTED: usize = 3;

/// A product of Cardmarket's lists (singles or non-singles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub category: String,
    pub expansion: i64,
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
    #[serde(default)]
    category_name: String,
    id_expansion: i64,
}

/// A products file: `{"products": [{"idProduct", "name", "categoryName",
/// "idExpansion", …}]}`.
pub fn parse_products(json: &[u8]) -> Result<Vec<Product>, serde_json::Error> {
    let raw: RawProducts = serde_json::from_slice(json)?;
    Ok(raw
        .products
        .into_iter()
        .map(|p| Product {
            id: p.id_product,
            name: p.name,
            category: p.category_name,
            expansion: p.id_expansion,
        })
        .collect())
}

/// An expansion's booster among its non-single products: the plain one
/// (`Surging Sparks Booster`), not the sleeved one, the 6- or 3-card packs
/// or a special series (`First Partner Illustration Booster Series 1`).
pub fn pick_booster<'a>(products: impl IntoIterator<Item = &'a Product>) -> Option<&'a Product> {
    products
        .into_iter()
        .filter(|p| p.category == BOOSTER_CATEGORY)
        .filter(|p| p.name.ends_with(" Booster") && !p.name.contains("Sleeved"))
        .min_by(|a, b| a.name.len().cmp(&b.name.len()).then(a.id.cmp(&b.id)))
}

/// A booster and its price (`pokemon_booster`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Booster {
    pub expansion: i64,
    pub name: String,
    pub price: Option<Decimal>,
}

/// A suggested booster: how many wished cards it may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suggestion<'a> {
    pub booster: &'a Booster,
    pub wished: usize,
}

/// The two lists of suggestions.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Suggestions<'a> {
    /// The boosters with the most wished cards (ties: the cheaper first).
    pub most_wished: Vec<Suggestion<'a>>,
    /// The cheapest boosters with at least one wished card and a price
    /// (ties: the one with more wished cards first).
    pub cheapest: Vec<Suggestion<'a>>,
}

/// The suggestions for the wished cards' expansions (one entry per wish,
/// `None` when not known), [`SUGGESTED`] boosters in each list.
pub fn suggest<'a>(
    wished: impl IntoIterator<Item = Option<i64>>,
    boosters: &'a HashMap<i64, Booster>,
) -> Suggestions<'a> {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for expansion in wished.into_iter().flatten() {
        if boosters.contains_key(&expansion) {
            *counts.entry(expansion).or_default() += 1;
        }
    }
    let all: Vec<Suggestion<'a>> = counts
        .into_iter()
        .filter_map(|(expansion, wished)| {
            Some(Suggestion {
                booster: boosters.get(&expansion)?,
                wished,
            })
        })
        .collect();
    // No price sorts after every price.
    let price = |s: &Suggestion| (s.booster.price.is_none(), s.booster.price);
    let name = |s: &Suggestion<'a>| s.booster.name.as_str();

    let mut most_wished = all.clone();
    most_wished.sort_by(|a, b| {
        b.wished
            .cmp(&a.wished)
            .then(price(a).cmp(&price(b)))
            .then(name(a).cmp(name(b)))
    });
    most_wished.truncate(SUGGESTED);

    let mut cheapest: Vec<Suggestion<'a>> = all
        .into_iter()
        .filter(|s| s.booster.price.is_some())
        .collect();
    cheapest.sort_by(|a, b| {
        price(a)
            .cmp(&price(b))
            .then(b.wished.cmp(&a.wished))
            .then(name(a).cmp(name(b)))
    });
    cheapest.truncate(SUGGESTED);

    Suggestions {
        most_wished,
        cheapest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn product(id: i64, name: &str, category: &str) -> Product {
        Product {
            id,
            name: name.into(),
            category: category.into(),
            expansion: 1,
        }
    }

    #[test]
    fn parses_a_products_file() {
        let json = r#"{"version": 1, "createdAt": "2026-11-12T02:00:00+0100", "products": [
            {"idProduct": 7, "name": "Surging Sparks Booster", "idCategory": 52,
             "categoryName": "Pokémon Booster", "idExpansion": 5879, "idMetacard": 0,
             "dateAdded": "2024-09-01 10:00:00"},
            {"idProduct": 8, "name": "Pikachu ex", "idExpansion": 5879}
        ]}"#;
        let products = parse_products(json.as_bytes()).unwrap();
        assert_eq!(products[0].category, "Pokémon Booster");
        assert_eq!(products[1].category, "");
        assert_eq!(products[1].expansion, 5879);
    }

    #[test]
    fn the_plain_booster_is_picked() {
        let products = [
            product(1, "Surging Sparks Sleeved Booster", BOOSTER_CATEGORY),
            product(2, "Surging Sparks Booster (6 Cards)", BOOSTER_CATEGORY),
            product(3, "Surging Sparks Fun Pack (3 Cards)", BOOSTER_CATEGORY),
            product(4, "Surging Sparks Booster Display", "Pokémon Display"),
            product(5, "Surging Sparks Booster", BOOSTER_CATEGORY),
        ];
        assert_eq!(pick_booster(&products).map(|p| p.id), Some(5));
    }

    #[test]
    fn a_special_series_is_no_booster() {
        let products = [product(
            1,
            "First Partner Illustration Booster Series 1",
            BOOSTER_CATEGORY,
        )];
        assert_eq!(pick_booster(&products), None);
    }

    fn boosters(list: &[(i64, &str, Option<Decimal>)]) -> HashMap<i64, Booster> {
        list.iter()
            .map(|&(expansion, name, price)| {
                let booster = Booster {
                    expansion,
                    name: name.into(),
                    price,
                };
                (expansion, booster)
            })
            .collect()
    }

    fn names(list: &[Suggestion]) -> Vec<(String, usize)> {
        list.iter()
            .map(|s| (s.booster.name.clone(), s.wished))
            .collect()
    }

    #[test]
    fn suggests_the_most_wished_and_the_cheapest() {
        let boosters = boosters(&[
            (1, "A", Some(dec!(6.37))),
            (2, "B", Some(dec!(23.52))),
            (3, "C", Some(dec!(5.16))),
            (4, "D", None),
            (5, "E", Some(dec!(11.84))),
            (6, "Unwished", Some(dec!(1))),
        ]);
        let wished = [
            Some(2),
            Some(2),
            Some(2),
            Some(4),
            Some(4),
            Some(1),
            Some(3),
            Some(5),
            None,
            Some(99), // a promo: no booster
        ];
        let suggestions = suggest(wished, &boosters);
        // A, C and E have one each: the cheaper first.
        assert_eq!(
            names(&suggestions.most_wished),
            [("B".into(), 3), ("D".into(), 2), ("C".into(), 1)]
        );
        // D has no price.
        assert_eq!(
            names(&suggestions.cheapest),
            [("C".into(), 1), ("A".into(), 1), ("E".into(), 1)]
        );
    }

    #[test]
    fn nothing_wished_nothing_suggested() {
        let boosters = boosters(&[(1, "A", Some(dec!(6)))]);
        assert_eq!(suggest([None], &boosters), Suggestions::default());
    }
}
