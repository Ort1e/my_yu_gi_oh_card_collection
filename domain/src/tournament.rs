//! Tournament results: rounds, games, deck labels, and the stats computed
//! from them (`docs/specs/tournaments.md`).
//!
//! New in the Rust app: Django only had a free `notes` text per tournament.
//!
//! Who wrote what: the enums, `split_labels` and `ordinal` are Claude's;
//! [`stats`] and [`Record::rate`] (the counting rules) are the owner's.

use serde::{Deserialize, Serialize};

/// How a round ended. There are no drawn rounds: when time runs out
/// unfinished, both players lose (`DoubleLoss`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundResult {
    Win,
    Loss,
    DoubleLoss,
    Bye,
    ForfeitWin,
}

impl RoundResult {
    pub const ALL: [RoundResult; 5] = [
        RoundResult::Win,
        RoundResult::Loss,
        RoundResult::DoubleLoss,
        RoundResult::Bye,
        RoundResult::ForfeitWin,
    ];

    /// The results a team can have in a round.
    pub const TEAM: [RoundResult; 3] =
        [RoundResult::Win, RoundResult::Loss, RoundResult::DoubleLoss];

    /// The value stored in `tournament_round.result`.
    pub fn code(self) -> &'static str {
        match self {
            RoundResult::Win => "win",
            RoundResult::Loss => "loss",
            RoundResult::DoubleLoss => "double_loss",
            RoundResult::Bye => "bye",
            RoundResult::ForfeitWin => "forfeit_win",
        }
    }

    pub fn from_code(code: &str) -> Option<RoundResult> {
        RoundResult::ALL.into_iter().find(|r| r.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            RoundResult::Win => "Win",
            RoundResult::Loss => "Loss",
            RoundResult::DoubleLoss => "Double loss",
            RoundResult::Bye => "Bye",
            RoundResult::ForfeitWin => "Win by forfeit",
        }
    }
}

/// The die roll of a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Die {
    Won,
    Lost,
}

impl Die {
    pub const ALL: [Die; 2] = [Die::Won, Die::Lost];

    pub fn code(self) -> &'static str {
        match self {
            Die::Won => "won",
            Die::Lost => "lost",
        }
    }

    pub fn from_code(code: &str) -> Option<Die> {
        Die::ALL.into_iter().find(|d| d.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            Die::Won => "Die won",
            Die::Lost => "Die lost",
        }
    }
}

/// How a game ended. A drawn game is replayed (so a round can have more
/// than 3 games); an unfinished one was cut by time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameResult {
    Won,
    Lost,
    Draw,
    Unfinished,
}

impl GameResult {
    pub const ALL: [GameResult; 4] = [
        GameResult::Won,
        GameResult::Lost,
        GameResult::Draw,
        GameResult::Unfinished,
    ];

    pub fn code(self) -> &'static str {
        match self {
            GameResult::Won => "won",
            GameResult::Lost => "lost",
            GameResult::Draw => "draw",
            GameResult::Unfinished => "unfinished",
        }
    }

    pub fn from_code(code: &str) -> Option<GameResult> {
        GameResult::ALL.into_iter().find(|g| g.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            GameResult::Won => "Won",
            GameResult::Lost => "Lost",
            GameResult::Draw => "Draw",
            GameResult::Unfinished => "Unfinished",
        }
    }
}

/// The labels typed in a field: comma-separated, trimmed, blanks dropped,
/// duplicates (case-insensitive) kept once, in the order typed.
pub fn split_labels(text: &str) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for label in text.split(',').map(str::trim).filter(|l| !l.is_empty()) {
        if !labels.iter().any(|l| l.eq_ignore_ascii_case(label)) {
            labels.push(label.to_string());
        }
    }
    labels
}

/// "1st", "2nd", "3rd", "4th", "11th", "21st"…
pub fn ordinal(n: i64) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

// ---------- stats (the owner's) ----------

/// One game, as the stats see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Game {
    pub result: GameResult,
    /// `None`: not noted.
    pub went_first: Option<bool>,
}

/// One round, with what the stats group by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundRecord {
    /// The deck version played, when known.
    pub version_id: Option<i64>,
    /// The labels of the deck played.
    pub own_labels: Vec<String>,
    pub opponent_labels: Vec<String>,
    /// `None` for a bye or when not noted.
    pub die: Option<Die>,
    pub result: RoundResult,
    pub games: Vec<Game>,
}

/// Wins, losses and draws of rounds or games. Rounds never draw; for games,
/// `draws` holds the drawn and the unfinished ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Record {
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
}

impl Record {
    /// The win rate: `wins / (wins + losses)`, draws left out. `None` when
    /// there are no wins nor losses.
    pub fn rate(&self) -> Option<f64> {
        todo!("owner: the win rate")
    }
}

/// Everything the `/tournaments` page shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    /// Rounds: `Win` and `ForfeitWin` are wins, `Loss` and `DoubleLoss` are
    /// losses, a `Bye` is neither.
    pub rounds: Record,
    /// Every noted game, whatever the round result.
    pub games: Record,
    pub byes: u32,
    /// Rounds, by version id (rounds without a version left out).
    pub per_version: Vec<(i64, Record)>,
    /// Rounds, by label of the deck played (a round counts once in each).
    pub per_own_label: Vec<(String, Record)>,
    /// Rounds, by label of the opponent's deck (once in each).
    pub matchups: Vec<(String, Record)>,
    /// Rounds where the die was won / lost.
    pub die_won: Record,
    pub die_lost: Record,
    /// Games going first / second (games without it noted left out).
    pub going_first: Record,
    pub going_second: Record,
}

