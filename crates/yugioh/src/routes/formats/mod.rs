//! `/formats`: the formats (Edison, Goat…) a deck version can play instead
//! of a Konami ban list: their list, and a format's read page with its own
//! list of forbidden and limited cards (`store::formats`, the rules:
//! `yugioh_domain::format`). `form.rs` creates and changes them, on
//! `/formats/:id/edit` (the same data, `FormatEditPage`).

pub mod form;

use std::collections::HashMap;
use std::ops::Deref;

use askama::Template;
use axum::extract::{Path, State};

use crate::store::db::Pool;
use crate::store::formats::{first_releases, FormatRow};
use crate::web::error::{AppResult, OrNotFound};
use crate::web::filters;
use crate::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct FormatListRow {
    id: i64,
    name: String,
    version: i64,
    cutoff: String,
    /// The Konami list it started from.
    based_on: Option<String>,
    /// Cards on its list.
    cards: i64,
    /// Deck versions playing it.
    used_by: i64,
}

#[derive(Template)]
#[template(path = "formats/list.html")]
pub struct FormatsPage {
    formats: Vec<FormatListRow>,
}

/// `GET /formats`: newest version of each name first.
pub async fn list(State(state): State<AppState>) -> AppResult<FormatsPage> {
    let formats = sqlx::query_as(
        "SELECT f.id, f.name, f.version, f.cutoff, b.date AS based_on,
                (SELECT count(*) FROM deck_format_entry WHERE format_id = f.id) AS cards,
                (SELECT count(*) FROM deck_version WHERE format_id = f.id) AS used_by
         FROM deck_format f LEFT JOIN advanced_ban_list b ON b.id = f.based_on
         ORDER BY f.name COLLATE NOCASE, f.version DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(FormatsPage { formats })
}

/// A card on a format's list.
pub struct EntryRow {
    card_data_id: i64,
    name: String,
    status: String,
    /// Its first TCG release, `None` when unknown.
    released: Option<String>,
    /// Released after the format's cutoff: out of the pool anyway.
    out_of_pool: bool,
}

impl EntryRow {
    /// For sorting the list by status: forbidden first.
    fn status_rank(&self) -> u8 {
        match self.status.as_str() {
            "Banned" => 0,
            "Limited" => 1,
            _ => 2,
        }
    }

    fn status_is(&self, status: &str) -> bool {
        self.status == status
    }
}

#[derive(Template)]
#[template(path = "formats/detail.html")]
pub struct FormatPage {
    format: FormatRow,
    based_on: Option<String>,
    errors: Vec<String>,
    entries: Vec<EntryRow>,
    /// The deck versions playing it: `(id, "Deck - version")`.
    used_by: Vec<(i64, String)>,
    /// Every version of this name: `(id, version, this one)`, newest first.
    versions: Vec<(i64, i64, bool)>,
    /// The card names known, for the "add a card" field (the edit page's).
    card_names: Vec<String>,
}

/// `/formats/:id/edit`: the read page's data, its fields read through
/// `Deref` by the template.
#[derive(Template)]
#[template(path = "formats/edit.html")]
pub struct FormatEditPage(FormatPage);

impl Deref for FormatEditPage {
    type Target = FormatPage;

    fn deref(&self) -> &FormatPage {
        &self.0
    }
}

impl FormatPage {
    fn notes(&self) -> &str {
        self.format.notes.as_deref().unwrap_or("")
    }

    fn statuses(&self) -> [&'static str; 3] {
        ["Banned", "Limited", "Semi-Limited"]
    }
}

/// The edit page of format `id`, with these errors at the top.
pub(crate) async fn edit_page(
    pool: &Pool,
    id: i64,
    errors: Vec<String>,
) -> AppResult<FormatEditPage> {
    Ok(FormatEditPage(format_page(pool, id, errors, true).await?))
}

/// Format `id`'s page data; the card names only for the edit page.
async fn format_page(
    pool: &Pool,
    id: i64,
    errors: Vec<String>,
    edit: bool,
) -> AppResult<FormatPage> {
    let format: FormatRow = sqlx::query_as(&format!("{} WHERE id = ?", FormatRow::SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()?;
    let based_on = match format.based_on {
        Some(list) => {
            sqlx::query_scalar("SELECT date FROM advanced_ban_list WHERE id = ?")
                .bind(list)
                .fetch_optional(pool)
                .await?
        }
        None => None,
    };

    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT e.card_data_id, coalesce(json_extract(d.json_data, '$.name'), d.en_name), e.status
         FROM deck_format_entry e JOIN card_data d ON d.id = e.card_data_id
         WHERE e.format_id = ?",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut conn = pool.acquire().await?;
    let releases: HashMap<i64, _> =
        first_releases(&mut conn, rows.iter().map(|(data, _, _)| *data)).await?;
    drop(conn);
    let cutoff = format.cutoff_date()?;
    let mut entries: Vec<EntryRow> = rows
        .into_iter()
        .map(|(card_data_id, name, status)| {
            let released = releases.get(&card_data_id).copied().flatten();
            EntryRow {
                card_data_id,
                name,
                status,
                out_of_pool: released.is_some_and(|r| r > cutoff),
                released: released.map(|r| r.to_string()),
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        (a.status_rank(), a.name.to_lowercase()).cmp(&(b.status_rank(), b.name.to_lowercase()))
    });

    let used_by = sqlx::query_as(
        "SELECT v.id, coalesce(d.name, 'No Deck') || ' - ' || v.version_name
         FROM deck_version v LEFT JOIN deck d ON d.id = v.deck_id
         WHERE v.format_id = ? ORDER BY d.name, v.version_name",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let versions = sqlx::query_as(
        "SELECT id, version, id = ? FROM deck_format WHERE name = ? ORDER BY version DESC",
    )
    .bind(id)
    .bind(&format.name)
    .fetch_all(pool)
    .await?;
    let card_names = if edit {
        sqlx::query_scalar(
            "SELECT DISTINCT coalesce(json_extract(json_data, '$.name'), en_name) AS name
             FROM card_data ORDER BY name COLLATE NOCASE",
        )
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };

    Ok(FormatPage {
        format,
        based_on,
        errors,
        entries,
        used_by,
        versions,
        card_names,
    })
}

/// `GET /formats/:id`.
pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<FormatPage> {
    format_page(&state.pool, id, Vec::new(), false).await
}

/// `GET /formats/:id/edit`.
pub async fn edit(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<FormatEditPage> {
    edit_page(&state.pool, id, Vec::new()).await
}
