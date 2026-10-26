//! Pokémon card pictures: `/pokemon/card-image/:card_id/:lang`, the French
//! print's picture for `fr` when TCGdex has it, else the English (or
//! Japanese) one. Downloaded once from TCGdex (`{image}/high.webp`) into
//! `<media>/pokemon_card_images/`, then served from there. A card TCGdex
//! has no picture of gets the same print's from the Pokémon TCG API
//! (`cards::find_image`, a full URL, PNG): looked up once.

use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::response::IntoResponse;

use crate::store::cards;
use common::db::Pool;
use common::media;
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

/// The picture files of the cards owned or wished (every print each may
/// show): the others are a cache to clean.
pub async fn used_files(pool: &Pool) -> sqlx::Result<HashSet<String>> {
    let cards: Vec<(String, String)> =
        sqlx::query_as("SELECT catalogue, tcgdex_id FROM pokemon_card")
            .fetch_all(pool)
            .await?;
    Ok(cards
        .iter()
        .flat_map(|(catalogue, tcgdex_id)| {
            ["base", "fr", "other"].map(|print| file_name(catalogue, tcgdex_id, print))
        })
        .collect())
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
    let path = state
        .media_dir
        .join("pokemon_card_images")
        .join(file_name(&catalogue, &tcgdex_id, print));
    // a TCGdex picture is a base URL: its quality and format are added
    let url = || {
        Ok(if url.starts_with(TCGDEX_ASSETS) {
            format!("{url}/high.webp")
        } else {
            url.clone()
        })
    };
    Ok(media::picture(media::fetch_once(&state, &path, url).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::media::content_type;

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
