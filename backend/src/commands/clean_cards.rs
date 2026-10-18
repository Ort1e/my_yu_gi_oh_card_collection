//! `clean-cards` (Django: `clean_cards`).
//!
//! 1. normalise `card_data.en_name` and `card.en_name` ([`normalize_card_name`]);
//! 2. link every card to the card data of its name (fetched if unknown);
//! 3. in each deck version, replace proxies by an owned copy of the same
//!    card that isn't in that version yet, moving its category assignments;
//! 4. delete the proxies that are left unused;
//! 5. move the rarity out of Cardmarket names (`Raigeki (V.4 - Ultra Rare)`
//!    → name `Raigeki`, rarity `Ultra Rare`) for the cards without a rarity.
//!
//! Differences from Django: sold cards are never used as replacements,
//! category assignments follow the replacement, proxies bought in a lot are
//! kept, a rename that would clash with another card data is skipped, and
//! step 5 is new (Django had no rarity).

use std::collections::HashMap;

use domain::card::{clean_card_name, normalize_card_name};
use domain::rarity::split_cardmarket_name;
use sqlx::SqliteConnection;

use super::finish;
use crate::store::card_data::get_or_fetch;
use crate::store::cards::{delete_orphan_proxies, version_card_ids, Zone, SOLD_EXISTS};
use crate::store::db::begin_write;
use crate::AppState;

/// Changes to make, computed before the write transaction (resolving names
/// may write card data through the pool).
#[derive(Default)]
struct Plan {
    /// card_data id, new en_name
    data_renames: Vec<(i64, String)>,
    /// card id, new en_name, new card_data id
    card_updates: Vec<(i64, String, Option<i64>)>,
    unresolved: Vec<String>,
}

pub async fn clean(state: &AppState, dry_run: bool) -> anyhow::Result<()> {
    let plan = plan(state, dry_run).await?;
    let pool = &state.pool;

    let mut tx = begin_write(pool).await?;

    for (id, name) in &plan.data_renames {
        sqlx::query("UPDATE card_data SET en_name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    for (id, name, card_data_id) in &plan.card_updates {
        sqlx::query("UPDATE card SET en_name = ?, card_data_id = ? WHERE id = ?")
            .bind(name)
            .bind(card_data_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    let replaced = replace_proxies(&mut tx).await?;
    let deleted = delete_orphan_proxies(&mut tx).await?;
    let rarities = rarities_from_names(&mut tx).await?;

    finish(tx, dry_run).await?;

    println!("card data renamed: {}", plan.data_renames.len());
    println!("cards renamed or relinked: {}", plan.card_updates.len());
    println!("proxies replaced by owned copies: {replaced}");
    println!("unused proxies deleted: {deleted}");
    println!("rarities taken from card names: {rarities}");
    if !plan.unresolved.is_empty() {
        let why = if dry_run {
            "not known locally"
        } else {
            "no card data found"
        };
        println!(
            "{} names {why}: {}",
            plan.unresolved.len(),
            plan.unresolved.join("; ")
        );
    }
    if dry_run {
        println!("dry run: nothing was saved");
    }
    Ok(())
}

async fn plan(state: &AppState, dry_run: bool) -> anyhow::Result<Plan> {
    let pool = &state.pool;
    let mut plan = Plan::default();

    // card data names after normalisation: lookup key -> id
    let data: Vec<(i64, String)> = sqlx::query_as("SELECT id, en_name FROM card_data ORDER BY id")
        .fetch_all(pool)
        .await?;
    let mut by_name: HashMap<String, i64> = data
        .iter()
        .map(|(id, name)| (clean_card_name(name), *id))
        .collect();

    for (id, name) in &data {
        let normalized = normalize_card_name(name);
        if normalized == *name {
            continue;
        }

        let key = clean_card_name(&normalized);
        if by_name.get(&key).is_some_and(|other| other != id) {
            println!("card data {id}: {name:?} would become {normalized:?}, which already exists, skipped");
            continue;
        }

        by_name.remove(&clean_card_name(name));
        by_name.insert(key, *id);
        plan.data_renames.push((*id, normalized));
    }

    let cards: Vec<(i64, String, Option<i64>)> = sqlx::query_as(
        "SELECT id, en_name, card_data_id FROM card WHERE en_name IS NOT NULL ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    // resolve each distinct name once
    let mut resolved: HashMap<String, Option<i64>> = HashMap::new();
    for (id, en_name, card_data_id) in cards {
        let normalized = normalize_card_name(&en_name);
        let key = clean_card_name(&normalized);

        let target = match resolved.get(&key) {
            Some(target) => *target,
            None => {
                let target = match by_name.get(&key) {
                    Some(id) => Some(*id),
                    None if dry_run => None,
                    None => get_or_fetch(pool, &state.http, &normalized).await?,
                };
                if target.is_none() {
                    plan.unresolved.push(normalized.clone());
                }
                resolved.insert(key, target);
                target
            }
        };

        // keep the current link when the name can't be resolved
        let new_link = target.or(card_data_id);
        if normalized != en_name || new_link != card_data_id {
            plan.card_updates.push((id, normalized, new_link));
        }
    }

    Ok(plan)
}

/// Replace proxies in deck zones by owned, unsold copies of the same card
/// not already in that version. Returns the number of replacements.
async fn replace_proxies(conn: &mut sqlx::SqliteConnection) -> anyhow::Result<usize> {
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
                "SELECT id FROM card WHERE is_proxy = 0 AND card_data_id = ? AND NOT {SOLD_EXISTS}
                 ORDER BY id"
            ))
            .bind(card_data_id)
            .fetch_all(&mut *conn)
            .await?;

            let Some(replacement) = owned.into_iter().find(|id| !in_version.contains(id)) else {
                continue;
            };

            sqlx::query(&format!(
                "UPDATE {} SET card_id = ? WHERE deck_version_id = ? AND card_id = ?",
                zone.table()
            ))
            .bind(replacement)
            .bind(version_id)
            .bind(proxy_id)
            .execute(&mut *conn)
            .await?;

            // the replacement takes the proxy's categories in this version
            let categories = "SELECT id FROM card_category WHERE deck_version_id = ?";
            sqlx::query(&format!(
                "INSERT OR IGNORE INTO card_category_assignment (category_id, card_id)
                 SELECT category_id, ? FROM card_category_assignment
                 WHERE card_id = ? AND category_id IN ({categories})"
            ))
            .bind(replacement)
            .bind(proxy_id)
            .bind(version_id)
            .execute(&mut *conn)
            .await?;
            sqlx::query(&format!(
                "DELETE FROM card_category_assignment WHERE card_id = ? AND category_id IN ({categories})"
            ))
            .bind(proxy_id)
            .bind(version_id)
            .execute(&mut *conn)
            .await?;

            replaced += 1;
        }
    }

    Ok(replaced)
}

