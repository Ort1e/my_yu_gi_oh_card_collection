//! The code printed at the bottom of a Pokémon card, typed in the card
//! finder to reach a card without searching by name: `FNE 067/064` (French
//! set code, number, set size), `SV6a 010/064` (Japanese), `TG05/TG30`
//! (a subset number), `30C 012/128` (a code starting with digits), `30C B`
//! (a card numbered by a letter: the Mew cards R, G, B of 30th Celebration),
//! or the number alone (the set is then chosen apart).
//!
//! The `/total` is kept: two sets can share a code (30th Celebration and its
//! Classic Collection are both `30C`), and the set size tells them apart.

use std::sync::LazyLock;

use regex::Regex;

/// A card number: up to 5 letters (`TG`, `GG`, `SV`, `SWSH`), then digits;
/// or a single letter (`B`).
static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[A-Za-z]{0,5}\d{1,4}|[A-Za-z])$").unwrap());
/// A set code: letters, digits, `.` or `-`, with at least one letter
/// ([`is_set_code`]): `FNE`, `SV6a`, `SV-P`, `30C`.
static SET_CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9.\-]{1,9}$").unwrap());

fn is_set_code(text: &str) -> bool {
    SET_CODE.is_match(text) && text.chars().any(|c| c.is_ascii_alphabetic())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardRef {
    /// As typed (`fne`): set lookups ignore the case.
    pub set_code: Option<String>,
    /// As printed, without the `/total` (`067`, `TG05`, `B`).
    pub number: String,
    /// The set size printed after the number (`064` in `067/064`), when typed.
    pub total: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardRefError {
    Empty,
    /// A set code but no number (`FNE`).
    NoNumber,
    /// Not a code and a number (`FNE 067 extra`, `abc!`).
    Invalid,
}

impl std::fmt::Display for CardRefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CardRefError::Empty => "nothing typed",
            CardRefError::NoNumber => "the card's number is missing (FNE 067)",
            CardRefError::Invalid => "not a printed code like FNE 067/064",
        })
    }
}

impl std::error::Error for CardRefError {}

/// `"FNE 067/064"` → set `FNE`, number `067`.
pub fn parse(text: &str) -> Result<CardRef, CardRefError> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let (code, number) = match tokens.as_slice() {
        [] => return Err(CardRefError::Empty),
        [one] => (None, *one),
        [code, number] => (Some(*code), *number),
        _ => return Err(CardRefError::Invalid),
    };

    let (number, total) = match number.split_once('/') {
        Some((number, total)) => (number, Some(total)),
        None => (number, None),
    };
    if !NUMBER.is_match(number) || total.is_some_and(|t| !NUMBER.is_match(t)) {
        let code_alone = code.is_none() && total.is_none() && is_set_code(number);
        return Err(if code_alone {
            CardRefError::NoNumber
        } else {
            CardRefError::Invalid
        });
    }
    if code.is_some_and(|code| !is_set_code(code)) {
        return Err(CardRefError::Invalid);
    }

    Ok(CardRef {
        set_code: code.map(str::to_string),
        number: number.to_string(),
        total: total.map(str::to_string),
    })
}

/// Whether a printed set size (`064`, `TG30`) is this set's (`official`
/// card count): only plain numbers are compared, a subset total never is.
pub fn is_set_size(total: &str, official: Option<i64>) -> bool {
    match (total.parse::<i64>(), official) {
        (Ok(total), Some(official)) => total == official,
        _ => false,
    }
}

/// Whether two card numbers are the same card's: `67` = `067`, `tg5` =
/// `TG05` (same letters, same value); otherwise equal ignoring the case.
pub fn same_number(a: &str, b: &str) -> bool {
    fn split(n: &str) -> Option<(String, u32)> {
        let digits_at = n.find(|c: char| c.is_ascii_digit())?;
        let (letters, digits) = n.split_at(digits_at);
        Some((letters.to_ascii_uppercase(), digits.parse().ok()?))
    }

    match (split(a), split(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a.eq_ignore_ascii_case(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card_ref(code: Option<&str>, number: &str, total: Option<&str>) -> CardRef {
        CardRef {
            set_code: code.map(str::to_string),
            number: number.to_string(),
            total: total.map(str::to_string),
        }
    }

    #[test]
    fn reads_printed_codes() {
        assert_eq!(
            parse("FNE 067/064"),
            Ok(card_ref(Some("FNE"), "067", Some("064")))
        );
        assert_eq!(parse("  fne   67 "), Ok(card_ref(Some("fne"), "67", None)));
        assert_eq!(
            parse("SV6a 010/064"),
            Ok(card_ref(Some("SV6a"), "010", Some("064")))
        );
        assert_eq!(parse("067/064"), Ok(card_ref(None, "067", Some("064"))));
        assert_eq!(parse("TG05/TG30"), Ok(card_ref(None, "TG05", Some("TG30"))));
        assert_eq!(parse("SV107"), Ok(card_ref(None, "SV107", None)));
        assert_eq!(parse("SV-P 001"), Ok(card_ref(Some("SV-P"), "001", None)));
    }

    #[test]
    fn reads_30th_celebration_codes() {
        assert_eq!(
            parse("30C 012/128"),
            Ok(card_ref(Some("30C"), "012", Some("128")))
        );
        assert_eq!(parse("30c B"), Ok(card_ref(Some("30c"), "B", None)));
        assert_eq!(parse("30C"), Err(CardRefError::NoNumber));
        assert_eq!(
            parse("12 34"),
            Err(CardRefError::Invalid),
            "a code has a letter"
        );
        assert!(is_set_size("128", Some(128)) && is_set_size("030", Some(30)));
        assert!(!is_set_size("030", Some(128)) && !is_set_size("TG30", Some(30)));
        assert!(!is_set_size("128", None));
    }

    #[test]
    fn explains_what_is_wrong() {
        assert_eq!(parse(""), Err(CardRefError::Empty));
        assert_eq!(parse("   "), Err(CardRefError::Empty));
        assert_eq!(parse("FNE"), Err(CardRefError::NoNumber));
        assert_eq!(parse("SV6a"), Err(CardRefError::NoNumber));
        assert_eq!(parse("/064"), Err(CardRefError::Invalid));
        assert_eq!(parse("FNE 067 extra"), Err(CardRefError::Invalid));
        assert_eq!(parse("FNE 067/??"), Err(CardRefError::Invalid));
        assert_eq!(parse("F!E 067"), Err(CardRefError::Invalid));
        assert_eq!(parse("Hypotrempe"), Err(CardRefError::NoNumber));
    }

    #[test]
    fn numbers_match_without_zeros_or_case() {
        assert!(same_number("67", "067"));
        assert!(same_number("tg5", "TG05"));
        assert!(same_number("SV107", "sv107"));
        assert!(!same_number("67", "068"));
        assert!(!same_number("TG05", "GG05"));
        assert!(!same_number("5", "TG05"));
        assert!(same_number("A", "a"));
    }
}
