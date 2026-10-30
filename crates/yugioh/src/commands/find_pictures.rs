//! `find-pictures`: give each copy the picture of its own printing from
//! Yugipedia (its language, its art) when there is no doubt
//! (`store::yugipedia::find_pictures`); the others keep ygoprodeck's picture
//! until one is chosen on their edit page. New cards get theirs when they
//! are linked; this is for the cards added before, and for the scans
//! Yugipedia adds later. A gallery is asked again after 30 days.
//! `--dry-run` asks no gallery, it works with the cached ones.

use crate::store::yugipedia::find_pictures;
use crate::AppState;

/// The `find-pictures` command: run, then print what was done.
pub async fn find(state: &AppState, dry_run: bool) -> anyhow::Result<()> {
    let report = find_pictures(state, None, dry_run, usize::MAX).await?;
    println!("galleries asked to Yugipedia: {}", report.asked);
    println!("copies given their printing's picture: {}", report.found);
    if !report.to_choose.is_empty() {
        let ids: Vec<String> = report.to_choose.iter().map(i64::to_string).collect();
        println!(
            "{} copies with several artworks, to choose on their edit page: {}",
            report.to_choose.len(),
            ids.join(", ")
        );
    }
    if report.waiting > 0 {
        println!("copies whose gallery is still to ask: {}", report.waiting);
    }
    if dry_run {
        println!("dry run: nothing was saved");
    }
    Ok(())
}
