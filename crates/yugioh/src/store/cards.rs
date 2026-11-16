//! Writes on `card` rows shared by the forms and the maintenance commands,
//! and the SQL fragments they share. (A deck's cards: `store::lists`.)

use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;
use yugioh_domain::deck_history::DeckZone;

use crate::store::db::Money;
use crate::web::card_rows::NewCards;
use ts_rs::TS;

/// A deck zone, as the deck builder's JSON writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Zone {
    Main,
    Extra,
    Side,
}

impl Zone {
    pub const ALL: [Zone; 3] = [Zone::Main, Zone::Extra, Zone::Side];

    pub fn deck_zone(self) -> DeckZone {
        match self {
            Zone::Main => DeckZone::Main,
            Zone::Extra => DeckZone::Extra,
            Zone::Side => DeckZone::Side,
        }
    }

    pub fn of(zone: DeckZone) -> Zone {
        match zone {
            DeckZone::Main => Zone::Main,
            DeckZone::Extra => Zone::Extra,
            DeckZone::Side => Zone::Side,
        }
    }
}

/// SQL condition on `card`: it is in a non-cancelled sale lot.
pub const SOLD_EXISTS: &str = "EXISTS (SELECT 1 FROM unite JOIN lot ON lot.id = unite.lot_id \
     WHERE unite.card_id = card.id AND lot.lot_type = 'sale' AND lot.is_cancelled = 0)";

/// Create the cards of `rows` (each `quantity` times) with one unite each in
/// `lot_id`, linked to their card data when `card_data_id` is set. Returns
/// the new card ids, in the order of the rows.
pub async fn insert_lot_cards(
    conn: &mut SqliteConnection,
    lot_id: i64,
    rows: &[NewCards],
) -> sqlx::Result<Vec<i64>> {
    let mut ids = Vec::new();
    // a sale lot sells the copies flagged "to sell" first: its unites point to
    // those very cards (their purchase stays), only the other lines make cards
    let is_sale: bool = sqlx::query_scalar("SELECT lot_type = 'sale' FROM lot WHERE id = ?")
        .bind(lot_id)
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or(false);
    for row in rows {
        for _ in 0..row.quantity {
            if is_sale {
                if let Some(card_id) = flagged_match(conn, row).await? {
                    sqlx::query("INSERT INTO unite (price, lot_id, card_id) VALUES (?, ?, ?)")
                        .bind(row.price.map(Money))
                        .bind(lot_id)
                        .bind(card_id)
                        .execute(&mut *conn)
                        .await?;
                    sold_flagged(conn, card_id).await?;
                    continue;
                }
            }
            let card_id = sqlx::query(
                "INSERT INTO card (name, en_name, code, last_known_status, rarity, card_data_id)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&row.name)
            .bind(&row.en_name)
            .bind(&row.code)
            .bind(row.status.map(yugioh_domain::card::CardStatus::code))
            .bind(row.rarity.map(yugioh_domain::rarity::Rarity::label))
            .bind(row.card_data_id)
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

/// Flag (or unflag) copies as "to sell" (`card.for_sale`). A copy already
/// sold is never flagged. Returns how many rows changed.
pub async fn set_for_sale(
    conn: &mut SqliteConnection,
    ids: &[i64],
    for_sale: bool,
) -> sqlx::Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut qb = sqlx::QueryBuilder::new("UPDATE card SET for_sale = ");
    qb.push_bind(for_sale).push(" WHERE id IN ");
    crate::store::db::push_ids(&mut qb, ids.iter().copied());
    if for_sale {
        qb.push(" AND NOT ").push(SOLD_EXISTS);
    }
    Ok(qb.build().execute(&mut *conn).await?.rows_affected())
}

/// A flagged copy a sale line takes: unsold, same code and rarity, the same
/// condition first. `None` when no flagged copy matches.
async fn flagged_match(conn: &mut SqliteConnection, row: &NewCards) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar(&format!(
        "SELECT card.id FROM card
         WHERE card.for_sale = 1 AND NOT {SOLD_EXISTS}
           AND card.code = ? COLLATE NOCASE AND card.rarity IS ?
         ORDER BY card.last_known_status IS ? DESC, card.id LIMIT 1"
    ))
    .bind(&row.code)
    .bind(row.rarity.map(yugioh_domain::rarity::Rarity::label))
    .bind(row.status.map(yugioh_domain::card::CardStatus::code))
    .fetch_optional(&mut *conn)
    .await
}

/// A flagged copy sold: its flag is cleared and the built decks' slots
/// holding it are emptied (like any sold copy: the slot is then missing).
async fn sold_flagged(conn: &mut SqliteConnection, card_id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE card SET for_sale = 0 WHERE id = ?")
        .bind(card_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("UPDATE deck_version_copy SET card_id = NULL WHERE card_id = ?")
        .bind(card_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}
