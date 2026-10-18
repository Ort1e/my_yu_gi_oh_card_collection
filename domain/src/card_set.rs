//! The cards of a set, from ygoprodeck's answers, for the "quick add from a
//! set" panel of the lot forms (`GET /api/sets/:code`).
//!
//! Pure: the backend fetches the JSON (`card_data::fetch_set`), this module
//! only reshapes it.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::card::Language;

/// One card number of a set, as sent to the panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetCard {
    /// The number in the set, without the language: `"037"`, `"SE1"`.
    pub number: String,
    pub ygopro_id: i64,
    pub name_en: String,
    /// `None` when ygoprodeck has no French name for the card.
    pub name_fr: Option<String>,
    /// Every rarity that code exists in, without duplicates.
    pub rarities: Vec<String>,
}

/// The number of a card code, without the set and the language:
///
/// - `"RA05-EN037"` → `Some("037")`
/// - `"ABPF-ENSE1"` → `Some("SE1")` (only the language `EN` is dropped)
/// - `"LOB-E001"` → `Some("001")` (old sets: a lone `E` before a digit)
/// - `"SDY-046"` → `Some("046")` (no language)
/// - `"nonsense"` → `None` (no `-`)
pub fn card_number(code: &str) -> Option<String> {
    let (_, number) = code.split_once('-')?;

    let number = if let Some(lang) = number.strip_prefix(Language::English.code()) {
        lang
    } else if let Some(lang) = number.strip_prefix(Language::English.old_code()) {
        if lang.starts_with(|c: char| c.is_ascii_digit()) {
            lang
        } else {
            number
        }
    } else {
        number
    };

    if number.is_empty() {
        None
    } else {
        Some(number.to_string())
    }
}

