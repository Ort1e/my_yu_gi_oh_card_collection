mod db;
mod error;
mod filters;
mod models;
mod pagination;
mod routes;

use std::path::PathBuf;

use axum::Router;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// Shared state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: db::Pool,
    pub http: reqwest::Client,
    /// Django's MEDIA_ROOT: card images and shipment files.
    pub media_dir: PathBuf,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let media_dir =
        PathBuf::from(std::env::var("MEDIA_DIR").unwrap_or_else(|_| "data/media".to_string()));
    let state = AppState {
        pool: db::init_pool().await,
        http: reqwest::Client::new(),
        media_dir: media_dir.clone(),
    };

    let app = Router::new()
        .merge(routes::router())
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/media", ServeDir::new(media_dir))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    // Local only by default: there is no login yet and /media exposes the
    // shipment PDFs (personal data). Set BIND_ADDR=0.0.0.0:3000 to serve the LAN.
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
