//! Adding, editing and deleting tournaments (behind the login).
//!
//! The rounds and their games repeat: `rounds-<i>-<field>` and
//! `rounds-<i>-games-<j>-<field>`, added and removed in the page by
//! `static/js/tournament_form.js`. Blank rows are skipped; rounds and games
//! are numbered in the order they come. Tournaments can be deleted (not
//! collection history): the version played is kept.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use yugioh_domain::tournament::{split_labels, Die, GameResult, RoundResult};
use serde::Deserialize;

use super::data::{self, RoundView, Which};
use crate::store::db::{begin_write, Pool};
use crate::store::models::version_full_name;
use crate::store::tournaments::{self, GameFields, Results, RoundFields, TournamentHead};
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// One `<option>` of a select.
pub struct Choice {
    value: &'static str,
    label: &'static str,
    selected: bool,
}

fn choices(
    options: impl IntoIterator<Item = (&'static str, &'static str)>,
    current: &str,
) -> Vec<Choice> {
    std::iter::once(("", "—"))
        .chain(options)
        .map(|(value, label)| Choice {
            value,
            label,
            selected: value == current,
        })
        .collect()
}

/// A game row as typed. `round` and `index` are the numbers in the field
/// names (`__r__` / `__g__` in the template rows the page script copies).
#[derive(Debug, Clone, Default)]
pub struct GameValues {
    round: String,
    index: String,
    result: String,
    /// `first`, `second` or empty.
    turn: String,
    comment: String,
}

/// The number shown for a field index ("0" → "1"), empty for a template row.
fn shown_number(index: &str) -> String {
    index
        .parse::<usize>()
        .map(|i| (i + 1).to_string())
        .unwrap_or_default()
}

impl GameValues {
    fn number(&self) -> String {
        shown_number(&self.index)
    }

    fn is_blank(&self) -> bool {
        self.result.is_empty() && self.turn.is_empty() && self.comment.is_empty()
    }

    fn result_choices(&self) -> Vec<Choice> {
        choices(GameResult::ALL.map(|g| (g.code(), g.label())), &self.result)
    }

    fn turn_choices(&self) -> Vec<Choice> {
        choices([("first", "First"), ("second", "Second")], &self.turn)
    }
}

/// A round row as typed.
#[derive(Debug, Clone, Default)]
pub struct RoundValues {
    index: String,
    opponent_deck: String,
    /// Comma-separated.
    labels: String,
    die: String,
    result: String,
    team_result: String,
    comment: String,
    games: Vec<GameValues>,
}

impl RoundValues {
    fn number(&self) -> String {
        shown_number(&self.index)
    }

    fn is_blank(&self) -> bool {
        self.opponent_deck.is_empty()
            && self.labels.is_empty()
            && self.die.is_empty()
            && self.result.is_empty()
            && self.team_result.is_empty()
            && self.comment.is_empty()
            && self.games.is_empty()
    }

    fn result_choices(&self) -> Vec<Choice> {
        choices(
            RoundResult::ALL.map(|r| (r.code(), r.label())),
            &self.result,
        )
    }

    fn team_choices(&self) -> Vec<Choice> {
        choices(
            RoundResult::TEAM.map(|r| (r.code(), r.label())),
            &self.team_result,
        )
    }

    fn die_choices(&self) -> Vec<Choice> {
        choices(Die::ALL.map(|d| (d.code(), d.label())), &self.die)
    }

    /// The index of the next game row the page script adds.
    fn next_game(&self) -> usize {
        self.games.len()
    }
}

/// The tournament fields as typed, to fill the form again.
#[derive(Debug, Default)]
pub struct TournamentValues {
    name: String,
    date: String,
    location: String,
    version_id: String,
    placement: String,
    players: String,
    is_team: bool,
    conclusion: String,
    clear_notes: bool,
    rounds: Vec<RoundValues>,
}