/// The cards of the set `prefix` (`"RA05"`), sorted by number.
///
/// - `en`: the English cards, i.e. the `data` arrays of the
///   `cardinfo.php?cardset=…` answers put together (one answer per set sharing
///   the prefix);
/// - `fr`: the same with `&language=fr` (may be empty).
///
/// Keeps only the `card_sets` entries whose `set_code` starts with
/// `"<prefix>-"`; matches French names to English cards by `id`.
pub fn set_cards(prefix: &str, en: &[Value], fr: &[Value]) -> Vec<SetCard> {
    fn get_cards_from_value(prefix: &str, val: &Value, fr: &[Value]) -> Vec<SetCard> {
        let empty = Vec::new();
        let sets = val
            .get("card_sets")
            .and_then(|s| s.as_array())
            .unwrap_or(&empty);
        // "RA05-": with the dash, so that "RA050-EN001" is not taken for RA05
        let wanted = format!("{prefix}-");

        sets.iter()
            .filter_map(|inside| {
                let set_code = inside.get("set_code")?.as_str()?;
                if set_code.starts_with(&wanted) {
                    let ygopro_id = val.get("id")?.as_i64()?;
                    let name_en = val.get("name")?.as_str()?.to_string();
                    let name_fr = fr
                        .iter()
                        .filter_map(|c| {
                            if c.get("id")?.as_i64()? == ygopro_id {
                                c.get("name")?.as_str().map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                        .next();
                    let rarities = vec![inside.get("set_rarity")?.as_str()?.to_string()];
                    Some(SetCard {
                        number: card_number(set_code).unwrap_or_else(|| set_code.to_string()),
                        ygopro_id,
                        name_en,
                        name_fr,
                        rarities,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    let en_cards: Vec<SetCard> = en
        .iter()
        .flat_map(|val| get_cards_from_value(prefix, val, fr))
        .collect();

    if en_cards.is_empty() {
        Vec::new()
    } else {
        // Group by number and merge rarities
        let mut cards_map: HashMap<String, SetCard> = HashMap::new();
        for card in en_cards {
            cards_map
                .entry(card.number.clone())
                .and_modify(|existing_card| {
                    // without duplicates, in the order ygoprodeck lists them
                    for rarity in &card.rarities {
                        if !existing_card.rarities.contains(rarity) {
                            existing_card.rarities.push(rarity.clone());
                        }
                    }
                })
                .or_insert(card);
        }
        let mut values = cards_map.into_values().collect::<Vec<SetCard>>();
        values.sort_by(|a, b| a.number.cmp(&b.number));
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---------- card_number ----------

    #[test]
    fn number_without_the_english_language() {
        assert_eq!(card_number("RA05-EN037").as_deref(), Some("037"));
        assert_eq!(card_number("LOB-EN001").as_deref(), Some("001"));
    }

    #[test]
    fn number_keeps_what_follows_en() {
        assert_eq!(card_number("ABPF-ENSE1").as_deref(), Some("SE1"));
    }

    #[test]
    fn number_of_old_sets_with_a_lone_e() {
        assert_eq!(card_number("LOB-E001").as_deref(), Some("001"));
    }

    #[test]
    fn number_without_a_language() {
        assert_eq!(card_number("SDY-046").as_deref(), Some("046"));
    }

    #[test]
    fn number_keeps_its_leading_zeros() {
        assert_eq!(card_number("RA05-EN001").as_deref(), Some("001"));
    }

    #[test]
    fn no_number_without_a_dash() {
        assert_eq!(card_number("nonsense"), None);
    }

    // Confusing codes: only a leading "EN", or a lone "E" followed by a
    // digit, is a language; everything else is part of the number.

    #[test]
    fn e_followed_by_a_letter_is_not_a_language() {
        assert_eq!(card_number("XYZ-EE010").as_deref(), Some("EE010"));
        assert_eq!(card_number("XYZ-ESE1").as_deref(), Some("ESE1"));
    }

    #[test]
    fn the_language_is_dropped_once() {
        assert_eq!(card_number("XYZ-ENE001").as_deref(), Some("E001"));
        assert_eq!(card_number("XYZ-ENEN01").as_deref(), Some("EN01"));
    }

    #[test]
    fn a_lone_e_without_a_digit_stays() {
        assert_eq!(card_number("XYZ-E").as_deref(), Some("E"));
        assert_eq!(card_number("XYZ-EX1").as_deref(), Some("EX1"));
    }

    #[test]
    fn en_inside_the_number_stays() {
        assert_eq!(card_number("XYZ-0EN1").as_deref(), Some("0EN1"));
        assert_eq!(card_number("XYZ-SE1").as_deref(), Some("SE1"));
    }

    #[test]
    fn other_languages_are_not_dropped() {
        // ygoprodeck codes are always English: anything else is the number
        assert_eq!(card_number("RA05-FR037").as_deref(), Some("FR037"));
        assert_eq!(card_number("LOB-F001").as_deref(), Some("F001"));
    }

    #[test]
    fn an_empty_number_is_no_number() {
        assert_eq!(card_number("RA05-"), None);
        assert_eq!(card_number("RA05-EN"), None);
        assert_eq!(
            card_number("LOB-E"),
            Some("E".to_string()),
            "a lone E stays"
        );
    }

    // ---------- set_cards ----------

    /// A card of a `cardinfo.php` answer, with only the fields used.
    fn card(id: i64, name: &str, sets: &[(&str, &str)]) -> Value {
        let sets: Vec<Value> = sets
            .iter()
            .map(|(code, rarity)| json!({ "set_code": code, "set_rarity": rarity }))
            .collect();
        json!({ "id": id, "name": name, "card_sets": sets })
    }

    /// The same card in a French answer: only the name changes.
    fn french(id: i64, name: &str) -> Value {
        json!({ "id": id, "name": name, "card_sets": [] })
    }

    fn numbers(cards: &[SetCard]) -> Vec<&str> {
        cards.iter().map(|c| c.number.as_str()).collect()
    }

    #[test]
    fn cards_of_the_set_sorted_by_number() {
        let en = [
            card(
                2,
                "Albion the Sanctifire Dragon",
                &[("RA05-EN037", "Ultra Rare")],
            ),
            card(1, "Tri-Horned Dragon", &[("RA05-EN001", "Starlight Rare")]),
            card(3, "Kuriboh", &[("RA05-EN010", "Common")]),
        ];
        let cards = set_cards("RA05", &en, &[]);
        assert_eq!(numbers(&cards), ["001", "010", "037"]);
        assert_eq!(cards[0].ygopro_id, 1);
        assert_eq!(cards[0].name_en, "Tri-Horned Dragon");
    }

    #[test]
    fn one_entry_per_code_with_all_its_rarities() {
        let en = [card(
            1,
            "Tri-Horned Dragon",
            &[
                ("RA05-EN001", "Starlight Rare"),
                ("RA05-EN001", "Secret Rare"),
                ("RA05-EN001", "Secret Rare"),
            ],
        )];
        let cards = set_cards("RA05", &en, &[]);
        assert_eq!(cards.len(), 1);
        // without duplicates, in the order ygoprodeck lists them
        assert_eq!(cards[0].rarities, ["Starlight Rare", "Secret Rare"]);
    }

    #[test]
    fn printings_of_other_sets_are_ignored() {
        let en = [card(
            1,
            "Tri-Horned Dragon",
            &[
                ("LOB-EN001", "Ultra Rare"),
                // a prefix that only starts like ours
                ("RA050-EN001", "Common"),
                ("RA05-EN001", "Starlight Rare"),
            ],
        )];
        let cards = set_cards("RA05", &en, &[]);
        assert_eq!(numbers(&cards), ["001"]);
        assert_eq!(cards[0].rarities, ["Starlight Rare"]);
    }

    #[test]
    fn a_card_with_no_printing_in_the_set_is_left_out() {
        let en = [
            card(1, "Tri-Horned Dragon", &[("RA05-EN001", "Starlight Rare")]),
            card(9, "Elsewhere", &[("LOB-EN009", "Common")]),
        ];
        assert_eq!(numbers(&set_cards("RA05", &en, &[])), ["001"]);
    }

    #[test]
    fn french_names_are_matched_by_id() {
        let en = [
            card(1, "Tri-Horned Dragon", &[("RA05-EN001", "Starlight Rare")]),
            card(
                2,
                "Albion the Sanctifire Dragon",
                &[("RA05-EN037", "Ultra Rare")],
            ),
        ];
        // in another order than the English answer, and without card 2
        let fr = [french(3, "Kuriboh"), french(1, "Dragon à Trois Cornes")];
        let cards = set_cards("RA05", &en, &fr);
        assert_eq!(cards[0].name_fr.as_deref(), Some("Dragon à Trois Cornes"));
        assert_eq!(cards[1].name_fr, None, "no French name");
        assert_eq!(cards[1].name_en, "Albion the Sanctifire Dragon");
    }

    #[test]
    fn several_codes_of_one_card_in_the_set() {
        // ABPF: the card is in the set and in its special edition
        let en = [card(
            7,
            "Gladiator Beast Laquari",
            &[("ABPF-EN010", "Common"), ("ABPF-ENSE1", "Super Rare")],
        )];
        let cards = set_cards("ABPF", &en, &[]);
        assert_eq!(numbers(&cards), ["010", "SE1"]);
        assert!(cards.iter().all(|c| c.ygopro_id == 7));
    }

    #[test]
    fn a_card_in_two_answers_counts_once() {
        // several sets share the prefix: the same card can come in each answer
        let entry = card(7, "Gladiator Beast Laquari", &[("ABPF-EN010", "Common")]);
        let cards = set_cards("ABPF", &[entry.clone(), entry], &[]);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].rarities, ["Common"]);
    }

    #[test]
    fn old_codes_give_their_number() {
        let en = [card(1, "Dark Magician", &[("LOB-E001", "Ultra Rare")])];
        assert_eq!(numbers(&set_cards("LOB", &en, &[])), ["001"]);
    }

    #[test]
    fn no_card_no_entry() {
        assert!(set_cards("RA05", &[], &[]).is_empty());
    }
}
