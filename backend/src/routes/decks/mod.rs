pub mod api;
pub mod form;

use askama::Template;
use axum::extract::{Path, Query, State};
use sqlx::Row;

use crate::store::db::Pool;
use crate::store::models::DeckVersionRow;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::pagination::{Page, PageParam, PAGE_SIZE};
use crate::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct DeckRow {
    id: i64,
    name: String,
    description: Option<String>,
}

#[derive(Template)]
#[template(path = "decks/list.html")]
pub struct DecksList {
    page_title: &'static str,
    page: Page,
    total: i64,
    decks: Vec<DeckRow>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(param): Query<PageParam>,
) -> AppResult<DecksList> {
    let pool = &state.pool;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM deck")
        .fetch_one(pool)
        .await?;
    let page = Page::new(param.page, total, String::new());
    let decks =
        sqlx::query_as("SELECT id, name, description FROM deck ORDER BY name, id LIMIT ? OFFSET ?")
            .bind(PAGE_SIZE)
            .bind(page.offset())
            .fetch_all(pool)
            .await?;

    Ok(DecksList {
        page_title: "All Decks",
        page,
        total,
        decks,
    })
}

#[derive(Template)]
#[template(path = "decks/detail.html")]
pub struct DeckDetail {
    deck: DeckRow,
    versions: Vec<DeckVersionRow>,
    /// Errors of the "new version" form.
    errors: Vec<String>,
}

pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<DeckDetail> {
    load_detail(&state.pool, id, Vec::new()).await
}

pub async fn load_detail(pool: &Pool, id: i64, errors: Vec<String>) -> AppResult<DeckDetail> {
    let deck = sqlx::query_as("SELECT id, name, description FROM deck WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()?;
    let versions = sqlx::query_as(&format!(
        "{} WHERE deck_version.deck_id = ? ORDER BY deck_version.version_name DESC",
        DeckVersionRow::SELECT
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;

    Ok(DeckDetail {
        deck,
        versions,
        errors,
    })
}

/// Shell page of the deck builder: the Svelte app (built from `frontend/`)
/// mounts on `#app` and talks to `/api/deck-versions/:id`.
#[derive(Template)]
#[template(path = "decks/builder.html")]
pub struct DeckBuilder {
    id: i64,
    title: String,
}

pub async fn builder(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<DeckBuilder> {
    let row = sqlx::query(
        "SELECT deck.name, deck_version.version_name
         FROM deck_version LEFT JOIN deck ON deck.id = deck_version.deck_id
         WHERE deck_version.id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .or_404()?;
    let deck: Option<String> = row.get(0);
    let version: String = row.get(1);
    Ok(DeckBuilder {
        id,
        title: format!("{} - {version}", deck.as_deref().unwrap_or("No Deck")),
    })
}
