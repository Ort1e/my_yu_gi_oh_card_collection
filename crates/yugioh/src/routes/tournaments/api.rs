//! The live page's JSON API (`static/js/tournament_live.js`), behind the login
//! like the deck builder's (401 `{"error": …}`).
//!
//! `PUT /api/tournaments/:id/rounds/:number` **replaces** round `number`
//! (or adds it when it is the next one), so a resend after a lost answer does
//! no harm; `PATCH /api/tournaments/:id` sets the end of the day: a field left
//! out stays, `0` or `""` empties it.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use yugioh_domain::tournament::{split_labels, Die, GameResult, RoundResult};

use super::data::{self, Which};
use crate::store::db::begin_write;
use crate::store::tournaments::{self, GameFields, RoundFields};
use crate::web::error::{bad_request, AppError, AppResult};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct GameInput {
    result: String,
    went_first: Option<bool>,
    comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RoundInput {
    opponent_deck: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    die: Option<String>,
    result: String,
    team_result: Option<String>,
    comment: Option<String>,
    #[serde(default)]
    games: Vec<GameInput>,
}

/// Blank text is nothing.
fn text(value: Option<String>) -> Option<String> {
    value
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

impl RoundInput {
    fn into_fields(self) -> AppResult<RoundFields> {
        let Some(result) = RoundResult::from_code(&self.result) else {
            return bad_request("Unknown round result.");
        };
        let team_result = match text(self.team_result) {
            None => None,
            Some(code) => match RoundResult::from_code(&code) {
                Some(r) => Some(r),
                None => return bad_request("Unknown team result."),
            },
        };
        let die = match text(self.die) {
            None => None,
            Some(code) => match Die::from_code(&code) {
                Some(d) => Some(d),
                None => return bad_request("Unknown die roll."),
            },
        };
        let mut games = Vec::new();
        for game in self.games {
            let Some(result) = GameResult::from_code(&game.result) else {
                return bad_request("Unknown game result.");
            };
            games.push(GameFields {
                result,
                went_first: game.went_first,
                comment: text(game.comment),
            });
        }

        Ok(RoundFields {
            opponent_deck: text(self.opponent_deck),
            labels: split_labels(&self.labels.join(",")),
            die,
            result,
            team_result,
            comment: text(self.comment),
            games,
        })
    }
}

/// `PUT /api/tournaments/:id/rounds/:number`.
pub async fn put_round(
    State(state): State<AppState>,
    Path((id, number)): Path<(i64, i64)>,
    Json(input): Json<RoundInput>,
) -> AppResult<Json<Value>> {
    let round = input.into_fields()?;

    let mut tx = begin_write(&state.pool).await?;
    let known: Option<i64> = sqlx::query_scalar("SELECT id FROM tournament WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if known.is_none() {
        return Err(AppError::NotFound);
    }
    if !tournaments::set_round(&mut tx, id, number, &round).await? {
        return bad_request("Rounds are numbered 1, 2, 3…: this one would leave a gap.");
    }
    tx.commit().await?;
    tracing::info!(tournament_id = id, round = number, "live round saved");

    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct EndInput {
    placement: Option<i64>,
    players: Option<i64>,
    conclusion: Option<String>,
}

/// `PATCH /api/tournaments/:id`.
pub async fn patch_end(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<EndInput>,
) -> AppResult<Json<Value>> {
    if input.placement.is_some_and(|p| p < 0) || input.players.is_some_and(|p| p < 0) {
        return bad_request("Placement and players can't be negative.");
    }

    let loaded = data::load(
        &state.pool,
        &Which {
            id: Some(id),
            ..Which::default()
        },
    )
    .await?;
    let Some(current) = loaded.tournaments.first() else {
        return Err(AppError::NotFound);
    };

    let placement = input
        .placement
        .map_or(current.placement, |p| (p > 0).then_some(p));
    let players = input
        .players
        .map_or(current.players, |p| (p > 0).then_some(p));
    let conclusion = match input.conclusion {
        Some(c) => text(Some(c)),
        None => current.conclusion.clone(),
    };

    let mut tx = begin_write(&state.pool).await?;
    if !tournaments::set_end(&mut tx, id, placement, players, conclusion.as_deref()).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    tracing::info!(tournament_id = id, "live end of day saved");

    Ok(Json(json!({ "ok": true })))
}
