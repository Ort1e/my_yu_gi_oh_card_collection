//! `scrap-ban-list` (Django: `scrap_ban_list`).
//!
//! Imports the EU JSON lists, then the US HTML pages (see `domain::ban_list`)
//! as a fallback, for when the EU JSON lags behind (it stopped at May 2026 for
//! a while): a US list is only imported for a date no list was imported for,
//! even with `--all`. The US lists are the ones the US index links, plus those
//! asked for with `--date` (the index only links the current list). A new list's
//! US page can be online before its table is (a copy of the previous list's
//! table, as on 2026-10-19): a US list with the same cards and statuses as the
//! list before it is skipped.
//!
//! Differences from Django: the US source; only the current list and the lists not imported
//! yet are downloaded and written unless `--all`; a card name that can't be resolved to card
//! data is reported and skipped (Django crashed on it); a card listed twice
//! keeps its first status.

use std::collections::HashSet;

use anyhow::Context;
use chrono::NaiveDate;
use domain::ban_list::{
    list_url, parse_list, parse_options, parse_us_list, parse_us_list_dates, us_list_url, BanList,
    BanListError, OPTIONS_URL, US_INDEX_URL,
};

use super::finish;
use crate::store::card_data::{find, get_or_fetch};
use crate::store::db::begin_write;
use crate::AppState;

pub async fn scrap(
    state: &AppState,
    all: bool,
    dry_run: bool,
    dates: &[NaiveDate],
) -> anyhow::Result<()> {
    scrap_eu(state, all, dry_run).await?;
    scrap_us(state, dry_run, dates).await?;

    if dry_run {
        println!("dry run: nothing was saved");
    }
    Ok(())
}

async fn imported_dates(state: &AppState) -> sqlx::Result<HashSet<NaiveDate>> {
    Ok(
        sqlx::query_scalar::<_, String>("SELECT date FROM advanced_ban_list")
            .fetch_all(&state.pool)
            .await?
            .iter()
            .filter_map(|d| d.parse().ok())
            .collect(),
    )
}

async fn scrap_eu(state: &AppState, all: bool, dry_run: bool) -> anyhow::Result<()> {
    let options = parse_options(&get_text(state, OPTIONS_URL).await?)
        .context("parsing the list of ban lists")?;
    let last = options.last().map(|(number, _)| *number);
    let imported = imported_dates(state).await?;
    // Option labels are months ("May 2026"): skip the lists of months already
    // imported without downloading them (Konami's server is slow).
    let imported_months: HashSet<String> = imported
        .iter()
        .map(|d| d.format("%B %Y").to_string())
        .collect();
    println!(
        "EU: {} ban lists available, {} already imported",
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

        import(state, label, &list, dry_run, false).await?;
    }
    Ok(())
}

async fn scrap_us(state: &AppState, dry_run: bool, asked: &[NaiveDate]) -> anyhow::Result<()> {
    let linked = parse_us_list_dates(&get_text(state, US_INDEX_URL).await?);
    let imported = imported_dates(state).await?;
    let dates = us_dates_to_import(&linked, asked, &imported);
    println!(
        "US: {} lists linked, {} to import",
        linked.len(),
        dates.len()
    );
    for date in asked.iter().filter(|d| imported.contains(d)) {
        println!("{date}: already imported, the US list is not used");
    }

    for date in dates {
        let url = us_list_url(date);
        let list = match parse_us_list(date, &get_text(state, &url).await?) {
            Err(BanListError::NotAList) => {
                println!("{date}: no list at {url}");
                continue;
            }
            list => list.with_context(|| format!("parsing {url}"))?,
        };
        let label = format!("US list of {}", date.format("%B %-d, %Y"));
        import(state, &label, &list, dry_run, true).await?;
    }
    Ok(())
}

