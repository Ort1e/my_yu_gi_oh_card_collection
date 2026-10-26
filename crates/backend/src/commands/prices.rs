//! `refresh-prices`: the Yu-Gi-Oh! prices (`yugioh::commands::prices`:
//! ygoprodeck, then Cardmarket's price guide), then the Pokémon cards' from
//! TCGdex (`pokemon::store::prices`). The server also runs it by itself
//! every week ([`spawn_weekly`]).

use std::time::Duration;

use common::db::today;
use common::AppState;
use yugioh::commands::prices as yugioh_prices;

pub async fn refresh(state: &AppState, dry_run: bool) -> anyhow::Result<()> {
    yugioh_prices::refresh(state, dry_run).await?;
    let report =
        pokemon::store::prices::refresh(&state.pool, &state.http, today(), dry_run).await?;
    println!("{}", report.summary());

    if dry_run {
        println!("dry run: nothing was saved");
    }
    Ok(())
}

/// The weekly refresh: checked a few minutes after the server starts, then
/// every few hours; it runs when the last refresh is a week old (or there is
/// none), so restarting the server doesn't refresh again.
pub fn spawn_weekly(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(START_DELAY).await;
        loop {
            match yugioh_prices::due(&state).await {
                Ok(true) => refresh_in_background(&state).await,
                Ok(false) => {}
                Err(e) => tracing::warn!("background price refresh: {e}"),
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

/// The three refreshes, each even when the one before failed (they are
/// independent).
async fn refresh_in_background(state: &AppState) {
    yugioh_prices::refresh_in_background(state).await;
    match pokemon::store::prices::refresh(&state.pool, &state.http, today(), false).await {
        Ok(report) => tracing::info!("{}", report.summary()),
        Err(e) => tracing::warn!("background Pokémon price refresh: {e}"),
    }
}

/// After `clean-cards` (5 minutes), which links new cards to their card data.
const START_DELAY: Duration = Duration::from_secs(10 * 60);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);
