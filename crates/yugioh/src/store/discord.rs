//! The `tournament_discord_post` side of the Discord posts
//! (`routes::tournaments::discord` writes the messages, `common::discord`
//! talks to the webhook).
//!
//! [`publish`] makes the channel match a tournament's messages, one per
//! "slot" (a round's number, [`SUMMARY`] the day's end): a slot never posted
//! is posted, a changed one **edited**, the same text again posts nothing,
//! and a slot gone (a round deleted on the edit form, the end emptied) has its
//! message deleted.

use std::collections::HashMap;

use common::discord::Webhook;

use crate::store::db::{begin_write, Pool};

/// The slot of the day's summary.
pub const SUMMARY: i64 = 0;

/// Posts, edits or deletes so that the channel shows `messages` (slot,
/// content; in the order they should appear) for tournament `id`.
pub async fn publish(
    pool: &Pool,
    http: &reqwest::Client,
    webhook: &Webhook,
    id: i64,
    messages: &[(i64, String)],
) -> Result<(), String> {
    let db = |e: sqlx::Error| e.to_string();
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT slot, content, message_id FROM tournament_discord_post WHERE tournament_id = ?",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    let mut posted: HashMap<i64, (String, String)> = rows
        .into_iter()
        .map(|(slot, content, message_id)| (slot, (content, message_id)))
        .collect();

    for (slot, content) in messages {
        let message_id = match posted.remove(slot) {
            Some((before, _)) if &before == content => continue,
            Some((_, message_id)) if webhook.edit(http, &message_id, content).await? => message_id,
            // never posted, or deleted in the channel since
            _ => webhook.post(http, content).await?,
        };
        let mut tx = begin_write(pool).await.map_err(db)?;
        sqlx::query(
            "INSERT INTO tournament_discord_post (tournament_id, slot, content, message_id)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (tournament_id, slot)
             DO UPDATE SET content = excluded.content, message_id = excluded.message_id",
        )
        .bind(id)
        .bind(slot)
        .bind(content)
        .bind(&message_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        tracing::info!(tournament_id = id, slot, "posted to Discord");
    }

    // what is left was posted for a slot that is gone
    for (slot, (_, message_id)) in posted {
        webhook.delete(http, &message_id).await?;
        let mut tx = begin_write(pool).await.map_err(db)?;
        sqlx::query("DELETE FROM tournament_discord_post WHERE tournament_id = ? AND slot = ?")
            .bind(id)
            .bind(slot)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        tracing::info!(tournament_id = id, slot, "Discord message deleted");
    }
    Ok(())
}
