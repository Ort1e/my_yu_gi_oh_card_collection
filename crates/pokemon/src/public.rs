//! The Pokémon pages of the public export (`common::export`): the copies
//! owned (not sold), identical copies on one tile ("× 2"), with their market
//! value; then the wishlist (`routes::wishlist::public_section`). Nothing
//! about prices paid, openings' notes, or the wishes' maximum prices.

use common::export::{Content, ExportCard, Section};
use common::web::filters;
use rust_decimal::Decimal;

use crate::routes::images::picture_file;
use crate::routes::CopyRow;
use crate::store::prices;
use common::db::Pool;

/// The collection, then the wishlist.
pub async fn sections(pool: &Pool) -> sqlx::Result<Vec<Section>> {
    Ok(vec![
        collection(pool).await?,
        crate::routes::wishlist::public_section(pool).await?,
    ])
}

async fn collection(pool: &Pool) -> sqlx::Result<Section> {
    let mut rows: Vec<CopyRow> = sqlx::query_as(&format!(
        "{} WHERE pc.sold_on IS NULL
         ORDER BY c.catalogue, c.set_id,
             CAST(ltrim(c.local_id, 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz') AS INTEGER),
             c.local_id, pc.id",
        CopyRow::SELECT
    ))
    .fetch_all(pool)
    .await?;
    let latest = prices::latest(pool).await?;
    for row in &mut rows {
        row.set_value(&latest);
    }

    // the same card, language, variant, condition and grading: one tile
    let key = |r: &CopyRow| {
        (
            r.card_id,
            r.language.clone(),
            r.variant.clone(),
            r.condition.clone(),
            r.full_art,
            r.grader.clone(),
            r.grade.clone(),
        )
    };
    let mut groups: Vec<(CopyRow, usize)> = Vec::new();
    for row in rows {
        match groups.last_mut() {
            Some((first, count)) if key(first) == key(&row) => *count += 1,
            _ => groups.push((row, 1)),
        }
    }

    let copies: usize = groups.iter().map(|(_, count)| count).sum();
    let value: Decimal = groups
        .iter()
        .filter_map(|(row, count)| Some(row.value? * Decimal::from(*count)))
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
                picture: picture_file(
                    &row.catalogue,
                    &row.tcgdex_id,
                    &row.language,
                    row.image.clone(),
                    row.image_fr.clone(),
                ),
            }
        })
        .collect();

    Ok(Section {
        slug: "pokemon",
        title: "Pokémon cards",
        body_class: "pokemon",
        summary: format!(
            "{copies} cards · {} € (market value)",
            filters::money(&value).unwrap_or_default()
        ),
        content: Content::Cards(cards),
    })
}
