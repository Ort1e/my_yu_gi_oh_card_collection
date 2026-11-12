//! A deck's history (`deck_snapshot`, `deck_snapshot_card`): what a
//! version's zones held, by card, written each time they are saved
//! ([`record`], the rules in `yugioh_domain::deck_history`). A snapshot
//! belongs to its deck and outlives its version. The important ones are
//! named, or are the list played in a tournament ([`freeze`]).

use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::{FromRow, SqliteConnection};
use ts_rs::TS;

use crate::store::lists;
use yugioh_domain::deck_history::{
    coalesce, DeckZone, LastSnapshot, SaveAction, SnapshotEntry, SnapshotList,
};

/// `created_at`'s text form.
const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// SQL condition on `deck_snapshot`: shown on the public site.
pub const PUBLIC_SNAPSHOT: &str =
    "(deck_snapshot.is_public = 1 OR deck_snapshot.tournament_id IS NOT NULL)";

/// A snapshot as the pages and the deck builder show it.
#[derive(Debug, Clone, FromRow, Serialize, TS)]
#[ts(export)]
pub struct Snapshot {
    pub id: i64,
    pub deck_id: i64,
    /// The version it was taken from; null once that one is deleted.
    pub version_id: Option<i64>,
    pub version_name: Option<String>,
    /// `YYYY-MM-DD HH:MM:SS`, local time.
    pub created_at: String,
    /// Null: an automatic one.
    pub name: Option<String>,
    pub tournament_id: Option<i64>,
    pub tournament_name: Option<String>,
    pub tournament_date: Option<String>,
    pub is_public: bool,
    pub main: i64,
    pub extra: i64,
    pub side: i64,
}

impl Snapshot {
    /// Named, a tournament's, or public: never replaced nor deleted by hand.
    pub fn kept(&self) -> bool {
        self.name.is_some() || self.tournament_id.is_some() || self.is_public
    }

    pub fn shown_publicly(&self) -> bool {
        self.is_public || self.tournament_id.is_some()
    }

    /// Its date, `YYYY-MM-DD`.
    pub fn date(&self) -> &str {
        self.created_at.get(..10).unwrap_or(&self.created_at)
    }

    /// Its time, `HH:MM`.
    pub fn time(&self) -> &str {
        self.created_at.get(11..16).unwrap_or("")
    }

    /// What to call it: its name, else its tournament, else its version.
    pub fn title(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }
        if let Some(t) = &self.tournament_name {
            return t.clone();
        }
        match &self.version_name {
            Some(v) => format!("{v} (saved)"),
            None => "Saved list".to_string(),
        }
    }
}

const SELECT: &str = "SELECT deck_snapshot.id, deck_snapshot.deck_id, deck_snapshot.version_id,
        deck_version.version_name, deck_snapshot.created_at, deck_snapshot.name,
        deck_snapshot.tournament_id, tournament.name AS tournament_name,
        tournament.date AS tournament_date, deck_snapshot.is_public,
        COALESCE((SELECT sum(count) FROM deck_snapshot_card c
                  WHERE c.snapshot_id = deck_snapshot.id AND c.zone = 'main'), 0) AS main,
        COALESCE((SELECT sum(count) FROM deck_snapshot_card c
                  WHERE c.snapshot_id = deck_snapshot.id AND c.zone = 'extra'), 0) AS extra,
        COALESCE((SELECT sum(count) FROM deck_snapshot_card c
                  WHERE c.snapshot_id = deck_snapshot.id AND c.zone = 'side'), 0) AS side
    FROM deck_snapshot
    LEFT JOIN deck_version ON deck_version.id = deck_snapshot.version_id
    LEFT JOIN tournament ON tournament.id = deck_snapshot.tournament_id";