/// The US lists to download, oldest first: the linked and asked ones no list
/// was imported for.
fn us_dates_to_import(
    linked: &[NaiveDate],
    asked: &[NaiveDate],
    imported: &HashSet<NaiveDate>,
) -> Vec<NaiveDate> {
    let mut dates: Vec<NaiveDate> = linked
        .iter()
        .chain(asked)
        .filter(|d| !imported.contains(d))
        .copied()
        .collect();
    dates.sort();
    dates.dedup();
    dates
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

/// Write `list`, replacing the entries of its date. With `skip_unchanged`, a
/// list with the same entries as the list before it is not written.
async fn import(
    state: &AppState,
    label: &str,
    list: &BanList,
    dry_run: bool,
    skip_unchanged: bool,
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
    if skip_unchanged {
        if let Some(previous) = same_as_previous(state, &date, &entries).await? {
            println!(
                "{label} ({date}): same cards as the list of {previous}, \
                 skipped (page not updated yet?)"
            );
            return Ok(());
        }
    }

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

/// The date of the list before `date`, when it has exactly `entries`.
async fn same_as_previous(
    state: &AppState,
    date: &str,
    entries: &[(i64, &str)],
) -> sqlx::Result<Option<String>> {
    let previous: Option<(i64, String)> =
        sqlx::query_as("SELECT id, date FROM advanced_ban_list WHERE date < ? ORDER BY date DESC LIMIT 1")
            .bind(date)
            .fetch_optional(&state.pool)
            .await?;
    let Some((id, previous_date)) = previous else {
        return Ok(None);
    };

    let previous_entries: HashSet<(i64, String)> =
        sqlx::query_as("SELECT card_data_id, status FROM ban_list_entry WHERE ban_list_id = ?")
            .bind(id)
            .fetch_all(&state.pool)
            .await?
            .into_iter()
            .collect();
    let entries: HashSet<(i64, String)> = entries
        .iter()
        .map(|(id, status)| (*id, status.to_string()))
        .collect();
    Ok((!entries.is_empty() && entries == previous_entries).then_some(previous_date))
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

    fn date(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    #[test]
    fn picks_the_us_lists_to_import() {
        let linked = [date(5, 18), date(9, 21), date(2, 2)];
        let imported: HashSet<NaiveDate> = [date(5, 18), date(9, 21)].into();

        // only the dates no list was imported for, asked or linked
        assert_eq!(
            us_dates_to_import(&linked, &[date(10, 19), date(5, 18)], &imported),
            vec![date(2, 2), date(10, 19)]
        );
        assert_eq!(
            us_dates_to_import(&[date(10, 19)], &[date(10, 19)], &imported),
            vec![date(10, 19)]
        );
    }

    #[tokio::test]
    async fn imports_and_replaces_a_list() {
        let t = TestApp::new().await;
        let first = list(&[
            ("Dark Magician", BanStatus::Banned),
            ("Kuriboh", BanStatus::Limited),
            ("dark magician", BanStatus::Limited), // listed twice: first status wins
        ]);
        import(&t.state, "May 2026", &first, false, false)
            .await
            .unwrap();

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
    async fn skips_a_list_unchanged_since_the_previous_one() {
        let t = TestApp::new().await;
        let may = list(&[("Kuriboh", BanStatus::Limited)]);
        import(&t.state, "May 2026", &may, false, false)
            .await
            .unwrap();

        let copy = BanList {
            date: date(9, 21),
            ..may.clone()
        };
        import(&t.state, "US list", &copy, false, true)
            .await
            .unwrap();
        assert_eq!(t.scalar("SELECT count(*) FROM advanced_ban_list").await, 1);

        let changed = BanList {
            entries: vec![("Kuriboh".to_string(), BanStatus::Banned)],
            ..copy
        };
        import(&t.state, "US list", &changed, false, true)
            .await
            .unwrap();
        assert_eq!(t.scalar("SELECT count(*) FROM advanced_ban_list").await, 2);
    }

    #[tokio::test]
    async fn dry_run_skips_unknown_names_and_saves_nothing() {
        let t = TestApp::new().await;
        let entries = list(&[
            ("Unknown Card", BanStatus::Banned),
            ("Kuriboh", BanStatus::Limited),
        ]);

        import(&t.state, "May 2026", &entries, true, false)
            .await
            .unwrap();

        assert_eq!(t.scalar("SELECT count(*) FROM advanced_ban_list").await, 0);
        assert_eq!(
            t.scalar("SELECT count(*) FROM card_data").await,
            2,
            "no API call"
        );
    }
}
