//! Load, save, delete and copy a deck version, and build it.
//!
//! A version is a list of lines (`store::lists`); only a built version holds
//! copies (`is_built`, `store::lists::assign_copies`): building it gives its
//! lines free copies, best condition first, the rest are missing; saving its
//! lines keeps the copies of the cards still listed and gives the new slots
//! free copies; unbuilding frees them. Other versions are never touched.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Deserializer, Serialize};
use yugioh_domain::deck_history::DeckZone;
use yugioh_domain::ydke::YdkeDeck;

use super::tournament::Tournament;
use super::{input_list, version_tiles, ApiCard, CardInput, Created};
use crate::routes::cards::card_types;
use crate::routes::decks::shopping::{version_wants_list, WANTS_NOTE};
use crate::store::db::{begin_write, exists, row_exists, Money, Pool};
use crate::store::errata::{self, CardRef, Context, MissingCard, Verdicts};
use crate::store::formats;
use crate::store::lists::{self, LISTED_IN_VERSION};
use crate::store::snapshots;
use crate::web::error::{bad_request, non_blank, required, AppResult, OrNotFound};
use crate::AppState;
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Ydke {
    all: String,
    /// The owned cards only.
    owned: String,
    /// The missing cards only.
    missing: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct DeckVersion {
    id: i64,
    deck_id: Option<i64>,
    deck_name: Option<String>,
    version_name: String,
    ban_list_id: Option<i64>,
    /// The format it plays instead of a ban list (never both).
    format_id: Option<i64>,
    is_built: bool,
    /// A plain version the owner means to build: its missing cards are on
    /// the shopping list (`/decks/shopping`).
    is_planned: bool,
    /// Shown on the public site by hand (a version played in a tournament is
    /// shown anyway).
    is_public: bool,
    deck_box: Option<String>,
    sleeves: Option<String>,
    /// What building the version did, only in the answer to building it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    build_report: Option<BuildReport>,
    tournament: Option<Tournament>,
    /// What is left of this month's budget (negative when over it), for the
    /// builder's header; `None` when no budget was ever set.
    #[ts(type = "string | null")]
    budget_left: Option<Money>,
    main: Vec<ApiCard>,
    extra: Vec<ApiCard>,
    side: Vec<ApiCard>,
    ydke: Ydke,
    /// Other versions of the same deck, for the version switcher.
    versions: Vec<VersionRef>,
    ban_lists: Vec<BanListRef>,
    formats: Vec<FormatRef>,
    card_types: Vec<String>,
}

/// What building a version did: copies put in it, slots left missing.
#[derive(Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct BuildReport {
    copies: usize,
    missing: usize,
}

#[derive(sqlx::FromRow)]
struct VersionHead {
    deck_id: Option<i64>,
    deck_name: Option<String>,
    version_name: String,
    ban_list_id: Option<i64>,
    format_id: Option<i64>,
    tournament_id: Option<i64>,
    is_built: bool,
    is_planned: bool,
    is_public: bool,
    deck_box: Option<String>,
    sleeves: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow, TS)]
#[ts(export)]
pub struct VersionRef {
    id: i64,
    version_name: String,
}

#[derive(Debug, Serialize, sqlx::FromRow, TS)]
#[ts(export)]
pub struct BanListRef {
    id: i64,
    date: String,
}

/// A format for the selector: `Edison v2`, and its cutoff (`YYYY-MM-DD`)
/// for the tiles of the cards out of its pool.
#[derive(Debug, Serialize, sqlx::FromRow, TS)]
#[ts(export)]
pub struct FormatRef {
    id: i64,
    label: String,
    cutoff: String,
}

fn ydke(main: &[ApiCard], extra: &[ApiCard], side: &[ApiCard]) -> Ydke {
    let url = |keep: &dyn Fn(&ApiCard) -> bool| {
        let ids = |cards: &[ApiCard]| {
            cards
                .iter()
                .filter(|c| keep(c))
                .filter_map(|c| c.ygopro_id.and_then(|id| u32::try_from(id).ok()))
                .collect()
        };
        YdkeDeck {
            main: ids(main),
            extra: ids(extra),
            side: ids(side),
        }
        .to_url()
    };
    Ydke {
        all: url(&|_| true),
        owned: url(&|c| !c.missing),
        missing: url(&|c| c.missing),
    }
}

