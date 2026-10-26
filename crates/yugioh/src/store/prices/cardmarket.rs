//! Cardmarket's price guide (`yugioh_domain::cardmarket_catalog`), refreshed with the
//! ygoprodeck prices (`refresh-prices`, weekly): the two files are downloaded
//! into memory, and only what the collection needs goes to the database
//! (migration 0009). The files themselves are never written anywhere.
//!
//! - `cardmarket_expansion`: each set code's expansion, found once from the
//!   set's card names (ygoprodeck), asked again after 30 days when not found;
//! - `cardmarket_product`: the products named like one of our card data, with
//!   their latest prices;
//! - `card.cardmarket_product_id`: each card's product, guessed again at each
//!   refresh unless chosen on the card's edit page;
//! - `cardmarket_price`: the price history of the linked products.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde_json::Value;
use sqlx::SqliteConnection;
use yugioh_domain::card_set::{code_rarities, code_without_language};
use yugioh_domain::cardmarket_catalog::{
    expansions, parse_prices, parse_products, pick_expansion, pick_product, Product, ProductPrices,
    PRICES_URL, PRODUCTS_URL,
};
use yugioh_domain::rarity::Rarity;

use crate::store::card_data::{fetch_set_infos, SetInfo};
use crate::store::db::{begin_write, commit_unless_dry_run, is_fresh, Money, Pool};
use crate::web::error::{AppError, AppResult};

/// A set code whose expansion wasn't found is looked for again after this.
const RECHECK_EXPANSION_DAYS: i64 = 30;

/// What a refresh did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Products named like one of our cards.
    pub products: usize,
    /// Set codes looked up this time, and how many got an expansion.
    pub sets_checked: usize,
    pub sets_found: usize,
    /// Cards (not chosen by hand) with / without a product.
    pub linked: usize,
    pub unlinked: usize,
    /// Rows of price history written.
    pub prices: usize,
}

impl Report {
    pub fn summary(&self) -> String {
        format!(
            "Cardmarket: {} products kept, {} set codes looked up ({} found), \
             {} cards linked, {} not, {} prices written",
            self.products,
            self.sets_checked,
            self.sets_found,
            self.linked,
            self.unlinked,
            self.prices
        )
    }
}

/// Download Cardmarket's files and ygoprodeck's lists of the sets not
/// mapped yet, then [`apply`] them. Every request is done before the write
/// transaction.
pub async fn refresh(
    pool: &Pool,
    http: &reqwest::Client,
    today: NaiveDate,
    dry_run: bool,
) -> AppResult<Report> {
    let bad = |what: &str, e: serde_json::Error| AppError::Internal(format!("{what}: {e}"));
    let products =
        parse_products(&download(http, PRODUCTS_URL).await?).map_err(|e| bad("products", e))?;
    let prices =
        parse_prices(&download(http, PRICES_URL).await?).map_err(|e| bad("price guide", e))?;

    let prefixes = sets_to_look_up(pool, today).await?;
    let sets = fetch_set_infos(http, &prefixes).await?;

    Ok(apply(pool, today, &products, &prices, &sets, dry_run).await?)
}

async fn download(http: &reqwest::Client, url: &str) -> AppResult<Vec<u8>> {
    let internal = |e: reqwest::Error| AppError::Internal(format!("Cardmarket {url}: {e}"));
    let bytes = http
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(internal)?
        .bytes()
        .await
        .map_err(internal)?;
    Ok(bytes.to_vec())
}

/// The set codes of our cards without an expansion: never looked up, or not
/// found more than [`RECHECK_EXPANSION_DAYS`] ago.
async fn sets_to_look_up(pool: &Pool, today: NaiveDate) -> sqlx::Result<Vec<String>> {
    let codes: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT code FROM card WHERE code IS NOT NULL")
            .fetch_all(pool)
            .await?;
    let known: Vec<(String, Option<i64>, String)> =
        sqlx::query_as("SELECT set_code, id_expansion, checked_at FROM cardmarket_expansion")
            .fetch_all(pool)
            .await?;
    let settled: HashSet<String> = known
        .into_iter()
        .filter(|(_, expansion, checked_at)| {
            expansion.is_some() || is_fresh(checked_at, RECHECK_EXPANSION_DAYS, today)
        })
        .map(|(code, _, _)| code)
        .collect();

    let mut prefixes: Vec<String> = codes
        .iter()
        .filter_map(|code| Some(code_without_language(code)?.0))
        .filter(|prefix| !settled.contains(prefix))
        .collect();
    prefixes.sort();
    prefixes.dedup();
    Ok(prefixes)
}

