//! Per-peer E2EE session management
//!
//! Manages the lifecycle of encrypted sessions between specific peer pairs:
//! - Session establishment via X25519 key exchange
//! - Message encryption/decryption with session keys
//! - Replay protection via message counters
//! - Session timeout and cleanup

#[cfg(not(feature = "std"))]
use alloc::{collections::BTreeMap, vec::Vec};
#[cfg(feature = "std")]
use std::collections::HashMap;

use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use rand_core::RngCore;

use super::peer_key::{KeyExchangeMessage, PeerIdentityKey, PeerSessionKey};
use super::EncryptionError;
use crate::NodeId;

/// Default session timeout (30 minutes)
pub const DEFAULT_SESSION_TIMEOUT_MS: u64 = 30 * 60 * 1000;

/// Maximum number of concurrent peer sessions
pub const DEFAULT_MAX_SESSIONS: usize = 16;

/// Session state in the E2EE handshake
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// We initiated and sent our public key, awaiting peer's response
    AwaitingPeerKey,
    /// Session established, ready for encrypted messages
    Established,
    /// Session closed or expired
    Closed,
}

/// A per-peer E2EE session
#[derive(Debug)]
pub struct PeerSession {
    /// Peer's node ID
    pub peer_node_id: NodeId,
    /// Session state
    pub state: SessionState,
    /// Derived session key (available once established)
    session_key: Option<PeerSessionKey>,
    /// Peer's public key (received during handshake)
    peer_public_key: Option<[u8; 32]>,
    /// Timestamp when session was created
    pub created_at_ms: u64,
    /// Timestamp of last activity
    pub last_activity_ms: u64,
    /// Outbound message counter (for replay protection)
    pub outbound_counter: u64,
    /// Highest inbound message counter seen (for replay protection)
    pub inbound_counter: u64,
}

impl PeerSession {
    /// Create a new session in awaiting state (we initiated)
    pub fn new_initiator(peer_node_id: NodeId, now_ms: u64) -> Self {
        Self {
            peer_node_id,
            state: SessionState::AwaitingPeerKey,
            session_key: None,
            peer_public_key: None,
            created_at_ms: now_ms,
            last_activity_ms: now_ms,
            outbound_counter: 0,
            inbound_counter: 0,
        }
    }

    /// Create a new established session (peer initiated, we're responding)
    pub fn new_responder(
        peer_node_id: NodeId,
        session_key: PeerSessionKey,
        peer_public_key: [u8; 32],
        now_ms: u64,
    ) -> Self {
        Self {
            peer_node_id,
            state: SessionState::Established,
            session_key: Some(session_key),
            peer_public_key: Some(peer_public_key),
            created_at_ms: now_ms,
            last_activity_ms: now_ms,
            outbound_counter: 0,
            inbound_counter: 0,
        }
    }

    /// Complete the handshake (transition from AwaitingPeerKey to Established)
    pub fn complete_handshake(
        &mut self,
        session_key: PeerSessionKey,
        peer_public_key: [u8; 32],
        now_ms: u64,
    ) {
        self.state = SessionState::Established;
        self.session_key = Some(session_key);
        self.peer_public_key = Some(peer_public_key);
        self.last_activity_ms = now_ms;
    }

    /// Check if session is established
    pub fn is_established(&self) -> bool {
        self.state == SessionState::Established && self.session_key.is_some()
    }

