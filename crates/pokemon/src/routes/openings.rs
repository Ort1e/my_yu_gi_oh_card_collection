//! Booster openings (`store::pokemon::openings`): their list, one opening
//! with its cards and value, and its date, language and note (behind the
//! login; a language changed changes its cards' too).
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
use crate::store::openings::{self, Opening};
use crate::store::{prices, sets};
use common::db::begin_write;
use common::web::error::{AppError, AppResult, OrNotFound};
use common::web::forms::{FormData, Pairs};
use common::AppState;
use pokemon_domain::{parse_language, Catalogue, LANGUAGES};

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
    language: String,
}

impl EditOpeningPage {
    /// The languages it can switch to: those of its catalogue (Japanese
    /// cards are other cards).
    fn languages(&self) -> Vec<(&'static str, &'static str)> {
        let catalogue = Catalogue::of_language(&self.opening.language);
        LANGUAGES
            .into_iter()
            .filter(|(code, _)| Catalogue::of_language(code) == catalogue)
            .collect()
    }

    fn language_is(&self, code: &str) -> bool {
        self.language == code
    }
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
        language: opening.language.clone(),
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
    let language = form
        .get("language")
        .and_then(parse_language)
        .unwrap_or_else(|| opening.language.clone());
    if Catalogue::of_language(&language) != Catalogue::of_language(&opening.language) {
        form.errors.push(
            "Language: Japanese cards are other cards than the French or English ones: \
             add them as their own opening."
                .into(),
        );
    }
    if !form.errors.is_empty() {
        let page = EditOpeningPage {
            opened_on: form.text("opened_on"),
            note: form.text("note"),
            language,
            opening,
            errors: form.errors,
        };
        return Ok(page.into_response());
    }

    // the code and names of its sets in the new language, found by its
    // cards' set ids (a French code means nothing in English); TCGdex
    // before the transaction
    let (mut set_code, mut set_name) = (opening.set_code.clone(), opening.set_name.clone());
    if language != opening.language {
        let set_ids: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT c.set_id FROM pokemon_copy pc JOIN pokemon_card c ON c.id = pc.card_id
             WHERE pc.opening_id = ? ORDER BY c.set_id",
        )
        .bind(id)
        .fetch_all(&state.pool)
        .await?;
        let mut found: Vec<sets::SetInfo> = Vec::new();
        for set_id in set_ids {
            match sets::by_code(&state.pool, &state.http, &language, &set_id).await {
                Ok(sets) => {
                    for set in sets {
                        if !found.iter().any(|known| known.id == set.id) {
                            found.push(set);
                        }
                    }
                }
                Err(e) => tracing::warn!("{e}"),
            }
        }
        found.sort_by(|a, b| b.cards.len().cmp(&a.cards.len()));
        if let Some(main) = found.first() {
            set_code = main.code.clone().unwrap_or_else(|| main.id.clone());
            let names: Vec<&str> = found.iter().map(|set| set.name.as_str()).collect();
            set_name = names.join(" / ");
        }
    }

    let fields = openings::OpeningFields {
        opened_on,
        note: form.get("note"),
        language: &language,
        set_code: &set_code,
        set_name: &set_name,
    };
    let mut tx = begin_write(&state.pool).await?;
    if !openings::update(&mut tx, id, &fields).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(Redirect::to(&format!("/pokemon/openings/{id}")).into_response())
}
