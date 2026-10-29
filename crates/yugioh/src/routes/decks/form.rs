//! Deck creation with its first version (Django: `AddDeckView`), edition,
//! deletion and new empty versions (Django: admin only). The versions
//! themselves are edited in the deck builder.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use yugioh_domain::tournament::split_labels;

use crate::routes::tournaments::{deck_labels, label_names};
use crate::store::cards::delete_orphan_proxies;
use crate::store::db::{begin_write, exists};
use crate::store::tournaments::set_deck_labels;
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

#[derive(Template, Default)]
#[template(path = "decks/add.html")]
pub struct AddDeckPage {
    errors: Vec<String>,
    name: String,
    description: String,
    version_name: String,
}

pub async fn page() -> AddDeckPage {
    AddDeckPage {
        version_name: "v1".into(),
        ..AddDeckPage::default()
    }
}

pub async fn create(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let name = form.required("name", "Name");
    let version_name = form.required("version_name", "Version name");
    let description = form.get("description").map(str::to_string);

    let (Some(name), Some(version_name)) = (name, version_name) else {
        return Ok(AddDeckPage {
            name: form.text("name"),
            description: form.text("description"),
            version_name: form.text("version_name"),
            errors: form.errors,
        }
        .into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    let deck_id: i64 = sqlx::query("INSERT INTO deck (name, description) VALUES (?, ?)")
        .bind(&name)
        .bind(&description)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (?, ?)")
        .bind(deck_id)
        .bind(&version_name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(deck_id, "deck created");

    Ok(Redirect::to(&format!("/decks/{deck_id}")).into_response())
}

#[derive(Template)]
#[template(path = "decks/edit.html")]
pub struct EditDeckPage {
    id: i64,
    errors: Vec<String>,
    name: String,
    description: String,
    /// Its labels, comma-separated (`yugioh_domain::tournament::split_labels`).
    labels: String,
    label_names: Vec<String>,
}

pub async fn edit_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<EditDeckPage> {
    let (name, description): (String, Option<String>) =
        sqlx::query_as("SELECT name, description FROM deck WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .or_404()?;

    Ok(EditDeckPage {
        id,
        errors: Vec::new(),
        name,
        description: description.unwrap_or_default(),
        labels: deck_labels(&state.pool, id).await?.join(", "),
        label_names: label_names(&state.pool).await?,
    })
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let Some(name) = form.required("name", "Name") else {
        return Ok(EditDeckPage {
            id,
            name: form.text("name"),
            description: form.text("description"),
            labels: form.text("labels"),
            label_names: label_names(&state.pool).await?,
            errors: form.errors,
        }
        .into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    let updated = sqlx::query("UPDATE deck SET name = ?, description = ? WHERE id = ?")
        .bind(&name)
        .bind(form.get("description"))
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    set_deck_labels(&mut tx, id, &split_labels(&form.text("labels"))).await?;
    tx.commit().await?;
    tracing::info!(deck_id = id, "deck edited");

    Ok(Redirect::to(&format!("/decks/{id}")).into_response())
}

/// Delete the deck with its versions (and their categories), then the proxy
/// cards no other deck uses.
pub async fn delete(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Redirect> {
    let mut tx = begin_write(&state.pool).await?;
    let deleted = sqlx::query("DELETE FROM deck WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(AppError::NotFound);
    }
    let proxies_deleted = delete_orphan_proxies(&mut tx).await?;
    tx.commit().await?;
    tracing::info!(deck_id = id, proxies_deleted, "deck deleted");

    Ok(Redirect::to("/decks"))
}

/// New empty version of the deck, opened in the deck builder.
pub async fn add_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let Some(version_name) = form.required("version_name", "Version name") else {
        let page = super::load_detail(&state.pool, id, form.errors).await?;
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    exists(&mut tx, "deck", id).await?;
    let version_id = sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (?, ?)")
        .bind(id)
        .bind(&version_name)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    tx.commit().await?;
    tracing::info!(deck_id = id, version_id, "deck version created");

    Ok(Redirect::to(&format!("/deck-builder/{version_id}")).into_response())
}