    /// Check if session is expired
    pub fn is_expired(&self, now_ms: u64, timeout_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_activity_ms) > timeout_ms
    }

    /// Get next outbound message counter and increment
    pub fn next_outbound_counter(&mut self) -> u64 {
        let counter = self.outbound_counter;
        self.outbound_counter = self.outbound_counter.wrapping_add(1);
        counter
    }

    /// Validate and update inbound message counter (replay protection)
    ///
    /// Returns true if the counter is valid (not previously seen).
    /// Uses >= check with next-counter storage to accept counter 0 initially.
    pub fn validate_inbound_counter(&mut self, counter: u64) -> bool {
        // inbound_counter stores the next expected counter (or 0 initially)
        // This allows counter 0 to be valid for the first message
        if counter >= self.inbound_counter {
            self.inbound_counter = counter.saturating_add(1);
            true
        } else {
            false
        }
    }

    /// Get the session key (if established)
    pub fn session_key(&self) -> Option<&PeerSessionKey> {
        self.session_key.as_ref()
    }

    /// Update last activity timestamp
    pub fn touch(&mut self, now_ms: u64) {
        self.last_activity_ms = now_ms;
    }

    /// Close the session
    pub fn close(&mut self) {
        self.state = SessionState::Closed;
    }
}

/// An encrypted peer-to-peer message
#[derive(Debug, Clone)]
pub struct PeerEncryptedMessage {
    /// Recipient's node ID
    pub recipient_node_id: NodeId,
    /// Sender's node ID
    pub sender_node_id: NodeId,
    /// Message counter (for replay protection)
    pub counter: u64,
    /// Random nonce (12 bytes)
    pub nonce: [u8; 12],
    /// Ciphertext with auth tag
    pub ciphertext: Vec<u8>,
}

impl PeerEncryptedMessage {
    /// Total overhead: recipient(4) + sender(4) + counter(8) + nonce(12) + tag(16) = 44 bytes
    pub const OVERHEAD: usize = 4 + 4 + 8 + 12 + 16;

    /// Encode to bytes for transmission
    ///
    /// Format: recipient(4) | sender(4) | counter(8) | nonce(12) | ciphertext
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(28 + self.ciphertext.len());
        buf.extend_from_slice(&self.recipient_node_id.as_u32().to_le_bytes());
        buf.extend_from_slice(&self.sender_node_id.as_u32().to_le_bytes());
        buf.extend_from_slice(&self.counter.to_le_bytes());
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.ciphertext);
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        // Minimum: 4 + 4 + 8 + 12 + 16 = 44 bytes (empty plaintext)
        if data.len() < 44 {
            return None;
        }

        let recipient_node_id =
            NodeId::new(u32::from_le_bytes([data[0], data[1], data[2], data[3]]));
        let sender_node_id = NodeId::new(u32::from_le_bytes([data[4], data[5], data[6], data[7]]));
        let counter = u64::from_le_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]);

        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&data[16..28]);

        let ciphertext = data[28..].to_vec();

        Some(Self {
            recipient_node_id,
            sender_node_id,
            counter,
            nonce,
            ciphertext,
        })
    }
}

/// Manager for all per-peer E2EE sessions
pub struct PeerSessionManager {
    /// Our node ID
    our_node_id: NodeId,
    /// Our long-term identity key
    identity_key: PeerIdentityKey,
    /// Active sessions by peer node ID
    #[cfg(feature = "std")]
    sessions: HashMap<NodeId, PeerSession>,
    #[cfg(not(feature = "std"))]
    sessions: BTreeMap<NodeId, PeerSession>,
    /// Maximum number of concurrent sessions
    max_sessions: usize,
    /// Session timeout in milliseconds
    session_timeout_ms: u64,
}

impl PeerSessionManager {
    /// Create a new session manager with a generated identity key
    pub fn new(our_node_id: NodeId) -> Self {
        Self {
            our_node_id,
            identity_key: PeerIdentityKey::generate(),
            #[cfg(feature = "std")]
            sessions: HashMap::new(),
            #[cfg(not(feature = "std"))]
            sessions: BTreeMap::new(),
            max_sessions: DEFAULT_MAX_SESSIONS,
            session_timeout_ms: DEFAULT_SESSION_TIMEOUT_MS,
        }
    }

    /// Create with a specific identity key
    pub fn with_identity_key(our_node_id: NodeId, identity_key: PeerIdentityKey) -> Self {
        Self {
            our_node_id,
            identity_key,
            #[cfg(feature = "std")]
            sessions: HashMap::new(),
            #[cfg(not(feature = "std"))]
            sessions: BTreeMap::new(),
            max_sessions: DEFAULT_MAX_SESSIONS,
            session_timeout_ms: DEFAULT_SESSION_TIMEOUT_MS,
        }
    }

