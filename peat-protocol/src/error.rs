//! Error types for the PEAT protocol
//!
//! This module provides a comprehensive error hierarchy with context,
//! recovery strategies, and structured error information.

use thiserror::Error;

/// Result type alias for PEAT protocol operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in the PEAT protocol
#[derive(Error, Debug)]
pub enum Error {
    /// Discovery phase errors
    #[error("Discovery error: {message}")]
    Discovery {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Cell formation errors
    #[error("Cell formation error: {message}")]
    SquadFormation {
        message: String,
        squad_id: Option<String>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Hierarchical operation errors
    #[error("Hierarchical operation error: {message}")]
    HierarchicalOp {
        message: String,
        operation: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Capability composition errors
    #[error("Capability composition error: {message}")]
    Composition {
        message: String,
        capability: Option<String>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// CRDT/Storage errors
    #[error("Storage error: {message}")]
    Storage {
        message: String,
        operation: Option<String>,
        key: Option<String>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Network errors
    #[error("Network error: {message}")]
    Network {
        message: String,
        peer_id: Option<String>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid state transition
    #[error("Invalid state transition from {from} to {to}: {reason}")]
    InvalidTransition {
        from: String,
        to: String,
        reason: String,
    },

    /// Resource not found
    #[error("Resource not found: {resource_type} with id {id}")]
    NotFound { resource_type: String, id: String },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    Configuration {
        message: String,
        config_key: Option<String>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Timeout errors
    #[error("Operation timed out after {duration_ms}ms: {operation}")]
    Timeout { operation: String, duration_ms: u64 },

    /// Ditto-specific errors
    #[error("Ditto error: {message}")]
    Ditto {
        message: String,
        operation: Option<String>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Invalid input provided
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Command conflict detected
    #[error("Command conflict detected: {0}")]
    ConflictDetected(String),

    /// Security/Authentication errors
    #[error("Security error: {0}")]
    Security(String),

    /// Event operation errors (ADR-027)
    #[error("Event operation error: {message}")]
    EventOp {
        message: String,
        operation: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl Error {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Error::Timeout { .. } | Error::Network { .. } => true,
            Error::Storage {
                operation: Some(op),
                ..
            } => op == "query" || op == "retrieve",
            _ => false,
        }
    }

    /// Get error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Error::Internal(_) => ErrorSeverity::Critical,
            Error::Configuration { .. } => ErrorSeverity::Critical,
            Error::InvalidTransition { .. } => ErrorSeverity::Error,
            Error::Timeout { .. } => ErrorSeverity::Warning,
            Error::Network { .. } => ErrorSeverity::Warning,
            Error::NotFound { .. } => ErrorSeverity::Info,
            _ => ErrorSeverity::Error,
        }
    }

    /// Get context information from the error
    pub fn context(&self) -> ErrorContext {
        match self {
            Error::Storage { key, operation, .. } => ErrorContext {
                key: key.clone(),
                operation: operation.clone(),
                ..Default::default()
            },
            Error::Network { peer_id, .. } => ErrorContext {
                peer_id: peer_id.clone(),
                ..Default::default()
            },
            Error::SquadFormation { squad_id, .. } => ErrorContext {
                squad_id: squad_id.clone(),
                ..Default::default()
            },
            Error::Composition { capability, .. } => ErrorContext {
                capability: capability.clone(),
                ..Default::default()
            },
            Error::Timeout {
                operation,
                duration_ms,
            } => ErrorContext {
                operation: Some(operation.clone()),
                duration_ms: Some(*duration_ms),
                ..Default::default()
            },
            _ => ErrorContext::default(),
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Critical errors that require immediate attention
    Critical,
    /// Standard errors that prevent operation completion
    Error,
    /// Warnings about recoverable issues
    Warning,
    /// Informational messages about error conditions
    Info,
}

/// Contextual information about an error
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    pub key: Option<String>,
    pub operation: Option<String>,
    pub peer_id: Option<String>,
    pub squad_id: Option<String>,
    pub capability: Option<String>,
    pub duration_ms: Option<u64>,
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Internal(err.to_string())
    }
}

/// Helper functions for creating common errors
impl Error {
    /// Create a storage error with context
    pub fn storage_error(
        message: impl Into<String>,
        operation: impl Into<String>,
        key: Option<String>,
    ) -> Self {
        Error::Storage {
            message: message.into(),
            operation: Some(operation.into()),
            key,
            source: None,
        }
    }

    /// Create a network error with context
    pub fn network_error(message: impl Into<String>, peer_id: Option<String>) -> Self {
        Error::Network {
            message: message.into(),
            peer_id,
            source: None,
        }
    }

    /// Create a timeout error with context
    pub fn timeout_error(operation: impl Into<String>, duration_ms: u64) -> Self {
        Error::Timeout {
            operation: operation.into(),
            duration_ms,
        }
    }

    /// Create a configuration error with context
    pub fn config_error(message: impl Into<String>, config_key: Option<String>) -> Self {
        Error::Configuration {
            message: message.into(),
            config_key,
            source: None,
        }
    }
}

#[cfg(test)]
mod tests;
