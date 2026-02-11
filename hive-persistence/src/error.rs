//! Error types for cap-persistence

use thiserror::Error;

/// Persistence error type
#[derive(Error, Debug)]
pub enum Error {
    /// Backend-specific error (Ditto, Automerge, etc.)
    #[error("Backend error: {0}")]
    Backend(#[from] hive_protocol::Error),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Document not found
    #[error("Document not found: {0}")]
    NotFound(String),

    /// Invalid query
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    /// Subscription error
    #[error("Subscription error: {0}")]
    Subscription(String),

    /// Transaction error
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Allow `?` on `anyhow::Result` in functions returning this Error
impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Internal(err.to_string())
    }
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;
