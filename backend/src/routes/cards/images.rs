//! Card images (Django: `ImageDownloader`). Images are cached in
//! `<media>/card_images/<name>.jpg`; a missing one is downloaded once from the
//! ygoprodeck URL stored in `card_data.json_data` (ygoprodeck asks not to
//! hotlink their images).

use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use domain::card::clean_card_name;

use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::AppState;

/// Same file name as Django: cleaned name, spaces → underscores.
fn file_name(en_name: &str) -> String {
    let name = clean_card_name(en_name)
        .replace(' ', "_")
        .replace(['/', '\\'], "_");
    format!("{name}.jpg")
}

pub async fn card_image(
    State(state): State<AppState>,
    Path(card_data_id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let (en_name, json_data): (String, String) =
        sqlx::query_as("SELECT en_name, json_data FROM card_data WHERE id = ?")
            .bind(card_data_id)
            .fetch_optional(&state.pool)
            .await?
            .or_404()?;

    let dir = state.media_dir.join("card_images");
    let path = dir.join(file_name(&en_name));
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(_) => download(&state, &json_data, dir, path).await?,
    };

    Ok((
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    ))
}

async fn download(
    state: &AppState,
    json_data: &str,
    dir: PathBuf,
    path: PathBuf,
) -> AppResult<Vec<u8>> {
    let json: serde_json::Value = serde_json::from_str(json_data)
        .map_err(|e| AppError::Internal(format!("bad card json: {e}")))?;
    let url = json["card_images"][0]["image_url"]
        .as_str()
        .ok_or(AppError::NotFound)?;

    let internal = |e: reqwest::Error| AppError::Internal(format!("downloading {url}: {e}"));
    let bytes = state
        .http
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(internal)?
        .bytes()
        .await
        .map_err(internal)?
        .to_vec();

    // Write to a temporary file then rename, so a concurrent request never
    // reads a half-written image.
    let io = |e: std::io::Error| AppError::Internal(format!("saving {}: {e}", path.display()));
    tokio::fs::create_dir_all(&dir).await.map_err(io)?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    tokio::fs::write(&tmp, &bytes).await.map_err(io)?;
    tokio::fs::rename(&tmp, &path).await.map_err(io)?;
    tracing::info!("downloaded card image {}", path.display());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_match_django() {
        assert_eq!(
            file_name("ash blossom & joyous spring"),
            "ash_blossom_&_joyous_spring.jpg"
        );
        assert_eq!(file_name("Dark Magician (V.2)"), "dark_magician.jpg");
        assert_eq!(file_name("a/b"), "a_b.jpg");
    }
}
