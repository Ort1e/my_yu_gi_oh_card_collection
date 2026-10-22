//! Writes on `card` rows shared by the forms, the deck builder API and the
//! maintenance commands, and the SQL fragments they share.

use std::collections::HashSet;

use domain::card::normalize_card_name;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::store::db::Money;
use crate::web::forms::NewCards;

/// A deck zone and its join table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Zone {
    Main,
    Extra,
    Side,
}

impl Zone {
    pub const ALL: [Zone; 3] = [Zone::Main, Zone::Extra, Zone::Side];

    pub fn table(self) -> &'static str {
        match self {
            Zone::Main => "deck_version_main_deck",
            Zone::Extra => "deck_version_extra_deck",
            Zone::Side => "deck_version_side_deck",
        }
    }

    /// The cards of this zone of a version, in the order they were added.
    pub async fn card_ids(
        self,
        conn: &mut SqliteConnection,
        version_id: i64,
    ) -> sqlx::Result<Vec<i64>> {
        sqlx::query_scalar(&format!(
            "SELECT card_id FROM {} WHERE deck_version_id = ? ORDER BY rowid",
            self.table()
        ))
        .bind(version_id)
        .fetch_all(&mut *conn)
        .await
    }

    /// Put a card in this zone of a version.
    pub async fn insert(
        self,
        conn: &mut SqliteConnection,
        version_id: i64,
        card_id: i64,
    ) -> sqlx::Result<()> {
        sqlx::query(&format!(
            "INSERT INTO {} (deck_version_id, card_id) VALUES (?, ?)",
            self.table()
        ))
        .bind(version_id)
        .bind(card_id)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }
}

/// SQL condition on `card`: it is in a non-cancelled sale lot.
pub const SOLD_EXISTS: &str = "EXISTS (SELECT 1 FROM unite JOIN lot ON lot.id = unite.lot_id \
     WHERE unite.card_id = card.id AND lot.lot_type = 'sale' AND lot.is_cancelled = 0)";

/// SQL subquery: ids of the cards in any zone of any deck version.
pub const IN_ANY_DECK: &str = "SELECT card_id FROM deck_version_main_deck
    UNION SELECT card_id FROM deck_version_extra_deck
    UNION SELECT card_id FROM deck_version_side_deck";

/// SQL subquery: ids of the cards in any zone of the deck version bound as `?1`.
pub const IN_VERSION: &str = "SELECT card_id FROM deck_version_main_deck WHERE deck_version_id = ?1
    UNION SELECT card_id FROM deck_version_extra_deck WHERE deck_version_id = ?1
    UNION SELECT card_id FROM deck_version_side_deck WHERE deck_version_id = ?1";

/// SQL subquery: ids of the cards **locked** by a built version other than
/// `version_id` (0: by any built version), i.e. its real cards. They are out
/// of the collection for every other version (`domain::built_deck`). The id
/// is an integer written in the SQL, so the subquery mixes with any binds.
pub fn locked_elsewhere(version_id: i64) -> String {
    format!(
        "SELECT zone.card_id
         FROM (SELECT deck_version_id, card_id FROM deck_version_main_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_extra_deck
               UNION ALL SELECT deck_version_id, card_id FROM deck_version_side_deck) AS zone
         JOIN deck_version ON deck_version.id = zone.deck_version_id
         JOIN card AS locked ON locked.id = zone.card_id
         WHERE deck_version.is_built = 1 AND deck_version.id != {version_id}
           AND locked.is_proxy = 0"
    )
}

/// A proxy copy of `card_id` (name, en_name, card data, code, status), like
/// the copies of `clone_version`. Returns its id.
pub async fn proxy_copy(conn: &mut SqliteConnection, card_id: i64) -> sqlx::Result<i64> {
    Ok(sqlx::query(
        "INSERT INTO card (name, en_name, card_data_id, code, last_known_status, is_proxy)
         SELECT name, en_name, card_data_id, code, last_known_status, 1 FROM card WHERE id = ?",
    )
    .bind(card_id)
    .execute(&mut *conn)
    .await?
    .last_insert_rowid())
}

