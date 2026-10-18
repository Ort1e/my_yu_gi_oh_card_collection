//! The tournament a deck version was played at.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::store::db::begin_write;
use crate::web::error::{bad_request, non_blank, required, AppResult, OrNotFound};
use crate::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Tournament {
    name: String,
    date: String,
    location: Option<String>,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TournamentInput {
    name: String,
    date: String,
    location: Option<String>,
    notes: Option<String>,
}

/// Create or update the tournament of this version.
pub async fn save_tournament(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<TournamentInput>,
) -> AppResult<Json<Tournament>> {
    let name = required(&input.name, "The tournament name is required.")?.to_string();
    let Ok(date) = input.date.trim().parse::<chrono::NaiveDate>() else {
        return bad_request("The tournament date is not a valid date.");
    };
    let tournament = Tournament {
        name,
        date: date.to_string(),
        location: non_blank(input.location),
        notes: non_blank(input.notes),
    };

    let mut tx = begin_write(&state.pool).await?;
    let tournament_id: Option<i64> =
        sqlx::query_scalar::<_, Option<i64>>("SELECT tournament_id FROM deck_version WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .or_404()?;
    match tournament_id {
        Some(t) => {
            sqlx::query(
                "UPDATE tournament SET name = ?, date = ?, location = ?, notes = ? WHERE id = ?",
            )
            .bind(&tournament.name)
            .bind(&tournament.date)
            .bind(&tournament.location)
            .bind(&tournament.notes)
            .bind(t)
            .execute(&mut *tx)
            .await?;
        }
        None => {
            let t: i64 = sqlx::query(
                "INSERT INTO tournament (name, date, location, notes) VALUES (?, ?, ?, ?)",
            )
            .bind(&tournament.name)
            .bind(&tournament.date)
            .bind(&tournament.location)
            .bind(&tournament.notes)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            sqlx::query("UPDATE deck_version SET tournament_id = ? WHERE id = ?")
                .bind(t)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    tracing::info!(version_id = id, "tournament saved");
    Ok(Json(tournament))
}
