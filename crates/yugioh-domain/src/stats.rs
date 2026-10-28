//! The collection's statistics (`/collection/stats`, also in the public
//! copy): copies and market value grouped by rarity, kind of card, set,
//! language or condition. No Django equivalent.

use std::collections::BTreeMap;

use rust_decimal::Decimal;

/// What kind of card, by where it is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CardKind {
    /// The main deck's monsters (Pendulum and Ritual ones included).
    Monster,
    /// Fusion, Synchro, Xyz, Link.
    ExtraDeck,
    Spell,
    Trap,
    /// Tokens, skill cards, no card data.
    Other,
}

impl CardKind {
    pub fn label(self) -> &'static str {
        match self {
            CardKind::Monster => "Monsters",
            CardKind::ExtraDeck => "Extra Deck monsters",
            CardKind::Spell => "Spells",
            CardKind::Trap => "Traps",
            CardKind::Other => "Other",
        }
    }
}

/// The kind of a card from ygoprodeck's `type` (`"XYZ Monster"`,
/// `"Synchro Pendulum Effect Monster"`, `"Spell Card"`…).
pub fn kind(card_type: &str) -> CardKind {
    let card_type = card_type.to_lowercase();
    if card_type.contains("spell") {
        CardKind::Spell
    } else if card_type.contains("trap") {
        CardKind::Trap
    } else if ["fusion", "synchro", "xyz", "link"]
        .iter()
        .any(|extra| card_type.contains(extra))
    {
        CardKind::ExtraDeck
    } else if card_type.contains("monster") {
        CardKind::Monster
    } else {
        CardKind::Other
    }
}

/// The set part of a card code: `MAMO-FR023` → `MAMO`, `LOB-005` → `LOB`.
pub fn set_prefix(code: &str) -> Option<&str> {
    let (prefix, rest) = code.trim().split_once('-')?;
    (!prefix.is_empty() && !rest.is_empty()).then_some(prefix)
}

/// The copies of one group and what they are worth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group<K> {
    pub key: K,
    pub copies: usize,
    /// The copies with a price, added up.
    pub value: Decimal,
}

/// The copies grouped by key, each copy as `(its key, its value if
/// priced)`, in the keys' order.
pub fn breakdown<K: Ord>(copies: impl IntoIterator<Item = (K, Option<Decimal>)>) -> Vec<Group<K>> {
    let mut groups: BTreeMap<K, (usize, Decimal)> = BTreeMap::new();
    for (key, value) in copies {
        let group = groups.entry(key).or_default();
        group.0 += 1;
        group.1 += value.unwrap_or_default();
    }
    groups
        .into_iter()
        .map(|(key, (copies, value))| Group { key, copies, value })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn kinds_from_ygoprodeck_types() {
        for (card_type, expected) in [
            ("Effect Monster", CardKind::Monster),
            ("Normal Monster", CardKind::Monster),
            ("Pendulum Effect Monster", CardKind::Monster),
            ("Ritual Effect Monster", CardKind::Monster),
            ("Flip Effect Monster", CardKind::Monster),
            ("XYZ Monster", CardKind::ExtraDeck),
            ("Synchro Tuner Monster", CardKind::ExtraDeck),
            ("Synchro Pendulum Effect Monster", CardKind::ExtraDeck),
            ("Fusion Monster", CardKind::ExtraDeck),
            ("Link Monster", CardKind::ExtraDeck),
            ("Spell Card", CardKind::Spell),
            ("Trap Card", CardKind::Trap),
            ("Token", CardKind::Other),
            ("Skill Card", CardKind::Other),
            ("", CardKind::Other),
        ] {
            assert_eq!(kind(card_type), expected, "{card_type}");
        }
    }

    #[test]
    fn set_prefixes() {
        assert_eq!(set_prefix("MAMO-FR023"), Some("MAMO"));
        assert_eq!(set_prefix("LOB-005"), Some("LOB"));
        assert_eq!(set_prefix(" SDK-E001 "), Some("SDK"));
        assert_eq!(set_prefix("LOB"), None);
        assert_eq!(set_prefix("-FR023"), None);
        assert_eq!(set_prefix(""), None);
    }

    #[test]
    fn copies_and_values_by_key() {
        let groups = breakdown([
            ("Rare", Some(dec!(1.50))),
            ("Common", Some(dec!(0.10))),
            ("Rare", None),
            ("Rare", Some(dec!(2.00))),
        ]);
        assert_eq!(
            groups,
            [
                Group {
                    key: "Common",
                    copies: 1,
                    value: dec!(0.10)
                },
                Group {
                    key: "Rare",
                    copies: 3,
                    value: dec!(3.50)
                },
            ]
        );
        assert!(breakdown::<&str>([]).is_empty());
    }
}
