//! `/tournaments`: every tournament with its record; `/tournaments/stats`:
//! the stats (`stats.rs`, `yugioh_domain::tournament::stats`), both following
//! the same filter; `/tournaments/:id`: one tournament round by round. The
//! forms are in `form.rs`, the labels page in `labels.rs` (docs/tour.md,
//! "Tournaments").

mod data;
pub mod form;
pub mod labels;
pub mod stats;

use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use yugioh_domain::tournament::{ordinal, stats, Record, RoundResult, Stats};

pub use self::data::{deck_labels, label_names};
use self::data::{RoundView, TournamentRow, Which};
use crate::store::db::Pool;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::filters;
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
    snapshot_id: Option<i64>,
    version_name: String,
    is_team: bool,
    record: String,
    placement: String,
    /// To sort by placement: the place, empty when not noted.
    placement_rank: String,
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
    since: String,
    decks: Vec<DeckOption>,
    filtered: bool,
    /// The stats page with the same filter.
    stats_href: String,
}

impl ListPage {
    /// Where the filter form goes (`filter.html`).
    fn filter_action(&self) -> &'static str {
        "/tournaments"
    }
}

impl Filter {
    /// `?since=…&deck=…` of what is set, "" when nothing is.
    fn query(&self) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        if let Some(since) = self.since() {
            query.append_pair("since", &since);
        }
        if let Some(deck) = self.deck_id() {
            query.append_pair("deck", &deck.to_string());
        }
        let query = query.finish();
        if query.is_empty() {
            query
        } else {
            format!("?{query}")
        }
    }
}

/// The decks with a list played in a tournament, for the filter's menu.
async fn deck_options(pool: &Pool, selected: Option<i64>) -> AppResult<Vec<DeckOption>> {
    let decks: Vec<(i64, String)> = sqlx::query_as(
        "SELECT DISTINCT deck.id, deck.name FROM deck
         JOIN deck_snapshot s ON s.deck_id = deck.id
         WHERE s.tournament_id IS NOT NULL ORDER BY deck.name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;

    Ok(decks
        .into_iter()
        .map(|(id, name)| DeckOption {
            id,
            name,
            selected: selected == Some(id),
        })
        .collect())
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
            snapshot_id: t.snapshot_id,
            version_name: t.version_full_name().unwrap_or_default(),
            is_team: t.is_team,
            record: tournament_record(&stats(&loaded.records_of(t))),
            placement: placement_text(t),
            placement_rank: t.placement.map(|p| p.to_string()).unwrap_or_default(),
        })
        .collect();

    Ok(ListPage {
        rows,
        since: which.since.clone().unwrap_or_default(),
        decks: deck_options(&state.pool, which.deck_id).await?,
        filtered: which.since.is_some() || which.deck_id.is_some(),
        stats_href: format!("/tournaments/stats{}", filter.query()),
    })
}

// ---------- detail ----------

pub struct GameLine {
    pub(crate) number: i64,
    pub(crate) result: &'static str,
    pub(crate) result_class: &'static str,
    pub(crate) turn: &'static str,
    pub(crate) comment: String,
}

pub struct RoundLine {
    pub(crate) number: i64,
    pub(crate) opponent: String,
    pub(crate) labels: String,
    pub(crate) die: &'static str,
    pub(crate) result: &'static str,
    pub(crate) result_class: &'static str,
    pub(crate) team_result: &'static str,
    pub(crate) comment: String,
    pub(crate) games: Vec<GameLine>,
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

/// One tournament, round by round: its page, and the page of the deck
/// version played (`routes::decks::version`, `tournaments/rounds.html`).
pub struct TournamentView {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) date: String,
    pub(crate) location: String,
    /// The list played.
    pub(crate) snapshot_id: Option<i64>,
    pub(crate) version_name: String,
    pub(crate) is_team: bool,
    pub(crate) record: String,
    pub(crate) placement: String,
    pub(crate) conclusion: String,
    pub(crate) notes: String,
    pub(crate) rounds: Vec<RoundLine>,
}

impl TournamentView {
    /// Tournament `id`, `None` when there is none.
    pub(crate) async fn load(pool: &Pool, id: i64) -> AppResult<Option<TournamentView>> {
        let which = Which {
            id: Some(id),
            ..Which::default()
        };
        let loaded = data::load(pool, &which).await?;
        let Some(t) = loaded.tournaments.first() else {
            return Ok(None);
        };

        Ok(Some(TournamentView {
            id,
            name: t.name.clone(),
            date: t.date.clone(),
            location: t.location.clone().unwrap_or_default(),
            snapshot_id: t.snapshot_id,
            version_name: t.version_full_name().unwrap_or_default(),
            is_team: t.is_team,
            record: tournament_record(&stats(&loaded.records_of(t))),
            placement: placement_text(t),
            conclusion: t.conclusion.clone().unwrap_or_default(),
            notes: t.notes.clone().unwrap_or_default(),
            rounds: loaded.rounds_of(id).iter().map(RoundLine::new).collect(),
        }))
    }
}

#[derive(Template)]
#[template(path = "tournaments/detail.html")]
pub struct DetailPage {
    tournament: TournamentView,
}

/// `GET /tournaments/:id`.
pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<DetailPage> {
    let tournament = TournamentView::load(&state.pool, id).await?.or_404()?;
    Ok(DetailPage { tournament })
}
