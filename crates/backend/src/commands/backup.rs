//! `backup --to DIR [--keep DAYS]`: a copy of the data that can be made
//! while the server runs (the NAS does it every morning, onto its other
//! volume; deploy/README.md):
//!
//! - `DIR/app-YYYY-MM-DD.db`: the database, by SQLite's `VACUUM INTO` (a
//!   consistent copy, even with writes going on); again the same day
//!   replaces it;
//! - `DIR/media/`: the media folder mirrored (new and changed files copied;
//!   files deleted from the media stay in the copy);
//! - the database copies older than `DAYS` (14) deleted.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use common::db::today;
use common::AppState;

/// What a backup did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Report {
    pub database: PathBuf,
    pub media_copied: usize,
    pub old_copies_deleted: usize,
}

pub(crate) async fn backup(state: &AppState, to: &Path, keep_days: i64) -> anyhow::Result<Report> {
    std::fs::create_dir_all(to)?;
    let today = today();

    // the database: into a temporary file, then renamed (an interrupted
    // copy never looks like a good one)
    let database = to.join(format!("app-{today}.db"));
    let partial = to.join(format!("app-{today}.db.partial"));
    if partial.exists() {
        std::fs::remove_file(&partial)?;
    }
    sqlx::query("VACUUM INTO ?")
        .bind(partial.to_string_lossy().into_owned())
        .execute(&state.pool)
        .await?;
    std::fs::rename(&partial, &database)?;

    let media = state.media_dir.clone();
    let copy = to.join("media");
    let to = to.to_path_buf();
    let (media_copied, old_copies_deleted) = tokio::task::spawn_blocking(move || {
        anyhow::Ok((mirror(&media, &copy)?, forget_old(&to, today, keep_days)?))
    })
    .await??;

    Ok(Report {
        database,
        media_copied,
        old_copies_deleted,
    })
}

/// Copy the files of `from` missing from `to` or of another size: how
/// many. A download in progress (`*.tmp`) is skipped.
fn mirror(from: &Path, to: &Path) -> std::io::Result<usize> {
    let mut copied = 0;
    if !from.exists() {
        return Ok(0);
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let target = to.join(entry.file_name());
        if metadata.is_dir() {
            copied += mirror(&entry.path(), &target)?;
        } else if !entry.file_name().to_string_lossy().ends_with(".tmp")
            && std::fs::metadata(&target).map_or(true, |t| t.len() != metadata.len())
        {
            std::fs::copy(entry.path(), &target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// Delete the database copies (`app-YYYY-MM-DD.db`) more than `keep_days`
/// old: how many.
fn forget_old(dir: &Path, today: NaiveDate, keep_days: i64) -> std::io::Result<usize> {
    let mut deleted = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let date = name
            .strip_prefix("app-")
            .and_then(|n| n.strip_suffix(".db"))
            .and_then(|d| d.parse::<NaiveDate>().ok());
        if date.is_some_and(|date| (today - date).num_days() >= keep_days) {
            std::fs::remove_file(entry.path())?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

pub(crate) async fn run(state: &AppState, to: &Path, keep_days: i64) -> anyhow::Result<()> {
    let report = backup(state, to, keep_days).await?;
    println!(
        "{}: database copied, {} media files copied, {} old copies deleted",
        report.database.display(),
        report.media_copied,
        report.old_copies_deleted
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_copies_are_forgotten() {
        let dir = std::env::temp_dir().join(format!("backup-test-{}", rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "app-2026-10-01.db",
            "app-2026-10-12.db",
            "app-2026-10-13.db",
            "notes.txt",
        ] {
            std::fs::write(dir.join(name), "x").unwrap();
        }
        let today = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        assert_eq!(forget_old(&dir, today, 14).unwrap(), 2);

        let mut left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["app-2026-10-13.db", "notes.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn rand_suffix() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}
