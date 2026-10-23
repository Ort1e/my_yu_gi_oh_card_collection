//! `refresh-prices`: today's market prices from ygoprodeck into `card_price`
//! (`store::card_prices`), then Cardmarket's price guide
//! (`store::prices::cardmarket`), then the Pokémon cards' from TCGdex
//! (`store::pokemon::prices`). The server also runs it by itself every week
//! ([`spawn_weekly`]).

use std::time::Duration;

use chrono::NaiveDate;

use crate::store::db::today;
use crate::store::pokemon;
use crate::store::prices::cardmarket;
use crate::store::prices::history::latest_refresh;
use crate::store::prices::ygoprodeck::refresh_prices;
use crate::AppState;

pub async fn refresh(state: &AppState, dry_run: bool) -> anyhow::Result<()> {
    let report = refresh_prices(&state.pool, &state.http, today(), dry_run).await?;
    println!(
        "{} cards asked, {} prices written ({} without a Cardmarket price), \
         {} unknown to ygoprodeck",
        report.asked, report.written, report.without_price, report.missing
    );
    let report = cardmarket::refresh(&state.pool, &state.http, today(), dry_run).await?;
    println!("{}", report.summary());
    let report = pokemon::prices::refresh(&state.pool, &state.http, today(), dry_run).await?;
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
            match due(&state).await {
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
    match refresh_prices(&state.pool, &state.http, today(), false).await {
        Ok(report) => report.log(),
        Err(e) => tracing::warn!("background price refresh: {e}"),
    }
    match cardmarket::refresh(&state.pool, &state.http, today(), false).await {
        Ok(report) => tracing::info!("{}", report.summary()),
        Err(e) => tracing::warn!("background Cardmarket refresh: {e}"),
    }
    match pokemon::prices::refresh(&state.pool, &state.http, today(), false).await {
        Ok(report) => tracing::info!("{}", report.summary()),
        Err(e) => tracing::warn!("background Pokémon price refresh: {e}"),
    }
}

/// After `clean-cards` (5 minutes), which links new cards to their card data.
const START_DELAY: Duration = Duration::from_secs(10 * 60);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);
const REFRESH_EVERY_DAYS: i64 = 7;

async fn due(state: &AppState) -> sqlx::Result<bool> {
    Ok(is_due(latest_refresh(&state.pool).await?, today()))
}

fn is_due(latest: Option<NaiveDate>, today: NaiveDate) -> bool {
    latest.is_none_or(|latest| (today - latest).num_days() >= REFRESH_EVERY_DAYS)
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;
    use crate::store::prices::history::prices_by_date;
    use crate::tests::TestApp;

    #[test]
    fn refreshes_weekly() {
        let d = |day| NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        assert!(is_due(None, d(19)));
        assert!(!is_due(Some(d(13)), d(19)));
        assert!(is_due(Some(d(12)), d(19)));
    }

    #[tokio::test]
    async fn cards_priced_today_are_not_asked_again() {
        let t = TestApp::new().await;
        let date = today().to_string();
        sqlx::query(
            "INSERT INTO card_price (card_data_id, date, cardmarket_price) VALUES (1, ?1, '0.25'), (2, ?1, NULL)",
        )
        .bind(&date)
        .execute(&t.state.pool)
        .await
        .unwrap();

        // nothing left to ask: no request is made
        let report = refresh_prices(&t.state.pool, &t.state.http, today(), false)
            .await
            .unwrap();
        assert_eq!((report.asked, report.written), (0, 0));

        let by_date = prices_by_date(&t.state.pool).await.unwrap();
        assert_eq!(by_date.len(), 1);
        assert_eq!(by_date.values().next().unwrap().cards[&1], dec!(0.25));
        assert_eq!(latest_refresh(&t.state.pool).await.unwrap(), Some(today()));
    }
}
