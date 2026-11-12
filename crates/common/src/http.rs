//! Talking to the outside APIs politely: one [`Throttle`] per service.

use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::web::error::{AppError, AppResult};

/// Keeps a service's requests at least `interval` apart, whichever task
/// sends them: `static API: Throttle = Throttle::new(…)`, then
/// `API.wait().await` before each request.
pub struct Throttle {
    interval: Duration,
    last: Mutex<Option<Instant>>,
}

impl Throttle {
    pub const fn new(interval: Duration) -> Throttle {
        Throttle {
            interval,
            last: Mutex::const_new(None),
        }
    }

    /// Wait for the service's turn.
    pub async fn wait(&self) {
        let mut last = self.last.lock().await;
        if let Some(at) = *last {
            tokio::time::sleep_until((at + self.interval).into()).await;
        }
        *last = Some(Instant::now());
    }
}

/// A whole file into memory (Cardmarket's catalogue files): an error names
/// the address, which must hold no secret.
pub async fn download(http: &reqwest::Client, url: &str) -> AppResult<Vec<u8>> {
    let internal = |e: reqwest::Error| AppError::Internal(format!("{url}: {}", e.without_url()));
    let bytes = http
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(internal)?
        .bytes()
        .await
        .map_err(internal)?;
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn requests_are_spaced() {
        static API: Throttle = Throttle::new(Duration::from_millis(30));
        let start = Instant::now();
        API.wait().await;
        API.wait().await;
        API.wait().await;
        assert!(start.elapsed() >= Duration::from_millis(60));
    }
}
