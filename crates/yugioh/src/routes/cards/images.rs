//! Card images (Django: `ImageDownloader`). Images are cached in
//! `<media>/card_images/<name>.jpg`; a missing one is downloaded once from the
//! ygoprodeck URL stored in `card_data.json_data` (ygoprodeck asks not to
//! hotlink their images).
//!
//! A copy can have its printing's own picture instead (`card.picture`, from
//! Yugipedia: `store::yugipedia`), served by `/card-picture/:card_id` and
//! kept in `<media>/card_pictures/` as a JPEG; the edit page shows the
//! pictures it can choose from with `/cards/:id/pictures/:n`.

use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use common::{media, photos};
use yugioh_domain::card::clean_card_name;
use yugioh_domain::printing_picture;
use yugioh_domain::rarity::Rarity;

use crate::store::db::Pool;
use crate::store::yugipedia;
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::AppState;

/// Same file name as Django: cleaned name, spaces → underscores.
pub(crate) fn file_name(en_name: &str) -> String {
    let name = clean_card_name(en_name)
        .replace(' ', "_")
        .replace(['/', '\\'], "_");
    format!("{name}.jpg")
}

/// The picture files the cards use (those of the card data linked to a
/// card, proxies included): the others are a cache to clean.
pub async fn used_files(pool: &Pool) -> sqlx::Result<HashSet<String>> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT en_name FROM card_data
         WHERE id IN (SELECT card_data_id FROM card WHERE card_data_id IS NOT NULL)",
    )
    .fetch_all(pool)
    .await?;
    Ok(names.iter().map(|name| file_name(name)).collect())
}

/// The copies' own picture files in use (`card_pictures`): the others are a
/// cache to clean (the pictures offered on an edit page and not chosen).
pub async fn used_picture_files(pool: &Pool) -> sqlx::Result<HashSet<String>> {
    let files: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT picture FROM card WHERE picture IS NOT NULL")
            .fetch_all(pool)
            .await?;
    Ok(files.iter().map(|f| yugipedia::file_name(f)).collect())
}

/// The pictures a copy coded `code` can choose from, among its card data's
/// gallery `files`: those of its printing.
pub fn offered(files: &[String], code: &str, rarity: Option<&str>) -> Vec<String> {
    let rarity = rarity.and_then(Rarity::from_label);
    printing_picture::candidates(files, code, rarity)
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// A Yugipedia file, downloaded once and kept as a JPEG.
async fn serve_file(state: &AppState, file: &str) -> AppResult<impl IntoResponse> {
    let path = yugipedia::file_path(state, file);
    if !path.exists() {
        // a page of new pictures (the public copy) asks for many at once
        yugipedia::YUGIPEDIA.wait().await;
    }
    let bytes = media::fetch_once_as(
        state,
        &path,
        || Ok(yugipedia::file_url(file)),
        |bytes| {
            photos::web_copy(&bytes).map_err(|e| AppError::Internal(format!("{file}: {}", e.0)))
        },
    )
    .await?;
    Ok(media::picture(bytes))
}

/// `GET /card-picture/:card_id`: the copy's own picture.
pub async fn card_picture(
    State(state): State<AppState>,
    Path(card_id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let file: String =
        sqlx::query_scalar("SELECT picture FROM card WHERE id = ? AND picture IS NOT NULL")
            .bind(card_id)
            .fetch_optional(&state.pool)
            .await?
            .or_404()?;
    serve_file(&state, &file).await
}

/// `GET /cards/:id/pictures/:n` (login): the `n`-th picture its edit page
/// offers, from the cached gallery (the edit page fetched it).
pub async fn offered_picture(
    State(state): State<AppState>,
    Path((id, n)): Path<(i64, usize)>,
) -> AppResult<impl IntoResponse> {
    let (card_data_id, code, rarity): (Option<i64>, Option<String>, Option<String>) =
        sqlx::query_as("SELECT card_data_id, code, rarity FROM card WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .or_404()?;
    let files = match card_data_id {
        Some(data_id) => yugipedia::cached(&state.pool, data_id, false)
            .await?
            .flatten()
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let offered = offered(&files, code.as_deref().unwrap_or(""), rarity.as_deref());
    let file = offered.get(n).or_404()?;
    serve_file(&state, file).await
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
