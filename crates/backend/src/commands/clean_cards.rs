//! `clean-cards` (Django: `clean_cards`).
//!
//! 1. normalise `card_data.en_name` and `card.en_name` ([`normalize_card_name`]);
//! 2. link every card to the card data of its name (fetched if unknown);
//! 3. in each deck version that is not built, replace proxies by an owned
//!    copy of the same card that isn't in that version yet nor locked by a
//!    built version, moving its category assignments (a built version's
//!    proxies are swapped by hand, in the deck builder);
//! 4. delete the proxies that are left unused;
//! 5. move the rarity out of Cardmarket names (`Raigeki (V.4 - Ultra Rare)`
//!    → name `Raigeki`, rarity `Ultra Rare`) for the cards without a rarity;
//! 6. put names in the language of their code: a card coded in another
//!    language than English (`MAMO-FR023`) but still named in English gets
//!    its name in that language from ygoprodeck (the code is the truth).
//!    ygoprodeck is only read, so a dry run looks the names up too;
//! 7. give the cards still without a rarity the rarity of their code, when
//!    their card data lists that code in one rarity only
//!    ([`single_rarity`]: `MAMO-FR023` matches ygoprodeck's `MAMO-EN023`).
//!    Codes printed in several rarities, or not listed, stay empty (to set
//!    by hand); proxies are left alone. Runs after step 5: a `(V.x - …)`
//!    name is more reliable than ygoprodeck.
//!
//! Differences from Django: sold cards are never used as replacements,
//! category assignments follow the replacement, proxies bought in a lot are
//! kept, a rename that would clash with another card data is skipped, and
//! steps 5 to 7 are new (Django had no rarity, and kept the names as typed).

use std::collections::HashMap;
use std::time::Duration;

use yugioh_domain::card::{clean_card_name, normalize_card_name};
use yugioh_domain::card_set::single_rarity;
use yugioh_domain::rarity::split_cardmarket_name;
use sqlx::SqliteConnection;

use crate::store::card_data::{get_or_fetch, plan_translations, Translation};
use crate::store::cards::{delete_orphan_proxies, replace_proxies};
use crate::store::db::{begin_write, commit_unless_dry_run};
use crate::AppState;

/// Changes to make, computed before the write transaction (resolving names
/// may write card data through the pool).
#[derive(Default)]
struct Plan {
    data_renames: Vec<DataRename>,
    card_updates: Vec<CardUpdate>,
    unresolved: Vec<String>,
    /// Step 6: names to put in the language of their code.
    translations: Vec<Translation>,
    /// Step 6: cards named in English with another language in their code,
    /// for which ygoprodeck has no translation ("Name (code)").
    untranslated: Vec<String>,
}

/// Step 1: a card data's normalised name.
struct DataRename {
    card_data_id: i64,
    en_name: String,
}

/// Steps 1 and 2: a card's normalised name and its card data.
struct CardUpdate {
    card_id: i64,
    en_name: String,
    card_data_id: Option<i64>,
}

/// What a run did (or would do, for a dry run).
struct Report {
    plan: Plan,
    replaced: usize,
    deleted: u64,
    rarities: usize,
    rarities_from_data: usize,
}

/// The `clean-cards` command: run, then print what was done.
pub async fn clean(state: &AppState, dry_run: bool) -> anyhow::Result<()> {
    run(state, dry_run).await?.print(dry_run);
    Ok(())
}

/// The background cleanup: a few minutes after the server starts, then once
/// a day. New cards are already cleaned when they are saved; this catches the
/// rest, mainly names ygoprodeck has translated since. Logs a summary.
pub fn spawn_daily(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(DAILY_START_DELAY).await;
        loop {
            match run(&state, false).await {
                Ok(report) => report.log(),
                Err(e) => tracing::warn!("background clean-cards: {e:#}"),
            }
            tokio::time::sleep(DAILY_INTERVAL).await;
        }
    });
}

const DAILY_START_DELAY: Duration = Duration::from_secs(5 * 60);
const DAILY_INTERVAL: Duration = Duration::from_secs(24 * 3600);

