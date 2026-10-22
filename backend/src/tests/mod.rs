//! Handler tests against a fresh, migrated SQLite database in a temporary
//! directory. No network: card data used by the tests is inserted up front.
//!
//! `TestApp` and the shared fixtures are here, the tests in one file per area.

mod advent;
mod auth;
mod book_advent;
mod books;
mod budget;
mod built_decks;
mod cardmarket;
mod collection;
mod deck_api;
mod decks;
mod edit_forms;
mod lots;
mod pages;
mod rarity;
mod tournaments;

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;

use crate::{app, AppState};

pub(crate) struct TestApp {
    pub(crate) state: AppState,
    dir: PathBuf,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl TestApp {
    pub(crate) async fn new() -> TestApp {
        TestApp::with_password(None).await
    }

    /// `password`: enable the login with this password.
    pub(crate) async fn with_password(password: Option<&str>) -> TestApp {
        // show handler errors in failing tests' output
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
        let dir = std::env::temp_dir().join(format!("ygo-test-{:016x}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let options =
            crate::store::db::connect_options(&format!("sqlite:{}", dir.join("app.db").display()));
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!("../migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO seller_source (id, name) VALUES (1, 'Cardmarket');
             INSERT INTO card_data (id, en_name, ygopro_id, card_type, json_data)
             VALUES (1, 'dark magician', 46986414, 'Normal Monster', '{}'),
                    (2, 'kuriboh', 40640057, 'Effect Monster', '{}');",
        )
        .execute(&pool)
        .await
        .unwrap();

        let auth = crate::web::auth::Auth::load(&pool, password.map(str::to_string))
            .await
            .unwrap();
        TestApp {
            state: AppState {
                pool,
                http: reqwest::Client::new(),
                media_dir: dir.join("media"),
                uploads_dir: dir.join("uploads"),
                cardmarket_account: "me".into(),
                auth,
            },
            dir,
        }
    }

    fn router(&self) -> Router {
        app(self.state.clone())
    }

    async fn get(&self, uri: &str) -> (StatusCode, String) {
        let response = self
            .router()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        read(response).await
    }

    /// POST an urlencoded form; returns (status, Location header or body).
    async fn post(&self, uri: &str, form: &[(&str, &str)]) -> (StatusCode, String) {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form)
            .finish();
        let response = self
            .router()
            .oneshot(
                Request::post(uri)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        read(response).await
    }

    pub(crate) async fn scalar(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }

    pub(crate) async fn text(&self, sql: &str) -> Option<String> {
        sqlx::query_scalar(sql)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }
}

async fn read(response: axum::response::Response) -> (StatusCode, String) {
    let status = response.status();
    if let Some(location) = response.headers().get(header::LOCATION) {
        return (status, location.to_str().unwrap().to_string());
    }
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

const LOT: &[(&str, &str)] = &[
    ("lot_type", "purchase"),
    ("price", "10,50"),
    ("buy_date", "2025-06-12"),
    ("received_date", ""),
    ("no_card_price", "0.00"),
    ("seller_select", ""),
    ("seller_name", "New Shop"),
    ("seller_source", "1"),
    ("cards-0-name", "Magicien Sombre"),
    ("cards-0-en_name", "Dark Magician (V.1 - Ultra Rare)"),
    ("cards-0-code", "LOB-FR005"),
    ("cards-0-status", "NM"),
    ("cards-0-price", "2.25"),
    ("cards-0-quantity", "2"),
    ("cards-3-name", "Kuriboh"),
    ("cards-3-is_proxy", "on"),
];

impl TestApp {
    async fn json(
        &self,
        method: &str,
        uri: &str,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let response = self
            .router()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, value)
    }

    /// A deck with one version, and a lot with 2 owned Dark Magicians and a
    /// Kuriboh proxy. Returns (version id, [dm1, dm2, kuriboh]).
    async fn deck_fixture(&self) -> (i64, Vec<i64>) {
        self.post("/decks/add", &[("name", "Test"), ("version_name", "v1")])
            .await;
        // the lot of `LOT`, inserted directly: saving it through the form
        // would start the background task (link, translate, replace proxies),
        // which would race with the test
        sqlx::query(
            "INSERT INTO seller (name, is_person, source_id) VALUES ('New Shop', 0, 1);
             INSERT INTO lot (lot_type, price, buy_date, seller_id, no_card_price)
                 VALUES ('purchase', '10.50', '2025-06-12', 1, '0.00');
             INSERT INTO card (name, en_name, code, last_known_status, card_data_id, is_proxy) VALUES
                 ('Magicien Sombre', 'dark magician', 'LOB-FR005', 'NM', 1, 0),
                 ('Magicien Sombre', 'dark magician', 'LOB-FR005', 'NM', 1, 0),
                 ('Kuriboh', 'kuriboh', NULL, NULL, 2, 1);
             INSERT INTO unite (price, lot_id, card_id)
                 SELECT CASE WHEN is_proxy THEN NULL ELSE '2.25' END, 1, id FROM card ORDER BY id;",
        )
        .execute(&self.state.pool)
        .await
        .unwrap();
        let version = self.scalar("SELECT id FROM deck_version").await;
        let cards = sqlx::query_scalar("SELECT id FROM card ORDER BY id")
            .fetch_all(&self.state.pool)
            .await
            .unwrap();
        (version, cards)
    }
}
