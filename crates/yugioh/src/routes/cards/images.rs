//! Card images (Django: `ImageDownloader`). Images are cached in
//! `<media>/card_images/<name>.jpg`; a missing one is downloaded once from the
//! ygoprodeck URL stored in `card_data.json_data` (ygoprodeck asks not to
//! hotlink their images).

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use common::media;
use yugioh_domain::card::clean_card_name;

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

    let path = state
        .media_dir
        .join("card_images")
        .join(file_name(&en_name));
    let url = || {
        let json: serde_json::Value = serde_json::from_str(&json_data)
            .map_err(|e| AppError::Internal(format!("bad card json: {e}")))?;
        json["card_images"][0]["image_url"]
            .as_str()
            .map(str::to_string)
            .ok_or(AppError::NotFound)
    };
    Ok(media::picture(media::fetch_once(&state, &path, url).await?))
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
