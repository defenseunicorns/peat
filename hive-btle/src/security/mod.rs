//! Mesh-wide encryption for HIVE-BTLE
//!
//! Provides ChaCha20-Poly1305 encryption for documents using a shared mesh key
//! derived from a formation secret. This enables confidentiality across multi-hop
//! BLE relay - intermediate nodes can forward encrypted documents without being
//! able to read their contents (unless they have the formation key).
//!
//! ## Design
//!
//! - **Algorithm**: ChaCha20-Poly1305 AEAD (authenticated encryption)
//! - **Key derivation**: HKDF-SHA256 from shared secret with mesh ID as context
//! - **Nonce**: Random 12 bytes per encryption (included in ciphertext)
//! - **Overhead**: 28 bytes (12-byte nonce + 16-byte auth tag)
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::security::MeshEncryptionKey;
//!
//! // Derive key from shared secret (all nodes use same secret)
//! let secret = [0x42u8; 32];
//! let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);
//!
//! // Encrypt document
//! let plaintext = b"HIVE document bytes...";
//! let encrypted = key.encrypt(plaintext).unwrap();
//!
//! // Decrypt document
//! let decrypted = key.decrypt(&encrypted).unwrap();
//! assert_eq!(plaintext.as_slice(), decrypted.as_slice());
//! ```

mod mesh_key;

pub use mesh_key::{EncryptedDocument, EncryptionError, MeshEncryptionKey};
