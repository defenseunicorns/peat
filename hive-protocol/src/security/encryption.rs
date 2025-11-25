//! # Encryption Module - Data Encryption for HIVE Protocol
//!
//! Implements ADR-006 Layer 5: Data Encryption.
//!
//! ## Overview
//!
//! Provides encryption for all data in transit and at rest using:
//! - **ChaCha20-Poly1305**: AEAD symmetric encryption
//! - **X25519**: Diffie-Hellman key exchange
//! - **HKDF-SHA256**: Key derivation
//!
//! ## Encryption Layers
//!
//! | Layer | Scope | Key Type |
//! |-------|-------|----------|
//! | Transport | Peer-to-peer connections | Session keys (DH) |
//! | Storage | At-rest documents | Device key |
//! | Cell Broadcast | Cell-wide messages | Group key |
//!
//! ## Usage
//!
//! ```ignore
//! use hive_protocol::security::{EncryptionManager, EncryptionKeypair};
//!
//! // Create encryption manager for this device
//! let keypair = EncryptionKeypair::generate();
//! let mut manager = EncryptionManager::new(keypair);
//!
//! // Establish secure channel with peer
//! let channel = manager.establish_channel(&peer_id, &peer_public_key)?;
//!
//! // Encrypt message for peer
//! let ciphertext = channel.encrypt(b"secret message")?;
//!
//! // Decrypt message from peer
//! let plaintext = channel.decrypt(&ciphertext)?;
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Key, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;
use tokio::sync::RwLock;
use x25519_dalek::{PublicKey, SharedSecret, StaticSecret};

use super::error::SecurityError;
use super::DeviceId;

/// Size of encryption nonce in bytes (96 bits for ChaCha20-Poly1305)
pub const NONCE_SIZE: usize = 12;

/// Size of symmetric key in bytes (256 bits)
pub const SYMMETRIC_KEY_SIZE: usize = 32;

/// Size of X25519 public key in bytes
pub const X25519_PUBLIC_KEY_SIZE: usize = 32;

/// HKDF info string for peer-to-peer key derivation
const HKDF_INFO_PEER: &[u8] = b"hive-protocol-v1-peer";

/// HKDF info string for group key derivation (reserved for future key distribution)
#[allow(dead_code)]
const HKDF_INFO_GROUP: &[u8] = b"hive-protocol-v1-group";

/// X25519 keypair for key exchange
#[derive(Clone)]
pub struct EncryptionKeypair {
    /// Secret key (kept private)
    secret: Arc<StaticSecret>,
    /// Public key (shared with peers)
    public: PublicKey,
}

impl EncryptionKeypair {
    /// Generate a new random keypair
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self {
            secret: Arc::new(secret),
            public,
        }
    }

    /// Create from existing secret key bytes
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        let secret = StaticSecret::from(*bytes);
        let public = PublicKey::from(&secret);
        Self {
            secret: Arc::new(secret),
            public,
        }
    }

    /// Get the public key
    pub fn public_key(&self) -> &PublicKey {
        &self.public
    }

    /// Get public key bytes
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }

    /// Perform Diffie-Hellman key exchange
    pub fn dh_exchange(&self, peer_public: &PublicKey) -> SharedSecret {
        self.secret.diffie_hellman(peer_public)
    }
}

impl std::fmt::Debug for EncryptionKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionKeypair")
            .field("public", &hex::encode(self.public.as_bytes()))
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

/// Symmetric key for encryption/decryption
#[derive(Clone)]
pub struct SymmetricKey {
    key: Key,
}

impl SymmetricKey {
    /// Create from raw bytes
    pub fn from_bytes(bytes: &[u8; SYMMETRIC_KEY_SIZE]) -> Self {
        Self {
            key: *Key::from_slice(bytes),
        }
    }

    /// Derive symmetric key from shared secret using HKDF
    pub fn derive_from_shared_secret(shared_secret: &SharedSecret, info: &[u8]) -> Self {
        let hk = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
        let mut key_bytes = [0u8; SYMMETRIC_KEY_SIZE];
        hk.expand(info, &mut key_bytes)
            .expect("HKDF expand should never fail with correct output length");
        Self::from_bytes(&key_bytes)
    }

