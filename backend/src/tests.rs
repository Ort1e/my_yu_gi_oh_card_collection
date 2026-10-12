//! Handler tests against a fresh, migrated SQLite database in a temporary
//! directory. No network: card data used by the tests is inserted up front.

use std::path::PathBuf;
use std::str::FromStr;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower::ServiceExt;

use crate::{app, AppState};

struct TestApp {
    state: AppState,
    dir: PathBuf,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl TestApp {
    async fn new() -> TestApp {
        let dir = std::env::temp_dir().join(format!("ygo-test-{:016x}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", dir.join("app.db").display()))
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await.unwrap();
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

        TestApp {
            state: AppState {
                pool,
                http: reqwest::Client::new(),
                media_dir: dir.join("media"),
                uploads_dir: dir.join("uploads"),
                cardmarket_account: "me".into(),
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

    async fn scalar(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.state.pool).await.unwrap()
    }

    async fn text(&self, sql: &str) -> Option<String> {
        sqlx::query_scalar(sql).fetch_one(&self.state.pool).await.unwrap()
    }
}

async fn read(response: axum::response::Response) -> (StatusCode, String) {
    let status = response.status();
    if let Some(location) = response.headers().get(header::LOCATION) {
        return (status, location.to_str().unwrap().to_string());
    }
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
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

#[tokio::test]
async fn pages_render() {
    let t = TestApp::new().await;
    for uri in ["/", "/budget", "/cards", "/lots", "/sellers", "/decks", "/lots/add", "/decks/add", "/budget/manage"] {
        let (status, body) = t.get(uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    assert_eq!(t.get("/lots/12345").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_lot_with_new_seller_and_cards() {
    let t = TestApp::new().await;
    let (status, location) = t.post("/lots/add", LOT).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    let lot_id = t.scalar("SELECT max(id) FROM lot").await;
    assert_eq!(location, format!("/lots/{lot_id}"));

    assert_eq!(t.text("SELECT price FROM lot").await.as_deref(), Some("10.50"));
    assert_eq!(t.scalar("SELECT is_person FROM seller WHERE name = 'New Shop'").await, 0);
    assert_eq!(t.scalar("SELECT source_id FROM seller WHERE name = 'New Shop'").await, 1);
    assert_eq!(t.scalar("SELECT count(*) FROM card").await, 3);
    assert_eq!(t.scalar("SELECT count(*) FROM unite WHERE price = '2.25'").await, 2);
    assert_eq!(t.scalar("SELECT count(*) FROM card WHERE en_name = 'dark magician'").await, 2);
    assert_eq!(t.scalar("SELECT is_proxy FROM card WHERE name = 'Kuriboh'").await, 1);

    // card data is linked in the background
    for _ in 0..50 {
        if t.scalar("SELECT count(*) FROM card WHERE card_data_id IS NOT NULL").await == 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(t.scalar("SELECT count(*) FROM card WHERE card_data_id IS NOT NULL").await, 3);

    let (status, page) = t.get(&location).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Magicien Sombre") && page.contains("4.50 €"), "{page}");
}

#[tokio::test]
async fn invalid_lot_is_rerendered_with_errors() {
    let t = TestApp::new().await;
    let (status, page) = t
        .post(
            "/lots/add",
            &[
                ("lot_type", "purchase"),
                ("price", "abc"),
                ("buy_date", ""),
                ("cards-0-name", "Kuriboh"),
                ("cards-0-price", "1.00"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("is not a valid amount"), "{page}");
    assert!(page.contains("Buy date is required."));
    assert!(page.contains("Select a seller or enter a new seller name."));
    assert!(page.contains(r#"value="Kuriboh""#), "typed values are kept");
    assert_eq!(t.scalar("SELECT count(*) FROM lot").await, 0);
}

#[tokio::test]
async fn pending_shipment_file_is_attached() {
    let t = TestApp::new().await;
    std::fs::create_dir_all(&t.state.uploads_dir).unwrap();
    let pending = t.state.uploads_dir.join("pending-0123456789abcdef.pdf");
    std::fs::write(&pending, b"%PDF-fake").unwrap();

    let mut form = LOT.to_vec();
    form.push(("pending", "0123456789abcdef"));
    let (status, _) = t.post("/lots/add", &form).await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let lot_id = t.scalar("SELECT max(id) FROM lot").await;
    let file = t.text("SELECT shipment_file FROM lot").await.unwrap();
    assert_eq!(file, format!("shipment_files/shipment_{lot_id}.pdf"));
    assert!(t.state.media_dir.join(&file).exists());
    assert!(!pending.exists());

    // an unknown token is rejected, nothing is created
    let mut form = LOT.to_vec();
    form.push(("pending", "ffffffffffffffff"));
    let (status, page) = t.post("/lots/add", &form).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("expired"));
    assert_eq!(t.scalar("SELECT count(*) FROM lot").await, 1);
}

#[tokio::test]
async fn upload_rejects_a_non_pdf() {
    let t = TestApp::new().await;
    let boundary = "XBOUNDARYX";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"shipment_file\"; filename=\"a.pdf\"\r\n\
         Content-Type: application/pdf\r\n\r\nnot a pdf\r\n--{boundary}--\r\n"
    );
    let response = t
        .router()
        .oneshot(
            Request::post("/lots/add/shipment")
                .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, page) = read(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Failed to process the shipment PDF"), "{page}");
    assert!(!t.state.uploads_dir.exists(), "nothing is saved");
}

#[tokio::test]
async fn add_cards_to_existing_lot() {
    let t = TestApp::new().await;
    t.post("/lots/add", LOT).await;
    let lot_id = t.scalar("SELECT max(id) FROM lot").await;

    let uri = format!("/lots/{lot_id}/cards");
    let (status, _) = t
        .post(&uri, &[("cards-0-name", "Kuriboh"), ("cards-0-quantity", "2")])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM unite").await, 5);

    let (status, page) = t.post(&uri, &[("cards-0-name", "")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("name is required"));

    assert_eq!(t.post("/lots/999/cards", &[("cards-0-name", "X")]).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_deck_with_first_version() {
    let t = TestApp::new().await;
    let (status, location) = t
        .post("/decks/add", &[("name", "Exodia"), ("description", ""), ("version_name", "v1")])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let deck_id = t.scalar("SELECT id FROM deck").await;
    assert_eq!(location, format!("/decks/{deck_id}"));
    assert_eq!(t.scalar("SELECT count(*) FROM deck_version WHERE version_name = 'v1'").await, 1);
    assert_eq!(t.text("SELECT description FROM deck").await, None);

    let (status, page) = t.post("/decks/add", &[("name", "X"), ("version_name", "")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Version name is required."));
}

#[tokio::test]
async fn manage_budgets_and_apports() {
    let t = TestApp::new().await;
    assert_eq!(
        t.post("/budget/manage/budget", &[("month", "2025-03"), ("amount", "25")]).await.0,
        StatusCode::SEE_OTHER
    );
    t.post("/budget/manage/budget", &[("month", "2025-03"), ("amount", "30,00")]).await;
    assert_eq!(t.scalar("SELECT count(*) FROM monthly_budget").await, 1);
    assert_eq!(t.text("SELECT amount FROM monthly_budget").await.as_deref(), Some("30.00"));
    assert_eq!(t.text("SELECT month FROM monthly_budget").await.as_deref(), Some("2025-03-01"));

    let budget_id = t.scalar("SELECT id FROM monthly_budget").await.to_string();
    let (status, _) = t
        .post(
            "/budget/manage/apports",
            &[("budget_id", &budget_id), ("date", "2025-03-15"), ("amount", "10"), ("description", "gift")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let apport_id = t.scalar("SELECT id FROM apport").await;

    let (_, page) = t.get("/budget").await;
    assert!(page.contains("40.00"), "budget includes the apport");

    t.post(&format!("/budget/manage/apports/{apport_id}/delete"), &[]).await;
    assert_eq!(t.scalar("SELECT count(*) FROM apport").await, 0);

    let (status, page) = t.post("/budget/manage/budget", &[("month", "march"), ("amount", "1")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("is not a valid month"));
}
