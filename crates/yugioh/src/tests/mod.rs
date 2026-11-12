//! The Yu-Gi-Oh! handler tests, on a fresh migrated database (no network:
//! the card data they need is inserted first). One file per area.

mod advent;
mod budget;
mod built_decks;
mod cardmarket;
mod collection;
mod deck_api;
mod decks;
mod edit_forms;
mod formats;
mod lots;
mod rarity;
mod security;
mod snapshots;
mod tournaments;

use std::ops::Deref;

// what the test files use through `super::*`
use crate::testing::LOT;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::testing::read;
use tower::ServiceExt;

/// The server under test, with the Yu-Gi-Oh! fixtures
/// (`crate::testing::insert_card_data`).
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
        let t = common::testing::TestApp::new(crate::app, password).await;
        crate::testing::insert_card_data(&t).await;
        TestApp(t)
    }

    /// A deck with one empty version, a lot with 2 owned Dark Magicians
    /// (Kuriboh not owned): (version id, [dm1, dm2]).
    pub(crate) async fn deck_fixture(&self) -> (i64, Vec<i64>) {
        crate::testing::deck_fixture(self).await
    }

    /// Save version `v`'s Main Deck: `(card data, name, copies)`; the answer
    /// (asserted 200).
    pub(crate) async fn set_main(
        &self,
        v: i64,
        cards: &[(Option<i64>, &str, usize)],
    ) -> serde_json::Value {
        self.patch(
            v,
            serde_json::json!({ "main": crate::testing::tiles(cards) }),
        )
        .await
    }

    /// `PATCH /api/deck-versions/:v` with `body`; the answer (asserted 200).
    pub(crate) async fn patch(&self, v: i64, body: serde_json::Value) -> serde_json::Value {
        let (status, answer) = self
            .json("PATCH", &format!("/api/deck-versions/{v}"), body)
            .await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        answer
    }
}
