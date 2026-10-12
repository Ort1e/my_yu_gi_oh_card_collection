//! Lots: a purchase or a sale of cards (Django: `models/lot.py`).

use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LotType {
    Purchase,
    Sale,
}

impl LotType {
    /// Value stored in `lot.lot_type`.
    pub fn code(self) -> &'static str {
        match self {
            LotType::Purchase => "purchase",
            LotType::Sale => "sale",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LotType::Purchase => "Purchase",
            LotType::Sale => "Sale",
        }
    }

    pub fn from_code(code: &str) -> Option<LotType> {
        match code {
            "purchase" => Some(LotType::Purchase),
            "sale" => Some(LotType::Sale),
            _ => None,
        }
    }
}

/// `Lot.shipping_cost`: what was paid beyond the cards themselves and the
/// non-card items: `price - (no_card_price + Σ unite prices)`. Unpriced
/// unites count as zero.
pub fn shipping_cost(
    price: Decimal,
    no_card_price: Decimal,
    unite_prices: impl IntoIterator<Item = Option<Decimal>>,
) -> Decimal {
    let cards: Decimal = unite_prices.into_iter().flatten().sum();
    price - (no_card_price + cards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn shipping_cost_ignores_unpriced_unites() {
        let cost = shipping_cost(
            dec!(18.17),
            dec!(1.00),
            [Some(dec!(5.00)), None, Some(dec!(10.02))],
        );
        assert_eq!(cost, dec!(2.15));
    }

    #[test]
    fn lot_type_codes() {
        assert_eq!(LotType::from_code("sale"), Some(LotType::Sale));
        assert_eq!(LotType::from_code(LotType::Purchase.code()), Some(LotType::Purchase));
        assert_eq!(LotType::from_code("other"), None);
    }
}