impl TournamentValues {
    fn from_form(form: &FormData) -> TournamentValues {
        let rounds = form
            .indices("rounds-")
            .into_iter()
            .map(|i| {
                let field = |f: &str| form.text(&format!("rounds-{i}-{f}"));
                let games = form
                    .indices(&format!("rounds-{i}-games-"))
                    .into_iter()
                    .map(|j| {
                        let field = |f: &str| form.text(&format!("rounds-{i}-games-{j}-{f}"));
                        GameValues {
                            round: String::new(),
                            index: String::new(),
                            result: field("result"),
                            turn: field("turn"),
                            comment: field("comment"),
                        }
                    })
                    .filter(|g| !g.is_blank())
                    .collect();
                RoundValues {
                    index: String::new(),
                    opponent_deck: field("opponent_deck"),
                    labels: field("labels"),
                    die: field("die"),
                    result: field("result"),
                    team_result: field("team_result"),
                    comment: field("comment"),
                    games,
                }
            })
            .filter(|r| !r.is_blank())
            .collect();

        let mut values = TournamentValues {
            name: form.text("name"),
            date: form.text("date"),
            location: form.text("location"),
            version_id: form.text("version_id"),
            placement: form.text("placement"),
            players: form.text("players"),
            is_team: form.checked("is_team"),
            conclusion: form.text("conclusion"),
            clear_notes: form.checked("clear_notes"),
            rounds,
        };
        values.renumber();
        values
    }

    /// The values of a stored tournament.
    fn stored(t: &data::TournamentRow, rounds: &[RoundView]) -> TournamentValues {
        let text = |n: Option<i64>| n.map(|n| n.to_string()).unwrap_or_default();
        let mut values = TournamentValues {
            name: t.name.clone(),
            date: t.date.clone(),
            location: t.location.clone().unwrap_or_default(),
            version_id: text(t.version_id),
            placement: text(t.placement),
            players: text(t.players),
            is_team: t.is_team,
            conclusion: t.conclusion.clone().unwrap_or_default(),
            clear_notes: false,
            rounds: rounds
                .iter()
                .map(|r| RoundValues {
                    index: String::new(),
                    opponent_deck: r.opponent_deck.clone().unwrap_or_default(),
                    labels: r.labels.join(", "),
                    die: r.die.map(Die::code).unwrap_or_default().into(),
                    result: r.result.code().into(),
                    team_result: r
                        .team_result
                        .map(RoundResult::code)
                        .unwrap_or_default()
                        .into(),
                    comment: r.comment.clone().unwrap_or_default(),
                    games: r
                        .games
                        .iter()
                        .map(|g| GameValues {
                            round: String::new(),
                            index: String::new(),
                            result: g.result.code().into(),
                            turn: match g.went_first {
                                Some(true) => "first",
                                Some(false) => "second",
                                None => "",
                            }
                            .into(),
                            comment: g.comment.clone().unwrap_or_default(),
                        })
                        .collect(),
                })
                .collect(),
        };
        values.renumber();
        values
    }

    /// Field indices 0..n in order (a re-rendered form starts afresh).
    fn renumber(&mut self) {
        for (i, round) in self.rounds.iter_mut().enumerate() {
            round.index = i.to_string();
            for (j, game) in round.games.iter_mut().enumerate() {
                game.round = i.to_string();
                game.index = j.to_string();
            }
        }
    }

    /// The index of the next round row the page script adds.
    fn next_round(&self) -> usize {
        self.rounds.len()
    }

    /// The empty rows the page script copies.
    fn blank_round(&self) -> RoundValues {
        RoundValues {
            index: "__r__".into(),
            ..RoundValues::default()
        }
    }

    fn blank_game(&self) -> GameValues {
        GameValues {
            round: "__r__".into(),
            index: "__g__".into(),
            ..GameValues::default()
        }
    }
}

