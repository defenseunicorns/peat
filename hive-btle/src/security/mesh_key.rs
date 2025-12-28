//! Mesh encryption key derivation and cryptographic operations

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use rand_core::RngCore;
use sha2::Sha256;

/// Errors that can occur during encryption/decryption
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncryptionError {
    /// Encryption operation failed
    EncryptionFailed,
    /// Decryption failed (wrong key or corrupted data)
    DecryptionFailed,
    /// Invalid encrypted document format
    InvalidFormat,
}

impl core::fmt::Display for EncryptionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EncryptionFailed => write!(f, "encryption failed"),
            Self::DecryptionFailed => write!(f, "decryption failed (wrong key or corrupted data)"),
            Self::InvalidFormat => write!(f, "invalid encrypted document format"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EncryptionError {}

/// An encrypted HIVE document
///
/// Contains the nonce and ciphertext (which includes the 16-byte Poly1305 auth tag).
#[derive(Debug, Clone)]
pub struct EncryptedDocument {
    /// 12-byte random nonce
    pub nonce: [u8; 12],
    /// Ciphertext with appended 16-byte auth tag
    pub ciphertext: Vec<u8>,
}

impl EncryptedDocument {
    /// Total overhead added by encryption (nonce + auth tag)
    pub const OVERHEAD: usize = 12 + 16; // nonce + Poly1305 tag

    /// Encode to bytes for wire transmission
    ///
    /// Format: nonce (12 bytes) || ciphertext (variable, includes tag)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(12 + self.ciphertext.len());
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.ciphertext);
        buf
    }

    /// Decode from bytes received over wire
    ///
    /// Returns None if data is too short (minimum: 12 nonce + 16 tag = 28 bytes)
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < Self::OVERHEAD {
            return None;
        }

        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&data[..12]);
        let ciphertext = data[12..].to_vec();

        Some(Self { nonce, ciphertext })
    }
}

/// Mesh-wide encryption key for HIVE documents
///
/// All nodes sharing the same formation secret derive the same key,
/// enabling encrypted communication across the mesh.
#[derive(Clone)]
pub struct MeshEncryptionKey {
    /// ChaCha20-Poly1305 256-bit key
    key: [u8; 32],
}

impl MeshEncryptionKey {
    /// HKDF info context for mesh encryption key derivation
    const HKDF_INFO: &'static [u8] = b"HIVE-BTLE-mesh-encryption-v1";

    /// Derive a mesh encryption key from a shared secret
    ///
    /// Uses HKDF-SHA256 with the mesh ID as salt and a fixed info string
    /// to derive a unique 256-bit key for this mesh.
    ///
    /// # Arguments
    /// * `mesh_id` - The mesh identifier (e.g., "DEMO", "ALPHA")
    /// * `secret` - 32-byte shared secret known to all mesh participants
    ///
    /// # Example
    /// ```ignore
    /// let secret = [0x42u8; 32]; // In practice, a securely shared secret
    /// let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);
    /// ```
    pub fn from_shared_secret(mesh_id: &str, secret: &[u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(mesh_id.as_bytes()), secret);
        let mut key = [0u8; 32];
        hk.expand(Self::HKDF_INFO, &mut key)
            .expect("32 bytes is valid output length for HKDF-SHA256");
        Self { key }
    }

    /// Encrypt plaintext document bytes
    ///
    /// Generates a random 12-byte nonce and encrypts using ChaCha20-Poly1305.
    /// The resulting ciphertext includes a 16-byte authentication tag.
    ///
    /// # Arguments
    /// * `plaintext` - Raw document bytes to encrypt
    ///
    /// # Returns
    /// * `Ok(EncryptedDocument)` - Encrypted document with nonce and ciphertext
    /// * `Err(EncryptionError)` - If encryption fails (should not happen in practice)
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedDocument, EncryptionError> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt with authentication
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        Ok(EncryptedDocument {
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    /// Decrypt encrypted document bytes
    ///
    /// Verifies the authentication tag and decrypts the ciphertext.
    ///
    /// # Arguments
    /// * `encrypted` - Encrypted document with nonce and ciphertext
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - Decrypted plaintext document bytes
    /// * `Err(EncryptionError)` - If decryption fails (wrong key or corrupted data)
    pub fn decrypt(&self, encrypted: &EncryptedDocument) -> Result<Vec<u8>, EncryptionError> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| EncryptionError::DecryptionFailed)?;

        let nonce = Nonce::from_slice(&encrypted.nonce);

        cipher
            .decrypt(nonce, encrypted.ciphertext.as_ref())
            .map_err(|_| EncryptionError::DecryptionFailed)
    }

    /// Encrypt and encode in one step
    ///
    /// Convenience method that encrypts plaintext and returns wire-format bytes.
    pub fn encrypt_to_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let encrypted = self.encrypt(plaintext)?;
        Ok(encrypted.encode())
    }

    /// Decode and decrypt in one step
    ///
    /// Convenience method that decodes wire-format bytes and decrypts.
    pub fn decrypt_from_bytes(&self, data: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let encrypted = EncryptedDocument::decode(data).ok_or(EncryptionError::InvalidFormat)?;
        self.decrypt(&encrypted)
    }
}

