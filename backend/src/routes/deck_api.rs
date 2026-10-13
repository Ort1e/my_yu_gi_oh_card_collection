//! JSON API of the deck builder (Svelte app in `frontend/`).
//! Django: `rest_api/deck_builder_api.py`, `DeckVersion` model methods.
//!
//! Deliberate differences from Django:
//! - deleting a version only removes orphan proxies that are not in a lot
//!   (Django's signal also deleted proxies bought in lots, with their unites);
//! - saving the zones drops the category assignments of cards that left the
//!   deck (Django needed the `clean_card_categories` command);
//! - YDKE import never reuses sold cards, and one owned copy is used once
//!   across all zones;
//! - clone keeps the ban list as well as the tournament;
//! - conditional categories are created here (Django: in the admin only).

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use domain::card::normalize_card_name;
use domain::monte_carlo::{self, Category, Condition, SimulationResult};
use domain::ydke::YdkeDeck;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use crate::card_data::{get_or_fetch, get_or_fetch_by_ygopro_id};
use crate::cards::{delete_orphan_proxies, new_proxy, Zone, SOLD_EXISTS};
use crate::db::{begin_write, Money, Pool};
use crate::error::{bad_request, AppError, AppResult, OrNotFound};
use crate::routes::cards::{card_types, CardFilter};
use crate::AppState;

// ---------- cards ----------

#[derive(Debug, Serialize)]
pub struct ApiCard {
    id: i64,
    name: String,
    en_name: Option<String>,
    code: Option<String>,
    status: Option<String>,
    is_proxy: bool,
    /// Cheapest price paid for this card (Django's deck price used the min).
    price: Option<Money>,
    card_data_id: Option<i64>,
    card_type: Option<String>,
    ygopro_id: Option<i64>,
    /// ban list id -> "Banned" | "Limited" | "Semi-Limited"
    ban: BTreeMap<i64, String>,
}

#[derive(sqlx::FromRow)]
struct CardRecord {
    id: i64,
    name: String,
    en_name: Option<String>,
    code: Option<String>,
    last_known_status: Option<String>,
    is_proxy: bool,
    card_data_id: Option<i64>,
    card_type: Option<String>,
    ygopro_id: Option<i64>,
}

fn push_ids(qb: &mut QueryBuilder<'_, Sqlite>, ids: impl IntoIterator<Item = i64>) {
    qb.push("(");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    sep.push_unseparated(")");
}

/// The cards with these ids, in the same order (unknown ids are skipped).
async fn load_cards(conn: &mut SqliteConnection, ids: &[i64]) -> AppResult<Vec<ApiCard>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let unique: Vec<i64> = ids
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut qb = QueryBuilder::new(
        "SELECT card.id, card.name, card.en_name, card.code, card.last_known_status,
                card.is_proxy, card.card_data_id, card_data.card_type, card_data.ygopro_id
         FROM card LEFT JOIN card_data ON card_data.id = card.card_data_id WHERE card.id IN ",
    );
    push_ids(&mut qb, unique.iter().copied());
    let records: Vec<CardRecord> = qb.build_query_as().fetch_all(&mut *conn).await?;

    let mut qb = QueryBuilder::new(
        "SELECT card_id, price FROM unite WHERE price IS NOT NULL AND card_id IN ",
    );
    push_ids(&mut qb, unique.iter().copied());
    let mut prices: HashMap<i64, Money> = HashMap::new();
    for (card_id, price) in qb
        .build_query_as::<(i64, Money)>()
        .fetch_all(&mut *conn)
        .await?
    {
        let best = prices.entry(card_id).or_insert(price);
        *best = (*best).min(price);
    }

    let data_ids: HashSet<i64> = records.iter().filter_map(|r| r.card_data_id).collect();
    let mut bans: HashMap<i64, BTreeMap<i64, String>> = HashMap::new();
    if !data_ids.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT card_data_id, ban_list_id, status FROM ban_list_entry WHERE card_data_id IN ",
        );
        push_ids(&mut qb, data_ids);
        for (data_id, ban_list_id, status) in qb
            .build_query_as::<(i64, i64, String)>()
            .fetch_all(&mut *conn)
            .await?
        {
            bans.entry(data_id).or_default().insert(ban_list_id, status);
        }
    }

    let mut by_id: HashMap<i64, ApiCard> = records
        .into_iter()
        .map(|r| {
            let card = ApiCard {
                id: r.id,
                price: prices.get(&r.id).copied(),
                ban: r
                    .card_data_id
                    .and_then(|d| bans.get(&d).cloned())
                    .unwrap_or_default(),
                name: r.name,
                en_name: r.en_name,
                code: r.code,
                status: r.last_known_status,
                is_proxy: r.is_proxy,
                card_data_id: r.card_data_id,
                card_type: r.card_type,
                ygopro_id: r.ygopro_id,
            };
            (card.id, card)
        })
        .collect();
    // keep the requested order; a duplicated id gets a single entry
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

