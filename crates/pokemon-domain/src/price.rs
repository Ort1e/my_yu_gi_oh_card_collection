//! A Pokémon copy's market price and value, from the Cardmarket numbers
//! TCGdex gives with a card (EUR, updated daily).
//!
//! TCGdex gives them per variant in `variants_detailed[].pricing.cardmarket`
//! (newer cards), else once for the card in `pricing.cardmarket`. Cardmarket
//! sells a card's normal and reverse holo prints as **one product**: the
//! plain fields (`trend`, `avg30`) are the normal print's, the `-holo` ones
//! (`trend-holo`, `avg30-holo`) the **reverse holo**'s. A card printed holo
//! only (illustration rares, Japanese holos) has its price in the plain
//! fields; a "cosmos holo" of a card also printed normal is its own product
//! (its own `variants_detailed` entry).
//!
//! A price is the trend, else the 30-day average (as for the Yu-Gi-Oh!
//! cards); `0` and `null` mean none.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde_json::Value;

use super::Variant;
use common_domain::condition_factor;
use common_domain::CardStatus;

/// Today's price of each variant TCGdex prices, from a card's JSON.
pub fn variant_prices(card: &Value) -> BTreeMap<Variant, Decimal> {
    let mut prices = BTreeMap::new();

    if let Some(details) = card.get("variants_detailed").and_then(Value::as_array) {
        for detail in details {
            let Some(variant) = detail["type"]
                .as_str()
                .and_then(Variant::from_detailed_type)
            else {
                continue;
            };
            if let Some(price) = price_in(&detail["pricing"]["cardmarket"], variant) {
                prices.entry(variant).or_insert(price);
            }
        }
    }

    // the card's own prices, for the variants not priced above
    let printed: Vec<Variant> = match card.get("variants").and_then(Value::as_object) {
        Some(flags) => Variant::ALL
            .into_iter()
            .filter(|v| flags.get(v.code()).and_then(Value::as_bool) == Some(true))
            .collect(),
        None => vec![Variant::Normal],
    };
    let has_normal = printed.contains(&Variant::Normal);
    for variant in printed {
        if prices.contains_key(&variant) {
            continue;
        }
        // a holo of a card also printed normal is another product
        if variant == Variant::Holo && has_normal {
            continue;
        }
        if let Some(price) = price_in(&card["pricing"]["cardmarket"], variant) {
            prices.insert(variant, price);
        }
    }

    prices
}

/// The price of `variant` in a `cardmarket` object.
fn price_in(cardmarket: &Value, variant: Variant) -> Option<Decimal> {
    if !cardmarket.is_object() {
        return None;
    }

    let suffix = match variant {
        // the reverse holo is the `-holo` half of the normal print's product
        Variant::Reverse => "-holo",
        _ => "",
    };
    ["trend", "avg30"]
        .iter()
        .find_map(|field| amount(&cardmarket[format!("{field}{suffix}")]))
}

/// A positive amount, rounded to the cent.
fn amount(value: &Value) -> Option<Decimal> {
    let number = value.as_number()?;
    let decimal: Decimal = number.to_string().parse().ok()?;
    (decimal > Decimal::ZERO).then(|| decimal.round_dp(2))
}

/// The stored form of [`variant_prices`]: `{"normal": "0.03", …}`.
pub fn to_json(prices: &BTreeMap<Variant, Decimal>) -> String {
    let map: serde_json::Map<String, Value> = prices
        .iter()
        .map(|(v, p)| (v.code().to_string(), Value::String(p.to_string())))
        .collect();
    Value::Object(map).to_string()
}

/// Back from [`to_json`]; unknown keys are skipped.
pub fn from_json(text: &str) -> BTreeMap<Variant, Decimal> {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
        return BTreeMap::new();
    };
    map.iter()
        .filter_map(|(k, v)| Some((Variant::from_code(k)?, v.as_str()?.parse().ok()?)))
        .collect()
}

