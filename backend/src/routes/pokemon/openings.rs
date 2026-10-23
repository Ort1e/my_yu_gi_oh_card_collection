//! Booster openings (`store::pokemon::openings`): their list, one opening
//! with its cards and value, and its date and note (behind the login).
//! Cards are added to an opening with the booster form
//! (`/pokemon/add/booster?opening=<id>`) or the one-card form (its
//! "From a booster opening" field, also on a copy's edit page).

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use rust_decimal::Decimal;
use serde::Deserialize;

use super::form::Added;
use super::{money, CopyRow};
use crate::store::db::begin_write;
use crate::store::pokemon::openings::{self, Opening};
use crate::store::pokemon::prices;
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// An opening in the list, with its value.
pub struct OpeningRow {
    opening: Opening,
    value: Decimal,
    /// Its best card: `(copy id, name, value)`.
    best: Option<(i64, String, String)>,
}

impl OpeningRow {
    fn value_text(&self) -> String {
        money(Some(self.value))
    }
}

#[derive(Template)]
#[template(path = "pokemon/openings.html")]
pub struct OpeningsPage {
    openings: Vec<OpeningRow>,
}

/// The copies of an opening, valued.
async fn copies(state: &AppState, opening_id: i64) -> AppResult<Vec<CopyRow>> {
    let mut copies: Vec<CopyRow> = sqlx::query_as(&format!(
        "{} WHERE pc.opening_id = ? ORDER BY c.set_id,
             CAST(ltrim(c.local_id, 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz') AS INTEGER),
             c.local_id, pc.id",
        CopyRow::SELECT
    ))
    .bind(opening_id)
    .fetch_all(&state.pool)
    .await?;
    let latest = prices::latest(&state.pool).await?;
    for copy in &mut copies {
        copy.set_value(&latest);
    }
    Ok(copies)
}

/// `/pokemon/openings`.
pub async fn list(State(state): State<AppState>) -> AppResult<OpeningsPage> {
    let mut rows = Vec::new();
    for opening in openings::all(&state.pool).await? {
        let copies = copies(&state, opening.id).await?;
        let value = copies
            .iter()
            .filter_map(|c| c.value)
            .sum::<Decimal>()
            .round_dp(2);
        let best = copies
            .iter()
            .filter(|c| c.value.is_some())
            .max_by_key(|c| c.value)
            .map(|c| (c.id, c.name(), c.value_text()));
        rows.push(OpeningRow {
            opening,
            value,
            best,
        });
    }
    Ok(OpeningsPage { openings: rows })
}

#[derive(Template)]
#[template(path = "pokemon/opening.html")]
pub struct OpeningPage {
    opening: Opening,
    copies: Vec<CopyRow>,
    value: Decimal,
    /// Just added (after a save), with the wishes they fulfil.
    added: Vec<Added>,
}

impl OpeningPage {
    fn value_text(&self) -> String {
        money(Some(self.value))
    }
}

#[derive(Debug, Deserialize)]
pub struct OpeningParams {
    added: Option<String>,
}

/// `/pokemon/openings/:id`.
pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<OpeningParams>,
) -> AppResult<OpeningPage> {
    let opening = openings::get(&state.pool, id).await?.or_404()?;
    let copies = copies(&state, id).await?;
    let value = copies
        .iter()
        .filter_map(|c| c.value)
        .sum::<Decimal>()
        .round_dp(2);
    Ok(OpeningPage {
        added: super::booster::added_copies(&state, params.added.as_deref()).await?,
        opening,
        copies,
        value,
    })
}

#[derive(Template)]
#[template(path = "pokemon/opening_edit.html")]
pub struct EditOpeningPage {
    opening: Opening,
    errors: Vec<String>,
    opened_on: String,
    note: String,
}

/// `GET /pokemon/openings/:id/edit`.
pub async fn edit_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<EditOpeningPage> {
    let opening = openings::get(&state.pool, id).await?.or_404()?;
    Ok(EditOpeningPage {
        opened_on: opening.opened_on.clone().unwrap_or_default(),
        note: opening.note.clone().unwrap_or_default(),
        opening,
        errors: Vec::new(),
    })
}

/// `POST /pokemon/openings/:id/edit`: the date (its cards' too) and the note.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let opening = openings::get(&state.pool, id).await?.or_404()?;
    let mut form = FormData::new(pairs);
    let opened_on = form.date("opened_on", "Opened on");
    if !form.errors.is_empty() {
        let page = EditOpeningPage {
            opened_on: form.text("opened_on"),
            note: form.text("note"),
            opening,
            errors: form.errors,
        };
        return Ok(page.into_response());
    }

    let mut tx = begin_write(&state.pool).await?;
    if !openings::update(&mut tx, id, opened_on, form.get("note")).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(Redirect::to(&format!("/pokemon/openings/{id}")).into_response())
}
