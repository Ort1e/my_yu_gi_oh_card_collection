//! The cards of a set, from ygoprodeck's answers, for the "quick add from a
//! set" panel of the lot forms. Spec: `docs/specs/set-lookup.md`.
//!
//! Pure: the backend fetches the JSON (`card_data::fetch_set`), this module
//! only reshapes it.

use serde::Serialize;
use serde_json::Value;

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
#[allow(unused_variables)] // TODO: remove once implemented
pub fn card_number(code: &str) -> Option<String> {
    todo!("see docs/specs/set-lookup.md, \"What it returns\"")
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
#[allow(unused_variables)] // TODO: remove once implemented
pub fn set_cards(prefix: &str, en: &[Value], fr: &[Value]) -> Vec<SetCard> {
    todo!("see docs/specs/set-lookup.md, \"Where the data comes from\"")
}

#[cfg(test)]
mod tests {
    // Unit tests to write, on small synthetic JSON built with `serde_json::json!`:
    // - card_number: every example of its doc comment;
    // - set_cards: two cards, one code with two rarities (one SetCard, two
    //   rarities), one card without a French name (name_fr: None), a card_sets
    //   entry from another set (ignored), the result sorted by number.
}
