//! X25519 key exchange for per-peer E2EE
//!
//! Provides Diffie-Hellman key exchange using Curve25519 (X25519) to establish
//! unique shared secrets between specific peer pairs. Combined with ChaCha20-Poly1305,
//! this enables end-to-end encryption where only the sender and recipient can
//! decrypt messages - even other mesh members with the formation key cannot read them.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use hkdf::Hkdf;
use rand_core::OsRng;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

use crate::NodeId;

/// HKDF info context for per-peer session key derivation
const PEER_E2EE_HKDF_INFO: &[u8] = b"HIVE-peer-e2ee-v1";

/// A long-term X25519 keypair for peer identity
///
/// Used to establish E2EE sessions with other peers. The public key can be
/// shared freely; the secret key must be kept private.
#[derive(Clone)]
pub struct PeerIdentityKey {
    secret: StaticSecret,
    public: PublicKey,
}

impl PeerIdentityKey {
    /// Generate a new random identity keypair
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Create from an existing secret key bytes
    pub fn from_secret_bytes(bytes: [u8; 32]) -> Self {
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Get the public key bytes (safe to share)
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }

    /// Get a reference to the public key
    pub fn public_key(&self) -> &PublicKey {
        &self.public
    }

    /// Perform X25519 key exchange with a peer's public key
    ///
    /// Returns a shared secret that both parties will derive identically.
    pub fn exchange(&self, peer_public: &PublicKey) -> SharedSecret {
        let shared = self.secret.diffie_hellman(peer_public);
        SharedSecret {
            bytes: shared.to_bytes(),
        }
    }

    /// Perform key exchange with peer's public key bytes
    pub fn exchange_with_bytes(&self, peer_public_bytes: &[u8; 32]) -> SharedSecret {
        let peer_public = PublicKey::from(*peer_public_bytes);
        self.exchange(&peer_public)
    }
}

impl core::fmt::Debug for PeerIdentityKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeerIdentityKey")
            .field("public", &hex_short(&self.public.to_bytes()))
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

/// An ephemeral X25519 keypair for forward secrecy
///
/// Used for a single key exchange and then discarded. Provides forward secrecy:
/// if the long-term key is compromised, past sessions remain secure.
pub struct EphemeralKey {
    secret: EphemeralSecret,
    public: PublicKey,
}

impl EphemeralKey {
    /// Generate a new random ephemeral keypair
    pub fn generate() -> Self {
        let secret = EphemeralSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Get the public key bytes (safe to share)
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }

    /// Perform X25519 key exchange (consumes the ephemeral secret)
    pub fn exchange(self, peer_public: &PublicKey) -> SharedSecret {
        let shared = self.secret.diffie_hellman(peer_public);
        SharedSecret {
            bytes: shared.to_bytes(),
        }
    }

    /// Perform key exchange with peer's public key bytes (consumes self)
    pub fn exchange_with_bytes(self, peer_public_bytes: &[u8; 32]) -> SharedSecret {
        let peer_public = PublicKey::from(*peer_public_bytes);
        self.exchange(&peer_public)
    }
}

impl core::fmt::Debug for EphemeralKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EphemeralKey")
            .field("public", &hex_short(&self.public.to_bytes()))
            .finish()
    }
}

/// Raw shared secret from X25519 key exchange
///
/// This should be processed through HKDF to derive the actual session key.
/// Never use the raw shared secret directly for encryption.
pub struct SharedSecret {
    bytes: [u8; 32],
}