#[derive(sqlx::FromRow)]
struct CardToLink {
    id: i64,
    code: Option<String>,
    rarity: Option<String>,
    en_name: String,
    json_data: String,
    cardmarket_product_id: Option<i64>,
}

/// Write what the collection needs from Cardmarket's `products` and
/// `prices`: the expansions of the set codes in `sets`, the products named
/// like our card data, the guessed links of the cards not chosen by hand,
/// and today's prices of the linked products. Rolled back with `dry_run`.
pub async fn apply(
    pool: &Pool,
    today: NaiveDate,
    products: &[Product],
    prices: &HashMap<i64, ProductPrices>,
    sets: &HashMap<String, SetInfo>,
    dry_run: bool,
) -> sqlx::Result<Report> {
    let all_expansions = expansions(products);
    let found: Vec<(&str, Option<i64>)> = sets
        .iter()
        .map(|(prefix, set)| {
            let expansion = pick_expansion(&set.names, &all_expansions, set.release);
            (prefix.as_str(), expansion)
        })
        .collect();

    let our_names: HashSet<String> = sqlx::query_scalar("SELECT en_name FROM card_data")
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
    let kept: Vec<&Product> = products
        .iter()
        .filter(|p| our_names.contains(&p.key()))
        .collect();

    let today = today.to_string();
    let mut tx = begin_write(pool).await?;
    save_expansions(&mut tx, &found, &today).await?;
    save_products(&mut tx, &kept, prices, &today).await?;
    let (linked, unlinked) = relink_cards(&mut tx, &kept, prices).await?;
    let saved_prices = save_linked_prices(&mut tx, prices, &today).await?;
    commit_unless_dry_run(tx, dry_run).await?;

    Ok(Report {
        products: kept.len(),
        sets_checked: found.len(),
        sets_found: found.iter().filter(|(_, e)| e.is_some()).count(),
        linked,
        unlinked,
        prices: saved_prices,
    })
}

