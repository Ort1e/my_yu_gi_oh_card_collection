//! Built decks (`store::lists`, migration 0027): a built deck version holds
//! copies of the collection, which then belong to it alone. Building takes
//! free copies (held by no built version), best condition first; the lines
//! still short are missing. The other versions are lists: they hold nothing,
//! so building never touches them. A copy bought later goes in by hand
//! ("Take free copies") or with the next save of the built version.
//!
//! New in the Rust app (Django had no built decks).

use std::collections::{HashMap, VecDeque};

use crate::ownership::{condition_rank, CardDataId, Copy, CopyId, Line, LineRef};

/// What building a list does: which free copy fills which line, and how
/// many each line still lacks. Other versions are never touched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildPlan {
    /// `(line, copy)`, a row per copy, lines in the order given, a line's
    /// copies in the order they are taken.
    pub assigned: Vec<(LineRef, CopyId)>,
    /// `(line, count)`: the lines short of copies ("missing", to buy), in
    /// the order given; a line filled is not here.
    pub missing: Vec<(LineRef, u32)>,
}

/// Building a list (written by Claude on the owner's request, 2026-11-09).
///
/// - `lines`: the version's lines, in zone order;
/// - `free`: for a card data, its free copies (owned, not sold, locked by no
///   built version), in any order.
///
/// Each line takes up to its count of its card's free copies, best first
/// (`ownership::condition_rank`, then the lowest id); lines of the same card
/// share them in the order of `lines`; a name-only line gets none.
/// Deterministic: the same input gives the same plan.
pub fn plan_build(lines: &[Line], free: &HashMap<CardDataId, Vec<Copy>>) -> BuildPlan {
    let mut plan = BuildPlan::default();
    // each card's copies, best first; taken from the front
    let mut queues: HashMap<CardDataId, VecDeque<CopyId>> = free
        .iter()
        .map(|(card_data_id, copies)| {
            let mut copies: Vec<&Copy> = copies.iter().collect();
            copies.sort_by_key(|c| (condition_rank(c.status), c.id));
            (*card_data_id, copies.into_iter().map(|c| c.id).collect())
        })
        .collect();

    for line in lines {
        let at = line.line_ref();
        let mut lacking = line.count;
        if let Some(queue) = line.card_data_id.and_then(|id| queues.get_mut(&id)) {
            while lacking > 0 {
                let Some(copy) = queue.pop_front() else { break };
                plan.assigned.push((at, copy));
                lacking -= 1;
            }
        }
        if lacking > 0 {
            plan.missing.push((at, lacking));
        }
    }
    plan
}

#[cfg(test)]
mod tests {
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
    fn enough_copies_fill_the_line() {
        let lines = [line(DeckZone::Main, 0, Some(ASH), 2)];
        let free = HashMap::from([(ASH, vec![copy(1, None), copy(2, None), copy(3, None)])]);
        let plan = plan_build(&lines, &free);
        let at = lines[0].line_ref();
        assert_eq!(plan.assigned, vec![(at, 1), (at, 2)]);
        assert_eq!(plan.missing, vec![]);
    }

    #[test]
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
        let plan = plan_build(&lines, &free);
        let at = lines[0].line_ref();
        assert_eq!(plan.assigned, vec![(at, 4), (at, 9)]);
    }

    #[test]
    fn a_line_partly_filled_and_the_next_one_missing() {
        let lines = [
            line(DeckZone::Main, 0, Some(ASH), 2),
            line(DeckZone::Side, 0, Some(ASH), 1),
            line(DeckZone::Extra, 3, None, 1),
        ];
        let free = HashMap::from([(ASH, vec![copy(1, None)])]);
        let plan = plan_build(&lines, &free);
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
    fn the_same_input_gives_the_same_plan() {
        let lines = [line(DeckZone::Main, 0, Some(ASH), 2)];
        let a = HashMap::from([(ASH, vec![copy(3, None), copy(1, None), copy(2, None)])]);
        let b = HashMap::from([(ASH, vec![copy(2, None), copy(3, None), copy(1, None)])]);
        assert_eq!(plan_build(&lines, &a), plan_build(&lines, &b));
    }
}