    /// Derive key for peer-to-peer encryption
    pub fn derive_for_peer(shared_secret: &SharedSecret) -> Self {
        Self::derive_from_shared_secret(shared_secret, HKDF_INFO_PEER)
    }

    /// Get raw key bytes
    pub fn as_bytes(&self) -> &[u8; SYMMETRIC_KEY_SIZE] {
        // Key is guaranteed to be 32 bytes (256-bit)
        self.key[..].try_into().unwrap()
    }

    /// Encrypt data with random nonce
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedData, SecurityError> {
        let cipher = ChaCha20Poly1305::new(&self.key);
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);

        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| SecurityError::EncryptionError(e.to_string()))?;

        Ok(EncryptedData {
            nonce: nonce.into(),
            ciphertext,
        })
    }

    /// Decrypt data
    pub fn decrypt(&self, encrypted: &EncryptedData) -> Result<Vec<u8>, SecurityError> {
        let cipher = ChaCha20Poly1305::new(&self.key);
        let nonce = Nonce::from_slice(&encrypted.nonce);

        cipher
            .decrypt(nonce, encrypted.ciphertext.as_ref())
            .map_err(|e| SecurityError::DecryptionError(e.to_string()))
    }
}

impl std::fmt::Debug for SymmetricKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymmetricKey")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

/// Encrypted data with nonce
#[derive(Debug, Clone)]
pub struct EncryptedData {
    /// Nonce used for encryption
    pub nonce: [u8; NONCE_SIZE],
    /// Encrypted ciphertext (includes auth tag)
    pub ciphertext: Vec<u8>,
}

impl EncryptedData {
    /// Serialize to bytes (nonce || ciphertext)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(NONCE_SIZE + self.ciphertext.len());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SecurityError> {
        if bytes.len() < NONCE_SIZE {
            return Err(SecurityError::DecryptionError(
                "ciphertext too short for nonce".to_string(),
            ));
        }

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&bytes[..NONCE_SIZE]);
        let ciphertext = bytes[NONCE_SIZE..].to_vec();

        Ok(Self { nonce, ciphertext })
    }
}

/// Secure channel between two peers
#[derive(Debug)]
pub struct SecureChannel {
    /// Peer identifier
    pub peer_id: DeviceId,
    /// Symmetric key for this channel
    key: SymmetricKey,
    /// When channel was established
    pub established_at: SystemTime,
}

impl SecureChannel {
    /// Create new secure channel
    pub fn new(peer_id: DeviceId, key: SymmetricKey) -> Self {
        Self {
            peer_id,
            key,
            established_at: SystemTime::now(),
        }
    }

    /// Encrypt message for peer
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedData, SecurityError> {
        self.key.encrypt(plaintext)
    }

    /// Decrypt message from peer
    pub fn decrypt(&self, encrypted: &EncryptedData) -> Result<Vec<u8>, SecurityError> {
        self.key.decrypt(encrypted)
    }

