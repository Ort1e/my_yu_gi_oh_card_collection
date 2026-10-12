//! Deck creation with its first version (Django: `AddDeckView`).

use askama::Template;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;

use crate::error::AppResult;
use crate::forms::{FormData, Pairs};
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

pub async fn create(State(state): State<AppState>, Form(pairs): Form<Pairs>) -> AppResult<Response> {
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

    let mut tx = state.pool.begin().await?;
    let deck_id: i64 =
        sqlx::query_scalar("INSERT INTO deck (name, description) VALUES (?, ?) RETURNING id")
            .bind(&name)
            .bind(&description)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (?, ?)")
        .bind(deck_id)
        .bind(&version_name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Redirect::to(&format!("/decks/{deck_id}")).into_response())
}
