//! Talking to the outside APIs politely: one [`Throttle`] per service.

use std::time::{Duration, Instant};

use tokio::sync::Mutex;

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
