//! Membership certificates for tactical mesh networks (ADR-048).
//!
//! Provides authority-issued certificates binding device identity to callsign,
//! with time-limited validity and permission-based access control.
//!
//! # Trust Model
//!
//! ```text
//! MeshGenesis (root of trust)
//!     │
//!     └── creator_public_key (authority)
//!             │
//!             └── signs ──► MembershipCertificate
//!                               ├── member_public_key ──► derives node_id
//!                               ├── callsign (authority-assigned)
//!                               ├── expires_at_ms
//!                               └── permissions
//! ```
//!
//! # Example
//!
//! ```ignore
//! use hive_protocol::security::{DeviceKeypair, MembershipCertificate, MemberPermissions};
//!
//! // Authority creates certificate for new member
//! let authority = DeviceKeypair::generate();
//! let member = DeviceKeypair::generate();
//!
//! let cert = MembershipCertificate::new(
//!     member.public_key_bytes(),
//!     "ALPHA-01".to_string(),
//!     "A1B2C3D4".to_string(),
//!     now_ms,
//!     now_ms + 24 * 60 * 60 * 1000,  // 24 hours
//!     MemberPermissions::RELAY | MemberPermissions::EMERGENCY,
//!     authority.public_key_bytes(),
//! );
//!
//! // Sign with authority's key
//! let signed_cert = cert.sign(&authority);
//!
//! // Verify certificate
//! assert!(signed_cert.verify().is_ok());
//! ```

use super::error::SecurityError;
use super::keypair::DeviceKeypair;
use bitflags::bitflags;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::collections::HashMap;

/// Size of membership certificate wire format (without variable callsign)
/// Base: 32 (pubkey) + 1 (callsign_len) + 8 (mesh_id) + 8 (issued) + 8 (expires) + 1 (perms) + 32 (issuer) + 64 (sig) = 154
/// Plus callsign bytes (max 16)
pub const CERTIFICATE_BASE_SIZE: usize = 154;

/// Maximum callsign length in bytes
pub const MAX_CALLSIGN_LEN: usize = 16;

/// Mesh ID length (8 hex characters = 4 bytes, but stored as 8-char string)
pub const MESH_ID_LEN: usize = 8;

bitflags! {
    /// Permission flags for mesh members.
    ///
    /// These flags control what operations a member can perform:
    /// - `RELAY`: Can relay messages for other nodes
    /// - `EMERGENCY`: Can trigger emergency alerts
    /// - `ENROLL`: Can enroll new members (delegation)
    /// - `ADMIN`: Full administrative privileges
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct MemberPermissions: u8 {
        /// Can relay messages for other nodes
        const RELAY      = 0b0000_0001;
        /// Can trigger emergency alerts
        const EMERGENCY  = 0b0000_0010;
        /// Can enroll new members (delegation of authority)
        const ENROLL     = 0b0000_0100;
        /// Full administrative privileges
        const ADMIN      = 0b1000_0000;
    }
}

impl Default for MemberPermissions {
    fn default() -> Self {
        // Default: can relay and send emergencies, but not enroll or admin
        Self::RELAY | Self::EMERGENCY
    }
}

impl MemberPermissions {
    /// Standard member permissions (relay + emergency)
    pub const STANDARD: Self = Self::RELAY.union(Self::EMERGENCY);

    /// Authority permissions (all flags)
    pub const AUTHORITY: Self = Self::all();
}

/// A membership certificate binding device identity to callsign.
///
/// Issued by mesh authority, contains:
/// - Member's Ed25519 public key
/// - Authority-assigned callsign
/// - Mesh identifier
/// - Validity period (issued_at to expires_at)
/// - Permission flags
/// - Issuer's public key and signature
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipCertificate {
    /// Member's Ed25519 public key (32 bytes)
    pub member_public_key: [u8; 32],

    /// Authority-assigned callsign (max 16 UTF-8 bytes)
    /// Examples: "ALPHA-01", "BRAVO-42", "ZULU-99"
    pub callsign: String,

    /// Mesh identifier (8 hex characters)
    pub mesh_id: String,

    /// Timestamp when certificate was issued (ms since Unix epoch)
    pub issued_at_ms: u64,

    /// Timestamp when certificate expires (ms since Unix epoch)
    /// 0 = no expiration (not recommended for production)
    pub expires_at_ms: u64,

    /// Permission flags
    pub permissions: MemberPermissions,

    /// Issuer's Ed25519 public key (32 bytes)
    /// For root certificates, this equals member_public_key (self-signed)
    pub issuer_public_key: [u8; 32],

    /// Ed25519 signature over all above fields (64 bytes)
    /// Empty until signed
    pub issuer_signature: [u8; 64],
}

