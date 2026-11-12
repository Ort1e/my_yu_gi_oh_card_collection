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
                "INSERT INTO card (name, en_name, code, last_known_status, rarity)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&row.name)
            .bind(&row.en_name)
            .bind(&row.code)
            .bind(row.status.map(yugioh_domain::card::CardStatus::code))
            .bind(row.rarity.map(yugioh_domain::rarity::Rarity::label))
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
