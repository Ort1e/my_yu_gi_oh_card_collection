//! The live tournament notes posted to Discord (`common::discord`,
//! `DISCORD_WEBHOOK_URL`), for a tournament opted in on the live start page
//! (`tournament.post_to_discord`).
//!
//! One formatter, the live page's (`static/js/tournament_live.js`): its round
//! `PUT` carries the round's line (`share_line`), its end `PATCH` the day's
//! text (`share_text`). Each is one message ("slot": the round's number, `0`
//! the summary), remembered in `tournament_discord_post`: the same text again
//! (a queue resend) posts nothing, a changed one (the opponent's deck written
//! after the round) **edits** the message instead of posting another. Done
//! after the commit in a task, one at a time; a failure is only a `warn!`
//! (the notes are saved anyway).

use common::discord::Webhook;
use tokio::sync::Mutex;

use crate::store::db::{begin_write, Pool};
use crate::AppState;

/// The slot of the day's summary.
pub const SUMMARY: i64 = 0;

/// One post or edit at a time: two quick saves of a round must not both see
/// "not posted yet" and post twice.
static POSTING: Mutex<()> = Mutex::const_new(());

/// Shares `text` as slot `slot` of tournament `id`, in the background, when
/// a webhook is set and the tournament is opted in. Nothing for a blank
/// text, nothing under `cfg(test)`.
pub fn spawn_share(state: &AppState, id: i64, slot: i64, text: Option<String>) {
    // never Discord from a test
    if cfg!(test) {
        return;
    }
    let Some(webhook) = state.discord.clone() else {
        return;
    };
    let Some(text) = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) else {
        return;
    };
    let pool = state.pool.clone();
    let http = state.http.clone();
    tokio::spawn(async move {
        if let Err(e) = share(&pool, &http, &webhook, id, slot, &text).await {
            tracing::warn!(tournament_id = id, slot, "Discord post failed: {e}");
        }
    });
}

struct Head {
    name: String,
    post_to_discord: bool,
    placement: Option<i64>,
    players: Option<i64>,
}

async fn share(
    pool: &Pool,
    http: &reqwest::Client,
    webhook: &Webhook,
    id: i64,
    slot: i64,
    text: &str,
) -> Result<(), String> {
    let _turn = POSTING.lock().await;
    let db = |e: sqlx::Error| e.to_string();

    let head: Option<(String, bool, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT name, post_to_discord, placement, players FROM tournament WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(db)?;
    let Some((name, post_to_discord, placement, players)) = head else {
        return Ok(());
    };
    let head = Head {
        name,
        post_to_discord,
        placement,
        players,
    };
    if !head.post_to_discord {
        return Ok(());
    }

    let content = compose(&head, slot, text);
    let posted: Option<(String, String)> = sqlx::query_as(
        "SELECT content, message_id FROM tournament_discord_post
         WHERE tournament_id = ? AND slot = ?",
    )
    .bind(id)
    .bind(slot)
    .fetch_optional(pool)
    .await
    .map_err(db)?;

    let message_id = match posted {
        Some((before, _)) if before == content => return Ok(()),
        Some((_, message_id)) if webhook.edit(http, &message_id, &content).await? => message_id,
        // never posted, or deleted in the channel since
        _ => webhook.post(http, &content).await?,
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
    .bind(&content)
    .bind(&message_id)
    .execute(&mut *tx)
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    tracing::info!(tournament_id = id, slot, "posted to Discord");
    Ok(())
}

/// The message: the first round's line under the tournament's name, the
/// summary under the name and the placement.
fn compose(head: &Head, slot: i64, text: &str) -> String {
    match slot {
        SUMMARY => {
            let result = match (head.placement, head.players) {
                (Some(p), Some(n)) => format!(": {p} / {n}"),
                (Some(p), None) => format!(": {p}"),
                (None, Some(n)) => format!(" ({n} players)"),
                (None, None) => String::new(),
            };
            format!("🏁 **{}**{result}\n{text}", head.name)
        }
        1 => format!("**{}**\n{text}", head.name),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(placement: Option<i64>, players: Option<i64>) -> Head {
        Head {
            name: "Locals".into(),
            post_to_discord: true,
            placement,
            players,
        }
    }

    #[test]
    fn composes_the_messages() {
        let h = head(None, None);
        assert_eq!(compose(&h, 1, "R1: W 2-0"), "**Locals**\nR1: W 2-0");
        assert_eq!(compose(&h, 2, "R2: L 0-2"), "R2: L 0-2");
        assert_eq!(compose(&h, SUMMARY, "R1: W"), "🏁 **Locals**\nR1: W");
        assert_eq!(
            compose(&head(Some(3), Some(32)), SUMMARY, "R1: W"),
            "🏁 **Locals**: 3 / 32\nR1: W"
        );
        assert_eq!(
            compose(&head(None, Some(16)), SUMMARY, "x"),
            "🏁 **Locals** (16 players)\nx"
        );
    }
}
