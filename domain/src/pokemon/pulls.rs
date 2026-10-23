//! A booster opening's numbers, typed in one field of the booster form:
//! `67 45r 12x2, TG05`. Each number is a card of the chosen set; a letter
//! after it gives the variant (`r` reverse holo, `h` holo, `n` normal),
//! `x2` the number of copies.

use std::sync::LazyLock;

use regex::Regex;

use super::Variant;

static PULL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?i)([a-z]{0,5}\d{1,4})([rhn])?(?:[x×*](\d{1,2}))?$").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    /// As typed (`67`, `TG05`): matched with `card_ref::same_number`.
    pub number: String,
    /// `None`: the card's default variant.
    pub variant: Option<Variant>,
    /// Copies of this card (1 to 99).
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullError {
    Empty,
    /// The pieces that are not a number (`abc`, `67q`, `12x0`).
    Unreadable(Vec<String>),
}

impl std::fmt::Display for PullError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PullError::Empty => f.write_str("no card number typed"),
            PullError::Unreadable(bad) => write!(
                f,
                "not a card number: {} (write 67, 45r for a reverse holo, 12x2 for two copies)",
                bad.join(", ")
            ),
        }
    }
}

impl std::error::Error for PullError {}

/// The pulls, in the order typed. Spaces, commas and semicolons separate
/// them; every unreadable piece is reported at once.
pub fn parse(text: &str) -> Result<Vec<Pull>, PullError> {
    let mut pulls = Vec::new();
    let mut bad = Vec::new();

    for piece in text
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|p| !p.is_empty())
    {
        match read(piece) {
            Some(pull) => pulls.push(pull),
            None => bad.push(piece.to_string()),
        }
    }

    if !bad.is_empty() {
        return Err(PullError::Unreadable(bad));
    }
    if pulls.is_empty() {
        return Err(PullError::Empty);
    }
    Ok(pulls)
}

fn read(piece: &str) -> Option<Pull> {
    let captures = PULL.captures(piece)?;
    let variant = captures.get(2).map(|m| match m.as_str() {
        "r" | "R" => Variant::Reverse,
        "h" | "H" => Variant::Holo,
        _ => Variant::Normal,
    });
    let count = match captures.get(3) {
        None => 1,
        Some(count) => count.as_str().parse().ok().filter(|n| *n >= 1)?,
    };

    Some(Pull {
        number: captures[1].to_string(),
        variant,
        count,
    })
}

/// The copies a list of pulls makes.
pub fn copies(pulls: &[Pull]) -> u32 {
    pulls.iter().map(|p| p.count).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pull(number: &str, variant: Option<Variant>, count: u32) -> Pull {
        Pull {
            number: number.to_string(),
            variant,
            count,
        }
    }

    #[test]
    fn reads_numbers_variants_and_counts() {
        let pulls = parse("67 45r 12x2, TG05; 3H 8nx3 9R×2").unwrap();
        assert_eq!(
            pulls,
            [
                pull("67", None, 1),
                pull("45", Some(Variant::Reverse), 1),
                pull("12", None, 2),
                pull("TG05", None, 1),
                pull("3", Some(Variant::Holo), 1),
                pull("8", Some(Variant::Normal), 3),
                pull("9", Some(Variant::Reverse), 2),
            ]
        );
        assert_eq!(copies(&pulls), 11);
    }

    #[test]
    fn reports_every_unreadable_piece() {
        assert_eq!(parse(" , "), Err(PullError::Empty));
        assert_eq!(
            parse("67 abc 12x0 45q"),
            Err(PullError::Unreadable(vec![
                "abc".into(),
                "12x0".into(),
                "45q".into()
            ]))
        );
    }
}
