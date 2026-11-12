//! Deck versions as lists (`yugioh_domain::ownership`, migration 0027): the
//! lines of a version (`deck_version_card`), and the copies a **built**
//! version holds (`deck_version_copy`, a `card_id` NULL slot = missing).
//!
//! A copy is held by one built version at most (a unique index); plain
//! versions hold none, so they never touch each other. Whether a line is
//! owned is worked out from the collection ([`availability`]).

use std::collections::{HashMap, HashSet};

use sqlx::{QueryBuilder, SqliteConnection};
use yugioh_domain::built_deck::plan_build;
use yugioh_domain::card::CardStatus;
use yugioh_domain::deck_history::{DeckZone, SnapshotList};
use yugioh_domain::ownership::{self, Availability, CardDataId, Copy, CopyId, Line, LineRef};

use crate::store::cards::SOLD_EXISTS;
use crate::store::db::push_ids;

/// SQL subquery: ids of the cards held by any built version (its copies).
pub const IN_BUILT_DECK: &str = "SELECT card_id FROM deck_version_copy WHERE card_id IS NOT NULL";

/// SQL subquery: ids of the card data listed by the version bound as `?1`.
pub const LISTED_IN_VERSION: &str =
    "SELECT card_data_id FROM deck_version_card WHERE deck_version_id = ?1 AND card_data_id IS NOT NULL";

/// SQL ordering of the zones (`main`, `extra`, `side`) for a `zone` column.
pub const ZONE_ORDER: &str = "CASE zone WHEN 'main' THEN 0 WHEN 'extra' THEN 1 ELSE 2 END";

/// The lines of a version, zone by zone, in their order.
pub async fn lines(conn: &mut SqliteConnection, version_id: i64) -> sqlx::Result<Vec<Line>> {
    let rows: Vec<(String, i64, Option<i64>, String, i64)> = sqlx::query_as(&format!(
        "SELECT zone, position, card_data_id, name, count FROM deck_version_card
         WHERE deck_version_id = ? ORDER BY {ZONE_ORDER}, position"
    ))
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(zone, position, card_data_id, name, count)| {
            Some(Line {
                zone: DeckZone::parse(&zone)?,
                position: u32::try_from(position).ok()?,
                card_data_id,
                name,
                count: u32::try_from(count).ok()?,
            })
        })
        .collect())
}

/// A version's lines as a snapshot's list (the history, the formats).
pub fn as_list(lines: &[Line]) -> SnapshotList {
    SnapshotList::from_cards(
        lines
            .iter()
            .flat_map(|l| (0..l.count).map(move |_| (l.zone, l.card_data_id, l.name.as_str()))),
    )
}

