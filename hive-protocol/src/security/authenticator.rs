//! Device authenticator for challenge-response authentication.

use super::device_id::DeviceId;
use super::error::SecurityError;
use super::keypair::DeviceKeypair;
use super::{CHALLENGE_NONCE_SIZE, DEFAULT_CHALLENGE_TIMEOUT_SECS};
use hive_schema::security::v1::{Challenge, SignedChallengeResponse};
use rand_core::{OsRng, RngCore};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Device authenticator manages challenge-response authentication.
///
/// # Overview
///
/// The authenticator uses Ed25519 signatures for mutual authentication:
/// 1. Generate a challenge with random nonce and timestamp
/// 2. Peer signs the challenge and returns their public key
/// 3. Verify signature and cache the verified peer identity
///
/// # Example
///
/// ```ignore
/// use hive_protocol::security::{DeviceKeypair, DeviceAuthenticator};
///
/// let keypair = DeviceKeypair::generate();
/// let authenticator = DeviceAuthenticator::new(keypair);
///
/// // Generate challenge for peer
/// let challenge = authenticator.generate_challenge();
///
/// // Peer creates response
/// let response = peer_authenticator.respond_to_challenge(&challenge)?;
///
/// // Verify response
/// let peer_id = authenticator.verify_response(&response)?;
/// println!("Authenticated peer: {}", peer_id);
/// ```
pub struct DeviceAuthenticator {
    /// This device's keypair
    keypair: DeviceKeypair,

    /// Verified peers cache
    verified_peers: RwLock<HashMap<DeviceId, VerifiedPeer>>,

    /// Challenge timeout duration
    challenge_timeout: Duration,
}

/// A verified peer's identity
#[derive(Debug, Clone)]
pub struct VerifiedPeer {
    /// The peer's device ID
    pub device_id: DeviceId,

    /// The peer's public key bytes
    pub public_key: [u8; 32],

    /// When this peer was verified
    pub verified_at: SystemTime,
}

impl DeviceAuthenticator {
    /// Create a new authenticator with the given keypair.
    pub fn new(keypair: DeviceKeypair) -> Self {
        Self::with_timeout(keypair, Duration::from_secs(DEFAULT_CHALLENGE_TIMEOUT_SECS))
    }

    /// Create an authenticator with a custom challenge timeout.
    pub fn with_timeout(keypair: DeviceKeypair, challenge_timeout: Duration) -> Self {
        DeviceAuthenticator {
            keypair,
            verified_peers: RwLock::new(HashMap::new()),
            challenge_timeout,
        }
    }

    /// Get this device's ID.
    pub fn device_id(&self) -> DeviceId {
        self.keypair.device_id()
    }