async fn load_version(pool: &Pool, id: i64) -> AppResult<DeckVersion> {
    let mut conn = pool.acquire().await?;
    let head: VersionHead = sqlx::query_as(
        "SELECT deck_version.deck_id, deck.name AS deck_name, deck_version.version_name,
                deck_version.ban_list_id, deck_version.format_id,
                (SELECT tournament_id FROM deck_snapshot
                 WHERE version_id = deck_version.id AND tournament_id IS NOT NULL
                 ORDER BY created_at DESC LIMIT 1) AS tournament_id,
                deck_version.is_built, deck_version.is_planned, deck_version.is_public,
                deck_version.deck_box, deck_version.sleeves
         FROM deck_version LEFT JOIN deck ON deck.id = deck_version.deck_id
         WHERE deck_version.id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .or_404()?;
    let (deck_id, tournament_id) = (head.deck_id, head.tournament_id);

    let tournament = match tournament_id {
        Some(t) => super::tournament::load(&mut conn, t).await?,
        None => None,
    };

    let [main, extra, side] = version_tiles(&mut conn, id).await?;

    let versions = sqlx::query_as(
        "SELECT id, version_name FROM deck_version
         WHERE deck_id IS ?1 OR id = ?2 ORDER BY version_name DESC",
    )
    .bind(deck_id)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let ban_lists = sqlx::query_as("SELECT id, date FROM advanced_ban_list ORDER BY date DESC")
        .fetch_all(&mut *conn)
        .await?;
    let formats = sqlx::query_as(
        "SELECT id, name || ' v' || version AS label, cutoff FROM deck_format
         ORDER BY name COLLATE NOCASE, version DESC",
    )
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);

    Ok(DeckVersion {
        id,
        deck_id,
        deck_name: head.deck_name,
        version_name: head.version_name,
        ban_list_id: head.ban_list_id,
        format_id: head.format_id,
        is_built: head.is_built,
        is_planned: head.is_planned,
        is_public: head.is_public,
        deck_box: head.deck_box,
        sleeves: head.sleeves,
        build_report: None,
        tournament,
        budget_left: crate::routes::budget::left_this_month(pool).await?,
        ydke: ydke(&main, &extra, &side),
        main,
        extra,
        side,
        versions,
        ban_lists,
        formats,
        card_types: card_types(pool).await?,
    })
}

pub async fn get_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<DeckVersion>> {
    Ok(Json(load_version(&state.pool, id).await?))
}

/// `Some(None)` when the field is present and `null`, `None` when absent.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, optional_fields)]
pub struct VersionUpdate {
    version_name: Option<String>,
    /// The zones' tiles, one per copy (grouped into lines by card).
    main: Option<Vec<CardInput>>,
    extra: Option<Vec<CardInput>>,
    side: Option<Vec<CardInput>>,
    #[serde(default, deserialize_with = "present")]
    ban_list_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "present")]
    format_id: Option<Option<i64>>,
    is_built: Option<bool>,
    /// Planned (a plain version only; building it clears it).
    is_planned: Option<bool>,
    /// Show the version on the public site, or hide it.
    is_public: Option<bool>,
    /// Present and `null` or blank: cleared.
    #[serde(default, deserialize_with = "present")]
    deck_box: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    sleeves: Option<Option<String>>,
    /// "Name this save": the name of the snapshot of the version's cards
    /// once saved (`store::snapshots::name_current`).
    snapshot_name: Option<String>,
}

/// Build version `id` (in `tx`): its copies (`lists::assign_copies`), then
/// the flag; no longer planned.
async fn build(conn: &mut sqlx::SqliteConnection, id: i64) -> AppResult<BuildReport> {
    let done = lists::assign_copies(conn, id).await?;
    sqlx::query("UPDATE deck_version SET is_built = 1, is_planned = 0 WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    tracing::info!(
        version_id = id,
        copies = done.copies,
        missing = done.missing,
        "deck version built"
    );
    Ok(BuildReport {
        copies: done.copies,
        missing: done.missing,
    })
}

