//! Handler tests against a fresh, migrated SQLite database in a temporary
//! directory (feature `test-support`). No network: a test inserts the card
//! data it needs up front.
//!
//! A collection builds its `TestApp` with its own routes (`build`): its
//! tests go through the real app, login layer included.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;

use crate::AppState;

pub struct TestApp {
    pub state: AppState,
    dir: PathBuf,
    build: fn(AppState) -> Router,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl TestApp {
    /// `build`: the app under test from its state; `password`: enable the
    /// login with this password.
    pub async fn new(build: fn(AppState) -> Router, password: Option<&str>) -> TestApp {
        // show handler errors in failing tests' output
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
        let dir = std::env::temp_dir().join(format!("ygo-test-{:016x}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let options = crate::db::connect_options(&format!("sqlite:{}", dir.join("app.db").display()));
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

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
            build,
        }
    }

    pub fn router(&self) -> Router {
        (self.build)(self.state.clone())
    }

    pub async fn get(&self, uri: &str) -> (StatusCode, String) {
        let response = self
            .router()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        read(response).await
    }

    /// POST an urlencoded form; returns (status, Location header or body).
    pub async fn post(&self, uri: &str, form: &[(&str, &str)]) -> (StatusCode, String) {
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

    /// A JSON request; returns (status, the JSON answer or null).
    pub async fn json(
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

    pub async fn scalar(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }

    pub async fn text(&self, sql: &str) -> Option<String> {
        sqlx::query_scalar(sql)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }

    /// Run SQL (fixtures), panicking on error.
    pub async fn exec(&self, sql: &str) {
        sqlx::query(sql).execute(&self.state.pool).await.unwrap();
    }
}

/// A response's status, and its Location header or its body.
pub async fn read(response: axum::response::Response) -> (StatusCode, String) {
    let status = response.status();
    if let Some(location) = response.headers().get(header::LOCATION) {
        return (status, location.to_str().unwrap().to_string());
    }
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}