/// What one copy is worth, not rounded: a graded copy's `estimate` when
/// noted (Cardmarket has no graded prices), else its variant's market price
/// × the condition factor of the Yu-Gi-Oh! cards (EX 0.8, G 0.6, P 0.4).
/// No language factor: TCGdex has one price for all the languages of a
/// catalogue. `None` when there is no price for its variant.
pub fn copy_value(
    variant: Variant,
    condition: Option<CardStatus>,
    estimate: Option<Decimal>,
    prices: &BTreeMap<Variant, Decimal>,
) -> Option<Decimal> {
    if estimate.is_some() {
        return estimate;
    }
    prices
        .get(&variant)
        .map(|price| price * condition_factor(condition))
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;
    use serde_json::json;

    use super::*;

    fn cardmarket(trend: f64, avg30: f64, trend_holo: f64, avg30_holo: Option<f64>) -> Value {
        json!({"unit": "EUR", "trend": trend, "avg30": avg30,
               "trend-holo": trend_holo, "avg30-holo": avg30_holo})
    }

    #[test]
    fn reverse_holos_are_the_holo_half_of_the_normal_product() {
        let normal_and_reverse = cardmarket(0.03, 0.04, 0.1, Some(0.13));
        let card = json!({
            "variants": {"normal": true, "reverse": true, "holo": true},
            "variants_detailed": [
                {"type": "Normal", "pricing": {"cardmarket": normal_and_reverse}},
                {"type": "Holo", "pricing": {"cardmarket": cardmarket(0.17, 0.25, 0.0, None)}},
                {"type": "Reverse", "pricing": {"cardmarket": normal_and_reverse}},
            ],
            "pricing": {"cardmarket": normal_and_reverse}
        });
        let prices = variant_prices(&card);
        assert_eq!(prices[&Variant::Normal], dec!(0.03));
        assert_eq!(prices[&Variant::Holo], dec!(0.17));
        assert_eq!(prices[&Variant::Reverse], dec!(0.10));
    }

    #[test]
    fn holo_only_cards_use_the_plain_fields() {
        let card = json!({
            "variants": {"normal": false, "holo": true, "reverse": false},
            "pricing": {"cardmarket": cardmarket(34.43, 31.61, 0.0, None)}
        });
        assert_eq!(
            variant_prices(&card),
            BTreeMap::from([(Variant::Holo, dec!(34.43))])
        );
    }

    #[test]
    fn falls_back_on_the_card_prices_and_the_average() {
        // no details; the trend of the reverse is 0: its 30-day average
        let card = json!({
            "variants": {"normal": true, "reverse": true, "holo": true},
            "pricing": {"cardmarket": cardmarket(0.0, 0.05, 0.0, Some(0.2))}
        });
        let prices = variant_prices(&card);
        assert_eq!(prices[&Variant::Normal], dec!(0.05));
        assert_eq!(prices[&Variant::Reverse], dec!(0.2));
        assert!(
            !prices.contains_key(&Variant::Holo),
            "a holo of a normal card is another product"
        );

        assert!(variant_prices(&json!({"variants": {"normal": true}})).is_empty());
        assert!(variant_prices(&json!({"pricing": {"cardmarket": null}})).is_empty());
    }

    #[test]
    fn stored_as_json() {
        let prices = BTreeMap::from([(Variant::Normal, dec!(0.03)), (Variant::Reverse, dec!(0.1))]);
        let text = to_json(&prices);
        assert_eq!(text, r#"{"normal":"0.03","reverse":"0.1"}"#);
        assert_eq!(from_json(&text), prices);
        assert!(from_json("nonsense").is_empty());
    }

    #[test]
    fn values_copies() {
        let prices = BTreeMap::from([(Variant::Holo, dec!(30))]);
        assert_eq!(
            copy_value(Variant::Holo, Some(CardStatus::Excellent), None, &prices),
            Some(dec!(24.0))
        );
        assert_eq!(
            copy_value(Variant::Holo, None, None, &prices),
            Some(dec!(30))
        );
        assert_eq!(copy_value(Variant::Normal, None, None, &prices), None);
        assert_eq!(
            copy_value(Variant::Normal, None, Some(dec!(120)), &prices),
            Some(dec!(120)),
            "a graded copy's estimate"
        );
    }
}
