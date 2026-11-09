//! Built decks: a built deck version **locks** its real (non-proxy) cards,
//! which then belong to it alone. Building a version takes its cards from
//! the other versions (they get a proxy instead), except the cards another
//! built version already holds: first built, first served.
//!
//! A built version's proxies are never replaced automatically by owned
//! copies (`store::cards::replace_proxies` skips it); the deck builder's
//! "Use my copy" does it by hand.
//!
//! New in the Rust app (Django had no built decks).

use std::collections::{HashMap, HashSet};

use crate::ownership::{CardDataId, Copy, CopyId, Line, LineRef};

/// What building a version changes (applied by `PATCH
/// /api/deck-versions/:id` with `{"is_built": true}`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildPlan {
    /// The cards of the version locked by another built version: they get a
    /// proxy in the version being built, in the order of `cards`.
    pub proxied_here: Vec<i64>,
    /// `(card, other version)`: the version being built keeps the card, the
    /// other version gets a proxy of it. In the order of `cards`, then by
    /// version id.
    pub proxied_elsewhere: Vec<(i64, i64)>,
}

/// The plan for building a version.
///
/// - `cards`: the version's real (non-proxy) cards, in zone order (an id may
///   repeat: count it once);
/// - `locked_elsewhere`: the cards already locked by another built version;
/// - `used_elsewhere`: for a card, the other versions it is in (built or not,
///   in any order, possibly repeated).
///
/// A card locked elsewhere is proxied here, and only here (the built version
/// holding it keeps it). Every other card that is in other versions is
/// proxied in each of them. Cards in no other version change nothing.
pub fn plan_build(
    cards: &[i64],
    locked_elsewhere: &HashSet<i64>,
    used_elsewhere: &HashMap<i64, Vec<i64>>,
) -> BuildPlan {
    let mut plan = BuildPlan::default();
    let mut seen = HashSet::new();

    for &card in cards {
        // `insert` is false when the id was already there: a repeated card
        if !seen.insert(card) {
            continue;
        }

        // held by another built version: the proxy goes here, nowhere else
        if locked_elsewhere.contains(&card) {
            plan.proxied_here.push(card);
            continue;
        }

        let Some(versions) = used_elsewhere.get(&card) else {
            continue;
        };
        let mut versions = versions.clone();
        versions.sort_unstable();
        versions.dedup();
        plan.proxied_elsewhere
            .extend(versions.into_iter().map(|version| (card, version)));
    }

    plan
}

/// What building a **list** does (`docs/specs/deck-lists.md`, step 3; it
/// replaces [`plan_build`] then): which free copy fills which line, and how
/// many each line still lacks. Other versions are never touched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListBuildPlan {
    /// `(line, copy)`, a row per copy, lines in the order given, a line's
    /// copies in the order they are taken.
    pub assigned: Vec<(LineRef, CopyId)>,
    /// `(line, count)`: the lines short of copies ("missing", to buy), in
    /// the order given; a line filled is not here.
    pub missing: Vec<(LineRef, u32)>,
}

/// The owner's (stub): building a list.
///
/// - `lines`: the version's lines, in zone order;
/// - `free`: for a card data, its free copies (owned, not sold, locked by no
///   built version), in any order.
///
/// Each line takes up to its count of its card's free copies, best first
/// (`ownership::condition_rank`, then the lowest id); lines of the same card
/// share them in the order of `lines`; a name-only line gets none.
/// Deterministic: the same input gives the same plan.
pub fn plan_list_build(lines: &[Line], free: &HashMap<CardDataId, Vec<Copy>>) -> ListBuildPlan {
    let _ = (lines, free);
    todo!("the owner's: see the tests")
}

#[cfg(test)]
mod list_tests {
    use common_domain::CardStatus;

    use super::*;
    use crate::deck_history::DeckZone;

    const ASH: CardDataId = 10;

    fn line(zone: DeckZone, position: u32, card_data_id: Option<CardDataId>, count: u32) -> Line {
        Line {
            zone,
            position,
            card_data_id,
            name: "card".to_string(),
            count,
        }
    }

    fn copy(id: CopyId, status: Option<CardStatus>) -> Copy {
        Copy {
            id,
            card_data_id: ASH,
            status,
            sold: false,
        }
    }

    #[test]
    #[ignore = "the owner's plan_list_build"]
    fn enough_copies_fill_the_line() {
        let lines = [line(DeckZone::Main, 0, Some(ASH), 2)];
        let free = HashMap::from([(ASH, vec![copy(1, None), copy(2, None), copy(3, None)])]);
        let plan = plan_list_build(&lines, &free);
        let at = lines[0].line_ref();
        assert_eq!(plan.assigned, vec![(at, 1), (at, 2)]);
        assert_eq!(plan.missing, vec![]);
    }

