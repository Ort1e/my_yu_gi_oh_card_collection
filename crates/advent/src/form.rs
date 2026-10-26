//! Creating a calendar and filling its days, one at a time (behind the
//! login). A day is an item of the collection ([`Calendar::find`]: a card by
//! its code or `#id`, a book by its ISBN or `#id`) and a photo.
//!
//! Photos ([`calendar_photos`]): the copy shown on the page is upright, at
//! most 1600 px and without metadata; the original is kept in `originals/`.

use askama::Template;
use axum::extract::{Multipart, Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::Datelike;

use crate::day_form::{back_to, check_day, DayForm};
use crate::pages::{load_doors, Door};
use crate::{calendar_id, calendar_photos, Calendar, Pick, Spec};
use common::db::{begin_write, Pool};
use common::web::error::{AppResult, OrNotFound};
use common::web::forms::{FormData, Pairs};
use common::AppState;

#[derive(Template)]
#[template(path = "advent/edit.html")]
pub struct EditPage {
    spec: &'static Spec,
    year: i64,
    doors: Vec<Door>,
    errors: Vec<String>,
    /// Above the days (HTML).
    above: String,
    /// Suggested in the day fields: `(id, label)`.
    suggestions: Vec<(i64, String)>,
}

async fn edit_page_with<C: Calendar>(
    pool: &Pool,
    year: i64,
    errors: Vec<String>,
) -> AppResult<EditPage> {
    let calendar = calendar_id::<C>(pool, year).await?;
    let extras = C::edit_extras(pool, calendar, year).await?;
    Ok(EditPage {
        spec: C::SPEC,
        year,
        doors: load_doors::<C>(pool, calendar).await?,
        errors,
        above: extras.above,
        suggestions: extras.suggestions,
    })
}

/// `GET <base>/:year/edit`.
pub async fn edit_page<C: Calendar>(
    State(state): State<AppState>,
    Path(year): Path<i64>,
) -> AppResult<EditPage> {
    edit_page_with::<C>(&state.pool, year, Vec::new()).await
}

#[derive(Template)]
#[template(path = "advent/add.html")]
pub struct AddPage {
    spec: &'static Spec,
    errors: Vec<String>,
    year: String,
}

impl AddPage {
    /// "an advent calendar", "a book advent calendar".
    fn a_name(&self) -> String {
        let name = self.spec.name.to_lowercase();
        let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        format!("{article} {name}")
    }
}

/// `GET <base>/add`: the year of the new calendar (suggested: the year after
/// the latest calendar, else this year).
pub async fn add_page<C: Calendar>(State(state): State<AppState>) -> AppResult<AddPage> {
    let latest: Option<i64> =
        sqlx::query_scalar(&format!("SELECT max(year) FROM {}", C::SPEC.table))
            .fetch_one(&state.pool)
            .await?;
    let year = latest.map_or_else(|| i64::from(chrono::Local::now().year()), |y| y + 1);
    Ok(AddPage {
        spec: C::SPEC,
        errors: Vec::new(),
        year: year.to_string(),
    })
}

/// `POST <base>/add` (field `year`): the new calendar, then its edit page to
/// fill the days.
pub async fn create<C: Calendar>(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let spec = C::SPEC;
    let form = FormData::new(pairs);
    let typed = form.text("year");
    let refuse = |error: String| {
        Ok(AddPage {
            spec,
            errors: vec![error],
            year: typed.clone(),
        }
        .into_response())
    };

    let Some(year) = typed
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|y| (2000..=2100).contains(y))
    else {
        return refuse("The year must be a year like 2026.".into());
    };
    let inserted = sqlx::query(&format!(
        "INSERT OR IGNORE INTO {} (year) VALUES (?)",
        spec.table
    ))
    .bind(year)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if inserted == 0 {
        return refuse(format!(
            "There is already a calendar for {year}: open it from the list to fill its days."
        ));
    }

    tracing::info!(year, calendar = spec.table, "advent calendar created");
    Ok(Redirect::to(&format!("{}/{year}/edit", spec.base)).into_response())
}

async fn current_photo<C: Calendar>(
    pool: &Pool,
    calendar_id: i64,
    day: i64,
) -> sqlx::Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(&format!(
        "SELECT photo FROM {}_entry WHERE calendar_id = ? AND day = ?",
        C::SPEC.table
    ))
    .bind(calendar_id)
    .bind(day)
    .fetch_optional(pool)
    .await?
    .flatten())
}

