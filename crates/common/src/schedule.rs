//! The server's background tasks: started by the `backend` binary, which
//! gives each its delay after the start.

use std::future::Future;
use std::time::Duration;

use chrono::NaiveDate;

use crate::AppState;

/// How often a [`when_due`] task is asked whether it is due.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);

/// Run `task` `start` after the server starts, then every `interval`.
pub fn every<F, Fut>(state: AppState, start: Duration, interval: Duration, task: F)
where
    F: Fn(AppState) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        tokio::time::sleep(start).await;
        loop {
            task(state.clone()).await;
            tokio::time::sleep(interval).await;
        }
    });
}

/// Run `task` when `due` says so: asked `start` after the server starts,
/// then every few hours. The due check reads the data (a last refresh
/// date: [`is_due`]), so restarting the server doesn't run the task again.
pub fn when_due<D, DFut, F, Fut>(
    state: AppState,
    start: Duration,
    name: &'static str,
    due: D,
    task: F,
) where
    D: Fn(AppState) -> DFut + Send + 'static,
    DFut: Future<Output = sqlx::Result<bool>> + Send,
    F: Fn(AppState) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        tokio::time::sleep(start).await;
        loop {
            match due(state.clone()).await {
                Ok(true) => task(state.clone()).await,
                Ok(false) => {}
                Err(e) => tracing::warn!("{name}: {e}"),
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

/// Whether a task last run on `latest` (`None`: never) is due again on
/// `today`, `every_days` days later.
pub fn is_due(latest: Option<NaiveDate>, every_days: i64, today: NaiveDate) -> bool {
    latest.is_none_or(|latest| (today - latest).num_days() >= every_days)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refreshes_weekly() {
        let d = |day| NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        assert!(is_due(None, 7, d(19)));
        assert!(!is_due(Some(d(13)), 7, d(19)));
        assert!(is_due(Some(d(12)), 7, d(19)));
    }
}
