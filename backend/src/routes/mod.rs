//! One folder per area of the app: `mod.rs` has its pages (list, detail),
//! `form.rs` its create/edit forms. Read-only routes go in `router()`, every
//! write route in `write_router()` (behind the login), and the few private
//! reads in `private_router()` (behind the login too).

mod advent;
mod books;
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
        .route("/books", get(books::list))
        .route("/books/:id", get(books::detail))
        .route("/books/authors", get(books::authors::list))
        .route("/books/authors/:id", get(books::authors::detail))
        .route("/books/series", get(books::series::list))
        .route("/books/series/:id", get(books::series::detail))
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

/// Pages that only read but are private (personal content): behind the login
/// like the write routes. Their files too (see `main::app`).
pub fn private_router() -> Router<AppState> {
    Router::new()
        .route("/advent-calendar", get(advent::list))
        .route("/advent-calendar/:year", get(advent::calendar))
        .route("/advent-calendar/:year/edit", get(advent::form::edit_page))
        .route("/books/advent-calendar", get(books::advent::list))
        .route("/books/advent-calendar/:year", get(books::advent::calendar))
        .route(
            "/books/advent-calendar/:year/edit",
            get(books::advent::form::edit_page),
        )
}

/// Every route that changes data. Kept apart so a login layer can be added
/// here in one place (see CLAUDE.md, "Auth").
pub fn write_router() -> Router<AppState> {
    Router::new()
        .route(
            "/advent-calendar/add",
            get(advent::form::add_page).post(advent::form::create),
        )
        .route(
            "/advent-calendar/:year/days/:day",
            post(advent::form::save_day).layer(DefaultBodyLimit::max(40 * 1024 * 1024)),
        )
        .route("/advent-calendar/:year/lot", post(advent::form::create_lot))
        .route(
            "/advent-calendar/:year/days/:day/rotate",
            post(advent::form::rotate),
        )
        .route(
            "/advent-calendar/:year/days/:day/delete",
            post(advent::form::remove_day),
        )
        .route(
            "/books/add",
            get(books::form::add_page).post(books::form::create),
        )
        .route(
            "/books/:id/edit",
            get(books::form::edit_page).post(books::form::update),
        )
        .route("/books/:id/delete", post(books::form::delete))
        .route("/books/api/isbn/:isbn", get(books::api::isbn))
        .route(
            "/books/advent-calendar/add",
            get(books::advent::form::add_page).post(books::advent::form::create),
        )
        .route(
            "/books/advent-calendar/:year/days/:day",
            post(books::advent::form::save_day).layer(DefaultBodyLimit::max(40 * 1024 * 1024)),
        )
        .route(
            "/books/advent-calendar/:year/days/:day/rotate",
            post(books::advent::form::rotate),
        )
        .route(
            "/books/advent-calendar/:year/days/:day/delete",
            post(books::advent::form::remove_day),
        )
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
