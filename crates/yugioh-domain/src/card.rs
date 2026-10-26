//! Card names, statuses and languages (Django: `models/card.py`,
//! `image_downloader.py`; `Language` is new).

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

/// `card.last_known_status` choices: shared with the Pokémon cards.
pub use common_domain::CardStatus;

/// The language of a card, as written in its code (`RA05-FR037`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    English,
    French,
    German,
    Italian,
    Spanish,
    Portuguese,
}

impl Language {
    pub const ALL: [Language; 6] = [
        Language::English,
        Language::French,
        Language::German,
        Language::Italian,
        Language::Spanish,
        Language::Portuguese,
    ];

    /// In the card codes (since about 2003): `RA05-FR037`.
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "EN",
            Language::French => "FR",
            Language::German => "DE",
            Language::Italian => "IT",
            Language::Spanish => "SP",
            Language::Portuguese => "PT",
        }
    }

    /// In the codes of the oldest sets: `LOB-E001`, `LOB-F001`.
    pub fn old_code(self) -> &'static str {
        match self {
            Language::English => "E",
            Language::French => "F",
            Language::German => "G",
            Language::Italian => "I",
            Language::Spanish => "S",
            Language::Portuguese => "P",
        }
    }

    /// Every way the language is written in a code or a Cardmarket shipment:
    /// the code first, then other spellings (`ES`, the ISO code of Spanish,
    /// in case Cardmarket uses it).
    pub fn spellings(self) -> &'static [&'static str] {
        match self {
            Language::English => &["EN"],
            Language::French => &["FR"],
            Language::German => &["DE"],
            Language::Italian => &["IT"],
            Language::Spanish => &["SP", "ES"],
            Language::Portuguese => &["PT"],
        }
    }

    /// ygoprodeck's `language` parameter for card names in this language;
    /// `None` for English (its default) and Spanish (not offered).
    pub fn ygoprodeck(self) -> Option<&'static str> {
        match self {
            Language::French => Some("fr"),
            Language::German => Some("de"),
            Language::Italian => Some("it"),
            Language::Portuguese => Some("pt"),
            Language::English | Language::Spanish => None,
        }
    }

    /// The language of a card code: `"MAMO-FR023"` → French, `"LOB-F001"`
    /// (old sets: one letter before a digit) → French, `"SDY-046"` → `None`.
    pub fn of_code(code: &str) -> Option<Language> {
        let (_, number) = code.trim().split_once('-')?;
        let number = number.to_ascii_uppercase();
        let modern = Language::ALL.into_iter().find(|l| {
            l.spellings()
                .iter()
                .any(|s| number.len() > s.len() && number.starts_with(s))
        });
        modern.or_else(|| {
            Language::ALL.into_iter().find(|l| {
                number
                    .strip_prefix(l.old_code())
                    .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
            })
        })
    }

    /// A two-letter language (any of its [`spellings`](Self::spellings)),
    /// case-insensitive.
    pub fn from_code(code: &str) -> Option<Language> {
        Language::ALL
            .into_iter()
            .find(|l| l.spellings().iter().any(|s| s.eq_ignore_ascii_case(code)))
    }
}

/// The language a card's name should be translated to: the language of its
/// code (the truth) when that isn't English and the name is still the English
/// one, e.g. `"Rainbow Bridge Bifrost"` with the code `MAMO-FR023`. A name
/// that is already something else is trusted.
pub fn name_to_translate(name: &str, code: &str, english_name: &str) -> Option<Language> {
    let language = Language::of_code(code).filter(|l| *l != Language::English)?;
    name.trim()
        .eq_ignore_ascii_case(english_name.trim())
        .then_some(language)
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
    fn language_codes_round_trip() {
        for l in Language::ALL {
            assert_eq!(Language::from_code(l.code()), Some(l));
        }
    }

    #[test]
    fn language_codes_ignore_case_and_accept_other_spellings() {
        assert_eq!(Language::from_code("fr"), Some(Language::French));
        assert_eq!(Language::from_code("ES"), Some(Language::Spanish));
        assert_eq!(Language::Spanish.code(), "SP", "the card code spelling");
        assert_eq!(Language::from_code("XX"), None);
        assert_eq!(
            Language::from_code("E"),
            None,
            "old codes are not two-letter codes"
        );
    }

    #[test]
    fn language_codes_are_unique() {
        let mut codes: Vec<&str> = Language::ALL
            .iter()
            .flat_map(|l| l.spellings().iter().copied())
            .collect();
        let mut old: Vec<&str> = Language::ALL.iter().map(|l| l.old_code()).collect();
        let (n, m) = (codes.len(), old.len());
        codes.sort_unstable();
        codes.dedup();
        old.sort_unstable();
        old.dedup();
        assert_eq!((codes.len(), old.len()), (n, m));
    }

    #[test]
    fn language_of_a_card_code() {
        assert_eq!(Language::of_code("MAMO-FR023"), Some(Language::French));
        assert_eq!(Language::of_code("RA05-EN037"), Some(Language::English));
        assert_eq!(Language::of_code("ra05-de001"), Some(Language::German));
        assert_eq!(Language::of_code("LOB-F001"), Some(Language::French));
        assert_eq!(Language::of_code("LOB-E001"), Some(Language::English));
        assert_eq!(Language::of_code("ABPF-SPSE1"), Some(Language::Spanish));
    }

    #[test]
    fn codes_without_a_language() {
        assert_eq!(Language::of_code("SDY-046"), None);
        assert_eq!(
            Language::of_code("ABPF-SE1"),
            None,
            "S then a letter: no language"
        );
        assert_eq!(
            Language::of_code("RA05-FR"),
            None,
            "a language but no number"
        );
        assert_eq!(Language::of_code("nonsense"), None);
    }

    #[test]
    fn english_names_with_another_language_code_are_translated() {
        assert_eq!(
            name_to_translate(
                "Rainbow Bridge Bifrost",
                "MAMO-FR023",
                "Rainbow Bridge Bifrost"
            ),
            Some(Language::French)
        );
        assert_eq!(
            name_to_translate(
                "rainbow bridge bifrost ",
                "MAMO-FR023",
                "Rainbow Bridge Bifrost"
            ),
            Some(Language::French)
        );
    }

    #[test]
    fn other_names_are_trusted() {
        // already French
        assert_eq!(
            name_to_translate("Dragon à Trois Cornes", "RA05-FR001", "Tri-Horned Dragon"),
            None
        );
        // an English card
        assert_eq!(
            name_to_translate("Tri-Horned Dragon", "RA05-EN001", "Tri-Horned Dragon"),
            None
        );
        // no language in the code
        assert_eq!(name_to_translate("Kuriboh", "SDY-046", "Kuriboh"), None);
    }

    #[test]
    fn status_codes_round_trip() {
        for s in CardStatus::ALL {
            assert_eq!(CardStatus::from_code(s.code()), Some(s));
        }
        assert_eq!(CardStatus::from_code("XX"), None);
    }
}
