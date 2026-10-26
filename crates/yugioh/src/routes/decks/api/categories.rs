//! Card categories of a deck version (plain or conditional) and their assignments.

use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};
use yugioh_domain::monte_carlo;

use crate::store::db::{begin_write, exists};
use crate::web::error::{bad_request, required, AppError, AppResult, OrNotFound};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct ApiCategory {
    id: i64,
    name: String,
    /// The rule of a conditional category, `None` for a plain one.
    condition: Option<ApiCondition>,
}

/// A card of a conditional category counts, in a hand, as a card of `then`
/// when the hand also has a card of one of the `if_any_of` categories, and
/// as a card of `otherwise` (or of nothing) when it doesn't.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiCondition {
    description: String,
    if_any_of: Vec<i64>,
    then: i64,
    otherwise: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct CategoryRecord {
    id: i64,
    name: String,
    condition_description: Option<String>,
    categorie_true_id: Option<i64>,
    categorie_false_id: Option<i64>,
}

/// The categories of a deck version, with their rules.
pub(super) async fn load_categories(
    conn: &mut SqliteConnection,
    version_id: i64,
) -> sqlx::Result<Vec<ApiCategory>> {
    let rows: Vec<CategoryRecord> = sqlx::query_as(
        "SELECT id, name, condition_description, categorie_true_id, categorie_false_id
         FROM card_category WHERE deck_version_id = ? ORDER BY id",
    )
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;
    let conditions: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT category_id, condition_category_id FROM card_category_condition
         WHERE category_id IN (SELECT id FROM card_category WHERE deck_version_id = ?)
         ORDER BY condition_category_id",
    )
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ApiCategory {
            id: r.id,
            name: r.name,
            condition: match (r.condition_description, r.categorie_true_id) {
                (Some(description), Some(then)) => Some(ApiCondition {
                    description,
                    if_any_of: conditions
                        .iter()
                        .filter(|(c, _)| *c == r.id)
                        .map(|(_, other)| *other)
                        .collect(),
                    then,
                    otherwise: r.categorie_false_id,
                }),
                _ => None,
            },
        })
        .collect())
}

impl From<ApiCategory> for monte_carlo::Category {
    fn from(c: ApiCategory) -> Self {
        monte_carlo::Category {
            id: c.id,
            name: c.name,
            condition: c.condition.map(|c| monte_carlo::Condition {
                if_any_of: c.if_any_of.into_iter().collect(),
                then: c.then,
                otherwise: c.otherwise,
            }),
        }
    }
}

pub async fn categories(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<ApiCategory>>> {
    let mut conn = state.pool.acquire().await?;
    exists(&mut conn, "deck_version", id).await?;
    Ok(Json(load_categories(&mut conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct CategoryInput {
    name: String,
    /// Present to create a conditional category.
    #[serde(default)]
    condition: Option<ApiCondition>,
}

pub async fn add_category(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<CategoryInput>,
) -> AppResult<(StatusCode, Json<ApiCategory>)> {
    let name = required(&input.name, "The category name can't be empty.")?.to_string();
    let mut condition = input.condition;
    if let Some(c) = &mut condition {
        c.description = required(&c.description, "Describe the condition.")?.to_string();
        if c.if_any_of.is_empty() {
            return bad_request("Choose at least one category for the condition.");
        }
        c.if_any_of.sort_unstable();
        c.if_any_of.dedup();
    }

    let mut tx = begin_write(&state.pool).await?;
    exists(&mut tx, "deck_version", id).await?;
    if let Some(c) = &condition {
        // every category of the rule must be one of this version's
        let mut wanted: HashSet<i64> = c.if_any_of.iter().copied().collect();
        wanted.insert(c.then);
        wanted.extend(c.otherwise);
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT count(*) FROM card_category WHERE deck_version_id = ",
        );
        query.push_bind(id).push(" AND id IN (");
        let mut ids = query.separated(", ");
        for category_id in &wanted {
            ids.push_bind(*category_id);
        }
        query.push(")");
        let found: i64 = query.build_query_scalar().fetch_one(&mut *tx).await?;
        if found as usize != wanted.len() {
            return bad_request("The condition uses categories that don't exist in this version.");
        }
    }

    let category_id = sqlx::query(
        "INSERT INTO card_category
             (name, deck_version_id, condition_description, categorie_true_id, categorie_false_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&name)
    .bind(id)
    .bind(condition.as_ref().map(|c| &c.description))
    .bind(condition.as_ref().map(|c| c.then))
    .bind(condition.as_ref().and_then(|c| c.otherwise))
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    for other in condition.iter().flat_map(|c| &c.if_any_of) {
        sqlx::query(
            "INSERT INTO card_category_condition (category_id, condition_category_id)
             VALUES (?, ?)",
        )
        .bind(category_id)
        .bind(other)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!(
        version_id = id,
        category_id,
        conditional = condition.is_some(),
        "category added"
    );

    Ok((
        StatusCode::CREATED,
        Json(ApiCategory {
            id: category_id,
            name,
            condition,
        }),
    ))
}

pub async fn delete_category(
    State(state): State<AppState>,
    Path((id, category_id)): Path<(i64, i64)>,
) -> AppResult<StatusCode> {
    let result = sqlx::query("DELETE FROM card_category WHERE id = ? AND deck_version_id = ?")
        .bind(category_id)
        .bind(id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    tracing::info!(version_id = id, category_id, "category deleted");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CardCategory {
    id: i64,
    name: String,
    assigned: bool,
}

/// The version's categories, flagged with whether this card is in them.
pub async fn card_categories(
    State(state): State<AppState>,
    Path((id, card_id)): Path<(i64, i64)>,
) -> AppResult<Json<Vec<CardCategory>>> {
    Ok(Json(
        sqlx::query_as(
            "SELECT c.id, c.name, EXISTS (
                 SELECT 1 FROM card_category_assignment a
                 WHERE a.category_id = c.id AND a.card_id = ?) AS assigned
             FROM card_category c WHERE c.deck_version_id = ? ORDER BY c.id",
        )
        .bind(card_id)
        .bind(id)
        .fetch_all(&state.pool)
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct AssignInput {
    assigned: bool,
}

pub async fn assign_category(
    State(state): State<AppState>,
    Path((id, card_id, category_id)): Path<(i64, i64, i64)>,
    Json(input): Json<AssignInput>,
) -> AppResult<StatusCode> {
    let mut conn = state.pool.acquire().await?;
    let belongs: Option<i64> =
        sqlx::query_scalar("SELECT id FROM card_category WHERE id = ? AND deck_version_id = ?")
            .bind(category_id)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    belongs.or_404()?;
    exists(&mut conn, "card", card_id).await?;

    let sql = if input.assigned {
        "INSERT INTO card_category_assignment (category_id, card_id) VALUES (?, ?)
         ON CONFLICT DO NOTHING"
    } else {
        "DELETE FROM card_category_assignment WHERE category_id = ? AND card_id = ?"
    };
    sqlx::query(sql)
        .bind(category_id)
        .bind(card_id)
        .execute(&mut *conn)
        .await?;
    tracing::info!(
        version_id = id,
        card_id,
        category_id,
        assigned = input.assigned,
        "card category changed"
    );
    Ok(StatusCode::NO_CONTENT)
}