impl MembershipCertificate {
    /// Create a new unsigned certificate.
    ///
    /// Call `sign()` with the issuer's keypair to complete the certificate.
    pub fn new(
        member_public_key: [u8; 32],
        callsign: String,
        mesh_id: String,
        issued_at_ms: u64,
        expires_at_ms: u64,
        permissions: MemberPermissions,
        issuer_public_key: [u8; 32],
    ) -> Self {
        Self {
            member_public_key,
            callsign,
            mesh_id,
            issued_at_ms,
            expires_at_ms,
            permissions,
            issuer_public_key,
            issuer_signature: [0u8; 64],
        }
    }

    /// Create a self-signed root certificate (for mesh authority).
    pub fn new_root(
        authority_keypair: &DeviceKeypair,
        callsign: String,
        mesh_id: String,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Self {
        let public_key = authority_keypair.public_key_bytes();
        let mut cert = Self::new(
            public_key,
            callsign,
            mesh_id,
            issued_at_ms,
            expires_at_ms,
            MemberPermissions::AUTHORITY,
            public_key, // Self-signed
        );
        cert.sign_with(authority_keypair);
        cert
    }

    /// Get the bytes that are signed (everything except the signature).
    fn signable_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(CERTIFICATE_BASE_SIZE + self.callsign.len());

        // member_public_key (32)
        buf.extend_from_slice(&self.member_public_key);

        // callsign_len (1) + callsign (variable)
        buf.push(self.callsign.len() as u8);
        buf.extend_from_slice(self.callsign.as_bytes());

        // mesh_id (8 bytes as UTF-8)
        buf.extend_from_slice(self.mesh_id.as_bytes());

        // issued_at_ms (8)
        buf.extend_from_slice(&self.issued_at_ms.to_le_bytes());

        // expires_at_ms (8)
        buf.extend_from_slice(&self.expires_at_ms.to_le_bytes());

        // permissions (1)
        buf.push(self.permissions.bits());

        // issuer_public_key (32)
        buf.extend_from_slice(&self.issuer_public_key);

        buf
    }

    /// Sign this certificate with the issuer's keypair.
    ///
    /// Modifies the certificate in place.
    pub fn sign_with(&mut self, issuer_keypair: &DeviceKeypair) {
        let signable = self.signable_bytes();
        let signature = issuer_keypair.sign(&signable);
        self.issuer_signature = signature.to_bytes();
    }

    /// Create a signed copy of this certificate.
    pub fn signed(mut self, issuer_keypair: &DeviceKeypair) -> Self {
        self.sign_with(issuer_keypair);
        self
    }

    /// Verify the certificate signature against the issuer's public key.
    pub fn verify(&self) -> Result<(), SecurityError> {
        let signable = self.signable_bytes();

        let verifying_key = VerifyingKey::from_bytes(&self.issuer_public_key)
            .map_err(|e| SecurityError::InvalidPublicKey(e.to_string()))?;

        let signature = Signature::from_bytes(&self.issuer_signature);

        verifying_key
            .verify(&signable, &signature)
            .map_err(|e| SecurityError::InvalidSignature(e.to_string()))
    }

    /// Check if the certificate is currently valid (not expired).
    pub fn is_valid(&self, now_ms: u64) -> bool {
        if self.expires_at_ms == 0 {
            // No expiration set
            return true;
        }
        now_ms >= self.issued_at_ms && now_ms < self.expires_at_ms
    }

    /// Check if the certificate is within the grace period.
    ///
    /// Grace period allows continued operation for a short time after expiration.
    pub fn is_in_grace_period(&self, now_ms: u64, grace_period_ms: u64) -> bool {
        if self.expires_at_ms == 0 {
            return false; // No expiration = no grace period
        }
        now_ms >= self.expires_at_ms && now_ms < self.expires_at_ms + grace_period_ms
    }