    /// Configure maximum sessions
    pub fn with_max_sessions(mut self, max: usize) -> Self {
        self.max_sessions = max;
        self
    }

    /// Configure session timeout
    pub fn with_session_timeout(mut self, timeout_ms: u64) -> Self {
        self.session_timeout_ms = timeout_ms;
        self
    }

    /// Get our public key bytes (for sharing with peers)
    pub fn our_public_key(&self) -> [u8; 32] {
        self.identity_key.public_key_bytes()
    }

    /// Get our node ID
    pub fn our_node_id(&self) -> NodeId {
        self.our_node_id
    }

    /// Initiate an E2EE session with a peer
    ///
    /// Returns a key exchange message to send to the peer.
    pub fn initiate_session(&mut self, peer_node_id: NodeId, now_ms: u64) -> KeyExchangeMessage {
        // Create session in awaiting state
        let session = PeerSession::new_initiator(peer_node_id, now_ms);
        self.sessions.insert(peer_node_id, session);

        // Enforce max sessions limit
        self.enforce_session_limit(now_ms);

        // Return key exchange message
        KeyExchangeMessage::new(
            self.our_node_id,
            self.identity_key.public_key_bytes(),
            false,
        )
    }

    /// Handle incoming key exchange message from peer
    ///
    /// Returns:
    /// - `Some((response, established))` if we should respond (response is our key exchange message)
    /// - `None` if the message is invalid or session limit reached
    pub fn handle_key_exchange(
        &mut self,
        msg: &KeyExchangeMessage,
        now_ms: u64,
    ) -> Option<(KeyExchangeMessage, bool)> {
        let peer_node_id = msg.sender_node_id;
        let peer_public = x25519_dalek::PublicKey::from(msg.public_key);

        // Compute shared secret
        let shared_secret = self.identity_key.exchange(&peer_public);
        let session_key = shared_secret.derive_session_key(self.our_node_id, peer_node_id);

        // Check if we have an existing session
        if let Some(session) = self.sessions.get_mut(&peer_node_id) {
            if session.state == SessionState::AwaitingPeerKey {
                // We initiated, peer is responding - complete handshake
                session.complete_handshake(session_key, msg.public_key, now_ms);
                return Some((
                    KeyExchangeMessage::new(
                        self.our_node_id,
                        self.identity_key.public_key_bytes(),
                        false,
                    ),
                    true, // session now established
                ));
            }
            // Already established or closed - ignore
            return None;
        }

        // Peer initiated - create new session as responder
        if self.sessions.len() >= self.max_sessions {
            // Try to clean up expired sessions
            self.cleanup_expired(now_ms);
            if self.sessions.len() >= self.max_sessions {
                log::warn!(
                    "Cannot accept E2EE session from {:?}: max sessions reached",
                    peer_node_id
                );
                return None;
            }
        }

        let session = PeerSession::new_responder(peer_node_id, session_key, msg.public_key, now_ms);
        self.sessions.insert(peer_node_id, session);

        // Return our key exchange response
        Some((
            KeyExchangeMessage::new(
                self.our_node_id,
                self.identity_key.public_key_bytes(),
                false,
            ),
            true, // session established
        ))
    }

    /// Check if we have an established session with a peer
    pub fn has_session(&self, peer_node_id: NodeId) -> bool {
        self.sessions
            .get(&peer_node_id)
            .is_some_and(|s| s.is_established())
    }

    /// Get session state for a peer
    pub fn session_state(&self, peer_node_id: NodeId) -> Option<SessionState> {
        self.sessions.get(&peer_node_id).map(|s| s.state)
    }

