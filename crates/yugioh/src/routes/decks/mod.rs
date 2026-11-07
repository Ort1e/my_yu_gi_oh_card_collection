//! The decks: the list, a deck's page with its versions, a version's cards
//! (`version.rs`), the built decks, the forms; the deck builder's shell
//! page and its JSON API.
//!
//! In the public copy (`common::export::public_copy`), only the versions
//! played in a tournament or shown by hand (`deck_version.is_public`, the
//! button on the version's page), and the decks that have one:
//! [`PUBLIC_VERSION`], [`PUBLIC_DECK`]. The others answer 404 there, so the
//! copy leaves them out.

pub mod api;
pub mod built;
pub mod form;
pub mod version;

use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use sqlx::Row;

use crate::store::db::Pool;
use crate::store::models::version_full_name;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::filters;
use crate::web::pagination::{self, Page, PageParam, PAGE_SIZE};
use crate::AppState;

/// SQL: whether `deck_version` is on the public site.
pub const PUBLIC_VERSION: &str =
    "(deck_version.is_public = 1 OR deck_version.tournament_id IS NOT NULL)";

/// SQL: whether `deck` is on the public site: one of its versions is.
pub const PUBLIC_DECK: &str =
    "EXISTS (SELECT 1 FROM deck_version WHERE deck_version.deck_id = deck.id
     AND (deck_version.is_public = 1 OR deck_version.tournament_id IS NOT NULL))";

#[derive(Debug, sqlx::FromRow)]
pub struct DeckRow {
    id: i64,
    name: String,
    description: Option<String>,
}

/// One deck of the list, with its versions count and last tournament.
#[derive(Debug, sqlx::FromRow)]
pub struct DeckListRow {
    id: i64,
    name: String,
    description: Option<String>,
    versions: i64,
    /// The date of the latest tournament one of its versions was played in.
    last_tournament: Option<String>,
}

/// The list's sort: `newest` (default: the last created deck first) or
/// `tournament` (the latest played in a tournament first, never played at
/// the end, newest first).
#[derive(Debug, Default, Deserialize)]
pub struct DeckSort {
    sort: Option<String>,
}

impl DeckSort {
    fn is_tournament(&self) -> bool {
        self.sort.as_deref() == Some("tournament")
    }

    fn order_by(&self) -> &'static str {
        if self.is_tournament() {
            " ORDER BY last_tournament IS NULL, last_tournament DESC, deck.id DESC"
        } else {
            " ORDER BY deck.id DESC"
        }
    }

    fn querystring(&self) -> String {
        pagination::querystring([("sort", self.sort.as_deref())])
    }
}

#[derive(Template)]
#[template(path = "decks/list.html")]
pub struct DecksList {
    page_title: &'static str,
    page: Page,
    total: i64,
    decks: Vec<DeckListRow>,
    sort: DeckSort,
}

impl DecksList {
    fn sort_is_tournament(&self) -> bool {
        self.sort.is_tournament()
    }
}

pub async fn list(
    State(state): State<AppState>,
    Query(param): Query<PageParam>,
    Query(sort): Query<DeckSort>,
) -> AppResult<DecksList> {
    let pool = &state.pool;
    let (decks_shown, versions_shown) = if common::export::public_copy() {
        (PUBLIC_DECK, PUBLIC_VERSION)
    } else {
        ("1", "1")
    };
    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM deck WHERE {decks_shown}"))
        .fetch_one(pool)
        .await?;
    let page = Page::new(param.page, total, sort.querystring());
    let decks = sqlx::query_as(&format!(
        "SELECT deck.id, deck.name, deck.description,
           (SELECT count(*) FROM deck_version WHERE deck_version.deck_id = deck.id
            AND {versions_shown}) AS versions,
           (SELECT max(t.date) FROM deck_version v JOIN tournament t ON t.id = v.tournament_id
            WHERE v.deck_id = deck.id) AS last_tournament
         FROM deck WHERE {decks_shown}{} LIMIT ? OFFSET ?",
        sort.order_by()
    ))
    .bind(PAGE_SIZE)
    .bind(page.offset())
    .fetch_all(pool)
    .await?;

    Ok(DecksList {
        page_title: "All Decks",
        page,
        total,
        decks,
        sort,
    })
}

/// One row of the versions table of a deck's page.
#[derive(Debug, sqlx::FromRow)]
pub struct VersionLine {
    id: i64,
    version_name: String,
    is_built: bool,
    is_public: bool,
    main_count: i64,
    extra_count: i64,
    side_count: i64,
    /// Cards still missing: the proxies in its zones.
    proxies: i64,
    /// `Edison v2`, when it plays a format.
    format_label: Option<String>,
    /// The date of the ban list it plays (`YYYY-MM-DD`).
    ban_list_date: Option<String>,
    tournament_id: Option<i64>,
    tournament_name: Option<String>,
}

