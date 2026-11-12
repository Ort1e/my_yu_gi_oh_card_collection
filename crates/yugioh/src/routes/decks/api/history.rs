//! The deck builder's History tab: the deck's saved lists
//! (`store::snapshots`), and what changed from one to the zones on the
//! screen, saved or not (`yugioh_domain::deck_history::diff`). Saved lists
//! are named here only (the snapshot page shows the name).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;
use ts_rs::TS;
use yugioh_domain::deck_history::{diff, DeckZone};

use super::{input_list, CardInput};
use crate::store::cards::Zone;
use crate::store::db::begin_write;
use crate::store::snapshots::{self, Snapshot};
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::AppState;

/// `GET /api/deck-versions/:id/history`: the snapshots of its deck, newest
/// first.
pub async fn history(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<Snapshot>>> {
    let mut conn = state.pool.acquire().await?;
    let deck_id: Option<i64> =
        sqlx::query_scalar::<_, Option<i64>>("SELECT deck_id FROM deck_version WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .or_404()?;
    let Some(deck_id) = deck_id else {
        return Ok(Json(Vec::new()));
    };
    Ok(Json(snapshots::for_deck(&mut conn, deck_id, false).await?))
}

/// The zones on the screen (their tiles), to compare with snapshot
/// `snapshot_id` of the same deck.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CompareInput {
    snapshot_id: i64,
    main: Vec<CardInput>,
    extra: Vec<CardInput>,
    side: Vec<CardInput>,
}

/// A card whose count changed from the snapshot to the screen.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ApiChange {
    zone: Zone,
    name: String,
    before: u32,
    after: u32,
}

/// `POST /api/deck-versions/:id/compare`.
pub async fn compare(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<CompareInput>,
) -> AppResult<Json<Vec<ApiChange>>> {
    let mut conn = state.pool.acquire().await?;
    same_deck(&mut conn, id, input.snapshot_id).await?;

    let saved = snapshots::cards(&mut conn, input.snapshot_id).await?;
    let screen = input_list(
        &mut conn,
        [
            (DeckZone::Main, input.main.as_slice()),
            (DeckZone::Extra, input.extra.as_slice()),
            (DeckZone::Side, input.side.as_slice()),
        ],
    )
    .await?;
    let changes = diff(&saved, &screen)
        .into_iter()
        .map(|c| ApiChange {
            zone: Zone::of(c.zone),
            name: c.name,
            before: c.before,
            after: c.after,
        })
        .collect();
    Ok(Json(changes))
}

/// Snapshot `snapshot_id` belongs to the deck of version `version_id`, else
/// 404.
async fn same_deck(
    conn: &mut SqliteConnection,
    version_id: i64,
    snapshot_id: i64,
) -> AppResult<()> {
    let same: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM deck_snapshot s JOIN deck_version v ON v.deck_id = s.deck_id
                        WHERE s.id = ? AND v.id = ?)",
    )
    .bind(snapshot_id)
    .bind(version_id)
    .fetch_one(&mut *conn)
    .await?;
    if same {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// A saved list's new name: `null` or blank = back to an automatic one.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SnapshotNameInput {
    name: Option<String>,
}

/// `POST /api/deck-versions/:id/history/:snapshot_id/name`: name (or
/// unname) one of the deck's saved lists. ("Name this save", for the zones
/// as saved now, is the version's `PATCH` `snapshot_name`.)
pub async fn name_snapshot(
    State(state): State<AppState>,
    Path((id, snapshot_id)): Path<(i64, i64)>,
    Json(input): Json<SnapshotNameInput>,
) -> AppResult<StatusCode> {
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    let mut tx = begin_write(&state.pool).await?;
    same_deck(&mut tx, id, snapshot_id).await?;
    snapshots::set_name(&mut tx, snapshot_id, name).await?;
    tx.commit().await?;
    tracing::info!(snapshot_id, named = name.is_some(), "deck snapshot named");
    Ok(StatusCode::NO_CONTENT)
}
