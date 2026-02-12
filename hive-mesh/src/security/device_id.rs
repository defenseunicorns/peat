//! Device identifier type derived from Ed25519 public key.

use super::error::SecurityError;
use crate::transport::NodeId;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Device identifier derived from Ed25519 public key.
///
/// The DeviceId is the first 16 bytes of SHA-256(public_key), providing:
/// - Uniqueness guarantee from the cryptographic hash
/// - Compact representation (32 hex chars instead of 64)
/// - Collision resistance from SHA-256
///
/// # Example
///
/// ```ignore
/// use hive_mesh::security::{DeviceKeypair, DeviceId};
///
/// let keypair = DeviceKeypair::generate();
/// let device_id = keypair.device_id();
///
/// // Convert to hex string for display/serialization
/// let hex = device_id.to_hex();
///
/// // Parse back from hex
/// let parsed = DeviceId::from_hex(&hex)?;
/// assert_eq!(device_id, parsed);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId([u8; 16]);

impl DeviceId {
    /// Create a DeviceId from an Ed25519 public key.
    ///
    /// The ID is computed as SHA-256(public_key)[0..16].
    pub fn from_public_key(public_key: &VerifyingKey) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(public_key.as_bytes());
        let hash = hasher.finalize();

        let mut id = [0u8; 16];
        id.copy_from_slice(&hash[..16]);
        DeviceId(id)
    }

    /// Create a DeviceId from raw public key bytes (32 bytes).
    pub fn from_public_key_bytes(bytes: &[u8]) -> Result<Self, SecurityError> {
        if bytes.len() != 32 {
            return Err(SecurityError::InvalidPublicKey(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }

        let verifying_key = VerifyingKey::from_bytes(bytes.try_into().unwrap())
            .map_err(|e| SecurityError::InvalidPublicKey(e.to_string()))?;

        Ok(Self::from_public_key(&verifying_key))
    }

    /// Create a DeviceId from raw bytes (16 bytes).
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        DeviceId(bytes)
    }

    /// Get the raw bytes of this DeviceId.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Convert to a lowercase hex string (32 characters).
    pub fn to_hex(self) -> String {
        hex_encode(&self.0)
    }

    /// Parse from a hex string (32 characters).
    pub fn from_hex(hex: &str) -> Result<Self, SecurityError> {
        let bytes = hex_decode(hex)
            .map_err(|e| SecurityError::InvalidDeviceId(format!("invalid hex: {}", e)))?;

        if bytes.len() != 16 {
            return Err(SecurityError::InvalidDeviceId(format!(
                "expected 16 bytes (32 hex chars), got {} bytes",
                bytes.len()
            )));
        }

        let mut id = [0u8; 16];
        id.copy_from_slice(&bytes);
        Ok(DeviceId(id))
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({})", self.to_hex())
    }
}

impl From<DeviceId> for NodeId {
    fn from(device_id: DeviceId) -> Self {
        NodeId::new(device_id.to_hex())
    }
}

impl TryFrom<&NodeId> for DeviceId {
    type Error = SecurityError;

    fn try_from(node_id: &NodeId) -> Result<Self, Self::Error> {
        DeviceId::from_hex(node_id.as_str())
    }
}

// Simple hex encode/decode without external dependency
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("odd length hex string".to_string());
    }

    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|e| format!("invalid hex at position {}: {}", i, e))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;

    #[test]
    fn test_device_id_from_public_key_deterministic() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let id1 = DeviceId::from_public_key(&verifying_key);
        let id2 = DeviceId::from_public_key(&verifying_key);

        assert_eq!(id1, id2);
    }

    #[test]
    fn test_device_id_different_keys_different_ids() {
        let key1 = SigningKey::generate(&mut OsRng);
        let key2 = SigningKey::generate(&mut OsRng);

        let id1 = DeviceId::from_public_key(&key1.verifying_key());
        let id2 = DeviceId::from_public_key(&key2.verifying_key());

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_device_id_hex_roundtrip() {
        let key = SigningKey::generate(&mut OsRng);
        let id = DeviceId::from_public_key(&key.verifying_key());

        let hex = id.to_hex();
        assert_eq!(hex.len(), 32);

        let parsed = DeviceId::from_hex(&hex).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_device_id_to_node_id() {
        let key = SigningKey::generate(&mut OsRng);
        let device_id = DeviceId::from_public_key(&key.verifying_key());

        let node_id: NodeId = device_id.into();
        assert_eq!(node_id.as_str(), device_id.to_hex());
    }

    #[test]
    fn test_device_id_from_invalid_hex() {
        let result = DeviceId::from_hex("not-valid-hex");
        assert!(result.is_err());

        let result = DeviceId::from_hex("abc"); // odd length
        assert!(result.is_err());

        let result = DeviceId::from_hex("00112233"); // too short
        assert!(result.is_err());
    }

    #[test]
    fn test_from_public_key_bytes() {
        let key = SigningKey::generate(&mut OsRng);
        let pk_bytes = key.verifying_key().to_bytes();

        let id = DeviceId::from_public_key_bytes(&pk_bytes).unwrap();
        let expected = DeviceId::from_public_key(&key.verifying_key());
        assert_eq!(id, expected);
    }

    #[test]
    fn test_from_public_key_bytes_wrong_length() {
        let result = DeviceId::from_public_key_bytes(&[0u8; 16]);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_bytes_and_as_bytes() {
        let raw = [1u8; 16];
        let id = DeviceId::from_bytes(raw);
        assert_eq!(id.as_bytes(), &raw);
    }

    #[test]
    fn test_display() {
        let id = DeviceId::from_bytes([0xab; 16]);
        let s = format!("{}", id);
        assert_eq!(s, "abababababababababababababababab");
    }

    #[test]
    fn test_debug() {
        let id = DeviceId::from_bytes([0xcd; 16]);
        let s = format!("{:?}", id);
        assert!(s.starts_with("DeviceId("));
        assert!(s.contains("cdcdcdcd"));
    }

    #[test]
    fn test_try_from_node_id() {
        let key = SigningKey::generate(&mut OsRng);
        let device_id = DeviceId::from_public_key(&key.verifying_key());
        let node_id: NodeId = device_id.into();

        let back: DeviceId = (&node_id).try_into().unwrap();
        assert_eq!(back, device_id);
    }

    #[test]
    fn test_try_from_invalid_node_id() {
        let node_id = NodeId::new("not-hex".into());
        let result: Result<DeviceId, _> = (&node_id).try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_decode_invalid_chars() {
        assert!(hex_decode("zz").is_err());
        assert!(hex_decode("gg").is_err());
    }

    #[test]
    fn test_hex_roundtrip() {
        let bytes = vec![0x00, 0xff, 0x0a, 0xb5];
        let encoded = hex_encode(&bytes);
        assert_eq!(encoded, "00ff0ab5");
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, bytes);
    }
}
