//! Maintenance commands, run with the server binary (Django's `manage.py`):
//!
//!   cargo run -p backend -- scrap-ban-list [--all] [--dry-run] [--date YYYY-MM-DD]…
//!   cargo run -p backend -- clean-cards [--dry-run]
//!
//! `--dry-run` does the work in a transaction that is rolled back and makes
//! no API call (names unknown locally are only reported).

mod ban_list;
mod clean_cards;

pub use clean_cards::spawn_daily as spawn_daily_cleanup;

use chrono::NaiveDate;

use crate::AppState;

pub const USAGE: &str = "usage:
  backend                                   run the web server
  backend scrap-ban-list [--all] [--dry-run] [--date YYYY-MM-DD]…
      import Konami's Forbidden & Limited lists: the EU JSON's current one and
      those not imported yet (--all: re-import every EU list), then, as a
      fallback, the US pages of dates still missing (--date: also the US list
      of that date, which the US index may not link yet)
  backend clean-cards [--dry-run]
      normalise card names, link cards to the right card data, replace
      proxies in decks by owned copies and delete unused proxies";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Serve,
    ScrapBanList {
        all: bool,
        dry_run: bool,
        /// US lists asked for by date (`--date`, repeatable).
        dates: Vec<NaiveDate>,
    },
    CleanCards {
        dry_run: bool,
    },
}

impl Command {
    /// Parse the arguments after the program name.
    pub fn parse(args: &[String]) -> Result<Command, String> {
        let Some((name, flags)) = args.split_first() else {
            return Ok(Command::Serve);
        };
        let mut flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        let dates = if name == "scrap-ban-list" {
            take_dates(&mut flags)?
        } else {
            Vec::new()
        };
        let allowed: &[&str] = match name.as_str() {
            "serve" => &[],
            "scrap-ban-list" => &["--all", "--dry-run"],
            "clean-cards" => &["--dry-run"],
            other => return Err(format!("unknown command {other:?}")),
        };
        if let Some(bad) = flags.iter().find(|f| !allowed.contains(f)) {
            return Err(format!("unknown option {bad:?} for {name}"));
        }
        let has = |flag: &str| flags.contains(&flag);
        Ok(match name.as_str() {
            "scrap-ban-list" => Command::ScrapBanList {
                all: has("--all"),
                dry_run: has("--dry-run"),
                dates,
            },
            "clean-cards" => Command::CleanCards {
                dry_run: has("--dry-run"),
            },
            _ => Command::Serve,
        })
    }
}

/// Remove the `--date YYYY-MM-DD` pairs from `flags` and parse their dates.
fn take_dates(flags: &mut Vec<&str>) -> Result<Vec<NaiveDate>, String> {
    let mut dates = Vec::new();
    while let Some(i) = flags.iter().position(|f| *f == "--date") {
        let value = flags.get(i + 1).ok_or("--date needs a date (YYYY-MM-DD)")?;
        let date = value
            .parse()
            .map_err(|_| format!("invalid date {value:?} for --date (YYYY-MM-DD)"))?;
        dates.push(date);
        flags.drain(i..=i + 1);
    }
    Ok(dates)
}

/// Run a maintenance command (anything but `Serve`).
pub async fn run(command: Command, state: &AppState) -> anyhow::Result<()> {
    match command {
        Command::Serve => unreachable!("the server is started by main"),
        Command::ScrapBanList {
            all,
            dry_run,
            dates,
        } => ban_list::scrap(state, all, dry_run, &dates).await,
        Command::CleanCards { dry_run } => clean_cards::clean(state, dry_run).await,
    }
}

/// Commit, or roll back for a dry run.
async fn finish(tx: sqlx::Transaction<'_, sqlx::Sqlite>, dry_run: bool) -> sqlx::Result<()> {
    if dry_run {
        tx.rollback().await
    } else {
        tx.commit().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        Command::parse(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn parses_commands() {
        assert_eq!(parse(&[]), Ok(Command::Serve));
        assert_eq!(parse(&["serve"]), Ok(Command::Serve));
        assert_eq!(
            parse(&["scrap-ban-list", "--dry-run"]),
            Ok(Command::ScrapBanList {
                all: false,
                dry_run: true,
                dates: vec![],
            })
        );
        assert_eq!(
            parse(&[
                "scrap-ban-list",
                "--date",
                "2026-10-19",
                "--all",
                "--date",
                "2026-05-18"
            ]),
            Ok(Command::ScrapBanList {
                all: true,
                dry_run: false,
                dates: vec![
                    NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(),
                    NaiveDate::from_ymd_opt(2026, 5, 18).unwrap(),
                ],
            })
        );
        assert!(parse(&["scrap-ban-list", "--date"]).is_err());
        assert!(parse(&["scrap-ban-list", "--date", "19/10/2026"]).is_err());
        assert!(parse(&["clean-cards", "--date", "2026-10-19"]).is_err());
        assert_eq!(
            parse(&["clean-cards"]),
            Ok(Command::CleanCards { dry_run: false })
        );
        assert!(parse(&["clean-cards", "--all"]).is_err());
        assert!(parse(&["nope"]).is_err());
    }
}
