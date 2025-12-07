//! # Formation Key - Shared Secret Authentication for HIVE Formations
//!
//! Provides pre-shared key (PSK) authentication for formation membership,
//! similar to Ditto's SharedKey identity model.
//!
//! ## Overview
//!
//! A formation key is a shared secret that all nodes in a formation possess.
//! When two nodes connect, they perform a challenge-response handshake to
//! verify mutual possession of the key before allowing sync operations.
//!
//! ## Security Model
//!
//! - **HMAC-SHA256** challenge-response (not replay-able)
//! - **Key derivation** from shared secret using HKDF
//! - **Formation isolation** - different formations can't sync
//!
//! ## Usage
//!
//! ```ignore
//! use hive_protocol::security::FormationKey;
//!
//! // Create formation key from shared secret
//! let secret = [0x42u8; 32]; // In practice, generate securely
//! let key = FormationKey::new("alpha-company", &secret);
//!
//! // Responder generates challenge
//! let (nonce, expected_response) = key.create_challenge();
//!
//! // Initiator computes response
//! let response = key.respond_to_challenge(&nonce);
//!
//! // Responder verifies
//! assert!(key.verify_response(&nonce, &response));
//! ```
//!
//! ## Wire Protocol
//!
//! After QUIC connection establishment:
//!
//! ```text
//! Initiator                              Responder
//!     |                                      |
//!     |  ---- QUIC Connect (TLS) ---->       |
//!     |                                      |
//!     |  <---- Challenge (32-byte nonce) --- |
//!     |                                      |
//!     |  ---- Response (32-byte HMAC) ---->  |
//!     |                                      |
//!     |  <---- OK / REJECT ---------------   |
//!     |                                      |
//! ```

use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use sha2::Sha256;

use super::error::SecurityError;

/// Size of challenge nonce in bytes
pub const FORMATION_CHALLENGE_SIZE: usize = 32;

/// Size of challenge response (HMAC-SHA256 output)
pub const FORMATION_RESPONSE_SIZE: usize = 32;

/// HKDF info string for formation key derivation
const HKDF_INFO_FORMATION: &[u8] = b"hive-protocol-v1-formation";

type HmacSha256 = Hmac<Sha256>;

/// Formation key for shared secret authentication
///
/// All nodes in a formation share this key. Used for challenge-response
/// authentication during connection establishment.
#[derive(Clone)]
pub struct FormationKey {
    /// Formation identifier (e.g., "alpha-company")
    formation_id: String,
    /// Derived HMAC key (32 bytes)
    hmac_key: [u8; 32],
}

impl FormationKey {
    /// Create a new formation key from a shared secret
    ///
    /// The key is derived using HKDF-SHA256 with the formation ID as context,
    /// ensuring different formations have different derived keys even with
    /// the same base secret.
    ///
    /// # Arguments
    ///
    /// * `formation_id` - Unique identifier for the formation
    /// * `shared_secret` - 32-byte shared secret (should be cryptographically random)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let secret = [0x42u8; 32];
    /// let key = FormationKey::new("alpha-company", &secret);
    /// ```
    pub fn new(formation_id: &str, shared_secret: &[u8; 32]) -> Self {
        use hkdf::Hkdf;

        // Derive HMAC key using HKDF
        // Salt = formation_id, IKM = shared_secret, info = HKDF_INFO_FORMATION
        let hk = Hkdf::<Sha256>::new(Some(formation_id.as_bytes()), shared_secret);
        let mut hmac_key = [0u8; 32];
        hk.expand(HKDF_INFO_FORMATION, &mut hmac_key)
            .expect("HKDF expand should never fail with 32-byte output");

        Self {
            formation_id: formation_id.to_string(),
            hmac_key,
        }
    }

    /// Create a formation key from a base64-encoded shared secret
    ///
    /// # Arguments
    ///
    /// * `formation_id` - Unique identifier for the formation
    /// * `base64_secret` - Base64-encoded shared secret (any length)
    ///
    /// # Returns
    ///
    /// `FormationKey` or error if base64 is invalid
    ///
    /// # Key Derivation
    ///
    /// - If the decoded secret is exactly 32 bytes, it's used directly
    /// - Otherwise, SHA-256 is used to derive a 32-byte key from the input
    ///
    /// This allows using the same shared secret format as Ditto (EC private key)
    /// while deriving a compatible symmetric key for Automerge peer authentication.
    pub fn from_base64(formation_id: &str, base64_secret: &str) -> Result<Self, SecurityError> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use sha2::{Digest, Sha256};

