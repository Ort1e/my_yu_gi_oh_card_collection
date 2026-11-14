//! `/books/:id/cover`: a book's cover, kept in `media/book_covers/` so the
//! public copy can hold it without its ISBN (the Open Library address has
//! it): `<id>.jpg` once downloaded, `<id>.none` when Open Library has none
//! (remembered: the export doesn't ask again each time). Delete both to ask
//! again, e.g. after the ISBN was corrected. The app's own pages keep
//! showing Open Library's address (see [`super::cover_url`]).

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use common::http::Throttle;
use common::media;
use common::web::error::{AppError, AppResult, OrNotFound};
use common::AppState;

/// Open Library allows about 100 cover requests per 5 minutes by ISBN.
static OPEN_LIBRARY: Throttle = Throttle::new(Duration::from_secs(4));

const TIMEOUT: Duration = Duration::from_secs(30);

fn folder(state: &AppState) -> PathBuf {
    state.media_dir.join("book_covers")
}

/// The files of the books' covers, for `clean-pictures`.
pub async fn used_files(pool: &common::db::Pool) -> sqlx::Result<HashSet<String>> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM book")
        .fetch_all(pool)
        .await?;
    Ok(ids
        .into_iter()
        .flat_map(|id| [format!("{id}.jpg"), format!("{id}.none")])
        .collect())
}

pub async fn cover(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let isbn: Option<String> = sqlx::query_scalar("SELECT isbn FROM book WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .or_404()?;
    let isbn = isbn.or_404()?;

    let path = folder(&state).join(format!("{id}.jpg"));
    if let Ok(bytes) = tokio::fs::read(&path).await {
        return Ok(media::picture(bytes));
    }
    let none = folder(&state).join(format!("{id}.none"));
    if tokio::fs::try_exists(&none).await.unwrap_or(false) {
        return Err(AppError::NotFound);
    }

    OPEN_LIBRARY.wait().await;
    let url = format!("https://covers.openlibrary.org/b/isbn/{isbn}-M.jpg?default=false");
    let response = state
        .http
        .get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("book cover: {}", e.without_url())))?;

    // no cover there: remembered
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        media::save(&none, b"").await?;
        return Err(AppError::NotFound);
    }
    let bytes = response
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("book cover: {}", e.without_url())))?
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("book cover: {}", e.without_url())))?
        .to_vec();
    media::save(&path, &bytes).await?;
    Ok(media::picture(bytes))
}
