//! Set lookup for the "quick add from a set" panel of the lot forms
//! (`static/js/card_rows.js`). See docs/tour.md, "Talking to ygoprodeck".

use axum::extract::{Path, State};
use axum::Json;
use yugioh_domain::card_set::{self, SetCard};
use serde::Serialize;

use crate::store::card_data::fetch_set;
use crate::web::error::{bad_request, AppResult};
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
pub async fn set_cards(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> AppResult<Json<SetResponse>> {
    let code = code.trim().to_uppercase();
    let Some(set) = fetch_set(&state.pool, &state.http, &code).await? else {
        return bad_request(format!("Unknown set code {code}."));
    };

    Ok(Json(SetResponse {
        cards: card_set::set_cards(&code, &set.en, &set.fr),
        name: set.name,
        code,
    }))
}