impl SharedSecret {
    /// Derive a session key for peer E2EE communication
    ///
    /// Uses HKDF-SHA256 with the node IDs as salt to bind the key to this
    /// specific peer pair. The node IDs are sorted to ensure both peers
    /// derive the same key regardless of who initiated.
    ///
    /// # Arguments
    /// * `our_node_id` - Our node identifier
    /// * `peer_node_id` - Peer's node identifier
    ///
    /// # Returns
    /// A 32-byte session key suitable for ChaCha20-Poly1305
    pub fn derive_session_key(&self, our_node_id: NodeId, peer_node_id: NodeId) -> PeerSessionKey {
        // Sort node IDs to ensure both peers derive the same key
        let (id1, id2) = if our_node_id.as_u32() < peer_node_id.as_u32() {
            (our_node_id.as_u32(), peer_node_id.as_u32())
        } else {
            (peer_node_id.as_u32(), our_node_id.as_u32())
        };

        // Create salt from sorted node IDs
        let mut salt = [0u8; 8];
        salt[..4].copy_from_slice(&id1.to_le_bytes());
        salt[4..].copy_from_slice(&id2.to_le_bytes());

        // Derive session key using HKDF
        let hk = Hkdf::<Sha256>::new(Some(&salt), &self.bytes);
        let mut key = [0u8; 32];
        hk.expand(PEER_E2EE_HKDF_INFO, &mut key)
            .expect("32 bytes is valid output length for HKDF-SHA256");

        PeerSessionKey { key }
    }
}

impl core::fmt::Debug for SharedSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SharedSecret")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

/// Session key for per-peer E2EE encryption
///
/// Derived from the X25519 shared secret via HKDF. Used with ChaCha20-Poly1305
/// for authenticated encryption of peer-to-peer messages.
#[derive(Clone)]
pub struct PeerSessionKey {
    key: [u8; 32],
}

impl PeerSessionKey {
    /// Get the raw key bytes for use with ChaCha20-Poly1305
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }
}

impl core::fmt::Debug for PeerSessionKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeerSessionKey")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

/// Key exchange message sent to initiate or respond to E2EE session
#[derive(Debug, Clone)]
pub struct KeyExchangeMessage {
    /// Sender's node ID
    pub sender_node_id: NodeId,
    /// Sender's public key (32 bytes)
    pub public_key: [u8; 32],
    /// Whether this is using an ephemeral key (for forward secrecy)
    pub is_ephemeral: bool,
}

impl KeyExchangeMessage {
    /// Create a new key exchange message
    pub fn new(sender_node_id: NodeId, public_key: [u8; 32], is_ephemeral: bool) -> Self {
        Self {
            sender_node_id,
            public_key,
            is_ephemeral,
        }
    }

    /// Encode to bytes for transmission
    ///
    /// Format: sender_node_id(4) | flags(1) | public_key(32) = 37 bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(37);
        buf.extend_from_slice(&self.sender_node_id.as_u32().to_le_bytes());
        buf.push(if self.is_ephemeral { 0x01 } else { 0x00 });
        buf.extend_from_slice(&self.public_key);
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 37 {
            return None;
        }

        let sender_node_id = NodeId::new(u32::from_le_bytes([data[0], data[1], data[2], data[3]]));
        let is_ephemeral = data[4] & 0x01 != 0;
        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(&data[5..37]);

        Some(Self {
            sender_node_id,
            public_key,
            is_ephemeral,
        })
    }
}

/// Helper to format bytes as short hex for debug output
fn hex_short(bytes: &[u8]) -> String {
    if bytes.len() <= 4 {
        hex::encode(bytes)
    } else {
        format!(
            "{}..{}",
            hex::encode(&bytes[..2]),
            hex::encode(&bytes[bytes.len() - 2..])
        )
    }
}

