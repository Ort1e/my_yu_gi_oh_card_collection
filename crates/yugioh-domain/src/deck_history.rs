//! A deck's history (new, not in Django): **snapshots** of what a deck
//! version's zones held, by card (card data, or the name of a card not
//! linked yet), not by physical copy. One is written each time a version's
//! zones are saved; the important ones get a name or a tournament.
//!
//! Written by Claude on the owner's request (2026-11-07), like `format` and
//! `errata`.
//!
//! - [`SnapshotList::from_cards`] groups a zone's copies into counts;
//! - [`diff`] tells what changed between two lists;
//! - [`coalesce`] decides whether a save writes a new snapshot, replaces the
//!   last one (a building session gives one snapshot, not twenty) or none.

use std::collections::BTreeMap;

use chrono::{NaiveDateTime, TimeDelta};

/// The zones of a deck, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeckZone {
    Main,
    Extra,
    Side,
}

impl DeckZone {
    pub const ALL: [DeckZone; 3] = [DeckZone::Main, DeckZone::Extra, DeckZone::Side];

    /// The word stored in `deck_snapshot_card.zone`.
    pub fn as_str(self) -> &'static str {
        match self {
            DeckZone::Main => "main",
            DeckZone::Extra => "extra",
            DeckZone::Side => "side",
        }
    }

    pub fn parse(s: &str) -> Option<DeckZone> {
        DeckZone::ALL.into_iter().find(|z| z.as_str() == s)
    }

    /// `Main deck`, for the pages.
    pub fn label(self) -> &'static str {
        match self {
            DeckZone::Main => "Main deck",
            DeckZone::Extra => "Extra deck",
            DeckZone::Side => "Side deck",
        }
    }
}

/// One card of a snapshot: how many copies of it a zone held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub zone: DeckZone,
    pub card_data_id: Option<i64>,
    pub name: String,
    pub count: u32,
}

impl SnapshotEntry {
    fn key(&self) -> CardKey {
        card_key(self.card_data_id, &self.name)
    }
}

/// What makes two copies "the same card": their card data, else (not
/// linked yet) their name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum CardKey {
    Data(i64),
    Name(String),
}

fn card_key(card_data_id: Option<i64>, name: &str) -> CardKey {
    match card_data_id {
        Some(id) => CardKey::Data(id),
        None => CardKey::Name(name.to_string()),
    }
}

/// A snapshot's cards, zone by zone, each card once with its count, in the
/// order of their first copy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotList {
    pub entries: Vec<SnapshotEntry>,
}

impl SnapshotList {
    /// The list of copies `(zone, card data, name)`, grouped.
    pub fn from_cards<'a>(
        cards: impl IntoIterator<Item = (DeckZone, Option<i64>, &'a str)>,
    ) -> SnapshotList {
        let mut entries: Vec<SnapshotEntry> = Vec::new();
        let mut index: BTreeMap<(DeckZone, CardKey), usize> = BTreeMap::new();
        for (zone, card_data_id, name) in cards {
            let key = (zone, card_key(card_data_id, name));
            match index.get(&key) {
                Some(&i) => entries[i].count += 1,
                None => {
                    index.insert(key, entries.len());
                    entries.push(SnapshotEntry {
                        zone,
                        card_data_id,
                        name: name.to_string(),
                        count: 1,
                    });
                }
            }
        }
        // zones in order, each keeping its first-seen order
        entries.sort_by_key(|e| e.zone);
        SnapshotList { entries }
    }

    /// The count of every card, by zone (order left aside).
    fn counts(&self) -> BTreeMap<(DeckZone, CardKey), u32> {
        let mut counts = BTreeMap::new();
        for e in &self.entries {
            *counts.entry((e.zone, e.key())).or_insert(0) += e.count;
        }
        counts
    }

    /// The same cards in the same zones, whatever their order.
    pub fn same_cards(&self, other: &SnapshotList) -> bool {
        self.counts() == other.counts()
    }

