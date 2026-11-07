//! Load, save, delete and copy a deck version, and build it.
//!
//! A built version (`is_built`, `yugioh_domain::built_deck`) locks its real cards:
//! building one turns its cards shared with other versions into proxies
//! there, saving zones with a card locked by another built version is
//! refused, saving a built version's zones takes its new cards from the
//! other versions (they get proxies, like building), and a copy gets proxies
//! for the locked cards.

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::QueryBuilder;
use yugioh_domain::built_deck::plan_build;
use yugioh_domain::ydke::YdkeDeck;

use super::tournament::Tournament;
use super::{load_cards, ApiCard, Created};
use crate::routes::cards::card_types;
use crate::routes::decks::built::version_wants_list;
use crate::store::cards::{
    delete_orphan_proxies, locked_elsewhere, proxy_copy, swap_card, Zone, IN_VERSION,
};
use crate::store::db::push_ids;
use crate::store::db::{begin_write, exists, row_exists, Pool};
use crate::store::errata::{self, CardRef, Context, MissingCard, Verdicts};
use crate::store::formats;
use crate::store::snapshots;
use crate::web::error::{bad_request, non_blank, required, AppResult, OrNotFound};
use crate::AppState;
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Ydke {
    with_proxies: String,
    without_proxies: String,
    only_proxies: String,
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
    /// Shown on the public site by hand (a version played in a tournament is
    /// shown anyway).
    is_public: bool,
    deck_box: Option<String>,
    sleeves: Option<String>,
    /// What building the version did, only in the answer to building it or
    /// to saving a built version's zones when cards were taken elsewhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    build_report: Option<BuildReport>,
    tournament: Option<Tournament>,
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

/// Lines for the deck builder: `"Raigeki (in Deck X - v2)"`,
/// `"Ash Blossom & Joyous Spring → Deck Y - v1, Deck Z - test"`.
#[derive(Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct BuildReport {
    proxied_here: Vec<String>,
    proxied_elsewhere: Vec<String>,
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
        with_proxies: url(&|_| true),
        without_proxies: url(&|c| !c.is_proxy),
        only_proxies: url(&|c| c.is_proxy),
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
                deck_version.is_built, deck_version.is_public,
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

    let main_ids = Zone::Main.card_ids(&mut conn, id).await?;
    let extra_ids = Zone::Extra.card_ids(&mut conn, id).await?;
    let side_ids = Zone::Side.card_ids(&mut conn, id).await?;
    let main = load_cards(&mut conn, &main_ids).await?;
    let extra = load_cards(&mut conn, &extra_ids).await?;
    let side = load_cards(&mut conn, &side_ids).await?;

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
        is_public: head.is_public,
        deck_box: head.deck_box,
        sleeves: head.sleeves,
        build_report: None,
        tournament,
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
    main: Option<Vec<i64>>,
    extra: Option<Vec<i64>>,
    side: Option<Vec<i64>>,
    #[serde(default, deserialize_with = "present")]
    ban_list_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "present")]
    format_id: Option<Option<i64>>,
    is_built: Option<bool>,
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

/// The cards of `card_ids` locked by a built version other than
/// `version_id`: (card id, card name, "Deck - version" holding it).
async fn locked_holders(
    conn: &mut sqlx::SqliteConnection,
    version_id: i64,
    card_ids: &[i64],
) -> sqlx::Result<Vec<(i64, String, String)>> {
    if card_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut qb = QueryBuilder::new(
        "SELECT card.id, card.name,
                coalesce(deck.name, 'No Deck') || ' - ' || deck_version.version_name
         FROM (SELECT deck_version_id, card_id FROM deck_version_main_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_extra_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_side_deck) AS zone
         JOIN deck_version ON deck_version.id = zone.deck_version_id
         LEFT JOIN deck ON deck.id = deck_version.deck_id
         JOIN card ON card.id = zone.card_id
         WHERE deck_version.is_built = 1 AND card.is_proxy = 0 AND deck_version.id != ",
    );
    qb.push_bind(version_id).push(" AND card.id IN ");
    push_ids(&mut qb, card_ids.iter().copied());
    qb.push(" ORDER BY card.name, card.id");
    qb.build_query_as().fetch_all(&mut *conn).await
}

/// "A", "A and B", "A, B and C".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The saving error for cards locked by other built versions.
fn locked_message(holders: &[(i64, String, String)]) -> String {
    let names: Vec<String> = holders.iter().map(|(_, name, _)| name.clone()).collect();
    let (are, they, them) = if holders.len() == 1 {
        ("is", "it is", "it")
    } else {
        ("are", "they are", "them")
    };
    let places = if holders.len() == 1 {
        holders[0].2.clone()
    } else {
        holders
            .iter()
            .map(|(_, name, version)| format!("{name}: {version}"))
            .collect::<Vec<_>>()
            .join("; ")
    };
    format!(
        "{} {are} already in a built deck ({places}), so {they} no longer in your \
         collection. Reload the page: {} will be gone from the reserve; add proxies \
         instead if this deck needs {them}.",
        and_list(&names),
        if holders.len() == 1 { "it" } else { "they" },
    )
}