async fn run(state: &AppState, dry_run: bool) -> anyhow::Result<Report> {
    let plan = plan(state, dry_run).await?;
    let pool = &state.pool;

    let mut tx = begin_write(pool).await?;

    for rename in &plan.data_renames {
        sqlx::query("UPDATE card_data SET en_name = ? WHERE id = ?")
            .bind(&rename.en_name)
            .bind(rename.card_data_id)
            .execute(&mut *tx)
            .await?;
    }

    for update in &plan.card_updates {
        sqlx::query("UPDATE card SET en_name = ?, card_data_id = ? WHERE id = ?")
            .bind(&update.en_name)
            .bind(update.card_data_id)
            .bind(update.card_id)
            .execute(&mut *tx)
            .await?;
    }

    let replaced = replace_proxies(&mut tx).await?;
    let deleted = delete_orphan_proxies(&mut tx).await?;
    let rarities = rarities_from_names(&mut tx).await?;
    let rarities_from_data = rarities_from_card_data(&mut tx).await?;

    for t in &plan.translations {
        sqlx::query("UPDATE card SET name = ? WHERE id = ?")
            .bind(&t.to)
            .bind(t.card_id)
            .execute(&mut *tx)
            .await?;
    }

    commit_unless_dry_run(tx, dry_run).await?;

    Ok(Report {
        plan,
        replaced,
        deleted,
        rarities,
        rarities_from_data,
    })
}

impl Report {
    fn print(&self, dry_run: bool) {
        let Report {
            plan,
            replaced,
            deleted,
            rarities,
            rarities_from_data,
        } = self;

        println!("card data renamed: {}", plan.data_renames.len());
        println!("cards renamed or relinked: {}", plan.card_updates.len());
        println!("proxies replaced by owned copies: {replaced}");
        println!("unused proxies deleted: {deleted}");
        println!("rarities taken from card names: {rarities}");
        println!("rarities taken from card data: {rarities_from_data}");
        for t in &plan.translations {
            println!(
                "card {} ({}): {:?} -> {:?}",
                t.card_id, t.code, t.from, t.to
            );
        }
        println!(
            "names put in the language of their code: {}",
            plan.translations.len()
        );
        if !plan.untranslated.is_empty() {
            println!(
                "{} cards named in English without a translation at ygoprodeck: {}",
                plan.untranslated.len(),
                plan.untranslated.join("; ")
            );
        }
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
    }

    /// One log line, with the counts only.
    fn log(&self) {
        tracing::info!(
            card_data_renamed = self.plan.data_renames.len(),
            cards_relinked = self.plan.card_updates.len(),
            proxies_replaced = self.replaced,
            proxies_deleted = self.deleted,
            rarities = self.rarities,
            rarities_from_data = self.rarities_from_data,
            names_translated = self.plan.translations.len(),
            without_translation = self.plan.untranslated.len(),
            unresolved = self.plan.unresolved.len(),
            "background clean-cards done"
        );
    }
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
            tracing::warn!(
                card_data_id = id,
                "card data {name:?} would become {normalized:?}, which already exists, skipped"
            );
            continue;
        }

        by_name.remove(&clean_card_name(name));
        by_name.insert(key, *id);
        plan.data_renames.push(DataRename {
            card_data_id: *id,
            en_name: normalized,
        });
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
            plan.card_updates.push(CardUpdate {
                card_id: id,
                en_name: normalized,
                card_data_id: new_link,
            });
        }
    }

    let (translations, untranslated) = plan_translations(&state.pool, &state.http, None).await?;
    plan.translations = translations;
    plan.untranslated = untranslated;

    Ok(plan)
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
            tracing::warn!(
                card_id = id,
                "{name:?} has a version suffix with an unknown rarity, skipped"
            );
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

