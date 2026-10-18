//! The reserve: owned cards to add to a deck version.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use sqlx::QueryBuilder;

use super::{load_cards, push_ids, ApiCard};
use crate::cards::version_card_ids;
use crate::db::exists;
use crate::error::AppResult;
use crate::routes::cards::CardFilter;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ReserveQuery {
    name: Option<String>,
    code: Option<String>,
    card_type: Option<String>,
}

const RESERVE_LIMIT: i64 = 300;

/// Owned (non-proxy, unsold) cards matching the filters and not in this version.
pub async fn reserve(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(query): Json<ReserveQuery>,
) -> AppResult<Json<Vec<ApiCard>>> {
    let mut conn = state.pool.acquire().await?;
    exists(&mut conn, "deck_version", id).await?;
    let filter = CardFilter {
        name: query.name,
        code: query.code,
        card_type: query.card_type,
        proxy: Some("false".into()),
        sold: Some("false".into()),
        status: None,
    };
    let in_version = version_card_ids(&mut conn, id).await?;

    let mut qb = QueryBuilder::new("SELECT card.id FROM card");
    filter.push_where(&mut qb);
    if !in_version.is_empty() {
        qb.push(" AND card.id NOT IN ");
        push_ids(&mut qb, in_version);
    }
    qb.push(" ORDER BY card.en_name, card.id LIMIT ")
        .push_bind(RESERVE_LIMIT);
    let ids: Vec<i64> = qb.build_query_scalar().fetch_all(&mut *conn).await?;
    Ok(Json(load_cards(&mut conn, &ids).await?))
}
