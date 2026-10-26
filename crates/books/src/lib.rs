//! The book collection (the novels at home): every route starts with
//! `/books`. Public reads like the cards; the writes and the ISBN lookup
//! behind the login; the book advent calendars private.
//!
//! - `routes/`: the pages (list, detail, add/edit/delete with the ISBN
//!   lookup and barcode scan, authors, series, the ISBN lookup's JSON, the
//!   advent calendars);
//! - `store/`: book writes (`books.rs`) and the ISBN lookup with its cache
//!   (`book_data.rs`);
//! - the pure rules: the `books-domain` crate.

pub mod routes;
pub mod store;
#[cfg(test)]
mod tests;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use common::{AppState, Routes};

/// The book collection's routes (assembled by `common::app`).
pub fn routes() -> Routes {
    Routes {
        public: Router::new()
            .route("/books", get(routes::list))
            .route("/books/:id", get(routes::detail))
            .route("/books/authors", get(routes::authors::list))
            .route("/books/authors/:id", get(routes::authors::detail))
            .route("/books/series", get(routes::series::list))
            .route("/books/series/:id", get(routes::series::detail)),
        private: Router::new()
            .route("/books/advent-calendar", get(routes::advent::list))
            .route(
                "/books/advent-calendar/:year",
                get(routes::advent::calendar),
            )
            .route(
                "/books/advent-calendar/:year/edit",
                get(routes::advent::form::edit_page),
            ),
        write: Router::new()
            .route(
                "/books/add",
                get(routes::form::add_page).post(routes::form::create),
            )
            .route(
                "/books/:id/edit",
                get(routes::form::edit_page).post(routes::form::update),
            )
            .route("/books/:id/delete", post(routes::form::delete))
            .route("/books/api/isbn/:isbn", get(routes::api::isbn))
            .route(
                "/books/advent-calendar/add",
                get(routes::advent::form::add_page).post(routes::advent::form::create),
            )
            .route(
                "/books/advent-calendar/:year/days/:day",
                post(routes::advent::form::save_day).layer(DefaultBodyLimit::max(40 * 1024 * 1024)),
            )
            .route(
                "/books/advent-calendar/:year/days/:day/rotate",
                post(routes::advent::form::rotate),
            )
            .route(
                "/books/advent-calendar/:year/days/:day/delete",
                post(routes::advent::form::remove_day),
            ),
        files: Router::new(),
        private_media: vec!["book_advent_calendar_photos"],
    }
}

/// The server with the book collection only (its tests).
pub fn app(state: AppState) -> Router {
    common::app(state, vec![routes()])
}
