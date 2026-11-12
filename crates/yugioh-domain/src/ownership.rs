//! Deck versions as **lists** (migration 0027, `store::lists`): a version holds
//! lines (a card and a count), not copies. Whether a line is owned is worked
//! out from the collection; only a **built** version holds copies (and, for
//! the cards it lacks, a "missing" mark).
//!
//! - [`availability`]: one card's copies seen from one list;
//! - [`missing`] and [`missing_for_all`]: what one list, or several sharing
//!   the same copies (the shopping list), lack;
//! - [`condition_rank`]: which copies a built deck takes first
//!   (`built_deck::plan_build`).
//!
//! Written by Claude on the owner's request (2026-11-09), like `format` and
//! `errata`.

use std::collections::HashMap;

use common_domain::CardStatus;

use crate::deck_history::DeckZone;

/// A `card_data.id`.
pub type CardDataId = i64;
/// A `card.id` (an owned copy).
pub type CopyId = i64;
/// A `deck_version.id`.
pub type ListId = i64;

/// One line of a list: `count` copies of a card in a zone
/// (`deck_version_card`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub zone: DeckZone,
    /// Its place in the zone (unique within the zone).
    pub position: u32,
    /// `None`: a card known by its name only (no card data yet): never owned.
    pub card_data_id: Option<CardDataId>,
    pub name: String,
    pub count: u32,
}

impl Line {
    pub fn line_ref(&self) -> LineRef {
        LineRef {
            zone: self.zone,
            position: self.position,
        }
    }
}

/// Which line of a list: its zone and position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineRef {
    pub zone: DeckZone,
    pub position: u32,
}

/// An owned copy of a card (a `card` row that is not a proxy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    pub id: CopyId,
    pub card_data_id: CardDataId,
    /// Its condition (`card.last_known_status`); `None`: not noted.
    pub status: Option<CardStatus>,
    /// Sold (a `sold_card` row): no longer in the collection.
    pub sold: bool,
}

/// How many copies of one card a list can count on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Availability {
    /// The copies in the collection (not sold).
    pub owned: usize,
    /// Owned and locked by no built version: free to use or to build with.
    pub free: usize,
    /// Locked by the list being looked at (when it is built).
    pub here: usize,
    /// Locked by another built version.
    pub elsewhere: usize,
}

/// The copies of **one** card data, and the built versions locking some of
/// them (`locks`: copy → the built version holding it), seen from `list`.
///
/// Rules: a sold copy doesn't count (even if a built version still holds
/// it); a copy locked by `list` itself is `here`, not `elsewhere`;
/// `owned = free + here + elsewhere`.
pub fn availability(
    copies: &[Copy],
    locks: &HashMap<CopyId, ListId>,
    list: ListId,
) -> Availability {
    let mut a = Availability::default();
    for copy in copies.iter().filter(|c| !c.sold) {
        a.owned += 1;
        match locks.get(&copy.id) {
            None => a.free += 1,
            Some(&holder) if holder == list => a.here += 1,
            Some(_) => a.elsewhere += 1,
        }
    }
    a
}

/// The order in which copies are taken for a deck: best condition first
/// (Mint, Near Mint, unknown, Excellent, Good, Poor), then the oldest copy
/// (lowest id). Used by [`crate::built_deck::plan_build`].
///
/// Unknown sits with Near Mint: most copies bought as singles are, and a
/// condition not noted is worth 1 (`condition_factor`).
pub fn condition_rank(status: Option<CardStatus>) -> u8 {
    match status {
        Some(CardStatus::Mint) => 0,
        Some(CardStatus::NearMint) | None => 1,
        Some(CardStatus::Excellent) => 2,
        Some(CardStatus::Good) => 3,
        Some(CardStatus::Poor) => 4,
    }
}

/// A line short of copies: `count` more are needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingLine {
    pub line: LineRef,
    pub card_data_id: Option<CardDataId>,
    pub name: String,
    pub count: u32,
}

