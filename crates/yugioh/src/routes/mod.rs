//! One folder per area of the app: `mod.rs` has its pages (list, detail),
//! `form.rs` its create/edit forms. Read-only routes go in `router()`, every
//! write route in `write_router()` (behind the login). The advent calendars
//! (private) come from the `advent` crate (`advent/` says what is behind
//! their doors).

mod advent;
mod budget;
mod cards;
mod collection;
mod dashboard;
mod decks;
mod lots;
mod reference;
mod sellers;
mod tournaments;

use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, patch, post, put};
use axum::Router;

use crate::AppState;

pub use cards::images::used_files as used_card_pictures;

/// Every route, by who may use them (assembled by `common::app`).
pub fn all() -> common::Routes {
    common::Routes {
        public: router(),
        write: write_router(),
        private: Router::new(),
        files: files_router(),
        private_media: vec!["shipment_files"],
    }
    .merge(::advent::routes::<advent::Cards>())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(dashboard::index))
        .route("/budget", get(budget::overview))
        .route("/cards", get(cards::list))
        .route("/cards/:id", get(cards::detail))
        .route("/collection/value", get(collection::page))
        .route("/sellers", get(sellers::list))
        .route("/sellers/:id", get(sellers::detail))
        .route("/lots", get(lots::list))
        .route("/lots/:id", get(lots::detail))
        .route("/decks", get(decks::list))
        .route("/decks/built", get(decks::built::page))
        .route("/decks/:id", get(decks::detail))
        .route("/deck-builder/:id", get(decks::builder))
        .route("/rarities", get(reference::rarities::page))
        .route("/tournaments", get(tournaments::list))
        .route("/tournaments/labels", get(tournaments::labels::list))
        .route("/tournaments/:id", get(tournaments::detail))
        .route("/api/sets/:code", get(reference::sets::set_cards))
        .route("/api/deck-versions/:id", get(decks::api::get_version))
        .route("/api/deck-versions/:id/reserve", post(decks::api::reserve))
        .route(
            "/api/deck-versions/:id/categories",
            get(decks::api::categories),
        )
        .route(
            "/api/deck-versions/:id/cards/:card_id/categories",
            get(decks::api::card_categories),
        )
        .route(
            "/api/deck-versions/:id/monte-carlo",
            post(decks::api::monte_carlo),
        )
}

/// Routes that serve files rather than pages (kept out of the request log:
/// a deck builder page loads dozens of card images).
pub fn files_router() -> Router<AppState> {
    Router::new().route("/card-image/:card_data_id", get(cards::images::card_image))
}

/// Every route that changes data. Kept apart so a login layer can be added
/// here in one place (see CLAUDE.md, "Auth").
pub fn write_router() -> Router<AppState> {
    Router::new()
        .route("/advent-calendar/:year/lot", post(advent::create_lot))
        .route(
            "/tournaments/add",
            get(tournaments::form::add_page).post(tournaments::form::create),
        )
        .route(
            "/tournaments/:id/edit",
            get(tournaments::form::edit_page).post(tournaments::form::update),
        )
        .route("/tournaments/:id/delete", post(tournaments::form::delete))
        .route(
            "/tournaments/labels/:id/rename",
            post(tournaments::labels::rename),
        )
        .route(
            "/tournaments/labels/:id/merge",
            post(tournaments::labels::merge),
        )
        .route("/lots/add", get(lots::form::page).post(lots::form::create))
        .route(
            "/lots/add/shipment",
            post(lots::form::upload).layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
        .route("/lots/:id/cards", post(lots::form::add_cards))
        .route(
            "/lots/:id/edit",
            get(lots::form::edit_page).post(lots::form::update),
        )
        .route(
            "/cards/:id/edit",
            get(cards::form::page).post(cards::form::update),
        )
        .route(
            "/sellers/:id/edit",
            get(sellers::form::page).post(sellers::form::update),
        )
        .route(
            "/decks/add",
            get(decks::form::page).post(decks::form::create),
        )
        .route(
            "/decks/:id/edit",
            get(decks::form::edit_page).post(decks::form::update),
        )
        .route("/decks/:id/delete", post(decks::form::delete))
        .route("/decks/:id/versions", post(decks::form::add_version))
        .route("/budget/manage", get(budget::form::page))
        .route("/budget/manage/budget", post(budget::form::set_budget))
        .route("/budget/manage/apports", post(budget::form::add_apport))
        .route(
            "/budget/manage/apports/:id/delete",
            post(budget::form::delete_apport),
        )
        // deck builder API
        .route(
            "/api/deck-versions/:id",
            patch(decks::api::update_version).delete(decks::api::delete_version),
        )
        .route(
            "/api/deck-versions/:id/clone",
            post(decks::api::clone_version),
        )
        .route(
            "/api/deck-versions/:id/proxies",
            post(decks::api::add_proxy),
        )
        .route(
            "/api/deck-versions/:id/proxies/:card_id",
            delete(decks::api::delete_proxy),
        )
        .route(
            "/api/deck-versions/:id/proxies/:card_id/replace",
            post(decks::api::replace_proxy),
        )
        .route(
            "/api/deck-versions/:id/categories",
            post(decks::api::add_category),
        )
        .route(
            "/api/deck-versions/:id/categories/:category_id",
            delete(decks::api::delete_category),
        )
        .route(
            "/api/deck-versions/:id/cards/:card_id/categories/:category_id",
            put(decks::api::assign_category),
        )
        .route("/api/decks/:id/import-ydke", post(decks::api::import_ydke))
}
