//! ygoprodeck's archetype names into `ygo_archetype` (0035), with the weekly
//! prices: the names offered for the opponents' decks
//! (`routes::tournaments::label_names`). After each refresh the labels that
//! match an archetype take its spelling (`store::tournaments::relabel`).

use serde_json::Value;
use yugioh_domain::archetype;

use super::YGOPRODECK;
use crate::store::db::{begin_write, Pool};
use crate::store::tournaments;
use crate::web::error::{AppError, AppResult};

const ARCHETYPES_URL: &str = "https://db.ygoprodeck.com/api/v7/archetypes.php";

/// What a refresh did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ArchetypeReport {
    pub archetypes: usize,
    /// Labels renamed to an archetype's spelling (merged ones included).
    pub relabelled: usize,
}

/// Download the archetype names into `ygo_archetype`, then give the labels
/// their spelling. Network first, then one write transaction.
pub async fn refresh_archetypes(pool: &Pool, http: &reqwest::Client) -> AppResult<ArchetypeReport> {
    let internal = |e: reqwest::Error| AppError::Internal(format!("ygoprodeck archetypes: {e}"));
    YGOPRODECK.wait().await;
    let answer: Value = http
        .get(ARCHETYPES_URL)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(internal)?
        .json()
        .await
        .map_err(internal)?;
    let names = archetype::names(&answer);
    if names.is_empty() {
        return Err(AppError::Internal(
            "ygoprodeck archetypes: an empty answer".into(),
        ));
    }
    store_archetypes(pool, &names).await
}

/// Replace `ygo_archetype` by these names and relabel (the part of
/// [`refresh_archetypes`] after the download; tests call it).
pub async fn store_archetypes(pool: &Pool, names: &[String]) -> AppResult<ArchetypeReport> {
    let mut tx = begin_write(pool).await?;
    sqlx::query("DELETE FROM ygo_archetype")
        .execute(&mut *tx)
        .await?;
    for name in names {
        sqlx::query("INSERT OR IGNORE INTO ygo_archetype (name, key) VALUES (?, ?)")
            .bind(name)
            .bind(archetype::key(name))
            .execute(&mut *tx)
            .await?;
    }
    let relabelled = tournaments::relabel(&mut tx).await?;
    tx.commit().await?;

    let report = ArchetypeReport {
        archetypes: names.len(),
        relabelled,
    };
    tracing::info!(
        archetypes = report.archetypes,
        relabelled = report.relabelled,
        "archetypes refreshed"
    );
    Ok(report)
}

/// Get the archetypes once when there are none yet (a new install, before
/// the first weekly refresh); a failure is logged: the lists then only have
/// our labels. Does nothing in the tests (never call ygoprodeck there: they
/// insert `ygo_archetype` rows).
pub async fn ensure_archetypes(pool: &Pool, http: &reqwest::Client) -> AppResult<()> {
    if cfg!(test) {
        return Ok(());
    }
    let known: i64 = sqlx::query_scalar("SELECT count(*) FROM ygo_archetype")
        .fetch_one(pool)
        .await?;
    if known == 0 {
        if let Err(e) = refresh_archetypes(pool, http).await {
            tracing::warn!("{e}");
        }
    }
    Ok(())
}
