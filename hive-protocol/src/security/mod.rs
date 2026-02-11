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

// Re-export stub submodules (generic primitives now live in hive-mesh)
mod callsign;
mod device_id;
mod encryption;
mod error;
mod formation_key;
mod keypair;

// HIVE-specific security modules (depend on hive-schema / domain types)
mod audit;
mod auth_state;
mod authenticator;
mod authorization;
mod membership;
mod transport;
mod user_auth;

// --- Generic security primitives re-exported from hive-mesh ---

pub use hive_mesh::security::{
    // Callsign generation
    CallsignError,
    CallsignGenerator,
    // Device identity
    DeviceId,
    DeviceKeypair,
    // Encryption
    EncryptedCellMessage,
    EncryptedData,
    EncryptedDocument,
    EncryptionKeypair,
    EncryptionManager,
    // Formation key authentication
    FormationAuthResult,
    FormationChallenge,
    FormationChallengeResponse,
    FormationKey,
    GroupKey,
    SecureChannel,
    SecurityError,
    SymmetricKey,
    // Module-level constants
    CHALLENGE_NONCE_SIZE,
    DEFAULT_CHALLENGE_TIMEOUT_SECS,
    // Formation constants
    FORMATION_CHALLENGE_SIZE,
    FORMATION_RESPONSE_SIZE,
    // Constants
    MAX_CALLSIGN_LENGTH,
    NATO_ALPHABET,
    // Encryption constants
    NONCE_SIZE,
    PUBLIC_KEY_SIZE,
    SIGNATURE_SIZE,
    SYMMETRIC_KEY_SIZE,
    TOTAL_CALLSIGNS,
    X25519_PUBLIC_KEY_SIZE,
};

// --- HIVE-specific exports ---

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

// Auth state tracking (ADR-048: Graceful Degradation)
pub use auth_state::{
    AuthConfig, AuthStateEvent, AuthStateMonitor, AuthStateTracker, CertificateState,
};

// Re-export protobuf types for convenience
pub use hive_schema::security::v1::{
    Challenge, DeviceIdentity, DeviceType as ProtoDeviceType,
    HierarchyLevel as ProtoHierarchyLevel, SecurityError as ProtoSecurityError, SignedBeacon,
    SignedChallengeResponse,
};

// Integration with main crate error type (moved from error.rs)
impl From<hive_mesh::security::SecurityError> for crate::Error {
    fn from(err: hive_mesh::security::SecurityError) -> Self {
        crate::Error::Security(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all public types are accessible
        let _: fn() -> DeviceKeypair = DeviceKeypair::generate;
    }
}
