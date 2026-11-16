use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::web::auth::ForbiddenPage;

/// Error returned by handlers: 404 for missing rows, 400 with a JSON
/// `{"error": ...}` body for invalid API input, 500 (logged) otherwise.
#[derive(Debug)]
pub enum AppError {
    NotFound,
    /// 401: not logged in.
    Unauthorized,
    /// 403: logged in, but not allowed to change `what` (a permission's
    /// label); `allowed`: what the login may change.
    Forbidden {
        what: String,
        allowed: String,
    },
    BadRequest(String),
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound => f.write_str("not found"),
            AppError::Unauthorized => f.write_str("log in"),
            AppError::Forbidden { what, .. } => write!(f, "not allowed to change {what}"),
            AppError::BadRequest(msg) | AppError::Internal(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found").into_response(),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Log in").into_response(),
            AppError::Forbidden { what, allowed } => {
                (StatusCode::FORBIDDEN, ForbiddenPage { what, allowed }).into_response()
            }
            AppError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": msg })),
            )
                .into_response(),
            AppError::Internal(msg) => {
                tracing::error!("{msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            e => AppError::Internal(format!("database error: {e}")),
        }
    }
}

impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self {
        AppError::Internal(format!("template error: {e}"))
    }
}

/// `.optional()`-style helper: turn `None` into a 404.
pub trait OrNotFound<T> {
    fn or_404(self) -> AppResult<T>;
}

impl<T> OrNotFound<T> for Option<T> {
    fn or_404(self) -> AppResult<T> {
        self.ok_or(AppError::NotFound)
    }
}

pub fn bad_request<T>(msg: impl Into<String>) -> AppResult<T> {
    Err(AppError::BadRequest(msg.into()))
}

/// API input: `text` trimmed, or a 400 with `message` when it is blank.
pub fn required<'a>(text: &'a str, message: &str) -> AppResult<&'a str> {
    match text.trim() {
        "" => bad_request(message),
        text => Ok(text),
    }
}

/// API input: `text` trimmed, `None` when absent or blank.
pub fn non_blank(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}
