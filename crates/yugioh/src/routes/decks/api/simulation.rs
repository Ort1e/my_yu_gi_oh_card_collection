//! Monte Carlo simulation of opening hands.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use std::collections::HashSet;

use sqlx::{QueryBuilder, SqliteConnection};
use yugioh_domain::monte_carlo::{self, Category, SimulationResult};

use super::categories::load_categories;
use yugioh_domain::deck_history::DeckZone;

use crate::store::db::{exists, push_ids};
use crate::store::lists;
use crate::web::error::{bad_request, AppError, AppResult};
use crate::AppState;
use ts_rs::TS;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SimulationInput {
    hand_size: usize,
    simulations: u32,
    /// The Main Deck to draw from (its tiles: card data ids, one per copy;
    /// `null` for a card known by its name only), saved or not; without it
    /// the saved Main Deck is used.
    #[serde(default)]
    #[ts(optional)]
    main: Option<Vec<Option<i64>>>,
}

/// A card known by its name only, in the draw: in no category.
const NAME_ONLY: i64 = -1;

/// What a simulation of version `id` needs: its Main Deck (a card data id
/// per copy), its categories and which cards are in them (also the
/// read-only page, `decks::version`).
pub(crate) async fn inputs(
    conn: &mut SqliteConnection,
    id: i64,
) -> AppResult<(Vec<i64>, Vec<Category>, Vec<(i64, i64)>)> {
    let main: Vec<i64> = lists::lines(&mut *conn, id)
        .await?
        .iter()
        .filter(|l| l.zone == DeckZone::Main)
        .flat_map(|l| (0..l.count).map(|_| l.card_data_id.unwrap_or(NAME_ONLY)))
        .collect();
    let categories = load_categories(&mut *conn, id).await?;
    let assignments: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT card_data_id, category_id FROM card_category_assignment
         WHERE category_id IN (SELECT id FROM card_category WHERE deck_version_id = ?)",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let categories = categories.into_iter().map(Category::from).collect();
    Ok((main, categories, assignments))
}

pub async fn monte_carlo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<SimulationInput>,
) -> AppResult<Json<SimulationResult>> {
    if !(1..=60).contains(&input.hand_size) {
        return bad_request("The hand size must be between 1 and 60.");
    }
    if !(1..=1_000_000).contains(&input.simulations) {
        return bad_request("The number of simulations must be between 1 and 1 000 000.");
    }
    let mut conn = state.pool.acquire().await?;
    exists(&mut conn, "deck_version", id).await?;
    let (saved_main, categories, assignments) = inputs(&mut conn, id).await?;
    let main = match input.main {
        Some(tiles) => {
            // the cards must exist: the screen's zones, maybe not saved
            let ids: HashSet<i64> = tiles.iter().flatten().copied().collect();
            if !ids.is_empty() {
                let mut qb = QueryBuilder::new("SELECT id FROM card_data WHERE id IN ");
                push_ids(&mut qb, ids.iter().copied());
                let known: HashSet<i64> = qb
                    .build_query_scalar()
                    .fetch_all(&mut *conn)
                    .await?
                    .into_iter()
                    .collect();
                if let Some(unknown) = ids.iter().find(|i| !known.contains(i)) {
                    return bad_request(format!("Card {unknown} doesn't exist any more."));
                }
            }
            // a card that left the Main Deck counts no more in its
            // categories (`simulate` counts the deck's copies)
            tiles.into_iter().map(|t| t.unwrap_or(NAME_ONLY)).collect()
        }
        None => saved_main,
    };
    drop(conn);

    let result = tokio::task::spawn_blocking(move || {
        monte_carlo::simulate(
            &main,
            &categories,
            &assignments,
            input.hand_size,
            input.simulations,
            &mut rand::thread_rng(),
        )
    })
    .await
    .map_err(|e| AppError::Internal(format!("simulation: {e}")))?;
    Ok(Json(result))
}
