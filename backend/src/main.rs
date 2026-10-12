mod card_data;
mod cards;
mod commands;
mod db;
mod error;
mod filters;
mod forms;
mod models;
mod pagination;
mod routes;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use axum::Router;
use commands::Command;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// Shared state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: db::Pool,
    pub http: reqwest::Client,
    /// Django's MEDIA_ROOT: card images and shipment files.
    pub media_dir: PathBuf,
    /// Shipment PDFs uploaded but not yet attached to a lot.
    pub uploads_dir: PathBuf,
    /// Our Cardmarket username, to tell purchases from sales in shipments.
    pub cardmarket_account: String,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match Command::parse(&args) {
        Ok(command) => command,
        Err(e) => {
            eprintln!("{e}\n\n{}", commands::USAGE);
            std::process::exit(2);
        }
    };

    let env =
        |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_string());
    let state = AppState {
        pool: db::init_pool().await,
        http: reqwest::Client::new(),
        media_dir: PathBuf::from(env("MEDIA_DIR", "data/media")),
        uploads_dir: PathBuf::from(env("UPLOADS_DIR", "uploads")),
        cardmarket_account: env("CARDMARKET_ACCOUNT", "0rtie"),
    };

    if command != Command::Serve {
        if let Err(e) = commands::run(command, &state).await {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    let app = app(state).layer(TraceLayer::new_for_http());

    // Local only by default: there is no login yet and /media exposes the
    // shipment PDFs (personal data). Set BIND_ADDR=0.0.0.0:3000 to serve the LAN.
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(routes::router())
        .merge(routes::write_router())
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/media", ServeDir::new(&state.media_dir))
        .with_state(state)
}
