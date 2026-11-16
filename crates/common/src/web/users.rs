//! The users page (`/users`, admins only): who may log in and what each may
//! change. The permissions are read generically from `app_permission`
//! (`yugioh`, `books`, `pokemon`, and one per Pokémon wishlist, which the
//! Pokémon crate writes), so this crate knows nothing of those tables.
//!
//! Every write reloads [`Auth`], so a change takes effect at once: a new
//! password (the session cookie signs the hash) logs that user out, except
//! the admin who changed their own, who is given a fresh cookie. Refused:
//! deleting yourself, and taking away or deleting the last admin.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};

use crate::db::{begin_write, today, Pool};
use crate::web::auth::{hash_password, valid_user_name, CurrentUser, User};
use crate::web::error::{AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

const MIN_PASSWORD: usize = 8;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(page))
        .route("/users/add", post(add))
        .route("/users/:id", post(save))
        .route("/users/:id/delete", post(delete))
}

/// Create a user with these permissions (`hash`: an argon2 string).
pub async fn create(
    pool: &Pool,
    name: &str,
    hash: &str,
    admin: bool,
    keys: &[String],
) -> sqlx::Result<i64> {
    let mut tx = begin_write(pool).await?;
    let id = sqlx::query(
        "INSERT INTO app_user (name, password_hash, is_admin, created_on) VALUES (?, ?, ?, ?)",
    )
    .bind(name)
    .bind(hash)
    .bind(admin)
    .bind(today().to_string())
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    grant(&mut tx, id, keys).await?;
    tx.commit().await?;
    Ok(id)
}

