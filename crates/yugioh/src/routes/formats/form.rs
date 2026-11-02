//! Creating and changing formats (behind the login): a new format from a
//! Konami list, its name, cutoff and notes, its list (add a card, change a
//! status, take one off), a new version (a copy: the decks of the old one
//! keep it), deleting one no deck plays.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::NaiveDate;
use yugioh_domain::ban_list::BanStatus;

use super::format_page;
use crate::store::card_data;
use crate::store::db::{begin_write, Pool};
use crate::store::formats::{self, ensure_set_dates};
use crate::web::error::{AppError, AppResult};
use crate::web::filters;
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// The fields typed, to fill the form again.
#[derive(Debug, Default)]
pub struct FormatValues {
    name: String,
    cutoff: String,
    cutoff_set: String,
    based_on: String,
    notes: String,
}

impl FormatValues {
    fn based_on_is(&self, id: &i64) -> bool {
        self.based_on == id.to_string()
    }
}

#[derive(Template)]
#[template(path = "formats/add.html")]
pub struct AddFormatPage {
    errors: Vec<String>,
    values: FormatValues,
    /// The Konami lists: `(id, date)`, latest first.
    ban_lists: Vec<(i64, String)>,
    /// The sets with a TCG date: `(name, date)`, for "the date of set …".
    sets: Vec<(String, String)>,
}

async fn add_page(
    pool: &Pool,
    errors: Vec<String>,
    values: FormatValues,
) -> AppResult<AddFormatPage> {
    Ok(AddFormatPage {
        errors,
        values,
        ban_lists: sqlx::query_as("SELECT id, date FROM advanced_ban_list ORDER BY date DESC")
            .fetch_all(pool)
            .await?,
        sets: sqlx::query_as(
            "SELECT name, tcg_date FROM ygo_set WHERE tcg_date IS NOT NULL ORDER BY name COLLATE NOCASE",
        )
        .fetch_all(pool)
        .await?,
    })
}

/// `GET /formats/add` (the sets' dates fetched once if there are none yet).
pub async fn add_form(State(state): State<AppState>) -> AppResult<AddFormatPage> {
    ensure_set_dates(&state.pool, &state.http).await?;
    add_page(&state.pool, Vec::new(), FormatValues::default()).await
}

/// The cutoff: the date typed, else the TCG date of the set named.
async fn read_cutoff(pool: &Pool, form: &mut FormData) -> AppResult<Option<NaiveDate>> {
    if let Some(date) = form.date("cutoff", "Cutoff") {
        return Ok(Some(date));
    }
    let Some(set) = form.get("cutoff_set").map(str::to_string) else {
        if form.get("cutoff").is_none() {
            form.errors
                .push("Cutoff: give a date, or the set it ends with.".into());
        }
        return Ok(None);
    };

    let date: Option<String> =
        sqlx::query_scalar("SELECT tcg_date FROM ygo_set WHERE name = ? AND tcg_date IS NOT NULL")
            .bind(&set)
            .fetch_optional(pool)
            .await?;
    match date.and_then(|d| d.parse().ok()) {
        Some(date) => Ok(Some(date)),
        None => {
            form.errors
                .push(format!("Cutoff: no TCG date known for the set \"{set}\"."));
            Ok(None)
        }
    }
}

/// `POST /formats/add`: version 1 of a new format, its list copied from the
/// Konami list chosen.
pub async fn create(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let pool = &state.pool;
    let mut form = FormData::new(pairs);
    let values = FormatValues {
        name: form.text("name"),
        cutoff: form.text("cutoff"),
        cutoff_set: form.text("cutoff_set"),
        based_on: form.text("based_on"),
        notes: form.text("notes"),
    };
    let name = form.required("name", "Name");
    let cutoff = read_cutoff(pool, &mut form).await?;
    let based_on = match form.get("based_on") {
        Some(_) => form.id("based_on", "Starts from"),
        None => None,
    };
    if let Some(name) = &name {
        let taken: bool = sqlx::query_scalar("SELECT count(*) > 0 FROM deck_format WHERE name = ?")
            .bind(name)
            .fetch_one(pool)
            .await?;
        if taken {
            form.errors.push(format!(
                "A format is already named \"{name}\": make a new version from its page."
            ));
        }
    }

    let (Some(name), Some(cutoff), true) = (name, cutoff, form.errors.is_empty()) else {
        return Ok(add_page(pool, form.errors, values).await?.into_response());
    };
    let mut tx = begin_write(pool).await?;
    let id = formats::create(&mut tx, &name, cutoff, based_on, form.get("notes")).await?;
    tx.commit().await?;
    tracing::info!(format_id = id, "format created");

    Ok(Redirect::to(&format!("/formats/{id}")).into_response())
}