        let secret_bytes = STANDARD.decode(base64_secret.trim()).map_err(|e| {
            SecurityError::AuthenticationFailed(format!("Invalid base64 shared secret: {}", e))
        })?;

        let secret: [u8; 32] = if secret_bytes.len() == 32 {
            // Exact 32 bytes - use directly (backwards compatible)
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&secret_bytes);
            arr
        } else {
            // Derive 32-byte key using SHA-256 (supports EC keys, etc.)
            let mut hasher = Sha256::new();
            hasher.update(&secret_bytes);
            hasher.finalize().into()
        };

        Ok(Self::new(formation_id, &secret))
    }

    /// Generate a random 32-byte shared secret (for initial setup)
    ///
    /// # Returns
    ///
    /// Base64-encoded 32-byte random secret
    pub fn generate_secret() -> String {
        use base64::{engine::general_purpose::STANDARD, Engine};

        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        STANDARD.encode(secret)
    }

    /// Get the formation ID
    pub fn formation_id(&self) -> &str {
        &self.formation_id
    }

    /// Create a challenge for the peer to respond to
    ///
    /// # Returns
    ///
    /// Tuple of (nonce, expected_response) where:
    /// - `nonce`: 32 random bytes to send to the peer
    /// - `expected_response`: What the peer should respond with
    pub fn create_challenge(
        &self,
    ) -> (
        [u8; FORMATION_CHALLENGE_SIZE],
        [u8; FORMATION_RESPONSE_SIZE],
    ) {
        let mut nonce = [0u8; FORMATION_CHALLENGE_SIZE];
        OsRng.fill_bytes(&mut nonce);

        let expected = self.compute_response(&nonce);
        (nonce, expected)
    }

    /// Respond to a challenge from a peer
    ///
    /// # Arguments
    ///
    /// * `nonce` - The challenge nonce received from the peer
    ///
    /// # Returns
    ///
    /// 32-byte HMAC response
    pub fn respond_to_challenge(&self, nonce: &[u8]) -> [u8; FORMATION_RESPONSE_SIZE] {
        self.compute_response(nonce)
    }

    /// Verify a peer's response to our challenge
    ///
    /// # Arguments
    ///
    /// * `nonce` - The challenge nonce we sent
    /// * `response` - The response from the peer
    ///
    /// # Returns
    ///
    /// `true` if the response is valid (peer has the same formation key)
    pub fn verify_response(&self, nonce: &[u8], response: &[u8; FORMATION_RESPONSE_SIZE]) -> bool {
        let expected = self.compute_response(nonce);

        // Constant-time comparison to prevent timing attacks
        use subtle::ConstantTimeEq;
        expected.ct_eq(response).into()
    }

    /// Compute HMAC response for a nonce
    ///
    /// Response = HMAC-SHA256(key, nonce || formation_id)
    fn compute_response(&self, nonce: &[u8]) -> [u8; FORMATION_RESPONSE_SIZE] {
        let mut mac =
            HmacSha256::new_from_slice(&self.hmac_key).expect("HMAC key should be valid length");

        mac.update(nonce);
        mac.update(self.formation_id.as_bytes());

        let result = mac.finalize();
        let mut response = [0u8; FORMATION_RESPONSE_SIZE];
        response.copy_from_slice(&result.into_bytes());
        response
    }
}

impl std::fmt::Debug for FormationKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormationKey")
            .field("formation_id", &self.formation_id)
            .field("hmac_key", &"[REDACTED]")
            .finish()
    }
}

/// Challenge message sent from responder to initiator
#[derive(Debug, Clone)]
pub struct FormationChallenge {
    /// Formation ID (so initiator knows which key to use)
    pub formation_id: String,
    /// Random nonce
    pub nonce: [u8; FORMATION_CHALLENGE_SIZE],
}

