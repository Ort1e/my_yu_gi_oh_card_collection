//! Card edition (Django: admin only). Cards are never deleted: they are the
//! history of the collection.
//!
//! When the English name changes, the card is unlinked from its card data and
//! relinked in the background, like a new card.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use domain::card::{normalize_card_name, CardStatus};

use crate::card_data::spawn_link_cards;
use crate::db::begin_write;
use crate::error::{AppResult, OrNotFound};
use crate::forms::{FormData, Pairs};
use crate::AppState;

/// Card fields as typed.
#[derive(Debug, Default, sqlx::FromRow)]
pub struct CardValues {
    name: String,
    en_name: String,
    code: String,
    status: String,
    is_proxy: bool,
}

#[derive(Template)]
#[template(path = "edit_card.html")]
pub struct EditCardPage {
    id: i64,
    errors: Vec<String>,
    card: CardValues,
}

impl EditCardPage {
    fn statuses(&self) -> [CardStatus; 5] {
        CardStatus::ALL
    }
}

pub async fn page(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<EditCardPage> {
    let card = sqlx::query_as(
        "SELECT name, coalesce(en_name, '') AS en_name, coalesce(code, '') AS code,
            coalesce(last_known_status, '') AS status, is_proxy
         FROM card WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .or_404()?;

    Ok(EditCardPage {
        id,
        errors: Vec::new(),
        card,
    })
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let name = form.required("name", "Name");
    let status = match form.get("status") {
        None => None,
        Some(code) => {
            let status = CardStatus::from_code(code);
            if status.is_none() {
                form.errors.push("Invalid status.".into());
            }
            status
        }
    };

    let (Some(name), true) = (name, form.errors.is_empty()) else {
        return Ok(EditCardPage {
            id,
            card: CardValues {
                name: form.text("name"),
                en_name: form.text("en_name"),
                code: form.text("code"),
                status: form.text("status"),
                is_proxy: form.checked("is_proxy"),
            },
            errors: form.errors,
        }
        .into_response());
    };

    let en_name = normalize_card_name(form.get("en_name").unwrap_or(&name));

    let mut tx = begin_write(&state.pool).await?;
    let old_en_name: Option<String> =
        sqlx::query_scalar::<_, Option<String>>("SELECT en_name FROM card WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .or_404()?;
    let renamed = old_en_name.as_deref() != Some(en_name.as_str());

    sqlx::query(
        "UPDATE card SET name = ?, en_name = ?, code = ?, last_known_status = ?, is_proxy = ?,
            card_data_id = CASE WHEN ? THEN NULL ELSE card_data_id END
         WHERE id = ?",
    )
    .bind(&name)
    .bind(&en_name)
    .bind(form.get("code"))
    .bind(status.map(|s| s.code()))
    .bind(form.checked("is_proxy"))
    .bind(renamed)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(card_id = id, relinked = renamed, "card edited");

    if renamed {
        spawn_link_cards(state, vec![id]);
    }
    Ok(Redirect::to(&format!("/cards/{id}")).into_response())
}