/// Validate a round; errors are added to `errors` (`n`: its number).
fn read_round(
    values: &RoundValues,
    n: usize,
    is_team: bool,
    errors: &mut Vec<String>,
) -> Option<RoundFields> {
    let errors_before = errors.len();
    let result = match values.result.as_str() {
        "" => {
            errors.push(format!("Round {n}: choose its result."));
            None
        }
        code => RoundResult::from_code(code).or_else(|| {
            errors.push(format!("Round {n}: invalid result."));
            None
        }),
    };
    let die = match values.die.as_str() {
        "" => None,
        code => Die::from_code(code).or_else(|| {
            errors.push(format!("Round {n}: invalid die."));
            None
        }),
    };
    let team_result = match values.team_result.as_str() {
        "" => None,
        _ if !is_team => {
            errors.push(format!(
                "Round {n}: a team result is only for team events (tick \"Team event\")."
            ));
            None
        }
        code => RoundResult::from_code(code)
            .filter(|r| RoundResult::TEAM.contains(r))
            .or_else(|| {
                errors.push(format!("Round {n}: invalid team result."));
                None
            }),
    };

    let mut games = Vec::new();
    for (m, game) in values.games.iter().enumerate() {
        let m = m + 1;
        let result = match game.result.as_str() {
            "" => {
                errors.push(format!("Round {n}, game {m}: choose its result."));
                continue;
            }
            code => match GameResult::from_code(code) {
                Some(result) => result,
                None => {
                    errors.push(format!("Round {n}, game {m}: invalid result."));
                    continue;
                }
            },
        };
        let went_first = match game.turn.as_str() {
            "" => None,
            "first" => Some(true),
            "second" => Some(false),
            _ => {
                errors.push(format!("Round {n}, game {m}: invalid first/second."));
                None
            }
        };
        games.push(GameFields {
            result,
            went_first,
            comment: non_empty(&game.comment),
        });
    }

    let labels = split_labels(&values.labels);
    if result == Some(RoundResult::Bye)
        && (!values.opponent_deck.is_empty()
            || !labels.is_empty()
            || die.is_some()
            || !values.games.is_empty())
    {
        errors.push(format!(
            "Round {n}: a bye has no opponent, labels, die nor games."
        ));
    }

    if errors.len() > errors_before {
        return None;
    }

    Some(RoundFields {
        opponent_deck: non_empty(&values.opponent_deck),
        labels,
        die,
        result: result?,
        team_result,
        comment: non_empty(&values.comment),
        games,
    })
}

fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Validate the form; errors are added to `form.errors`.
async fn read_form(
    pool: &Pool,
    form: &mut FormData,
    values: &TournamentValues,
) -> AppResult<Option<(TournamentHead, Results)>> {
    let name = form.required("name", "Name");
    let date = form.required_date("date", "Date");
    let version_id = form.id("version_id", "Deck version played");
    if let Some(version) = version_id {
        let known: Option<i64> = sqlx::query_scalar("SELECT id FROM deck_version WHERE id = ?")
            .bind(version)
            .fetch_optional(pool)
            .await?;
        if known.is_none() {
            form.errors
                .push("Deck version played: this version doesn't exist any more.".into());
        }
    }
    let placement = form.number("placement", "Placement", 1..=100_000);
    let players = form.number("players", "Players", 1..=100_000);
    if let (Some(p), Some(n)) = (placement, players) {
        if p > n {
            form.errors
                .push(format!("Placement: {p} is more than the {n} players."));
        }
    }

    let mut rounds = Vec::new();
    for (n, round) in values.rounds.iter().enumerate() {
        if let Some(round) = read_round(round, n + 1, values.is_team, &mut form.errors) {
            rounds.push(round);
        }
    }

    if !form.errors.is_empty() {
        return Ok(None);
    }

    let (Some(name), Some(date)) = (name, date) else {
        return Ok(None);
    };
    Ok(Some((
        TournamentHead {
            name,
            date,
            location: form.get("location").map(str::to_string),
            version_id,
        },
        Results {
            placement,
            players,
            is_team: values.is_team,
            conclusion: form.get("conclusion").map(str::to_string),
            rounds,
        },
    )))
}

pub struct VersionChoice {
    id: i64,
    name: String,
    selected: bool,
}

