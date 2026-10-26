//! Reading tournaments back with their rounds, games and labels, for the
//! pages and the stats (`yugioh_domain::tournament::stats`).

use std::collections::HashMap;

use yugioh_domain::tournament::{Die, Game, GameResult, RoundRecord, RoundResult};
use sqlx::QueryBuilder;

use crate::store::db::push_ids;
use crate::store::db::Pool;
use crate::store::models::version_full_name;
use crate::web::error::AppResult;

/// Which tournaments to read: all of them by default.
#[derive(Debug, Default, Clone)]
pub struct Which {
    pub id: Option<i64>,
    /// On or after this date (`YYYY-MM-DD`).
    pub since: Option<String>,
    /// Played with a version of this deck.
    pub deck_id: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct TournamentRow {
    pub id: i64,
    pub name: String,
    pub date: String,
    pub location: Option<String>,
    pub placement: Option<i64>,
    pub players: Option<i64>,
    pub is_team: bool,
    pub conclusion: Option<String>,
    /// The old free-text notes, until moved and cleared.
    pub notes: Option<String>,
    pub version_id: Option<i64>,
    pub deck_id: Option<i64>,
    deck_name: Option<String>,
    version_name: Option<String>,
}

impl TournamentRow {
    /// "Deck - version" of the version played.
    pub fn version_full_name(&self) -> Option<String> {
        self.version_name
            .as_deref()
            .map(|v| version_full_name(self.deck_name.as_deref(), v))
    }
}

#[derive(Debug)]
pub struct RoundView {
    pub number: i64,
    pub opponent_deck: Option<String>,
    pub die: Option<Die>,
    pub result: RoundResult,
    pub team_result: Option<RoundResult>,
    pub comment: Option<String>,
    /// The opponent's deck labels, by name.
    pub labels: Vec<String>,
    pub games: Vec<GameView>,
}

#[derive(Debug)]
pub struct GameView {
    pub number: i64,
    pub result: GameResult,
    pub went_first: Option<bool>,
    pub comment: Option<String>,
}

#[derive(sqlx::FromRow)]
struct RoundRow {
    id: i64,
    tournament_id: i64,
    number: i64,
    opponent_deck: Option<String>,
    die: Option<String>,
    result: String,
    team_result: Option<String>,
    comment: Option<String>,
}

#[derive(sqlx::FromRow)]
struct GameRow {
    round_id: i64,
    number: i64,
    result: String,
    went_first: Option<bool>,
    comment: Option<String>,
}

/// A value the CHECK constraints guarantee, read back as its enum.
fn decode<T>(value: &str, parse: fn(&str) -> Option<T>) -> sqlx::Result<T> {
    parse(value).ok_or_else(|| sqlx::Error::Decode(format!("unknown code {value:?}").into()))
}

/// Tournaments with their rounds, latest first.
pub struct Loaded {
    pub tournaments: Vec<TournamentRow>,
    /// By tournament id, in round order.
    pub rounds: HashMap<i64, Vec<RoundView>>,
    /// The labels of each of your decks, by deck id.
    pub deck_labels: HashMap<i64, Vec<String>>,
}

impl Loaded {
    pub fn rounds_of(&self, tournament_id: i64) -> &[RoundView] {
        self.rounds
            .get(&tournament_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The rounds of a tournament as the stats see them.
    pub fn records_of(&self, tournament: &TournamentRow) -> Vec<RoundRecord> {
        let own_labels = tournament
            .deck_id
            .and_then(|d| self.deck_labels.get(&d))
            .cloned()
            .unwrap_or_default();
        self.rounds_of(tournament.id)
            .iter()
            .map(|round| RoundRecord {
                version_id: tournament.version_id,
                own_labels: own_labels.clone(),
                opponent_labels: round.labels.clone(),
                die: round.die,
                result: round.result,
                games: round
                    .games
                    .iter()
                    .map(|g| Game {
                        result: g.result,
                        went_first: g.went_first,
                    })
                    .collect(),
            })
            .collect()
    }

    /// Every round of every tournament loaded.
    pub fn all_records(&self) -> Vec<RoundRecord> {
        self.tournaments
            .iter()
            .flat_map(|t| self.records_of(t))
            .collect()
    }
}

pub async fn load(pool: &Pool, which: &Which) -> AppResult<Loaded> {
    let mut qb = QueryBuilder::new(
        "SELECT t.id, t.name, t.date, t.location, t.placement, t.players, t.is_team,
                t.conclusion, t.notes, v.id AS version_id, v.deck_id, deck.name AS deck_name,
                v.version_name
         FROM tournament t
         LEFT JOIN deck_version v ON v.tournament_id = t.id
         LEFT JOIN deck ON deck.id = v.deck_id
         WHERE 1",
    );
    if let Some(id) = which.id {
        qb.push(" AND t.id = ").push_bind(id);
    }
    if let Some(since) = &which.since {
        qb.push(" AND t.date >= ").push_bind(since.clone());
    }
    if let Some(deck) = which.deck_id {
        qb.push(" AND v.deck_id = ").push_bind(deck);
    }
    qb.push(" ORDER BY t.date DESC, t.id DESC");
    let tournaments: Vec<TournamentRow> = qb.build_query_as().fetch_all(pool).await?;

    let mut rounds: HashMap<i64, Vec<RoundView>> = HashMap::new();
    let mut deck_labels: HashMap<i64, Vec<String>> = HashMap::new();
    if tournaments.is_empty() {
        return Ok(Loaded {
            tournaments,
            rounds,
            deck_labels,
        });
    }

    let ids = || tournaments.iter().map(|t| t.id);
    let mut qb = QueryBuilder::new(
        "SELECT id, tournament_id, number, opponent_deck, die, result, team_result, comment
         FROM tournament_round WHERE tournament_id IN ",
    );
    push_ids(&mut qb, ids());
    qb.push(" ORDER BY tournament_id, number");
    let round_rows: Vec<RoundRow> = qb.build_query_as().fetch_all(pool).await?;

    let mut labels: HashMap<i64, Vec<String>> = HashMap::new();
    let mut games: HashMap<i64, Vec<GameView>> = HashMap::new();
    if !round_rows.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT rl.round_id, l.name FROM tournament_round_label rl
             JOIN deck_label l ON l.id = rl.label_id WHERE rl.round_id IN ",
        );
        push_ids(&mut qb, round_rows.iter().map(|r| r.id));
        qb.push(" ORDER BY l.name COLLATE NOCASE");
        let rows: Vec<(i64, String)> = qb.build_query_as().fetch_all(pool).await?;
        for (round, name) in rows {
            labels.entry(round).or_default().push(name);
        }

        let mut qb = QueryBuilder::new(
            "SELECT round_id, number, result, went_first, comment FROM tournament_game
             WHERE round_id IN ",
        );
        push_ids(&mut qb, round_rows.iter().map(|r| r.id));
        qb.push(" ORDER BY round_id, number");
        let rows: Vec<GameRow> = qb.build_query_as().fetch_all(pool).await?;
        for g in rows {
            games.entry(g.round_id).or_default().push(GameView {
                number: g.number,
                result: decode(&g.result, GameResult::from_code)?,
                went_first: g.went_first,
                comment: g.comment,
            });
        }
    }

