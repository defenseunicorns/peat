//! Security error types for HIVE Protocol authentication.

use thiserror::Error;

/// Errors that can occur during security operations.
#[derive(Error, Debug)]
pub enum SecurityError {
    /// Invalid signature - verification failed
    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    /// Challenge has expired
    #[error("challenge expired: valid until {0}")]
    ChallengeExpired(u64),

    /// Challenge nonce mismatch
    #[error("nonce mismatch: expected {expected}, got {actual}")]
    NonceMismatch { expected: String, actual: String },

    /// Invalid public key format
    #[error("invalid public key: {0}")]
    InvalidPublicKey(String),

    /// Invalid device ID format
    #[error("invalid device ID: {0}")]
    InvalidDeviceId(String),

    /// Keypair error (generation, loading, saving)
    #[error("keypair error: {0}")]
    KeypairError(String),

    /// Peer not authenticated
    #[error("peer not authenticated: {0}")]
    PeerNotAuthenticated(String),

    /// Authentication failed
    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    /// IO error (file operations)
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization error
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// Internal error
    #[error("internal security error: {0}")]
    Internal(String),

    /// Peer not found
    #[error("peer not found: {0}")]
    PeerNotFound(String),
}

impl SecurityError {
    /// Get the error code for protocol messages
    pub fn code(&self) -> &'static str {
        match self {
            SecurityError::InvalidSignature(_) => "INVALID_SIGNATURE",
            SecurityError::ChallengeExpired(_) => "CHALLENGE_EXPIRED",
            SecurityError::NonceMismatch { .. } => "NONCE_MISMATCH",
            SecurityError::InvalidPublicKey(_) => "INVALID_PUBLIC_KEY",
            SecurityError::InvalidDeviceId(_) => "INVALID_DEVICE_ID",
            SecurityError::KeypairError(_) => "KEYPAIR_ERROR",
            SecurityError::PeerNotAuthenticated(_) => "PEER_NOT_AUTHENTICATED",
            SecurityError::AuthenticationFailed(_) => "AUTH_FAILED",
            SecurityError::IoError(_) => "IO_ERROR",
            SecurityError::SerializationError(_) => "SERIALIZATION_ERROR",
            SecurityError::Internal(_) => "INTERNAL_ERROR",
            SecurityError::PeerNotFound(_) => "PEER_NOT_FOUND",
        }
    }

    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(self, SecurityError::ChallengeExpired(_))
    }
}

// Integration with main crate error type
impl From<SecurityError> for crate::Error {
    fn from(err: SecurityError) -> Self {
        crate::Error::Security(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        let err = SecurityError::InvalidSignature("test".to_string());
        assert_eq!(err.code(), "INVALID_SIGNATURE");

        let err = SecurityError::ChallengeExpired(12345);
        assert_eq!(err.code(), "CHALLENGE_EXPIRED");
        assert!(err.is_recoverable());
    }
}