    #[test]
    #[ignore = "the owner's plan_list_build"]
    fn best_condition_first_then_oldest() {
        use CardStatus::*;
        let lines = [line(DeckZone::Main, 0, Some(ASH), 2)];
        let free = HashMap::from([(
            ASH,
            vec![
                copy(5, Some(Good)),
                copy(9, Some(Mint)),
                copy(4, Some(Mint)),
            ],
        )]);
        let plan = plan_list_build(&lines, &free);
        let at = lines[0].line_ref();
        assert_eq!(plan.assigned, vec![(at, 4), (at, 9)]);
    }

    #[test]
    #[ignore = "the owner's plan_list_build"]
    fn a_line_partly_filled_and_the_next_one_missing() {
        let lines = [
            line(DeckZone::Main, 0, Some(ASH), 2),
            line(DeckZone::Side, 0, Some(ASH), 1),
            line(DeckZone::Extra, 3, None, 1),
        ];
        let free = HashMap::from([(ASH, vec![copy(1, None)])]);
        let plan = plan_list_build(&lines, &free);
        assert_eq!(plan.assigned, vec![(lines[0].line_ref(), 1)]);
        assert_eq!(
            plan.missing,
            vec![
                (lines[0].line_ref(), 1),
                (lines[1].line_ref(), 1),
                (lines[2].line_ref(), 1),
            ]
        );
    }

    #[test]
    #[ignore = "the owner's plan_list_build"]
    fn the_same_input_gives_the_same_plan() {
        let lines = [line(DeckZone::Main, 0, Some(ASH), 2)];
        let a = HashMap::from([(ASH, vec![copy(3, None), copy(1, None), copy(2, None)])]);
        let b = HashMap::from([(ASH, vec![copy(2, None), copy(3, None), copy(1, None)])]);
        assert_eq!(plan_list_build(&lines, &a), plan_list_build(&lines, &b));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locked(ids: &[i64]) -> HashSet<i64> {
        ids.iter().copied().collect()
    }

    fn used(pairs: &[(i64, &[i64])]) -> HashMap<i64, Vec<i64>> {
        pairs
            .iter()
            .map(|(card, versions)| (*card, versions.to_vec()))
            .collect()
    }

    #[test]
    fn cards_in_no_other_version_change_nothing() {
        let plan = plan_build(&[1, 2, 3], &locked(&[]), &used(&[]));
        assert_eq!(plan, BuildPlan::default());
        assert_eq!(
            plan_build(&[], &locked(&[]), &used(&[])),
            BuildPlan::default()
        );
    }

    #[test]
    fn shared_cards_get_a_proxy_in_the_other_versions() {
        let plan = plan_build(
            &[1, 2, 3],
            &locked(&[]),
            &used(&[(2, &[30, 10]), (3, &[20])]),
        );
        assert_eq!(plan.proxied_here, Vec::<i64>::new());
        assert_eq!(plan.proxied_elsewhere, vec![(2, 10), (2, 30), (3, 20)]);
    }

    #[test]
    fn a_card_locked_elsewhere_is_proxied_here_only() {
        // card 2 is held by built version 10: this version gets the proxy,
        // and version 20 (not built) keeps its copy of it too
        let plan = plan_build(&[1, 2], &locked(&[2]), &used(&[(1, &[20]), (2, &[10, 20])]));
        assert_eq!(plan.proxied_here, vec![2]);
        assert_eq!(plan.proxied_elsewhere, vec![(1, 20)]);
    }

    #[test]
    fn the_order_follows_the_cards() {
        let plan = plan_build(
            &[5, 3, 4, 1],
            &locked(&[4, 1]),
            &used(&[(5, &[9]), (3, &[7]), (4, &[8]), (1, &[8])]),
        );
        assert_eq!(plan.proxied_here, vec![4, 1]);
        assert_eq!(plan.proxied_elsewhere, vec![(5, 9), (3, 7)]);
    }

    #[test]
    fn repeated_ids_count_once() {
        let plan = plan_build(
            &[1, 1, 2, 2],
            &locked(&[2]),
            &used(&[(1, &[10, 10]), (2, &[10])]),
        );
        assert_eq!(plan.proxied_here, vec![2]);
        assert_eq!(plan.proxied_elsewhere, vec![(1, 10)]);
    }
}