    for r in round_rows {
        let view = RoundView {
            number: r.number,
            opponent_deck: r.opponent_deck,
            die: r
                .die
                .as_deref()
                .map(|d| decode(d, Die::from_code))
                .transpose()?,
            result: decode(&r.result, RoundResult::from_code)?,
            team_result: r
                .team_result
                .as_deref()
                .map(|t| decode(t, RoundResult::from_code))
                .transpose()?,
            comment: r.comment,
            labels: labels.remove(&r.id).unwrap_or_default(),
            games: games.remove(&r.id).unwrap_or_default(),
        };
        rounds.entry(r.tournament_id).or_default().push(view);
    }

    let deck_ids: Vec<i64> = tournaments.iter().filter_map(|t| t.deck_id).collect();
    if !deck_ids.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT dl.deck_id, l.name FROM deck_deck_label dl
             JOIN deck_label l ON l.id = dl.label_id WHERE dl.deck_id IN ",
        );
        push_ids(&mut qb, deck_ids.into_iter());
        qb.push(" ORDER BY l.name COLLATE NOCASE");
        let rows: Vec<(i64, String)> = qb.build_query_as().fetch_all(pool).await?;
        for (deck, name) in rows {
            deck_labels.entry(deck).or_default().push(name);
        }
    }

    Ok(Loaded {
        tournaments,
        rounds,
        deck_labels,
    })
}

/// The labels of one of your decks, by name.
pub async fn deck_labels(pool: &Pool, deck_id: i64) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT l.name FROM deck_deck_label dl JOIN deck_label l ON l.id = dl.label_id
         WHERE dl.deck_id = ? ORDER BY l.name COLLATE NOCASE",
    )
    .bind(deck_id)
    .fetch_all(pool)
    .await
}

/// Every label name, for the fields' suggestions.
pub async fn label_names(pool: &Pool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT name FROM deck_label ORDER BY name COLLATE NOCASE")
        .fetch_all(pool)
        .await
}
