//! The Yu-Gi-Oh! page of the public export (`common::export`): the cards
//! owned (no proxy, not sold), identical copies on one tile ("× 3"), with
//! their market estimate. Nothing about lots, sellers, prices paid, decks
//! or tournaments.

use std::collections::HashMap;

use common::export::{Content, ExportCard, Section};
use rust_decimal::Decimal;

use crate::routes::cards::images::file_name;
use crate::store::cards::SOLD_EXISTS;
use crate::store::db::Pool;
use crate::store::models::CardRow;
use crate::web::filters;

pub async fn section(pool: &Pool) -> sqlx::Result<Section> {
    let rows: Vec<CardRow> = sqlx::query_as(&format!(
        "SELECT {} FROM card WHERE card.is_proxy = 0 AND NOT {SOLD_EXISTS}
         ORDER BY card.en_name, card.code, card.id",
        CardRow::columns()
    ))
    .fetch_all(pool)
    .await?;
    let pictures: HashMap<i64, String> =
        sqlx::query_as::<_, (i64, String)>("SELECT id, en_name FROM card_data")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|(id, en_name)| (id, format!("card_images/{}", file_name(&en_name))))
            .collect();

    // the same card, code, rarity and condition: one tile
    let mut groups: Vec<(CardRow, usize)> = Vec::new();
    for row in rows {
        let key = |r: &CardRow| {
            (
                r.name.clone(),
                r.code.clone(),
                r.rarity.clone(),
                r.last_known_status.clone(),
                r.card_data_id,
            )
        };
        match groups.last_mut() {
            Some((first, count)) if key(first) == key(&row) => *count += 1,
            _ => groups.push((row, 1)),
        }
    }

    let copies: usize = groups.iter().map(|(_, count)| count).sum();
    let value: Decimal = groups
        .iter()
        .filter_map(|(row, count)| Some(row.estimate()?.0 * Decimal::from(*count)))
        .sum();
    let cards = groups
        .into_iter()
        .map(|(row, count)| {
            let mut tile = row.tile();
            if count > 1 {
                tile.tags.insert(0, format!("× {count}"));
            }
            ExportCard {
                tile,
                picture: row.card_data_id.and_then(|id| pictures.get(&id).cloned()),
            }
        })
        .collect();

    Ok(Section {
        slug: "yugioh",
        title: "Yu-Gi-Oh! cards",
        body_class: "",
        summary: format!(
            "{copies} cards · {} € (market estimate)",
            filters::money(&value).unwrap_or_default()
        ),
        content: Content::Cards(cards),
    })
}
