//! REST error type mapped to HTTP via `http-error`.

use api_types::ApiResponse;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http_error::HttpError as _;

/// Errors that can surface from the REST surface.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// Requested resource does not exist.
    #[error("not found: {0}")]
    NotFound(&'static str),

    /// Authenticated principal lacks the required role.
    #[error("forbidden: {0}")]
    Forbidden(&'static str),

    /// Malformed client input.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Anything unexpected.
    #[error("internal error: {0}")]
    Internal(String),
}

impl ServerError {
    /// Stable machine-readable error code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Forbidden(_) => "forbidden",
            Self::BadRequest(_) => "bad_request",
            Self::Internal(_) => "internal",
        }
    }
}

impl http_error::HttpError for ServerError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let body = ApiResponse::<()>::error_with(self.code(), self.to_string());
        (self.status_code(), Json(body)).into_response()
    }
}
