//! The Yu-Gi-Oh! part of `refresh-prices`: today's market prices from
//! ygoprodeck into `card_price` (`store::prices::ygoprodeck`), then
//! Cardmarket's price guide (`store::prices::cardmarket`). The backend runs
//! it with the Pokémon prices, by hand and every week ([`due`]).

use chrono::NaiveDate;

use crate::store::db::today;
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
    Ok(())
}

/// The two refreshes in the background (logged), each even when the one
/// before failed (they are independent).
pub async fn refresh_in_background(state: &AppState) {
    match refresh_prices(&state.pool, &state.http, today(), false).await {
        Ok(report) => report.log(),
        Err(e) => tracing::warn!("background price refresh: {e}"),
    }
    match cardmarket::refresh(&state.pool, &state.http, today(), false).await {
        Ok(report) => tracing::info!("{}", report.summary()),
        Err(e) => tracing::warn!("background Cardmarket refresh: {e}"),
    }
}

const REFRESH_EVERY_DAYS: i64 = 7;

/// Whether the last refresh is a week old (or there is none).
pub async fn due(state: &AppState) -> sqlx::Result<bool> {
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
