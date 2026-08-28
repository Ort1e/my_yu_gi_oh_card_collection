mod db;
mod routes;

use axum::Router;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let pool = db::init_pool().await;

    let app = Router::new()
        // Simple, server-rendered pages (askama + htmx)
        .merge(routes::pages::router())
        // JSON API consumed by the Svelte app on the complex page
        .nest("/api", routes::api::router())
        // Serves /static/* — this is where the built Svelte bundle for
        // the complex page ends up (frontend/dist -> static/complex/)
        .nest_service("/static", ServeDir::new("static"))
        .with_state(pool)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
