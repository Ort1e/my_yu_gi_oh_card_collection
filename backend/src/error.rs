use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Error returned by handlers: 404 for missing rows, 500 (logged) otherwise.
#[derive(Debug)]
pub enum AppError {
    NotFound,
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found").into_response(),
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
