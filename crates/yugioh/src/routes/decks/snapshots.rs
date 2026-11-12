//! A deck's history (`store::snapshots`): the History table of the deck's
//! page ([`history`]), a snapshot's page `/deck-snapshots/:id` (its cards,
//! what changed since the previous one of its version or `?vs=` another
//! snapshot or a version's current list) and its writes: public, delete (an automatic one only), and a new version from its cards.
//!
//! In the public copy: the public snapshots only (`PUBLIC_SNAPSHOT`), each
//! compared with the previous public one of its deck.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::Redirect;
use axum::Form;
use serde::Deserialize;
use yugioh_domain::card::sort_name;
use yugioh_domain::deck_history::{diff, summary, Change, DeckZone, SnapshotList};
use yugioh_domain::stats::{kind, CardKind};

use super::PUBLIC_VERSION;
use crate::store::db::{begin_write, Pool};
use crate::store::lists;
use crate::store::models::{picture_url, version_full_name};
use crate::store::snapshots::{self, Snapshot};
use crate::web::error::{bad_request, AppError, AppResult, OrNotFound};
use crate::web::filters;
use crate::web::forms::{FormData, Pairs};
use crate::AppState;
use common::web::tiles::{CardShape, CardTile};

/// One row of a deck's History table.
pub struct HistoryLine {
    pub snapshot: Snapshot,
    /// Copies added and taken out since the previous snapshot of its
    /// version; `None` for the first one.
    pub changes: Option<(u32, u32)>,
}

impl HistoryLine {
    pub fn changes_text(&self) -> String {
        match self.changes {
            Some((0, 0)) => "same cards".to_string(),
            Some((plus, minus)) => format!("+{plus} −{minus}"),
            None => "first".to_string(),
        }
    }
}

/// The snapshot before `snapshots[i]` (newest first) to compare it with: the
/// previous one of its version (in the public copy, of its deck: they are
/// the public ones only).
fn previous_index(snapshots: &[Snapshot], i: usize, public: bool) -> Option<usize> {
    let s = &snapshots[i];
    snapshots
        .iter()
        .enumerate()
        .skip(i + 1)
        .find(|(_, p)| public || (s.version_id.is_some() && p.version_id == s.version_id))
        .map(|(j, _)| j)
}

/// A deck's snapshots, newest first, each with its changes.
pub async fn history(pool: &Pool, deck_id: i64, public: bool) -> AppResult<Vec<HistoryLine>> {
    let mut conn = pool.acquire().await?;
    let list = snapshots::for_deck(&mut conn, deck_id, public).await?;
    let mut cards = Vec::with_capacity(list.len());
    for s in &list {
        cards.push(snapshots::cards(&mut conn, s.id).await?);
    }
    let changes: Vec<Option<(u32, u32)>> = (0..list.len())
        .map(|i| previous_index(&list, i, public).map(|j| summary(&diff(&cards[j], &cards[i]))))
        .collect();
    Ok(list
        .into_iter()
        .zip(changes)
        .map(|(snapshot, changes)| HistoryLine { snapshot, changes })
        .collect())
}

/// What the page compares with: `vs` = a snapshot id, `version:<id>` (that
/// version's current list); empty: the previous snapshot.
#[derive(Debug, Deserialize, Default)]
pub struct Compare {
    #[serde(default)]
    vs: String,
}

/// One zone of the snapshot.
pub struct ZoneTiles {
    name: &'static str,
    count: u32,
    tiles: Vec<CardTile>,
}

impl ZoneTiles {
    fn tiles(&self) -> &[CardTile] {
        &self.tiles
    }
}

/// A change line: `+2 Ash Blossom` in the Main deck.
pub struct ChangeLine {
    zone: &'static str,
    delta: String,
    added: bool,
    name: String,
    /// `1 → 3`.
    counts: String,
}

impl From<&Change> for ChangeLine {
    fn from(c: &Change) -> ChangeLine {
        ChangeLine {
            zone: c.zone.label(),
            delta: format!("{:+}", c.delta()).replace('-', "−"),
            added: c.added(),
            name: c.name.clone(),
            counts: format!("{} → {}", c.before, c.after),
        }
    }
}

/// A choice of the "Compare with" selector.
pub struct CompareChoice {
    value: String,
    label: String,
    selected: bool,
}