/// In `version_id`, put `new_card` in place of `old_card` (in whichever zone
/// it is), and move its category assignments of that version.
pub async fn swap_card(
    conn: &mut SqliteConnection,
    version_id: i64,
    old_card: i64,
    new_card: i64,
) -> sqlx::Result<()> {
    for zone in Zone::ALL {
        sqlx::query(&format!(
            "UPDATE {} SET card_id = ? WHERE deck_version_id = ? AND card_id = ?",
            zone.table()
        ))
        .bind(new_card)
        .bind(version_id)
        .bind(old_card)
        .execute(&mut *conn)
        .await?;
    }

    let categories = "SELECT id FROM card_category WHERE deck_version_id = ?";
    sqlx::query(&format!(
        "INSERT OR IGNORE INTO card_category_assignment (category_id, card_id)
         SELECT category_id, ? FROM card_category_assignment
         WHERE card_id = ? AND category_id IN ({categories})"
    ))
    .bind(new_card)
    .bind(old_card)
    .bind(version_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(&format!(
        "DELETE FROM card_category_assignment WHERE card_id = ? AND category_id IN ({categories})"
    ))
    .bind(old_card)
    .bind(version_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The cards in any zone of a deck version.
pub async fn version_card_ids(
    conn: &mut SqliteConnection,
    version_id: i64,
) -> sqlx::Result<HashSet<i64>> {
    Ok(sqlx::query_scalar(IN_VERSION)
        .bind(version_id)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .collect())
}

/// Create the cards of `rows` (each `quantity` times) with one unite each in
/// `lot_id`. Returns the new card ids.
pub async fn insert_lot_cards(
    conn: &mut SqliteConnection,
    lot_id: i64,
    rows: &[NewCards],
) -> sqlx::Result<Vec<i64>> {
    let mut ids = Vec::new();
    for row in rows {
        for _ in 0..row.quantity {
            let card_id = sqlx::query(
                "INSERT INTO card (name, en_name, code, last_known_status, rarity, is_proxy)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&row.name)
            .bind(&row.en_name)
            .bind(&row.code)
            .bind(row.status.map(domain::card::CardStatus::code))
            .bind(row.rarity.map(domain::rarity::Rarity::label))
            .bind(row.is_proxy)
            .execute(&mut *conn)
            .await?
            .last_insert_rowid();
            sqlx::query("INSERT INTO unite (price, lot_id, card_id) VALUES (?, ?, ?)")
                .bind(row.price.map(Money))
                .bind(lot_id)
                .bind(card_id)
                .execute(&mut *conn)
                .await?;
            ids.push(card_id);
        }
    }
    Ok(ids)
}

/// Create a proxy card named `name` (its `en_name` is normalised).
pub async fn new_proxy(
    conn: &mut SqliteConnection,
    name: &str,
    card_data_id: Option<i64>,
) -> sqlx::Result<i64> {
    Ok(
        sqlx::query("INSERT INTO card (name, en_name, card_data_id, is_proxy) VALUES (?, ?, ?, 1)")
            .bind(name)
            .bind(normalize_card_name(name))
            .bind(card_data_id)
            .execute(&mut *conn)
            .await?
            .last_insert_rowid(),
    )
}

/// Delete the proxy cards that are in no deck, no lot and no advent
/// calendar. Returns how many were deleted.
///
/// Django's cleanup also deleted proxies bought in a lot (with their unites).
pub async fn delete_orphan_proxies(conn: &mut SqliteConnection) -> sqlx::Result<u64> {
    Ok(sqlx::query(&format!(
        "DELETE FROM card WHERE is_proxy = 1
           AND id NOT IN ({IN_ANY_DECK})
           AND id NOT IN (SELECT card_id FROM unite)
           AND id NOT IN (SELECT card_id FROM advent_calendar_entry)"
    ))
    .execute(&mut *conn)
    .await?
    .rows_affected())
}

/// Replace proxies in deck zones by owned, unsold copies of the same card
/// not already in that version nor locked by another built version, moving
/// their category assignments. Returns the number of replacements. Run by
/// `clean-cards` and after new cards are saved (`card_data::spawn_link_cards`).
pub async fn replace_proxies(conn: &mut SqliteConnection) -> sqlx::Result<usize> {
    let mut replaced = 0;

    for zone in Zone::ALL {
        let proxies: Vec<(i64, i64, i64)> = sqlx::query_as(&format!(
            "SELECT z.deck_version_id, z.card_id, card.card_data_id
             FROM {} z JOIN card ON card.id = z.card_id
             WHERE card.is_proxy = 1 AND card.card_data_id IS NOT NULL
             ORDER BY z.deck_version_id, z.card_id",
            zone.table()
        ))
        .fetch_all(&mut *conn)
        .await?;

        for (version_id, proxy_id, card_data_id) in proxies {
            let in_version = version_card_ids(conn, version_id).await?;

            let owned: Vec<i64> = sqlx::query_scalar(&format!(
                "SELECT id FROM card
                 WHERE is_proxy = 0 AND card_data_id = ? AND NOT {SOLD_EXISTS}
                   AND id NOT IN ({})
                 ORDER BY id",
                locked_elsewhere(version_id)
            ))
            .bind(card_data_id)
            .fetch_all(&mut *conn)
            .await?;

            let Some(replacement) = owned.into_iter().find(|id| !in_version.contains(id)) else {
                continue;
            };

            swap_card(conn, version_id, proxy_id, replacement).await?;
            replaced += 1;
        }
    }

    Ok(replaced)
}