/// Set the rarity of the cards without one whose code was printed in one
/// rarity only, from their card data (step 7). Returns how many cards changed.
async fn rarities_from_card_data(conn: &mut SqliteConnection) -> anyhow::Result<usize> {
    let cards: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT c.id, c.code, d.json_data FROM card c JOIN card_data d ON d.id = c.card_data_id
         WHERE c.rarity IS NULL AND c.is_proxy = 0 AND c.code IS NOT NULL ORDER BY c.id",
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut changed = 0;
    for (id, code, json) in &cards {
        let json: serde_json::Value = match serde_json::from_str(json) {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!(card_id = id, "unreadable card data ({e}), skipped");
                continue;
            }
        };
        let Some(rarity) = single_rarity(&json, code) else {
            continue;
        };
        sqlx::query("UPDATE card SET rarity = ? WHERE id = ?")
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
    async fn takes_the_rarity_of_single_rarity_codes_from_card_data() {
        let t = TestApp::new().await;
        sqlx::query(
            r#"INSERT INTO card_data (id, en_name, ygopro_id, json_data) VALUES (3, 'tri-horned dragon', 39111158,
                 '{"card_sets": [{"set_code": "RA05-EN001", "set_rarity": "Starlight Rare"},
                                 {"set_code": "RA05-EN001", "set_rarity": "Secret Rare"},
                                 {"set_code": "LOB-EN005", "set_rarity": "Ultra Rare"}]}');
             INSERT INTO card (id, name, en_name, code, card_data_id, rarity, is_proxy) VALUES
                 (1, 'Dragon à Trois Cornes', 'tri-horned dragon', 'LOB-F005', 3, NULL, 0),
                 (2, 'Tri-Horned Dragon', 'tri-horned dragon', 'RA05-EN001', 3, NULL, 0),
                 (3, 'Tri-Horned Dragon', 'tri-horned dragon', 'LOB-EN005', 3, NULL, 1),
                 (4, 'Tri-Horned Dragon', 'tri-horned dragon', 'LOB-EN005', 3, 'Common', 0),
                 (5, 'Tri-Horned Dragon', 'tri-horned dragon', 'SDY-001', 3, NULL, 0);
             -- a proxy in a lot, so that step 4 keeps it
             INSERT INTO lot (id, lot_type, price, buy_date) VALUES (1, 'purchase', '1.00', '2025-01-01');
             INSERT INTO unite (lot_id, card_id) VALUES (1, 3);"#,
        )
        .execute(&t.state.pool)
        .await
        .unwrap();

        clean(&t.state, true).await.unwrap();
        assert_eq!(
            t.text("SELECT rarity FROM card WHERE id = 1").await,
            None,
            "dry run"
        );

        clean(&t.state, false).await.unwrap();
        let rarities: Vec<Option<String>> =
            sqlx::query_scalar("SELECT rarity FROM card ORDER BY id")
                .fetch_all(&t.state.pool)
                .await
                .unwrap();
        assert_eq!(
            rarities,
            [
                Some("Ultra Rare".into()), // one rarity, old French code
                None,                      // several rarities
                None,                      // a proxy
                Some("Common".into()),     // already set, kept
                None,                      // code not listed
            ]
        );
    }

    #[tokio::test]
    async fn names_already_in_their_language_are_kept() {
        let t = TestApp::new().await;
        // no card here needs a translation, so nothing calls ygoprodeck
        sqlx::query(
            "INSERT INTO card_data (id, en_name, ygopro_id, json_data)
                 VALUES (3, 'tri-horned dragon', 39111158, '{\"name\": \"Tri-Horned Dragon\"}');
             INSERT INTO card (id, name, en_name, code, card_data_id, is_proxy) VALUES
                 (1, 'Dragon à Trois Cornes', 'tri-horned dragon', 'RA05-FR001', 3, 0),
                 (2, 'Tri-Horned Dragon', 'tri-horned dragon', 'RA05-EN001', 3, 0),
                 (3, 'Tri-Horned Dragon', 'tri-horned dragon', 'SDY-001', 3, 0);",
        )
        .execute(&t.state.pool)
        .await
        .unwrap();

        let (translations, untranslated) = plan_translations(&t.state.pool, &t.state.http, None)
            .await
            .unwrap();
        assert!(translations.is_empty() && untranslated.is_empty());
    }

    /// A French-coded card still named in English, and its card data.
    async fn english_named_french_card(t: &TestApp) {
        sqlx::query(
            "INSERT INTO card_data (id, en_name, ygopro_id, json_data)
                 VALUES (3, 'tri-horned dragon', 39111158, '{\"name\": \"Tri-Horned Dragon\"}');
             INSERT INTO card (id, name, en_name, code, card_data_id, is_proxy)
                 VALUES (1, 'Tri-Horned Dragon', 'tri-horned dragon', 'RA05-FR001', 3, 0);",
        )
        .execute(&t.state.pool)
        .await
        .unwrap();
    }

    fn today() -> String {
        crate::store::db::today().to_string()
    }

    #[tokio::test]
    async fn remembered_translations_are_used_without_asking_ygoprodeck() {
        let t = TestApp::new().await;
        english_named_french_card(&t).await;
        sqlx::query("INSERT INTO card_translation VALUES (39111158, 'fr', 'Dragon à Trois Cornes', '2020-01-01')")
            .execute(&t.state.pool)
            .await
            .unwrap();

        clean(&t.state, false).await.unwrap();
        assert_eq!(
            t.text("SELECT name FROM card WHERE id = 1")
                .await
                .as_deref(),
            Some("Dragon à Trois Cornes"),
            "a name found, however old, is not asked again"
        );
    }

    #[tokio::test]
    async fn a_recent_missing_translation_is_not_asked_again() {
        let t = TestApp::new().await;
        english_named_french_card(&t).await;
        sqlx::query("INSERT INTO card_translation (ygopro_id, language, name, checked_at) VALUES (39111158, 'fr', NULL, ?)")
            .bind(today())
            .execute(&t.state.pool)
            .await
            .unwrap();

        let (translations, untranslated) = plan_translations(&t.state.pool, &t.state.http, None)
            .await
            .unwrap();
        assert!(translations.is_empty());
        assert_eq!(untranslated, ["Tri-Horned Dragon (RA05-FR001)"]);
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
