//! Tournament writes, shared by the tournament forms and the one-off
//! `import-tournament-notes` command: a tournament with its rounds, games
//! and labels (created on the way), and the deck labels. Labels left without
//! any use are deleted (docs/tour.md, "Tournaments").

use chrono::NaiveDate;
use domain::tournament::{Die, GameResult, RoundResult};
use sqlx::SqliteConnection;

/// The name, date and place of a tournament, and the version played.
#[derive(Debug, Clone)]
pub struct TournamentHead {
    pub name: String,
    pub date: NaiveDate,
    pub location: Option<String>,
    /// The deck version played (`deck_version.tournament_id`), if known.
    pub version_id: Option<i64>,
}

/// What happened at a tournament.
#[derive(Debug, Clone, Default)]
pub struct Results {
    pub placement: Option<i64>,
    pub players: Option<i64>,
    pub is_team: bool,
    pub conclusion: Option<String>,
    /// In order: numbered 1..n when saved.
    pub rounds: Vec<RoundFields>,
}

#[derive(Debug, Clone)]
pub struct RoundFields {
    /// The opponent's deck as written ("clown crew/mitsu").
    pub opponent_deck: Option<String>,
    /// The opponent's deck labels.
    pub labels: Vec<String>,
    pub die: Option<Die>,
    pub result: RoundResult,
    pub team_result: Option<RoundResult>,
    pub comment: Option<String>,
    /// In order: numbered 1..n when saved.
    pub games: Vec<GameFields>,
}

#[derive(Debug, Clone)]
pub struct GameFields {
    pub result: GameResult,
    pub went_first: Option<bool>,
    pub comment: Option<String>,
}

/// Insert a tournament. Call inside a write transaction.
pub async fn insert(
    conn: &mut SqliteConnection,
    head: &TournamentHead,
    results: &Results,
) -> sqlx::Result<i64> {
    let id = sqlx::query("INSERT INTO tournament (name, date, location) VALUES (?, ?, ?)")
        .bind(&head.name)
        .bind(head.date.to_string())
        .bind(&head.location)
        .execute(&mut *conn)
        .await?
        .last_insert_rowid();
    set_version(conn, id, head.version_id).await?;
    set_results(conn, id, results).await?;
    Ok(id)
}

/// Update a tournament; `false` when there is none with this id.
/// `clear_notes`: also empty the old free-text notes.
pub async fn update(
    conn: &mut SqliteConnection,
    id: i64,
    head: &TournamentHead,
    results: &Results,
    clear_notes: bool,
) -> sqlx::Result<bool> {
    let updated = sqlx::query(
        "UPDATE tournament SET name = ?, date = ?, location = ?,
                notes = CASE WHEN ? THEN NULL ELSE notes END
         WHERE id = ?",
    )
    .bind(&head.name)
    .bind(head.date.to_string())
    .bind(&head.location)
    .bind(clear_notes)
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if updated == 0 {
        return Ok(false);
    }

    set_version(conn, id, head.version_id).await?;
    set_results(conn, id, results).await?;
    Ok(true)
}

