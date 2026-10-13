//! Seller edition (Django: admin only). Sellers are never deleted.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;

use crate::db::begin_write;
use crate::error::{AppError, AppResult, OrNotFound};
use crate::forms::{FormData, Pairs};
use crate::AppState;

/// Seller fields as typed.
#[derive(Debug, Default, sqlx::FromRow)]
pub struct SellerValues {
    name: String,
    is_person: bool,
    source: String,
}

#[derive(Template)]
#[template(path = "edit_seller.html")]
pub struct EditSellerPage {
    id: i64,
    errors: Vec<String>,
    seller: SellerValues,
    sources: Vec<(i64, String)>,
}

impl EditSellerPage {
    async fn new(
        state: &AppState,
        id: i64,
        errors: Vec<String>,
        seller: SellerValues,
    ) -> AppResult<EditSellerPage> {
        Ok(EditSellerPage {
            id,
            errors,
            seller,
            sources: sqlx::query_as("SELECT id, name FROM seller_source ORDER BY name")
                .fetch_all(&state.pool)
                .await?,
        })
    }

    fn source_is(&self, id: &i64) -> bool {
        self.seller.source == id.to_string()
    }
}

pub async fn page(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<EditSellerPage> {
    let seller = sqlx::query_as(
        "SELECT name, is_person, coalesce(CAST(source_id AS TEXT), '') AS source
         FROM seller WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .or_404()?;
    EditSellerPage::new(&state, id, Vec::new(), seller).await
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let name = form.required("name", "Name");
    let source_id = form.id("source", "Source");

    if let Some(name) = &name {
        let taken: Option<i64> =
            sqlx::query_scalar("SELECT id FROM seller WHERE name = ? AND id != ?")
                .bind(name)
                .bind(id)
                .fetch_optional(&state.pool)
                .await?;
        if taken.is_some() {
            form.errors
                .push(format!("A seller named \"{name}\" already exists."));
        }
    }

    let (Some(name), true) = (name, form.errors.is_empty()) else {
        let seller = SellerValues {
            name: form.text("name"),
            is_person: form.checked("is_person"),
            source: form.text("source"),
        };
        let page = EditSellerPage::new(&state, id, form.errors, seller).await?;
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    let updated =
        sqlx::query("UPDATE seller SET name = ?, is_person = ?, source_id = ? WHERE id = ?")
            .bind(&name)
            .bind(form.checked("is_person"))
            .bind(source_id)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    tracing::info!(seller_id = id, "seller edited");

    Ok(Redirect::to(&format!("/sellers/{id}")).into_response())
}
