//! Formats (new, not in Django): a named way to play, like Edison or Goat:
//! a **card pool** (the cards released in the TCG on or before a cutoff
//! date) and **its own list** of forbidden and limited cards. A deck
//! version plays a Konami ban list or a format.
//!
//! A card's first TCG release is the earliest TCG date of the sets it was
//! printed in (ygoprodeck's `card_sets`, the sets' dates from its
//! `cardsets.php`); a card with no dated set (OCG only) is in no pool.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde_json::Value;

use crate::ban_list::BanStatus;

/// A set name as both ygoprodeck answers write it once decoded
/// (`Legendary 5D&apos;s Decks` → `Legendary 5D's Decks`).
fn set_name(name: &str) -> String {
    html_escape::decode_html_entities(name.trim()).into_owned()
}

/// Every set of ygoprodeck's `cardsets.php`: `(name, code, TCG date)`.
pub fn set_dates(cardsets: &Value) -> Vec<(String, Option<String>, Option<NaiveDate>)> {
    cardsets
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|set| {
            let name = set_name(set["set_name"].as_str()?);
            let code = set["set_code"].as_str().map(str::to_string);
            let date = set["tcg_date"].as_str().and_then(|d| d.parse().ok());
            Some((name, code, date))
        })
        .filter(|(name, _, _)| !name.is_empty())
        .collect()
}

/// The sets a card was printed in: the names of its card data's
/// `card_sets` (none for an OCG card).
pub fn card_set_names(card: &Value) -> Vec<String> {
    card["card_sets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|set| set["set_name"].as_str())
        .map(set_name)
        .collect()
}

/// A card's first TCG release: the earliest date of `card_sets` (set names,
/// as in ygoprodeck's `card_sets`) in `set_dates`; `None` when none of them
/// has a date.
pub fn first_release(
    card_sets: &[&str],
    set_dates: &HashMap<&str, NaiveDate>,
) -> Option<NaiveDate> {
    card_sets
        .iter()
        .filter_map(|set| set_dates.get(set))
        .min()
        .copied()
}

/// How many copies a status allows: 0, 1, 2, else 3.
pub fn allowed(status: Option<BanStatus>) -> usize {
    match status {
        Some(BanStatus::Banned) => 0,
        Some(BanStatus::Limited) => 1,
        Some(BanStatus::SemiLimited) => 2,
        None => 3,
    }
}

/// One copy in a deck (main, extra or side: they count together).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckCard {
    /// The card's English name: the copies of a card, whatever their
    /// language, count together.
    pub name: String,
    /// `None`: no TCG release known (an OCG card, or not linked yet).
    pub first_release: Option<NaiveDate>,
}

/// What a format allows.
#[derive(Debug, Clone, Default)]
pub struct FormatRules {
    pub cutoff: NaiveDate,
    /// The list, by English card name.
    pub statuses: HashMap<String, BanStatus>,
}

/// Why a card doesn't fit a format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    /// Released after the cutoff.
    TooRecent {
        released: NaiveDate,
    },
    /// No TCG release date known.
    NoRelease,
    TooMany {
        copies: usize,
        allowed: usize,
    },
}

/// A card of the deck that doesn't fit the format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub name: String,
    pub issue: Issue,
}

impl Problem {
    /// For the deck builder: `"Raigeki: 2 copies, 1 allowed (Limited)"`.
    pub fn message(&self) -> String {
        let name = &self.name;
        match &self.issue {
            Issue::TooRecent { released } => format!(
                "{name}: released on {}, after the format's cutoff",
                released.format("%d/%m/%Y")
            ),
            Issue::NoRelease => {
                format!("{name}: no TCG release known (OCG only, or not linked yet)")
            }
            Issue::TooMany { copies, allowed: 0 } => {
                format!("{name}: forbidden ({copies} in the deck)")
            }
            Issue::TooMany { copies, allowed } => {
                format!("{name}: {copies} copies, {allowed} allowed")
            }
        }
    }
}

