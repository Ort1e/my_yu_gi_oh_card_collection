//! Parsing and validation of the HTML forms.
//!
//! Forms are read as raw `(name, value)` pairs (`Form<Vec<(String, String)>>`)
//! because the card rows repeat: `cards-<i>-<field>`. Values are kept as
//! strings so an invalid form can be re-rendered as typed.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use domain::card::{normalize_card_name, CardStatus};
use domain::rarity::Rarity;
use rust_decimal::Decimal;

pub type Pairs = Vec<(String, String)>;

/// Form values by name (last value wins), plus collected validation errors.
#[derive(Debug, Default)]
pub struct FormData {
    values: BTreeMap<String, String>,
    pub errors: Vec<String>,
}

impl FormData {
    pub fn new(pairs: Pairs) -> FormData {
        FormData {
            values: pairs.into_iter().collect(),
            errors: Vec::new(),
        }
    }

    /// Trimmed value, `None` when absent or blank.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
    }

    pub fn text(&self, name: &str) -> String {
        self.get(name).unwrap_or_default().to_string()
    }

    pub fn checked(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn required(&mut self, name: &str, label: &str) -> Option<String> {
        let value = self.get(name).map(str::to_string);
        if value.is_none() {
            self.errors.push(format!("{label} is required."));
        }
        value
    }

    pub fn decimal(&mut self, name: &str, label: &str) -> Option<Decimal> {
        let value = self.get(name)?;
        match parse_decimal(value) {
            Some(d) if d >= Decimal::ZERO => Some(d),
            _ => {
                self.errors
                    .push(format!("{label}: \"{value}\" is not a valid amount."));
                None
            }
        }
    }

    pub fn required_decimal(&mut self, name: &str, label: &str) -> Option<Decimal> {
        self.required(name, label)?;
        self.decimal(name, label)
    }

    pub fn date(&mut self, name: &str, label: &str) -> Option<NaiveDate> {
        let value = self.get(name)?;
        match value.parse() {
            Ok(d) => Some(d),
            Err(_) => {
                self.errors
                    .push(format!("{label}: \"{value}\" is not a valid date."));
                None
            }
        }
    }

    pub fn required_date(&mut self, name: &str, label: &str) -> Option<NaiveDate> {
        self.required(name, label)?;
        self.date(name, label)
    }

    /// A whole number in `range`, `None` when blank; an error otherwise.
    pub fn number(
        &mut self,
        name: &str,
        label: &str,
        range: std::ops::RangeInclusive<i64>,
    ) -> Option<i64> {
        let value = self.get(name)?;
        match value.parse::<i64>() {
            Ok(n) if range.contains(&n) => Some(n),
            _ => {
                self.errors.push(format!(
                    "{label}: \"{value}\" should be a number between {} and {}.",
                    range.start(),
                    range.end()
                ));
                None
            }
        }
    }

    pub fn id(&mut self, name: &str, label: &str) -> Option<i64> {
        let value = self.get(name)?;
        match value.parse() {
            Ok(id) => Some(id),
            Err(_) => {
                self.errors.push(format!("{label}: invalid choice."));
                None
            }
        }
    }

    /// Indices of the repeated rows `<prefix><i>-…` present, in order:
    /// `indices("cards-")`, `indices("rounds-2-games-")`.
    pub fn indices(&self, prefix: &str) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .values
            .keys()
            .filter_map(|k| k.strip_prefix(prefix)?.split_once('-')?.0.parse().ok())
            .collect();
        indices.sort_unstable();
        indices.dedup();
        indices
    }

    /// The card rows as typed (for re-rendering), skipping fully empty ones.
    pub fn card_rows(&self) -> Vec<CardRowValues> {
        self.indices("cards-")
            .into_iter()
            .map(|i| {
                let field = |f: &str| self.text(&format!("cards-{i}-{f}"));
                CardRowValues {
                    name: field("name"),
                    en_name: field("en_name"),
                    code: field("code"),
                    status: field("status"),
                    rarity: field("rarity"),
                    is_proxy: self.checked(&format!("cards-{i}-is_proxy")),
                    price: field("price"),
                    quantity: field("quantity"),
                }
            })
            .filter(|row| !row.is_blank())
            .collect()
    }

    /// Validate the card rows; errors are added to `self.errors`.
    pub fn new_cards(&mut self) -> Vec<NewCards> {
        let rows = self.card_rows();
        let mut cards = Vec::with_capacity(rows.len());
        for (n, row) in rows.iter().enumerate() {
            let n = n + 1;
            let mut ok = true;
            if row.name.is_empty() {
                self.errors.push(format!("Card {n}: name is required."));
                ok = false;
            }
            let status = match row.status.as_str() {
                "" => None,
                code => {
                    let status = CardStatus::from_code(code);
                    if status.is_none() {
                        self.errors.push(format!("Card {n}: invalid status."));
                        ok = false;
                    }
                    status
                }
            };
            let rarity = match row.rarity.as_str() {
                "" => None,
                label => {
                    let rarity = Rarity::from_label(label);
                    if rarity.is_none() {
                        self.errors.push(format!("Card {n}: unknown rarity."));
                        ok = false;
                    }
                    rarity
                }
            };
            let price = match row.price.as_str() {
                "" => None,
                p => match parse_decimal(p).filter(|d| *d >= Decimal::ZERO) {
                    Some(d) => Some(d),
                    None => {
                        self.errors
                            .push(format!("Card {n}: \"{p}\" is not a valid price."));
                        ok = false;
                        None
                    }
                },
            };
            let quantity = match row.quantity.as_str() {
                "" => 1,
                q => match q.parse::<u32>() {
                    Ok(q) if (1..=100).contains(&q) => q,
                    _ => {
                        self.errors
                            .push(format!("Card {n}: quantity must be between 1 and 100."));
                        ok = false;
                        1
                    }
                },
            };
            if ok {
                let en_name = if row.en_name.is_empty() {
                    &row.name
                } else {
                    &row.en_name
                };
                cards.push(NewCards {
                    name: row.name.clone(),
                    en_name: normalize_card_name(en_name),
                    code: Some(row.code.clone()).filter(|c| !c.is_empty()),
                    status,
                    rarity,
                    is_proxy: row.is_proxy,
                    price,
                    quantity,
                });
            }
        }
        cards
    }
}

