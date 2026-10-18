//! Load, save, delete and copy a deck version.

use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use domain::ydke::YdkeDeck;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::QueryBuilder;

use super::tournament::Tournament;
use super::{load_cards, push_ids, ApiCard, Created};
use crate::routes::cards::card_types;
use crate::store::cards::{delete_orphan_proxies, Zone, IN_VERSION};
use crate::store::db::{begin_write, exists, row_exists, Pool};
use crate::web::error::{bad_request, required, AppResult, OrNotFound};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct Ydke {
    with_proxies: String,
    without_proxies: String,
    only_proxies: String,
}

#[derive(Debug, Serialize)]
pub struct DeckVersion {
    id: i64,
    deck_id: Option<i64>,
    deck_name: Option<String>,
    version_name: String,
    ban_list_id: Option<i64>,
    tournament: Option<Tournament>,
    main: Vec<ApiCard>,
    extra: Vec<ApiCard>,
    side: Vec<ApiCard>,
    ydke: Ydke,
    /// Other versions of the same deck, for the version switcher.
    versions: Vec<VersionRef>,
    ban_lists: Vec<BanListRef>,
    card_types: Vec<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct VersionRef {
    id: i64,
    version_name: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BanListRef {
    id: i64,
    date: String,
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
    let (deck_id, deck_name, version_name, ban_list_id, tournament_id): (
        Option<i64>,
        Option<String>,
        String,
        Option<i64>,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT deck_version.deck_id, deck.name, deck_version.version_name,
                deck_version.ban_list_id, deck_version.tournament_id
         FROM deck_version LEFT JOIN deck ON deck.id = deck_version.deck_id
         WHERE deck_version.id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .or_404()?;

    let tournament = match tournament_id {
        Some(t) => {
            sqlx::query_as("SELECT name, date, location, notes FROM tournament WHERE id = ?")
                .bind(t)
                .fetch_optional(&mut *conn)
                .await?
        }
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
    drop(conn);

    Ok(DeckVersion {
        id,
        deck_id,
        deck_name,
        version_name,
        ban_list_id,
        tournament,
        ydke: ydke(&main, &extra, &side),
        main,
        extra,
        side,
        versions,
        ban_lists,
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

#[derive(Debug, Deserialize)]
pub struct VersionUpdate {
    version_name: Option<String>,
    main: Option<Vec<i64>>,
    extra: Option<Vec<i64>>,
    side: Option<Vec<i64>>,
    #[serde(default, deserialize_with = "present")]
    ban_list_id: Option<Option<i64>>,
}

pub async fn update_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(update): Json<VersionUpdate>,
) -> AppResult<Json<DeckVersion>> {
    let renamed = update.version_name.is_some();
    let ban_list_changed = update.ban_list_id.is_some();
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
    tx.commit().await?;
    tracing::info!(
        version_id = id,
        zones_changed,
        proxies_deleted,
        renamed,
        ban_list_changed,
        "deck version saved"
    );

    Ok(Json(load_version(&state.pool, id).await?))
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Deserialize)]
pub struct NameInput {
    name: String,
}

/// New version of the same deck with the same cards: real cards are shared,
/// proxy cards are duplicated.
pub async fn clone_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<NameInput>,
) -> AppResult<(StatusCode, Json<Created>)> {
    let name = required(&input.name, "The version name can't be empty.")?;
    let mut tx = begin_write(&state.pool).await?;
    exists(&mut tx, "deck_version", id).await?;
    let new_id: i64 = sqlx::query(
        "INSERT INTO deck_version (deck_id, version_name, tournament_id, ban_list_id)
         SELECT deck_id, ?, tournament_id, ban_list_id FROM deck_version WHERE id = ?
        ",
    )
    .bind(name)
    .bind(id)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();

    for zone in Zone::ALL {
        for card_id in zone.card_ids(&mut tx, id).await? {
            let is_proxy: bool = sqlx::query_scalar("SELECT is_proxy FROM card WHERE id = ?")
                .bind(card_id)
                .fetch_one(&mut *tx)
                .await?;
            let card_id = if is_proxy {
                sqlx::query(
                    "INSERT INTO card (name, en_name, card_data_id, code, last_known_status, is_proxy)
                     SELECT name, en_name, card_data_id, code, last_known_status, 1
                     FROM card WHERE id = ?",
                )
                .bind(card_id)
                .execute(&mut *tx)
                .await?
        .last_insert_rowid()
            } else {
                card_id
            };
            zone.insert(&mut tx, new_id, card_id).await?;
        }
    }
    tx.commit().await?;
    tracing::info!(
        from_version_id = id,
        version_id = new_id,
        "deck version copied"
    );
    Ok((StatusCode::CREATED, Json(Created { id: new_id })))
}
