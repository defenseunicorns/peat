//! Security error types for mesh security primitives.

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

    // Encryption errors (Phase 4: Encryption & Audit)
    /// Encryption operation failed
    #[error("encryption error: {0}")]
    EncryptionError(String),

    /// Decryption operation failed
    #[error("decryption error: {0}")]
    DecryptionError(String),

    /// Key exchange failed
    #[error("key exchange error: {0}")]
    KeyExchangeError(String),

    /// No group key for cell
    #[error("no group key for cell: {cell_id}")]
    NoGroupKey { cell_id: String },

    /// Key generation mismatch
    #[error("key generation mismatch: expected {expected}, got {actual}")]
    KeyGenerationMismatch { expected: u64, actual: u64 },
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
            // Encryption errors
            SecurityError::EncryptionError(_) => "ENCRYPTION_ERROR",
            SecurityError::DecryptionError(_) => "DECRYPTION_ERROR",
            SecurityError::KeyExchangeError(_) => "KEY_EXCHANGE_ERROR",
            SecurityError::NoGroupKey { .. } => "NO_GROUP_KEY",
            SecurityError::KeyGenerationMismatch { .. } => "KEY_GENERATION_MISMATCH",
        }
    }

    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(self, SecurityError::ChallengeExpired(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_error_codes() {
        let cases: Vec<(SecurityError, &str)> = vec![
            (
                SecurityError::InvalidSignature("x".into()),
                "INVALID_SIGNATURE",
            ),
            (SecurityError::ChallengeExpired(0), "CHALLENGE_EXPIRED"),
            (
                SecurityError::NonceMismatch {
                    expected: "a".into(),
                    actual: "b".into(),
                },
                "NONCE_MISMATCH",
            ),
            (
                SecurityError::InvalidPublicKey("x".into()),
                "INVALID_PUBLIC_KEY",
            ),
            (
                SecurityError::InvalidDeviceId("x".into()),
                "INVALID_DEVICE_ID",
            ),
            (SecurityError::KeypairError("x".into()), "KEYPAIR_ERROR"),
            (
                SecurityError::PeerNotAuthenticated("x".into()),
                "PEER_NOT_AUTHENTICATED",
            ),
            (
                SecurityError::AuthenticationFailed("x".into()),
                "AUTH_FAILED",
            ),
            (
                SecurityError::IoError(std::io::Error::new(std::io::ErrorKind::Other, "x")),
                "IO_ERROR",
            ),
            (
                SecurityError::SerializationError("x".into()),
                "SERIALIZATION_ERROR",
            ),
            (SecurityError::Internal("x".into()), "INTERNAL_ERROR"),
            (SecurityError::PeerNotFound("x".into()), "PEER_NOT_FOUND"),
            (
                SecurityError::PermissionDenied {
                    permission: "w".into(),
                    entity_id: "e".into(),
                    roles: vec!["r".into()],
                },
                "PERMISSION_DENIED",
            ),
            (
                SecurityError::CertificateError("x".into()),
                "CERTIFICATE_ERROR",
            ),
            (
                SecurityError::InvalidCertificateChain("x".into()),
                "INVALID_CERT_CHAIN",
            ),
            (
                SecurityError::CertificateExpired("x".into()),
                "CERTIFICATE_EXPIRED",
            ),
            (
                SecurityError::CertificateRevoked("x".into()),
                "CERTIFICATE_REVOKED",
            ),
            (
                SecurityError::UserNotFound {
                    username: "u".into(),
                },
                "USER_NOT_FOUND",
            ),
            (
                SecurityError::UserAlreadyExists {
                    username: "u".into(),
                },
                "USER_EXISTS",
            ),
            (
                SecurityError::InvalidCredential {
                    username: "u".into(),
                },
                "INVALID_CREDENTIAL",
            ),
            (SecurityError::InvalidMfaCode, "INVALID_MFA"),
            (
                SecurityError::AccountLocked {
                    username: "u".into(),
                },
                "ACCOUNT_LOCKED",
            ),
            (
                SecurityError::AccountDisabled {
                    username: "u".into(),
                },
                "ACCOUNT_DISABLED",
            ),
            (
                SecurityError::AccountPending {
                    username: "u".into(),
                },
                "ACCOUNT_PENDING",
            ),
            (SecurityError::SessionNotFound, "SESSION_NOT_FOUND"),
            (SecurityError::SessionExpired, "SESSION_EXPIRED"),
            (
                SecurityError::UnsupportedAuthMethod { method: "m".into() },
                "UNSUPPORTED_AUTH",
            ),
            (
                SecurityError::PasswordHashError {
                    message: "m".into(),
                },
                "PASSWORD_HASH_ERROR",
            ),
            (
                SecurityError::TotpError {
                    message: "m".into(),
                },
                "TOTP_ERROR",
            ),
            (
                SecurityError::EncryptionError("x".into()),
                "ENCRYPTION_ERROR",
            ),
            (
                SecurityError::DecryptionError("x".into()),
                "DECRYPTION_ERROR",
            ),
            (
                SecurityError::KeyExchangeError("x".into()),
                "KEY_EXCHANGE_ERROR",
            ),
            (
                SecurityError::NoGroupKey {
                    cell_id: "c".into(),
                },
                "NO_GROUP_KEY",
            ),
            (
                SecurityError::KeyGenerationMismatch {
                    expected: 1,
                    actual: 2,
                },
                "KEY_GENERATION_MISMATCH",
            ),
        ];

        for (err, expected_code) in cases {
            assert_eq!(err.code(), expected_code, "wrong code for {}", err);
        }
    }

    #[test]
    fn test_is_recoverable() {
        assert!(SecurityError::ChallengeExpired(0).is_recoverable());

        // All others should NOT be recoverable
        assert!(!SecurityError::InvalidSignature("x".into()).is_recoverable());
        assert!(!SecurityError::NonceMismatch {
            expected: "a".into(),
            actual: "b".into()
        }
        .is_recoverable());
        assert!(!SecurityError::Internal("x".into()).is_recoverable());
        assert!(!SecurityError::SessionExpired.is_recoverable());
        assert!(!SecurityError::EncryptionError("x".into()).is_recoverable());
        assert!(!SecurityError::AccountLocked {
            username: "u".into()
        }
        .is_recoverable());
    }

    #[test]
    fn test_error_display_messages() {
        assert_eq!(
            SecurityError::InvalidSignature("bad sig".into()).to_string(),
            "invalid signature: bad sig"
        );
        assert_eq!(
            SecurityError::NonceMismatch {
                expected: "aaa".into(),
                actual: "bbb".into()
            }
            .to_string(),
            "nonce mismatch: expected aaa, got bbb"
        );
        let pd = SecurityError::PermissionDenied {
            permission: "write".into(),
            entity_id: "e1".into(),
            roles: vec!["admin".into()],
        };
        assert!(pd
            .to_string()
            .contains("permission denied: write for entity e1"));
        assert_eq!(
            SecurityError::UserNotFound {
                username: "alice".into()
            }
            .to_string(),
            "user not found: alice"
        );
        assert_eq!(
            SecurityError::KeyGenerationMismatch {
                expected: 3,
                actual: 5
            }
            .to_string(),
            "key generation mismatch: expected 3, got 5"
        );
        assert_eq!(
            SecurityError::NoGroupKey {
                cell_id: "cell-1".into()
            }
            .to_string(),
            "no group key for cell: cell-1"
        );
        assert_eq!(
            SecurityError::InvalidMfaCode.to_string(),
            "invalid MFA code"
        );
        assert_eq!(
            SecurityError::SessionNotFound.to_string(),
            "session not found"
        );
        assert_eq!(SecurityError::SessionExpired.to_string(), "session expired");
    }

    #[test]
    fn test_io_error_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let sec_err: SecurityError = io_err.into();
        assert_eq!(sec_err.code(), "IO_ERROR");
        assert!(sec_err.to_string().contains("file missing"));
    }
}