/// Every deck version, for "Deck version played".
async fn version_choices(pool: &Pool, current: &str) -> sqlx::Result<Vec<VersionChoice>> {
    let rows: Vec<(i64, Option<String>, String)> = sqlx::query_as(
        "SELECT v.id, deck.name, v.version_name FROM deck_version v
         LEFT JOIN deck ON deck.id = v.deck_id
         ORDER BY deck.name COLLATE NOCASE, v.version_name DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, deck, version)| VersionChoice {
            id,
            name: version_full_name(deck.as_deref(), &version),
            selected: current == id.to_string(),
        })
        .collect())
}

// ---------- add, edit ----------

/// The add and edit pages (`id` is `None` when adding).
#[derive(Template)]
#[template(path = "tournaments/form.html")]
pub struct FormPage {
    id: Option<i64>,
    errors: Vec<String>,
    values: TournamentValues,
    versions: Vec<VersionChoice>,
    label_names: Vec<String>,
    /// The old free-text notes, shown beside the form while not empty.
    notes: String,
}

impl FormPage {
    async fn new(
        pool: &Pool,
        id: Option<i64>,
        errors: Vec<String>,
        values: TournamentValues,
    ) -> AppResult<FormPage> {
        let notes = match id {
            Some(id) => {
                sqlx::query_scalar::<_, Option<String>>("SELECT notes FROM tournament WHERE id = ?")
                    .bind(id)
                    .fetch_optional(pool)
                    .await?
                    .or_404()?
                    .unwrap_or_default()
            }
            None => String::new(),
        };
        Ok(FormPage {
            id,
            errors,
            versions: version_choices(pool, &values.version_id).await?,
            label_names: data::label_names(pool).await?,
            values,
            notes,
        })
    }

    fn action(&self) -> String {
        match self.id {
            Some(id) => format!("/tournaments/{id}/edit"),
            None => "/tournaments/add".into(),
        }
    }
}

/// `/tournaments/add?version=<id>`: that version pre-selected.
#[derive(Debug, Deserialize)]
pub struct AddParams {
    version: Option<i64>,
}

/// `GET /tournaments/add`.
pub async fn add_page(
    State(state): State<AppState>,
    Query(params): Query<AddParams>,
) -> AppResult<FormPage> {
    let values = TournamentValues {
        version_id: params.version.map(|v| v.to_string()).unwrap_or_default(),
        ..TournamentValues::default()
    };
    FormPage::new(&state.pool, None, Vec::new(), values).await
}

/// `POST /tournaments/add`.
pub async fn create(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let values = TournamentValues::from_form(&form);
    let Some((head, results)) = read_form(&state.pool, &mut form, &values).await? else {
        let page = FormPage::new(&state.pool, None, form.errors, values).await?;
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    let id = tournaments::insert(&mut tx, &head, &results).await?;
    tx.commit().await?;
    tracing::info!(
        tournament_id = id,
        rounds = results.rounds.len(),
        "tournament added"
    );

    Ok(Redirect::to(&format!("/tournaments/{id}")).into_response())
}

/// `GET /tournaments/:id/edit`.
pub async fn edit_page(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<FormPage> {
    let which = Which {
        id: Some(id),
        ..Which::default()
    };
    let loaded = data::load(&state.pool, &which).await?;
    let t = loaded.tournaments.first().or_404()?;
    let values = TournamentValues::stored(t, loaded.rounds_of(id));
    FormPage::new(&state.pool, Some(id), Vec::new(), values).await
}

/// `POST /tournaments/:id/edit`.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let values = TournamentValues::from_form(&form);
    let Some((head, results)) = read_form(&state.pool, &mut form, &values).await? else {
        let page = FormPage::new(&state.pool, Some(id), form.errors, values).await?;
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    if !tournaments::update(&mut tx, id, &head, &results, values.clear_notes).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    tracing::info!(
        tournament_id = id,
        rounds = results.rounds.len(),
        "tournament edited"
    );

    Ok(Redirect::to(&format!("/tournaments/{id}")).into_response())
}

/// `POST /tournaments/:id/delete`.
pub async fn delete(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Redirect> {
    let mut tx = begin_write(&state.pool).await?;
    if !tournaments::delete(&mut tx, id).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    tracing::info!(tournament_id = id, "tournament deleted");

    Ok(Redirect::to("/tournaments"))
}