    /// Check if the certificate has expired beyond the grace period.
    pub fn is_expired(&self, now_ms: u64, grace_period_ms: u64) -> bool {
        if self.expires_at_ms == 0 {
            return false; // No expiration
        }
        now_ms >= self.expires_at_ms + grace_period_ms
    }

    /// Get time remaining until expiration (0 if expired).
    pub fn time_remaining_ms(&self, now_ms: u64) -> u64 {
        if self.expires_at_ms == 0 || now_ms >= self.expires_at_ms {
            0
        } else {
            self.expires_at_ms - now_ms
        }
    }

    /// Check if the member has a specific permission.
    pub fn has_permission(&self, permission: MemberPermissions) -> bool {
        self.permissions.contains(permission)
    }

    /// Check if this is a self-signed root certificate.
    pub fn is_root(&self) -> bool {
        self.member_public_key == self.issuer_public_key
    }

    /// Encode certificate to wire format.
    ///
    /// Format:
    /// ```text
    /// [member_pubkey:32][callsign_len:1][callsign:N][mesh_id:8]
    /// [issued_at:8][expires_at:8][permissions:1][issuer_pubkey:32][signature:64]
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(CERTIFICATE_BASE_SIZE + self.callsign.len());

        buf.extend_from_slice(&self.member_public_key);
        buf.push(self.callsign.len() as u8);
        buf.extend_from_slice(self.callsign.as_bytes());
        buf.extend_from_slice(self.mesh_id.as_bytes());
        buf.extend_from_slice(&self.issued_at_ms.to_le_bytes());
        buf.extend_from_slice(&self.expires_at_ms.to_le_bytes());
        buf.push(self.permissions.bits());
        buf.extend_from_slice(&self.issuer_public_key);
        buf.extend_from_slice(&self.issuer_signature);