impl core::fmt::Debug for MeshEncryptionKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Don't expose key bytes in debug output
        f.debug_struct("MeshEncryptionKey")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_derivation_deterministic() {
        let secret = [0x42u8; 32];
        let key1 = MeshEncryptionKey::from_shared_secret("DEMO", &secret);
        let key2 = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        // Same inputs produce same key
        assert_eq!(key1.key, key2.key);
    }

    #[test]
    fn test_key_derivation_different_mesh_id() {
        let secret = [0x42u8; 32];
        let key1 = MeshEncryptionKey::from_shared_secret("DEMO", &secret);
        let key2 = MeshEncryptionKey::from_shared_secret("ALPHA", &secret);

        // Different mesh IDs produce different keys
        assert_ne!(key1.key, key2.key);
    }

    #[test]
    fn test_key_derivation_different_secret() {
        let secret1 = [0x42u8; 32];
        let secret2 = [0x43u8; 32];
        let key1 = MeshEncryptionKey::from_shared_secret("DEMO", &secret1);
        let key2 = MeshEncryptionKey::from_shared_secret("DEMO", &secret2);

        // Different secrets produce different keys
        assert_ne!(key1.key, key2.key);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let secret = [0x42u8; 32];
        let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        let plaintext = b"Hello, HIVE mesh!";
        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_decrypt_empty() {
        let secret = [0x42u8; 32];
        let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        let plaintext = b"";
        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let secret = [0x42u8; 32];
        let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        let plaintext = b"Same message";
        let encrypted1 = key.encrypt(plaintext).unwrap();
        let encrypted2 = key.encrypt(plaintext).unwrap();

        // Different nonces produce different ciphertext (probabilistic encryption)
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);

        // But both decrypt to same plaintext
        assert_eq!(key.decrypt(&encrypted1).unwrap(), plaintext.as_slice());
        assert_eq!(key.decrypt(&encrypted2).unwrap(), plaintext.as_slice());
    }

    #[test]
    fn test_wrong_key_fails() {
        let secret1 = [0x42u8; 32];
        let secret2 = [0x43u8; 32];
        let key1 = MeshEncryptionKey::from_shared_secret("DEMO", &secret1);
        let key2 = MeshEncryptionKey::from_shared_secret("DEMO", &secret2);

        let plaintext = b"Secret message";
        let encrypted = key1.encrypt(plaintext).unwrap();

        // Wrong key fails to decrypt (authentication fails)
        let result = key2.decrypt(&encrypted);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), EncryptionError::DecryptionFailed);
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let secret = [0x42u8; 32];
        let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        let plaintext = b"Authentic message";
        let mut encrypted = key.encrypt(plaintext).unwrap();

        // Tamper with ciphertext
        if !encrypted.ciphertext.is_empty() {
            encrypted.ciphertext[0] ^= 0xFF;
        }

        // Decryption fails (authentication fails)
        let result = key.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypted_document_encode_decode() {
        let secret = [0x42u8; 32];
        let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        let plaintext = b"Wire format test";
        let encrypted = key.encrypt(plaintext).unwrap();

        // Encode to bytes
        let wire_bytes = encrypted.encode();

        // Decode from bytes
        let decoded = EncryptedDocument::decode(&wire_bytes).unwrap();

        assert_eq!(encrypted.nonce, decoded.nonce);
        assert_eq!(encrypted.ciphertext, decoded.ciphertext);

        // Decrypt decoded document
        let decrypted = key.decrypt(&decoded).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_convenience_methods() {
        let secret = [0x42u8; 32];
        let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        let plaintext = b"Convenience test";

        // encrypt_to_bytes / decrypt_from_bytes
        let wire_bytes = key.encrypt_to_bytes(plaintext).unwrap();
        let decrypted = key.decrypt_from_bytes(&wire_bytes).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypted_document_decode_too_short() {
        // Less than 28 bytes (12 nonce + 16 tag minimum)
        let short_data = [0u8; 27];
        assert!(EncryptedDocument::decode(&short_data).is_none());

        // Exactly 28 bytes is valid (empty plaintext)
        let minimal_data = [0u8; 28];
        assert!(EncryptedDocument::decode(&minimal_data).is_some());
    }

    #[test]
    fn test_overhead_calculation() {
        let secret = [0x42u8; 32];
        let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        let plaintext = b"Testing overhead";
        let encrypted = key.encrypt(plaintext).unwrap();
        let wire_bytes = encrypted.encode();

        // Wire format: nonce (12) + ciphertext (plaintext.len() + 16 tag)
        let expected_size = 12 + plaintext.len() + 16;
        assert_eq!(wire_bytes.len(), expected_size);
        assert_eq!(
            wire_bytes.len() - plaintext.len(),
            EncryptedDocument::OVERHEAD
        );
    }

    #[test]
    fn test_debug_redacts_key() {
        let secret = [0x42u8; 32];
        let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        let debug_str = format!("{:?}", key);
        assert!(debug_str.contains("REDACTED"));
        assert!(!debug_str.contains("42")); // Key bytes not exposed
    }

    #[test]
    fn test_realistic_document_size() {
        let secret = [0x42u8; 32];
        let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

        // Simulate a typical HIVE document (100 bytes)
        let doc = vec![0xABu8; 100];
        let encrypted = key.encrypt(&doc).unwrap();
        let wire_bytes = encrypted.encode();

        // 100 + 28 = 128 bytes
        assert_eq!(wire_bytes.len(), 128);

        // Well under BLE MTU (244 bytes) and MAX_DOCUMENT_SIZE (512 bytes)
        assert!(wire_bytes.len() < 244);
        assert!(wire_bytes.len() < 512);
    }
}
