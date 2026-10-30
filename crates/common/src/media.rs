//! Pictures kept in the media folder: downloaded once from their URL, then
//! served from there (card pictures: ygoprodeck asks not to hotlink theirs,
//! and TCGdex's are big). A cache: the files no card uses any more are
//! deleted ([`remove_unused`]), and downloaded again if ever needed.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use axum::http::header;
use axum::response::IntoResponse;

use crate::web::error::{AppError, AppResult};
use crate::AppState;

const TIMEOUT: Duration = Duration::from_secs(30);

/// The picture saved at `path`, downloaded the first time from the URL
/// `url()` gives (only called then).
pub async fn fetch_once(
    state: &AppState,
    path: &Path,
    url: impl FnOnce() -> AppResult<String>,
) -> AppResult<Vec<u8>> {
    if let Ok(bytes) = tokio::fs::read(path).await {
        return Ok(bytes);
    }

    let url = url()?;
    let fail = |e: reqwest::Error| AppError::Internal(format!("downloading {url}: {e}"));
    let bytes = state
        .http
        .get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(fail)?
        .bytes()
        .await
        .map_err(fail)?
        .to_vec();
    save(path, &bytes).await?;
    tracing::info!("downloaded {}", path.display());
    Ok(bytes)
}

/// Write a downloaded picture at `path` (its folder created): a temporary
/// file then a rename, so a concurrent request never reads a half-written
/// picture.
pub async fn save(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let io = |e: std::io::Error| AppError::Internal(format!("saving {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(io)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    tokio::fs::write(&tmp, bytes).await.map_err(io)?;
    tokio::fs::rename(&tmp, path).await.map_err(io)?;
    Ok(())
}

/// What [`remove_unused`] found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Unused {
    pub files: usize,
    pub bytes: u64,
}

/// A download still being written (`<name>.<pid>.tmp`, renamed when done)
/// is left alone for this long.
const TMP_GRACE: Duration = Duration::from_secs(3600);

/// Delete the files of `dir` whose name isn't in `used` (`dry_run`: only
/// count them). A missing folder has nothing to delete.
pub async fn remove_unused(
    dir: &Path,
    used: &HashSet<String>,
    dry_run: bool,
) -> std::io::Result<Unused> {
    let mut unused = Unused::default();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(unused),
        Err(e) => return Err(e),
    };

    while let Some(entry) = entries.next_entry().await? {
        let metadata = entry.metadata().await?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !metadata.is_file() || used.contains(&name) {
            continue;
        }

        let recent = metadata
            .modified()
            .ok()
            .and_then(|at| at.elapsed().ok())
            .is_some_and(|age| age < TMP_GRACE);
        if name.ends_with(".tmp") && recent {
            continue;
        }

        if !dry_run {
            tokio::fs::remove_file(entry.path()).await?;
        }
        unused.files += 1;
        unused.bytes += metadata.len();
    }
    Ok(unused)
}

/// A picture's type, from its first bytes.
pub fn content_type(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, ..] => "image/jpeg",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "application/octet-stream",
    }
}

/// A picture as an answer: its type, kept a day by the browsers.
pub fn picture(bytes: Vec<u8>) -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, content_type(&bytes)),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unused_files_are_deleted() {
        let dir = std::env::temp_dir().join(format!("media-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["kept.jpg", "old.jpg", "old.webp"] {
            std::fs::write(dir.join(name), "abc").unwrap();
        }
        let used = HashSet::from(["kept.jpg".to_string()]);

        let counted = remove_unused(&dir, &used, true).await.unwrap();
        assert_eq!(counted, Unused { files: 2, bytes: 6 });
        assert!(dir.join("old.jpg").exists());

        assert_eq!(remove_unused(&dir, &used, false).await.unwrap(), counted);
        let mut left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["kept.jpg"]);

        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            remove_unused(&dir, &used, false).await.unwrap(),
            Unused::default()
        );
    }

    #[test]
    fn types_come_from_the_bytes() {
        assert_eq!(content_type(b"\x89PNG\r\n"), "image/png");
        assert_eq!(content_type(b"\xFF\xD8\xFF\xE0"), "image/jpeg");
        assert_eq!(content_type(b"RIFF....WEBPVP8 "), "image/webp");
        assert_eq!(content_type(b"<html>"), "application/octet-stream");
    }
}