/// What one list lacks, given the copies free for it (`free`: card data →
/// count; a card data absent = none).
///
/// Lines of the same card (main and side) share its free copies, in the
/// order of `lines`; a name-only line (no card data) lacks its whole count;
/// a line with enough copies is not in the answer.
pub fn missing(lines: &[Line], free: &HashMap<CardDataId, usize>) -> Vec<MissingLine> {
    take(lines, &mut free.clone())
}

/// The shopping list's rule: what several lists lack together, when they
/// share the same owned copies (`owned`: card data → count, the copies
/// locked by built versions included).
///
/// `lists` come in priority order (built first, then planned, as the owner
/// sorts them): each takes from what the ones before left. Two planned decks
/// that both play 3 *Ash Blossom* when you own 1 need 5 more, not 4. A list
/// lacking nothing still has an entry (empty).
///
/// A built version's own copies are in `owned`, so a copy owned but not yet
/// put in it ("Use my copy") is not to buy.
pub fn missing_for_all(
    lists: &[(ListId, Vec<Line>)],
    owned: &HashMap<CardDataId, usize>,
) -> HashMap<ListId, Vec<MissingLine>> {
    let mut pool = owned.clone();
    lists
        .iter()
        .map(|(list, lines)| (*list, take(lines, &mut pool)))
        .collect()
}

