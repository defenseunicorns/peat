//! Error types for cap-transport

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Transport error type
#[derive(Error, Debug)]
pub enum Error {
    /// Backend error (Ditto, Automerge, etc.)
    #[error("Backend error: {0}")]
    Backend(#[from] cap_protocol::Error),

    /// HTTP server error
    #[error("HTTP server error: {0}")]
    Http(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid query parameter
    #[error("Invalid query parameter: {0}")]
    InvalidQuery(String),

    /// Resource not found
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Internal server error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

/// Convert transport Error to HTTP response
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            Error::Backend(ref e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Error::Http(ref msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Error::Serialization(ref e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Error::InvalidQuery(ref msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Error::NotFound(ref msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Error::Internal(ref msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = Json(json!({
            "error": error_message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}
