//! Pokémon card pictures: `/pokemon/card-image/:card_id/:lang`, the French
//! print's picture for `fr` when TCGdex has it, else the English (or
//! Japanese) one. Downloaded once from TCGdex (`{image}/high.webp`) into
//! `<media>/pokemon_card_images/`, then served from there. A card TCGdex
//! has no picture of gets the same print's from the Pokémon TCG API
//! (`cards::find_image`, a full URL, PNG): looked up once.

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;

use crate::store::cards;
use common::web::error::{AppError, AppResult, OrNotFound};
use common::AppState;

/// `(catalogue, tcgdex_id, image, image_fr, image_from)`.
type Picture = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

const TCGDEX_ASSETS: &str = "https://assets.tcgdex.net/";

/// `ja-SV6a-010-base.img`, `intl-sv06.5-067-fr.img`.
fn file_name(catalogue: &str, tcgdex_id: &str, print: &str) -> String {
    let id: String = tcgdex_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{catalogue}-{id}-{print}.img")
}

/// The picture's type, from its first bytes (WebP from TCGdex, PNG from
/// the Pokémon TCG API).
fn content_type(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, ..] => "image/jpeg",
        _ => "image/webp",
    }
}

async fn download(state: &AppState, url: &str) -> AppResult<Vec<u8>> {
    // a TCGdex picture is a base URL: its quality and format are added
    let url = if url.starts_with(TCGDEX_ASSETS) {
        format!("{url}/high.webp")
    } else {
        url.to_string()
    };
    let fail = |e: reqwest::Error| AppError::Internal(format!("downloading {url}: {e}"));
    let bytes = state
        .http
        .get(&url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(fail)?
        .bytes()
        .await
        .map_err(fail)?;
    Ok(bytes.to_vec())
}

pub async fn card_image(
    State(state): State<AppState>,
    Path((card_id, language)): Path<(i64, String)>,
) -> AppResult<impl IntoResponse> {
    let (catalogue, tcgdex_id, image, image_fr, image_from): Picture = sqlx::query_as(
        "SELECT catalogue, tcgdex_id, image, image_fr, image_from FROM pokemon_card WHERE id = ?",
    )
    .bind(card_id)
    .fetch_optional(&state.pool)
    .await?
    .or_404()?;

    let image = match (image, &image_fr, &image_from) {
        (None, None, None) => cards::find_image(&state.pool, &state.http, card_id).await?,
        (image, ..) => image,
    };
    let (url, print) = match (language.as_str(), image_fr) {
        ("fr", Some(url)) => (url, "fr"),
        (_, fr) => {
            let url = image.or(fr).ok_or(AppError::NotFound)?;
            let print = if url.starts_with(TCGDEX_ASSETS) {
                "base"
            } else {
                "other"
            };
            (url, print)
        }
    };
    let dir = state.media_dir.join("pokemon_card_images");
    let path = dir.join(file_name(&catalogue, &tcgdex_id, print));
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(_) => {
            let bytes = download(&state, &url).await?;
            // a temporary file then a rename: a concurrent request never
            // reads a half-written picture
            let io =
                |e: std::io::Error| AppError::Internal(format!("saving {}: {e}", path.display()));
            tokio::fs::create_dir_all(&dir).await.map_err(io)?;
            let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
            tokio::fs::write(&tmp, &bytes).await.map_err(io)?;
            tokio::fs::rename(&tmp, &path).await.map_err(io)?;
            bytes
        }
    };

    Ok((
        [
            (header::CONTENT_TYPE, content_type(&bytes)),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(
            file_name("intl", "sv06.5-067", "fr"),
            "intl-sv06.5-067-fr.img"
        );
        assert_eq!(file_name("ja", "SV-P/001", "base"), "ja-SV-P_001-base.img");
        assert_eq!(content_type(b"\x89PNG\r\n"), "image/png");
        assert_eq!(content_type(b"RIFF....WEBP"), "image/webp");
    }
}
