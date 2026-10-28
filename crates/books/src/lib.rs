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

use axum::routing::{get, post};
use axum::Router;
use common::{AppState, Routes};

/// The book collection's routes (assembled by `common::app`).
pub fn routes() -> Routes {
    Routes {
        collection: "books",
        public: Router::new()
            .route("/books", get(routes::list))
            .route("/books/:id", get(routes::detail))
            .route("/books/authors", get(routes::authors::list))
            .route("/books/authors/:id", get(routes::authors::detail))
            .route("/books/series", get(routes::series::list))
            .route("/books/series/:id", get(routes::series::detail)),
        private: Router::new(),
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
            .route("/books/api/isbn/:isbn", get(routes::api::isbn)),
        files: Router::new(),
        private_media: Vec::new(),
    }
    .merge(advent::routes::<routes::advent::Books>())
}

/// The server with the book collection only (its tests).
pub fn app(state: AppState) -> Router {
    common::app(state, vec![routes()])
}
