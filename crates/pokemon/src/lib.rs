//! The Pokémon card collection: every route starts with `/pokemon`. Card
//! data and prices from TCGdex (the Pokémon TCG API for the pictures it
//! lacks); copies from boosters or singles, booster openings, a wishlist, a
//! rarity guide. Reads are public, writes and lookups behind the login.
//!
//! - `routes/`: the pages (collection, a copy, add/edit, booster openings,
//!   wishlist, rarity guide, the card finder's JSON, pictures);
//! - `store/`: TCGdex and the Pokémon TCG API clients and caches, copy,
//!   wish and opening writes, prices;
//! - the pure rules: the `pokemon-domain` crate.

pub mod public;
pub mod routes;
pub mod store;
#[cfg(test)]
mod tests;

use axum::routing::{get, post};
use axum::Router;
use common::{AppState, Routes};

/// The Pokémon collection's routes (assembled by `common::app`).
pub fn routes() -> Routes {
    Routes {
        public: Router::new()
            .route("/pokemon", get(routes::list))
            .route("/pokemon/rarities", get(routes::rarities::page))
            .route("/pokemon/wishlist", get(routes::wishlist::list))
            .route("/pokemon/openings", get(routes::openings::list))
            .route("/pokemon/openings/:id", get(routes::openings::detail))
            .route("/pokemon/:id", get(routes::detail)),
        write: Router::new()
            .route(
                "/pokemon/add",
                get(routes::form::add_page).post(routes::form::create),
            )
            .route(
                "/pokemon/add/booster",
                get(routes::booster::page).post(routes::booster::submit),
            )
            .route(
                "/pokemon/:id/edit",
                get(routes::form::edit_page).post(routes::form::update),
            )
            .route(
                "/pokemon/wishlist/add",
                get(routes::wishlist::add_page).post(routes::wishlist::create),
            )
            .route(
                "/pokemon/wishlist/:id/edit",
                get(routes::wishlist::edit_page).post(routes::wishlist::update),
            )
            .route(
                "/pokemon/wishlist/:id/delete",
                post(routes::wishlist::delete),
            )
            .route("/pokemon/wishlist/:id/got", post(routes::wishlist::got))
            .route(
                "/pokemon/openings/:id/edit",
                get(routes::openings::edit_page).post(routes::openings::update),
            )
            .route("/pokemon/api/search", get(routes::api::search))
            .route("/pokemon/api/card", get(routes::api::card)),
        private: Router::new(),
        files: Router::new().route(
            "/pokemon/card-image/:card_id/:lang",
            get(routes::images::card_image),
        ),
        private_media: Vec::new(),
    }
}

/// The server with the Pokémon collection only (its tests).
pub fn app(state: AppState) -> Router {
    common::app(state, vec![routes()])
}
