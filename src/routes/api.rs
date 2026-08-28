use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::db::Pool;

pub fn router() -> Router<Pool> {
    Router::new().route("/items", get(list_items).post(create_item))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Item {
    pub id: i64,
    pub name: String,
}

async fn list_items(State(pool): State<Pool>) -> Json<Vec<Item>> {
    let items = sqlx::query_as::<_, Item>("SELECT id, name FROM items ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
    Json(items)
}

#[derive(serde::Deserialize)]
pub struct NewItem {
    pub name: String,
}

async fn create_item(State(pool): State<Pool>, Json(payload): Json<NewItem>) -> Json<Item> {
    let id = sqlx::query("INSERT INTO items (name) VALUES (?)")
        .bind(&payload.name)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    Json(Item {
        id,
        name: payload.name,
    })
}