/// Delete a tournament (rounds and games cascade, the version played is
/// kept); `false` when there is none with this id.
pub async fn delete(conn: &mut SqliteConnection, id: i64) -> sqlx::Result<bool> {
    let deleted = sqlx::query("DELETE FROM tournament WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    delete_unused_labels(conn).await?;
    Ok(deleted > 0)
}

/// The version played: only `version_id` points to this tournament.
async fn set_version(
    conn: &mut SqliteConnection,
    id: i64,
    version_id: Option<i64>,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE deck_version SET tournament_id = NULL WHERE tournament_id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    if let Some(version) = version_id {
        sqlx::query("UPDATE deck_version SET tournament_id = ? WHERE id = ?")
            .bind(id)
            .bind(version)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Replace the results of a tournament: its fields, then its rounds (the old
/// ones deleted with their games and labels), numbered in order.
pub async fn set_results(
    conn: &mut SqliteConnection,
    id: i64,
    results: &Results,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE tournament SET placement = ?, players = ?, is_team = ?, conclusion = ?
         WHERE id = ?",
    )
    .bind(results.placement)
    .bind(results.players)
    .bind(results.is_team)
    .bind(&results.conclusion)
    .bind(id)
    .execute(&mut *conn)
    .await?;

    sqlx::query("DELETE FROM tournament_round WHERE tournament_id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    for (n, round) in results.rounds.iter().enumerate() {
        let round_id = sqlx::query(
            "INSERT INTO tournament_round
                 (tournament_id, number, opponent_deck, die, result, team_result, comment)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(n as i64 + 1)
        .bind(&round.opponent_deck)
        .bind(round.die.map(Die::code))
        .bind(round.result.code())
        .bind(round.team_result.map(RoundResult::code))
        .bind(&round.comment)
        .execute(&mut *conn)
        .await?
        .last_insert_rowid();

        for label in &round.labels {
            let label_id = label_id(conn, label).await?;
            sqlx::query(
                "INSERT OR IGNORE INTO tournament_round_label (round_id, label_id) VALUES (?, ?)",
            )
            .bind(round_id)
            .bind(label_id)
            .execute(&mut *conn)
            .await?;
        }

        for (g, game) in round.games.iter().enumerate() {
            sqlx::query(
                "INSERT INTO tournament_game (round_id, number, result, went_first, comment)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(round_id)
            .bind(g as i64 + 1)
            .bind(game.result.code())
            .bind(game.went_first)
            .bind(&game.comment)
            .execute(&mut *conn)
            .await?;
        }
    }

    delete_unused_labels(conn).await
}

/// The id of a label by name (case-insensitive: an existing label keeps its
/// spelling), created when new.
pub async fn label_id(conn: &mut SqliteConnection, name: &str) -> sqlx::Result<i64> {
    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM deck_label WHERE name = ?")
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(id) = existing {
        return Ok(id);
    }

    Ok(sqlx::query("INSERT INTO deck_label (name) VALUES (?)")
        .bind(name)
        .execute(&mut *conn)
        .await?
        .last_insert_rowid())
}

/// Replace the labels of one of your decks.
pub async fn set_deck_labels(
    conn: &mut SqliteConnection,
    deck_id: i64,
    labels: &[String],
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM deck_deck_label WHERE deck_id = ?")
        .bind(deck_id)
        .execute(&mut *conn)
        .await?;
    add_deck_labels(conn, deck_id, labels).await?;
    delete_unused_labels(conn).await
}

/// Add labels to one of your decks (the ones it has stay).
pub async fn add_deck_labels(
    conn: &mut SqliteConnection,
    deck_id: i64,
    labels: &[String],
) -> sqlx::Result<()> {
    for label in labels {
        let label_id = label_id(conn, label).await?;
        sqlx::query("INSERT OR IGNORE INTO deck_deck_label (deck_id, label_id) VALUES (?, ?)")
            .bind(deck_id)
            .bind(label_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Rename a label. `Ok(false)` when another label already has that name
/// (merge them instead) or there is no label with this id.
pub async fn rename_label(conn: &mut SqliteConnection, id: i64, name: &str) -> sqlx::Result<bool> {
    let taken: Option<i64> =
        sqlx::query_scalar("SELECT id FROM deck_label WHERE name = ? AND id != ?")
            .bind(name)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    if taken.is_some() {
        return Ok(false);
    }

    let updated = sqlx::query("UPDATE deck_label SET name = ? WHERE id = ?")
        .bind(name)
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(updated > 0)
}

/// Move every use of label `from` to label `into`, then delete `from`.
pub async fn merge_labels(conn: &mut SqliteConnection, from: i64, into: i64) -> sqlx::Result<()> {
    for table in ["deck_deck_label", "tournament_round_label"] {
        sqlx::query(&format!(
            "UPDATE OR IGNORE {table} SET label_id = ? WHERE label_id = ?"
        ))
        .bind(into)
        .bind(from)
        .execute(&mut *conn)
        .await?;
    }
    // the rows left were duplicates of uses `into` already had
    sqlx::query("DELETE FROM deck_label WHERE id = ?")
        .bind(from)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Delete the labels no deck and no round uses any more.
pub async fn delete_unused_labels(conn: &mut SqliteConnection) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM deck_label
         WHERE id NOT IN (SELECT label_id FROM deck_deck_label)
           AND id NOT IN (SELECT label_id FROM tournament_round_label)",
    )
    .execute(conn)
    .await?;
    Ok(())
}
