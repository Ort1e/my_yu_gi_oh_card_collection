//! Card names and statuses (Django: `models/card.py`, `image_downloader.py`).

use std::sync::LazyLock;

use regex::Regex;

static PARENTHESES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\([^)]*\)").unwrap());
// Django uses a lookbehind `(?<=\bmaliss)\s+[qpc]\s+`; the regex crate has no
// lookbehind, so capture "maliss" and put it back.
static MALISS_LETTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(maliss)\s+[qpc]\s+").unwrap());

/// Normalised English name stored in `card.en_name` (applied on every card save
/// in Django): drop parenthesised parts, trim, lowercase, and remove the
/// stylised " q "/" p "/" c " letter in "Maliss" card names.
pub fn normalize_card_name(name: &str) -> String {
    let name = PARENTHESES.replace_all(name, "").trim().to_lowercase();
    MALISS_LETTER.replace_all(&name, "$1 ").into_owned()
}

/// Name used to look up / fetch `card_data` from the ygoprodeck API: keep what
/// is before the first '(', trim, lowercase.
pub fn clean_card_name(name: &str) -> String {
    let name = match name.split_once('(') {
        Some((before, _)) => before.trim(),
        None => name,
    };
    name.to_lowercase()
}

/// `card.last_known_status` choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CardStatus {
    #[serde(rename = "MT")]
    Mint,
    #[serde(rename = "NM")]
    NearMint,
    #[serde(rename = "EX")]
    Excellent,
    #[serde(rename = "G")]
    Good,
    #[serde(rename = "P")]
    Poor,
}

impl CardStatus {
    pub const ALL: [CardStatus; 5] = [
        CardStatus::Mint,
        CardStatus::NearMint,
        CardStatus::Excellent,
        CardStatus::Good,
        CardStatus::Poor,
    ];

    /// Value stored in the database.
    pub fn code(self) -> &'static str {
        match self {
            CardStatus::Mint => "MT",
            CardStatus::NearMint => "NM",
            CardStatus::Excellent => "EX",
            CardStatus::Good => "G",
            CardStatus::Poor => "P",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CardStatus::Mint => "Mint",
            CardStatus::NearMint => "Near Mint",
            CardStatus::Excellent => "Excellent",
            CardStatus::Good => "Good",
            CardStatus::Poor => "Poor",
        }
    }

    pub fn from_code(code: &str) -> Option<CardStatus> {
        CardStatus::ALL.into_iter().find(|s| s.code() == code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from my_ygo_cards/tests/test_normalize_card_name.py

    #[test]
    fn basic_normalization() {
        assert_eq!(normalize_card_name("  Dark Magician  "), "dark magician");
    }

    #[test]
    fn parentheses_are_removed() {
        assert_eq!(
            normalize_card_name("Blue-Eyes White Dragon (Limited Edition)"),
            "blue-eyes white dragon"
        );
    }

    #[test]
    fn maliss_q_is_removed() {
        assert_eq!(normalize_card_name("Maliss q Alpha"), "maliss alpha");
    }

    #[test]
    fn maliss_p_is_removed() {
        assert_eq!(normalize_card_name("Maliss p Beta"), "maliss beta");
    }

    #[test]
    fn non_maliss_q_is_not_removed() {
        assert_eq!(normalize_card_name("Other q Name"), "other q name");
    }

    #[test]
    fn maliss_without_spaces_is_untouched() {
        assert_eq!(normalize_card_name("Malissq Alpha"), "malissq alpha");
    }

    #[test]
    fn combined_normalization() {
        assert_eq!(
            normalize_card_name("  Maliss q Gamma (Promo) "),
            "maliss gamma"
        );
    }

    #[test]
    fn c_removal() {
        assert_eq!(
            normalize_card_name("  Maliss c Gamma (Promo) "),
            "maliss gamma"
        );
    }

    #[test]
    fn clean_card_name_cuts_at_parenthesis() {
        assert_eq!(clean_card_name("Dark Magician (V.2)"), "dark magician");
        assert_eq!(clean_card_name("Dark Magician"), "dark magician");
    }

    #[test]
    fn status_codes_round_trip() {
        for s in CardStatus::ALL {
            assert_eq!(CardStatus::from_code(s.code()), Some(s));
        }
        assert_eq!(CardStatus::from_code("XX"), None);
    }
}