impl FormationChallenge {
    /// Serialize to bytes for wire transmission
    ///
    /// Format: formation_id_len (2 bytes) || formation_id || nonce (32 bytes)
    pub fn to_bytes(&self) -> Vec<u8> {
        let id_bytes = self.formation_id.as_bytes();
        let mut bytes = Vec::with_capacity(2 + id_bytes.len() + FORMATION_CHALLENGE_SIZE);

        bytes.extend_from_slice(&(id_bytes.len() as u16).to_le_bytes());
        bytes.extend_from_slice(id_bytes);
        bytes.extend_from_slice(&self.nonce);

        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SecurityError> {
        if bytes.len() < 2 {
            return Err(SecurityError::AuthenticationFailed(
                "Challenge too short".to_string(),
            ));
        }

        let id_len = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;

        if bytes.len() < 2 + id_len + FORMATION_CHALLENGE_SIZE {
            return Err(SecurityError::AuthenticationFailed(
                "Challenge truncated".to_string(),
            ));
        }

        let formation_id = String::from_utf8(bytes[2..2 + id_len].to_vec()).map_err(|e| {
            SecurityError::AuthenticationFailed(format!("Invalid formation ID: {}", e))
        })?;

        let mut nonce = [0u8; FORMATION_CHALLENGE_SIZE];
        nonce.copy_from_slice(&bytes[2 + id_len..2 + id_len + FORMATION_CHALLENGE_SIZE]);

        Ok(Self {
            formation_id,
            nonce,
        })
    }
}

/// Response message sent from initiator to responder
#[derive(Debug, Clone)]
pub struct FormationChallengeResponse {
    /// HMAC response
    pub response: [u8; FORMATION_RESPONSE_SIZE],
}

impl FormationChallengeResponse {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.response.to_vec()
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SecurityError> {
        if bytes.len() < FORMATION_RESPONSE_SIZE {
            return Err(SecurityError::AuthenticationFailed(
                "Response too short".to_string(),
            ));
        }

        let mut response = [0u8; FORMATION_RESPONSE_SIZE];
        response.copy_from_slice(&bytes[..FORMATION_RESPONSE_SIZE]);

        Ok(Self { response })
    }
}

/// Result of formation authentication handshake
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormationAuthResult {
    /// Authentication successful
    Accepted,
    /// Authentication failed (wrong key or formation)
    Rejected,
}

impl FormationAuthResult {
    /// Serialize to single byte
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Accepted => 0x01,
            Self::Rejected => 0x00,
        }
    }

    /// Deserialize from byte
    pub fn from_byte(byte: u8) -> Self {
        if byte == 0x01 {
            Self::Accepted
        } else {
            Self::Rejected
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formation_key_creation() {
        let secret = [0x42u8; 32];
        let key = FormationKey::new("alpha-company", &secret);

        assert_eq!(key.formation_id(), "alpha-company");
    }

    #[test]
    fn test_formation_key_from_base64() {
        let secret = FormationKey::generate_secret();
        let key = FormationKey::from_base64("test-formation", &secret).unwrap();

        assert_eq!(key.formation_id(), "test-formation");
    }

    #[test]
    fn test_formation_key_from_base64_invalid() {
        let result = FormationKey::from_base64("test", "not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_formation_key_from_base64_derives_key_for_non_32_bytes() {
        use base64::{engine::general_purpose::STANDARD, Engine};

        // Short secret (16 bytes) - should be derived via SHA-256
        let short_secret = STANDARD.encode([0u8; 16]);
        let result = FormationKey::from_base64("test", &short_secret);
        assert!(result.is_ok(), "Short key should be derived via SHA-256");

        // Long secret (138 bytes like EC key) - should be derived via SHA-256
        let long_secret = STANDARD.encode([0xABu8; 138]);
        let result = FormationKey::from_base64("test", &long_secret);
        assert!(
            result.is_ok(),
            "Long key (EC format) should be derived via SHA-256"
        );

        // Verify that different inputs produce different keys
        let key1 = FormationKey::from_base64("test", &short_secret).unwrap();
        let key2 = FormationKey::from_base64("test", &long_secret).unwrap();

        // Create challenges to verify keys are different
        let (nonce, _) = key1.create_challenge();
        let response1 = key1.respond_to_challenge(&nonce);
        assert!(
            !key2.verify_response(&nonce, &response1),
            "Different input keys should produce different derived keys"
        );
    }

    #[test]
    fn test_challenge_response_success() {
        let secret = [0x42u8; 32];
        let key = FormationKey::new("alpha-company", &secret);

        // Responder creates challenge
        let (nonce, _expected) = key.create_challenge();

        // Initiator responds (with same key)
        let response = key.respond_to_challenge(&nonce);

        // Responder verifies
        assert!(key.verify_response(&nonce, &response));
    }

    #[test]
    fn test_challenge_response_wrong_key() {
        let secret1 = [0x42u8; 32];
        let secret2 = [0x43u8; 32]; // Different secret

        let key1 = FormationKey::new("alpha-company", &secret1);
        let key2 = FormationKey::new("alpha-company", &secret2);

        // Responder (key1) creates challenge
        let (nonce, _expected) = key1.create_challenge();

        // Initiator (key2) responds with wrong key
        let response = key2.respond_to_challenge(&nonce);

        // Responder verifies - should fail
        assert!(!key1.verify_response(&nonce, &response));
    }

    #[test]
    fn test_challenge_response_wrong_formation() {
        let secret = [0x42u8; 32];

        let key1 = FormationKey::new("alpha-company", &secret);
        let key2 = FormationKey::new("bravo-company", &secret); // Different formation

        // Responder (key1) creates challenge
        let (nonce, _expected) = key1.create_challenge();

        // Initiator (key2) responds with wrong formation
        let response = key2.respond_to_challenge(&nonce);

        // Responder verifies - should fail (different formation IDs in HMAC)
        assert!(!key1.verify_response(&nonce, &response));
    }

    #[test]
    fn test_different_nonces_produce_different_responses() {
        let secret = [0x42u8; 32];
        let key = FormationKey::new("alpha-company", &secret);

        let (nonce1, _) = key.create_challenge();
        let (nonce2, _) = key.create_challenge();

        let response1 = key.respond_to_challenge(&nonce1);
        let response2 = key.respond_to_challenge(&nonce2);

        // Different nonces should produce different responses
        assert_ne!(response1, response2);
    }

    #[test]
    fn test_challenge_serialization() {
        let mut nonce = [0u8; FORMATION_CHALLENGE_SIZE];
        nonce[0] = 0x42;

        let challenge = FormationChallenge {
            formation_id: "test-formation".to_string(),
            nonce,
        };

        let bytes = challenge.to_bytes();
        let restored = FormationChallenge::from_bytes(&bytes).unwrap();

        assert_eq!(challenge.formation_id, restored.formation_id);
        assert_eq!(challenge.nonce, restored.nonce);
    }

    #[test]
    fn test_response_serialization() {
        let mut response_bytes = [0u8; FORMATION_RESPONSE_SIZE];
        response_bytes[0] = 0x42;

        let response = FormationChallengeResponse {
            response: response_bytes,
        };

        let bytes = response.to_bytes();
        let restored = FormationChallengeResponse::from_bytes(&bytes).unwrap();

        assert_eq!(response.response, restored.response);
    }

    #[test]
    fn test_auth_result_serialization() {
        assert_eq!(
            FormationAuthResult::from_byte(FormationAuthResult::Accepted.to_byte()),
            FormationAuthResult::Accepted
        );
        assert_eq!(
            FormationAuthResult::from_byte(FormationAuthResult::Rejected.to_byte()),
            FormationAuthResult::Rejected
        );
    }

    #[test]
    fn test_generate_secret() {
        let secret1 = FormationKey::generate_secret();
        let secret2 = FormationKey::generate_secret();

        // Should be different (random)
        assert_ne!(secret1, secret2);

        // Should be valid base64 that decodes to 32 bytes
        use base64::{engine::general_purpose::STANDARD, Engine};
        let decoded = STANDARD.decode(&secret1).unwrap();
        assert_eq!(decoded.len(), 32);
    }
}
