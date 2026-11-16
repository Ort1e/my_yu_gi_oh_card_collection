//! The Pokémon pages' handler tests, on a fresh migrated database (no
//! network: TCGdex's cards, prices and sets are inserted first).

mod boosters;
mod pokemon;
mod wishlists;

use std::ops::Deref;

// what the test files use through `super::*`
use axum::http::StatusCode;

/// The Pokémon collection's server under test.
pub(crate) struct TestApp(common::testing::TestApp);

impl Deref for TestApp {
    type Target = common::testing::TestApp;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TestApp {
    pub(crate) async fn new() -> TestApp {
        TestApp::with_password(None).await
    }

    /// `password`: enable the login with this password.
    pub(crate) async fn with_password(password: Option<&str>) -> TestApp {
        TestApp(common::testing::TestApp::new(crate::app, password).await)
    }

    /// With these users and settings.
    pub(crate) async fn with_config(config: common::web::auth::AuthConfig) -> TestApp {
        TestApp(common::testing::TestApp::with_config(crate::app, config).await)
    }
}
