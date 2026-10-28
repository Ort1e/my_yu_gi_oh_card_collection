//! The Yu-Gi-Oh! collection: the cards owned, purchase and sale lots,
//! sellers, the monthly budget, decks (versions, categories, YDKE, the deck
//! builder's JSON API, built decks), tournaments, ban lists, the collection
//! value, the advent calendar, market prices.
//!
//! - `routes/`: the pages and the deck builder's API, one folder per area;
//! - `store/`: card writes and shared SQL, ygoprodeck card data, prices,
//!   the collection value, shipment files, tournaments;
//! - `web/`: common's web plumbing plus the lot forms' card rows;
//! - `commands/`: the maintenance commands (run by the backend);
//! - the pure rules: the `yugioh-domain` crate.

pub mod commands;
pub mod routes;
pub mod store;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
#[cfg(test)]
mod tests;
pub mod web;

use axum::Router;
pub use common::AppState;
use common::Routes;

/// The Yu-Gi-Oh! collection's routes (assembled by `common::app`).
pub fn routes() -> Routes {
    routes::all()
}

/// The server with the Yu-Gi-Oh! collection only (its tests).
pub fn app(state: AppState) -> Router {
    common::app(state, vec![routes()])
}
