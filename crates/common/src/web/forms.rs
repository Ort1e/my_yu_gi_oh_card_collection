//! Parsing and validation of the HTML forms.
//!
//! Forms are read as raw `(name, value)` pairs (`Form<Vec<(String, String)>>`)
//! because some rows repeat: `cards-<i>-<field>`, `rounds-<i>-games-<j>-…`. Values are kept as
//! strings so an invalid form can be re-rendered as typed.

use std::collections::BTreeMap;

use chrono::NaiveDate;
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
}

/// Accepts "1.50" and "1,50".
pub fn parse_decimal(s: &str) -> Option<Decimal> {
    s.trim().replace(',', ".").parse().ok()
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
    fn required_fields() {
        let mut f = form(&[("price", " "), ("buy_date", "2025-13-01")]);
        assert_eq!(f.required_decimal("price", "Price"), None);
        assert_eq!(f.required_date("buy_date", "Buy date"), None);
        assert_eq!(f.errors.len(), 2);
    }
}