// We need hex for debug formatting
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_key_generation() {
        let key1 = PeerIdentityKey::generate();
        let key2 = PeerIdentityKey::generate();

        // Different keys should have different public keys
        assert_ne!(key1.public_key_bytes(), key2.public_key_bytes());
    }

    #[test]
    fn test_identity_key_from_bytes() {
        let key1 = PeerIdentityKey::generate();
        let secret_bytes = key1.secret.to_bytes();

        let key2 = PeerIdentityKey::from_secret_bytes(secret_bytes);

        // Same secret should produce same public key
        assert_eq!(key1.public_key_bytes(), key2.public_key_bytes());
    }

    #[test]
    fn test_key_exchange_produces_same_shared_secret() {
        let alice = PeerIdentityKey::generate();
        let bob = PeerIdentityKey::generate();

        // Alice computes shared secret with Bob's public key
        let alice_shared = alice.exchange(bob.public_key());

        // Bob computes shared secret with Alice's public key
        let bob_shared = bob.exchange(alice.public_key());

        // Both should have the same shared secret
        assert_eq!(alice_shared.bytes, bob_shared.bytes);
    }

    #[test]
    fn test_session_key_derivation_is_symmetric() {
        let alice = PeerIdentityKey::generate();
        let bob = PeerIdentityKey::generate();

        let alice_node = NodeId::new(0x11111111);
        let bob_node = NodeId::new(0x22222222);

        let alice_shared = alice.exchange(bob.public_key());
        let bob_shared = bob.exchange(alice.public_key());

        // Derive session keys (note: different order of node IDs)
        let alice_session = alice_shared.derive_session_key(alice_node, bob_node);
        let bob_session = bob_shared.derive_session_key(bob_node, alice_node);

        // Both should derive the same session key
        assert_eq!(alice_session.key, bob_session.key);
    }

    #[test]
    fn test_different_peers_get_different_session_keys() {
        let alice = PeerIdentityKey::generate();
        let bob = PeerIdentityKey::generate();
        let charlie = PeerIdentityKey::generate();

        let alice_node = NodeId::new(0x11111111);
        let bob_node = NodeId::new(0x22222222);
        let charlie_node = NodeId::new(0x33333333);

        // Alice-Bob session
        let alice_bob_shared = alice.exchange(bob.public_key());
        let alice_bob_session = alice_bob_shared.derive_session_key(alice_node, bob_node);

        // Alice-Charlie session
        let alice_charlie_shared = alice.exchange(charlie.public_key());
        let alice_charlie_session =
            alice_charlie_shared.derive_session_key(alice_node, charlie_node);

        // Different peer pairs should have different session keys
        assert_ne!(alice_bob_session.key, alice_charlie_session.key);
    }

    #[test]
    fn test_ephemeral_key_exchange() {
        let alice_static = PeerIdentityKey::generate();
        let bob_ephemeral = EphemeralKey::generate();

        let bob_public_bytes = bob_ephemeral.public_key_bytes();

        // Alice uses Bob's ephemeral public key
        let alice_shared = alice_static.exchange_with_bytes(&bob_public_bytes);

        // Bob uses Alice's static public key (consumes ephemeral)
        let bob_shared = bob_ephemeral.exchange(alice_static.public_key());

        // Both should have the same shared secret
        assert_eq!(alice_shared.bytes, bob_shared.bytes);
    }

    #[test]
    fn test_key_exchange_message_encode_decode() {
        let key = PeerIdentityKey::generate();
        let msg = KeyExchangeMessage::new(NodeId::new(0x12345678), key.public_key_bytes(), true);

        let encoded = msg.encode();
        assert_eq!(encoded.len(), 37);

        let decoded = KeyExchangeMessage::decode(&encoded).unwrap();
        assert_eq!(decoded.sender_node_id.as_u32(), 0x12345678);
        assert_eq!(decoded.public_key, key.public_key_bytes());
        assert!(decoded.is_ephemeral);
    }

    #[test]
    fn test_key_exchange_message_static_flag() {
        let key = PeerIdentityKey::generate();
        let msg = KeyExchangeMessage::new(
            NodeId::new(0xAABBCCDD),
            key.public_key_bytes(),
            false, // static key
        );

        let encoded = msg.encode();
        let decoded = KeyExchangeMessage::decode(&encoded).unwrap();

        assert!(!decoded.is_ephemeral);
    }

    #[test]
    fn test_key_exchange_message_decode_too_short() {
        let short_data = [0u8; 36]; // Need 37 bytes
        assert!(KeyExchangeMessage::decode(&short_data).is_none());
    }

    #[test]
    fn test_debug_redacts_secrets() {
        let key = PeerIdentityKey::generate();
        let debug_str = format!("{:?}", key);

        assert!(debug_str.contains("REDACTED"));
        // Should not contain raw key bytes
    }
}
