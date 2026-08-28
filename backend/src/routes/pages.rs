use askama::Template;
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use sqlx::SqlitePool;

use crate::db::Pool;

pub fn router() -> Router<Pool> {
    Router::new()
        .route("/", get(home))
        .route("/items", get(items_partial).post(add_item))
        .route("/complex", get(complex_page))
}

// --- Home page: plain server-rendered page ---

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    title: &'static str,
}

async fn home() -> HomeTemplate {
    HomeTemplate { title: "Home" }
}

// --- Example of a htmx-powered "simple page" pattern:
// the page loads once, then a small fragment is swapped in
// by htmx without a full page reload or any JS framework. ---

#[derive(sqlx::FromRow)]
struct Item {
    id: i64,
    name: String,
}

#[derive(Template)]
#[template(path = "items_fragment.html")]
struct ItemsFragment {
    items: Vec<Item>,
}

async fn items_partial(State(pool): State<Pool>) -> ItemsFragment {
    let items = sqlx::query_as::<_, Item>("SELECT id, name FROM items ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
    ItemsFragment { items }
}

#[derive(serde::Deserialize)]
struct NewItem {
    name: String,
}

async fn add_item(
    State(pool): State<SqlitePool>,
    axum::Form(payload): axum::Form<NewItem>,
) -> ItemsFragment {
    sqlx::query("INSERT INTO items (name) VALUES (?)")
        .bind(&payload.name)
        .execute(&pool)
        .await
        .ok();
    items_partial(State(pool)).await
}

// --- The one complex page: just serves a shell div,
// the Svelte bundle takes over from there. ---

#[derive(Template)]
#[template(path = "complex_page.html")]
struct ComplexPageTemplate {
    title: &'static str,
}

async fn complex_page() -> ComplexPageTemplate {
    ComplexPageTemplate {
        title: "Complex Page",
    }
}