    /// Get this device's public key bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.keypair.public_key_bytes()
    }

    /// Generate a challenge for authenticating a peer.
    ///
    /// The challenge contains:
    /// - Random 32-byte nonce
    /// - Current timestamp
    /// - This device's ID
    /// - Expiration timestamp
    pub fn generate_challenge(&self) -> Challenge {
        let mut nonce = [0u8; CHALLENGE_NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        let expires = now + self.challenge_timeout;

        Challenge {
            nonce: nonce.to_vec(),
            timestamp: Some(hive_schema::common::v1::Timestamp {
                seconds: now.as_secs(),
                nanos: now.subsec_nanos(),
            }),
            challenger_id: self.device_id().to_hex(),
            expires_at: Some(hive_schema::common::v1::Timestamp {
                seconds: expires.as_secs(),
                nanos: expires.subsec_nanos(),
            }),
        }
    }

    /// Create a signed response to a challenge.
    ///
    /// Signs the challenge data with this device's private key.
    pub fn respond_to_challenge(
        &self,
        challenge: &Challenge,
    ) -> Result<SignedChallengeResponse, SecurityError> {
        // Check challenge hasn't expired
        self.check_challenge_expiry(challenge)?;

        // Create message to sign: nonce || challenger_id || timestamp
        let message = self.create_challenge_message(challenge);

        // Sign the message
        let signature = self.keypair.sign(&message);

        Ok(SignedChallengeResponse {
            challenge_nonce: challenge.nonce.clone(),
            public_key: self.keypair.public_key_bytes().to_vec(),
            signature: signature.to_bytes().to_vec(),
            timestamp: Some(hive_schema::common::v1::Timestamp {
                seconds: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                nanos: 0,
            }),
            device_type: 0,       // DEVICE_TYPE_UNSPECIFIED for MVP
            certificates: vec![], // Empty for MVP (no X.509 chain)
        })
    }

    /// Verify a peer's challenge response.
    ///
    /// On success, caches the peer's identity and returns their DeviceId.
    pub fn verify_response(
        &self,
        response: &SignedChallengeResponse,
    ) -> Result<DeviceId, SecurityError> {
        // Parse public key
        let public_key = DeviceKeypair::verifying_key_from_bytes(&response.public_key)?;

        // Derive device ID from public key
        let peer_device_id = DeviceId::from_public_key(&public_key);

        // Recreate the message that should have been signed
        // Note: In a full implementation, we would look up the original challenge
        // For MVP, we verify the signature is valid for the provided nonce
        let mut message = response.challenge_nonce.clone();
        message.extend_from_slice(self.device_id().to_hex().as_bytes());
        // Append timestamp if available
        if let Some(ts) = &response.timestamp {
            message.extend_from_slice(&ts.seconds.to_le_bytes());
        }

        // Parse and verify signature
        let signature = DeviceKeypair::signature_from_bytes(&response.signature)?;
        DeviceKeypair::verify_with_key(&public_key, &message, &signature)?;

        // Cache the verified peer
        let verified_peer = VerifiedPeer {
            device_id: peer_device_id,
            public_key: public_key.to_bytes(),
            verified_at: SystemTime::now(),
        };

        self.verified_peers
            .write()
            .map_err(|e| SecurityError::Internal(format!("lock poisoned: {}", e)))?
            .insert(peer_device_id, verified_peer);

        Ok(peer_device_id)
    }

    /// Check if a peer is verified.
    pub fn is_verified(&self, device_id: &DeviceId) -> bool {
        self.verified_peers
            .read()
            .map(|cache| cache.contains_key(device_id))
            .unwrap_or(false)
    }

    /// Get a verified peer's info.
    pub fn get_verified_peer(&self, device_id: &DeviceId) -> Option<VerifiedPeer> {
        self.verified_peers
            .read()
            .ok()
            .and_then(|cache| cache.get(device_id).cloned())
    }

    /// Remove a peer from the verified cache.
    pub fn remove_peer(&self, device_id: &DeviceId) {
        if let Ok(mut cache) = self.verified_peers.write() {
            cache.remove(device_id);
        }
    }

    /// Clear all verified peers.
    pub fn clear_verified_peers(&self) {
        if let Ok(mut cache) = self.verified_peers.write() {
            cache.clear();
        }
    }

    /// Get number of verified peers.
    pub fn verified_peer_count(&self) -> usize {
        self.verified_peers
            .read()
            .map(|cache| cache.len())
            .unwrap_or(0)
    }

    /// Create the message bytes that should be signed for a challenge.
    fn create_challenge_message(&self, challenge: &Challenge) -> Vec<u8> {
        let mut message = challenge.nonce.clone();
        message.extend_from_slice(challenge.challenger_id.as_bytes());
        if let Some(ts) = &challenge.timestamp {
            message.extend_from_slice(&ts.seconds.to_le_bytes());
        }
        message
    }

    /// Check if a challenge has expired.
    fn check_challenge_expiry(&self, challenge: &Challenge) -> Result<(), SecurityError> {
        if let Some(expires) = &challenge.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();

            if now.as_secs() > expires.seconds {
                return Err(SecurityError::ChallengeExpired(expires.seconds));
            }
        }
        Ok(())
    }
}