/// Set the rarity of the cards named like `Raigeki (V.4 - Ultra Rare)` and
/// strip that suffix from their name. Returns how many cards changed.
async fn rarities_from_names(conn: &mut SqliteConnection) -> anyhow::Result<usize> {
    let cards: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM card WHERE rarity IS NULL AND name LIKE '%(V.%' ORDER BY id",
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut changed = 0;
    for (id, name) in &cards {
        let (short, rarity) = split_cardmarket_name(name);
        let Some(rarity) = rarity else {
            println!("card {id}: {name:?} has a version suffix with an unknown rarity, skipped");
            continue;
        };
        sqlx::query("UPDATE card SET name = ?, rarity = ? WHERE id = ?")
            .bind(short)
            .bind(rarity.label())
            .bind(id)
            .execute(&mut *conn)
            .await?;
        changed += 1;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TestApp;

    #[tokio::test]
    async fn cleans_names_links_and_proxies() {
        let t = TestApp::new().await;
        // card_data 1 = dark magician, 2 = kuriboh (from TestApp)
        sqlx::query(
            "INSERT INTO card_data (id, en_name, ygopro_id, json_data) VALUES (3, 'maliss q alpha', 3, '{}');
             INSERT INTO card (id, name, en_name, card_data_id, is_proxy) VALUES
                 (10, 'Maliss q Alpha', 'Maliss q Alpha (Promo)', NULL, 0),
                 (19, 'DM sold', 'dark magician', 1, 0),
                 (20, 'DM owned', 'dark magician', 1, 0),
                 (30, 'DM proxy in deck', 'dark magician', 1, 1),
                 (31, 'Kuriboh proxy in deck', 'kuriboh', 2, 1),
                 (32, 'DM orphan proxy', 'dark magician', 1, 1),
                 (33, 'DM proxy bought in a lot', 'dark magician', 1, 1);
             INSERT INTO lot (id, lot_type, price, buy_date) VALUES
                 (1, 'purchase', '1.00', '2025-01-01'), (2, 'sale', '1.00', '2025-02-01');
             INSERT INTO unite (lot_id, card_id) VALUES (1, 19), (1, 33), (2, 19);
             INSERT INTO deck (id, name) VALUES (1, 'Deck');
             INSERT INTO deck_version (id, deck_id, version_name) VALUES (1, 1, 'v1');
             INSERT INTO deck_version_main_deck (deck_version_id, card_id) VALUES (1, 30), (1, 31);
             INSERT INTO card_category (id, name, deck_version_id) VALUES (1, 'starter', 1);
             INSERT INTO card_category_assignment (category_id, card_id) VALUES (1, 30);",
        )
        .execute(&t.state.pool)
        .await
        .unwrap();

        clean(&t.state, false).await.unwrap();

        assert_eq!(
            t.text("SELECT en_name FROM card_data WHERE id = 3")
                .await
                .as_deref(),
            Some("maliss alpha")
        );
        assert_eq!(
            t.text("SELECT en_name FROM card WHERE id = 10")
                .await
                .as_deref(),
            Some("maliss alpha")
        );
        assert_eq!(
            t.scalar("SELECT card_data_id FROM card WHERE id = 10")
                .await,
            3
        );

        // proxy 30 replaced by the owned, unsold copy 20, with its category
        let main: Vec<i64> = sqlx::query_scalar(
            "SELECT card_id FROM deck_version_main_deck WHERE deck_version_id = 1 ORDER BY card_id",
        )
        .fetch_all(&t.state.pool)
        .await
        .unwrap();
        assert_eq!(main, vec![20, 31]);
        assert_eq!(
            t.scalar("SELECT card_id FROM card_category_assignment")
                .await,
            20
        );

        let cards: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM card WHERE is_proxy = 1 ORDER BY id")
                .fetch_all(&t.state.pool)
                .await
                .unwrap();
        assert_eq!(
            cards,
            vec![31, 33],
            "unused proxies deleted, the one in a lot kept"
        );

        // a second run changes nothing
        clean(&t.state, false).await.unwrap();
        assert_eq!(t.scalar("SELECT count(*) FROM card").await, 5);
    }

    #[tokio::test]
    async fn moves_the_rarity_out_of_cardmarket_names() {
        let t = TestApp::new().await;
        sqlx::query(
            "INSERT INTO card (id, name, en_name, card_data_id, rarity, is_proxy) VALUES
                 (1, 'Raigeki (V.4 - Ultra Rare)', 'raigeki', NULL, NULL, 0),
                 (2, 'Kuriboh (V.6 - Collectors Rare)', 'kuriboh', 2, NULL, 0),
                 (3, 'Odd (V.1 - Shiny)', 'odd', NULL, NULL, 0),
                 (4, 'Kept (V.1 - Rare)', 'kept', NULL, 'Super Rare', 0);",
        )
        .execute(&t.state.pool)
        .await
        .unwrap();

        clean(&t.state, true).await.unwrap();
        assert_eq!(
            t.text("SELECT name FROM card WHERE id = 1")
                .await
                .as_deref(),
            Some("Raigeki (V.4 - Ultra Rare)"),
            "dry run"
        );

        clean(&t.state, false).await.unwrap();
        let card = |id: i64| {
            sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT name, rarity FROM card WHERE id = ?",
            )
            .bind(id)
            .fetch_one(&t.state.pool)
        };
        assert_eq!(
            card(1).await.unwrap(),
            ("Raigeki".into(), Some("Ultra Rare".into()))
        );
        assert_eq!(
            card(2).await.unwrap(),
            ("Kuriboh".into(), Some("Collector's Rare".into()))
        );
        assert_eq!(card(3).await.unwrap(), ("Odd (V.1 - Shiny)".into(), None));
        assert_eq!(
            card(4).await.unwrap(),
            ("Kept (V.1 - Rare)".into(), Some("Super Rare".into())),
            "a card with a rarity is left alone"
        );
    }

    #[tokio::test]
    async fn dry_run_saves_nothing() {
        let t = TestApp::new().await;
        sqlx::query("INSERT INTO card (name, en_name, is_proxy) VALUES ('X', 'Dark Magician', 1)")
            .execute(&t.state.pool)
            .await
            .unwrap();

        clean(&t.state, true).await.unwrap();

        assert_eq!(
            t.text("SELECT en_name FROM card").await.as_deref(),
            Some("Dark Magician")
        );
        assert_eq!(t.scalar("SELECT count(*) FROM card").await, 1);
    }
}
