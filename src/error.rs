//! One error type for every handler. Handlers return `Result<_, AppError>` and
//! `?` straight through `sqlx`/`anyhow` errors; `IntoResponse` decides how each
//! variant should look to the browser (redirect, 404, 400, or a logged 500).

use axum::{
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// No valid session — send the browser to the login form.
    #[error("unauthorized")]
    Unauthorized,
    /// Row didn't exist, or existed but isn't owned by the current user.
    /// Same response either way so ownership can't be probed by id.
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Unauthorized => Redirect::to("/login").into_response(),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            AppError::Database(err) => {
                tracing::error!(?err, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "something went wrong").into_response()
            }
            AppError::Other(err) => {
                tracing::error!(?err, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "something went wrong").into_response()
            }
        }
    }
}
