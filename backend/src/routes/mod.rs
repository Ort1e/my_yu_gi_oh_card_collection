mod budget;
mod budget_form;
mod card_form;
mod cards;
mod dashboard;
mod deck_api;
mod deck_form;
mod decks;
mod images;
mod lot_form;
mod lots;
mod rarities;
mod seller_form;
mod sellers;
mod sets;

use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, patch, post, put};
use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(dashboard::index))
        .route("/budget", get(budget::overview))
        .route("/cards", get(cards::list))
        .route("/cards/:id", get(cards::detail))
        .route("/sellers", get(sellers::list))
        .route("/sellers/:id", get(sellers::detail))
        .route("/lots", get(lots::list))
        .route("/lots/:id", get(lots::detail))
        .route("/decks", get(decks::list))
        .route("/decks/:id", get(decks::detail))
        .route("/deck-builder/:id", get(decks::builder))
        .route("/rarities", get(rarities::page))
        .route("/api/sets/:code", get(sets::set_cards))
        .route("/api/deck-versions/:id", get(deck_api::get_version))
        .route("/api/deck-versions/:id/reserve", post(deck_api::reserve))
        .route(
            "/api/deck-versions/:id/categories",
            get(deck_api::categories),
        )
        .route(
            "/api/deck-versions/:id/cards/:card_id/categories",
            get(deck_api::card_categories),
        )
        .route(
            "/api/deck-versions/:id/monte-carlo",
            post(deck_api::monte_carlo),
        )
}

/// Routes that serve files rather than pages (kept out of the request log:
/// a deck builder page loads dozens of card images).
pub fn files_router() -> Router<AppState> {
    Router::new().route("/card-image/:card_data_id", get(images::card_image))
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
        .route(
            "/lots/:id/edit",
            get(lot_form::edit_page).post(lot_form::update),
        )
        .route(
            "/cards/:id/edit",
            get(card_form::page).post(card_form::update),
        )
        .route(
            "/sellers/:id/edit",
            get(seller_form::page).post(seller_form::update),
        )
        .route("/decks/add", get(deck_form::page).post(deck_form::create))
        .route(
            "/decks/:id/edit",
            get(deck_form::edit_page).post(deck_form::update),
        )
        .route("/decks/:id/delete", post(deck_form::delete))
        .route("/decks/:id/versions", post(deck_form::add_version))
        .route("/budget/manage", get(budget_form::page))
        .route("/budget/manage/budget", post(budget_form::set_budget))
        .route("/budget/manage/apports", post(budget_form::add_apport))
        .route(
            "/budget/manage/apports/:id/delete",
            post(budget_form::delete_apport),
        )
        // deck builder API
        .route(
            "/api/deck-versions/:id",
            patch(deck_api::update_version).delete(deck_api::delete_version),
        )
        .route(
            "/api/deck-versions/:id/clone",
            post(deck_api::clone_version),
        )
        .route("/api/deck-versions/:id/proxies", post(deck_api::add_proxy))
        .route(
            "/api/deck-versions/:id/proxies/:card_id",
            delete(deck_api::delete_proxy),
        )
        .route(
            "/api/deck-versions/:id/categories",
            post(deck_api::add_category),
        )
        .route(
            "/api/deck-versions/:id/categories/:category_id",
            delete(deck_api::delete_category),
        )
        .route(
            "/api/deck-versions/:id/cards/:card_id/categories/:category_id",
            put(deck_api::assign_category),
        )
        .route(
            "/api/deck-versions/:id/tournament",
            put(deck_api::save_tournament),
        )
        .route("/api/decks/:id/import-ydke", post(deck_api::import_ydke))
}
