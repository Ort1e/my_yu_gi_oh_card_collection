//! Formats (`deck_format`, migration 0022; the rules: `yugioh_domain::format`):
//! the sets' TCG dates (`ygo_set`, from ygoprodeck's `cardsets.php`, with
//! the weekly prices), the cards' first releases, a format's rules, a deck
//! version's problems, and the writes: a format made from a Konami list, a
//! new version of one.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde_json::Value;
use sqlx::{QueryBuilder, SqliteConnection};
use yugioh_domain::ban_list::BanStatus;
use yugioh_domain::format::{self, DeckCard, FormatRules, Problem};

use super::card_data::{SETS_URL, YGOPRODECK};
use super::cards::Zone;
use super::db::{begin_write, push_ids, Pool};
use crate::web::error::{AppError, AppResult};

/// A format as stored.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FormatRow {
    pub id: i64,
    pub name: String,
    pub version: i64,
    /// `YYYY-MM-DD`.
    pub cutoff: String,
    pub based_on: Option<i64>,
    pub notes: Option<String>,
}

impl FormatRow {
    pub const SELECT: &'static str =
        "SELECT id, name, version, cutoff, based_on, notes FROM deck_format";

    /// `Edison v2`.
    pub fn label(&self) -> String {
        format!("{} v{}", self.name, self.version)
    }

    pub fn cutoff_date(&self) -> AppResult<NaiveDate> {
        self.cutoff.parse().map_err(|_| {
            AppError::Internal(format!("format {}: cutoff {:?}", self.id, self.cutoff))
        })
    }
}

/// The English name a card is counted by: its card data's (`Dark Magician`),
/// else what the copy says.
const CARD_NAME: &str =
    "coalesce(json_extract(card_data.json_data, '$.name'), card.en_name, card.name)";

