//! The tournament a deck version was played at, as the deck builder shows
//! it: a summary with links to `/tournaments/:id`. Tournaments are edited on
//! their own pages (`routes::tournaments`), no longer here.

use serde::Serialize;
use sqlx::SqliteConnection;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Tournament {
    id: i64,
    name: String,
    date: String,
    location: Option<String>,
    placement: Option<i64>,
    players: Option<i64>,
}

/// The tournament with this id, if any.
pub async fn load(conn: &mut SqliteConnection, id: i64) -> sqlx::Result<Option<Tournament>> {
    sqlx::query_as(
        "SELECT id, name, date, location, placement, players FROM tournament WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
}