/// Replace the lines of a version by `list` (positions 1, 2… in each zone).
/// A built version's copies go with them: call [`assign_copies`] after.
pub async fn write_lines(
    conn: &mut SqliteConnection,
    version_id: i64,
    list: &SnapshotList,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM deck_version_card WHERE deck_version_id = ?")
        .bind(version_id)
        .execute(&mut *conn)
        .await?;
    for zone in DeckZone::ALL {
        for (i, e) in list.zone(zone).enumerate() {
            sqlx::query(
                "INSERT INTO deck_version_card (deck_version_id, zone, card_data_id, name, count, position)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(version_id)
            .bind(zone.as_str())
            .bind(e.card_data_id)
            .bind(&e.name)
            .bind(e.count)
            .bind(i as i64 + 1)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

/// The slots of a built version: line → its copies in slot order (`None`
/// = missing). Empty for a plain version.
pub async fn copies(
    conn: &mut SqliteConnection,
    version_id: i64,
) -> sqlx::Result<HashMap<LineRef, Vec<Option<CopyId>>>> {
    let rows: Vec<(String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT zone, position, card_id FROM deck_version_copy
         WHERE deck_version_id = ? ORDER BY zone, position, slot",
    )
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut slots: HashMap<LineRef, Vec<Option<CopyId>>> = HashMap::new();
    for (zone, position, card_id) in rows {
        let (Some(zone), Ok(position)) = (DeckZone::parse(&zone), u32::try_from(position)) else {
            continue;
        };
        slots
            .entry(LineRef { zone, position })
            .or_default()
            .push(card_id);
    }
    Ok(slots)
}

/// The copies of these card data (real cards, sold ones flagged), by card
/// data, in id order.
pub async fn owned_copies(
    conn: &mut SqliteConnection,
    card_data_ids: impl IntoIterator<Item = CardDataId>,
) -> sqlx::Result<HashMap<CardDataId, Vec<Copy>>> {
    let ids: HashSet<CardDataId> = card_data_ids.into_iter().collect();
    let mut by_card: HashMap<CardDataId, Vec<Copy>> = HashMap::new();
    if ids.is_empty() {
        return Ok(by_card);
    }
    let mut qb = QueryBuilder::new(format!(
        "SELECT card.id, card.card_data_id, card.last_known_status, {SOLD_EXISTS} AS sold
         FROM card WHERE card.card_data_id IN "
    ));
    push_ids(&mut qb, ids);
    qb.push(" ORDER BY card.id");
    let rows: Vec<(i64, i64, Option<String>, bool)> =
        qb.build_query_as().fetch_all(&mut *conn).await?;
    for (id, card_data_id, status, sold) in rows {
        by_card.entry(card_data_id).or_default().push(Copy {
            id,
            card_data_id,
            status: status.as_deref().and_then(CardStatus::from_code),
            sold,
        });
    }
    Ok(by_card)
}

/// Which built version holds each of these copies.
pub async fn locks(
    conn: &mut SqliteConnection,
    copies: impl IntoIterator<Item = CopyId>,
) -> sqlx::Result<HashMap<CopyId, i64>> {
    let ids: Vec<CopyId> = copies.into_iter().collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut qb = QueryBuilder::new(
        "SELECT card_id, deck_version_id FROM deck_version_copy WHERE card_id IN ",
    );
    push_ids(&mut qb, ids);
    qb.build_query_as::<(i64, i64)>()
        .fetch_all(&mut *conn)
        .await
        .map(|rows| rows.into_iter().collect())
}

/// What the collection holds of these card data, seen from `version_id`.
pub async fn availability(
    conn: &mut SqliteConnection,
    version_id: i64,
    card_data_ids: impl IntoIterator<Item = CardDataId>,
) -> sqlx::Result<HashMap<CardDataId, Availability>> {
    let owned = owned_copies(conn, card_data_ids).await?;
    let locks = locks(conn, owned.values().flatten().map(|c| c.id)).await?;
    Ok(owned
        .iter()
        .map(|(card, copies)| (*card, ownership::availability(copies, &locks, version_id)))
        .collect())
}

/// The copies of these card data free for `version_id`: owned, unsold,
/// held by no other built version (its own included).
pub async fn free_copies(
    conn: &mut SqliteConnection,
    version_id: i64,
    card_data_ids: impl IntoIterator<Item = CardDataId>,
) -> sqlx::Result<HashMap<CardDataId, Vec<Copy>>> {
    let owned = owned_copies(conn, card_data_ids).await?;
    let locks = locks(conn, owned.values().flatten().map(|c| c.id)).await?;
    Ok(owned
        .into_iter()
        .map(|(card, copies)| {
            let copies = copies
                .into_iter()
                .filter(|c| !c.sold && locks.get(&c.id).is_none_or(|v| *v == version_id))
                .collect();
            (card, copies)
        })
        .collect())
}

/// The copies a version would play: a built version's, else the ones
/// building it would take now (`plan_build`, nothing written).
pub async fn playing_copies(
    conn: &mut SqliteConnection,
    version_id: i64,
    is_built: bool,
) -> sqlx::Result<Vec<CopyId>> {
    if is_built {
        return sqlx::query_scalar(
            "SELECT card_id FROM deck_version_copy WHERE deck_version_id = ? AND card_id IS NOT NULL",
        )
        .bind(version_id)
        .fetch_all(&mut *conn)
        .await;
    }
    let lines = lines(conn, version_id).await?;
    let free = free_copies(
        conn,
        version_id,
        lines.iter().filter_map(|l| l.card_data_id),
    )
    .await?;
    Ok(plan_build(&lines, &free)
        .assigned
        .into_iter()
        .map(|(_, copy)| copy)
        .collect())
}

/// What a built version's assignment did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Assigned {
    /// Slots with a copy.
    pub copies: usize,
    /// Slots without (to buy).
    pub missing: usize,
}

/// Give built version `version_id` its copies: each line keeps the copies it
/// held (by card, unsold), and its other slots take free copies
/// (`plan_build`: best condition first); the rest are missing. Run when
/// it is built, when its lines are saved, and by "Take free copies". Other
/// versions are never touched.
pub async fn assign_copies(conn: &mut SqliteConnection, version_id: i64) -> sqlx::Result<Assigned> {
    let lines = lines(conn, version_id).await?;
    let data_ids: Vec<CardDataId> = lines.iter().filter_map(|l| l.card_data_id).collect();
    let owned = owned_copies(conn, data_ids.iter().copied()).await?;

    // what it held, by card, before the lines changed (a copy sold since is
    // let go)
    let held: Vec<(Option<i64>, i64)> = sqlx::query_as(
        "SELECT card.card_data_id, card.id FROM deck_version_copy
         JOIN card ON card.id = deck_version_copy.card_id
         WHERE deck_version_copy.deck_version_id = ? ORDER BY zone, position, slot",
    )
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM deck_version_copy WHERE deck_version_id = ?")
        .bind(version_id)
        .execute(&mut *conn)
        .await?;
    let sold: HashSet<CopyId> = owned
        .values()
        .flatten()
        .filter(|c| c.sold)
        .map(|c| c.id)
        .collect();
    let mut kept: HashMap<CardDataId, Vec<CopyId>> = HashMap::new();
    for (card_data_id, card_id) in held {
        if let Some(card_data_id) = card_data_id.filter(|_| !sold.contains(&card_id)) {
            kept.entry(card_data_id).or_default().push(card_id);
        }
    }

    // each line first takes back its own copies
    let mut slots: HashMap<LineRef, Vec<Option<CopyId>>> = HashMap::new();
    let mut short: Vec<Line> = Vec::new();
    for line in &lines {
        let mine = line.card_data_id.and_then(|id| kept.get_mut(&id));
        let mut filled = Vec::new();
        if let Some(mine) = mine {
            let n = (line.count as usize).min(mine.len());
            filled.extend(mine.drain(..n).map(Some));
        }
        let lacking = line.count as usize - filled.len();
        slots.insert(line.line_ref(), filled);
        if lacking > 0 {
            short.push(Line {
                count: lacking as u32,
                ..line.clone()
            });
        }
    }

    // then free copies: owned, unsold, held by no built version (its own let
    // go above included)
    let taken: HashSet<CopyId> = slots.values().flatten().flatten().copied().collect();
    let locks = locks(conn, owned.values().flatten().map(|c| c.id)).await?;
    let free: HashMap<CardDataId, Vec<Copy>> = owned
        .into_iter()
        .map(|(card, copies)| {
            let copies = copies
                .into_iter()
                .filter(|c| !c.sold && !taken.contains(&c.id) && !locks.contains_key(&c.id))
                .collect();
            (card, copies)
        })
        .collect();
    let plan = plan_build(&short, &free);
    for (line, copy) in plan.assigned {
        slots.entry(line).or_default().push(Some(copy));
    }

    let mut done = Assigned::default();
    for line in &lines {
        let mut filled = slots.remove(&line.line_ref()).unwrap_or_default();
        filled.resize(line.count as usize, None);
        for (slot, card_id) in filled.into_iter().enumerate() {
            match card_id {
                Some(_) => done.copies += 1,
                None => done.missing += 1,
            }
            sqlx::query(
                "INSERT INTO deck_version_copy (deck_version_id, zone, position, slot, card_id)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(version_id)
            .bind(line.zone.as_str())
            .bind(line.position)
            .bind(slot as i64)
            .bind(card_id)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(done)
}

/// A built version unbuilt: its copies are free again.
pub async fn release_copies(conn: &mut SqliteConnection, version_id: i64) -> sqlx::Result<u64> {
    Ok(
        sqlx::query("DELETE FROM deck_version_copy WHERE deck_version_id = ?")
            .bind(version_id)
            .execute(&mut *conn)
            .await?
            .rows_affected(),
    )
}

/// What the lists lack (`ownership::missing`), one list: the copies free for
/// it (its own when built included).
pub async fn missing(
    conn: &mut SqliteConnection,
    version_id: i64,
    lines: &[Line],
) -> sqlx::Result<Vec<ownership::MissingLine>> {
    let available = availability(
        conn,
        version_id,
        lines.iter().filter_map(|l| l.card_data_id),
    )
    .await?
    .into_iter()
    .map(|(card, a)| (card, a.free + a.here))
    .collect();
    Ok(ownership::missing(lines, &available))
}
