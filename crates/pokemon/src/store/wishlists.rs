//! The Pokémon wishlists (`pokemon_wishlist`, migration 0030): one per
//! person, each with its own edit permission `pokemon_wishlist:<id>` in
//! `common`'s `app_permission` (written here with the list, so it is added,
//! renamed and removed with it: deleting the permission drops its grants).
//! After any write here, the routes reload `Auth` (`state.auth.reload`).

use common::db::{begin_write, Pool};
use sqlx::SqliteConnection;

use super::cards;

/// The permission key that lets a user change this list's wishes.
pub fn permission(id: i64) -> String {
    format!("pokemon_wishlist:{id}")
}

/// How the permission is named on the users page (after "change").
fn label(name: &str) -> String {
    format!("the Pokémon wishlist “{name}”")
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Wishlist {
    pub id: i64,
    pub name: String,
    pub fills_from_collection: bool,
}

/// A list with its counts, for the index.
#[derive(Debug, sqlx::FromRow)]
pub struct Summary {
    pub id: i64,
    pub name: String,
    pub fills_from_collection: bool,
    pub wanted: i64,
    pub got: i64,
}

pub async fn summaries(pool: &Pool) -> sqlx::Result<Vec<Summary>> {
    sqlx::query_as(
        "SELECT l.id, l.name, l.fills_from_collection,
                COALESCE(SUM(w.id IS NOT NULL AND w.got_on IS NULL), 0) AS wanted,
                COALESCE(SUM(w.got_on IS NOT NULL), 0) AS got
         FROM pokemon_wishlist l LEFT JOIN pokemon_wish w ON w.wishlist_id = l.id
         GROUP BY l.id ORDER BY l.position, l.id",
    )
    .fetch_all(pool)
    .await
}

pub async fn find(pool: &Pool, id: i64) -> sqlx::Result<Option<Wishlist>> {
    sqlx::query_as("SELECT id, name, fills_from_collection FROM pokemon_wishlist WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// The list a wish is on.
pub async fn of_wish(pool: &Pool, wish_id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT wishlist_id FROM pokemon_wish WHERE id = ?")
        .bind(wish_id)
        .fetch_optional(pool)
        .await
        .map(Option::flatten)
}

/// Whether a list is called this (any case), other than `except`.
pub async fn name_taken(pool: &Pool, name: &str, except: Option<i64>) -> sqlx::Result<bool> {
    let taken: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pokemon_wishlist WHERE name = ? AND id != ?")
            .bind(name)
            .bind(except.unwrap_or(0))
            .fetch_one(pool)
            .await?;
    Ok(taken > 0)
}

/// A new list and its permission.
pub async fn create(pool: &Pool, name: &str, fills_from_collection: bool) -> sqlx::Result<i64> {
    let mut tx = begin_write(pool).await?;
    let position: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(position), 0) + 1 FROM pokemon_wishlist")
            .fetch_one(&mut *tx)
            .await?;
    let id = sqlx::query(
        "INSERT INTO pokemon_wishlist (name, fills_from_collection, position) VALUES (?, ?, ?)",
    )
    .bind(name)
    .bind(fills_from_collection)
    .bind(position)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    sqlx::query("INSERT INTO app_permission (key, label, position) VALUES (?, ?, ?)")
        .bind(permission(id))
        .bind(label(name))
        .bind(100 + position)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(id)
}

/// Rename a list (and its permission's label) and set its flag.
pub async fn update(
    pool: &Pool,
    id: i64,
    name: &str,
    fills_from_collection: bool,
) -> sqlx::Result<bool> {
    let mut tx = begin_write(pool).await?;
    let done =
        sqlx::query("UPDATE pokemon_wishlist SET name = ?, fills_from_collection = ? WHERE id = ?")
            .bind(name)
            .bind(fills_from_collection)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    sqlx::query("UPDATE app_permission SET label = ? WHERE key = ?")
        .bind(label(name))
        .bind(permission(id))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(done.rows_affected() == 1)
}

/// Delete a list with its wishes and its permission (users lose that
/// grant); the cards nobody owns or wishes any more go too.
pub async fn delete(pool: &Pool, id: i64) -> sqlx::Result<bool> {
    let mut tx = begin_write(pool).await?;
    sqlx::query("DELETE FROM app_permission WHERE key = ?")
        .bind(permission(id))
        .execute(&mut *tx)
        .await?;
    let done = sqlx::query("DELETE FROM pokemon_wishlist WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    cards::delete_orphans(&mut tx).await?;
    tx.commit().await?;
    Ok(done.rows_affected() == 1)
}

/// The lists the copies added to the collection tick off.
pub async fn filled_from_collection(conn: &mut SqliteConnection) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar("SELECT id FROM pokemon_wishlist WHERE fills_from_collection = 1")
        .fetch_all(conn)
        .await
}
