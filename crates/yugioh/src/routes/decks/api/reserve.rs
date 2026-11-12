//! The card search of the deck builder: cards to add to a version, the
//! owned ones (by name, code, type) and, by name, the others known to the
//! app. Each result is one tile with what the collection holds of it.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use sqlx::QueryBuilder;

use super::{card_tiles, ApiCard};
use crate::routes::cards::CardFilter;
use crate::store::db::{exists, push_ids};
use crate::web::error::AppResult;
use crate::AppState;
use common::sql::like_pattern;
use ts_rs::TS;

#[derive(Debug, Deserialize, TS)]
#[ts(export, optional_fields = nullable)]
pub struct ReserveQuery {
    name: Option<String>,
    code: Option<String>,
    card_type: Option<String>,
}

const RESERVE_LIMIT: i64 = 300;

/// The cards matching the filters: those with an unsold copy first (named
/// like it), then, without a code filter, the other known cards named so.
pub async fn reserve(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(query): Json<ReserveQuery>,
) -> AppResult<Json<Vec<ApiCard>>> {
    let mut conn = state.pool.acquire().await?;
    exists(&mut conn, "deck_version", id).await?;
    let text = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let (name, code, card_type) = (text(&query.name), text(&query.code), text(&query.card_type));
    let filter = CardFilter {
        name: name.clone(),
        code: code.clone(),
        card_type: card_type.clone(),
        sold: Some("false".into()),
        ..CardFilter::default()
    };

    let mut qb = QueryBuilder::new("SELECT card.card_data_id, MIN(card.name) FROM card");
    filter.push_where(&mut qb);
    qb.push(
        " AND card.card_data_id IS NOT NULL
         GROUP BY card.card_data_id ORDER BY MIN(card.en_name) LIMIT ",
    )
    .push_bind(RESERVE_LIMIT);
    let mut found: Vec<(Option<i64>, String)> = qb.build_query_as().fetch_all(&mut *conn).await?;

    if let (Some(name), None) = (&name, &code) {
        let mut qb = QueryBuilder::new(
            "SELECT id, coalesce(json_extract(json_data, '$.name'), en_name) FROM card_data
             WHERE en_name LIKE ",
        );
        qb.push_bind(like_pattern(name)).push(" ESCAPE '\\'");
        if let Some(card_type) = &card_type {
            qb.push(" AND card_type = ")
                .push_bind(card_type.clone())
                .push(" COLLATE NOCASE");
        }
        let owned: Vec<i64> = found.iter().filter_map(|(id, _)| *id).collect();
        if !owned.is_empty() {
            qb.push(" AND id NOT IN ");
            push_ids(&mut qb, owned);
        }
        qb.push(" ORDER BY en_name LIMIT ")
            .push_bind(RESERVE_LIMIT - found.len() as i64);
        let others: Vec<(i64, String)> = qb.build_query_as().fetch_all(&mut *conn).await?;
        found.extend(others.into_iter().map(|(id, name)| (Some(id), name)));
    }
    Ok(Json(card_tiles(&mut conn, id, &found).await?))
}