impl std::fmt::Debug for DeviceAuthenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceAuthenticator")
            .field("device_id", &self.device_id())
            .field("verified_peer_count", &self.verified_peer_count())
            .field("challenge_timeout", &self.challenge_timeout)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_authenticator() -> DeviceAuthenticator {
        let keypair = DeviceKeypair::generate();
        DeviceAuthenticator::new(keypair)
    }

    #[test]
    fn test_generate_challenge() {
        let auth = create_test_authenticator();
        let challenge = auth.generate_challenge();

        assert_eq!(challenge.nonce.len(), CHALLENGE_NONCE_SIZE);
        assert!(!challenge.challenger_id.is_empty());
        assert!(challenge.timestamp.is_some());
        assert!(challenge.expires_at.is_some());
    }

    #[test]
    fn test_challenge_nonce_unique() {
        let auth = create_test_authenticator();
        let c1 = auth.generate_challenge();
        let c2 = auth.generate_challenge();

        assert_ne!(c1.nonce, c2.nonce);
    }

    #[test]
    fn test_respond_to_challenge() {
        let auth1 = create_test_authenticator();
        let auth2 = create_test_authenticator();

        let challenge = auth1.generate_challenge();
        let response = auth2.respond_to_challenge(&challenge).unwrap();

        assert_eq!(response.public_key.len(), 32);
        assert_eq!(response.signature.len(), 64);
        assert_eq!(response.challenge_nonce, challenge.nonce);
    }

    #[test]
    fn test_full_authentication_flow() {
        let auth1 = create_test_authenticator();
        let auth2 = create_test_authenticator();

        // Auth1 generates challenge for Auth2
        let challenge = auth1.generate_challenge();

        // Auth2 responds
        let response = auth2.respond_to_challenge(&challenge).unwrap();

        // Auth1 verifies
        let peer_id = auth1.verify_response(&response).unwrap();

        // Peer ID should match Auth2's device ID
        assert_eq!(peer_id, auth2.device_id());

        // Peer should now be in verified cache
        assert!(auth1.is_verified(&peer_id));
    }

    #[test]
    fn test_expired_challenge_rejected() {
        let auth = create_test_authenticator();

        // Create a challenge with expiration in the past
        let mut challenge = auth.generate_challenge();
        challenge.expires_at = Some(hive_schema::common::v1::Timestamp {
            seconds: 0, // Way in the past
            nanos: 0,
        });

        let result = auth.respond_to_challenge(&challenge);
        assert!(matches!(result, Err(SecurityError::ChallengeExpired(_))));
    }

    #[test]
    fn test_invalid_signature_rejected() {
        let auth1 = create_test_authenticator();
        let auth2 = create_test_authenticator();

        let challenge = auth1.generate_challenge();
        let mut response = auth2.respond_to_challenge(&challenge).unwrap();

        // Corrupt the signature
        response.signature[0] ^= 0xFF;

        let result = auth1.verify_response(&response);
        assert!(matches!(result, Err(SecurityError::InvalidSignature(_))));
    }

    #[test]
    fn test_remove_peer() {
        let auth1 = create_test_authenticator();
        let auth2 = create_test_authenticator();

        // Authenticate
        let challenge = auth1.generate_challenge();
        let response = auth2.respond_to_challenge(&challenge).unwrap();
        let peer_id = auth1.verify_response(&response).unwrap();

        assert!(auth1.is_verified(&peer_id));

        // Remove
        auth1.remove_peer(&peer_id);
        assert!(!auth1.is_verified(&peer_id));
    }

    #[test]
    fn test_clear_verified_peers() {
        let auth1 = create_test_authenticator();
        let auth2 = create_test_authenticator();
        let auth3 = create_test_authenticator();

        // Authenticate two peers
        let c1 = auth1.generate_challenge();
        let r1 = auth2.respond_to_challenge(&c1).unwrap();
        auth1.verify_response(&r1).unwrap();

        let c2 = auth1.generate_challenge();
        let r2 = auth3.respond_to_challenge(&c2).unwrap();
        auth1.verify_response(&r2).unwrap();

        assert_eq!(auth1.verified_peer_count(), 2);

        auth1.clear_verified_peers();
        assert_eq!(auth1.verified_peer_count(), 0);
    }
}