async fn grant(conn: &mut sqlx::SqliteConnection, id: i64, keys: &[String]) -> sqlx::Result<()> {
    for key in keys {
        sqlx::query(
            "INSERT OR IGNORE INTO app_user_permission (user_id, permission) VALUES (?, ?)",
        )
        .bind(id)
        .bind(key)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Change a user; `hash` `None` keeps the password.
async fn update(
    pool: &Pool,
    id: i64,
    name: &str,
    hash: Option<&str>,
    admin: bool,
    keys: &[String],
) -> sqlx::Result<()> {
    let mut tx = begin_write(pool).await?;
    sqlx::query(
        "UPDATE app_user SET name = ?, is_admin = ?, password_hash = COALESCE(?, password_hash)
         WHERE id = ?",
    )
    .bind(name)
    .bind(admin)
    .bind(hash)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM app_user_permission WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    grant(&mut tx, id, keys).await?;
    tx.commit().await
}

/// Set a user's password, creating the user when missing (`backend
/// set-password`). `admin`: also make them an admin.
pub async fn set_password(pool: &Pool, name: &str, password: &str, admin: bool) -> AppResult<bool> {
    let hash = hash_blocking(password).await?;
    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM app_user WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    match existing {
        Some(id) => {
            sqlx::query(
                "UPDATE app_user SET password_hash = ?, is_admin = is_admin OR ? WHERE id = ?",
            )
            .bind(hash)
            .bind(admin)
            .bind(id)
            .execute(pool)
            .await?;
            Ok(false)
        }
        None => {
            create(pool, name, &hash, admin, &[]).await?;
            Ok(true)
        }
    }
}

async fn hash_blocking(password: &str) -> AppResult<String> {
    let password = password.to_string();
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| crate::web::error::AppError::Internal(e.to_string()))?
        .map_err(crate::web::error::AppError::Internal)
}

struct Tick {
    key: String,
    label: String,
    checked: bool,
}

struct UserRow {
    id: i64,
    name: String,
    admin: bool,
    is_me: bool,
    boxes: Vec<Tick>,
}

#[derive(Template)]
#[template(path = "users.html")]
struct UsersPage {
    users: Vec<UserRow>,
    /// The empty boxes of the "add" form.
    new_boxes: Vec<Tick>,
    errors: Vec<String>,
    saved: bool,
}

fn boxes(state: &AppState, granted: impl Fn(&str) -> bool) -> Vec<Tick> {
    state
        .auth
        .grantable()
        .into_iter()
        .map(|(key, label)| Tick {
            checked: granted(&key),
            key,
            label,
        })
        .collect()
}

fn render(state: &AppState, me: &CurrentUser, errors: Vec<String>, saved: bool) -> UsersPage {
    let users = state
        .auth
        .users()
        .into_iter()
        .map(|user| UserRow {
            id: user.id,
            is_me: user.name == me.name,
            boxes: boxes(state, |key| user.permissions.contains(key)),
            name: user.name,
            admin: user.admin,
        })
        .collect();
    UsersPage {
        users,
        new_boxes: boxes(state, |_| false),
        errors,
        saved,
    }
}

#[derive(serde::Deserialize)]
struct Saved {
    saved: Option<String>,
}

async fn page(
    State(state): State<AppState>,
    me: CurrentUser,
    Query(query): Query<Saved>,
) -> AppResult<UsersPage> {
    me.require_admin()?;
    Ok(render(&state, &me, Vec::new(), query.saved.is_some()))
}

/// The page again with the errors (nothing saved).
fn refused(state: &AppState, me: &CurrentUser, errors: Vec<String>) -> Response {
    (StatusCode::BAD_REQUEST, render(state, me, errors, false)).into_response()
}

/// The ticked permissions that exist.
fn ticked(state: &AppState, pairs: &Pairs) -> Vec<String> {
    let grantable = state.auth.grantable();
    pairs
        .iter()
        .filter(|(name, _)| name == "perm")
        .map(|(_, key)| key.clone())
        .filter(|key| grantable.iter().any(|(known, _)| known == key))
        .collect()
}

/// The name of the form, checked: valid and free (`except`: the user's own).
fn checked_name(state: &AppState, form: &FormData, except: Option<i64>) -> Result<String, String> {
    let name = form.text("name").to_lowercase();
    if !valid_user_name(&name) {
        return Err("The name is lower case letters, digits and _ (32 at most).".into());
    }
    if state
        .auth
        .users()
        .iter()
        .any(|u| u.name == name && Some(u.id) != except)
    {
        return Err(format!("There already is a user {name}."));
    }
    Ok(name)
}

fn password_error(password: &str) -> Option<String> {
    (password.chars().count() < MIN_PASSWORD)
        .then(|| format!("The password takes {MIN_PASSWORD} characters at least."))
}

async fn add(
    State(state): State<AppState>,
    me: CurrentUser,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    me.require_admin()?;
    let keys = ticked(&state, &pairs);
    let form = FormData::new(pairs);
    let password = form.text("password");

    let mut errors = Vec::new();
    let name = checked_name(&state, &form, None)
        .map_err(|e| errors.push(e))
        .ok();
    errors.extend(password_error(&password));
    let Some(name) = name.filter(|_| errors.is_empty()) else {
        return Ok(refused(&state, &me, errors));
    };

    let hash = hash_blocking(&password).await?;
    create(&state.pool, &name, &hash, form.checked("admin"), &keys).await?;
    state.auth.reload(&state.pool).await?;
    tracing::info!(by = me.name, user = name, "user created");
    Ok(Redirect::to("/users?saved=1").into_response())
}

fn other_admins(state: &AppState, id: i64) -> bool {
    state.auth.users().iter().any(|u| u.admin && u.id != id)
}

async fn save(
    State(state): State<AppState>,
    me: CurrentUser,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    me.require_admin()?;
    let user: User = state
        .auth
        .users()
        .into_iter()
        .find(|u| u.id == id)
        .or_404()?;
    let keys = ticked(&state, &pairs);
    let form = FormData::new(pairs);
    let admin = form.checked("admin");
    let password = form.text("password");

    let mut errors = Vec::new();
    let name = checked_name(&state, &form, Some(id))
        .map_err(|e| errors.push(e))
        .ok();
    if !password.is_empty() {
        errors.extend(password_error(&password));
    }
    if user.admin && !admin && !other_admins(&state, id) {
        errors.push("This is the last admin: it stays one.".into());
    }
    let Some(name) = name.filter(|_| errors.is_empty()) else {
        return Ok(refused(&state, &me, errors));
    };

    let hash = match password.as_str() {
        "" => None,
        password => Some(hash_blocking(password).await?),
    };
    update(&state.pool, id, &name, hash.as_deref(), admin, &keys).await?;
    state.auth.reload(&state.pool).await?;
    tracing::info!(by = me.name, user = name, "user changed");

    // an admin's own session follows their new name or password; another
    // user's ends with it (the cookie signs the hash)
    let mut response = Redirect::to("/users?saved=1").into_response();
    if user.name == me.name {
        if let Some(cookie) = state.auth.fresh_cookie(&name) {
            if let Ok(value) = cookie.parse() {
                response.headers_mut().insert(header::SET_COOKIE, value);
            }
        }
    }
    Ok(response)
}

async fn delete(
    State(state): State<AppState>,
    me: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    me.require_admin()?;
    let user = state
        .auth
        .users()
        .into_iter()
        .find(|u| u.id == id)
        .or_404()?;

    let mut errors = Vec::new();
    if user.name == me.name {
        errors.push("You can't delete yourself.".to_string());
    } else if user.admin && !other_admins(&state, id) {
        errors.push("This is the last admin: it stays.".to_string());
    }
    if !errors.is_empty() {
        return Ok(refused(&state, &me, errors));
    }

    sqlx::query("DELETE FROM app_user WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    state.auth.reload(&state.pool).await?;
    tracing::info!(by = me.name, user = user.name, "user deleted");
    Ok(Redirect::to("/users?saved=1").into_response())
}
