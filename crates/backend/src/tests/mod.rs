//! The tests of the whole app (every collection's routes, the login):
//! the pages render, the login guards the writes.

mod auth;
mod pages;
mod security;

use std::ops::Deref;

// what the test files use through `super::*`
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

/// The server under test, with the Yu-Gi-Oh! fixtures
/// (`yugioh::testing::insert_card_data`).
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
        TestApp::with_config(common::web::auth::AuthConfig::password(password)).await
    }

    /// With these users and settings (private mode, `Secure` cookies…).
    pub(crate) async fn with_config(config: common::web::auth::AuthConfig) -> TestApp {
        let t = common::testing::TestApp::with_config(crate::app, config).await;
        yugioh::testing::insert_card_data(&t).await;
        TestApp(t)
    }

    /// A deck with one version, a lot with 2 owned Dark Magicians and a
    /// Kuriboh proxy: (version id, [dm1, dm2, kuriboh]).
    pub(crate) async fn deck_fixture(&self) -> (i64, Vec<i64>) {
        yugioh::testing::deck_fixture(self).await
    }
}
