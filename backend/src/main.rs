mod commands;
mod routes;
mod store;
#[cfg(test)]
mod tests;
mod web;

use std::path::PathBuf;

use axum::http::{header, HeaderValue};
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use axum::Router;
use commands::Command;
use store::db;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeader;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::level_filters::LevelFilter;
use tracing::Level;
use tracing_subscriber::EnvFilter;
use web::auth;

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
    pub auth: auth::Auth,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();

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
    let pool = db::init_pool().await;
    let auth = auth::Auth::load(&pool, std::env::var("APP_PASSWORD").ok())
        .await
        .expect("cannot load the session key");
    let state = AppState {
        pool,
        http: reqwest::Client::new(),
        media_dir: PathBuf::from(env("MEDIA_DIR", "data/media")),
        uploads_dir: PathBuf::from(env("UPLOADS_DIR", "uploads")),
        cardmarket_account: env("CARDMARKET_ACCOUNT", "0rtie"),
        auth,
    };

    // The DB is tracked in git (data submodule): always close the pool on the
    // way out, so SQLite checkpoints the WAL into app.db and deletes it.
    let pool = state.pool.clone();

    if command != Command::Serve {
        let result = commands::run(command, &state).await;
        pool.close().await;
        if let Err(e) = result {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    let login = state.auth.enabled();
    commands::spawn_daily_cleanup(state.clone());
    let app = app(state);

    // Local only by default. Serving the LAN (BIND_ADDR=0.0.0.0:3000) needs
    // APP_PASSWORD: without login anyone could edit, and read the shipment
    // PDFs (personal data).
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    let local = listener.local_addr().unwrap();
    if !login && !local.ip().is_loopback() {
        eprintln!("error: set APP_PASSWORD to listen on {local} (not a loopback address)");
        std::process::exit(1);
    }
    tracing::info!(
        "listening on {local}, login {}",
        if login { "enabled" } else { "disabled" }
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
    pool.close().await;
    tracing::info!("stopped");
}

/// Ctrl-C or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("cannot listen for SIGTERM");
    tokio::select! {
        _ = ctrl_c => {}
        _ = term.recv() => {}
    }
}

pub fn app(state: AppState) -> Router {
    let login_required = from_fn_with_state(state.clone(), auth::require_login);
    let shipments = Router::new()
        .nest_service(
            "/media/shipment_files",
            ServeDir::new(state.media_dir.join("shipment_files")),
        )
        .layer(login_required.clone());

    Router::new()
        .merge(routes::router())
        .merge(routes::write_router().route_layer(login_required))
        .route("/login", get(auth::page).post(auth::login))
        .route("/logout", post(auth::logout))
        // One INFO line per page or API call: method, path, status and time.
        // A layer only wraps the routes added before it, so the files below
        // (card images, CSS, media) are not logged.
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .merge(routes::files_router())
        .merge(shipments)
        // CSS and the deck builder bundle: `no-cache` makes browsers check for
        // a newer file on each page load (a 304 when unchanged) instead of
        // reusing a stale copy after an update.
        .nest_service(
            "/static",
            SetResponseHeader::overriding(
                ServeDir::new("static"),
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            ),
        )
        .nest_service("/media", ServeDir::new(&state.media_dir))
        .with_state(state)
}
