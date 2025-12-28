//! Security module for HIVE-BTLE
//!
//! Provides two layers of encryption:
//!
//! ## Phase 1: Mesh-Wide Encryption
//!
//! All formation members share a secret and can encrypt/decrypt documents.
//! Protects against external eavesdroppers.
//!
//! ```ignore
//! use hive_btle::security::MeshEncryptionKey;
//!
//! let secret = [0x42u8; 32];
//! let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);
//! let encrypted = key.encrypt(b"document").unwrap();
//! ```
//!
//! ## Phase 2: Per-Peer E2EE
//!
//! Two specific peers establish a unique session via X25519 key exchange.
//! Only sender and recipient can decrypt - other mesh members cannot.
//!
//! ```ignore
//! use hive_btle::security::PeerSessionManager;
//! use hive_btle::NodeId;
//!
//! let mut alice = PeerSessionManager::new(NodeId::new(0x11111111));
//! let mut bob = PeerSessionManager::new(NodeId::new(0x22222222));
//!
//! // Key exchange
//! let alice_msg = alice.initiate_session(NodeId::new(0x22222222), now_ms);
//! let (bob_response, _) = bob.handle_key_exchange(&alice_msg, now_ms).unwrap();
//! alice.handle_key_exchange(&bob_response, now_ms).unwrap();
//!
//! // Now Alice and Bob can communicate securely
//! let encrypted = alice.encrypt_for_peer(NodeId::new(0x22222222), b"secret", now_ms).unwrap();
//! let decrypted = bob.decrypt_from_peer(&encrypted, now_ms).unwrap();
//! ```
//!
//! ## Encryption Layers
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  Phase 1: Mesh-Wide (Formation Key)                             │
//! │  ┌─────────────────────────────────────────────────────────┐    │
//! │  │  All formation members can decrypt                       │    │
//! │  │  Protects: External eavesdroppers                        │    │
//! │  │  Overhead: 30 bytes                                      │    │
//! │  └─────────────────────────────────────────────────────────┘    │
//! │                                                                  │
//! │  Phase 2: Per-Peer E2EE (Session Key)                           │
//! │  ┌─────────────────────────────────────────────────────────┐    │
//! │  │  Only sender + recipient can decrypt                     │    │
//! │  │  Protects: Other mesh members, compromised relays        │    │
//! │  │  Overhead: 44 bytes                                      │    │
//! │  └─────────────────────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

mod mesh_key;
mod peer_key;
mod peer_session;

// Phase 1: Mesh-wide encryption
pub use mesh_key::{EncryptedDocument, EncryptionError, MeshEncryptionKey};

// Phase 2: Per-peer E2EE
pub use peer_key::{
    EphemeralKey, KeyExchangeMessage, PeerIdentityKey, PeerSessionKey, SharedSecret,
};
pub use peer_session::{
    PeerEncryptedMessage, PeerSession, PeerSessionManager, SessionState, DEFAULT_MAX_SESSIONS,
    DEFAULT_SESSION_TIMEOUT_MS,
};
