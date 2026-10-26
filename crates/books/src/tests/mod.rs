//! The book pages' handler tests, on a fresh migrated database (no network:
//! the ISBN lookups come from `book_data` rows inserted first).

mod book_advent;
mod books;

use std::ops::Deref;

// what the test files use through `super::*`
use axum::http::StatusCode;

/// The book collection's server under test.
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
}