    /// Get channel age in seconds
    pub fn age_secs(&self) -> u64 {
        self.established_at
            .elapsed()
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Group key for cell broadcast encryption
#[derive(Clone)]
pub struct GroupKey {
    /// Cell ID this key is for
    pub cell_id: String,
    /// Symmetric key for the group
    key: SymmetricKey,
    /// Key generation/version number
    pub generation: u64,
    /// When key was created
    pub created_at: SystemTime,
}

impl GroupKey {
    /// Generate new random group key
    pub fn generate(cell_id: String) -> Self {
        let mut key_bytes = [0u8; SYMMETRIC_KEY_SIZE];
        OsRng.fill_bytes(&mut key_bytes);
        Self {
            cell_id,
            key: SymmetricKey::from_bytes(&key_bytes),
            generation: 1,
            created_at: SystemTime::now(),
        }
    }

    /// Create from bytes with generation number
    pub fn from_bytes(
        cell_id: String,
        key_bytes: &[u8; SYMMETRIC_KEY_SIZE],
        generation: u64,
    ) -> Self {
        Self {
            cell_id,
            key: SymmetricKey::from_bytes(key_bytes),
            generation,
            created_at: SystemTime::now(),
        }
    }

    /// Encrypt message for cell broadcast
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedCellMessage, SecurityError> {
        let encrypted = self.key.encrypt(plaintext)?;
        Ok(EncryptedCellMessage {
            cell_id: self.cell_id.clone(),
            generation: self.generation,
            encrypted,
        })
    }

    /// Decrypt cell message
    pub fn decrypt(&self, message: &EncryptedCellMessage) -> Result<Vec<u8>, SecurityError> {
        if message.cell_id != self.cell_id {
            return Err(SecurityError::DecryptionError(format!(
                "cell ID mismatch: expected {}, got {}",
                self.cell_id, message.cell_id
            )));
        }
        if message.generation != self.generation {
            return Err(SecurityError::DecryptionError(format!(
                "key generation mismatch: expected {}, got {}",
                self.generation, message.generation
            )));
        }
        self.key.decrypt(&message.encrypted)
    }

    /// Rotate to new key generation
    pub fn rotate(&self) -> Self {
        let mut key_bytes = [0u8; SYMMETRIC_KEY_SIZE];
        OsRng.fill_bytes(&mut key_bytes);
        Self {
            cell_id: self.cell_id.clone(),
            key: SymmetricKey::from_bytes(&key_bytes),
            generation: self.generation + 1,
            created_at: SystemTime::now(),
        }
    }

    /// Get key bytes for distribution
    pub fn key_bytes(&self) -> [u8; SYMMETRIC_KEY_SIZE] {
        *self.key.as_bytes()
    }
}

impl std::fmt::Debug for GroupKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupKey")
            .field("cell_id", &self.cell_id)
            .field("generation", &self.generation)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

/// Encrypted message for cell broadcast
#[derive(Debug, Clone)]
pub struct EncryptedCellMessage {
    /// Cell this message is for
    pub cell_id: String,
    /// Key generation used
    pub generation: u64,
    /// Encrypted data
    pub encrypted: EncryptedData,
}

impl EncryptedCellMessage {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let cell_id_bytes = self.cell_id.as_bytes();
        let encrypted_bytes = self.encrypted.to_bytes();

        let mut bytes = Vec::new();
        // Format: cell_id_len (4 bytes) || cell_id || generation (8 bytes) || encrypted_data
        bytes.extend_from_slice(&(cell_id_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(cell_id_bytes);
        bytes.extend_from_slice(&self.generation.to_le_bytes());
        bytes.extend_from_slice(&encrypted_bytes);
        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SecurityError> {
        if bytes.len() < 12 {
            return Err(SecurityError::DecryptionError(
                "message too short".to_string(),
            ));
        }

        let cell_id_len = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        if bytes.len() < 4 + cell_id_len + 8 {
            return Err(SecurityError::DecryptionError(
                "message truncated".to_string(),
            ));
        }

        let cell_id = String::from_utf8(bytes[4..4 + cell_id_len].to_vec())
            .map_err(|e| SecurityError::DecryptionError(e.to_string()))?;
        let generation = u64::from_le_bytes(
            bytes[4 + cell_id_len..4 + cell_id_len + 8]
                .try_into()
                .unwrap(),
        );
        let encrypted = EncryptedData::from_bytes(&bytes[4 + cell_id_len + 8..])?;

        Ok(Self {
            cell_id,
            generation,
            encrypted,
        })
    }
}

/// Encrypted document for at-rest storage
#[derive(Debug, Clone)]
pub struct EncryptedDocument {
    /// Encrypted data
    pub encrypted: EncryptedData,
    /// Device ID that encrypted this document
    pub encrypted_by: DeviceId,
    /// Timestamp when encrypted
    pub encrypted_at: u64,
}

impl EncryptedDocument {
    /// Create from encrypted data
    pub fn new(encrypted: EncryptedData, device_id: DeviceId) -> Self {
        let encrypted_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            encrypted,
            encrypted_by: device_id,
            encrypted_at,
        }
    }
}

/// Encryption manager for HIVE Protocol
///
/// Manages encryption keys and secure channels:
/// - Peer-to-peer session keys via X25519 key exchange
/// - Cell group keys for broadcast encryption
/// - Device key for at-rest encryption
pub struct EncryptionManager {
    /// This device's keypair
    keypair: EncryptionKeypair,
    /// This device's ID (derived from signing keypair)
    device_id: DeviceId,
    /// Secure channels to peers
    peer_channels: Arc<RwLock<HashMap<DeviceId, SecureChannel>>>,
    /// Cell group keys
    cell_keys: Arc<RwLock<HashMap<String, GroupKey>>>,
    /// Device encryption key for at-rest
    device_key: SymmetricKey,
}

impl EncryptionManager {
    /// Create new encryption manager
    pub fn new(keypair: EncryptionKeypair, device_id: DeviceId) -> Self {
        // Derive device key from keypair public key
        let hk = Hkdf::<Sha256>::new(None, keypair.public_key_bytes().as_ref());
        let mut device_key_bytes = [0u8; SYMMETRIC_KEY_SIZE];
        hk.expand(b"hive-protocol-v1-device", &mut device_key_bytes)
            .expect("HKDF expand should never fail");

        Self {
            keypair,
            device_id,
            peer_channels: Arc::new(RwLock::new(HashMap::new())),
            cell_keys: Arc::new(RwLock::new(HashMap::new())),
            device_key: SymmetricKey::from_bytes(&device_key_bytes),
        }
    }

    /// Get this device's public key
    pub fn public_key(&self) -> &PublicKey {
        self.keypair.public_key()
    }

    /// Get this device's public key bytes
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.keypair.public_key_bytes()
    }

    /// Establish secure channel with peer via X25519 key exchange
    pub async fn establish_channel(
        &self,
        peer_id: DeviceId,
        peer_public_key: &[u8; X25519_PUBLIC_KEY_SIZE],
    ) -> Result<(), SecurityError> {
        let peer_public = PublicKey::from(*peer_public_key);
        let shared_secret = self.keypair.dh_exchange(&peer_public);
        let symmetric_key = SymmetricKey::derive_for_peer(&shared_secret);

        let channel = SecureChannel::new(peer_id, symmetric_key);
        self.peer_channels.write().await.insert(peer_id, channel);

        Ok(())
    }

    /// Get secure channel for peer
    pub async fn get_channel(&self, peer_id: &DeviceId) -> Option<SecureChannel> {
        let channels = self.peer_channels.read().await;
        channels.get(peer_id).map(|c| SecureChannel {
            peer_id: c.peer_id,
            key: c.key.clone(),
            established_at: c.established_at,
        })
    }

    /// Check if channel exists for peer
    pub async fn has_channel(&self, peer_id: &DeviceId) -> bool {
        self.peer_channels.read().await.contains_key(peer_id)
    }

    /// Remove channel (peer disconnected)
    pub async fn remove_channel(&self, peer_id: &DeviceId) {
        self.peer_channels.write().await.remove(peer_id);
    }

    /// Encrypt message for specific peer
    pub async fn encrypt_for_peer(
        &self,
        peer_id: &DeviceId,
        plaintext: &[u8],
    ) -> Result<EncryptedData, SecurityError> {
        let channels = self.peer_channels.read().await;
        let channel = channels.get(peer_id).ok_or_else(|| {
            SecurityError::EncryptionError(format!("no channel for peer: {}", peer_id))
        })?;
        channel.encrypt(plaintext)
    }

    /// Decrypt message from peer
    pub async fn decrypt_from_peer(
        &self,
        peer_id: &DeviceId,
        encrypted: &EncryptedData,
    ) -> Result<Vec<u8>, SecurityError> {
        let channels = self.peer_channels.read().await;
        let channel = channels.get(peer_id).ok_or_else(|| {
            SecurityError::DecryptionError(format!("no channel for peer: {}", peer_id))
        })?;
        channel.decrypt(encrypted)
    }

    /// Create or get group key for cell
    pub async fn get_or_create_cell_key(&self, cell_id: &str) -> GroupKey {
        let mut keys = self.cell_keys.write().await;
        if let Some(key) = keys.get(cell_id) {
            key.clone()
        } else {
            let key = GroupKey::generate(cell_id.to_string());
            keys.insert(cell_id.to_string(), key.clone());
            key
        }
    }

    /// Set cell key (received from leader)
    pub async fn set_cell_key(&self, key: GroupKey) {
        self.cell_keys
            .write()
            .await
            .insert(key.cell_id.clone(), key);
    }

    /// Get cell key
    pub async fn get_cell_key(&self, cell_id: &str) -> Option<GroupKey> {
        self.cell_keys.read().await.get(cell_id).cloned()
    }

    /// Rotate cell key (when member leaves)
    pub async fn rotate_cell_key(&self, cell_id: &str) -> Result<GroupKey, SecurityError> {
        let mut keys = self.cell_keys.write().await;
        let old_key = keys.get(cell_id).ok_or_else(|| {
            SecurityError::EncryptionError(format!("no key for cell: {}", cell_id))
        })?;
        let new_key = old_key.rotate();
        keys.insert(cell_id.to_string(), new_key.clone());
        Ok(new_key)
    }

    /// Remove cell key (left cell)
    pub async fn remove_cell_key(&self, cell_id: &str) {
        self.cell_keys.write().await.remove(cell_id);
    }

    /// Encrypt message for cell broadcast
    pub async fn encrypt_for_cell(
        &self,
        cell_id: &str,
        plaintext: &[u8],
    ) -> Result<EncryptedCellMessage, SecurityError> {
        let keys = self.cell_keys.read().await;
        let key = keys.get(cell_id).ok_or_else(|| {
            SecurityError::EncryptionError(format!("no key for cell: {}", cell_id))
        })?;
        key.encrypt(plaintext)
    }

    /// Decrypt cell message
    pub async fn decrypt_cell_message(
        &self,
        message: &EncryptedCellMessage,
    ) -> Result<Vec<u8>, SecurityError> {
        let keys = self.cell_keys.read().await;
        let key = keys.get(&message.cell_id).ok_or_else(|| {
            SecurityError::DecryptionError(format!("no key for cell: {}", message.cell_id))
        })?;
        key.decrypt(message)
    }

    /// Encrypt document for at-rest storage
    pub fn encrypt_document(&self, plaintext: &[u8]) -> Result<EncryptedDocument, SecurityError> {
        let encrypted = self.device_key.encrypt(plaintext)?;
        Ok(EncryptedDocument::new(encrypted, self.device_id))
    }

    /// Decrypt document from storage
    pub fn decrypt_document(&self, document: &EncryptedDocument) -> Result<Vec<u8>, SecurityError> {
        if document.encrypted_by != self.device_id {
            return Err(SecurityError::DecryptionError(
                "document encrypted by different device".to_string(),
            ));
        }
        self.device_key.decrypt(&document.encrypted)
    }

    /// Get number of active peer channels
    pub async fn peer_channel_count(&self) -> usize {
        self.peer_channels.read().await.len()
    }

    /// Get number of cell keys
    pub async fn cell_key_count(&self) -> usize {
        self.cell_keys.read().await.len()
    }
}

impl std::fmt::Debug for EncryptionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionManager")
            .field("device_id", &self.device_id)
            .field("public_key", &hex::encode(self.keypair.public_key_bytes()))
            .finish()
    }
}