/// One snapshot (with `public_only`, `None` for one not shown publicly).
pub async fn load(
    conn: &mut SqliteConnection,
    id: i64,
    public_only: bool,
) -> sqlx::Result<Option<Snapshot>> {
    let public = if public_only {
        format!(" AND {PUBLIC_SNAPSHOT}")
    } else {
        String::new()
    };
    sqlx::query_as(&format!("{SELECT} WHERE deck_snapshot.id = ?{public}"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}

/// A deck's snapshots, newest first.
pub async fn for_deck(
    conn: &mut SqliteConnection,
    deck_id: i64,
    public_only: bool,
) -> sqlx::Result<Vec<Snapshot>> {
    let public = if public_only {
        format!(" AND {PUBLIC_SNAPSHOT}")
    } else {
        String::new()
    };
    sqlx::query_as(&format!(
        "{SELECT} WHERE deck_snapshot.deck_id = ?{public}
         ORDER BY deck_snapshot.created_at DESC, deck_snapshot.id DESC"
    ))
    .bind(deck_id)
    .fetch_all(&mut *conn)
    .await
}

/// The snapshot linked to a tournament, if any.
pub async fn of_tournament(
    conn: &mut SqliteConnection,
    tournament_id: i64,
) -> sqlx::Result<Option<Snapshot>> {
    sqlx::query_as(&format!("{SELECT} WHERE deck_snapshot.tournament_id = ?"))
        .bind(tournament_id)
        .fetch_optional(&mut *conn)
        .await
}

/// The cards of a snapshot.
pub async fn cards(conn: &mut SqliteConnection, id: i64) -> sqlx::Result<SnapshotList> {
    let rows: Vec<(String, Option<i64>, String, i64)> = sqlx::query_as(
        "SELECT zone, card_data_id, name, count FROM deck_snapshot_card
         WHERE snapshot_id = ? ORDER BY CASE zone WHEN 'main' THEN 0 WHEN 'extra' THEN 1 ELSE 2 END,
                                        position",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(SnapshotList {
        entries: rows
            .into_iter()
            .filter_map(|(zone, card_data_id, name, count)| {
                Some(SnapshotEntry {
                    zone: DeckZone::parse(&zone)?,
                    card_data_id,
                    name,
                    count: u32::try_from(count).ok()?,
                })
            })
            .collect(),
    })
}

/// What a version's zones hold now, by card.
pub async fn version_cards(
    conn: &mut SqliteConnection,
    version_id: i64,
) -> sqlx::Result<SnapshotList> {
    Ok(lists::as_list(&lists::lines(conn, version_id).await?))
}

fn now() -> NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// After a version's zones were saved: a snapshot of them, unless they are
/// the same as its last one, which an unnamed one of the same session gets
/// replaced by (`deck_history::coalesce`). The id written, if any. Call
/// inside the write transaction of the save.
pub async fn record(conn: &mut SqliteConnection, version_id: i64) -> sqlx::Result<Option<i64>> {
    let Some(deck_id) = deck_of(conn, version_id).await? else {
        return Ok(None);
    };
    let cards = version_cards(conn, version_id).await?;
    let now = now();

    let last: Option<Snapshot> = sqlx::query_as(&format!(
        "{SELECT} WHERE deck_snapshot.version_id = ?
         ORDER BY deck_snapshot.created_at DESC, deck_snapshot.id DESC LIMIT 1"
    ))
    .bind(version_id)
    .fetch_optional(&mut *conn)
    .await?;
    let last = match last {
        Some(s) => {
            let created_at = NaiveDateTime::parse_from_str(&s.created_at, TIME_FORMAT)
                .unwrap_or(NaiveDateTime::MIN);
            Some((
                s.id,
                LastSnapshot {
                    created_at,
                    kept: s.kept(),
                    cards: self::cards(conn, s.id).await?,
                },
            ))
        }
        None => None,
    };

    let action = coalesce(last.as_ref().map(|(_, l)| l), &cards, now);
    let id = match (action, last) {
        (SaveAction::Skip, _) => return Ok(None),
        (SaveAction::Replace, Some((id, _))) => {
            sqlx::query("UPDATE deck_snapshot SET created_at = ? WHERE id = ?")
                .bind(now.format(TIME_FORMAT).to_string())
                .bind(id)
                .execute(&mut *conn)
                .await?;
            id
        }
        _ => insert(conn, deck_id, version_id, now).await?,
    };
    write_cards(conn, id, &cards).await?;
    Ok(Some(id))
}

/// "Name this save": `name` on the snapshot of what a version holds now, its
/// last one when it is an automatic one with these cards, else a new one.
/// Its id (`None` without the version or its deck).
pub async fn name_current(
    conn: &mut SqliteConnection,
    version_id: i64,
    name: &str,
) -> sqlx::Result<Option<i64>> {
    let Some(deck_id) = deck_of(conn, version_id).await? else {
        return Ok(None);
    };
    let cards = version_cards(conn, version_id).await?;
    let last: Option<Snapshot> = sqlx::query_as(&format!(
        "{SELECT} WHERE deck_snapshot.version_id = ?
         ORDER BY deck_snapshot.created_at DESC, deck_snapshot.id DESC LIMIT 1"
    ))
    .bind(version_id)
    .fetch_optional(&mut *conn)
    .await?;

    let reusable = match &last {
        Some(s) if !s.kept() => self::cards(conn, s.id).await?.same_cards(&cards),
        _ => false,
    };
    let id = match last {
        Some(s) if reusable => s.id,
        _ => {
            let id = insert(conn, deck_id, version_id, now()).await?;
            write_cards(conn, id, &cards).await?;
            id
        }
    };
    set_name(conn, id, Some(name)).await?;
    Ok(Some(id))
}

/// A new snapshot of what a version holds now, linked to `tournament_id`
/// (the one linked before is unlinked): the list played, frozen. `None`
/// without the version or its deck.
pub async fn freeze(
    conn: &mut SqliteConnection,
    version_id: i64,
    tournament_id: i64,
) -> sqlx::Result<Option<i64>> {
    let Some(deck_id) = deck_of(conn, version_id).await? else {
        return Ok(None);
    };
    unlink_tournament(conn, tournament_id).await?;
    let cards = version_cards(conn, version_id).await?;
    let id = insert(conn, deck_id, version_id, now()).await?;
    write_cards(conn, id, &cards).await?;
    sqlx::query("UPDATE deck_snapshot SET tournament_id = ? WHERE id = ?")
        .bind(tournament_id)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(Some(id))
}

/// No snapshot is that tournament's list any more.
pub async fn unlink_tournament(
    conn: &mut SqliteConnection,
    tournament_id: i64,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE deck_snapshot SET tournament_id = NULL WHERE tournament_id = ?")
        .bind(tournament_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn deck_of(conn: &mut SqliteConnection, version_id: i64) -> sqlx::Result<Option<i64>> {
    Ok(
        sqlx::query_scalar::<_, Option<i64>>("SELECT deck_id FROM deck_version WHERE id = ?")
            .bind(version_id)
            .fetch_optional(&mut *conn)
            .await?
            .flatten(),
    )
}

async fn insert(
    conn: &mut SqliteConnection,
    deck_id: i64,
    version_id: i64,
    at: NaiveDateTime,
) -> sqlx::Result<i64> {
    Ok(
        sqlx::query("INSERT INTO deck_snapshot (deck_id, version_id, created_at) VALUES (?, ?, ?)")
            .bind(deck_id)
            .bind(version_id)
            .bind(at.format(TIME_FORMAT).to_string())
            .execute(&mut *conn)
            .await?
            .last_insert_rowid(),
    )
}

/// Replace the cards of snapshot `id`.
async fn write_cards(
    conn: &mut SqliteConnection,
    id: i64,
    cards: &SnapshotList,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM deck_snapshot_card WHERE snapshot_id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    for zone in DeckZone::ALL {
        for (position, e) in cards.zone(zone).enumerate() {
            sqlx::query(
                "INSERT INTO deck_snapshot_card (snapshot_id, zone, card_data_id, name, count, position)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(zone.as_str())
            .bind(e.card_data_id)
            .bind(&e.name)
            .bind(e.count)
            .bind(position as i64 + 1)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

/// Name a snapshot (`None`: back to an automatic one).
pub async fn set_name(
    conn: &mut SqliteConnection,
    id: i64,
    name: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE deck_snapshot SET name = ? WHERE id = ?")
        .bind(name)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn set_public(conn: &mut SqliteConnection, id: i64, public: bool) -> sqlx::Result<()> {
    sqlx::query("UPDATE deck_snapshot SET is_public = ? WHERE id = ?")
        .bind(public)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Delete a snapshot; `false` when it is kept ([`Snapshot::kept`]) or gone.
pub async fn delete(conn: &mut SqliteConnection, id: i64) -> sqlx::Result<bool> {
    let deleted = sqlx::query(
        "DELETE FROM deck_snapshot WHERE id = ?
           AND name IS NULL AND tournament_id IS NULL AND is_public = 0",
    )
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(deleted > 0)
}
