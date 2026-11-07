//! The deck builder's History tab: the deck's saved lists
//! (`store::snapshots`), and what changed from one to the zones on the
//! screen, saved or not (`yugioh_domain::deck_history::diff`).

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use yugioh_domain::deck_history::{diff, DeckZone};

use crate::store::cards::Zone;
use crate::store::snapshots::{self, Snapshot};
use crate::web::error::{AppResult, OrNotFound};
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

/// The zones on the screen (card ids), to compare with snapshot
/// `snapshot_id` of the same deck.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CompareInput {
    snapshot_id: i64,
    main: Vec<i64>,
    extra: Vec<i64>,
    side: Vec<i64>,
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
    let same_deck: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM deck_snapshot s JOIN deck_version v ON v.deck_id = s.deck_id
                        WHERE s.id = ? AND v.id = ?)",
    )
    .bind(input.snapshot_id)
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if !same_deck {
        return Err(crate::web::error::AppError::NotFound);
    }

    let saved = snapshots::cards(&mut conn, input.snapshot_id).await?;
    let screen = snapshots::cards_of_ids(
        &mut conn,
        [
            (DeckZone::Main, &input.main),
            (DeckZone::Extra, &input.extra),
            (DeckZone::Side, &input.side),
        ],
    )
    .await?;
    let changes = diff(&saved, &screen)
        .into_iter()
        .map(|c| ApiChange {
            zone: match c.zone {
                DeckZone::Main => Zone::Main,
                DeckZone::Extra => Zone::Extra,
                DeckZone::Side => Zone::Side,
            },
            name: c.name,
            before: c.before,
            after: c.after,
        })
        .collect();
    Ok(Json(changes))
}