#[derive(Template)]
#[template(path = "decks/snapshot.html")]
pub struct SnapshotPage {
    snapshot: Snapshot,
    deck_name: String,
    zones: Vec<ZoneTiles>,
    /// What it is compared with, `None`: nothing to compare with.
    compared: Option<String>,
    changes: Vec<ChangeLine>,
    choices: Vec<CompareChoice>,
    /// Its version's page can be linked (it exists, and is public in the copy).
    version_shown: bool,
    /// Compared with a version's current list (the changes go from this one).
    vs_version: bool,
    /// The deck builder's History tab to name it: its version's, else
    /// another version of the deck (none left: it can't be renamed).
    rename_href: Option<String>,
}

impl SnapshotPage {
    fn compared_is_version(&self) -> bool {
        self.vs_version
    }

    fn full_title(&self) -> String {
        version_full_name(Some(&self.deck_name), &self.snapshot.title())
    }
}

/// The tiles of a zone: one per card, `×3` when there are more copies;
/// sorted like the version page (kind, then English name).
async fn zone_tiles(pool: &Pool, cards: &SnapshotList, zone: DeckZone) -> AppResult<ZoneTiles> {
    let ids: Vec<i64> = cards.zone(zone).filter_map(|e| e.card_data_id).collect();
    let mut types: HashMap<i64, (Option<String>, String)> = HashMap::new();
    for id in ids {
        if let Some(row) = sqlx::query_as::<_, (Option<String>, String)>(
            "SELECT card_type, en_name FROM card_data WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        {
            types.insert(id, row);
        }
    }

    let mut entries: Vec<((CardKind, String), CardTile)> = cards
        .zone(zone)
        .map(|e| {
            let data = e.card_data_id.and_then(|id| types.get(&id));
            let card_kind = match data {
                Some((Some(card_type), _)) => kind(card_type),
                _ => CardKind::Other,
            };
            let english = data.map(|(_, en)| en.clone());
            let tile = CardTile {
                shape: CardShape::YuGiOh,
                image: picture_url(None, e.card_data_id),
                name: e.name.clone(),
                other_name: english
                    .clone()
                    .filter(|en| *en != e.name)
                    .unwrap_or_default(),
                tags: if e.count > 1 {
                    vec![format!("×{}", e.count)]
                } else {
                    Vec::new()
                },
                ..CardTile::default()
            };
            let key = (card_kind, sort_name(english.as_deref().unwrap_or(&e.name)));
            (key, tile)
        })
        .collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));

    Ok(ZoneTiles {
        name: zone.label(),
        count: cards.zone_count(zone),
        tiles: entries.into_iter().map(|(_, t)| t).collect(),
    })
}

