//! `import-tournament-notes <file.json> [--dry-run]`: one-off load of the
//! tournaments' old free-text notes, transcribed by hand into a JSON file
//! (docs/tour.md, "Tournaments"). The file lives in the private
//! data submodule and is deleted once imported: the notes are personal.
//!
//! Each entry names an existing tournament by id and gives its results and
//! rounds; the deck version played already points to it. A tournament that
//! already has rounds is skipped, so running the command twice changes
//! nothing. The notes themselves stay in `tournament.notes` until cleared
//! from the edit page. Only ids and counts are printed, never the text.

use std::path::Path;

use anyhow::{bail, Context};
use domain::tournament::{split_labels, Die, GameResult, RoundResult};
use serde::Deserialize;

use crate::store::db::begin_write;
use crate::store::tournaments::{self, GameFields, Results, RoundFields};
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NotesFile {
    tournaments: Vec<TournamentNotes>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TournamentNotes {
    id: i64,
    placement: Option<i64>,
    players: Option<i64>,
    #[serde(default)]
    is_team: bool,
    conclusion: Option<String>,
    /// Labels added to the deck of the version played.
    #[serde(default)]
    own_labels: Vec<String>,
    rounds: Vec<RoundNotes>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoundNotes {
    opponent_deck: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    die: Option<Die>,
    result: RoundResult,
    team_result: Option<RoundResult>,
    comment: Option<String>,
    #[serde(default)]
    games: Vec<GameNotes>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GameNotes {
    result: GameResult,
    went_first: Option<bool>,
    comment: Option<String>,
}

/// Labels as typed: the same cleanup as the form's comma-separated field.
fn clean_labels(labels: &[String]) -> Vec<String> {
    split_labels(&labels.join(","))
}

impl TournamentNotes {
    fn results(&self) -> anyhow::Result<Results> {
        if let (Some(p), Some(n)) = (self.placement, self.players) {
            if p > n {
                bail!("tournament #{}: placement {p} over {n} players", self.id);
            }
        }
        Ok(Results {
            placement: self.placement,
            players: self.players,
            is_team: self.is_team,
            conclusion: self.conclusion.clone(),
            rounds: self
                .rounds
                .iter()
                .map(|r| RoundFields {
                    opponent_deck: r.opponent_deck.clone(),
                    labels: clean_labels(&r.labels),
                    die: r.die,
                    result: r.result,
                    team_result: r.team_result,
                    comment: r.comment.clone(),
                    games: r
                        .games
                        .iter()
                        .map(|g| GameFields {
                            result: g.result,
                            went_first: g.went_first,
                            comment: g.comment.clone(),
                        })
                        .collect(),
                })
                .collect(),
        })
    }
}

/// What the import did, for the report.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub imported: Vec<(i64, usize)>,
    /// Tournaments that already had rounds.
    pub skipped: Vec<i64>,
}

pub async fn import(state: &AppState, file: &Path, dry_run: bool) -> anyhow::Result<()> {
    let report = run(state, file, dry_run).await?;
    for (id, rounds) in &report.imported {
        println!("tournament #{id}: {rounds} rounds");
    }
    for id in &report.skipped {
        println!("tournament #{id}: already has rounds, skipped");
    }
    println!(
        "{} tournaments imported, {} skipped{}",
        report.imported.len(),
        report.skipped.len(),
        if dry_run {
            " (dry run: nothing saved)"
        } else {
            ""
        }
    );
    Ok(())
}

async fn run(state: &AppState, file: &Path, dry_run: bool) -> anyhow::Result<Report> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let notes: NotesFile = serde_json::from_str(&text).context("reading the tournaments JSON")?;

    let mut report = Report::default();
    let mut tx = begin_write(&state.pool).await?;
    for t in &notes.tournaments {
        let results = t.results()?;

        let known: Option<i64> = sqlx::query_scalar("SELECT id FROM tournament WHERE id = ?")
            .bind(t.id)
            .fetch_optional(&mut *tx)
            .await?;
        if known.is_none() {
            bail!("tournament #{} doesn't exist", t.id);
        }
        let rounds: i64 =
            sqlx::query_scalar("SELECT count(*) FROM tournament_round WHERE tournament_id = ?")
                .bind(t.id)
                .fetch_one(&mut *tx)
                .await?;
        if rounds > 0 {
            report.skipped.push(t.id);
            continue;
        }

        tournaments::set_results(&mut tx, t.id, &results).await?;
        let deck: Option<i64> = sqlx::query_scalar(
            "SELECT deck_id FROM deck_version WHERE tournament_id = ? AND deck_id IS NOT NULL",
        )
        .bind(t.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(deck) = deck {
            tournaments::add_deck_labels(&mut tx, deck, &clean_labels(&t.own_labels)).await?;
        }
        report.imported.push((t.id, results.rounds.len()));
    }

    if dry_run {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }
    tracing::info!(
        imported = report.imported.len(),
        skipped = report.skipped.len(),
        dry_run,
        "tournament notes imported"
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TestApp;

    /// Synthetic data only (never the real notes).
    const FILE: &str = r#"{"tournaments": [
        {"id": 1, "placement": 3, "players": 8, "conclusion": "Fun.",
         "own_labels": ["Orcust", "K9"],
         "rounds": [
            {"opponent_deck": "mitsu/K9", "labels": ["Mitsurugi", "k9"], "die": "won",
             "result": "loss", "comment": "Close.",
             "games": [{"result": "won", "went_first": true},
                       {"result": "draw"},
                       {"result": "lost", "went_first": false, "comment": "Bricked."},
                       {"result": "lost"}]},
            {"result": "bye"}
         ]},
        {"id": 2, "is_team": true,
         "rounds": [{"labels": ["Tenpai"], "result": "double_loss", "team_result": "win",
                     "games": [{"result": "won"}, {"result": "lost"}, {"result": "unfinished"}]}]}
    ]}"#;

    async fn setup() -> (TestApp, std::path::PathBuf) {
        let t = TestApp::new().await;
        sqlx::query(
            "INSERT INTO deck (id, name) VALUES (1, 'Orcust');
             INSERT INTO tournament (id, name, date, notes) VALUES
                 (1, 'Locals', '2025-09-21', 'old text'), (2, 'Team', '2025-11-30', NULL);
             INSERT INTO deck_version (id, deck_id, version_name, tournament_id) VALUES (1, 1, 'v1', 1);
             INSERT INTO deck_label (name) VALUES ('K9');",
        )
        .execute(&t.state.pool)
        .await
        .unwrap();
        let file = std::env::temp_dir().join(format!("notes-{:016x}.json", rand::random::<u64>()));
        std::fs::write(&file, FILE).unwrap();
        (t, file)
    }

    #[tokio::test]
    async fn imports_once() {
        let (t, file) = setup().await;

        // a dry run saves nothing
        let report = run(&t.state, &file, true).await.unwrap();
        assert_eq!(report.imported, vec![(1, 2), (2, 1)]);
        assert_eq!(t.scalar("SELECT count(*) FROM tournament_round").await, 0);

        let report = run(&t.state, &file, false).await.unwrap();
        assert_eq!(report.imported, vec![(1, 2), (2, 1)]);
        assert_eq!(t.scalar("SELECT count(*) FROM tournament_round").await, 3);
        assert_eq!(t.scalar("SELECT count(*) FROM tournament_game").await, 7);
        assert_eq!(
            t.scalar("SELECT count(*) FROM tournament_game WHERE result = 'draw'")
                .await,
            1
        );
        // labels: "k9" is the existing K9, the others created
        assert_eq!(
            t.text(
                "SELECT group_concat(name, ',') FROM (SELECT name FROM deck_label ORDER BY name)"
            )
            .await
            .as_deref(),
            Some("K9,Mitsurugi,Orcust,Tenpai")
        );
        assert_eq!(
            t.scalar("SELECT count(*) FROM deck_deck_label WHERE deck_id = 1")
                .await,
            2
        );
        assert_eq!(
            t.text("SELECT team_result FROM tournament_round WHERE tournament_id = 2")
                .await
                .as_deref(),
            Some("win")
        );
        // the old notes stay until cleared by hand
        assert_eq!(
            t.text("SELECT notes FROM tournament WHERE id = 1")
                .await
                .as_deref(),
            Some("old text")
        );

        // a second run skips what is there
        let report = run(&t.state, &file, false).await.unwrap();
        assert_eq!(report.skipped, vec![1, 2]);
        assert_eq!(t.scalar("SELECT count(*) FROM tournament_round").await, 3);

        std::fs::remove_file(file).unwrap();
    }

    #[tokio::test]
    async fn refuses_an_unknown_tournament_or_a_typo() {
        let (t, file) = setup().await;
        std::fs::write(&file, r#"{"tournaments": [{"id": 9, "rounds": []}]}"#).unwrap();
        assert!(run(&t.state, &file, false).await.is_err());
        std::fs::write(
            &file,
            r#"{"tournaments": [{"id": 1, "rounds": [{"result": "draw"}]}]}"#,
        )
        .unwrap();
        assert!(run(&t.state, &file, false).await.is_err());
        std::fs::write(
            &file,
            r#"{"tournaments": [{"id": 1, "placment": 3, "rounds": []}]}"#,
        )
        .unwrap();
        assert!(run(&t.state, &file, false).await.is_err());
        std::fs::remove_file(file).unwrap();
    }
}
