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

    /// Permission denied for operation
    #[error("permission denied: {permission} for entity {entity_id} with roles [{roles:?}]")]
    PermissionDenied {
        permission: String,
        entity_id: String,
        roles: Vec<String>,
    },

    /// Certificate validation failed
    #[error("certificate error: {0}")]
    CertificateError(String),

    /// Certificate chain invalid
    #[error("invalid certificate chain: {0}")]
    InvalidCertificateChain(String),

    /// Certificate expired
    #[error("certificate expired: {0}")]
    CertificateExpired(String),

    /// Certificate revoked
    #[error("certificate revoked: {0}")]
    CertificateRevoked(String),

    // User Authentication errors (Phase 3)
    /// User not found in database
    #[error("user not found: {username}")]
    UserNotFound { username: String },

    /// User already exists
    #[error("user already exists: {username}")]
    UserAlreadyExists { username: String },

    /// Invalid credential (wrong password)
    #[error("invalid credential for user: {username}")]
    InvalidCredential { username: String },

    /// Invalid MFA code (TOTP)
    #[error("invalid MFA code")]
    InvalidMfaCode,

    /// Account is locked (too many failed attempts)
    #[error("account locked: {username}")]
    AccountLocked { username: String },

    /// Account is disabled by admin
    #[error("account disabled: {username}")]
    AccountDisabled { username: String },

    /// Account is pending activation
    #[error("account pending activation: {username}")]
    AccountPending { username: String },

    /// Session not found
    #[error("session not found")]
    SessionNotFound,

    /// Session expired
    #[error("session expired")]
    SessionExpired,

    /// Unsupported authentication method
    #[error("unsupported auth method: {method}")]
    UnsupportedAuthMethod { method: String },

    /// Password hashing error
    #[error("password hash error: {message}")]
    PasswordHashError { message: String },

    /// TOTP generation/verification error
    #[error("TOTP error: {message}")]
    TotpError { message: String },
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
            SecurityError::PermissionDenied { .. } => "PERMISSION_DENIED",
            SecurityError::CertificateError(_) => "CERTIFICATE_ERROR",
            SecurityError::InvalidCertificateChain(_) => "INVALID_CERT_CHAIN",
            SecurityError::CertificateExpired(_) => "CERTIFICATE_EXPIRED",
            SecurityError::CertificateRevoked(_) => "CERTIFICATE_REVOKED",
            // User auth errors
            SecurityError::UserNotFound { .. } => "USER_NOT_FOUND",
            SecurityError::UserAlreadyExists { .. } => "USER_EXISTS",
            SecurityError::InvalidCredential { .. } => "INVALID_CREDENTIAL",
            SecurityError::InvalidMfaCode => "INVALID_MFA",
            SecurityError::AccountLocked { .. } => "ACCOUNT_LOCKED",
            SecurityError::AccountDisabled { .. } => "ACCOUNT_DISABLED",
            SecurityError::AccountPending { .. } => "ACCOUNT_PENDING",
            SecurityError::SessionNotFound => "SESSION_NOT_FOUND",
            SecurityError::SessionExpired => "SESSION_EXPIRED",
            SecurityError::UnsupportedAuthMethod { .. } => "UNSUPPORTED_AUTH",
            SecurityError::PasswordHashError { .. } => "PASSWORD_HASH_ERROR",
            SecurityError::TotpError { .. } => "TOTP_ERROR",
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