    /// How many copies a zone holds.
    pub fn zone_count(&self, zone: DeckZone) -> u32 {
        self.entries
            .iter()
            .filter(|e| e.zone == zone)
            .map(|e| e.count)
            .sum()
    }

    pub fn zone(&self, zone: DeckZone) -> impl Iterator<Item = &SnapshotEntry> {
        self.entries.iter().filter(move |e| e.zone == zone)
    }
}

/// A card whose count changed in a zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub zone: DeckZone,
    pub card_data_id: Option<i64>,
    pub name: String,
    pub before: u32,
    pub after: u32,
}

impl Change {
    /// `+2`, `-1`.
    pub fn delta(&self) -> i64 {
        i64::from(self.after) - i64::from(self.before)
    }

    pub fn added(&self) -> bool {
        self.after > self.before
    }
}

/// What changed from `old` to `new`: by zone (main, extra, side), the cards
/// added first then the ones taken out, each by name. A card moved from the
/// main deck to the side deck is two changes.
pub fn diff(old: &SnapshotList, new: &SnapshotList) -> Vec<Change> {
    let (before, after) = (old.counts(), new.counts());
    // the newest name of a card (a name can be translated in between)
    let mut names: BTreeMap<(DeckZone, CardKey), (Option<i64>, &str)> = BTreeMap::new();
    for e in old.entries.iter().chain(&new.entries) {
        names.insert((e.zone, e.key()), (e.card_data_id, &e.name));
    }

    let mut changes: Vec<Change> = names
        .into_iter()
        .filter_map(|(key, (card_data_id, name))| {
            let b = before.get(&key).copied().unwrap_or(0);
            let a = after.get(&key).copied().unwrap_or(0);
            (a != b).then(|| Change {
                zone: key.0,
                card_data_id,
                name: name.to_string(),
                before: b,
                after: a,
            })
        })
        .collect();
    changes.sort_by(|x, y| {
        (x.zone, !x.added(), x.name.to_lowercase()).cmp(&(
            y.zone,
            !y.added(),
            y.name.to_lowercase(),
        ))
    });
    changes
}

/// `+5 −3`: the copies added and taken out, all zones together.
pub fn summary(changes: &[Change]) -> (u32, u32) {
    changes.iter().fold((0, 0), |(plus, minus), c| {
        if c.added() {
            (plus + c.after - c.before, minus)
        } else {
            (plus, minus + c.before - c.after)
        }
    })
}

/// Saves of a version this close to its last unnamed snapshot replace it.
pub const SESSION: TimeDelta = TimeDelta::minutes(30);

/// The last snapshot of the version being saved, as [`coalesce`] needs it.
#[derive(Debug, Clone)]
pub struct LastSnapshot {
    pub created_at: NaiveDateTime,
    /// Named, linked to a tournament or public: never replaced.
    pub kept: bool,
    pub cards: SnapshotList,
}

/// What a save of a version's zones does to its history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveAction {
    /// The same cards as its last snapshot: nothing to write.
    Skip,
    /// The last snapshot is an unnamed one from the same session: it gets
    /// the new cards and time.
    Replace,
    /// A new snapshot.
    Append,
}