pub async fn update_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(update): Json<VersionUpdate>,
) -> AppResult<Json<DeckVersion>> {
    let renamed = update.version_name.is_some();
    let ban_list_changed = update.ban_list_id.is_some() || update.format_id.is_some();
    let mut tx = begin_write(&state.pool).await?;
    exists(&mut tx, "deck_version", id).await?;

    if let Some(name) = &update.version_name {
        let name = required(name, "The version name can't be empty.")?;
        sqlx::query("UPDATE deck_version SET version_name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    if let Some(ban_list_id) = update.ban_list_id {
        if let Some(b) = ban_list_id {
            if !row_exists(&mut tx, "advanced_ban_list", b).await? {
                return bad_request("Unknown ban list.");
            }
        }
        sqlx::query("UPDATE deck_version SET ban_list_id = ? WHERE id = ?")
            .bind(ban_list_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(format_id) = update.format_id {
        if let Some(f) = format_id {
            if !row_exists(&mut tx, "deck_format", f).await? {
                return bad_request("Unknown format.");
            }
        }
        sqlx::query("UPDATE deck_version SET format_id = ? WHERE id = ?")
            .bind(format_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if ban_list_changed {
        let both: bool = sqlx::query_scalar(
            "SELECT ban_list_id IS NOT NULL AND format_id IS NOT NULL FROM deck_version WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if both {
            return bad_request("A version plays a ban list or a format, not both.");
        }
    }

    let zones_changed = update.main.is_some() || update.extra.is_some() || update.side.is_some();
    if zones_changed {
        // a zone not sent keeps what it holds
        let saved = lists::as_list(&lists::lines(&mut tx, id).await?);
        let kept = |zone: DeckZone| -> Vec<CardInput> {
            saved
                .zone(zone)
                .flat_map(|e| (0..e.count).map(|_| CardInput::new(e.card_data_id, &e.name)))
                .collect()
        };
        let main = update.main.clone().unwrap_or_else(|| kept(DeckZone::Main));
        let extra = update
            .extra
            .clone()
            .unwrap_or_else(|| kept(DeckZone::Extra));
        let side = update.side.clone().unwrap_or_else(|| kept(DeckZone::Side));
        let list = input_list(
            &mut tx,
            [
                (DeckZone::Main, main.as_slice()),
                (DeckZone::Extra, extra.as_slice()),
                (DeckZone::Side, side.as_slice()),
            ],
        )
        .await?;
        lists::write_lines(&mut tx, id, &list).await?;
    }
    if let Some(is_planned) = update.is_planned {
        // a built version is not planned: it is built
        sqlx::query("UPDATE deck_version SET is_planned = (? AND is_built = 0) WHERE id = ?")
            .bind(is_planned)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(is_public) = update.is_public {
        sqlx::query("UPDATE deck_version SET is_public = ? WHERE id = ?")
            .bind(is_public)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    for (column, value) in [("deck_box", &update.deck_box), ("sleeves", &update.sleeves)] {
        if let Some(value) = value {
            sqlx::query(&format!(
                "UPDATE deck_version SET {column} = ? WHERE id = ?"
            ))
            .bind(non_blank(value.clone()))
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
    }

    if zones_changed {
        sqlx::query(&format!(
            "DELETE FROM card_category_assignment
             WHERE category_id IN (SELECT id FROM card_category WHERE deck_version_id = ?1)
               AND card_data_id NOT IN ({LISTED_IN_VERSION})"
        ))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }

    // building comes after the zones: it works on the saved cards
    let was_built: bool = sqlx::query_scalar("SELECT is_built FROM deck_version WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let mut build_report = None;
    match update.is_built {
        Some(true) if !was_built => {
            build_report = Some(build(&mut tx, id).await?);
            // a built deck is on the public site by default (hidden by hand
            // afterwards, it stays hidden: only building turns it on)
            if update.is_public.is_none() {
                sqlx::query("UPDATE deck_version SET is_public = 1 WHERE id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        Some(false) if was_built => {
            let freed = lists::release_copies(&mut tx, id).await?;
            sqlx::query("UPDATE deck_version SET is_built = 0 WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tracing::info!(version_id = id, freed, "deck version unbuilt");
        }
        // a built version's new lines get free copies; the cards still
        // listed keep theirs
        None | Some(true) if was_built && zones_changed => {
            lists::assign_copies(&mut tx, id).await?;
        }
        _ => {}
    }

    // the history: what the zones hold now, by card
    let snapshot_id = if zones_changed {
        snapshots::record(&mut tx, id).await?
    } else {
        None
    };
    if let Some(name) = &update.snapshot_name {
        let name = required(name, "The name can't be empty.")?;
        snapshots::name_current(&mut tx, id, name).await?;
    }
    tx.commit().await?;
    tracing::info!(
        version_id = id,
        zones_changed,
        ?snapshot_id,
        renamed,
        ban_list_changed,
        "deck version saved"
    );

    let mut version = load_version(&state.pool, id).await?;
    version.build_report = build_report;
    Ok(Json(version))
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Deleted {
    deck_id: Option<i64>,
}

pub async fn delete_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Deleted>> {
    let mut tx = begin_write(&state.pool).await?;
    let deck_id: Option<i64> =
        sqlx::query_scalar::<_, Option<i64>>("SELECT deck_id FROM deck_version WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .or_404()?;
    // its lines and copies cascade: the copies are free again
    sqlx::query("DELETE FROM deck_version WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(version_id = id, ?deck_id, "deck version deleted");
    Ok(Json(Deleted { deck_id }))
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct NameInput {
    name: String,
}

/// New version of the same deck with the same lines, a plain list (not
/// built: it holds no copies).
pub async fn clone_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<NameInput>,
) -> AppResult<(StatusCode, Json<Created>)> {
    let new_id = copy_version(&state.pool, id, &input.name).await?;
    Ok((StatusCode::CREATED, Json(Created { id: new_id })))
}

/// The copy itself (also the deck page's "new version" form): the id of the
/// new version `name`.
pub(crate) async fn copy_version(pool: &Pool, id: i64, name: &str) -> AppResult<i64> {
    let name = required(name, "The version name can't be empty.")?;
    let mut tx = begin_write(pool).await?;
    exists(&mut tx, "deck_version", id).await?;
    let new_id: i64 = sqlx::query(
        "INSERT INTO deck_version (deck_id, version_name, ban_list_id, format_id)
         SELECT deck_id, ?, ban_list_id, format_id FROM deck_version WHERE id = ?
        ",
    )
    .bind(name)
    .bind(id)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();

    sqlx::query(
        "INSERT INTO deck_version_card (deck_version_id, zone, card_data_id, name, count, position)
         SELECT ?, zone, card_data_id, name, count, position FROM deck_version_card
         WHERE deck_version_id = ?",
    )
    .bind(new_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    snapshots::record(&mut tx, new_id).await?;
    tx.commit().await?;
    tracing::info!(
        from_version_id = id,
        version_id = new_id,
        "deck version copied"
    );
    Ok(new_id)
}

/// The answer of [`check_version`]: `{"format": "Edison v2", "problems":
/// ["Raigeki: 2 copies, 1 allowed"], "errata": [...], "errata_pending": 0}`;
/// `format` null and no problem without a format. `errata`: the missing
/// cards and the built version's copies whose text was errata'd after the
/// cutoff of the format or the ban list (`store::errata`); `errata_pending`:
/// cards still being looked up on Yugipedia. `wants`: the Cardmarket wants
/// list of the version's missing cards, one `count name` per line (empty: none); `wants_note`:
/// what to do with it (shared with `/decks/built`).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Check {
    format: Option<String>,
    problems: Vec<String>,
    errata: Vec<MissingCard>,
    errata_pending: usize,
    wants: String,
    wants_note: &'static str,
}

/// `GET /api/deck-versions/:id/check`: its problems in the format it plays,
/// and its cards errata'd since the format's cutoff or the ban list's date.
pub async fn check_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Check>> {
    let mut conn = state.pool.acquire().await?;
    exists(&mut conn, "deck_version", id).await?;
    let (format, problems) = match formats::version_problems(&mut conn, id).await? {
        Some((format, problems)) => (
            Some(format.label()),
            problems.iter().map(|p| p.message()).collect(),
        ),
        None => (None, Vec::new()),
    };
    drop(conn);

    let contexts = errata::contexts(&state.pool, &[id]).await?;
    let (errata, errata_pending) = match contexts.get(&id) {
        Some(context) => {
            let cards: Vec<CardRef> =
                errata::version_cards(&mut *state.pool.acquire().await?, id).await?;
            let probes: Vec<(&Context, &CardRef)> = cards.iter().map(|c| (context, c)).collect();
            let verdicts = Verdicts::load(&state, &probes).await?;
            let missing = verdicts.missing(Some(context), cards.iter());
            // only the cards with a warning: a missing card without errata is no news
            let lines = missing
                .lines
                .into_values()
                .filter(|m| m.warning.is_some())
                .collect();
            (lines, verdicts.pending)
        }
        None => (Vec::new(), 0),
    };
    Ok(Json(Check {
        format,
        problems,
        errata,
        errata_pending,
        wants: version_wants_list(&state.pool, id).await?,
        wants_note: WANTS_NOTE,
    }))
}
