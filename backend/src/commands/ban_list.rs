//! `scrap-ban-list` (Django: `scrap_ban_list`).
//!
//! Differences from Django: only the current list and the lists not imported
//! yet are downloaded and written unless `--all`; a card name that can't be resolved to card
//! data is reported and skipped (Django crashed on it); a card listed twice
//! keeps its first status.

use std::collections::HashSet;

use anyhow::Context;
use chrono::NaiveDate;
use domain::ban_list::{list_url, parse_list, parse_options, BanList, OPTIONS_URL};

use super::finish;
use crate::card_data::{find, get_or_fetch};
use crate::db::begin_write;
use crate::AppState;

pub async fn scrap(state: &AppState, all: bool, dry_run: bool) -> anyhow::Result<()> {
    let options = parse_options(&get_text(state, OPTIONS_URL).await?)
        .context("parsing the list of ban lists")?;
    let last = options.last().map(|(number, _)| *number);
    let imported: HashSet<NaiveDate> =
        sqlx::query_scalar::<_, String>("SELECT date FROM advanced_ban_list")
            .fetch_all(&state.pool)
            .await?
            .iter()
            .filter_map(|d| d.parse().ok())
            .collect();
    // Option labels are months ("May 2026"): skip the lists of months already
    // imported without downloading them (Konami's server is slow).
    let imported_months: HashSet<String> = imported
        .iter()
        .map(|d| d.format("%B %Y").to_string())
        .collect();
    println!(
        "{} ban lists available, {} already imported",
        options.len(),
        imported.len()
    );

    for (number, label) in &options {
        let is_current = Some(*number) == last;
        let skip_known = !all && !is_current;

        if skip_known && imported_months.contains(label.trim()) {
            continue;
        }

        let url = list_url(*number, is_current);
        let list =
            parse_list(&get_text(state, &url).await?).with_context(|| format!("parsing {url}"))?;

        if skip_known && imported.contains(&list.date) {
            continue;
        }

        import(state, label, &list, dry_run).await?;
    }

    if dry_run {
        println!("dry run: nothing was saved");
    }
    Ok(())
}

async fn get_text(state: &AppState, url: &str) -> anyhow::Result<String> {
    Ok(state
        .http
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("downloading {url}"))?
        .text()
        .await?)
}

async fn import(
    state: &AppState,
    label: &str,
    list: &BanList,
    dry_run: bool,
) -> anyhow::Result<()> {
    // Resolve the names first: fetching card data writes through the pool,
    // which must not wait on our own write transaction.
    let mut entries: Vec<(i64, &str)> = Vec::new();
    let mut seen = HashSet::new();
    let mut unresolved = Vec::new();
    for (name, status) in &list.entries {
        let card_data_id = if dry_run {
            find(&state.pool, name).await?
        } else {
            get_or_fetch(&state.pool, &state.http, name).await?
        };
        match card_data_id {
            Some(id) if seen.insert(id) => entries.push((id, status.code())),
            Some(_) => {}
            None => unresolved.push(name.as_str()),
        }
    }

    let date = list.date.to_string();
    let mut tx = begin_write(&state.pool).await?;
    sqlx::query("INSERT INTO advanced_ban_list (date) VALUES (?) ON CONFLICT (date) DO NOTHING")
        .bind(&date)
        .execute(&mut *tx)
        .await?;
    let ban_list_id: i64 = sqlx::query_scalar("SELECT id FROM advanced_ban_list WHERE date = ?")
        .bind(&date)
        .fetch_one(&mut *tx)
        .await?;
    let before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ban_list_entry WHERE ban_list_id = ?")
            .bind(ban_list_id)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("DELETE FROM ban_list_entry WHERE ban_list_id = ?")
        .bind(ban_list_id)
        .execute(&mut *tx)
        .await?;
    for (card_data_id, status) in &entries {
        sqlx::query(
            "INSERT INTO ban_list_entry (ban_list_id, card_data_id, status) VALUES (?, ?, ?)",
        )
        .bind(ban_list_id)
        .bind(card_data_id)
        .bind(status)
        .execute(&mut *tx)
        .await?;
    }
    finish(tx, dry_run).await?;

    println!("{label} ({date}): {} cards (was {before})", entries.len());
    if !unresolved.is_empty() {
        let why = if dry_run {
            "not known locally"
        } else {
            "no card data found"
        };
        println!(
            "  {} skipped, {why}: {}",
            unresolved.len(),
            unresolved.join("; ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use domain::ban_list::BanStatus;

    use super::*;
    use crate::tests::TestApp;

    fn list(entries: &[(&str, BanStatus)]) -> BanList {
        BanList {
            date: NaiveDate::from_ymd_opt(2026, 5, 18).unwrap(),
            entries: entries.iter().map(|(n, s)| (n.to_string(), *s)).collect(),
        }
    }

    #[tokio::test]
    async fn imports_and_replaces_a_list() {
        let t = TestApp::new().await;
        let first = list(&[
            ("Dark Magician", BanStatus::Banned),
            ("Kuriboh", BanStatus::Limited),
            ("dark magician", BanStatus::Limited), // listed twice: first status wins
        ]);
        import(&t.state, "May 2026", &first, false).await.unwrap();

        assert_eq!(t.scalar("SELECT count(*) FROM ban_list_entry").await, 2);
        assert_eq!(
            t.text("SELECT status FROM ban_list_entry WHERE card_data_id = 1")
                .await
                .as_deref(),
            Some("Banned")
        );

        // re-importing the same date replaces its entries
        import(
            &t.state,
            "May 2026",
            &list(&[("Kuriboh", BanStatus::SemiLimited)]),
            false,
        )
        .await
        .unwrap();
        assert_eq!(t.scalar("SELECT count(*) FROM advanced_ban_list").await, 1);
        assert_eq!(
            t.text("SELECT status FROM ban_list_entry").await.as_deref(),
            Some("Semi-Limited")
        );
    }

    #[tokio::test]
    async fn dry_run_skips_unknown_names_and_saves_nothing() {
        let t = TestApp::new().await;
        let entries = list(&[
            ("Unknown Card", BanStatus::Banned),
            ("Kuriboh", BanStatus::Limited),
        ]);

        import(&t.state, "May 2026", &entries, true).await.unwrap();

        assert_eq!(t.scalar("SELECT count(*) FROM advanced_ban_list").await, 0);
        assert_eq!(
            t.scalar("SELECT count(*) FROM card_data").await,
            2,
            "no API call"
        );
    }
}
