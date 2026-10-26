//! The app's binary: reads the settings, opens the database (running the
//! migrations), then serves every collection (`yugioh`, `books`, `pokemon`,
//! assembled by `common::app`) or runs a maintenance command.

mod commands;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use axum::Router;
use commands::Command;
use common::db;
use common::web::auth;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

pub use common::AppState;

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
    commands::spawn_weekly_price_refresh(state.clone());
    commands::spawn_daily_picture_cleanup(state.clone());
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

/// The server: the routes of every collection, assembled by `common::app`.
pub fn app(state: AppState) -> Router {
    common::app(
        state,
        vec![yugioh::routes(), books::routes(), pokemon::routes()],
    )
}