use rand_core::RngCore;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp1 = EncryptionKeypair::generate();
        let kp2 = EncryptionKeypair::generate();

        // Different keypairs should have different public keys
        assert_ne!(kp1.public_key_bytes(), kp2.public_key_bytes());
    }

    #[test]
    fn test_keypair_from_bytes() {
        let kp1 = EncryptionKeypair::generate();
        let secret_bytes = [42u8; 32]; // Fixed secret for testing
        let kp2 = EncryptionKeypair::from_secret_bytes(&secret_bytes);
        let kp3 = EncryptionKeypair::from_secret_bytes(&secret_bytes);

        // Same secret should produce same public key
        assert_eq!(kp2.public_key_bytes(), kp3.public_key_bytes());
        // Different from random
        assert_ne!(kp1.public_key_bytes(), kp2.public_key_bytes());
    }

    #[test]
    fn test_dh_key_exchange() {
        let alice = EncryptionKeypair::generate();
        let bob = EncryptionKeypair::generate();

        // Both parties derive the same shared secret
        let alice_shared = alice.dh_exchange(bob.public_key());
        let bob_shared = bob.dh_exchange(alice.public_key());

        assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
    }

    #[test]
    fn test_symmetric_key_encrypt_decrypt() {
        let key = SymmetricKey::from_bytes(&[42u8; 32]);
        let plaintext = b"Hello, World!";

        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_symmetric_key_different_nonces() {
        let key = SymmetricKey::from_bytes(&[42u8; 32]);
        let plaintext = b"Hello, World!";

        let encrypted1 = key.encrypt(plaintext).unwrap();
        let encrypted2 = key.encrypt(plaintext).unwrap();

        // Same plaintext, different nonces = different ciphertext
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);

        // Both decrypt correctly
        assert_eq!(key.decrypt(&encrypted1).unwrap(), plaintext);
        assert_eq!(key.decrypt(&encrypted2).unwrap(), plaintext);
    }

    #[test]
    fn test_wrong_key_decryption_fails() {
        let key1 = SymmetricKey::from_bytes(&[42u8; 32]);
        let key2 = SymmetricKey::from_bytes(&[43u8; 32]);
        let plaintext = b"Hello, World!";

        let encrypted = key1.encrypt(plaintext).unwrap();
        let result = key2.decrypt(&encrypted);

        assert!(result.is_err());
    }

    #[test]
    fn test_encrypted_data_serialization() {
        let key = SymmetricKey::from_bytes(&[42u8; 32]);
        let plaintext = b"Hello, World!";

        let encrypted = key.encrypt(plaintext).unwrap();
        let bytes = encrypted.to_bytes();
        let restored = EncryptedData::from_bytes(&bytes).unwrap();

        assert_eq!(encrypted.nonce, restored.nonce);
        assert_eq!(encrypted.ciphertext, restored.ciphertext);

        let decrypted = key.decrypt(&restored).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_secure_channel() {
        let alice = EncryptionKeypair::generate();
        let bob = EncryptionKeypair::generate();

        // Derive shared keys
        let alice_shared = alice.dh_exchange(bob.public_key());
        let bob_shared = bob.dh_exchange(alice.public_key());

        let alice_key = SymmetricKey::derive_for_peer(&alice_shared);
        let bob_key = SymmetricKey::derive_for_peer(&bob_shared);

        let alice_id = DeviceId::from_bytes([1u8; 16]);
        let bob_id = DeviceId::from_bytes([2u8; 16]);

        let alice_channel = SecureChannel::new(bob_id, alice_key);
        let bob_channel = SecureChannel::new(alice_id, bob_key);

        // Alice sends to Bob
        let message = b"Secret message from Alice";
        let encrypted = alice_channel.encrypt(message).unwrap();
        let decrypted = bob_channel.decrypt(&encrypted).unwrap();

        assert_eq!(message.as_slice(), decrypted.as_slice());

        // Bob sends to Alice
        let reply = b"Reply from Bob";
        let encrypted_reply = bob_channel.encrypt(reply).unwrap();
        let decrypted_reply = alice_channel.decrypt(&encrypted_reply).unwrap();

        assert_eq!(reply.as_slice(), decrypted_reply.as_slice());
    }

    #[test]
    fn test_group_key() {
        let key = GroupKey::generate("cell-1".to_string());
        let plaintext = b"Broadcast message";

        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
        assert_eq!(encrypted.cell_id, "cell-1");
        assert_eq!(encrypted.generation, 1);
    }

    #[test]
    fn test_group_key_rotation() {
        let key1 = GroupKey::generate("cell-1".to_string());
        let key2 = key1.rotate();

        assert_eq!(key1.cell_id, key2.cell_id);
        assert_eq!(key1.generation + 1, key2.generation);
        assert_ne!(key1.key_bytes(), key2.key_bytes());

        // Old key can't decrypt new messages
        let message = b"New message";
        let encrypted = key2.encrypt(message).unwrap();
        assert!(key1.decrypt(&encrypted).is_err());

        // New key can decrypt new messages
        let decrypted = key2.decrypt(&encrypted).unwrap();
        assert_eq!(message.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypted_cell_message_serialization() {
        let key = GroupKey::generate("cell-1".to_string());
        let plaintext = b"Cell broadcast";

        let encrypted = key.encrypt(plaintext).unwrap();
        let bytes = encrypted.to_bytes();
        let restored = EncryptedCellMessage::from_bytes(&bytes).unwrap();

        assert_eq!(encrypted.cell_id, restored.cell_id);
        assert_eq!(encrypted.generation, restored.generation);

        let decrypted = key.decrypt(&restored).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[tokio::test]
    async fn test_encryption_manager_peer_channels() {
        let alice_kp = EncryptionKeypair::generate();
        let bob_kp = EncryptionKeypair::generate();

        let alice_id = DeviceId::from_bytes([1u8; 16]);
        let bob_id = DeviceId::from_bytes([2u8; 16]);

        let alice_mgr = EncryptionManager::new(alice_kp.clone(), alice_id);
        let bob_mgr = EncryptionManager::new(bob_kp.clone(), bob_id);

        // Establish channels
        alice_mgr
            .establish_channel(bob_id, &bob_mgr.public_key_bytes())
            .await
            .unwrap();
        bob_mgr
            .establish_channel(alice_id, &alice_mgr.public_key_bytes())
            .await
            .unwrap();

        // Verify channels exist
        assert!(alice_mgr.has_channel(&bob_id).await);
        assert!(bob_mgr.has_channel(&alice_id).await);

        // Alice sends to Bob
        let message = b"Hello Bob!";
        let encrypted = alice_mgr.encrypt_for_peer(&bob_id, message).await.unwrap();
        let decrypted = bob_mgr
            .decrypt_from_peer(&alice_id, &encrypted)
            .await
            .unwrap();

        assert_eq!(message.as_slice(), decrypted.as_slice());
    }

    #[tokio::test]
    async fn test_encryption_manager_cell_keys() {
        let kp = EncryptionKeypair::generate();
        let device_id = DeviceId::from_bytes([1u8; 16]);
        let mgr = EncryptionManager::new(kp, device_id);

        // Create cell key
        let key = mgr.get_or_create_cell_key("cell-1").await;
        assert_eq!(key.cell_id, "cell-1");
        assert_eq!(key.generation, 1);

        // Get same key
        let key2 = mgr.get_or_create_cell_key("cell-1").await;
        assert_eq!(key.generation, key2.generation);

        // Encrypt for cell
        let message = b"Cell message";
        let encrypted = mgr.encrypt_for_cell("cell-1", message).await.unwrap();
        let decrypted = mgr.decrypt_cell_message(&encrypted).await.unwrap();

        assert_eq!(message.as_slice(), decrypted.as_slice());

        // Rotate key
        let new_key = mgr.rotate_cell_key("cell-1").await.unwrap();
        assert_eq!(new_key.generation, 2);
    }

    #[test]
    fn test_document_encryption() {
        let kp = EncryptionKeypair::generate();
        let device_id = DeviceId::from_bytes([1u8; 16]);
        let mgr = EncryptionManager::new(kp, device_id);

        let document = b"Sensitive document content";
        let encrypted = mgr.encrypt_document(document).unwrap();
        let decrypted = mgr.decrypt_document(&encrypted).unwrap();

        assert_eq!(document.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_document_wrong_device_fails() {
        let kp1 = EncryptionKeypair::generate();
        let kp2 = EncryptionKeypair::generate();
        let device_id1 = DeviceId::from_bytes([1u8; 16]);
        let device_id2 = DeviceId::from_bytes([2u8; 16]);

        let mgr1 = EncryptionManager::new(kp1, device_id1);
        let mgr2 = EncryptionManager::new(kp2, device_id2);

        let document = b"Sensitive document";
        let encrypted = mgr1.encrypt_document(document).unwrap();

        // Different device can't decrypt
        let result = mgr2.decrypt_document(&encrypted);
        assert!(result.is_err());
    }
}
