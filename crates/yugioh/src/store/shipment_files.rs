//! The shipment PDFs of the lots, as files. An uploaded PDF waits as
//! `<uploads>/pending-<token>.pdf` (deleted after 24 h) until its lot is
//! saved, then moves to `<media>/shipment_files/shipment_<lot id>.pdf`.
//!
//! Only the files are handled here; their text (personal data) is parsed in
//! `yugioh_domain::cardmarket` and never stored or logged.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

use regex::Regex;

use crate::web::error::{AppError, AppResult};

static TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[0-9a-f]{16}$").unwrap());
const PENDING_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

/// The pending file of `token`, `None` for a malformed token.
pub fn pending_path(uploads: &Path, token: &str) -> Option<PathBuf> {
    TOKEN
        .is_match(token)
        .then(|| uploads.join(format!("pending-{token}.pdf")))
}

/// Keep an uploaded PDF until its lot is saved. Returns its token.
pub async fn save_pending(uploads: &Path, pdf: &[u8]) -> AppResult<String> {
    remove_stale(uploads).await;

    let token = format!("{:016x}", rand::random::<u64>());
    let path = pending_path(uploads, &token).expect("valid token");
    let io = |e: std::io::Error| AppError::Internal(format!("saving upload: {e}"));
    tokio::fs::create_dir_all(uploads).await.map_err(io)?;
    tokio::fs::write(&path, pdf).await.map_err(io)?;
    Ok(token)
}

/// Remove pending uploads that were never turned into a lot.
async fn remove_stale(uploads: &Path) {
    let Ok(mut dir) = tokio::fs::read_dir(uploads).await else {
        return;
    };
    while let Ok(Some(entry)) = dir.next_entry().await {
        let stale = entry
            .metadata()
            .await
            .and_then(|m| m.modified())
            .map(|t| SystemTime::now().duration_since(t).unwrap_or_default() > PENDING_MAX_AGE)
            .unwrap_or(false);
        if stale && entry.file_name().to_string_lossy().starts_with("pending-") {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

/// A pending PDF moved to its lot's place.
pub struct Attached {
    /// Path relative to the media dir, as stored in `lot.shipment_file`.
    pub relative: String,
    from: PathBuf,
    to: PathBuf,
}

impl Attached {
    /// Put the file back where it was (the lot was not saved).
    pub async fn undo(self) {
        let _ = tokio::fs::rename(self.to, self.from).await;
    }
}

/// Move the pending PDF `from` to the shipment file of `lot_id`.
pub async fn attach(media: &Path, lot_id: i64, from: PathBuf) -> AppResult<Attached> {
    let (relative, to) = destination(media, lot_id);
    let io = |e: std::io::Error| AppError::Internal(format!("moving shipment file: {e}"));
    tokio::fs::create_dir_all(to.parent().unwrap())
        .await
        .map_err(io)?;
    tokio::fs::rename(&from, &to).await.map_err(io)?;
    Ok(Attached { relative, from, to })
}

/// `shipment_files/shipment_<lot id>.pdf` (relative to the media dir) and its
/// absolute path; a numeric suffix is added if that file already exists.
fn destination(media: &Path, lot_id: i64) -> (String, PathBuf) {
    let mut n = 1;
    loop {
        let relative = if n == 1 {
            format!("shipment_files/shipment_{lot_id}.pdf")
        } else {
            format!("shipment_files/shipment_{lot_id}_{n}.pdf")
        };
        let path = media.join(&relative);
        if !path.exists() {
            return (relative, path);
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_tokens_are_validated() {
        let dir = Path::new("/tmp/uploads");
        assert!(pending_path(dir, "0123456789abcdef").is_some());
        assert!(pending_path(dir, "../../etc/passwd").is_none());
        assert!(pending_path(dir, "0123456789ABCDEF").is_none());
    }
}
