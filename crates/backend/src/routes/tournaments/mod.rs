//! `/tournaments`: every tournament with its record, and the stats
//! (`yugioh_domain::tournament::stats`); `/tournaments/:id`: one tournament round
//! by round. The forms are in `form.rs`, the labels page in `labels.rs`
//! (docs/tour.md, "Tournaments").

mod data;
pub mod form;
pub mod labels;

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use yugioh_domain::tournament::{ordinal, stats, Record, RoundResult, Stats};

pub use self::data::{deck_labels, label_names};
use self::data::{RoundView, TournamentRow, Which};
use crate::web::error::{AppResult, OrNotFound};
use crate::AppState;

/// "4-3", or "4-3-1" with draws.
fn record_text(r: &Record) -> String {
    if r.draws > 0 {
        format!("{}-{}-{}", r.wins, r.losses, r.draws)
    } else {
        format!("{}-{}", r.wins, r.losses)
    }
}

/// "57%", "—" without wins nor losses.
fn rate_text(r: &Record) -> String {
    r.rate()
        .map(|rate| format!("{:.0}%", rate * 100.0))
        .unwrap_or_else(|| "—".into())
}

/// "2-5", "2-1, 1 bye", "" without rounds.
fn tournament_record(s: &Stats) -> String {
    let rounds = s.rounds.wins + s.rounds.losses;
    if rounds + s.byes == 0 {
        return String::new();
    }

    let mut text = record_text(&s.rounds);
    match s.byes {
        0 => {}
        1 => text.push_str(", 1 bye"),
        n => text.push_str(&format!(", {n} byes")),
    }
    text
}

/// "7th of 11", "7th", "11 players", "".
fn placement_text(t: &TournamentRow) -> String {
    match (t.placement, t.players) {
        (Some(p), Some(n)) => format!("{} of {n}", ordinal(p)),
        (Some(p), None) => ordinal(p),
        (None, Some(n)) => format!("{n} players"),
        (None, None) => String::new(),
    }
}

/// A CSS class for a result: `win`, `loss` or `other`.
fn result_class(result: RoundResult) -> &'static str {
    match result {
        RoundResult::Win | RoundResult::ForfeitWin => "win",
        RoundResult::Loss | RoundResult::DoubleLoss => "loss",
        RoundResult::Bye => "other",
    }
}

// ---------- list and stats ----------

/// `?since=YYYY-MM-DD&deck=<id>`, blank or invalid values ignored.
#[derive(Debug, Default, Deserialize)]
pub struct Filter {
    since: Option<String>,
    deck: Option<String>,
}

impl Filter {
    fn since(&self) -> Option<String> {
        let since = self.since.as_deref()?.trim();
        since
            .parse::<chrono::NaiveDate>()
            .ok()
            .map(|d| d.to_string())
    }

    fn deck_id(&self) -> Option<i64> {
        self.deck.as_deref()?.trim().parse().ok()
    }
}

pub struct ListRow {
    id: i64,
    date: String,
    name: String,
    location: String,
    version_id: Option<i64>,
    version_name: String,
    is_team: bool,
    record: String,
    placement: String,
    /// To sort by placement: the place, empty when not noted.
    placement_rank: String,
}

pub struct StatLine {
    name: String,
    href: Option<String>,
    rounds: u32,
    record: String,
    rate: String,
}

impl StatLine {
    fn new(name: impl Into<String>, href: Option<String>, r: &Record) -> StatLine {
        StatLine {
            name: name.into(),
            href,
            rounds: r.wins + r.losses + r.draws,
            record: record_text(r),
            rate: rate_text(r),
        }
    }
}

/// One table of the stats.
pub struct StatSection {
    title: &'static str,
    /// The heading of the first column.
    what: &'static str,
    /// "Rounds" or "Games".
    counted: &'static str,
    lines: Vec<StatLine>,
}

pub struct DeckOption {
    id: i64,
    name: String,
    selected: bool,
}

#[derive(Template)]
#[template(path = "tournaments/list.html")]
pub struct ListPage {
    rows: Vec<ListRow>,
    sections: Vec<StatSection>,
    byes: u32,
    since: String,
    decks: Vec<DeckOption>,
    filtered: bool,
}

fn sections(s: &Stats, version_names: &HashMap<i64, String>) -> Vec<StatSection> {
    let labels = |lines: &[(String, Record)]| {
        lines
            .iter()
            .map(|(name, r)| StatLine::new(name.clone(), None, r))
            .collect()
    };
    vec![
        StatSection {
            title: "Overall",
            what: "",
            counted: "Played",
            lines: vec![
                StatLine::new("Rounds", None, &s.rounds),
                StatLine::new("Games", None, &s.games),
            ],
        },
        StatSection {
            title: "By deck version",
            what: "Version",
            counted: "Rounds",
            lines: s
                .per_version
                .iter()
                .map(|(id, r)| {
                    let name = version_names
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| format!("Version #{id}"));
                    StatLine::new(name, Some(format!("/deck-builder/{id}")), r)
                })
                .collect(),
        },
        StatSection {
            title: "By your deck's labels",
            what: "Label",
            counted: "Rounds",
            lines: labels(&s.per_own_label),
        },
        StatSection {
            title: "Matchups",
            what: "Against",
            counted: "Rounds",
            lines: labels(&s.matchups),
        },
        StatSection {
            title: "Die",
            what: "",
            counted: "Rounds",
            lines: vec![
                StatLine::new("Die won", None, &s.die_won),
                StatLine::new("Die lost", None, &s.die_lost),
            ],
        },
        StatSection {
            title: "First or second",
            what: "",
            counted: "Games",
            lines: vec![
                StatLine::new("Going first", None, &s.going_first),
                StatLine::new("Going second", None, &s.going_second),
            ],
        },
    ]
}