        buf
    }

    /// Decode certificate from wire format.
    pub fn decode(data: &[u8]) -> Result<Self, SecurityError> {
        // Minimum size: base size with empty callsign
        if data.len() < CERTIFICATE_BASE_SIZE {
            return Err(SecurityError::SerializationError(format!(
                "certificate too short: {} bytes, need at least {}",
                data.len(),
                CERTIFICATE_BASE_SIZE
            )));
        }

        let mut offset = 0;

        // member_public_key (32)
        let mut member_public_key = [0u8; 32];
        member_public_key.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        // callsign_len (1) + callsign (variable)
        let callsign_len = data[offset] as usize;
        offset += 1;

        if callsign_len > MAX_CALLSIGN_LEN {
            return Err(SecurityError::SerializationError(format!(
                "callsign too long: {} bytes, max {}",
                callsign_len, MAX_CALLSIGN_LEN
            )));
        }

        if offset + callsign_len > data.len() {
            return Err(SecurityError::SerializationError(
                "truncated callsign".to_string(),
            ));
        }

        let callsign =
            String::from_utf8(data[offset..offset + callsign_len].to_vec()).map_err(|e| {
                SecurityError::SerializationError(format!("invalid callsign UTF-8: {}", e))
            })?;
        offset += callsign_len;

        // mesh_id (8)
        if offset + MESH_ID_LEN > data.len() {
            return Err(SecurityError::SerializationError(
                "truncated mesh_id".to_string(),
            ));
        }
        let mesh_id =
            String::from_utf8(data[offset..offset + MESH_ID_LEN].to_vec()).map_err(|e| {
                SecurityError::SerializationError(format!("invalid mesh_id UTF-8: {}", e))
            })?;
        offset += MESH_ID_LEN;

        // issued_at_ms (8)
        if offset + 8 > data.len() {
            return Err(SecurityError::SerializationError(
                "truncated issued_at".to_string(),
            ));
        }
        let issued_at_ms = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // expires_at_ms (8)
        if offset + 8 > data.len() {
            return Err(SecurityError::SerializationError(
                "truncated expires_at".to_string(),
            ));
        }
        let expires_at_ms = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // permissions (1)
        if offset + 1 > data.len() {
            return Err(SecurityError::SerializationError(
                "truncated permissions".to_string(),
            ));
        }
        let permissions = MemberPermissions::from_bits_truncate(data[offset]);
        offset += 1;

        // issuer_public_key (32)
        if offset + 32 > data.len() {
            return Err(SecurityError::SerializationError(
                "truncated issuer_public_key".to_string(),
            ));
        }
        let mut issuer_public_key = [0u8; 32];
        issuer_public_key.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        // issuer_signature (64)
        if offset + 64 > data.len() {
            return Err(SecurityError::SerializationError(
                "truncated signature".to_string(),
            ));
        }
        let mut issuer_signature = [0u8; 64];
        issuer_signature.copy_from_slice(&data[offset..offset + 64]);

        Ok(Self {
            member_public_key,
            callsign,
            mesh_id,
            issued_at_ms,
            expires_at_ms,
            permissions,
            issuer_public_key,
            issuer_signature,
        })
    }

    /// Convert to a lightweight MembershipToken for constrained devices.
    ///
    /// This creates a new token with the authority's signature. The token has:
    /// - Callsign truncated to 12 characters (if longer)
    /// - mesh_id converted from 8-char hex to 4-byte binary
    /// - No permission field (tokens don't carry permissions)
    ///
    /// # Arguments
    /// * `authority_keypair` - The authority's keypair to sign the token
    ///
    /// # Example
    /// ```ignore
    /// let token = certificate.to_token(&authority_keypair);
    /// // Send token over BLE to WearTAK
    /// ```
    #[cfg(feature = "bluetooth")]
    pub fn to_token(
        &self,
        authority_keypair: &DeviceKeypair,
    ) -> hive_btle::security::MembershipToken {
        use hive_btle::security::MembershipToken;

        // Convert 8-char hex mesh_id to 4 bytes
        let mesh_id_bytes = Self::hex_to_bytes(&self.mesh_id);

        // Truncate callsign to 12 chars if needed
        let callsign = if self.callsign.len() > hive_btle::security::MAX_CALLSIGN_LEN {
            &self.callsign[..hive_btle::security::MAX_CALLSIGN_LEN]
        } else {
            &self.callsign
        };

        // Create a DeviceIdentity from the authority keypair for signing
        let authority_identity = hive_btle::security::DeviceIdentity::from_private_key(
            &authority_keypair.secret_key_bytes(),
        )
        .expect("valid keypair");

        MembershipToken::issue_at(
            &authority_identity,
            mesh_id_bytes,
            self.member_public_key,
            callsign,
            self.issued_at_ms,
            self.expires_at_ms,
        )
    }

    /// Create a MembershipCertificate from a MembershipToken.
    ///
    /// This upgrades a lightweight token to a full certificate with:
    /// - mesh_id expanded from 4-byte binary to 8-char hex
    /// - Default permissions (STANDARD: RELAY | EMERGENCY)
    /// - The authority's signature (re-signed for certificate format)
    ///
    /// # Arguments
    /// * `token` - The token to convert
    /// * `authority_keypair` - The authority's keypair to sign the certificate
    ///
    /// # Example
    /// ```ignore
    /// let cert = MembershipCertificate::from_token(&token, &authority_keypair);
    /// ```
    #[cfg(feature = "bluetooth")]
    pub fn from_token(
        token: &hive_btle::security::MembershipToken,
        authority_keypair: &DeviceKeypair,
    ) -> Self {
        // Convert 4-byte mesh_id to 8-char hex
        let mesh_id = format!(
            "{:02X}{:02X}{:02X}{:02X}",
            token.mesh_id[0], token.mesh_id[1], token.mesh_id[2], token.mesh_id[3]
        );

        let callsign = token.callsign_str().to_string();

        let mut cert = Self::new(
            token.public_key,
            callsign,
            mesh_id,
            token.issued_at_ms,
            token.expires_at_ms,
            MemberPermissions::STANDARD, // Default permissions
            authority_keypair.public_key_bytes(),
        );

        cert.sign_with(authority_keypair);
        cert
    }

    /// Helper to convert 8-char hex string to 4 bytes.
    #[cfg(feature = "bluetooth")]
    fn hex_to_bytes(hex: &str) -> [u8; 4] {
        let mut bytes = [0u8; 4];
        if hex.len() == 8 {
            for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
                if i < 4 {
                    let s = std::str::from_utf8(chunk).unwrap_or("00");
                    bytes[i] = u8::from_str_radix(s, 16).unwrap_or(0);
                }
            }
        }
        bytes
    }
}

