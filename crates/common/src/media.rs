//! Pictures kept in the media folder: downloaded once from their URL, then
//! served from there (card pictures: ygoprodeck asks not to hotlink theirs,
//! and TCGdex's are big).

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

    // a temporary file then a rename: a concurrent request never reads a
    // half-written picture
    let io = |e: std::io::Error| AppError::Internal(format!("saving {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(io)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    tokio::fs::write(&tmp, &bytes).await.map_err(io)?;
    tokio::fs::rename(&tmp, path).await.map_err(io)?;
    tracing::info!("downloaded {}", path.display());
    Ok(bytes)
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

    #[test]
    fn types_come_from_the_bytes() {
        assert_eq!(content_type(b"\x89PNG\r\n"), "image/png");
        assert_eq!(content_type(b"\xFF\xD8\xFF\xE0"), "image/jpeg");
        assert_eq!(content_type(b"RIFF....WEBPVP8 "), "image/webp");
        assert_eq!(content_type(b"<html>"), "application/octet-stream");
    }
}