/// Each set code looked up and its expansion (NULL: not found, asked again
/// later).
async fn save_expansions(
    conn: &mut SqliteConnection,
    found: &[(&str, Option<i64>)],
    today: &str,
) -> sqlx::Result<()> {
    for (prefix, expansion) in found {
        sqlx::query(
            "INSERT OR REPLACE INTO cardmarket_expansion (set_code, id_expansion, checked_at)
             VALUES (?, ?, ?)",
        )
        .bind(prefix)
        .bind(expansion)
        .bind(today)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// The products named like our card data, with their latest prices.
async fn save_products(
    conn: &mut SqliteConnection,
    kept: &[&Product],
    prices: &HashMap<i64, ProductPrices>,
    today: &str,
) -> sqlx::Result<()> {
    for product in kept {
        let product_prices = prices.get(&product.id).copied().unwrap_or_default();
        sqlx::query(
            "INSERT INTO cardmarket_product
                 (id, name, name_key, id_expansion, added, trend, avg30, low, prices_date)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, name_key = excluded.name_key,
                 id_expansion = excluded.id_expansion, added = excluded.added,
                 trend = excluded.trend, avg30 = excluded.avg30, low = excluded.low,
                 prices_date = excluded.prices_date",
        )
        .bind(product.id)
        .bind(&product.name)
        .bind(product.key())
        .bind(product.expansion)
        .bind(product.added.to_string())
        .bind(product_prices.trend.map(Money))
        .bind(product_prices.avg30.map(Money))
        .bind(product_prices.low.map(Money))
        .bind(today)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Guess again the product of every card not chosen by hand. Returns how
/// many got one and how many didn't.
async fn relink_cards(
    conn: &mut SqliteConnection,
    kept: &[&Product],
    prices: &HashMap<i64, ProductPrices>,
) -> sqlx::Result<(usize, usize)> {
    let expansion_of: HashMap<String, i64> = sqlx::query_as::<_, (String, i64)>(
        "SELECT set_code, id_expansion FROM cardmarket_expansion WHERE id_expansion IS NOT NULL",
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    let mut by_expansion_name: ProductsByName = HashMap::new();
    for product in kept {
        let price = prices.get(&product.id).and_then(ProductPrices::value);
        by_expansion_name
            .entry((product.expansion, product.key()))
            .or_default()
            .push((product.id, price));
    }

    let cards: Vec<CardToLink> = sqlx::query_as(
        "SELECT card.id, card.code, card.rarity, card_data.en_name, card_data.json_data,
                card.cardmarket_product_id
         FROM card JOIN card_data ON card_data.id = card.card_data_id
         WHERE card.cardmarket_product_manual = 0",
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut linked = 0;
    for card in &cards {
        let product = guess_product(card, &expansion_of, &by_expansion_name);
        linked += usize::from(product.is_some());

        if product != card.cardmarket_product_id {
            sqlx::query("UPDATE card SET cardmarket_product_id = ? WHERE id = ?")
                .bind(product)
                .bind(card.id)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok((linked, cards.len() - linked))
}

/// Today's price of every product a card is linked to. Returns how many.
async fn save_linked_prices(
    conn: &mut SqliteConnection,
    prices: &HashMap<i64, ProductPrices>,
    today: &str,
) -> sqlx::Result<usize> {
    let linked: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT cardmarket_product_id FROM card WHERE cardmarket_product_id IS NOT NULL",
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut saved = 0;
    for product_id in linked {
        let Some(price) = prices.get(&product_id).and_then(ProductPrices::value) else {
            continue;
        };

        sqlx::query(
            "INSERT OR REPLACE INTO cardmarket_price (product_id, date, price) VALUES (?, ?, ?)",
        )
        .bind(product_id)
        .bind(today)
        .bind(Money(price))
        .execute(&mut *conn)
        .await?;
        saved += 1;
    }
    Ok(saved)
}

/// The products of each (expansion, name key), with their price.
type ProductsByName = HashMap<(i64, String), Vec<(i64, Option<rust_decimal::Decimal>)>>;

/// A card's product: its set code's expansion, then the products of its name
/// there (`pick_product`).
fn guess_product(
    card: &CardToLink,
    expansion_of: &HashMap<String, i64>,
    by_expansion_name: &ProductsByName,
) -> Option<i64> {
    let code = card.code.as_deref()?;
    let (prefix, _) = code_without_language(code)?;
    let expansion = *expansion_of.get(&prefix)?;
    let products = by_expansion_name.get(&(expansion, card.en_name.clone()))?;

    let json: Value = serde_json::from_str(&card.json_data).unwrap_or_default();
    let rarity = card.rarity.as_deref().and_then(Rarity::from_label);
    pick_product(products, &code_rarities(&json, code), rarity)
}

/// A product the owner can pick for a card on its edit page.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Candidate {
    pub id: i64,
    pub id_expansion: i64,
    pub added: String,
    pub trend: Option<Money>,
    pub low: Option<Money>,
    /// The set codes mapped to its expansion (`SUDA`), comma-separated.
    pub set_codes: Option<String>,
}

impl Candidate {
    /// `806175 · expansion 5902 (SUDA) · added 2025-01-15 · trend 48.18 € · low 36.99 €`
    pub fn label(&self) -> String {
        let euros = |price: Option<Money>| price.map_or("—".to_string(), |p| format!("{p} €"));
        let sets = self
            .set_codes
            .as_deref()
            .map_or(String::new(), |codes| format!(" ({codes})"));
        format!(
            "{} · expansion {}{sets} · added {} · trend {} · low {}",
            self.id,
            self.id_expansion,
            self.added,
            euros(self.trend),
            euros(self.low)
        )
    }
}

/// The products named like the card data `card_data_id`, newest first.
pub async fn candidates(pool: &Pool, card_data_id: i64) -> sqlx::Result<Vec<Candidate>> {
    sqlx::query_as(
        "SELECT p.id, p.id_expansion, p.added, p.trend, p.low,
                (SELECT group_concat(set_code, ', ') FROM cardmarket_expansion AS e
                  WHERE e.id_expansion = p.id_expansion) AS set_codes
         FROM cardmarket_product AS p
         JOIN card_data ON card_data.en_name = p.name_key
         WHERE card_data.id = ?
         ORDER BY p.added DESC, p.id",
    )
    .bind(card_data_id)
    .fetch_all(pool)
    .await
}