/// Build version `id` (in `tx`): `plan_build`, then the proxies, then the
/// flag. Returns what was done, for the deck builder. Also run again when a
/// built version's zones are saved: only its new shared cards change then.
async fn build(conn: &mut sqlx::SqliteConnection, id: i64) -> AppResult<BuildReport> {
    let mut in_zones = Vec::new();
    for zone in Zone::ALL {
        in_zones.extend(zone.card_ids(conn, id).await?);
    }
    let real: HashSet<i64> = if in_zones.is_empty() {
        HashSet::new()
    } else {
        let mut qb = QueryBuilder::new("SELECT id FROM card WHERE is_proxy = 0 AND id IN ");
        push_ids(&mut qb, in_zones.iter().copied());
        qb.build_query_scalar()
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect()
    };
    let cards: Vec<i64> = in_zones.into_iter().filter(|c| real.contains(c)).collect();

    let locked: HashSet<i64> = sqlx::query_scalar(&locked_elsewhere(id))
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .collect();
    let mut used_elsewhere: HashMap<i64, Vec<i64>> = HashMap::new();
    if !cards.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT card_id, deck_version_id
             FROM (SELECT deck_version_id, card_id FROM deck_version_main_deck
                   UNION ALL SELECT deck_version_id, card_id FROM deck_version_extra_deck
                   UNION ALL SELECT deck_version_id, card_id FROM deck_version_side_deck)
             WHERE deck_version_id != ",
        );
        qb.push_bind(id).push(" AND card_id IN ");
        push_ids(&mut qb, cards.iter().copied());
        let rows: Vec<(i64, i64)> = qb.build_query_as().fetch_all(&mut *conn).await?;
        for (card, version) in rows {
            used_elsewhere.entry(card).or_default().push(version);
        }
    }

    let plan = plan_build(&cards, &locked, &used_elsewhere);

    // the report's names, read before the swaps
    let mut report = BuildReport::default();
    for (_, name, version) in locked_holders(conn, id, &plan.proxied_here).await? {
        report.proxied_here.push(format!("{name} (in {version})"));
    }
    let mut elsewhere: Vec<(i64, Vec<String>)> = Vec::new();
    for (card, version) in &plan.proxied_elsewhere {
        let version_name: String = sqlx::query_scalar(
            "SELECT coalesce(deck.name, 'No Deck') || ' - ' || deck_version.version_name
             FROM deck_version LEFT JOIN deck ON deck.id = deck_version.deck_id
             WHERE deck_version.id = ?",
        )
        .bind(version)
        .fetch_one(&mut *conn)
        .await?;
        match elsewhere.last_mut() {
            Some((last, versions)) if last == card => versions.push(version_name),
            _ => elsewhere.push((*card, vec![version_name])),
        }
    }
    for (card, versions) in elsewhere {
        let name: String = sqlx::query_scalar("SELECT name FROM card WHERE id = ?")
            .bind(card)
            .fetch_one(&mut *conn)
            .await?;
        report
            .proxied_elsewhere
            .push(format!("{name} → {}", versions.join(", ")));
    }

    for card in &plan.proxied_here {
        let proxy = proxy_copy(conn, *card).await?;
        swap_card(conn, id, *card, proxy).await?;
    }
    for (card, version) in &plan.proxied_elsewhere {
        let proxy = proxy_copy(conn, *card).await?;
        swap_card(conn, *version, *card, proxy).await?;
    }
    sqlx::query("UPDATE deck_version SET is_built = 1 WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;

    tracing::info!(
        version_id = id,
        proxied_here = plan.proxied_here.len(),
        proxied_elsewhere = plan.proxied_elsewhere.len(),
        "deck version built"
    );
    Ok(report)
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

    let zones = [
        (Zone::Main, &update.main),
        (Zone::Extra, &update.extra),
        (Zone::Side, &update.side),
    ];
    let mut zones_changed = false;
    for (zone, ids) in zones {
        let Some(ids) = ids else { continue };
        zones_changed = true;
        let mut seen = HashSet::new();
        let ids: Vec<i64> = ids.iter().copied().filter(|i| seen.insert(*i)).collect();
        if !ids.is_empty() {
            let mut qb = QueryBuilder::new("SELECT count(*) FROM card WHERE id IN ");
            push_ids(&mut qb, ids.iter().copied());
            let found: i64 = qb.build_query_scalar().fetch_one(&mut *tx).await?;
            if found != ids.len() as i64 {
                return bad_request("Some cards don't exist any more, reload the page.");
            }

            let holders = locked_holders(&mut tx, id, &ids).await?;
            if !holders.is_empty() {
                return bad_request(locked_message(&holders));
            }
        }
        sqlx::query(&format!(
            "DELETE FROM {} WHERE deck_version_id = ?",
            zone.table()
        ))
        .bind(id)
        .execute(&mut *tx)
        .await?;
        for card_id in ids {
            zone.insert(&mut tx, id, card_id).await?;
        }
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

    let mut proxies_deleted = 0;
    if zones_changed {
        sqlx::query(&format!(
            "DELETE FROM card_category_assignment
             WHERE category_id IN (SELECT id FROM card_category WHERE deck_version_id = ?1)
               AND card_id NOT IN ({IN_VERSION})"
        ))
        .bind(id)
        .execute(&mut *tx)
        .await?;
        // a proxy taken out of the deck and in no other one is not kept
        proxies_deleted = delete_orphan_proxies(&mut tx).await?;
    }

    // building comes after the zones: it works on the saved cards
    let was_built: bool = sqlx::query_scalar("SELECT is_built FROM deck_version WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let mut build_report = None;
    match update.is_built {
        Some(true) if !was_built => build_report = Some(build(&mut tx, id).await?),
        Some(false) if was_built => {
            // the proxies made in other versions stay: clean-cards swaps them back
            sqlx::query("UPDATE deck_version SET is_built = 0 WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tracing::info!(version_id = id, "deck version unbuilt");
        }
        // a built version saved with new cards takes them from the other
        // versions too, like building did (locked ones were refused above)
        None | Some(true) if was_built && zones_changed => {
            let report = build(&mut tx, id).await?;
            if !report.proxied_elsewhere.is_empty() {
                build_report = Some(report);
            }
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
        proxies_deleted,
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
    sqlx::query("DELETE FROM deck_version WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let proxies_deleted = delete_orphan_proxies(&mut tx).await?;
    tx.commit().await?;
    tracing::info!(
        version_id = id,
        ?deck_id,
        proxies_deleted,
        "deck version deleted"
    );
    Ok(Json(Deleted { deck_id }))
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct NameInput {
    name: String,
}

/// New version of the same deck with the same cards: real cards are shared,
/// proxy cards are duplicated. The copy is not built, and the cards locked by
/// a built version (the original's too) become proxies in it.
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

    let locked: HashSet<i64> = sqlx::query_scalar(&locked_elsewhere(new_id))
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .collect();
    for zone in Zone::ALL {
        for card_id in zone.card_ids(&mut tx, id).await? {
            let is_proxy: bool = sqlx::query_scalar("SELECT is_proxy FROM card WHERE id = ?")
                .bind(card_id)
                .fetch_one(&mut *tx)
                .await?;
            let card_id = if is_proxy || locked.contains(&card_id) {
                proxy_copy(&mut tx, card_id).await?
            } else {
                card_id
            };
            zone.insert(&mut tx, new_id, card_id).await?;
        }
    }
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
/// `format` null and no problem without a format. `errata`: the proxies and
/// owned copies whose text was errata'd after the cutoff of the format or
/// the ban list (`store::errata`); `errata_pending`: cards still being
/// looked up on Yugipedia. `wants`: the Cardmarket wants list of the
/// version's proxies, one `count name` per line (empty: none).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Check {
    format: Option<String>,
    problems: Vec<String>,
    errata: Vec<MissingCard>,
    errata_pending: usize,
    wants: String,
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
            let cards: Vec<CardRef> = sqlx::query_as(&format!(
                "SELECT {}
                 FROM (SELECT card_id FROM deck_version_main_deck WHERE deck_version_id = ?1
                       UNION ALL SELECT card_id FROM deck_version_extra_deck WHERE deck_version_id = ?1
                       UNION ALL SELECT card_id FROM deck_version_side_deck WHERE deck_version_id = ?1) AS zone
                 JOIN card ON card.id = zone.card_id
                 LEFT JOIN card_data ON card_data.id = card.card_data_id
                 ORDER BY card.name",
                CardRef::COLUMNS
            ))
            .bind(id)
            .fetch_all(&state.pool)
            .await?;
            let probes: Vec<(&Context, &CardRef)> = cards.iter().map(|c| (context, c)).collect();
            let verdicts = Verdicts::load(&state, &probes).await?;
            let missing = verdicts.missing(Some(context), cards.iter());
            // only the cards with a warning: a proxy without errata is no news
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
    }))
}
