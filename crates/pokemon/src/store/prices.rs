//! The Pokémon cards' Cardmarket prices (from TCGdex), in `pokemon_price`:
//! one row per card and date, each variant's price
//! (`pokemon_domain::price`). Refreshed weekly with the Yu-Gi-Oh! prices
//! (`commands::prices`), for the cards owned (not sold) or wished.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use pokemon_domain::price::{from_json, to_json, variant_prices};
use pokemon_domain::{Catalogue, Variant};
use rust_decimal::Decimal;

use super::{cards, tcgdex};
use common::db::{begin_write, commit_unless_dry_run, Pool};
use common::web::error::AppResult;

pub type VariantPrices = BTreeMap<Variant, Decimal>;

/// Each card's latest prices.
pub async fn latest(pool: &Pool) -> sqlx::Result<HashMap<i64, VariantPrices>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT p.card_id, p.prices FROM pokemon_price p
         WHERE p.date = (SELECT max(date) FROM pokemon_price WHERE card_id = p.card_id)",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, prices)| (id, from_json(&prices)))
        .collect())
}

/// A card's prices, oldest first: `(date, prices)`.
pub async fn history(pool: &Pool, card_id: i64) -> sqlx::Result<Vec<(String, VariantPrices)>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT date, prices FROM pokemon_price WHERE card_id = ? ORDER BY date")
            .bind(card_id)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(date, prices)| (date, from_json(&prices)))
        .collect())
}

#[derive(Debug, Default)]
pub struct Report {
    pub asked: usize,
    pub written: usize,
    pub without_price: usize,
    pub failed: usize,
    pub orphans_deleted: u64,
}

impl Report {
    pub fn summary(&self) -> String {
        format!(
            "Pokémon: {} cards asked, {} prices written ({} without a price), {} failed, \
             {} unused cards deleted",
            self.asked, self.written, self.without_price, self.failed, self.orphans_deleted
        )
    }
}

/// Today's prices of every card owned (not sold) or wished, not priced on
/// `date` yet: one TCGdex call each. `dry_run`: count them, ask nothing.
pub async fn refresh(
    pool: &Pool,
    http: &reqwest::Client,
    date: NaiveDate,
    dry_run: bool,
) -> AppResult<Report> {
    let mut report = Report::default();
    let date = date.to_string();
    let wanted: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT c.id, c.catalogue, c.tcgdex_id FROM pokemon_card c
         WHERE (EXISTS (SELECT 1 FROM pokemon_copy WHERE card_id = c.id AND sold_on IS NULL)
                OR EXISTS (SELECT 1 FROM pokemon_wish WHERE card_id = c.id AND got_copy_id IS NULL))
           AND NOT EXISTS (SELECT 1 FROM pokemon_price WHERE card_id = c.id AND date = ?)
         ORDER BY c.id",
    )
    .bind(&date)
    .fetch_all(pool)
    .await?;
    report.asked = wanted.len();

    let mut found = Vec::new();
    if !dry_run {
        for (id, catalogue, tcgdex_id) in wanted {
            let language = Catalogue::from_code(&catalogue)
                .unwrap_or(Catalogue::International)
                .data_language();
            match tcgdex::card(http, language, &tcgdex_id).await {
                Ok(Some(json)) => found.push((id, variant_prices(&json))),
                Ok(None) => report.failed += 1,
                Err(e) => {
                    tracing::warn!("{e}");
                    report.failed += 1;
                }
            }
        }
    }

    let mut tx = begin_write(pool).await?;
    for (id, prices) in found {
        if prices.is_empty() {
            report.without_price += 1;
            continue;
        }
        sqlx::query(
            "INSERT INTO pokemon_price (card_id, date, prices) VALUES (?, ?, ?)
             ON CONFLICT (card_id, date) DO UPDATE SET prices = excluded.prices",
        )
        .bind(id)
        .bind(&date)
        .bind(to_json(&prices))
        .execute(&mut *tx)
        .await?;
        report.written += 1;
    }
    report.orphans_deleted = cards::delete_orphans(&mut tx).await?;
    commit_unless_dry_run(tx, dry_run).await?;
    Ok(report)
}
