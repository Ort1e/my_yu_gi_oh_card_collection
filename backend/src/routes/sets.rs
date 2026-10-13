//! Set lookup for the "quick add from a set" panel of the lot forms
//! (`static/js/card_rows.js`). Spec: `docs/specs/set-lookup.md`.

use axum::extract::{Path, State};
use axum::Json;
use domain::card_set::SetCard;
use serde::Serialize;

use crate::error::AppResult;
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct SetResponse {
    /// The prefix, upper case: `"RA05"`.
    code: String,
    /// `"Rarity Collection 5"`.
    name: String,
    cards: Vec<SetCard>,
}

/// `GET /api/sets/:code`: the cards of a set. Unknown code → 400
/// `{"error": "Unknown set code RA99."}` (`bad_request`).
#[allow(unused_variables)] // TODO: remove once implemented
pub async fn set_cards(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> AppResult<Json<SetResponse>> {
    todo!("card_data::fetch_set, then domain::card_set::set_cards")
}
