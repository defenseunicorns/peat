//! Ed25519 keypair for device identity and signing.

use super::device_id::DeviceId;
use super::error::SecurityError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use std::path::Path;

/// Ed25519 keypair for device identity and authentication.
///
/// The keypair consists of:
/// - A 32-byte secret (signing) key
/// - A 32-byte public (verifying) key
///
/// The [`DeviceId`] is derived from the public key.
///
/// # Example
///
/// ```ignore
/// use hive_mesh::security::DeviceKeypair;
///
/// // Generate a new keypair
/// let keypair = DeviceKeypair::generate();
///
/// // Get the device ID
/// let device_id = keypair.device_id();
///
/// // Sign a message
/// let message = b"hello world";
/// let signature = keypair.sign(message);
///
/// // Verify the signature
/// assert!(keypair.verify(message, &signature).is_ok());
/// ```
#[derive(Clone)]
pub struct DeviceKeypair {
    signing_key: SigningKey,
}

impl DeviceKeypair {
    /// Generate a new random keypair.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        DeviceKeypair { signing_key }
    }

    /// Create from an existing signing key.
    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        DeviceKeypair { signing_key }
    }

    /// Create from raw secret key bytes (32 bytes).
    pub fn from_secret_bytes(bytes: &[u8]) -> Result<Self, SecurityError> {
        if bytes.len() != 32 {
            return Err(SecurityError::KeypairError(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }

        let signing_key = SigningKey::from_bytes(bytes.try_into().unwrap());
        Ok(DeviceKeypair { signing_key })
    }

    /// Load keypair from a file (raw 32-byte secret key).
    pub fn load_from_file(path: &Path) -> Result<Self, SecurityError> {
        let bytes = std::fs::read(path)?;
        Self::from_secret_bytes(&bytes)
    }

    /// Save keypair to a file (raw 32-byte secret key).
    ///
    /// # Security Note
    ///
    /// In MVP, this saves the key unencrypted. Production deployments
    /// should use encrypted key storage (Phase 2).
    pub fn save_to_file(&self, path: &Path) -> Result<(), SecurityError> {
        std::fs::write(path, self.signing_key.to_bytes())?;
        Ok(())
    }

    /// Get the device ID derived from this keypair's public key.
    pub fn device_id(&self) -> DeviceId {
        DeviceId::from_public_key(&self.signing_key.verifying_key())
    }

    /// Get the public (verifying) key.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Get the public key as bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Get the secret key bytes (32 bytes).
    ///
    /// # Security Warning
    ///
    /// This exposes the private key material. Only use for:
    /// - Secure storage/persistence
    /// - Cross-crate interop (e.g., converting to hive_btle::DeviceIdentity)
    pub fn secret_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Sign a message with the secret key.
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    /// Verify a signature against this keypair's public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), SecurityError> {
        self.signing_key
            .verifying_key()
            .verify(message, signature)
            .map_err(|e| SecurityError::InvalidSignature(e.to_string()))
    }

    /// Verify a signature against a specific public key.
    pub fn verify_with_key(
        public_key: &VerifyingKey,
        message: &[u8],
        signature: &Signature,
    ) -> Result<(), SecurityError> {
        public_key
            .verify(message, signature)
            .map_err(|e| SecurityError::InvalidSignature(e.to_string()))
    }

    /// Parse a signature from bytes.
    pub fn signature_from_bytes(bytes: &[u8]) -> Result<Signature, SecurityError> {
        if bytes.len() != 64 {
            return Err(SecurityError::InvalidSignature(format!(
                "expected 64 bytes, got {}",
                bytes.len()
            )));
        }

        // ed25519-dalek v2 from_bytes returns Signature directly (infallible after length check)
        Ok(Signature::from_bytes(bytes.try_into().unwrap()))
    }

    /// Parse a verifying key from bytes.
    pub fn verifying_key_from_bytes(bytes: &[u8]) -> Result<VerifyingKey, SecurityError> {
        if bytes.len() != 32 {
            return Err(SecurityError::InvalidPublicKey(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }

        VerifyingKey::from_bytes(bytes.try_into().unwrap())
            .map_err(|e| SecurityError::InvalidPublicKey(e.to_string()))
    }
}

impl std::fmt::Debug for DeviceKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKeypair")
            .field("device_id", &self.device_id())
            .field("public_key", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_keypair() {
        let keypair = DeviceKeypair::generate();
        let device_id = keypair.device_id();
        assert_eq!(device_id.to_hex().len(), 32);
    }

    #[test]
    fn test_sign_and_verify() {
        let keypair = DeviceKeypair::generate();
        let message = b"test message";

        let signature = keypair.sign(message);
        assert!(keypair.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_verify_wrong_message_fails() {
        let keypair = DeviceKeypair::generate();
        let signature = keypair.sign(b"original message");

        let result = keypair.verify(b"different message", &signature);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_wrong_key_fails() {
        let keypair1 = DeviceKeypair::generate();
        let keypair2 = DeviceKeypair::generate();

        let message = b"test message";
        let signature = keypair1.sign(message);

        let result = keypair2.verify(message, &signature);
        assert!(result.is_err());
    }

    #[test]
    fn test_save_and_load_keypair() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_key.bin");

        let keypair1 = DeviceKeypair::generate();
        keypair1.save_to_file(&path).unwrap();

        let keypair2 = DeviceKeypair::load_from_file(&path).unwrap();

        // Device IDs should match
        assert_eq!(keypair1.device_id(), keypair2.device_id());

        // Signatures should be verifiable across both
        let message = b"test";
        let sig = keypair1.sign(message);
        assert!(keypair2.verify(message, &sig).is_ok());
    }

    #[test]
    fn test_from_secret_bytes() {
        let keypair1 = DeviceKeypair::generate();
        let secret_bytes = keypair1.signing_key.to_bytes();

        let keypair2 = DeviceKeypair::from_secret_bytes(&secret_bytes).unwrap();
        assert_eq!(keypair1.device_id(), keypair2.device_id());
    }

    #[test]
    fn test_signature_from_bytes_roundtrip() {
        let keypair = DeviceKeypair::generate();
        let signature = keypair.sign(b"test");

        let sig_bytes = signature.to_bytes();
        let parsed = DeviceKeypair::signature_from_bytes(&sig_bytes).unwrap();

        assert_eq!(signature, parsed);
    }
}
