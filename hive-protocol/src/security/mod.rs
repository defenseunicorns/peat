//! # Security Module - Device Authentication (PKI) for HIVE Protocol
//!
//! Implements ADR-006 Layer 1: Device Identity and Authentication.
//!
//! ## Overview
//!
//! This module provides cryptographic device authentication using Ed25519 signatures.
//! Every device has a keypair that proves its identity through challenge-response.
//!
//! ## Key Types
//!
//! - [`DeviceId`] - Unique identifier derived from Ed25519 public key
//! - [`DeviceKeypair`] - Ed25519 keypair for signing and identity
//! - [`DeviceAuthenticator`] - Manages challenge-response authentication
//!
//! ## Usage
//!
//! ```ignore
//! use hive_protocol::security::{DeviceKeypair, DeviceAuthenticator, DeviceId};
//!
//! // Generate a new device identity
//! let keypair = DeviceKeypair::generate();
//! let device_id = keypair.device_id();
//!
//! // Create authenticator
//! let authenticator = DeviceAuthenticator::new(keypair);
//!
//! // Generate challenge for peer
//! let challenge = authenticator.generate_challenge();
//!
//! // Respond to challenge (peer side)
//! let response = peer_authenticator.respond_to_challenge(&challenge)?;
//!
//! // Verify peer response
//! let peer_id = authenticator.verify_response(&response)?;
//! ```

mod audit;
mod authenticator;
mod authorization;
mod device_id;
mod encryption;
mod error;
mod formation_key;
mod keypair;
mod membership;
mod transport;
mod user_auth;

pub use audit::{
    AuditEventType, AuditLogEntry, AuditLogger, FileAuditLogger, MemoryAuditLogger,
    NullAuditLogger, SecurityViolation,
};
pub use authenticator::{DeviceAuthenticator, VerifiedPeer};
pub use authorization::{
    AuthenticatedEntity, AuthorizationContext, AuthorizationController, AuthorizationPolicy,
    CellMembershipContext, DeviceIdentityInfo, DeviceType, HierarchyLevel, Permission, Role,
    UserIdentityInfo,
};
pub use device_id::DeviceId;
pub use encryption::{
    EncryptedCellMessage, EncryptedData, EncryptedDocument, EncryptionKeypair, EncryptionManager,
    GroupKey, SecureChannel, SymmetricKey, NONCE_SIZE, SYMMETRIC_KEY_SIZE, X25519_PUBLIC_KEY_SIZE,
};
pub use error::SecurityError;
pub use formation_key::{
    FormationAuthResult, FormationChallenge, FormationChallengeResponse, FormationKey,
    FORMATION_CHALLENGE_SIZE, FORMATION_RESPONSE_SIZE,
};
pub use keypair::DeviceKeypair;
pub use transport::{AuthenticatedConnection, AuthenticationChannel, SecureMeshTransport};
pub use user_auth::{
    AccountStatus, AuthMethod, Credential, LocalUserStore, MilitaryRank, OrganizationUnit,
    SecurityClearance, SessionId, UserAuthenticator, UserIdentity, UserIdentityBuilder, UserRecord,
    UserSession, UserStore,
};

// Membership certificates (ADR-048: Tactical Trust)
pub use membership::{
    CertificateRegistry, MemberPermissions, MembershipCertificate, CERTIFICATE_BASE_SIZE,
    MAX_CALLSIGN_LEN, MESH_ID_LEN,
};

// Re-export protobuf types for convenience
pub use hive_schema::security::v1::{
    Challenge, DeviceIdentity, DeviceType as ProtoDeviceType,
    HierarchyLevel as ProtoHierarchyLevel, SecurityError as ProtoSecurityError, SignedBeacon,
    SignedChallengeResponse,
};

/// Default challenge timeout in seconds
pub const DEFAULT_CHALLENGE_TIMEOUT_SECS: u64 = 30;

/// Size of challenge nonce in bytes
pub const CHALLENGE_NONCE_SIZE: usize = 32;

/// Size of Ed25519 public key in bytes
pub const PUBLIC_KEY_SIZE: usize = 32;

/// Size of Ed25519 signature in bytes
pub const SIGNATURE_SIZE: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all public types are accessible
        let _: fn() -> DeviceKeypair = DeviceKeypair::generate;
    }
}