    /// Encrypt a message for a specific peer
    ///
    /// Returns the encrypted message, or an error if no established session exists.
    pub fn encrypt_for_peer(
        &mut self,
        peer_node_id: NodeId,
        plaintext: &[u8],
        now_ms: u64,
    ) -> Result<PeerEncryptedMessage, EncryptionError> {
        let session = self
            .sessions
            .get_mut(&peer_node_id)
            .ok_or(EncryptionError::EncryptionFailed)?;

        if !session.is_established() {
            return Err(EncryptionError::EncryptionFailed);
        }

        // Copy the key bytes before making mutable calls to session
        let session_key_bytes = *session
            .session_key()
            .ok_or(EncryptionError::EncryptionFailed)?
            .as_bytes();
        let counter = session.next_outbound_counter();
        session.touch(now_ms);

        // Create cipher
        let cipher = ChaCha20Poly1305::new_from_slice(&session_key_bytes)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        Ok(PeerEncryptedMessage {
            recipient_node_id: peer_node_id,
            sender_node_id: self.our_node_id,
            counter,
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    /// Decrypt a message from a peer
    ///
    /// Returns the plaintext, or an error if decryption fails.
    pub fn decrypt_from_peer(
        &mut self,
        msg: &PeerEncryptedMessage,
        now_ms: u64,
    ) -> Result<Vec<u8>, EncryptionError> {
        // Verify we're the intended recipient
        if msg.recipient_node_id != self.our_node_id {
            return Err(EncryptionError::DecryptionFailed);
        }

        let session = self
            .sessions
            .get_mut(&msg.sender_node_id)
            .ok_or(EncryptionError::DecryptionFailed)?;

        if !session.is_established() {
            return Err(EncryptionError::DecryptionFailed);
        }

        // Replay protection - counter must be >= next expected counter
        if !session.validate_inbound_counter(msg.counter) {
            log::warn!(
                "Replay attack detected from {:?}: counter {} < next expected {}",
                msg.sender_node_id,
                msg.counter,
                session.inbound_counter
            );
            return Err(EncryptionError::DecryptionFailed);
        }

        // Copy the key bytes before making mutable calls to session
        let session_key_bytes = *session
            .session_key()
            .ok_or(EncryptionError::DecryptionFailed)?
            .as_bytes();
        session.touch(now_ms);

        // Create cipher
        let cipher = ChaCha20Poly1305::new_from_slice(&session_key_bytes)
            .map_err(|_| EncryptionError::DecryptionFailed)?;

        let nonce = Nonce::from_slice(&msg.nonce);

        // Decrypt
        cipher
            .decrypt(nonce, msg.ciphertext.as_ref())
            .map_err(|_| EncryptionError::DecryptionFailed)
    }

    /// Close a session with a peer
    pub fn close_session(&mut self, peer_node_id: NodeId) {
        if let Some(session) = self.sessions.get_mut(&peer_node_id) {
            session.close();
        }
    }

    /// Remove a session entirely
    pub fn remove_session(&mut self, peer_node_id: NodeId) -> Option<PeerSession> {
        self.sessions.remove(&peer_node_id)
    }

    /// Cleanup expired sessions
    pub fn cleanup_expired(&mut self, now_ms: u64) -> Vec<NodeId> {
        let timeout = self.session_timeout_ms;
        let expired: Vec<NodeId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.is_expired(now_ms, timeout))
            .map(|(id, _)| *id)
            .collect();

        for id in &expired {
            self.sessions.remove(id);
        }

        expired
    }

    /// Get number of active sessions
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get number of established sessions
    pub fn established_count(&self) -> usize {
        self.sessions
            .values()
            .filter(|s| s.is_established())
            .count()
    }

    /// Enforce max sessions limit by removing oldest expired or closed sessions
    fn enforce_session_limit(&mut self, now_ms: u64) {
        // First try to remove expired sessions
        self.cleanup_expired(now_ms);

        // If still over limit, remove oldest closed sessions
        while self.sessions.len() > self.max_sessions {
            let oldest = self
                .sessions
                .iter()
                .filter(|(_, s)| s.state == SessionState::Closed)
                .min_by_key(|(_, s)| s.last_activity_ms)
                .map(|(id, _)| *id);

            if let Some(id) = oldest {
                self.sessions.remove(&id);
            } else {
                // No closed sessions to remove, remove oldest non-established
                let oldest = self
                    .sessions
                    .iter()
                    .filter(|(_, s)| !s.is_established())
                    .min_by_key(|(_, s)| s.last_activity_ms)
                    .map(|(id, _)| *id);

                if let Some(id) = oldest {
                    self.sessions.remove(&id);
                } else {
                    break; // Can't remove any more
                }
            }
        }
    }
}

impl core::fmt::Debug for PeerSessionManager {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeerSessionManager")
            .field("our_node_id", &self.our_node_id)
            .field("session_count", &self.sessions.len())
            .field("max_sessions", &self.max_sessions)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_manager_creation() {
        let manager = PeerSessionManager::new(NodeId::new(0x11111111));
        assert_eq!(manager.our_node_id().as_u32(), 0x11111111);
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_initiate_session() {
        let mut manager = PeerSessionManager::new(NodeId::new(0x11111111));
        let msg = manager.initiate_session(NodeId::new(0x22222222), 1000);

        assert_eq!(msg.sender_node_id.as_u32(), 0x11111111);
        assert_eq!(manager.session_count(), 1);
        assert_eq!(
            manager.session_state(NodeId::new(0x22222222)),
            Some(SessionState::AwaitingPeerKey)
        );
    }

    #[test]
    fn test_full_key_exchange() {
        let mut alice = PeerSessionManager::new(NodeId::new(0x11111111));
        let mut bob = PeerSessionManager::new(NodeId::new(0x22222222));

        // Alice initiates
        let alice_msg = alice.initiate_session(NodeId::new(0x22222222), 1000);

        // Bob receives and responds
        let (bob_response, bob_established) = bob.handle_key_exchange(&alice_msg, 1000).unwrap();
        assert!(bob_established);
        assert!(bob.has_session(NodeId::new(0x11111111)));

        // Alice receives Bob's response
        let (_, alice_established) = alice.handle_key_exchange(&bob_response, 1000).unwrap();
        assert!(alice_established);
        assert!(alice.has_session(NodeId::new(0x22222222)));
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let mut alice = PeerSessionManager::new(NodeId::new(0x11111111));
        let mut bob = PeerSessionManager::new(NodeId::new(0x22222222));

        // Establish session
        let alice_msg = alice.initiate_session(NodeId::new(0x22222222), 1000);
        let (bob_response, _) = bob.handle_key_exchange(&alice_msg, 1000).unwrap();
        alice.handle_key_exchange(&bob_response, 1000).unwrap();

        // Alice sends to Bob
        let plaintext = b"Hello, Bob!";
        let encrypted = alice
            .encrypt_for_peer(NodeId::new(0x22222222), plaintext, 2000)
            .unwrap();

        // Bob decrypts
        let decrypted = bob.decrypt_from_peer(&encrypted, 2000).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_bidirectional_communication() {
        let mut alice = PeerSessionManager::new(NodeId::new(0x11111111));
        let mut bob = PeerSessionManager::new(NodeId::new(0x22222222));

        // Establish session
        let alice_msg = alice.initiate_session(NodeId::new(0x22222222), 1000);
        let (bob_response, _) = bob.handle_key_exchange(&alice_msg, 1000).unwrap();
        alice.handle_key_exchange(&bob_response, 1000).unwrap();

        // Alice -> Bob
        let msg1 = alice
            .encrypt_for_peer(NodeId::new(0x22222222), b"From Alice", 2000)
            .unwrap();
        let dec1 = bob.decrypt_from_peer(&msg1, 2000).unwrap();
        assert_eq!(dec1, b"From Alice");

        // Bob -> Alice
        let msg2 = bob
            .encrypt_for_peer(NodeId::new(0x11111111), b"From Bob", 2000)
            .unwrap();
        let dec2 = alice.decrypt_from_peer(&msg2, 2000).unwrap();
        assert_eq!(dec2, b"From Bob");
    }

    #[test]
    fn test_replay_protection() {
        let mut alice = PeerSessionManager::new(NodeId::new(0x11111111));
        let mut bob = PeerSessionManager::new(NodeId::new(0x22222222));

        // Establish session
        let alice_msg = alice.initiate_session(NodeId::new(0x22222222), 1000);
        let (bob_response, _) = bob.handle_key_exchange(&alice_msg, 1000).unwrap();
        alice.handle_key_exchange(&bob_response, 1000).unwrap();

        // Send message
        let encrypted = alice
            .encrypt_for_peer(NodeId::new(0x22222222), b"Message", 2000)
            .unwrap();

        // First decrypt succeeds
        let result1 = bob.decrypt_from_peer(&encrypted, 2000);
        assert!(result1.is_ok());

        // Replay attempt fails
        let result2 = bob.decrypt_from_peer(&encrypted, 2000);
        assert!(result2.is_err());
    }

    #[test]
    fn test_wrong_recipient_rejected() {
        let mut alice = PeerSessionManager::new(NodeId::new(0x11111111));
        let mut bob = PeerSessionManager::new(NodeId::new(0x22222222));
        let mut charlie = PeerSessionManager::new(NodeId::new(0x33333333));

        // Alice-Bob session
        let alice_msg = alice.initiate_session(NodeId::new(0x22222222), 1000);
        let (bob_response, _) = bob.handle_key_exchange(&alice_msg, 1000).unwrap();
        alice.handle_key_exchange(&bob_response, 1000).unwrap();

        // Alice sends to Bob
        let encrypted = alice
            .encrypt_for_peer(NodeId::new(0x22222222), b"For Bob", 2000)
            .unwrap();

        // Charlie tries to decrypt (no session, should fail)
        let result = charlie.decrypt_from_peer(&encrypted, 2000);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_expiry() {
        let mut manager =
            PeerSessionManager::new(NodeId::new(0x11111111)).with_session_timeout(10_000);

        // Create session at t=1000
        manager.initiate_session(NodeId::new(0x22222222), 1000);

        // Not expired at t=5000
        let expired = manager.cleanup_expired(5000);
        assert!(expired.is_empty());
        assert_eq!(manager.session_count(), 1);

        // Expired at t=20000 (10 seconds after last activity)
        let expired = manager.cleanup_expired(20000);
        assert_eq!(expired.len(), 1);
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_max_sessions_limit() {
        let mut manager = PeerSessionManager::new(NodeId::new(0x11111111)).with_max_sessions(2);

        manager.initiate_session(NodeId::new(0x22222222), 1000);
        manager.initiate_session(NodeId::new(0x33333333), 2000);
        manager.initiate_session(NodeId::new(0x44444444), 3000);

        // Should have evicted oldest to make room
        assert!(manager.session_count() <= 2);
    }

    #[test]
    fn test_peer_encrypted_message_encode_decode() {
        // Ciphertext must be at least 16 bytes (auth tag) for decode to succeed
        let ciphertext = vec![
            0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            0x99, 0x00,
        ];
        let msg = PeerEncryptedMessage {
            recipient_node_id: NodeId::new(0x22222222),
            sender_node_id: NodeId::new(0x11111111),
            counter: 42,
            nonce: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            ciphertext: ciphertext.clone(),
        };

        let encoded = msg.encode();
        let decoded = PeerEncryptedMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.recipient_node_id.as_u32(), 0x22222222);
        assert_eq!(decoded.sender_node_id.as_u32(), 0x11111111);
        assert_eq!(decoded.counter, 42);
        assert_eq!(decoded.nonce, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(decoded.ciphertext, ciphertext);
    }

    #[test]
    fn test_close_session() {
        let mut manager = PeerSessionManager::new(NodeId::new(0x11111111));
        manager.initiate_session(NodeId::new(0x22222222), 1000);

        manager.close_session(NodeId::new(0x22222222));
        assert_eq!(
            manager.session_state(NodeId::new(0x22222222)),
            Some(SessionState::Closed)
        );
    }
}
