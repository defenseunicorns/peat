//! # Security Primitives for Mesh Networks
//!
//! Generic cryptographic security primitives for mesh networking:
//!
//! - [`DeviceId`] - Unique device identifier derived from Ed25519 public key
//! - [`DeviceKeypair`] - Ed25519 keypair for signing and identity
//! - [`SecurityError`] - Error types for security operations
//! - [`EncryptionKeypair`] / [`EncryptionManager`] - X25519/ChaCha20 encryption
//! - [`FormationKey`] - HMAC-SHA256 PSK formation authentication
//! - [`CallsignGenerator`] - NATO phonetic callsign generation
//!
//! These primitives have no domain-specific dependencies and can be used
//! by any mesh networking application.

pub mod callsign;
pub mod device_id;
pub mod encryption;
pub mod error;
pub mod formation_key;
pub mod keypair;

pub use callsign::{
    CallsignError, CallsignGenerator, MAX_CALLSIGN_LENGTH, NATO_ALPHABET, TOTAL_CALLSIGNS,
};
pub use device_id::DeviceId;
pub use encryption::{
    EncryptedCellMessage, EncryptedData, EncryptedDocument, EncryptionKeypair, EncryptionManager,
    GroupKey, SecureChannel, SymmetricKey, NONCE_SIZE, SYMMETRIC_KEY_SIZE, X25519_PUBLIC_KEY_SIZE,
};
pub use error::SecurityError;
pub use formation_key::{
    FormationAuthResult, FormationChallenge, FormationChallengeResponse, FormationKey,
    FORMATION_CHALLENGE_SIZE, FORMATION_RESPONSE_SIZE,
};
pub use keypair::DeviceKeypair;

/// Default challenge timeout in seconds
pub const DEFAULT_CHALLENGE_TIMEOUT_SECS: u64 = 30;

/// Size of challenge nonce in bytes
pub const CHALLENGE_NONCE_SIZE: usize = 32;

/// Size of Ed25519 public key in bytes
pub const PUBLIC_KEY_SIZE: usize = 32;

/// Size of Ed25519 signature in bytes
pub const SIGNATURE_SIZE: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all public types are accessible
        let _: fn() -> DeviceKeypair = DeviceKeypair::generate;
        let _: fn() -> EncryptionKeypair = EncryptionKeypair::generate;
        let _: fn() -> CallsignGenerator = CallsignGenerator::new;
    }
}
