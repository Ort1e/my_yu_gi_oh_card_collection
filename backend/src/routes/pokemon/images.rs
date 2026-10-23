//! Pokémon card pictures: `/pokemon/card-image/:card_id/:lang`, the French
//! print's picture for `fr` when TCGdex has it, else the English (or
//! Japanese) one. Downloaded once from TCGdex (`{image}/high.webp`) into
//! `<media>/pokemon_card_images/`, then served from there. A card TCGdex
//! has no picture of borrows one (`cards::borrow_image`), once.

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;

use crate::store::pokemon::{cards, tcgdex};

/// `(catalogue, tcgdex_id, image, image_fr, image_from)`.
type Picture = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::AppState;

/// `ja-SV6a-010-base.webp`, `intl-sv06.5-067-fr.webp`.
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
    format!("{catalogue}-{id}-{print}.webp")
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
        (None, None, None) => cards::borrow_image(&state.pool, &state.http, card_id).await?,
        (image, ..) => image,
    };
    let borrowed = image_from.as_deref().is_some_and(|from| !from.is_empty());
    let (url, print) = match (language.as_str(), image_fr) {
        ("fr", Some(url)) => (url, "fr"),
        (_, fr) => (
            image.or(fr).ok_or(AppError::NotFound)?,
            if borrowed { "borrowed" } else { "base" },
        ),
    };
    let dir = state.media_dir.join("pokemon_card_images");
    let path = dir.join(file_name(&catalogue, &tcgdex_id, print));
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(_) => {
            let bytes = tcgdex::image(&state.http, &url).await?;
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
            (header::CONTENT_TYPE, "image/webp"),
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
            "intl-sv06.5-067-fr.webp"
        );
        assert_eq!(file_name("ja", "SV-P/001", "base"), "ja-SV-P_001-base.webp");
    }
}
