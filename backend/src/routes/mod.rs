mod budget;
mod cards;
mod dashboard;
mod decks;
mod images;
mod lots;
mod sellers;

use axum::routing::get;
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
