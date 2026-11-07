//! Monte Carlo simulation of opening hands.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use std::collections::HashSet;

use sqlx::{QueryBuilder, SqliteConnection};
use yugioh_domain::monte_carlo::{self, Category, SimulationResult};

use super::categories::load_categories;
use crate::store::cards::Zone;
use crate::store::db::{exists, push_ids};
use crate::web::error::{bad_request, AppError, AppResult};
use crate::AppState;
use ts_rs::TS;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SimulationInput {
    hand_size: usize,
    simulations: u32,
    /// The Main Deck to draw from (card ids), saved or not; without it the
    /// saved Main Deck is used.
    #[serde(default)]
    #[ts(optional)]
    main: Option<Vec<i64>>,
}

/// What a simulation of version `id` needs: its Main Deck, its categories
/// and which cards are in them (also the read-only page, `decks::version`).
pub(crate) async fn inputs(
    conn: &mut SqliteConnection,
    id: i64,
) -> AppResult<(Vec<i64>, Vec<Category>, Vec<(i64, i64)>)> {
    let main = Zone::Main.card_ids(&mut *conn, id).await?;
    let categories = load_categories(&mut *conn, id).await?;
    let assignments: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT card_id, category_id FROM card_category_assignment
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
    let (saved_main, categories, mut assignments) = inputs(&mut conn, id).await?;
    let main = match input.main {
        Some(ids) => {
            // the cards must exist: ids of the screen's zones, maybe not saved
            if !ids.is_empty() {
                let mut qb = QueryBuilder::new("SELECT id FROM card WHERE id IN ");
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
            // the cards that left the Main Deck on the screen no longer count
            // in their categories ("cards in the deck")
            let kept: HashSet<i64> = ids.iter().copied().collect();
            let removed: HashSet<i64> = saved_main
                .iter()
                .copied()
                .filter(|card| !kept.contains(card))
                .collect();
            assignments.retain(|(card, _)| !removed.contains(card));
            ids
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
