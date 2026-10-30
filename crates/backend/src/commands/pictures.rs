//! `clean-pictures`: delete the cached card pictures no card uses any more
//! (`media/card_images/`, `media/card_pictures/` (the printings' own),
//! `media/pokemon_card_images/`; `common::media`).
//! They are downloaded again if a card needs one later. The server also
//! does it once a day ([`spawn_daily`]).

use std::time::Duration;

use common::media::{self, Unused};
use common::{schedule, AppState};

/// Each picture folder with what was (or would be) deleted from it.
async fn clean_all(state: &AppState, dry_run: bool) -> anyhow::Result<Vec<(&'static str, Unused)>> {
    let folders = [
        (
            "card_images",
            yugioh::routes::used_card_pictures(&state.pool).await?,
        ),
        (
            "card_pictures",
            yugioh::routes::used_printing_pictures(&state.pool).await?,
        ),
        (
            "pokemon_card_images",
            pokemon::routes::images::used_files(&state.pool).await?,
        ),
    ];

    let mut reports = Vec::new();
    for (folder, used) in folders {
        let unused = media::remove_unused(&state.media_dir.join(folder), &used, dry_run).await?;
        reports.push((folder, unused));
    }
    Ok(reports)
}

fn summary(folder: &str, unused: &Unused) -> String {
    format!(
        "{folder}: {} unused pictures ({:.1} MB)",
        unused.files,
        unused.bytes as f64 / 1_000_000.0
    )
}

pub async fn clean(state: &AppState, dry_run: bool) -> anyhow::Result<()> {
    for (folder, unused) in clean_all(state, dry_run).await? {
        println!("{}", summary(folder, &unused));
    }

    if dry_run {
        println!("dry run: nothing was deleted");
    }
    Ok(())
}

/// After the daily card cleanup (5 minutes), which may unlink card data.
const START_DELAY: Duration = Duration::from_secs(15 * 60);
const INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// The daily cleanup of the pictures, logged when it deletes any.
pub fn spawn_daily(state: AppState) {
    schedule::every(state, START_DELAY, INTERVAL, |state| async move {
        match clean_all(&state, false).await {
            Ok(reports) => {
                for (folder, unused) in reports.iter().filter(|(_, u)| u.files > 0) {
                    tracing::info!("{}", summary(folder, unused));
                }
            }
            Err(e) => tracing::warn!("background clean-pictures: {e:#}"),
        }
    });
}
