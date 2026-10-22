//! `/tournaments/labels`: the deck labels (archetype/engine names) with
//! their uses; rename one, or merge it into another (a second spelling of
//! the same strategy).

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;

use crate::store::db::{begin_write, Pool};
use crate::store::tournaments;
use crate::web::error::AppResult;
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct LabelRow {
    id: i64,
    name: String,
    /// Your decks with it.
    decks: i64,
    /// Opponents' rounds with it.
    rounds: i64,
}

#[derive(Template)]
#[template(path = "tournaments/labels.html")]
pub struct LabelsPage {
    errors: Vec<String>,
    labels: Vec<LabelRow>,
}

async fn page(pool: &Pool, errors: Vec<String>) -> AppResult<LabelsPage> {
    let labels = sqlx::query_as(
        "SELECT l.id, l.name,
                (SELECT count(*) FROM deck_deck_label WHERE label_id = l.id) AS decks,
                (SELECT count(*) FROM tournament_round_label WHERE label_id = l.id) AS rounds
         FROM deck_label l ORDER BY l.name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    Ok(LabelsPage { errors, labels })
}

/// `GET /tournaments/labels`.
pub async fn list(State(state): State<AppState>) -> AppResult<LabelsPage> {
    page(&state.pool, Vec::new()).await
}

/// `POST /tournaments/labels/:id/rename` (`name`).
pub async fn rename(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let Some(name) = form.required("name", "The new name") else {
        return Ok(page(&state.pool, form.errors).await?.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    if !tournaments::rename_label(&mut tx, id, &name).await? {
        drop(tx);
        let error = format!(
            "Another label is already named \"{name}\" (or this one is gone): \
             merge them instead."
        );
        return Ok(page(&state.pool, vec![error]).await?.into_response());
    }
    tx.commit().await?;
    tracing::info!(label_id = id, "deck label renamed");

    Ok(Redirect::to("/tournaments/labels").into_response())
}

/// `POST /tournaments/labels/:id/merge` (`into`: the label kept).
pub async fn merge(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let into = form.id("into", "Merge into");
    let into = match into {
        Some(into) if into != id => into,
        _ => {
            let error = "Merge into: choose another label.".to_string();
            return Ok(page(&state.pool, vec![error]).await?.into_response());
        }
    };

    let mut tx = begin_write(&state.pool).await?;
    let known: i64 = sqlx::query_scalar("SELECT count(*) FROM deck_label WHERE id IN (?, ?)")
        .bind(id)
        .bind(into)
        .fetch_one(&mut *tx)
        .await?;
    if known != 2 {
        drop(tx);
        let error = "One of these labels doesn't exist any more.".to_string();
        return Ok(page(&state.pool, vec![error]).await?.into_response());
    }
    tournaments::merge_labels(&mut tx, id, into).await?;
    tx.commit().await?;
    tracing::info!(label_id = id, into, "deck label merged");

    Ok(Redirect::to("/tournaments/labels").into_response())
}