impl VersionLine {
    /// On the public site: shown by hand, or played in a tournament.
    fn on_public_site(&self) -> bool {
        self.is_public || self.tournament_id.is_some()
    }
}

/// How a "new version" starts: the form's `start` field.
#[derive(Debug, PartialEq, Eq)]
pub enum Start {
    Empty,
    /// A copy of this version.
    Copy(i64),
    /// Cards imported from a YDKE.
    Ydke,
}

impl Start {
    /// `empty`, `copy:<version id>` or `ydke`; `None` for anything else.
    pub fn parse(value: &str) -> Option<Start> {
        match value.trim() {
            "empty" => Some(Start::Empty),
            "ydke" => Some(Start::Ydke),
            other => other
                .strip_prefix("copy:")
                .and_then(|id| id.parse().ok())
                .map(Start::Copy),
        }
    }
}

#[derive(Template)]
#[template(path = "decks/detail.html")]
pub struct DeckDetail {
    deck: DeckRow,
    /// Its labels, comma-separated.
    labels: String,
    /// Whether a version of it was played in a tournament.
    has_tournaments: bool,
    /// Its versions (in the public copy, those on the public site).
    versions: Vec<VersionLine>,
    /// What the "new version" form got, to fill it again after an error.
    start: String,
    version_name: String,
    ydke: String,
    /// Errors of the "new version" form.
    errors: Vec<String>,
}

impl DeckDetail {
    /// Whether the form starts from `value` (`empty`, `copy:12`, `ydke`).
    fn starts_with(&self, value: &str) -> bool {
        self.start == value
    }
}

pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<DeckDetail> {
    load_detail(&state.pool, id, Vec::new()).await
}

pub async fn load_detail(pool: &Pool, id: i64, errors: Vec<String>) -> AppResult<DeckDetail> {
    let public = common::export::public_copy();
    let deck = sqlx::query_as(&format!(
        "SELECT id, name, description FROM deck WHERE id = ?{}",
        if public {
            format!(" AND {PUBLIC_DECK}")
        } else {
            String::new()
        }
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .or_404()?;
    let versions = sqlx::query_as(&format!(
        "SELECT deck_version.id, deck_version.version_name, deck_version.is_built,
                deck_version.is_public,
                (SELECT count(*) FROM deck_version_main_deck WHERE deck_version_id = deck_version.id) AS main_count,
                (SELECT count(*) FROM deck_version_extra_deck WHERE deck_version_id = deck_version.id) AS extra_count,
                (SELECT count(*) FROM deck_version_side_deck WHERE deck_version_id = deck_version.id) AS side_count,
                (SELECT count(*) FROM (
                    SELECT card_id FROM deck_version_main_deck WHERE deck_version_id = deck_version.id
                    UNION ALL SELECT card_id FROM deck_version_extra_deck WHERE deck_version_id = deck_version.id
                    UNION ALL SELECT card_id FROM deck_version_side_deck WHERE deck_version_id = deck_version.id
                 ) AS zone JOIN card ON card.id = zone.card_id WHERE card.is_proxy = 1) AS proxies,
                (SELECT name || ' v' || version FROM deck_format
                 WHERE deck_format.id = deck_version.format_id) AS format_label,
                (SELECT date FROM advanced_ban_list
                 WHERE advanced_ban_list.id = deck_version.ban_list_id) AS ban_list_date,
                deck_version.tournament_id, tournament.name AS tournament_name
         FROM deck_version LEFT JOIN tournament ON tournament.id = deck_version.tournament_id
         WHERE deck_version.deck_id = ?{} ORDER BY deck_version.version_name DESC",
        if public {
            format!(" AND {PUBLIC_VERSION}")
        } else {
            String::new()
        }
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;

    let labels = crate::routes::tournaments::deck_labels(pool, id)
        .await?
        .join(", ");
    let has_tournaments = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM deck_version WHERE deck_id = ? AND tournament_id IS NOT NULL)",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(DeckDetail {
        deck,
        labels,
        has_tournaments,
        versions,
        start: String::from("empty"),
        version_name: String::new(),
        ydke: String::new(),
        errors,
    })
}

/// Shell page of the deck builder: the Svelte app (built from `frontend/`)
/// mounts on `#app` and talks to `/api/deck-versions/:id`.
#[derive(Template)]
#[template(path = "decks/builder.html")]
pub struct DeckBuilder {
    id: i64,
    title: String,
}

pub async fn builder(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<DeckBuilder> {
    let row = sqlx::query(
        "SELECT deck.name, deck_version.version_name
         FROM deck_version LEFT JOIN deck ON deck.id = deck_version.deck_id
         WHERE deck_version.id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .or_404()?;
    let deck: Option<String> = row.get(0);
    let version: String = row.get(1);
    Ok(DeckBuilder {
        id,
        title: version_full_name(deck.as_deref(), &version),
    })
}