/// The deck's problems in this format, by card name: a card released after
/// the cutoff (or without a release date), more copies than its status
/// allows. One problem of each kind per card name, however many copies.
pub fn check(deck: &[DeckCard], format: &FormatRules) -> Vec<Problem> {
    let mut by_name: BTreeMap<&str, Vec<&DeckCard>> = BTreeMap::new();
    for card in deck {
        by_name.entry(card.name.as_str()).or_default().push(card);
    }

    let mut problems = Vec::new();
    for (name, copies) in by_name {
        // the copies are the same card: the earliest date known speaks for all
        let released = copies.iter().filter_map(|c| c.first_release).min();
        match released {
            None => problems.push(Problem {
                name: name.to_string(),
                issue: Issue::NoRelease,
            }),
            Some(released) if released > format.cutoff => problems.push(Problem {
                name: name.to_string(),
                issue: Issue::TooRecent { released },
            }),
            Some(_) => {}
        }

        let allowed = allowed(format.statuses.get(name).copied());
        if copies.len() > allowed {
            problems.push(Problem {
                name: name.to_string(),
                issue: Issue::TooMany {
                    copies: copies.len(),
                    allowed,
                },
            });
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    fn card(name: &str, released: Option<&str>) -> DeckCard {
        DeckCard {
            name: name.into(),
            first_release: released.map(date),
        }
    }

    #[test]
    fn ygoprodecks_sets() {
        let sets = serde_json::json!([
            { "set_name": "Legendary 5D&apos;s Decks", "set_code": "L5DD", "tcg_date": "2025-11-06" },
            { "set_name": "An OCG set", "set_code": "OCG1" },
            { "set_code": "NONAME" },
        ]);
        assert_eq!(
            set_dates(&sets),
            [
                (
                    "Legendary 5D's Decks".to_string(),
                    Some("L5DD".to_string()),
                    Some(date("2025-11-06"))
                ),
                ("An OCG set".to_string(), Some("OCG1".to_string()), None),
            ]
        );
        let card = serde_json::json!({ "card_sets": [
            { "set_name": "Legendary 5D's Decks", "set_code": "L5DD-EN001" },
            { "set_name": "Duelist&#039;s Advance" },
        ] });
        assert_eq!(
            card_set_names(&card),
            ["Legendary 5D's Decks", "Duelist's Advance"]
        );
        assert!(card_set_names(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn first_release_is_the_earliest_dated_set() {
        let dates = HashMap::from([
            ("Legend of Blue Eyes White Dragon", date("2002-03-08")),
            ("Starter Deck: Yugi", date("2002-03-29")),
        ]);
        assert_eq!(
            first_release(
                &[
                    "Starter Deck: Yugi",
                    "Legend of Blue Eyes White Dragon",
                    "An OCG set"
                ],
                &dates
            ),
            Some(date("2002-03-08"))
        );
        assert_eq!(first_release(&["An OCG set"], &dates), None);
        assert_eq!(first_release(&[], &dates), None);
    }

    #[test]
    fn checks_the_pool_and_the_counts() {
        let format = FormatRules {
            cutoff: date("2010-03-01"),
            statuses: HashMap::from([
                ("Pot of Greed".to_string(), BanStatus::Banned),
                ("Monster Reborn".to_string(), BanStatus::Limited),
                (
                    "Brionac, Dragon of the Ice Barrier".to_string(),
                    BanStatus::SemiLimited,
                ),
            ]),
        };
        let mut deck = vec![
            card("Pot of Greed", Some("2002-03-08")),
            card("Monster Reborn", Some("2002-03-08")),
            // the main and the side count together
            card("Monster Reborn", Some("2002-03-08")),
            card("Brionac, Dragon of the Ice Barrier", Some("2009-07-14")),
            card("Brionac, Dragon of the Ice Barrier", Some("2009-07-14")),
            card("Sangan", Some("2002-03-08")),
            card("Sangan", Some("2002-03-08")),
            card("Sangan", Some("2002-03-08")),
            card("Ash Blossom & Joyous Spring", Some("2017-02-09")),
            // a reprint known by one copy only: the earliest date wins
            card("Card of Safe Return", None),
            card("Card of Safe Return", Some("2003-04-01")),
            card("An OCG Card", None),
        ];
        let problems = check(&deck, &format);
        assert_eq!(
            problems,
            [
                Problem {
                    name: "An OCG Card".into(),
                    issue: Issue::NoRelease,
                },
                Problem {
                    name: "Ash Blossom & Joyous Spring".into(),
                    issue: Issue::TooRecent {
                        released: date("2017-02-09"),
                    },
                },
                Problem {
                    name: "Monster Reborn".into(),
                    issue: Issue::TooMany {
                        copies: 2,
                        allowed: 1,
                    },
                },
                Problem {
                    name: "Pot of Greed".into(),
                    issue: Issue::TooMany {
                        copies: 1,
                        allowed: 0,
                    },
                },
            ]
        );
        assert_eq!(problems[2].message(), "Monster Reborn: 2 copies, 1 allowed");
        assert_eq!(
            problems[3].message(),
            "Pot of Greed: forbidden (1 in the deck)"
        );
        assert_eq!(
            problems[1].message(),
            "Ash Blossom & Joyous Spring: released on 09/02/2017, after the format's cutoff"
        );

        // a 4th Sangan; released on the cutoff day itself is fine
        deck.push(card("Sangan", Some("2002-03-08")));
        deck.push(card("Gorz the Emissary of Darkness", Some("2010-03-01")));
        let problems = check(&deck, &format);
        assert!(problems.contains(&Problem {
            name: "Sangan".into(),
            issue: Issue::TooMany {
                copies: 4,
                allowed: 3,
            },
        }));
        assert!(!problems.iter().any(|p| p.name.starts_with("Gorz")));
        assert!(check(&[], &format).is_empty());
    }
}
