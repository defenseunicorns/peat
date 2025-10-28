//! Error types for the CAP protocol

use thiserror::Error;

/// Result type alias for CAP protocol operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in the CAP protocol
#[derive(Error, Debug)]
pub enum Error {
    /// Bootstrap phase errors
    #[error("Bootstrap error: {0}")]
    Bootstrap(String),

    /// Squad formation errors
    #[error("Squad formation error: {0}")]
    SquadFormation(String),

    /// Hierarchical operation errors
    #[error("Hierarchical operation error: {0}")]
    HierarchicalOp(String),

    /// Capability composition errors
    #[error("Capability composition error: {0}")]
    Composition(String),

    /// CRDT/Storage errors
    #[error("Storage error: {0}")]
    Storage(String),

    /// Network errors
    #[error("Network error: {0}")]
    Network(String),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid state transition
    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },

    /// Resource not found
    #[error("Resource not found: {resource_type} with id {id}")]
    NotFound { resource_type: String, id: String },

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Timeout errors
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),
}