async fn zone_card_ids(
    conn: &mut SqliteConnection,
    version_id: i64,
    zone: Zone,
) -> AppResult<Vec<i64>> {
    Ok(sqlx::query_scalar(&format!(
        "SELECT card_id FROM {} WHERE deck_version_id = ? ORDER BY rowid",
        zone.table()
    ))
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?)
}

// ---------- deck version ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Tournament {
    name: String,
    date: String,
    location: Option<String>,
    notes: Option<String>,
}

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

    let main_ids = zone_card_ids(&mut conn, id, Zone::Main).await?;
    let extra_ids = zone_card_ids(&mut conn, id, Zone::Extra).await?;
    let side_ids = zone_card_ids(&mut conn, id, Zone::Side).await?;
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
        let name = name.trim();
        if name.is_empty() {
            return bad_request("The version name can't be empty.");
        }
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
            sqlx::query(&format!(
                "INSERT INTO {} (deck_version_id, card_id) VALUES (?, ?)",
                zone.table()
            ))
            .bind(id)
            .bind(card_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    if zones_changed {
        sqlx::query(
            "DELETE FROM card_category_assignment
             WHERE category_id IN (SELECT id FROM card_category WHERE deck_version_id = ?1)
               AND card_id NOT IN (
                   SELECT card_id FROM deck_version_main_deck WHERE deck_version_id = ?1
                   UNION SELECT card_id FROM deck_version_extra_deck WHERE deck_version_id = ?1
                   UNION SELECT card_id FROM deck_version_side_deck WHERE deck_version_id = ?1)",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!(
        version_id = id,
        zones_changed,
        renamed,
        ban_list_changed,
        "deck version saved"
    );

    Ok(Json(load_version(&state.pool, id).await?))
}

async fn row_exists(conn: &mut SqliteConnection, table: &str, id: i64) -> AppResult<bool> {
    let found: Option<i64> = sqlx::query_scalar(&format!("SELECT id FROM {table} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(found.is_some())
}

async fn exists(conn: &mut SqliteConnection, table: &str, id: i64) -> AppResult<()> {
    if row_exists(conn, table, id).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
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

#[derive(Debug, Serialize)]
pub struct Created {
    id: i64,
}

/// New version of the same deck with the same cards: real cards are shared,
/// proxy cards are duplicated.
pub async fn clone_version(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<NameInput>,
) -> AppResult<(StatusCode, Json<Created>)> {
    let name = input.name.trim();
    if name.is_empty() {
        return bad_request("The version name can't be empty.");
    }
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
        for card_id in zone_card_ids(&mut tx, id, zone).await? {
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
            sqlx::query(&format!(
                "INSERT INTO {} (deck_version_id, card_id) VALUES (?, ?)",
                zone.table()
            ))
            .bind(new_id)
            .bind(card_id)
            .execute(&mut *tx)
            .await?;
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

// ---------- proxies ----------

#[derive(Debug, Deserialize)]
pub struct ProxyInput {
    name: String,
    zone: Zone,
}

pub async fn add_proxy(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<ProxyInput>,
) -> AppResult<(StatusCode, Json<ApiCard>)> {
    let name = input.name.trim();
    if name.is_empty() {
        return bad_request("The card name can't be empty.");
    }
    exists(&mut *state.pool.acquire().await?, "deck_version", id).await?;
    // network call, before the transaction
    let card_data_id = get_or_fetch(&state.pool, &state.http, name).await?;

    let mut tx = begin_write(&state.pool).await?;
    let card_id = new_proxy(&mut tx, name, card_data_id).await?;
    sqlx::query(&format!(
        "INSERT INTO {} (deck_version_id, card_id) VALUES (?, ?)",
        input.zone.table()
    ))
    .bind(id)
    .bind(card_id)
    .execute(&mut *tx)
    .await?;
    let card = load_cards(&mut tx, &[card_id])
        .await?
        .pop()
        .expect("just inserted");
    tx.commit().await?;
    tracing::info!(version_id = id, card_id, zone = ?input.zone, "proxy added");
    Ok((StatusCode::CREATED, Json(card)))
}

pub async fn delete_proxy(
    State(state): State<AppState>,
    Path((id, card_id)): Path<(i64, i64)>,
) -> AppResult<StatusCode> {
    let mut tx = begin_write(&state.pool).await?;
    let is_proxy: bool = sqlx::query_scalar("SELECT is_proxy FROM card WHERE id = ?")
        .bind(card_id)
        .fetch_optional(&mut *tx)
        .await?
        .or_404()?;
    if !is_proxy {
        return bad_request("Only proxy cards can be deleted here.");
    }
    for zone in Zone::ALL {
        sqlx::query(&format!(
            "DELETE FROM {} WHERE deck_version_id = ? AND card_id = ?",
            zone.table()
        ))
        .bind(id)
        .bind(card_id)
        .execute(&mut *tx)
        .await?;
    }
    delete_orphan_proxies(&mut tx).await?;
    tx.commit().await?;
    tracing::info!(version_id = id, card_id, "proxy removed");
    Ok(StatusCode::NO_CONTENT)
}

// ---------- reserve ----------

#[derive(Debug, Deserialize)]
pub struct ReserveQuery {
    name: Option<String>,
    code: Option<String>,
    card_type: Option<String>,
}

const RESERVE_LIMIT: i64 = 300;

/// Owned (non-proxy, unsold) cards matching the filters and not in this version.
pub async fn reserve(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(query): Json<ReserveQuery>,
) -> AppResult<Json<Vec<ApiCard>>> {
    let mut conn = state.pool.acquire().await?;
    exists(&mut conn, "deck_version", id).await?;
    let filter = CardFilter {
        name: query.name,
        code: query.code,
        card_type: query.card_type,
        proxy: Some("false".into()),
        sold: Some("false".into()),
        status: None,
    };
    let mut qb = QueryBuilder::new("SELECT card.id FROM card");
    filter.push_where(&mut qb);
    qb.push(
        " AND card.id NOT IN (
            SELECT card_id FROM deck_version_main_deck WHERE deck_version_id = ",
    )
    .push_bind(id)
    .push(" UNION SELECT card_id FROM deck_version_extra_deck WHERE deck_version_id = ")
    .push_bind(id)
    .push(" UNION SELECT card_id FROM deck_version_side_deck WHERE deck_version_id = ")
    .push_bind(id)
    .push(") ORDER BY card.en_name, card.id LIMIT ")
    .push_bind(RESERVE_LIMIT);
    let ids: Vec<i64> = qb.build_query_scalar().fetch_all(&mut *conn).await?;
    Ok(Json(load_cards(&mut conn, &ids).await?))
}

// ---------- categories ----------

#[derive(Debug, Serialize)]
pub struct ApiCategory {
    id: i64,
    name: String,
    /// The rule of a conditional category, `None` for a plain one.
    condition: Option<ApiCondition>,
}

/// A card of a conditional category counts, in a hand, as a card of `then`
/// when the hand also has a card of one of the `if_any_of` categories, and
/// as a card of `otherwise` (or of nothing) when it doesn't.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiCondition {
    description: String,
    if_any_of: Vec<i64>,
    then: i64,
    otherwise: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct CategoryRecord {
    id: i64,
    name: String,
    condition_description: Option<String>,
    categorie_true_id: Option<i64>,
    categorie_false_id: Option<i64>,
}

/// The categories of a deck version, with their rules.
async fn load_categories(
    conn: &mut SqliteConnection,
    version_id: i64,
) -> sqlx::Result<Vec<ApiCategory>> {
    let rows: Vec<CategoryRecord> = sqlx::query_as(
        "SELECT id, name, condition_description, categorie_true_id, categorie_false_id
         FROM card_category WHERE deck_version_id = ? ORDER BY id",
    )
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;
    let conditions: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT category_id, condition_category_id FROM card_category_condition
         WHERE category_id IN (SELECT id FROM card_category WHERE deck_version_id = ?)
         ORDER BY condition_category_id",
    )
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ApiCategory {
            id: r.id,
            name: r.name,
            condition: match (r.condition_description, r.categorie_true_id) {
                (Some(description), Some(then)) => Some(ApiCondition {
                    description,
                    if_any_of: conditions
                        .iter()
                        .filter(|(c, _)| *c == r.id)
                        .map(|(_, other)| *other)
                        .collect(),
                    then,
                    otherwise: r.categorie_false_id,
                }),
                _ => None,
            },
        })
        .collect())
}

pub async fn categories(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<ApiCategory>>> {
    let mut conn = state.pool.acquire().await?;
    exists(&mut conn, "deck_version", id).await?;
    Ok(Json(load_categories(&mut conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct CategoryInput {
    name: String,
    /// Present to create a conditional category.
    #[serde(default)]
    condition: Option<ApiCondition>,
}

pub async fn add_category(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<CategoryInput>,
) -> AppResult<(StatusCode, Json<ApiCategory>)> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return bad_request("The category name can't be empty.");
    }
    let mut condition = input.condition;
    if let Some(c) = &mut condition {
        c.description = c.description.trim().to_string();
        if c.description.is_empty() {
            return bad_request("Describe the condition.");
        }
        if c.if_any_of.is_empty() {
            return bad_request("Choose at least one category for the condition.");
        }
        c.if_any_of.sort_unstable();
        c.if_any_of.dedup();
    }

    let mut tx = begin_write(&state.pool).await?;
    exists(&mut tx, "deck_version", id).await?;
    if let Some(c) = &condition {
        // every category of the rule must be one of this version's
        let mut wanted: HashSet<i64> = c.if_any_of.iter().copied().collect();
        wanted.insert(c.then);
        wanted.extend(c.otherwise);
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT count(*) FROM card_category WHERE deck_version_id = ",
        );
        query.push_bind(id).push(" AND id IN (");
        let mut ids = query.separated(", ");
        for category_id in &wanted {
            ids.push_bind(*category_id);
        }
        query.push(")");
        let found: i64 = query.build_query_scalar().fetch_one(&mut *tx).await?;
        if found as usize != wanted.len() {
            return bad_request("The condition uses categories that don't exist in this version.");
        }
    }

    let category_id = sqlx::query(
        "INSERT INTO card_category
             (name, deck_version_id, condition_description, categorie_true_id, categorie_false_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&name)
    .bind(id)
    .bind(condition.as_ref().map(|c| &c.description))
    .bind(condition.as_ref().map(|c| c.then))
    .bind(condition.as_ref().and_then(|c| c.otherwise))
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    for other in condition.iter().flat_map(|c| &c.if_any_of) {
        sqlx::query(
            "INSERT INTO card_category_condition (category_id, condition_category_id)
             VALUES (?, ?)",
        )
        .bind(category_id)
        .bind(other)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!(
        version_id = id,
        category_id,
        conditional = condition.is_some(),
        "category added"
    );

    Ok((
        StatusCode::CREATED,
        Json(ApiCategory {
            id: category_id,
            name,
            condition,
        }),
    ))
}

pub async fn delete_category(
    State(state): State<AppState>,
    Path((id, category_id)): Path<(i64, i64)>,
) -> AppResult<StatusCode> {
    let result = sqlx::query("DELETE FROM card_category WHERE id = ? AND deck_version_id = ?")
        .bind(category_id)
        .bind(id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    tracing::info!(version_id = id, category_id, "category deleted");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CardCategory {
    id: i64,
    name: String,
    assigned: bool,
}

/// The version's categories, flagged with whether this card is in them.
pub async fn card_categories(
    State(state): State<AppState>,
    Path((id, card_id)): Path<(i64, i64)>,
) -> AppResult<Json<Vec<CardCategory>>> {
    Ok(Json(
        sqlx::query_as(
            "SELECT c.id, c.name, EXISTS (
                 SELECT 1 FROM card_category_assignment a
                 WHERE a.category_id = c.id AND a.card_id = ?) AS assigned
             FROM card_category c WHERE c.deck_version_id = ? ORDER BY c.id",
        )
        .bind(card_id)
        .bind(id)
        .fetch_all(&state.pool)
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct AssignInput {
    assigned: bool,
}

pub async fn assign_category(
    State(state): State<AppState>,
    Path((id, card_id, category_id)): Path<(i64, i64, i64)>,
    Json(input): Json<AssignInput>,
) -> AppResult<StatusCode> {
    let mut conn = state.pool.acquire().await?;
    let belongs: Option<i64> =
        sqlx::query_scalar("SELECT id FROM card_category WHERE id = ? AND deck_version_id = ?")
            .bind(category_id)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    belongs.or_404()?;
    exists(&mut conn, "card", card_id).await?;

    let sql = if input.assigned {
        "INSERT INTO card_category_assignment (category_id, card_id) VALUES (?, ?)
         ON CONFLICT DO NOTHING"
    } else {
        "DELETE FROM card_category_assignment WHERE category_id = ? AND card_id = ?"
    };
    sqlx::query(sql)
        .bind(category_id)
        .bind(card_id)
        .execute(&mut *conn)
        .await?;
    tracing::info!(
        version_id = id,
        card_id,
        category_id,
        assigned = input.assigned,
        "card category changed"
    );
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Monte Carlo ----------

#[derive(Debug, Deserialize)]
pub struct SimulationInput {
    hand_size: usize,
    simulations: u32,
}

pub async fn monte_carlo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<SimulationInput>,
) -> AppResult<Json<SimulationResult>> {
    if !(1..=60).contains(&input.hand_size) {
        return bad_request("The hand size must be between 1 and 60.");
    }
    if !(1..=1_000_000).contains(&input.simulations) {
        return bad_request("The number of simulations must be between 1 and 1 000 000.");
    }
    let mut conn = state.pool.acquire().await?;
    exists(&mut conn, "deck_version", id).await?;
    let main = zone_card_ids(&mut conn, id, Zone::Main).await?;

    let categories = load_categories(&mut conn, id).await?;
    let assignments: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT card_id, category_id FROM card_category_assignment
         WHERE category_id IN (SELECT id FROM card_category WHERE deck_version_id = ?)",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);

    let categories: Vec<Category> = categories
        .into_iter()
        .map(|c| Category {
            id: c.id,
            name: c.name,
            condition: c.condition.map(|c| Condition {
                if_any_of: c.if_any_of.into_iter().collect(),
                then: c.then,
                otherwise: c.otherwise,
            }),
        })
        .collect();

    let result = tokio::task::spawn_blocking(move || {
        monte_carlo::simulate(
            &main,
            &categories,
            &assignments,
            input.hand_size,
            input.simulations,
            &mut rand::thread_rng(),
        )
    })
    .await
    .map_err(|e| AppError::Internal(format!("simulation: {e}")))?;
    Ok(Json(result))
}

// ---------- tournament ----------

#[derive(Debug, Deserialize)]
pub struct TournamentInput {
    name: String,
    date: String,
    location: Option<String>,
    notes: Option<String>,
}

/// Create or update the tournament of this version.
pub async fn save_tournament(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<TournamentInput>,
) -> AppResult<Json<Tournament>> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return bad_request("The tournament name is required.");
    }
    let Ok(date) = input.date.trim().parse::<chrono::NaiveDate>() else {
        return bad_request("The tournament date is not a valid date.");
    };
    let blank_to_none =
        |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let tournament = Tournament {
        name,
        date: date.to_string(),
        location: blank_to_none(input.location),
        notes: blank_to_none(input.notes),
    };

    let mut tx = begin_write(&state.pool).await?;
    let tournament_id: Option<i64> =
        sqlx::query_scalar::<_, Option<i64>>("SELECT tournament_id FROM deck_version WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .or_404()?;
    match tournament_id {
        Some(t) => {
            sqlx::query(
                "UPDATE tournament SET name = ?, date = ?, location = ?, notes = ? WHERE id = ?",
            )
            .bind(&tournament.name)
            .bind(&tournament.date)
            .bind(&tournament.location)
            .bind(&tournament.notes)
            .bind(t)
            .execute(&mut *tx)
            .await?;
        }
        None => {
            let t: i64 = sqlx::query(
                "INSERT INTO tournament (name, date, location, notes) VALUES (?, ?, ?, ?)",
            )
            .bind(&tournament.name)
            .bind(&tournament.date)
            .bind(&tournament.location)
            .bind(&tournament.notes)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            sqlx::query("UPDATE deck_version SET tournament_id = ? WHERE id = ?")
                .bind(t)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    tracing::info!(version_id = id, "tournament saved");
    Ok(Json(tournament))
}

// ---------- YDKE import ----------

#[derive(Debug, Deserialize)]
pub struct YdkeInput {
    ydke: String,
    name: Option<String>,
}

/// New version of `deck_id` from a YDKE URL. Each card uses an owned copy
/// not already used by this import when there is one, else a new proxy.
pub async fn import_ydke(
    State(state): State<AppState>,
    Path(deck_id): Path<i64>,
    Json(input): Json<YdkeInput>,
) -> AppResult<(StatusCode, Json<Created>)> {
    let deck = match YdkeDeck::parse(input.ydke.trim()) {
        Ok(d) => d,
        Err(e) => return bad_request(e.to_string()),
    };
    exists(&mut *state.pool.acquire().await?, "deck", deck_id).await?;
    let name = input
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Imported".to_string());

    // Resolve card data first (network), outside the transaction.
    let mut data: HashMap<u32, Option<(i64, String)>> = HashMap::new();
    for &ygopro_id in deck.main.iter().chain(&deck.extra).chain(&deck.side) {
        if let Entry::Vacant(entry) = data.entry(ygopro_id) {
            entry.insert(
                get_or_fetch_by_ygopro_id(&state.pool, &state.http, ygopro_id.into()).await?,
            );
        }
    }

    let mut tx = begin_write(&state.pool).await?;
    let version_id: i64 =
        sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (?, ?)")
            .bind(deck_id)
            .bind(&name)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();

    let mut used: HashSet<i64> = HashSet::new();
    for (zone, ids) in [
        (Zone::Main, &deck.main),
        (Zone::Extra, &deck.extra),
        (Zone::Side, &deck.side),
    ] {
        for ygopro_id in ids {
            let card_id = match &data[ygopro_id] {
                Some((data_id, en_name)) => {
                    let owned: Vec<i64> = sqlx::query_scalar(&format!(
                        "SELECT id FROM card
                         WHERE is_proxy = 0 AND (card_data_id = ? OR en_name = ?)
                           AND NOT {SOLD_EXISTS} ORDER BY id"
                    ))
                    .bind(data_id)
                    .bind(normalize_card_name(en_name))
                    .fetch_all(&mut *tx)
                    .await?;
                    match owned.into_iter().find(|id| !used.contains(id)) {
                        Some(id) => id,
                        None => new_proxy(&mut tx, en_name, Some(*data_id)).await?,
                    }
                }
                None => {
                    let name = format!("Unknown-{ygopro_id}");
                    new_proxy(&mut tx, &name, None).await?
                }
            };
            used.insert(card_id);
            sqlx::query(&format!(
                "INSERT INTO {} (deck_version_id, card_id) VALUES (?, ?)",
                zone.table()
            ))
            .bind(version_id)
            .bind(card_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    tracing::info!(deck_id, version_id, "deck version imported from YDKE");
    Ok((StatusCode::CREATED, Json(Created { id: version_id })))
}