pub async fn page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(compare): Query<Compare>,
) -> AppResult<SnapshotPage> {
    let pool = &state.pool;
    let public = common::export::public_copy();
    let mut conn = pool.acquire().await?;
    let snapshot = snapshots::load(&mut conn, id, public).await?.or_404()?;
    let deck_name: String = sqlx::query_scalar("SELECT name FROM deck WHERE id = ?")
        .bind(snapshot.deck_id)
        .fetch_one(&mut *conn)
        .await?;
    let cards = snapshots::cards(&mut conn, id).await?;

    let others = snapshots::for_deck(&mut conn, snapshot.deck_id, public).await?;
    let versions: Vec<(i64, String)> = if public {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT id, version_name FROM deck_version WHERE deck_id = ?
             ORDER BY version_name DESC",
        )
        .bind(snapshot.deck_id)
        .fetch_all(&mut *conn)
        .await?
    };

    // what to compare with: (label, cards)
    let vs = compare.vs.trim();
    let target: Option<(String, SnapshotList)> = if let Some(v) = vs.strip_prefix("version:") {
        let v: i64 = v.parse().map_err(|_| AppError::NotFound)?;
        let (_, name) = versions.iter().find(|(id, _)| *id == v).or_404()?;
        Some((
            format!("{name} now"),
            snapshots::version_cards(&mut conn, v).await?,
        ))
    } else if !vs.is_empty() {
        let other: i64 = vs.parse().map_err(|_| AppError::NotFound)?;
        let other = others.iter().find(|s| s.id == other).or_404()?;
        Some((
            describe(other),
            snapshots::cards(&mut conn, other.id).await?,
        ))
    } else {
        let i = others.iter().position(|s| s.id == id);
        match i.and_then(|i| previous_index(&others, i, public)) {
            Some(j) => Some((
                describe(&others[j]),
                snapshots::cards(&mut conn, others[j].id).await?,
            )),
            None => None,
        }
    };
    let version_shown = match snapshot.version_id {
        Some(v) if public => {
            sqlx::query_scalar(&format!(
                "SELECT EXISTS (SELECT 1 FROM deck_version WHERE id = ? AND {PUBLIC_VERSION})"
            ))
            .bind(v)
            .fetch_one(&mut *conn)
            .await?
        }
        Some(_) => true,
        None => false,
    };
    drop(conn);

    // the changes from the other list to this one (or from this one to a
    // version's current list: what was changed since)
    let (compared, changes) = match &target {
        Some((label, other)) if vs.starts_with("version:") => {
            (Some(label.clone()), diff(&cards, other))
        }
        Some((label, other)) => (Some(label.clone()), diff(other, &cards)),
        None => (None, Vec::new()),
    };

    let mut zones = Vec::new();
    for zone in DeckZone::ALL {
        zones.push(zone_tiles(pool, &cards, zone).await?);
    }

    let mut choices = vec![CompareChoice {
        value: String::new(),
        label: "The previous one".to_string(),
        selected: vs.is_empty(),
    }];
    choices.extend(versions.iter().map(|(v, name)| {
        let value = format!("version:{v}");
        CompareChoice {
            selected: vs == value,
            value,
            label: format!("{name}, now"),
        }
    }));
    choices.extend(others.iter().filter(|s| s.id != id).map(|s| CompareChoice {
        value: s.id.to_string(),
        label: describe(s),
        selected: vs == s.id.to_string(),
    }));

    let rename_href = snapshot
        .version_id
        .or_else(|| versions.first().map(|(v, _)| *v))
        .map(|v| format!("/deck-builder/{v}#history"));

    Ok(SnapshotPage {
        rename_href,
        snapshot,
        deck_name,
        zones,
        compared,
        changes: changes.iter().map(ChangeLine::from).collect(),
        choices,
        version_shown,
        vs_version: vs.starts_with("version:"),
    })
}

/// `v2 (saved), 07/11/2026 14:05`.
fn describe(s: &Snapshot) -> String {
    format!(
        "{}, {} {}",
        s.title(),
        filters::french_date(s.date()),
        s.time()
    )
}

async fn deck_of(pool: &Pool, id: i64) -> AppResult<i64> {
    sqlx::query_scalar("SELECT deck_id FROM deck_snapshot WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()
}

/// `POST /deck-snapshots/:id/public`: `public` checked or not.
pub async fn set_public(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Redirect> {
    deck_of(&state.pool, id).await?;
    let public = FormData::new(pairs).checked("public");
    let mut tx = begin_write(&state.pool).await?;
    snapshots::set_public(&mut tx, id, public).await?;
    tx.commit().await?;
    tracing::info!(snapshot_id = id, public, "deck snapshot shown or hidden");
    Ok(Redirect::to(&format!("/deck-snapshots/{id}")))
}

/// `POST /deck-snapshots/:id/delete`: an automatic one only.
pub async fn delete(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Redirect> {
    let deck_id = deck_of(&state.pool, id).await?;
    let mut tx = begin_write(&state.pool).await?;
    if !snapshots::delete(&mut tx, id).await? {
        return bad_request("A named, public or tournament list is kept: unname it first.");
    }
    tx.commit().await?;
    tracing::info!(snapshot_id = id, deck_id, "deck snapshot deleted");
    Ok(Redirect::to(&format!("/decks/{deck_id}#history")))
}

/// `POST /deck-snapshots/:id/branch`: a new version `version_name` of its
/// deck with its list, opened in the builder.
pub async fn branch(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Redirect> {
    let deck_id = deck_of(&state.pool, id).await?;
    let form = FormData::new(pairs);
    let name = form.text("version_name");
    let name = name.trim();
    if name.is_empty() {
        return bad_request("The version name can't be empty.");
    }

    let mut tx = begin_write(&state.pool).await?;
    let cards = snapshots::cards(&mut tx, id).await?;
    let version_id: i64 =
        sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (?, ?)")
            .bind(deck_id)
            .bind(name)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
    lists::write_lines(&mut tx, version_id, &cards).await?;
    snapshots::record(&mut tx, version_id).await?;
    tx.commit().await?;
    tracing::info!(
        snapshot_id = id,
        deck_id,
        version_id,
        "deck version made from a snapshot"
    );
    Ok(Redirect::to(&format!("/deck-builder/{version_id}")))
}