/// The stats of these rounds (see each field of [`Stats`] for its rule).
/// The groupings (`per_version`, `per_own_label`, `matchups`) are sorted by
/// number of rounds counted, most first, then by id / name.
pub fn stats(rounds: &[RoundRecord]) -> Stats {
    let _ = rounds;
    todo!("owner: the tournament stats")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- Claude's helpers ----------

    #[test]
    fn codes_round_trip() {
        for r in RoundResult::ALL {
            assert_eq!(RoundResult::from_code(r.code()), Some(r));
        }
        for d in Die::ALL {
            assert_eq!(Die::from_code(d.code()), Some(d));
        }
        for g in GameResult::ALL {
            assert_eq!(GameResult::from_code(g.code()), Some(g));
        }
        assert_eq!(RoundResult::from_code("draw"), None);
    }

    #[test]
    fn labels_are_split_and_deduplicated() {
        assert_eq!(
            split_labels(" Mitsurugi, K9 ,, k9, "),
            vec!["Mitsurugi", "K9"]
        );
        assert!(split_labels("  ").is_empty());
    }

    #[test]
    fn ordinals() {
        let got: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111]
            .into_iter()
            .map(ordinal)
            .collect();
        assert_eq!(
            got,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st",
                "111th"
            ]
        );
    }

    // ---------- the owner's stats ----------

    use GameResult::{Draw, Lost, Unfinished, Won};

    fn game(result: GameResult, went_first: Option<bool>) -> Game {
        Game { result, went_first }
    }

    fn round(result: RoundResult, games: &[GameResult]) -> RoundRecord {
        RoundRecord {
            version_id: Some(1),
            own_labels: vec!["Orcust".into()],
            opponent_labels: vec![],
            die: None,
            result,
            games: games.iter().map(|&g| game(g, None)).collect(),
        }
    }

    fn record(wins: u32, losses: u32, draws: u32) -> Record {
        Record {
            wins,
            losses,
            draws,
        }
    }

    #[test]
    fn rate_leaves_draws_out() {
        assert_eq!(record(1, 3, 2).rate(), Some(0.25));
        assert_eq!(record(2, 0, 0).rate(), Some(1.0));
        assert_eq!(record(0, 0, 4).rate(), None);
        assert_eq!(Record::default().rate(), None);
    }

    #[test]
    fn no_rounds_no_stats() {
        assert_eq!(stats(&[]), Stats::default());
    }

    #[test]
    fn rounds_count_forfeits_as_wins_and_double_losses_as_losses() {
        let s = stats(&[
            round(RoundResult::Win, &[Won, Won]),
            round(RoundResult::ForfeitWin, &[]),
            round(RoundResult::Loss, &[Won, Lost, Lost]),
            round(RoundResult::DoubleLoss, &[Lost, Won, Unfinished]),
            round(RoundResult::Bye, &[]),
        ]);
        assert_eq!(s.rounds, record(2, 2, 0));
        assert_eq!(s.byes, 1);
        // every game counts, whatever the round; unfinished with the draws
        assert_eq!(s.games, record(4, 3, 1));
    }

    #[test]
    fn a_bye_only_tournament() {
        let s = stats(&[round(RoundResult::Bye, &[])]);
        assert_eq!(s.rounds, Record::default());
        assert_eq!(s.byes, 1);
        assert_eq!(s.per_version, vec![]);
        assert_eq!(s.per_own_label, vec![]);
    }

    #[test]
    fn a_drawn_game_adds_a_game() {
        let s = stats(&[round(RoundResult::Win, &[Won, Draw, Lost, Won])]);
        assert_eq!(s.games, record(2, 1, 1));
        assert_eq!(s.rounds, record(1, 0, 0));
    }

    #[test]
    fn groupings_count_a_round_once_per_label() {
        let mut mitsu_k9 = round(RoundResult::Loss, &[]);
        mitsu_k9.opponent_labels = vec!["Mitsurugi".into(), "K9".into()];
        let mut k9 = round(RoundResult::Win, &[]);
        k9.opponent_labels = vec!["K9".into()];
        k9.own_labels = vec!["Orcust".into(), "K9".into()];
        k9.version_id = Some(2);
        let mut unknown = round(RoundResult::Win, &[]);
        unknown.version_id = None;

        let s = stats(&[mitsu_k9, k9, unknown]);
        assert_eq!(
            s.matchups,
            vec![
                ("K9".into(), record(1, 1, 0)),
                ("Mitsurugi".into(), record(0, 1, 0))
            ]
        );
        assert_eq!(
            s.per_own_label,
            vec![
                ("Orcust".into(), record(2, 1, 0)),
                ("K9".into(), record(1, 0, 0))
            ]
        );
        // most rounds first, then by id; the round without a version left out
        assert_eq!(
            s.per_version,
            vec![(1, record(0, 1, 0)), (2, record(1, 0, 0))]
        );
    }

    #[test]
    fn die_and_first_second() {
        let mut won_die = round(RoundResult::Win, &[]);
        won_die.die = Some(Die::Won);
        won_die.games = vec![game(Won, Some(true)), game(Won, Some(false))];
        let mut lost_die = round(RoundResult::Loss, &[]);
        lost_die.die = Some(Die::Lost);
        lost_die.games = vec![
            game(Lost, Some(false)),
            game(Won, Some(true)),
            game(Lost, None),
        ];
        let no_die = round(RoundResult::Loss, &[]);

        let s = stats(&[won_die, lost_die, no_die]);
        assert_eq!(s.die_won, record(1, 0, 0));
        assert_eq!(s.die_lost, record(0, 1, 0));
        assert_eq!(s.going_first, record(2, 0, 0));
        assert_eq!(s.going_second, record(1, 1, 0));
    }
}
