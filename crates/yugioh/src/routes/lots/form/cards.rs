//! Cards added to an existing lot: `/lots/:id/add-cards` (the lot page only
//! links to it, reading and entering stay separate pages).

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;

use crate::store::card_data::spawn_link_cards;
use crate::store::cards::insert_lot_cards;
use crate::store::db::{begin_write, exists, Pool};
use crate::store::models::LotRow;
use crate::web::card_rows::{CardRowValues, CardRows};
use crate::web::error::{AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

#[derive(Template)]
#[template(path = "lots/add_cards.html")]
pub struct AddCardsPage {
    lot: LotRow,
    /// Errors and card rows of the last submission (at least one row).
    errors: Vec<String>,
    rows: Vec<CardRowValues>,
}

impl AddCardsPage {
    async fn load(
        pool: &Pool,
        lot_id: i64,
        errors: Vec<String>,
        mut rows: Vec<CardRowValues>,
    ) -> AppResult<AddCardsPage> {
        if rows.is_empty() {
            rows.push(CardRowValues::default());
        }
        let lot: LotRow = sqlx::query_as(&format!("{} WHERE lot.id = ?", LotRow::SELECT))
            .bind(lot_id)
            .fetch_optional(pool)
            .await?
            .or_404()?;
        Ok(AddCardsPage { lot, errors, rows })
    }

    fn blank_row(&self) -> CardRowValues {
        CardRowValues::default()
    }

    fn statuses(&self) -> [yugioh_domain::card::CardStatus; 5] {
        yugioh_domain::card::CardStatus::ALL
    }

    fn rarities(&self) -> [yugioh_domain::rarity::Rarity; 34] {
        yugioh_domain::rarity::Rarity::ALL
    }
}

pub async fn add_cards_page(
    State(state): State<AppState>,
    Path(lot_id): Path<i64>,
) -> AppResult<AddCardsPage> {
    AddCardsPage::load(&state.pool, lot_id, Vec::new(), Vec::new()).await
}

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
        let page = AddCardsPage::load(&state.pool, lot_id, form.errors, rows).await?;
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
