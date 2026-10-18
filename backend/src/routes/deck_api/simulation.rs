//! Monte Carlo simulation of opening hands.

use axum::extract::{Path, State};
use axum::Json;
use domain::monte_carlo::{self, Category, SimulationResult};
use serde::Deserialize;

use super::categories::load_categories;
use crate::cards::Zone;
use crate::db::exists;
use crate::error::{bad_request, AppError, AppResult};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct SimulationInput {
    hand_size: usize,
    simulations: u32,
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
    let main = Zone::Main.card_ids(&mut conn, id).await?;

    let categories = load_categories(&mut conn, id).await?;
    let assignments: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT card_id, category_id FROM card_category_assignment
         WHERE category_id IN (SELECT id FROM card_category WHERE deck_version_id = ?)",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);

    let categories: Vec<Category> = categories.into_iter().map(Category::from).collect();

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
