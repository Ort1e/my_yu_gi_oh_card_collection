mod budget;
mod budget_form;
mod cards;
mod dashboard;
mod deck_form;
mod decks;
mod images;
mod lot_form;
mod lots;
mod sellers;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(dashboard::index))
        .route("/budget", get(budget::overview))
        .route("/cards", get(cards::list))
        .route("/cards/:id", get(cards::detail))
        .route("/card-image/:card_data_id", get(images::card_image))
        .route("/sellers", get(sellers::list))
        .route("/sellers/:id", get(sellers::detail))
        .route("/lots", get(lots::list))
        .route("/lots/:id", get(lots::detail))
        .route("/decks", get(decks::list))
        .route("/decks/:id", get(decks::detail))
}

/// Every route that changes data. Kept apart so a login layer can be added
/// here in one place (see CLAUDE.md, "Auth").
pub fn write_router() -> Router<AppState> {
    Router::new()
        .route("/lots/add", get(lot_form::page).post(lot_form::create))
        .route(
            "/lots/add/shipment",
            post(lot_form::upload).layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
        .route("/lots/:id/cards", post(lot_form::add_cards))
        .route("/decks/add", get(deck_form::page).post(deck_form::create))
        .route("/budget/manage", get(budget_form::page))
        .route("/budget/manage/budget", post(budget_form::set_budget))
        .route("/budget/manage/apports", post(budget_form::add_apport))
        .route("/budget/manage/apports/:id/delete", post(budget_form::delete_apport))
}