/// The lines take their copies from `pool` in order (it keeps what is
/// left); the answer is what they still lack.
fn take(lines: &[Line], pool: &mut HashMap<CardDataId, usize>) -> Vec<MissingLine> {
    let mut short = Vec::new();
    for line in lines {
        let wanted = line.count as usize;
        let taken = match line.card_data_id.and_then(|id| pool.get_mut(&id)) {
            Some(left) => {
                let taken = wanted.min(*left);
                *left -= taken;
                taken
            }
            None => 0,
        };
        if taken < wanted {
            short.push(MissingLine {
                line: line.line_ref(),
                card_data_id: line.card_data_id,
                name: line.name.clone(),
                count: (wanted - taken) as u32,
            });
        }
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASH: CardDataId = 10;
    const NIBIRU: CardDataId = 20;

    fn copy(id: CopyId, card_data_id: CardDataId, status: Option<CardStatus>, sold: bool) -> Copy {
        Copy {
            id,
            card_data_id,
            status,
            sold,
        }
    }

    fn line(zone: DeckZone, position: u32, card_data_id: Option<CardDataId>, count: u32) -> Line {
        Line {
            zone,
            position,
            card_data_id,
            name: format!("card {card_data_id:?}"),
            count,
        }
    }

    fn short(l: &Line, count: u32) -> MissingLine {
        MissingLine {
            line: l.line_ref(),
            card_data_id: l.card_data_id,
            name: l.name.clone(),
            count,
        }
    }

    // --- availability

    #[test]
    fn no_copy_is_nothing() {
        assert_eq!(
            availability(&[], &HashMap::new(), 1),
            Availability::default()
        );
    }

    #[test]
    fn unlocked_copies_are_free() {
        let copies = [copy(1, ASH, None, false), copy(2, ASH, None, false)];
        let a = availability(&copies, &HashMap::new(), 1);
        assert_eq!(
            a,
            Availability {
                owned: 2,
                free: 2,
                here: 0,
                elsewhere: 0
            }
        );
    }

    #[test]
    fn sold_copies_dont_count_even_locked() {
        let copies = [copy(1, ASH, None, true), copy(2, ASH, None, false)];
        let locks = HashMap::from([(1, 7)]);
        let a = availability(&copies, &locks, 1);
        assert_eq!(
            a,
            Availability {
                owned: 1,
                free: 1,
                here: 0,
                elsewhere: 0
            }
        );
    }

    #[test]
    fn locks_here_and_elsewhere() {
        let copies = [
            copy(1, ASH, None, false),
            copy(2, ASH, None, false),
            copy(3, ASH, None, false),
        ];
        let locks = HashMap::from([(1, 5), (2, 7)]);
        let a = availability(&copies, &locks, 5);
        assert_eq!(
            a,
            Availability {
                owned: 3,
                free: 1,
                here: 1,
                elsewhere: 1
            }
        );
        assert_eq!(a.owned, a.free + a.here + a.elsewhere);
    }

    // --- condition_rank

    #[test]
    fn best_condition_first() {
        use CardStatus::*;
        let order = [
            Some(Mint),
            Some(NearMint),
            None,
            Some(Excellent),
            Some(Good),
            Some(Poor),
        ];
        let ranks: Vec<u8> = order.iter().map(|s| condition_rank(*s)).collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{ranks:?}");
        assert!(condition_rank(Some(Mint)) < condition_rank(Some(Excellent)));
        assert!(condition_rank(Some(Good)) < condition_rank(Some(Poor)));
    }

    // --- missing

    #[test]
    fn enough_copies_lack_nothing() {
        let lines = [line(DeckZone::Main, 0, Some(ASH), 3)];
        assert_eq!(missing(&lines, &HashMap::from([(ASH, 3)])), vec![]);
        assert_eq!(missing(&lines, &HashMap::from([(ASH, 5)])), vec![]);
    }

    #[test]
    fn a_line_partly_owned() {
        let lines = [
            line(DeckZone::Main, 0, Some(ASH), 3),
            line(DeckZone::Main, 1, Some(NIBIRU), 2),
        ];
        let got = missing(&lines, &HashMap::from([(ASH, 1)]));
        assert_eq!(got, vec![short(&lines[0], 2), short(&lines[1], 2)]);
    }

    #[test]
    fn lines_of_one_card_share_its_copies_in_order() {
        // 2 in the main deck, 2 in the side: 3 owned fill the main first
        let lines = [
            line(DeckZone::Main, 0, Some(ASH), 2),
            line(DeckZone::Side, 0, Some(ASH), 2),
        ];
        let got = missing(&lines, &HashMap::from([(ASH, 3)]));
        assert_eq!(got, vec![short(&lines[1], 1)]);
    }

    #[test]
    fn a_name_only_line_lacks_everything() {
        let lines = [line(DeckZone::Extra, 0, None, 2)];
        assert_eq!(missing(&lines, &HashMap::new()), vec![short(&lines[0], 2)]);
    }

    // --- missing_for_all

    #[test]
    fn lists_take_in_turn() {
        // the spec's example: 3 Ash in two decks, 1 owned: 2 + 3 to buy
        let a = vec![line(DeckZone::Main, 0, Some(ASH), 3)];
        let b = vec![line(DeckZone::Main, 4, Some(ASH), 3)];
        let got = missing_for_all(
            &[(1, a.clone()), (2, b.clone())],
            &HashMap::from([(ASH, 1)]),
        );
        assert_eq!(got[&1], vec![short(&a[0], 2)]);
        assert_eq!(got[&2], vec![short(&b[0], 3)]);
    }

    #[test]
    fn the_first_list_is_served_first() {
        let a = vec![line(DeckZone::Main, 0, Some(ASH), 2)];
        let b = vec![line(DeckZone::Main, 0, Some(ASH), 2)];
        let owned = HashMap::from([(ASH, 3)]);
        let got = missing_for_all(&[(2, b.clone()), (1, a.clone())], &owned);
        assert_eq!(got[&2], vec![]);
        assert_eq!(got[&1], vec![short(&a[0], 1)]);
    }

    #[test]
    fn every_list_has_an_entry() {
        let a = vec![line(DeckZone::Main, 0, Some(NIBIRU), 1)];
        let got = missing_for_all(&[(1, a), (2, vec![])], &HashMap::from([(NIBIRU, 1)]));
        assert_eq!(got.len(), 2);
        assert!(got[&1].is_empty() && got[&2].is_empty());
    }
}
