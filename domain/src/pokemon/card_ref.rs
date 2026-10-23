//! The code printed at the bottom of a Pokémon card, typed in the card
//! finder to reach a card without searching by name: `FNE 067/064` (French
//! set code, number, set size), `SV6a 010/064` (Japanese), `TG05/TG30`
//! (a subset number), or the number alone (the set is then chosen apart).

use std::sync::LazyLock;

use regex::Regex;

/// A card number: up to 5 letters (`TG`, `GG`, `SV`, `SWSH`), then digits.
static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z]{0,5}\d{1,4}$").unwrap());
/// A set code: a letter, then letters, digits, `.` or `-` (`FNE`, `SV6a`, `SV-P`).
static SET_CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9.\-]{1,9}$").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardRef {
    /// As typed (`fne`): set lookups ignore the case.
    pub set_code: Option<String>,
    /// As printed, without the `/total` (`067`, `TG05`).
    pub number: String,
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

    let number = number.split('/').next().unwrap_or_default();
    if !NUMBER.is_match(number) {
        let code_alone = code.is_none() && SET_CODE.is_match(number);
        return Err(if code_alone {
            CardRefError::NoNumber
        } else {
            CardRefError::Invalid
        });
    }
    if code.is_some_and(|code| !SET_CODE.is_match(code)) {
        return Err(CardRefError::Invalid);
    }

    Ok(CardRef {
        set_code: code.map(str::to_string),
        number: number.to_string(),
    })
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

    fn card_ref(code: Option<&str>, number: &str) -> CardRef {
        CardRef {
            set_code: code.map(str::to_string),
            number: number.to_string(),
        }
    }

    #[test]
    fn reads_printed_codes() {
        assert_eq!(parse("FNE 067/064"), Ok(card_ref(Some("FNE"), "067")));
        assert_eq!(parse("  fne   67 "), Ok(card_ref(Some("fne"), "67")));
        assert_eq!(parse("SV6a 010/064"), Ok(card_ref(Some("SV6a"), "010")));
        assert_eq!(parse("067/064"), Ok(card_ref(None, "067")));
        assert_eq!(parse("TG05/TG30"), Ok(card_ref(None, "TG05")));
        assert_eq!(parse("SV107"), Ok(card_ref(None, "SV107")));
        assert_eq!(parse("SV-P 001"), Ok(card_ref(Some("SV-P"), "001")));
    }

    #[test]
    fn explains_what_is_wrong() {
        assert_eq!(parse(""), Err(CardRefError::Empty));
        assert_eq!(parse("   "), Err(CardRefError::Empty));
        assert_eq!(parse("FNE"), Err(CardRefError::NoNumber));
        assert_eq!(parse("SV6a"), Err(CardRefError::NoNumber));
        assert_eq!(parse("/064"), Err(CardRefError::Invalid));
        assert_eq!(parse("FNE 067 extra"), Err(CardRefError::Invalid));
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
