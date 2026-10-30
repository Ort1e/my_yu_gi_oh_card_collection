//! `GET /books/api/isbn/:isbn`: what the catalogues know about an ISBN, for
//! the "Look up" button and the barcode scan of the book form
//! (`static/js/book_form.js`). Behind the login: it calls Open Library (and
//! writes its cache).
//!
//! 200 `{"isbn", "title", "authors": [..], "publisher", "year", "language",
//! "pages", "series", "volume", "format", "owned_book_id"}` (each may be
//! null; `owned_book_id`: the book already in the list with this ISBN);
//! 400 `{"error"}` for an invalid ISBN; 404 `{"error", "isbn",
//! "owned_book_id"}` when no catalogue knows it.
//!
//! `POST /books/api/isbn-scan` (multipart, `photo`): the ISBN on a photo of
//! the barcode (`crate::barcode`), for the "Scan" buttons: 200 `{"isbn"}`,
//! 422 `{"error"}` when no book barcode is found, 400 `{"error"}` when it
//! isn't a photo.

use axum::extract::multipart::MultipartError;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use books_domain::book::BookFormat;
use books_domain::isbn::Isbn;
use serde_json::json;

use crate::store::{book_data, books};
use common::web::error::{bad_request, AppError, AppResult};
use common::AppState;

pub async fn isbn(State(state): State<AppState>, Path(text): Path<String>) -> AppResult<Response> {
    let isbn = match Isbn::parse(&text) {
        Ok(isbn) => isbn,
        Err(why) => return bad_request(format!("\"{text}\": {why}.")),
    };

    let mut conn = state.pool.acquire().await?;
    let owned = books::with_isbn(&mut conn, &isbn).await?;
    drop(conn);

    let Some(info) = book_data::lookup(&state.pool, &state.http, &isbn).await? else {
        let body = json!({
            "error": format!("No catalogue knows the ISBN {isbn}: fill the fields by hand."),
            "isbn": isbn.as_str(),
            "owned_book_id": owned,
        });
        return Ok((StatusCode::NOT_FOUND, Json(body)).into_response());
    };

    Ok(Json(json!({
        "isbn": isbn.as_str(),
        "title": info.title,
        "authors": info.authors,
        "publisher": info.publisher,
        "year": info.year,
        "language": info.language,
        "pages": info.pages,
        "series": info.series,
        "volume": info.volume,
        "format": info.format.map(BookFormat::label),
        "owned_book_id": owned,
    }))
    .into_response())
}

/// `POST /books/api/isbn-scan`: read the ISBN on the photo sent.
pub async fn scan(mut multipart: Multipart) -> AppResult<Response> {
    let read = |e: MultipartError| AppError::Internal(format!("reading the photo: {e}"));
    let mut photo = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(read)? {
        if field.name() == Some("photo") {
            photo = field.bytes().await.map_err(read)?.to_vec();
        }
    }
    if photo.is_empty() {
        return bad_request("No photo sent.");
    }

    let found = tokio::task::spawn_blocking(move || crate::barcode::read_isbn(&photo))
        .await
        .map_err(|e| AppError::Internal(format!("reading the barcode: {e}")))?;
    match found {
        Ok(Some(isbn)) => Ok(Json(json!({ "isbn": isbn.as_str() })).into_response()),
        Ok(None) => {
            let body = json!({
                "error": "No ISBN barcode found in the photo: try closer, flat and well lit, \
                          or type the 13 digits.",
            });
            Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(body)).into_response())
        }
        Err(why) => bad_request(format!("{why}.")),
    }
}
