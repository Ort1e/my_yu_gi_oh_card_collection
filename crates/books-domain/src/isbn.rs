//! ISBNs (books; new, not in Django): parsing what is typed or scanned,
//! check digits, ISBN-10 → ISBN-13.
//!
//! Books are stored with their ISBN-13, digits only (`9780553573398`): the
//! barcode on the back of a book is that number (EAN-13), and older ISBN-10s
//! convert to it.

use std::fmt;

/// A valid ISBN-13 (check digit verified), digits only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Isbn(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsbnError {
    /// Not 10 or 13 digits (once spaces and hyphens are removed).
    Length,
    /// A character other than a digit, a space, a hyphen, or a final `X`
    /// (ISBN-10).
    Character,
    /// The last digit doesn't match: a typo or a misread barcode.
    CheckDigit,
    /// 13 digits not starting with 978/979: a barcode, but not a book's.
    NotABook,
}

impl fmt::Display for IsbnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            IsbnError::Length => "an ISBN has 13 digits (or 10 for old books)",
            IsbnError::Character => "an ISBN only has digits (and hyphens)",
            IsbnError::CheckDigit => "its last digit doesn't match: a typo?",
            IsbnError::NotABook => "this barcode isn't a book's ISBN (978… or 979…)",
        })
    }
}

impl std::error::Error for IsbnError {}

impl Isbn {
    /// Accepts `978-0-553-57339-8`, `978 0553 573398`, an ISBN-10
    /// (`0-553-57339-X`, last character may be `X`) or the 13 digits of a
    /// barcode. An ISBN-10 is converted to its ISBN-13.
    pub fn parse(text: &str) -> Result<Isbn, IsbnError> {
        let chars: Vec<char> = text
            .trim()
            .chars()
            .filter(|c| !matches!(c, '-' | ' ' | '\u{2010}'..='\u{2014}'))
            .collect();

        match chars.len() {
            13 => {
                let digits = digits(&chars)?;
                if !(digits.starts_with(&[9, 7, 8]) || digits.starts_with(&[9, 7, 9])) {
                    return Err(IsbnError::NotABook);
                }

                if check_digit_13(&digits[..12]) != digits[12] {
                    return Err(IsbnError::CheckDigit);
                }

                Ok(Isbn(to_string(&digits)))
            }
            10 => {
                let (last, first) = chars.split_last().expect("10 characters");
                let mut digits = digits(first)?;
                let check = match last {
                    'X' | 'x' => 10,
                    c => c.to_digit(10).ok_or(IsbnError::Character)?,
                };
                if check_digit_10(&digits) != check {
                    return Err(IsbnError::CheckDigit);
                }

                digits.splice(0..0, [9, 7, 8]);
                digits.push(check_digit_13(&digits));
                Ok(Isbn(to_string(&digits)))
            }
            _ if chars.iter().any(|c| !c.is_ascii_digit()) => Err(IsbnError::Character),
            _ => Err(IsbnError::Length),
        }
    }

    /// The 13 digits.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The ISBN-10 of a `978…` ISBN (what old catalogues know), `None` for
    /// `979…` (they have none).
    pub fn isbn10(&self) -> Option<String> {
        let digits: Vec<u32> = self.0.chars().filter_map(|c| c.to_digit(10)).collect();
        if !digits.starts_with(&[9, 7, 8]) {
            return None;
        }

        let body = &digits[3..12];
        let check = match check_digit_10(body) {
            10 => 'X',
            d => char::from_digit(d, 10).expect("a digit"),
        };
        Some(format!("{}{check}", to_string(body)))
    }
}

impl fmt::Display for Isbn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn digits(chars: &[char]) -> Result<Vec<u32>, IsbnError> {
    chars
        .iter()
        .map(|c| c.to_digit(10).ok_or(IsbnError::Character))
        .collect()
}

fn to_string(digits: &[u32]) -> String {
    digits
        .iter()
        .map(|&d| char::from_digit(d, 10).expect("a digit"))
        .collect()
}

/// EAN-13: weights 1, 3, 1, 3… on the first 12 digits.
fn check_digit_13(first12: &[u32]) -> u32 {
    let sum: u32 = first12
        .iter()
        .enumerate()
        .map(|(i, d)| if i % 2 == 0 { *d } else { d * 3 })
        .sum();
    (10 - sum % 10) % 10
}

/// ISBN-10: weights 10, 9, … 2 on the first 9 digits; 10 is written `X`.
fn check_digit_10(first9: &[u32]) -> u32 {
    let sum: u32 = first9
        .iter()
        .enumerate()
        .map(|(i, d)| d * (10 - i as u32))
        .sum();
    (11 - sum % 11) % 11
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_isbn13_in_any_spelling() {
        for text in [
            "9780553573398",
            "978-0-553-57339-8",
            " 978 0553 573398 ",
            "978‐0‐553‐57339‐8", // typographic hyphens, pasted from a web page
        ] {
            assert_eq!(Isbn::parse(text).unwrap().as_str(), "9780553573398");
        }
    }

    #[test]
    fn converts_isbn10() {
        assert_eq!(
            Isbn::parse("0-553-57339-X").unwrap().as_str(),
            "9780553573398"
        );
        assert_eq!(Isbn::parse("055357339x").unwrap().as_str(), "9780553573398");
        assert_eq!(Isbn::parse("0316556343").unwrap().as_str(), "9780316556347");
    }

    #[test]
    fn gives_back_isbn10() {
        let isbn = Isbn::parse("9780553573398").unwrap();
        assert_eq!(isbn.isbn10().as_deref(), Some("055357339X"));
        assert_eq!(
            Isbn::parse("9780316556347").unwrap().isbn10().as_deref(),
            Some("0316556343")
        );
        assert_eq!(Isbn::parse("9791032705582").unwrap().isbn10(), None);
    }

    #[test]
    fn refuses_wrong_isbns() {
        assert_eq!(Isbn::parse("9780553573399"), Err(IsbnError::CheckDigit));
        assert_eq!(Isbn::parse("0553573391"), Err(IsbnError::CheckDigit));
        assert_eq!(Isbn::parse("97805535733"), Err(IsbnError::Length));
        assert_eq!(Isbn::parse(""), Err(IsbnError::Length));
        assert_eq!(Isbn::parse("978O553573398"), Err(IsbnError::Character));
        assert_eq!(Isbn::parse("Circe"), Err(IsbnError::Character));
        // a grocery barcode: valid EAN-13, not a book
        assert_eq!(Isbn::parse("3017620422003"), Err(IsbnError::NotABook));
    }
}