/// Registry for storing and looking up membership certificates.
///
/// Provides O(1) lookup by member public key or callsign.
#[derive(Debug, Default)]
pub struct CertificateRegistry {
    /// Certificates indexed by member public key
    by_public_key: HashMap<[u8; 32], MembershipCertificate>,

    /// Public key lookup by callsign
    callsign_to_pubkey: HashMap<String, [u8; 32]>,
}

impl CertificateRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a certificate.
    ///
    /// Returns the previous certificate if one existed for this public key.
    pub fn register(&mut self, cert: MembershipCertificate) -> Option<MembershipCertificate> {
        let pubkey = cert.member_public_key;
        let callsign = cert.callsign.clone();

        // Remove old callsign mapping if exists
        if let Some(old_cert) = self.by_public_key.get(&pubkey) {
            self.callsign_to_pubkey.remove(&old_cert.callsign);
        }

        // Add new mappings
        self.callsign_to_pubkey.insert(callsign, pubkey);
        self.by_public_key.insert(pubkey, cert)
    }

    /// Get a certificate by member public key.
    pub fn get_by_pubkey(&self, pubkey: &[u8; 32]) -> Option<&MembershipCertificate> {
        self.by_public_key.get(pubkey)
    }

    /// Get a certificate by callsign.
    pub fn get_by_callsign(&self, callsign: &str) -> Option<&MembershipCertificate> {
        self.callsign_to_pubkey
            .get(callsign)
            .and_then(|pk| self.by_public_key.get(pk))
    }

    /// Remove a certificate by public key.
    pub fn remove(&mut self, pubkey: &[u8; 32]) -> Option<MembershipCertificate> {
        if let Some(cert) = self.by_public_key.remove(pubkey) {
            self.callsign_to_pubkey.remove(&cert.callsign);
            Some(cert)
        } else {
            None
        }
    }

    /// Check if a callsign is already in use.
    pub fn is_callsign_taken(&self, callsign: &str) -> bool {
        self.callsign_to_pubkey.contains_key(callsign)
    }

    /// Get all registered certificates.
    pub fn certificates(&self) -> impl Iterator<Item = &MembershipCertificate> {
        self.by_public_key.values()
    }

    /// Get the number of registered certificates.
    pub fn len(&self) -> usize {
        self.by_public_key.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.by_public_key.is_empty()
    }

    /// Remove expired certificates.
    ///
    /// Returns the number of certificates removed.
    pub fn remove_expired(&mut self, now_ms: u64, grace_period_ms: u64) -> usize {
        let expired: Vec<[u8; 32]> = self
            .by_public_key
            .iter()
            .filter(|(_, cert)| cert.is_expired(now_ms, grace_period_ms))
            .map(|(pk, _)| *pk)
            .collect();

        let count = expired.len();
        for pk in expired {
            self.remove(&pk);
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    #[test]
    fn test_create_and_sign_certificate() {
        let authority = DeviceKeypair::generate();
        let member = DeviceKeypair::generate();

        let now = now_ms();
        let expires = now + 24 * 60 * 60 * 1000; // 24 hours

        let cert = MembershipCertificate::new(
            member.public_key_bytes(),
            "ALPHA-01".to_string(),
            "A1B2C3D4".to_string(),
            now,
            expires,
            MemberPermissions::STANDARD,
            authority.public_key_bytes(),
        )
        .signed(&authority);

        assert!(cert.verify().is_ok());
        assert!(cert.is_valid(now));
        assert!(!cert.is_root());
    }

    #[test]
    fn test_root_certificate() {
        let authority = DeviceKeypair::generate();
        let now = now_ms();
        let expires = now + 24 * 60 * 60 * 1000;

        let cert = MembershipCertificate::new_root(
            &authority,
            "COMMAND".to_string(),
            "A1B2C3D4".to_string(),
            now,
            expires,
        );

        assert!(cert.verify().is_ok());
        assert!(cert.is_root());
        assert!(cert.has_permission(MemberPermissions::ADMIN));
        assert!(cert.has_permission(MemberPermissions::ENROLL));
    }

    #[test]
    fn test_certificate_encode_decode() {
        let authority = DeviceKeypair::generate();
        let member = DeviceKeypair::generate();

        let now = now_ms();
        let expires = now + 24 * 60 * 60 * 1000;

        let cert = MembershipCertificate::new(
            member.public_key_bytes(),
            "BRAVO-42".to_string(),
            "DEADBEEF".to_string(),
            now,
            expires,
            MemberPermissions::RELAY | MemberPermissions::EMERGENCY,
            authority.public_key_bytes(),
        )
        .signed(&authority);

        let encoded = cert.encode();
        let decoded = MembershipCertificate::decode(&encoded).unwrap();

        assert_eq!(decoded.member_public_key, cert.member_public_key);
        assert_eq!(decoded.callsign, cert.callsign);
        assert_eq!(decoded.mesh_id, cert.mesh_id);
        assert_eq!(decoded.issued_at_ms, cert.issued_at_ms);
        assert_eq!(decoded.expires_at_ms, cert.expires_at_ms);
        assert_eq!(decoded.permissions, cert.permissions);
        assert_eq!(decoded.issuer_public_key, cert.issuer_public_key);
        assert_eq!(decoded.issuer_signature, cert.issuer_signature);

        // Decoded certificate should also verify
        assert!(decoded.verify().is_ok());
    }

    #[test]
    fn test_certificate_expiration() {
        let authority = DeviceKeypair::generate();
        let member = DeviceKeypair::generate();

        let now = 1000000u64;
        let expires = now + 1000; // Expires in 1 second
        let grace = 500; // 0.5 second grace

        let cert = MembershipCertificate::new(
            member.public_key_bytes(),
            "TEST-01".to_string(),
            "12345678".to_string(),
            now,
            expires,
            MemberPermissions::STANDARD,
            authority.public_key_bytes(),
        )
        .signed(&authority);

        // Before expiration
        assert!(cert.is_valid(now + 500));
        assert!(!cert.is_in_grace_period(now + 500, grace));
        assert!(!cert.is_expired(now + 500, grace));
        assert_eq!(cert.time_remaining_ms(now + 500), 500);

        // At expiration (in grace period)
        assert!(!cert.is_valid(expires));
        assert!(cert.is_in_grace_period(expires, grace));
        assert!(!cert.is_expired(expires, grace));

        // After grace period
        assert!(!cert.is_valid(expires + grace));
        assert!(!cert.is_in_grace_period(expires + grace, grace));
        assert!(cert.is_expired(expires + grace, grace));
    }

    #[test]
    fn test_invalid_signature() {
        let authority = DeviceKeypair::generate();
        let attacker = DeviceKeypair::generate();
        let member = DeviceKeypair::generate();

        let now = now_ms();

        // Certificate claims to be from authority but signed by attacker
        let mut cert = MembershipCertificate::new(
            member.public_key_bytes(),
            "FAKE-01".to_string(),
            "A1B2C3D4".to_string(),
            now,
            now + 1000,
            MemberPermissions::ADMIN,
            authority.public_key_bytes(), // Claims authority
        );
        cert.sign_with(&attacker); // But signed by attacker

        assert!(cert.verify().is_err());
    }

    #[test]
    fn test_tampered_certificate() {
        let authority = DeviceKeypair::generate();
        let member = DeviceKeypair::generate();

        let now = now_ms();

        let mut cert = MembershipCertificate::new(
            member.public_key_bytes(),
            "ALPHA-01".to_string(),
            "A1B2C3D4".to_string(),
            now,
            now + 1000,
            MemberPermissions::STANDARD,
            authority.public_key_bytes(),
        )
        .signed(&authority);

        // Tamper with permissions
        cert.permissions = MemberPermissions::ADMIN;

        assert!(cert.verify().is_err());
    }

    #[test]
    fn test_certificate_registry() {
        let authority = DeviceKeypair::generate();
        let member1 = DeviceKeypair::generate();
        let member2 = DeviceKeypair::generate();

        let now = now_ms();
        let expires = now + 24 * 60 * 60 * 1000;

        let cert1 = MembershipCertificate::new(
            member1.public_key_bytes(),
            "ALPHA-01".to_string(),
            "A1B2C3D4".to_string(),
            now,
            expires,
            MemberPermissions::STANDARD,
            authority.public_key_bytes(),
        )
        .signed(&authority);

        let cert2 = MembershipCertificate::new(
            member2.public_key_bytes(),
            "BRAVO-02".to_string(),
            "A1B2C3D4".to_string(),
            now,
            expires,
            MemberPermissions::STANDARD,
            authority.public_key_bytes(),
        )
        .signed(&authority);

        let mut registry = CertificateRegistry::new();

        // Register certificates
        assert!(registry.register(cert1.clone()).is_none());
        assert!(registry.register(cert2.clone()).is_none());
        assert_eq!(registry.len(), 2);

        // Lookup by public key
        let found = registry.get_by_pubkey(&member1.public_key_bytes()).unwrap();
        assert_eq!(found.callsign, "ALPHA-01");

        // Lookup by callsign
        let found = registry.get_by_callsign("BRAVO-02").unwrap();
        assert_eq!(found.member_public_key, member2.public_key_bytes());

        // Check callsign taken
        assert!(registry.is_callsign_taken("ALPHA-01"));
        assert!(!registry.is_callsign_taken("CHARLIE-03"));

        // Remove
        let removed = registry.remove(&member1.public_key_bytes());
        assert!(removed.is_some());
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_callsign_taken("ALPHA-01"));
    }

    #[test]
    fn test_registry_remove_expired() {
        let authority = DeviceKeypair::generate();
        let member1 = DeviceKeypair::generate();
        let member2 = DeviceKeypair::generate();

        let now = 1000000u64;
        let grace = 1000u64;

        // cert1: already expired beyond grace
        let cert1 = MembershipCertificate::new(
            member1.public_key_bytes(),
            "EXPIRED-01".to_string(),
            "A1B2C3D4".to_string(),
            now - 10000,
            now - 5000, // Expired 5 seconds ago
            MemberPermissions::STANDARD,
            authority.public_key_bytes(),
        )
        .signed(&authority);

        // cert2: still valid
        let cert2 = MembershipCertificate::new(
            member2.public_key_bytes(),
            "VALID-02".to_string(),
            "A1B2C3D4".to_string(),
            now,
            now + 10000, // Expires in 10 seconds
            MemberPermissions::STANDARD,
            authority.public_key_bytes(),
        )
        .signed(&authority);

        let mut registry = CertificateRegistry::new();
        registry.register(cert1);
        registry.register(cert2);
        assert_eq!(registry.len(), 2);

        // Remove expired (with 1 second grace)
        let removed = registry.remove_expired(now, grace);
        assert_eq!(removed, 1);
        assert_eq!(registry.len(), 1);
        assert!(registry.get_by_callsign("VALID-02").is_some());
        assert!(registry.get_by_callsign("EXPIRED-01").is_none());
    }

    #[test]
    fn test_permissions() {
        assert!(MemberPermissions::STANDARD.contains(MemberPermissions::RELAY));
        assert!(MemberPermissions::STANDARD.contains(MemberPermissions::EMERGENCY));
        assert!(!MemberPermissions::STANDARD.contains(MemberPermissions::ENROLL));
        assert!(!MemberPermissions::STANDARD.contains(MemberPermissions::ADMIN));

        assert!(MemberPermissions::AUTHORITY.contains(MemberPermissions::RELAY));
        assert!(MemberPermissions::AUTHORITY.contains(MemberPermissions::EMERGENCY));
        assert!(MemberPermissions::AUTHORITY.contains(MemberPermissions::ENROLL));
        assert!(MemberPermissions::AUTHORITY.contains(MemberPermissions::ADMIN));
    }

    #[cfg(feature = "bluetooth")]
    mod token_conversion_tests {
        use super::*;

        #[test]
        fn test_certificate_to_token() {
            let authority = DeviceKeypair::generate();
            let member = DeviceKeypair::generate();

            let now = 1000000u64;
            let expires = now + 86_400_000; // 24 hours

            let cert = MembershipCertificate::new(
                member.public_key_bytes(),
                "ALPHA-07".to_string(),
                "A1B2C3D4".to_string(),
                now,
                expires,
                MemberPermissions::STANDARD,
                authority.public_key_bytes(),
            )
            .signed(&authority);

            // Convert to token
            let token = cert.to_token(&authority);

            // Verify token properties
            assert_eq!(token.public_key, member.public_key_bytes());
            assert_eq!(token.callsign_str(), "ALPHA-07");
            assert_eq!(token.mesh_id_hex(), "A1B2C3D4");
            assert_eq!(token.issued_at_ms, now);
            assert_eq!(token.expires_at_ms, expires);

            // Token should be verifiable
            let authority_identity = hive_btle::security::DeviceIdentity::from_private_key(
                &authority.secret_key_bytes(),
            )
            .unwrap();
            assert!(token.verify(&authority_identity.public_key()));
        }

        #[test]
        fn test_token_to_certificate() {
            let authority = DeviceKeypair::generate();
            let member_pubkey = DeviceKeypair::generate().public_key_bytes();

            // Create a token (simulating what WearTAK might receive)
            let authority_identity = hive_btle::security::DeviceIdentity::from_private_key(
                &authority.secret_key_bytes(),
            )
            .unwrap();

            let mesh_id = [0xA1, 0xB2, 0xC3, 0xD4];
            let now = 1000000u64;
            let expires = now + 86_400_000;

            let token = hive_btle::security::MembershipToken::issue_at(
                &authority_identity,
                mesh_id,
                member_pubkey,
                "BRAVO-03",
                now,
                expires,
            );

            // Convert to certificate
            let cert = MembershipCertificate::from_token(&token, &authority);

            // Verify certificate properties
            assert_eq!(cert.member_public_key, member_pubkey);
            assert_eq!(cert.callsign, "BRAVO-03");
            assert_eq!(cert.mesh_id, "A1B2C3D4");
            assert_eq!(cert.issued_at_ms, now);
            assert_eq!(cert.expires_at_ms, expires);
            assert_eq!(cert.permissions, MemberPermissions::STANDARD);
            assert_eq!(cert.issuer_public_key, authority.public_key_bytes());

            // Certificate should verify
            assert!(cert.verify().is_ok());
        }

        #[test]
        fn test_roundtrip_conversion() {
            let authority = DeviceKeypair::generate();
            let member = DeviceKeypair::generate();

            let now = 1000000u64;
            let expires = now + 86_400_000;

            // Start with certificate
            let original_cert = MembershipCertificate::new(
                member.public_key_bytes(),
                "CHARLIE-99".to_string(),
                "DEADBEEF".to_string(),
                now,
                expires,
                MemberPermissions::RELAY | MemberPermissions::EMERGENCY | MemberPermissions::ENROLL,
                authority.public_key_bytes(),
            )
            .signed(&authority);

            // Convert to token (loses ENROLL permission)
            let token = original_cert.to_token(&authority);

            // Convert back to certificate (gets STANDARD permissions)
            let recovered_cert = MembershipCertificate::from_token(&token, &authority);

            // Core fields preserved
            assert_eq!(
                recovered_cert.member_public_key,
                original_cert.member_public_key
            );
            assert_eq!(recovered_cert.callsign, original_cert.callsign);
            assert_eq!(recovered_cert.mesh_id, original_cert.mesh_id);
            assert_eq!(recovered_cert.issued_at_ms, original_cert.issued_at_ms);
            assert_eq!(recovered_cert.expires_at_ms, original_cert.expires_at_ms);

            // Permissions reset to STANDARD (tokens don't carry permissions)
            assert_eq!(recovered_cert.permissions, MemberPermissions::STANDARD);

            // Both should verify
            assert!(original_cert.verify().is_ok());
            assert!(recovered_cert.verify().is_ok());
        }

        #[test]
        fn test_long_callsign_truncation() {
            let authority = DeviceKeypair::generate();
            let member = DeviceKeypair::generate();

            // Certificate with 16-char callsign (max for MembershipCertificate)
            let cert = MembershipCertificate::new(
                member.public_key_bytes(),
                "ALPHA-BRAVO-1234".to_string(), // 16 chars
                "A1B2C3D4".to_string(),
                1000,
                2000,
                MemberPermissions::STANDARD,
                authority.public_key_bytes(),
            )
            .signed(&authority);

            // Convert to token (max 12 chars)
            let token = cert.to_token(&authority);

            // Callsign should be truncated
            assert_eq!(token.callsign_str(), "ALPHA-BRAVO-");
            assert_eq!(token.callsign_str().len(), 12);
        }
    }
}
