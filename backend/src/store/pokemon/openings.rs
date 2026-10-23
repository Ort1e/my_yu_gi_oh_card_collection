//! Booster openings (`pokemon_opening`, migration 0013): the booster form
//! saves one with its copies (`pokemon_copy.opening_id`); cards can be added
//! to it later, and a copy moved to another one on its edit page. Never
//! deleted, like the copies.

use chrono::NaiveDate;
use sqlx::SqliteConnection;

use crate::store::db::{today, Pool};

/// An opening as the pages show it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Opening {
    pub id: i64,
    pub language: String,
    pub set_code: String,
    pub set_name: String,
    pub opened_on: Option<String>,
    pub note: Option<String>,
    /// Its copies (sold ones included).
    pub cards: i64,
}

impl Opening {
    const SELECT: &'static str = "SELECT o.id, o.language, o.set_code, o.set_name, o.opened_on,
            o.note, (SELECT count(*) FROM pokemon_copy WHERE opening_id = o.id) AS cards
         FROM pokemon_opening o";

    /// `2026-10-23 · 30ᵉ Anniversaire (fr, 10 cards)`: the copy forms' choice.
    pub fn label(&self) -> String {
        format!(
            "{} · {} ({}, {} cards)",
            self.opened_on.as_deref().unwrap_or("no date"),
            self.set_name,
            self.language,
            self.cards
        )
    }
}

/// Every opening, the latest first.
pub async fn all(pool: &Pool) -> sqlx::Result<Vec<Opening>> {
    sqlx::query_as(&format!(
        "{} ORDER BY o.opened_on IS NULL, o.opened_on DESC, o.id DESC",
        Opening::SELECT
    ))
    .fetch_all(pool)
    .await
}

pub async fn get(pool: &Pool, id: i64) -> sqlx::Result<Option<Opening>> {
    sqlx::query_as(&format!("{} WHERE o.id = ?", Opening::SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn insert(
    conn: &mut SqliteConnection,
    language: &str,
    set_code: &str,
    set_name: &str,
    opened_on: Option<NaiveDate>,
) -> sqlx::Result<i64> {
    let done = sqlx::query(
        "INSERT INTO pokemon_opening (language, set_code, set_name, opened_on, added_on)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(language)
    .bind(set_code)
    .bind(set_name)
    .bind(opened_on.map(|d| d.to_string()))
    .bind(today().to_string())
    .execute(conn)
    .await?;
    Ok(done.last_insert_rowid())
}

/// The date and the note; `false` when there is no such opening. The date
/// goes to its copies too (they were obtained that day).
pub async fn update(
    conn: &mut SqliteConnection,
    id: i64,
    opened_on: Option<NaiveDate>,
    note: Option<&str>,
) -> sqlx::Result<bool> {
    let opened_on = opened_on.map(|d| d.to_string());
    let done = sqlx::query("UPDATE pokemon_opening SET opened_on = ?, note = ? WHERE id = ?")
        .bind(&opened_on)
        .bind(note)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    if done.rows_affected() == 0 {
        return Ok(false);
    }
    sqlx::query("UPDATE pokemon_copy SET obtained_on = ? WHERE opening_id = ?")
        .bind(&opened_on)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(true)
}