/// Whether saving `cards` at `now` writes a snapshot, given the version's
/// last one (`None`: it has none yet).
pub fn coalesce(
    last: Option<&LastSnapshot>,
    cards: &SnapshotList,
    now: NaiveDateTime,
) -> SaveAction {
    let Some(last) = last else {
        return SaveAction::Append;
    };

    if last.cards.same_cards(cards) {
        return SaveAction::Skip;
    }

    let recent = now.signed_duration_since(last.created_at) < SESSION;
    if recent && !last.kept {
        SaveAction::Replace
    } else {
        SaveAction::Append
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DeckZone::*;

    fn list(cards: &[(DeckZone, Option<i64>, &str)]) -> SnapshotList {
        SnapshotList::from_cards(cards.iter().map(|&(z, id, n)| (z, id, n)))
    }

    fn at(h: u32, m: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 7)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn groups_copies_by_card_in_first_seen_order() {
        let l = list(&[
            (Side, Some(3), "Ash"),
            (Main, Some(1), "Raigeki"),
            (Main, Some(2), "Pot"),
            (Main, Some(1), "Raigeki"),
            (Main, None, "Unlinked"),
            (Main, None, "Unlinked"),
        ]);
        let shown: Vec<(DeckZone, &str, u32)> = l
            .entries
            .iter()
            .map(|e| (e.zone, e.name.as_str(), e.count))
            .collect();
        assert_eq!(
            shown,
            [
                (Main, "Raigeki", 2),
                (Main, "Pot", 1),
                (Main, "Unlinked", 2),
                (Side, "Ash", 1)
            ]
        );
        assert_eq!(l.zone_count(Main), 5);
        assert_eq!(l.zone_count(Extra), 0);
    }

    #[test]
    fn same_cards_ignores_order() {
        let a = list(&[(Main, Some(1), "A"), (Main, Some(2), "B")]);
        let b = list(&[(Main, Some(2), "B"), (Main, Some(1), "A")]);
        assert!(a.same_cards(&b));
        let c = list(&[(Side, Some(2), "B"), (Main, Some(1), "A")]);
        assert!(!a.same_cards(&c));
    }

    #[test]
    fn diff_lists_additions_then_removals_by_zone() {
        let old = list(&[
            (Main, Some(1), "Raigeki"),
            (Main, Some(2), "Pot"),
            (Main, Some(2), "Pot"),
            (Side, Some(3), "Ash"),
        ]);
        let new = list(&[
            (Main, Some(2), "Pot"),
            (Main, Some(4), "Called by"),
            (Main, Some(4), "Called by"),
            (Main, Some(3), "Ash"),
            (Extra, Some(5), "Link"),
        ]);
        let changes = diff(&old, &new);
        let shown: Vec<(DeckZone, &str, i64)> = changes
            .iter()
            .map(|c| (c.zone, c.name.as_str(), c.delta()))
            .collect();
        assert_eq!(
            shown,
            [
                (Main, "Ash", 1),
                (Main, "Called by", 2),
                (Main, "Pot", -1),
                (Main, "Raigeki", -1),
                (Extra, "Link", 1),
                (Side, "Ash", -1),
            ]
        );
        assert_eq!(summary(&diff(&old, &new)), (4, 3));
    }

    #[test]
    fn diff_keys_on_card_data_and_keeps_the_new_name() {
        let old = list(&[(Main, Some(1), "Raigeki")]);
        let new = list(&[(Main, Some(1), "Foudre"), (Main, Some(1), "Foudre")]);
        let changes = diff(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].name, "Foudre");
        assert_eq!((changes[0].before, changes[0].after), (1, 2));
        assert!(diff(&new, &new).is_empty());
    }

    #[test]
    fn coalesce_rules() {
        let a = list(&[(Main, Some(1), "A")]);
        let b = list(&[(Main, Some(2), "B")]);
        let last = |kept: bool, created_at: NaiveDateTime| LastSnapshot {
            created_at,
            kept,
            cards: a.clone(),
        };

        assert_eq!(coalesce(None, &a, at(10, 0)), SaveAction::Append);
        assert_eq!(
            coalesce(Some(&last(false, at(10, 0))), &a, at(15, 0)),
            SaveAction::Skip
        );
        assert_eq!(
            coalesce(Some(&last(false, at(10, 0))), &b, at(10, 29)),
            SaveAction::Replace
        );
        assert_eq!(
            coalesce(Some(&last(false, at(10, 0))), &b, at(10, 30)),
            SaveAction::Append
        );
        assert_eq!(
            coalesce(Some(&last(true, at(10, 0))), &b, at(10, 1)),
            SaveAction::Append
        );
    }

    #[test]
    fn zone_words() {
        for z in DeckZone::ALL {
            assert_eq!(DeckZone::parse(z.as_str()), Some(z));
        }
        assert_eq!(DeckZone::parse("hand"), None);
    }
}
