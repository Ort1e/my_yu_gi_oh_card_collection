//! The wishlist's rules: whether a wished card's price is under the most
//! the owner wants to pay, and whether a new copy fulfils a wish.

use std::collections::BTreeMap;

use rust_decimal::Decimal;

use super::Variant;

/// A wish, as far as these rules need it.
#[derive(Debug, Clone, PartialEq)]
pub struct Wish {
    pub card_id: i64,
    /// `None`: any language (within the card's catalogue).
    pub language: Option<String>,
    /// `None`: any variant.
    pub variant: Option<Variant>,
    pub max_price: Option<Decimal>,
}

/// A copy, as far as these rules need it.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyKey {
    pub card_id: i64,
    pub language: String,
    pub variant: Variant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Today's price is at or under the maximum: time to buy.
    UnderMax,
    OverMax,
    /// No maximum noted.
    NoMax,
    /// No market price for what is wished.
    NoPrice,
}

impl Verdict {
    /// The CSS class of the wishlist row.
    pub fn css_class(self) -> &'static str {
        match self {
            Verdict::UnderMax => "under-max",
            Verdict::OverMax => "over-max",
            Verdict::NoMax | Verdict::NoPrice => "",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Verdict::UnderMax => "Under your price",
            Verdict::OverMax => "Over your price",
            Verdict::NoMax => "",
            Verdict::NoPrice => "No price",
        }
    }
}

impl Wish {
    /// Today's price of what is wished: its variant's, or the cheapest
    /// variant's when any will do.
    pub fn price(&self, prices: &BTreeMap<Variant, Decimal>) -> Option<Decimal> {
        match self.variant {
            Some(variant) => prices.get(&variant).copied(),
            None => prices.values().min().copied(),
        }
    }

    pub fn verdict(&self, price: Option<Decimal>) -> Verdict {
        match (price, self.max_price) {
            (None, _) => Verdict::NoPrice,
            (Some(_), None) => Verdict::NoMax,
            (Some(price), Some(max)) if price <= max => Verdict::UnderMax,
            (Some(_), Some(_)) => Verdict::OverMax,
        }
    }

    /// Same card, and the language and variant wished (or any).
    pub fn fulfilled_by(&self, copy: &CopyKey) -> bool {
        self.card_id == copy.card_id
            && self.language.as_ref().is_none_or(|l| *l == copy.language)
            && self.variant.is_none_or(|v| v == copy.variant)
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    fn wish(language: Option<&str>, variant: Option<Variant>, max: Option<Decimal>) -> Wish {
        Wish {
            card_id: 7,
            language: language.map(str::to_string),
            variant,
            max_price: max,
        }
    }

    #[test]
    fn prices_and_verdicts() {
        let prices = BTreeMap::from([(Variant::Normal, dec!(0.5)), (Variant::Reverse, dec!(2))]);
        let any = wish(None, None, Some(dec!(1)));
        assert_eq!(any.price(&prices), Some(dec!(0.5)), "the cheapest variant");
        assert_eq!(any.verdict(any.price(&prices)), Verdict::UnderMax);

        let reverse = wish(None, Some(Variant::Reverse), Some(dec!(1)));
        assert_eq!(reverse.verdict(reverse.price(&prices)), Verdict::OverMax);
        assert_eq!(
            reverse.verdict(Some(dec!(1))),
            Verdict::UnderMax,
            "at the maximum"
        );

        let holo = wish(None, Some(Variant::Holo), None);
        assert_eq!(holo.verdict(holo.price(&prices)), Verdict::NoPrice);
        assert_eq!(holo.verdict(Some(dec!(3))), Verdict::NoMax);
    }

    #[test]
    fn copies_that_fulfil_a_wish() {
        let copy = |card_id, language: &str, variant| CopyKey {
            card_id,
            language: language.to_string(),
            variant,
        };
        let any = wish(None, None, None);
        assert!(any.fulfilled_by(&copy(7, "en", Variant::Reverse)));
        assert!(!any.fulfilled_by(&copy(8, "en", Variant::Reverse)));

        let french_holo = wish(Some("fr"), Some(Variant::Holo), None);
        assert!(french_holo.fulfilled_by(&copy(7, "fr", Variant::Holo)));
        assert!(!french_holo.fulfilled_by(&copy(7, "en", Variant::Holo)));
        assert!(!french_holo.fulfilled_by(&copy(7, "fr", Variant::Normal)));
    }
}
