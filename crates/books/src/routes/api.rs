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

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use books_domain::book::BookFormat;
use books_domain::isbn::Isbn;
use serde_json::json;

use crate::store::{book_data, books};
use common::web::error::{bad_request, AppResult};
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
