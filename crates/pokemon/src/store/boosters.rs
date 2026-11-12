//! The boosters the wishlist suggests (`pokemon_domain::booster`, migration
//! 0028): each card's Cardmarket expansion in `pokemon_card.cardmarket_expansion`,
//! each expansion's booster and its latest price in `pokemon_booster`.
//! Refreshed weekly with the prices (`refresh-prices`): Cardmarket's three
//! files (singles, non-singles, price guide) are read in memory, never
//! written anywhere.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use common::db::{begin_write, commit_unless_dry_run, Money, Pool};
use common::http::download;
use common::web::error::{AppError, AppResult};
use common_domain::cardmarket::{parse_prices, price_guide_url, products_url, ProductPrices};
use pokemon_domain::booster::{parse_products, pick_booster, Booster, Product, GAME};

/// What a refresh did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Cards with / without a Cardmarket expansion.
    pub linked: usize,
    pub unlinked: usize,
    /// Boosters kept, and how many have a price.
    pub boosters: usize,
    pub priced: usize,
}

impl Report {
    pub fn summary(&self) -> String {
        format!(
            "Pokémon boosters: {} cards linked to their Cardmarket expansion, {} not, \
             {} boosters ({} with a price)",
            self.linked, self.unlinked, self.boosters, self.priced
        )
    }
}

/// Download Cardmarket's files, then [`apply`] them.
pub async fn refresh(
    pool: &Pool,
    http: &reqwest::Client,
    date: NaiveDate,
    dry_run: bool,
) -> AppResult<Report> {
    let bad = |what: &str, e: serde_json::Error| AppError::Internal(format!("{what}: {e}"));
    let singles = parse_products(&download(http, &products_url("singles", GAME)).await?)
        .map_err(|e| bad("Pokémon singles", e))?;
    let others = parse_products(&download(http, &products_url("nonsingles", GAME)).await?)
        .map_err(|e| bad("Pokémon products", e))?;
    let prices = parse_prices(&download(http, &price_guide_url(GAME)).await?)
        .map_err(|e| bad("Pokémon price guide", e))?;
    Ok(apply(pool, date, &singles, &others, &prices, dry_run).await?)
}

/// Link every card to its product's expansion, and keep the booster of each
/// of these expansions with its price. Rolled back with `dry_run`.
pub async fn apply(
    pool: &Pool,
    date: NaiveDate,
    singles: &[Product],
    others: &[Product],
    prices: &HashMap<i64, ProductPrices>,
    dry_run: bool,
) -> sqlx::Result<Report> {
    let mut report = Report::default();
    let expansion_of: HashMap<i64, i64> = singles.iter().map(|p| (p.id, p.expansion)).collect();
    let cards: Vec<(i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, json_extract(json, '$.pricing.cardmarket.idProduct') FROM pokemon_card",
    )
    .fetch_all(pool)
    .await?;

    let mut tx = begin_write(pool).await?;
    let mut expansions: HashSet<i64> = HashSet::new();
    for (id, product) in cards {
        let expansion = product.and_then(|p| expansion_of.get(&p).copied());
        match expansion {
            Some(expansion) => {
                report.linked += 1;
                expansions.insert(expansion);
            }
            None => report.unlinked += 1,
        }
        sqlx::query("UPDATE pokemon_card SET cardmarket_expansion = ? WHERE id = ?")
            .bind(expansion)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    let mut by_expansion: HashMap<i64, Vec<&Product>> = HashMap::new();
    for product in others.iter().filter(|p| expansions.contains(&p.expansion)) {
        by_expansion
            .entry(product.expansion)
            .or_default()
            .push(product);
    }
    sqlx::query("DELETE FROM pokemon_booster")
        .execute(&mut *tx)
        .await?;
    for (expansion, products) in by_expansion {
        let Some(booster) = pick_booster(products) else {
            continue;
        };
        let price = prices.get(&booster.id).and_then(ProductPrices::value);
        report.boosters += 1;
        report.priced += usize::from(price.is_some());
        sqlx::query(
            "INSERT INTO pokemon_booster (id_expansion, id_product, name, price, date)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(expansion)
        .bind(booster.id)
        .bind(&booster.name)
        .bind(price.map(Money))
        .bind(date.to_string())
        .execute(&mut *tx)
        .await?;
    }
    commit_unless_dry_run(tx, dry_run).await?;
    Ok(report)
}

/// Every booster known, by expansion.
pub async fn all(pool: &Pool) -> sqlx::Result<HashMap<i64, Booster>> {
    let rows: Vec<(i64, String, Option<Money>)> =
        sqlx::query_as("SELECT id_expansion, name, price FROM pokemon_booster")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(expansion, name, price)| {
            let booster = Booster {
                expansion,
                name,
                price: price.map(|m| m.0),
            };
            (expansion, booster)
        })
        .collect())
}

/// A card's expansion in SQL (`pokemon_card c`): its own, else (a card come
/// since the last refresh) that of another card of its TCGdex set.
pub const EXPANSION: &str = "COALESCE(c.cardmarket_expansion,
        (SELECT o.cardmarket_expansion FROM pokemon_card o
         WHERE o.catalogue = c.catalogue AND o.set_id = c.set_id
           AND o.cardmarket_expansion IS NOT NULL LIMIT 1))";
