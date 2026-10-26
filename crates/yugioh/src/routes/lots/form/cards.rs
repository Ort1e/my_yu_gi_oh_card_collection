//! Cards added to an existing lot, from its page ("Add cards").

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;

use crate::routes::lots::load_detail;
use crate::store::card_data::spawn_link_cards;
use crate::store::cards::insert_lot_cards;
use crate::store::db::{begin_write, exists};
use crate::web::card_rows::CardRows;
use crate::web::error::AppResult;
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

pub async fn add_cards(
    State(state): State<AppState>,
    Path(lot_id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let cards = form.new_cards();
    if cards.is_empty() && form.errors.is_empty() {
        form.errors.push("Card 1: name is required.".into());
    }
    if !form.errors.is_empty() {
        let rows = form.card_rows();
        let page = load_detail(&state.pool, lot_id, form.errors, rows).await?;
        return Ok(page.into_response());
    }

    let mut tx = begin_write(&state.pool).await?;
    exists(&mut tx, "lot", lot_id).await?;
    let card_ids = insert_lot_cards(&mut tx, lot_id, &cards).await?;
    tx.commit().await?;
    tracing::info!(lot_id, cards = card_ids.len(), "cards added to lot");

    spawn_link_cards(state, card_ids);
    Ok(Redirect::to(&format!("/lots/{lot_id}")).into_response())
}
