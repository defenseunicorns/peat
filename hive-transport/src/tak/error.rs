//! TAK transport error types

use std::io;
use thiserror::Error;

/// Error types for TAK transport operations
#[derive(Debug, Error)]
pub enum TakError {
    /// Connection to TAK server or mesh failed
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Authentication with TAK server failed
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Failed to encode CoT message
    #[error("Message encoding failed: {0}")]
    EncodingError(String),

    /// Failed to decode incoming CoT message
    #[error("Message decoding failed: {0}")]
    DecodingError(String),

    /// Message queue is full, message was dropped
    #[error("Queue full, message dropped")]
    QueueFull,

    /// Operation timed out
    #[error("Connection timeout")]
    Timeout,

    /// TLS/SSL error during connection
    #[error("TLS/SSL error: {0}")]
    TlsError(String),

    /// Low-level I/O error
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    /// Transport is not connected
    #[error("Not connected")]
    NotConnected,

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Multicast error
    #[error("Multicast error: {0}")]
    MulticastError(String),

    /// Protocol framing error
    #[error("Protocol framing error: {0}")]
    FramingError(String),

    /// Channel closed unexpectedly
    #[error("Channel closed")]
    ChannelClosed,
}

impl TakError {
    /// Check if this error is recoverable (worth retrying)
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            TakError::ConnectionFailed(_)
                | TakError::Timeout
                | TakError::IoError(_)
                | TakError::NotConnected
        )
    }

    /// Check if this is an authentication error
    pub fn is_auth_error(&self) -> bool {
        matches!(
            self,
            TakError::AuthenticationFailed(_) | TakError::TlsError(_)
        )
    }
}
