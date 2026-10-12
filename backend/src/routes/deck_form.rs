//! Deck creation with its first version (Django: `AddDeckView`), edition,
//! deletion and new empty versions (Django: admin only). The versions
//! themselves are edited in the deck builder.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;

use crate::cards::delete_orphan_proxies;
use crate::db::begin_write;
use crate::error::{AppError, AppResult, OrNotFound};
use crate::forms::{FormData, Pairs};
use crate::routes::decks;
use crate::AppState;

#[derive(Template, Default)]
#[template(path = "add_deck.html")]
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

    Ok(Redirect::to(&format!("/decks/{deck_id}")).into_response())
}

#[derive(Template)]
#[template(path = "edit_deck.html")]
pub struct EditDeckPage {
    id: i64,
    errors: Vec<String>,
    name: String,
    description: String,
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
    tx.commit().await?;

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
    delete_orphan_proxies(&mut tx).await?;
    tx.commit().await?;

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
        let page = decks::load_detail(&state.pool, id, form.errors).await?;
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    sqlx::query_scalar::<_, i64>("SELECT id FROM deck WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .or_404()?;
    let version_id = sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (?, ?)")
        .bind(id)
        .bind(&version_name)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    tx.commit().await?;

    Ok(Redirect::to(&format!("/deck-builder/{version_id}")).into_response())
}
