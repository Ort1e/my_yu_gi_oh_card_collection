//! Opening-hand simulation (Django: `monte_carlos/monte_carlos.py`).
//!
//! Draws `hand_size` cards from the main deck `simulations` times and counts,
//! per category, how often its cards show up. A conditional category resolves
//! to its `then` category when one of the *other* drawn cards belongs to one of
//! its `if_any_of` categories, otherwise to `otherwise` (if any); the resolved
//! category is counted as well.

use std::collections::{BTreeMap, HashMap, HashSet};

use rand::seq::SliceRandom;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone)]
pub struct Condition {
    pub if_any_of: HashSet<i64>,
    pub then: i64,
    pub otherwise: Option<i64>,
}

impl Condition {
    fn resolve(&self, other_categories: &HashSet<i64>) -> Option<i64> {
        if self.if_any_of.iter().any(|c| other_categories.contains(c)) {
            Some(self.then)
        } else {
            self.otherwise
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SimulationResult {
    pub total_simulations: u32,
    pub categories: Vec<CategoryStats>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CategoryStats {
    pub category_id: i64,
    pub name: String,
    /// Number of main-deck cards assigned to this category.
    pub cards_in_deck: u32,
    /// Total hits over all simulations.
    pub occurrences: u32,
    /// cards of this category in a hand -> number of hands.
    pub per_hand: BTreeMap<u32, u32>,
}

/// `assignments` are `(card_id, category_id)` pairs; ones whose category is
/// not in `categories` are ignored. Results keep the order of `categories`.
pub fn simulate(
    main_deck: &[i64],
    categories: &[Category],
    assignments: &[(i64, i64)],
    hand_size: usize,
    simulations: u32,
    rng: &mut impl Rng,
) -> SimulationResult {
    let index: HashMap<i64, usize> = categories
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    let mut card_categories: HashMap<i64, Vec<usize>> = HashMap::new();
    let mut cards_in_deck = vec![0u32; categories.len()];
    for &(card_id, category_id) in assignments {
        if let Some(&i) = index.get(&category_id) {
            card_categories.entry(card_id).or_default().push(i);
            cards_in_deck[i] += 1;
        }
    }
    let no_categories = Vec::new();
    let categories_of = |card: i64| card_categories.get(&card).unwrap_or(&no_categories);

    let mut occurrences = vec![0u32; categories.len()];
    let mut per_hand: Vec<BTreeMap<u32, u32>> = vec![BTreeMap::new(); categories.len()];
    let hand_size = hand_size.min(main_deck.len());

    for _ in 0..simulations {
        let hand: Vec<i64> = main_deck
            .choose_multiple(rng, hand_size)
            .copied()
            .collect();
        let mut in_hand = vec![0u32; categories.len()];

        for (pos, &card) in hand.iter().enumerate() {
            let mut others: Option<HashSet<i64>> = None;
            for &i in categories_of(card) {
                in_hand[i] += 1;

                let Some(condition) = &categories[i].condition else {
                    continue;
                };
                let others = others.get_or_insert_with(|| {
                    hand.iter()
                        .enumerate()
                        .filter(|&(p, _)| p != pos)
                        .flat_map(|(_, &c)| categories_of(c))
                        .map(|&j| categories[j].id)
                        .collect()
                });
                if let Some(&j) = condition.resolve(others).and_then(|id| index.get(&id)) {
                    in_hand[j] += 1;
                }
            }
        }

        for (i, &n) in in_hand.iter().enumerate() {
            occurrences[i] += n;
            *per_hand[i].entry(n).or_default() += 1;
        }
    }

    SimulationResult {
        total_simulations: simulations,
        categories: categories
            .iter()
            .zip(per_hand)
            .enumerate()
            .map(|(i, (c, per_hand))| CategoryStats {
                category_id: c.id,
                name: c.name.clone(),
                cards_in_deck: cards_in_deck[i],
                occurrences: occurrences[i],
                per_hand,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn category(id: i64, name: &str) -> Category {
        Category {
            id,
            name: name.into(),
            condition: None,
        }
    }

    #[test]
    fn whole_deck_drawn_counts_every_assignment() {
        let deck = [1, 2, 3];
        let cats = [category(10, "starter"), category(11, "extender")];
        let assignments = [(1, 10), (2, 10), (3, 11)];
        let mut rng = StdRng::seed_from_u64(1);

        let r = simulate(&deck, &cats, &assignments, 5, 100, &mut rng);
        assert_eq!(r.total_simulations, 100);
        assert_eq!(r.categories[0].cards_in_deck, 2);
        assert_eq!(r.categories[0].occurrences, 200);
        assert_eq!(r.categories[0].per_hand, BTreeMap::from([(2, 100)]));
        assert_eq!(r.categories[1].occurrences, 100);
    }

    #[test]
    fn conditional_resolves_against_other_cards() {
        // card 1 is "conditional": a starter only if another card is a name
        let cats = [
            Category {
                id: 1,
                name: "cond".into(),
                condition: Some(Condition {
                    if_any_of: HashSet::from([3]),
                    then: 2,
                    otherwise: Some(4),
                }),
            },
            category(2, "starter"),
            category(3, "name"),
            category(4, "brick"),
        ];
        let mut rng = StdRng::seed_from_u64(7);

        let with_name = simulate(&[100, 200], &cats, &[(100, 1), (200, 3)], 2, 10, &mut rng);
        assert_eq!(with_name.categories[1].occurrences, 10);
        assert_eq!(with_name.categories[3].occurrences, 0);

        let alone = simulate(&[100, 300], &cats, &[(100, 1)], 2, 10, &mut rng);
        assert_eq!(alone.categories[1].occurrences, 0);
        assert_eq!(alone.categories[3].occurrences, 10);
    }

    #[test]
    fn partial_hands_are_distributed() {
        let deck: Vec<i64> = (1..=40).collect();
        let cats = [category(1, "starter")];
        let assignments: Vec<(i64, i64)> = (1..=10).map(|c| (c, 1)).collect();
        let mut rng = StdRng::seed_from_u64(42);

        let r = simulate(&deck, &cats, &assignments, 5, 2000, &mut rng);
        let stats = &r.categories[0];
        assert_eq!(stats.per_hand.values().sum::<u32>(), 2000);
        // expected value 5 * 10/40 = 1.25 per hand
        let mean = stats.occurrences as f64 / 2000.0;
        assert!((mean - 1.25).abs() < 0.1, "mean {mean}");
    }

    #[test]
    fn serializes_for_the_api() {
        let mut rng = StdRng::seed_from_u64(0);
        let r = simulate(&[1], &[category(5, "a")], &[(1, 5)], 1, 1, &mut rng);
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["categories"][0]["per_hand"]["1"], 1);
    }
}
