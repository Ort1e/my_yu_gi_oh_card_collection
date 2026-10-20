pub mod api;
pub mod built;
pub mod form;

use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use sqlx::Row;

use crate::store::db::Pool;
use crate::store::models::DeckVersionRow;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::pagination::{self, Page, PageParam, PAGE_SIZE};
use crate::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct DeckRow {
    id: i64,
    name: String,
    description: Option<String>,
}

/// One deck of the list, with its versions count and last tournament.
#[derive(Debug, sqlx::FromRow)]
pub struct DeckListRow {
    id: i64,
    name: String,
    description: Option<String>,
    versions: i64,
    /// The date of the latest tournament one of its versions was played in.
    last_tournament: Option<String>,
}

/// The list's sort: `newest` (default: the last created deck first) or
/// `tournament` (the latest played in a tournament first, never played at
/// the end, newest first).
#[derive(Debug, Default, Deserialize)]
pub struct DeckSort {
    sort: Option<String>,
}

impl DeckSort {
    fn is_tournament(&self) -> bool {
        self.sort.as_deref() == Some("tournament")
    }

    fn order_by(&self) -> &'static str {
        if self.is_tournament() {
            " ORDER BY last_tournament IS NULL, last_tournament DESC, deck.id DESC"
        } else {
            " ORDER BY deck.id DESC"
        }
    }

    fn querystring(&self) -> String {
        pagination::querystring([("sort", self.sort.as_deref())])
    }
}

#[derive(Template)]
#[template(path = "decks/list.html")]
pub struct DecksList {
    page_title: &'static str,
    page: Page,
    total: i64,
    decks: Vec<DeckListRow>,
    sort: DeckSort,
}

impl DecksList {
    fn sort_is_tournament(&self) -> bool {
        self.sort.is_tournament()
    }
}

pub async fn list(
    State(state): State<AppState>,
    Query(param): Query<PageParam>,
    Query(sort): Query<DeckSort>,
) -> AppResult<DecksList> {
    let pool = &state.pool;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM deck")
        .fetch_one(pool)
        .await?;
    let page = Page::new(param.page, total, sort.querystring());
    let decks = sqlx::query_as(&format!(
        "SELECT deck.id, deck.name, deck.description,
           (SELECT count(*) FROM deck_version v WHERE v.deck_id = deck.id) AS versions,
           (SELECT max(t.date) FROM deck_version v JOIN tournament t ON t.id = v.tournament_id
            WHERE v.deck_id = deck.id) AS last_tournament
         FROM deck{} LIMIT ? OFFSET ?",
        sort.order_by()
    ))
    .bind(PAGE_SIZE)
    .bind(page.offset())
    .fetch_all(pool)
    .await?;

    Ok(DecksList {
        page_title: "All Decks",
        page,
        total,
        decks,
        sort,
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