/// Download every set's TCG date (`cardsets.php`) into `ygo_set`. Network
/// first, then one write transaction. Returns how many sets.
pub async fn refresh_set_dates(pool: &Pool, http: &reqwest::Client) -> AppResult<usize> {
    let internal = |e: reqwest::Error| AppError::Internal(format!("ygoprodeck sets: {e}"));
    YGOPRODECK.wait().await;
    let answer: Value = http
        .get(SETS_URL)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(internal)?
        .json()
        .await
        .map_err(internal)?;
    let sets = format::set_dates(&answer);
    if sets.is_empty() {
        return Err(AppError::Internal(
            "ygoprodeck sets: an empty answer".into(),
        ));
    }

    let mut tx = begin_write(pool).await?;
    sqlx::query("DELETE FROM ygo_set").execute(&mut *tx).await?;
    for (name, code, date) in &sets {
        // a name twice (reprints under one name): the earliest date
        sqlx::query(
            "INSERT INTO ygo_set (name, code, tcg_date) VALUES (?, ?, ?)
             ON CONFLICT (name) DO UPDATE SET tcg_date = min(coalesce(tcg_date, excluded.tcg_date),
                                                          coalesce(excluded.tcg_date, tcg_date))",
        )
        .bind(name)
        .bind(code)
        .bind(date.map(|d| d.to_string()))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!(sets = sets.len(), "set dates refreshed");
    Ok(sets.len())
}

/// Get the sets' dates once when there are none yet (a new install, before
/// the first weekly refresh); a failure is logged: the pages then say that
/// no release is known.
pub async fn ensure_set_dates(pool: &Pool, http: &reqwest::Client) -> AppResult<()> {
    let known: i64 = sqlx::query_scalar("SELECT count(*) FROM ygo_set")
        .fetch_one(pool)
        .await?;
    if known == 0 {
        if let Err(e) = refresh_set_dates(pool, http).await {
            tracing::warn!("{e}");
        }
    }
    Ok(())
}

/// The first TCG release of these card data (`None`: no dated set).
pub async fn first_releases(
    conn: &mut SqliteConnection,
    card_data_ids: impl IntoIterator<Item = i64>,
) -> sqlx::Result<HashMap<i64, Option<NaiveDate>>> {
    let ids: Vec<i64> = card_data_ids.into_iter().collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let dates: Vec<(String, String)> =
        sqlx::query_as("SELECT name, tcg_date FROM ygo_set WHERE tcg_date IS NOT NULL")
            .fetch_all(&mut *conn)
            .await?;
    let dates: HashMap<&str, NaiveDate> = dates
        .iter()
        .filter_map(|(name, date)| Some((name.as_str(), date.parse().ok()?)))
        .collect();

    let mut qb = QueryBuilder::new("SELECT id, json_data FROM card_data WHERE id IN ");
    push_ids(&mut qb, ids);
    let rows: Vec<(i64, String)> = qb.build_query_as().fetch_all(&mut *conn).await?;
    Ok(rows
        .into_iter()
        .map(|(id, json)| {
            let card: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
            let sets = format::card_set_names(&card);
            let sets: Vec<&str> = sets.iter().map(String::as_str).collect();
            (id, format::first_release(&sets, &dates))
        })
        .collect())
}

/// A format and its rules (its list by English card name).
pub async fn rules(
    conn: &mut SqliteConnection,
    format_id: i64,
) -> AppResult<Option<(FormatRow, FormatRules)>> {
    let Some(row): Option<FormatRow> =
        sqlx::query_as(&format!("{} WHERE id = ?", FormatRow::SELECT))
            .bind(format_id)
            .fetch_optional(&mut *conn)
            .await?
    else {
        return Ok(None);
    };

    let entries: Vec<(String, String)> = sqlx::query_as(
        "SELECT coalesce(json_extract(card_data.json_data, '$.name'), card_data.en_name), status
         FROM deck_format_entry JOIN card_data ON card_data.id = deck_format_entry.card_data_id
         WHERE format_id = ?",
    )
    .bind(format_id)
    .fetch_all(&mut *conn)
    .await?;
    let rules = FormatRules {
        cutoff: row.cutoff_date()?,
        statuses: entries
            .into_iter()
            .filter_map(|(name, status)| Some((name, BanStatus::from_code(&status)?)))
            .collect(),
    };
    Ok(Some((row, rules)))
}

/// The problems of a deck version in its format; `None` when it plays no
/// format.
pub async fn version_problems(
    conn: &mut SqliteConnection,
    version_id: i64,
) -> AppResult<Option<(FormatRow, Vec<Problem>)>> {
    let format_id: Option<i64> =
        sqlx::query_scalar("SELECT format_id FROM deck_version WHERE id = ?")
            .bind(version_id)
            .fetch_optional(&mut *conn)
            .await?
            .flatten();
    let Some(format_id) = format_id else {
        return Ok(None);
    };
    let Some((row, rules)) = rules(conn, format_id).await? else {
        return Ok(None);
    };

    let mut card_ids = Vec::new();
    for zone in Zone::ALL {
        card_ids.extend(zone.card_ids(conn, version_id).await?);
    }
    let mut copies: Vec<(String, Option<i64>)> = Vec::new();
    if !card_ids.is_empty() {
        let mut qb = QueryBuilder::new(format!(
            "SELECT card.id, {CARD_NAME}, card.card_data_id
             FROM card LEFT JOIN card_data ON card_data.id = card.card_data_id WHERE card.id IN "
        ));
        push_ids(&mut qb, card_ids.iter().copied());
        let rows: Vec<(i64, String, Option<i64>)> =
            qb.build_query_as().fetch_all(&mut *conn).await?;
        let by_id: HashMap<i64, (String, Option<i64>)> = rows
            .into_iter()
            .map(|(id, name, data)| (id, (name, data)))
            .collect();
        // one entry per copy: a card in two zones counts twice
        copies = card_ids
            .iter()
            .filter_map(|id| by_id.get(id).cloned())
            .collect();
    }

    let releases = first_releases(conn, copies.iter().filter_map(|(_, data)| *data)).await?;
    let deck: Vec<DeckCard> = copies
        .into_iter()
        .map(|(name, data)| DeckCard {
            first_release: data.and_then(|d| releases.get(&d).copied().flatten()),
            name,
        })
        .collect();
    Ok(Some((row, format::check(&deck, &rules))))
}

/// Every format's status of these card data: card data → `"f<format id>"`
/// → status, for the deck builder's cards (next to the ban lists' ids).
pub async fn statuses(
    conn: &mut SqliteConnection,
    card_data_ids: impl IntoIterator<Item = i64>,
) -> sqlx::Result<HashMap<i64, BTreeMap<String, String>>> {
    let ids: Vec<i64> = card_data_ids.into_iter().collect();
    let mut statuses: HashMap<i64, BTreeMap<String, String>> = HashMap::new();
    if ids.is_empty() {
        return Ok(statuses);
    }

    let mut qb = QueryBuilder::new(
        "SELECT card_data_id, format_id, status FROM deck_format_entry WHERE card_data_id IN ",
    );
    push_ids(&mut qb, ids);
    let rows: Vec<(i64, i64, String)> = qb.build_query_as().fetch_all(&mut *conn).await?;
    for (data, format, status) in rows {
        statuses
            .entry(data)
            .or_default()
            .insert(format!("f{format}"), status);
    }
    Ok(statuses)
}

/// A new format (version 1), its list copied from the Konami list
/// `based_on` (empty without one). Call inside a write transaction.
pub async fn create(
    conn: &mut SqliteConnection,
    name: &str,
    cutoff: NaiveDate,
    based_on: Option<i64>,
    notes: Option<&str>,
) -> sqlx::Result<i64> {
    let id = sqlx::query(
        "INSERT INTO deck_format (name, version, cutoff, based_on, notes) VALUES (?, 1, ?, ?, ?)",
    )
    .bind(name)
    .bind(cutoff.to_string())
    .bind(based_on)
    .bind(notes)
    .execute(&mut *conn)
    .await?
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO deck_format_entry (format_id, card_data_id, status)
         -- a card twice in a list: the strictest status
         SELECT ?, card_data_id,
                CASE min(CASE status WHEN 'Banned' THEN 0 WHEN 'Limited' THEN 1 ELSE 2 END)
                    WHEN 0 THEN 'Banned' WHEN 1 THEN 'Limited' ELSE 'Semi-Limited' END
         FROM ban_list_entry WHERE ban_list_id = ? GROUP BY card_data_id",
    )
    .bind(id)
    .bind(based_on)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

/// A new version of format `id`: a copy (its list too), numbered after the
/// highest version of that name. Call inside a write transaction.
pub async fn new_version(conn: &mut SqliteConnection, id: i64) -> sqlx::Result<i64> {
    let new_id = sqlx::query(
        "INSERT INTO deck_format (name, version, cutoff, based_on, notes)
         SELECT name, (SELECT max(version) FROM deck_format AS same WHERE same.name = deck_format.name) + 1,
                cutoff, based_on, notes
         FROM deck_format WHERE id = ?",
    )
    .bind(id)
    .execute(&mut *conn)
    .await?
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO deck_format_entry (format_id, card_data_id, status)
         SELECT ?, card_data_id, status FROM deck_format_entry WHERE format_id = ?",
    )
    .bind(new_id)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(new_id)
}
