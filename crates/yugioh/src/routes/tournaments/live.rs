//! The phone page for noting a tournament round by round
//! (`static/js/tournament_live.js`, docs/tour.md, "Tournaments").
//!
//! `GET /tournaments/live` goes to today's tournament, or shows a one-tap
//! start form pre-filled from the latest tournament; `POST` creates it.
//! `GET /tournaments/:id/live` is the page itself: its data (rounds, the
//! chips of the decks met lately, every label) go to the script as JSON, the
//! saves through `api.rs`. Private: never in the public copy.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Form, Json};
use serde::Serialize;

use super::data::{self, StartDefaults, Which};
use super::form::{version_choices, VersionChoice};
use crate::store::card_data::ensure_archetypes;
use crate::store::db::{begin_write, today};
use crate::store::tournaments::{self, Played, Results, TournamentHead};
use crate::web::error::{AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// How far back the chips' counting goes.
const CHIPS_DAYS: i64 = 90;

#[derive(Template)]
#[template(path = "tournaments/live_start.html")]
pub struct StartPage {
    errors: Vec<String>,
    name: String,
    location: String,
    is_team: bool,
    versions: Vec<VersionChoice>,
}

impl StartPage {
    async fn new(
        state: &AppState,
        errors: Vec<String>,
        defaults: StartDefaults,
    ) -> AppResult<StartPage> {
        let current = defaults
            .version_id
            .map(|v| v.to_string())
            .unwrap_or_default();
        Ok(StartPage {
            errors,
            name: defaults.name,
            location: defaults.location,
            is_team: defaults.is_team,
            versions: version_choices(&state.pool, &current).await?,
        })
    }
}

/// `GET /tournaments/live`.
pub async fn start_page(State(state): State<AppState>) -> AppResult<Response> {
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM tournament WHERE date = ? ORDER BY id DESC LIMIT 1")
            .bind(today().to_string())
            .fetch_optional(&state.pool)
            .await?;
    if let Some(id) = existing {
        return Ok(Redirect::to(&format!("/tournaments/{id}/live")).into_response());
    }

    let defaults = data::start_defaults(&state.pool).await?;
    Ok(StartPage::new(&state, Vec::new(), defaults)
        .await?
        .into_response())
}

/// `POST /tournaments/live`: the tournament, today, no round yet.
pub async fn start(State(state): State<AppState>, Form(pairs): Form<Pairs>) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let name = form.required("name", "Name");
    let is_team = form.checked("is_team");
    let version = if form.text("version_id").is_empty() {
        None
    } else {
        form.id("version_id", "Deck list")
    };
    if let Some(version) = version {
        let known: Option<i64> = sqlx::query_scalar("SELECT id FROM deck_version WHERE id = ?")
            .bind(version)
            .fetch_optional(&state.pool)
            .await?;
        if known.is_none() {
            form.errors
                .push("Deck list: this version doesn't exist any more.".into());
        }
    }
    let Some(name) = name.filter(|_| form.errors.is_empty()) else {
        let defaults = StartDefaults {
            name: form.text("name"),
            location: form.text("location"),
            is_team,
            version_id: version,
        };
        let page = StartPage::new(&state, form.errors, defaults).await?;
        return Ok(page.into_response());
    };

    let location = form.text("location");
    let head = TournamentHead {
        name,
        date: today(),
        location: (!location.trim().is_empty()).then(|| location.trim().to_string()),
        played: version.map_or(Played::NotNoted, Played::Version),
    };
    let results = Results {
        is_team,
        ..Results::default()
    };

    let mut tx = begin_write(&state.pool).await?;
    let id = tournaments::insert(&mut tx, &head, &results).await?;
    tx.commit().await?;
    tracing::info!(tournament_id = id, "live tournament started");

    Ok(Redirect::to(&format!("/tournaments/{id}/live")).into_response())
}

#[derive(Serialize)]
struct GameJson {
    result: &'static str,
    went_first: Option<bool>,
    comment: String,
}

#[derive(Serialize)]
struct RoundJson {
    number: i64,
    opponent_deck: String,
    labels: Vec<String>,
    die: &'static str,
    result: &'static str,
    team_result: &'static str,
    comment: String,
    games: Vec<GameJson>,
}

#[derive(Serialize)]
struct EndJson {
    placement: Option<i64>,
    players: Option<i64>,
    conclusion: String,
}

/// What the script starts from.
#[derive(Serialize)]
pub struct LiveJson {
    id: i64,
    name: String,
    is_team: bool,
    rounds: Vec<RoundJson>,
    /// The labels met most lately, most often first.
    chips: Vec<String>,
    /// Every label, for the search field.
    labels: Vec<String>,
    end: EndJson,
}

#[derive(Template)]
#[template(path = "tournaments/live.html")]
pub struct LivePage {
    id: i64,
    name: String,
    deck: String,
    is_team: bool,
    data_json: String,
}

/// The live data of tournament `id` (404 when none), and what the page's
/// head shows: the deck played.
async fn live_data(state: &AppState, id: i64) -> AppResult<(LiveJson, String)> {
    ensure_archetypes(&state.pool, &state.http).await?;
    let loaded = data::load(
        &state.pool,
        &Which {
            id: Some(id),
            ..Which::default()
        },
    )
    .await?;
    let t = loaded.tournaments.first().or_404()?;

    let since = today() - chrono::Duration::days(CHIPS_DAYS);
    let json = LiveJson {
        id,
        name: t.name.clone(),
        is_team: t.is_team,
        rounds: loaded
            .rounds_of(id)
            .iter()
            .map(|r| RoundJson {
                number: r.number,
                opponent_deck: r.opponent_deck.clone().unwrap_or_default(),
                labels: r.labels.clone(),
                die: r.die.map_or("", |d| d.code()),
                result: r.result.code(),
                team_result: r.team_result.map_or("", |t| t.code()),
                comment: r.comment.clone().unwrap_or_default(),
                games: r
                    .games
                    .iter()
                    .map(|g| GameJson {
                        result: g.result.code(),
                        went_first: g.went_first,
                        comment: g.comment.clone().unwrap_or_default(),
                    })
                    .collect(),
            })
            .collect(),
        chips: data::frequent_opponent_labels(&state.pool, &since.to_string()).await?,
        labels: data::label_names(&state.pool).await?,
        end: EndJson {
            placement: t.placement,
            players: t.players,
            conclusion: t.conclusion.clone().unwrap_or_default(),
        },
    };
    Ok((json, t.version_full_name().unwrap_or_default()))
}

/// `GET /tournaments/:id/live`.
pub async fn page(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<LivePage> {
    let (json, deck) = live_data(&state, id).await?;

    // no `</script>` can end the data block early
    let data_json = serde_json::to_string(&json)
        .unwrap_or_else(|_| "null".into())
        .replace('<', "\\u003c");

    Ok(LivePage {
        id,
        name: json.name,
        deck,
        is_team: json.is_team,
        data_json,
    })
}

/// `GET /api/tournaments/:id/live`: the same data as the page's, fresh (a
/// page opened from the phone's cache asks for it).
pub async fn data_api(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<LiveJson>> {
    Ok(Json(live_data(&state, id).await?.0))
}