/// `GET /tournaments`.
pub async fn list(
    State(state): State<AppState>,
    Query(filter): Query<Filter>,
) -> AppResult<ListPage> {
    let which = Which {
        since: filter.since(),
        deck_id: filter.deck_id(),
        ..Which::default()
    };
    let loaded = data::load(&state.pool, &which).await?;

    let rows = loaded
        .tournaments
        .iter()
        .map(|t| ListRow {
            id: t.id,
            date: t.date.clone(),
            name: t.name.clone(),
            location: t.location.clone().unwrap_or_default(),
            version_id: t.version_id,
            version_name: t.version_full_name().unwrap_or_default(),
            is_team: t.is_team,
            record: tournament_record(&stats(&loaded.records_of(t))),
            placement: placement_text(t),
            placement_rank: t.placement.map(|p| p.to_string()).unwrap_or_default(),
        })
        .collect();

    let version_names: HashMap<i64, String> = loaded
        .tournaments
        .iter()
        .filter_map(|t| Some((t.version_id?, t.version_full_name()?)))
        .collect();
    let all = stats(&loaded.all_records());

    let decks: Vec<(i64, String)> = sqlx::query_as(
        "SELECT DISTINCT deck.id, deck.name FROM deck
         JOIN deck_version v ON v.deck_id = deck.id
         WHERE v.tournament_id IS NOT NULL ORDER BY deck.name COLLATE NOCASE",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(ListPage {
        rows,
        sections: sections(&all, &version_names),
        byes: all.byes,
        since: which.since.clone().unwrap_or_default(),
        decks: decks
            .into_iter()
            .map(|(id, name)| DeckOption {
                id,
                name,
                selected: which.deck_id == Some(id),
            })
            .collect(),
        filtered: which.since.is_some() || which.deck_id.is_some(),
    })
}

// ---------- detail ----------

pub struct GameLine {
    number: i64,
    result: &'static str,
    result_class: &'static str,
    turn: &'static str,
    comment: String,
}

pub struct RoundLine {
    number: i64,
    opponent: String,
    labels: String,
    die: &'static str,
    result: &'static str,
    result_class: &'static str,
    team_result: &'static str,
    comment: String,
    games: Vec<GameLine>,
}

impl RoundLine {
    fn new(round: &RoundView) -> RoundLine {
        use yugioh_domain::tournament::GameResult;

        RoundLine {
            number: round.number,
            opponent: round.opponent_deck.clone().unwrap_or_default(),
            labels: round.labels.join(", "),
            die: round.die.map(|d| d.label()).unwrap_or(""),
            result: round.result.label(),
            result_class: result_class(round.result),
            team_result: round.team_result.map(RoundResult::label).unwrap_or(""),
            comment: round.comment.clone().unwrap_or_default(),
            games: round
                .games
                .iter()
                .map(|g| GameLine {
                    number: g.number,
                    result: g.result.label(),
                    result_class: match g.result {
                        GameResult::Won => "win",
                        GameResult::Lost => "loss",
                        GameResult::Draw | GameResult::Unfinished => "other",
                    },
                    turn: match g.went_first {
                        Some(true) => "first",
                        Some(false) => "second",
                        None => "",
                    },
                    comment: g.comment.clone().unwrap_or_default(),
                })
                .collect(),
        }
    }
}

#[derive(Template)]
#[template(path = "tournaments/detail.html")]
pub struct DetailPage {
    id: i64,
    name: String,
    date: String,
    location: String,
    version_id: Option<i64>,
    version_name: String,
    is_team: bool,
    record: String,
    placement: String,
    conclusion: String,
    notes: String,
    rounds: Vec<RoundLine>,
}

/// `GET /tournaments/:id`.
pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<DetailPage> {
    let which = Which {
        id: Some(id),
        ..Which::default()
    };
    let loaded = data::load(&state.pool, &which).await?;
    let t = loaded.tournaments.first().or_404()?;

    Ok(DetailPage {
        id,
        name: t.name.clone(),
        date: t.date.clone(),
        location: t.location.clone().unwrap_or_default(),
        version_id: t.version_id,
        version_name: t.version_full_name().unwrap_or_default(),
        is_team: t.is_team,
        record: tournament_record(&stats(&loaded.records_of(t))),
        placement: placement_text(t),
        conclusion: t.conclusion.clone().unwrap_or_default(),
        notes: t.notes.clone().unwrap_or_default(),
        rounds: loaded.rounds_of(id).iter().map(RoundLine::new).collect(),
    })
}