/// Accepts "1.50" and "1,50".
pub fn parse_decimal(s: &str) -> Option<Decimal> {
    s.trim().replace(',', ".").parse().ok()
}

/// One card row of a lot form, as typed.
#[derive(Debug, Clone, Default)]
pub struct CardRowValues {
    pub name: String,
    pub en_name: String,
    pub code: String,
    pub status: String,
    /// A rarity label (`Ultra Rare`), or empty.
    pub rarity: String,
    pub is_proxy: bool,
    pub price: String,
    pub quantity: String,
}

impl CardRowValues {
    fn is_blank(&self) -> bool {
        self.name.is_empty()
            && self.en_name.is_empty()
            && self.code.is_empty()
            && self.price.is_empty()
    }
}

/// A validated card row: `quantity` identical cards, each in its own unite.
#[derive(Debug, Clone)]
pub struct NewCards {
    pub name: String,
    /// Already normalised.
    pub en_name: String,
    pub code: Option<String>,
    pub status: Option<CardStatus>,
    pub rarity: Option<Rarity>,
    pub is_proxy: bool,
    pub price: Option<Decimal>,
    pub quantity: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(pairs: &[(&str, &str)]) -> FormData {
        FormData::new(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    #[test]
    fn parses_card_rows() {
        let mut f = form(&[
            ("cards-0-name", "Dark Magician"),
            ("cards-0-price", "1,50"),
            ("cards-0-status", "NM"),
            ("cards-0-is_proxy", "on"),
            ("cards-3-name", "Kuriboh"),
            ("cards-3-en_name", "Kuriboh (Promo)"),
            ("cards-3-quantity", "3"),
            ("cards-5-name", ""),
            ("cards-5-price", ""),
        ]);
        let cards = f.new_cards();
        assert!(f.errors.is_empty(), "{:?}", f.errors);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].en_name, "dark magician");
        assert_eq!(cards[0].price, Some("1.50".parse().unwrap()));
        assert_eq!(cards[0].status, Some(CardStatus::NearMint));
        assert!(cards[0].is_proxy);
        assert_eq!(cards[1].en_name, "kuriboh");
        assert_eq!(cards[1].quantity, 3);
    }

    #[test]
    fn reports_invalid_rows() {
        let mut f = form(&[
            ("cards-0-price", "abc"),
            ("cards-1-name", "X"),
            ("cards-1-quantity", "0"),
            ("cards-2-name", "Y"),
            ("cards-2-status", "ZZ"),
        ]);
        assert!(f.new_cards().is_empty());
        assert_eq!(f.errors.len(), 4, "{:?}", f.errors);
    }

    #[test]
    fn required_fields() {
        let mut f = form(&[("price", " "), ("buy_date", "2025-13-01")]);
        assert_eq!(f.required_decimal("price", "Price"), None);
        assert_eq!(f.required_date("buy_date", "Buy date"), None);
        assert_eq!(f.errors.len(), 2);
    }
}