/// `POST /formats/:id/edit`: name, cutoff, notes.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let pool = &state.pool;
    let mut form = FormData::new(pairs);
    let name = form.required("name", "Name");
    let cutoff = form.required_date("cutoff", "Cutoff");
    let (Some(name), Some(cutoff), true) = (name, cutoff, form.errors.is_empty()) else {
        return Ok(format_page(pool, id, form.errors).await?.into_response());
    };

    let mut tx = begin_write(pool).await?;
    let result = sqlx::query("UPDATE deck_format SET name = ?, cutoff = ?, notes = ? WHERE id = ?")
        .bind(&name)
        .bind(cutoff.to_string())
        .bind(form.get("notes"))
        .bind(id)
        .execute(&mut *tx)
        .await;
    match result {
        Ok(done) if done.rows_affected() == 0 => return Err(AppError::NotFound),
        Ok(_) => {}
        // UNIQUE (name, version)
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            drop(tx);
            let error = format!("\"{name}\" already has a version with this number.");
            return Ok(format_page(pool, id, vec![error]).await?.into_response());
        }
        Err(e) => return Err(e.into()),
    }
    tx.commit().await?;
    tracing::info!(format_id = id, "format edited");

    Ok(Redirect::to(&format!("/formats/{id}")).into_response())
}

/// `POST /formats/:id/cards` (`name`: English, `status`): a card on the
/// list (ygoprodeck asked for a card not known yet), or its new status.
pub async fn add_card(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let pool = &state.pool;
    let mut form = FormData::new(pairs);
    let name = form.required("name", "Card");
    let status = form.get("status").and_then(BanStatus::from_code);
    if status.is_none() {
        form.errors.push("Status: choose one.".into());
    }
    let (Some(name), Some(status), true) = (name, status, form.errors.is_empty()) else {
        return Ok(format_page(pool, id, form.errors).await?.into_response());
    };

    // the network before the write transaction
    let Some(card_data_id) = card_data::get_or_fetch(pool, &state.http, &name).await? else {
        let error = format!("No card named \"{name}\" (its English name, as on ygoprodeck).");
        return Ok(format_page(pool, id, vec![error]).await?.into_response());
    };
    let mut tx = begin_write(pool).await?;
    crate::store::db::exists(&mut tx, "deck_format", id).await?;
    sqlx::query(
        "INSERT INTO deck_format_entry (format_id, card_data_id, status) VALUES (?, ?, ?)
         ON CONFLICT (format_id, card_data_id) DO UPDATE SET status = excluded.status",
    )
    .bind(id)
    .bind(card_data_id)
    .bind(status.code())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(format_id = id, card_data_id, "format list: card added");

    Ok(Redirect::to(&format!("/formats/{id}#list")).into_response())
}

/// `POST /formats/:id/cards/:card_data_id` (`status`; empty: off the list,
/// unlimited).
pub async fn set_status(
    State(state): State<AppState>,
    Path((id, card_data_id)): Path<(i64, i64)>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let form = FormData::new(pairs);
    let status = form.get("status");
    let mut tx = begin_write(&state.pool).await?;
    match status.map(BanStatus::from_code) {
        None => {
            sqlx::query("DELETE FROM deck_format_entry WHERE format_id = ? AND card_data_id = ?")
                .bind(id)
                .bind(card_data_id)
                .execute(&mut *tx)
                .await?;
        }
        Some(Some(status)) => {
            sqlx::query(
                "UPDATE deck_format_entry SET status = ? WHERE format_id = ? AND card_data_id = ?",
            )
            .bind(status.code())
            .bind(id)
            .bind(card_data_id)
            .execute(&mut *tx)
            .await?;
        }
        Some(None) => {
            drop(tx);
            let error = "Status: invalid choice.".to_string();
            return Ok(format_page(&state.pool, id, vec![error])
                .await?
                .into_response());
        }
    }
    tx.commit().await?;
    tracing::info!(
        format_id = id,
        card_data_id,
        ?status,
        "format list: status changed"
    );

    Ok(Redirect::to(&format!("/formats/{id}#list")).into_response())
}

/// `POST /formats/:id/new-version`: a copy numbered after the last one;
/// the next changes go there, the decks of this one keep it.
pub async fn new_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    let mut tx = begin_write(&state.pool).await?;
    crate::store::db::exists(&mut tx, "deck_format", id).await?;
    let new_id = formats::new_version(&mut tx, id).await?;
    tx.commit().await?;
    tracing::info!(
        from_format_id = id,
        format_id = new_id,
        "format: new version"
    );

    Ok(Redirect::to(&format!("/formats/{new_id}")).into_response())
}

/// `POST /formats/:id/delete`: refused while a deck version plays it.
pub async fn delete(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Response> {
    let mut tx = begin_write(&state.pool).await?;
    let used: i64 = sqlx::query_scalar("SELECT count(*) FROM deck_version WHERE format_id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if used > 0 {
        drop(tx);
        let error = format!(
            "{used} deck version{} play{} this format: choose another ban list or format for {} \
             in the deck builder first.",
            if used == 1 { "" } else { "s" },
            if used == 1 { "s" } else { "" },
            if used == 1 { "it" } else { "them" },
        );
        return Ok(format_page(&state.pool, id, vec![error])
            .await?
            .into_response());
    }
    let deleted = sqlx::query("DELETE FROM deck_format WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    tracing::info!(format_id = id, "format deleted");

    Ok(Redirect::to("/formats").into_response())
}