/// `POST <base>/:year/days/:day` (multipart): the item (field
/// [`Spec::item`]; empty keeps the current one) and/or a photo (`photo`).
pub async fn save_day<C: Calendar>(
    State(state): State<AppState>,
    Path((year, day)): Path<(i64, i64)>,
    multipart: Multipart,
) -> AppResult<Response> {
    let spec = C::SPEC;
    let entry_table = format!("{}_entry", spec.table);
    check_day(day)?;
    let calendar = calendar_id::<C>(&state.pool, year).await?;

    let mut form = DayForm::read(multipart, spec.item).await?;

    let mut errors = Vec::new();
    let current: Option<i64> = sqlx::query_scalar(&format!(
        "SELECT {} FROM {entry_table} WHERE calendar_id = ? AND day = ?",
        spec.item_column
    ))
    .bind(calendar)
    .bind(day)
    .fetch_optional(&state.pool)
    .await?;
    let pick = if form.item.is_empty() {
        if current.is_none() {
            errors.push(format!(
                "Day {day}: choose its {} ({}).",
                spec.item, spec.choose
            ));
        }
        current.map(Pick::Existing)
    } else {
        match C::find(&state, &form.item).await? {
            Ok(pick) => Some(pick),
            Err(why) => {
                errors.push(format!("Day {day}: {why}"));
                None
            }
        }
    };

    let processed = form.photo(day, &mut errors).await?;

    let (Some(pick), true) = (pick, errors.is_empty()) else {
        return Ok(edit_page_with::<C>(&state.pool, year, errors)
            .await?
            .into_response());
    };

    let old_photo = current_photo::<C>(&state.pool, calendar, day).await?;
    let new_photo = match &processed {
        Some(photo) => Some(
            calendar_photos::write(&state.media_dir, spec.photo_folder, year, day, photo).await?,
        ),
        None => None,
    };

    let saved = async {
        let mut tx = begin_write(&state.pool).await?;
        let (item_id, added) = match &pick {
            Pick::Existing(id) => (*id, false),
            Pick::New(new) => (C::add(&mut tx, new).await?, true),
        };
        sqlx::query(&format!(
            "INSERT INTO {entry_table} (calendar_id, day, {column}, photo)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (calendar_id, day) DO UPDATE
                 SET {column} = excluded.{column},
                     photo = coalesce(excluded.photo, {entry_table}.photo)",
            column = spec.item_column
        ))
        .bind(calendar)
        .bind(day)
        .bind(item_id)
        .bind(new_photo.as_ref().map(|p| &p.relative))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>((item_id, added))
    }
    .await;
    let (item_id, added) = match saved {
        Ok(saved) => saved,
        Err(e) => {
            if let Some(new_photo) = &new_photo {
                calendar_photos::remove(&new_photo.files).await;
            }
            return Err(e.into());
        }
    };
    if let (Some(old), Some(_)) = (&old_photo, &new_photo) {
        calendar_photos::remove(&calendar_photos::files(&state.media_dir, old)).await;
    }

    tracing::info!(
        calendar = spec.table,
        year,
        day,
        item_id,
        added,
        new_photo = new_photo.is_some(),
        "advent day saved"
    );
    Ok(back_to(spec.base, year, day))
}

/// `POST <base>/:year/days/:day/rotate` (field `direction`: `left` or
/// `right`): turn the day's photo a quarter turn. The original stays as
/// uploaded.
pub async fn rotate<C: Calendar>(
    State(state): State<AppState>,
    Path((year, day)): Path<(i64, i64)>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let spec = C::SPEC;
    check_day(day)?;
    let calendar = calendar_id::<C>(&state.pool, year).await?;
    let clockwise = FormData::new(pairs).get("direction") == Some("right");
    let old = current_photo::<C>(&state.pool, calendar, day)
        .await?
        .or_404()?;

    let new = calendar_photos::rotate(
        &state.media_dir,
        spec.photo_folder,
        year,
        day,
        &old,
        clockwise,
    )
    .await?;
    sqlx::query(&format!(
        "UPDATE {}_entry SET photo = ? WHERE calendar_id = ? AND day = ?",
        spec.table
    ))
    .bind(&new)
    .bind(calendar)
    .bind(day)
    .execute(&state.pool)
    .await?;
    calendar_photos::remove(&[state.media_dir.join(&old)]).await;

    tracing::info!(
        calendar = spec.table,
        year,
        day,
        clockwise,
        "advent photo rotated"
    );
    Ok(back_to(spec.base, year, day))
}

/// `POST <base>/:year/days/:day/delete`: empty the day (its photo files are
/// deleted; the item stays in the collection).
pub async fn remove_day<C: Calendar>(
    State(state): State<AppState>,
    Path((year, day)): Path<(i64, i64)>,
) -> AppResult<Response> {
    let spec = C::SPEC;
    check_day(day)?;
    let calendar = calendar_id::<C>(&state.pool, year).await?;
    let photo = current_photo::<C>(&state.pool, calendar, day).await?;

    sqlx::query(&format!(
        "DELETE FROM {}_entry WHERE calendar_id = ? AND day = ?",
        spec.table
    ))
    .bind(calendar)
    .bind(day)
    .execute(&state.pool)
    .await?;
    if let Some(photo) = photo {
        calendar_photos::remove(&calendar_photos::files(&state.media_dir, &photo)).await;
    }

    tracing::info!(calendar = spec.table, year, day, "advent day emptied");
    Ok(back_to(spec.base, year, day))
}
