# HIVE Protocol Specification: Security Framework

**Spec ID**: HIVE-SPEC-005
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: (r)evolve - Revolve Team LLC

## Abstract

This document specifies the security framework for HIVE Protocol. It defines device authentication, user authorization, encryption, key management, and audit logging requirements.

## Table of Contents

1. [Introduction](#1-introduction)
2. [Terminology](#2-terminology)
3. [Security Architecture](#3-security-architecture)
4. [Device Identity](#4-device-identity)
5. [Authentication](#5-authentication)
6. [Authorization](#6-authorization)
7. [Encryption](#7-encryption)
8. [Key Management](#8-key-management)
9. [Audit Logging](#9-audit-logging)
10. [Threat Model](#10-threat-model)
11. [Implementation Requirements](#11-implementation-requirements)

---

## 1. Introduction

### 1.1 Purpose

The HIVE security framework ensures that tactical mesh networks operate securely in contested environments. It provides:
- Device identity verification
- Cell membership authentication
- Role-based access control
- End-to-end encryption
- Comprehensive audit logging

### 1.2 Security Objectives

| Objective | Mechanism |
|-----------|-----------|
| Authenticity | Ed25519 signatures |
| Confidentiality | ChaCha20-Poly1305 AEAD |
| Integrity | Cryptographic hashes + signatures |
| Authorization | RBAC + hierarchy verification |
| Non-repudiation | Signed audit logs |

### 1.3 Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Device Keypair** | Ed25519 signing key for device identity |
| **Device ID** | SHA-256 hash of public key (32 bytes) |
| **Formation Key** | Pre-shared secret for cell admission |
| **Group Key** | Symmetric key for cell broadcast encryption |
| **Secure Channel** | Encrypted peer-to-peer connection |
| **Principal** | Entity (device or user) with permissions |
| **Clearance** | Security classification level |

---

## 3. Security Architecture

### 3.1 Layer Model

```
┌─────────────────────────────────────────────────────────────────┐
│                    Application Security                          │
│  (Input validation, business logic authorization)                │
├─────────────────────────────────────────────────────────────────┤
│                    Protocol Security                             │
│  (Message signing, CRDT authentication, replay protection)       │
├─────────────────────────────────────────────────────────────────┤
│                    Transport Security                            │
│  (TLS 1.3 via QUIC, secure channels, bypass encryption)         │
├─────────────────────────────────────────────────────────────────┤
│                    Identity Security                             │
│  (Device PKI, user auth, formation key verification)            │
├─────────────────────────────────────────────────────────────────┤
│                    Hardware Security (Optional)                  │
│  (TPM, Secure Enclave, PUF-derived keys)                        │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 Trust Boundaries

```
┌─────────────────────────────────────────────────────────────────┐
│                      Untrusted Network                           │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    Cell Boundary                           │  │
│  │  ┌─────────────────────────────────────────────────────┐  │  │
│  │  │              Authenticated Peers                     │  │  │
│  │  │  ┌─────────────────────────────────────────────┐    │  │  │
│  │  │  │         Authorized Principals               │    │  │  │
│  │  │  │  ┌─────────────────────────────────────┐   │    │  │  │
│  │  │  │  │    Local Process (Trusted)          │   │    │  │  │
│  │  │  │  └─────────────────────────────────────┘   │    │  │  │
│  │  │  └─────────────────────────────────────────────┘    │  │  │
│  │  └─────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 4. Device Identity

### 4.1 Key Generation

Devices MUST generate an Ed25519 keypair at initialization:

```rust
pub struct DeviceKeypair {
    /// Ed25519 signing key (32 bytes secret)
    signing_key: SigningKey,
    /// Ed25519 verification key (32 bytes public)
    verifying_key: VerifyingKey,
}

impl DeviceKeypair {
    pub fn generate() -> Self {
        let mut rng = OsRng;
        let signing_key = SigningKey::generate(&mut rng);
        let verifying_key = signing_key.verifying_key();
        Self { signing_key, verifying_key }
    }

    pub fn device_id(&self) -> DeviceId {
        let hash = Sha256::digest(self.verifying_key.as_bytes());
        DeviceId(hash.into())
    }
}
```

### 4.2 Key Storage

Device keys MUST be stored securely:

| Platform | Storage Mechanism |
|----------|-------------------|
| Linux | File with mode 0600, encrypted at rest |
| Android | Android Keystore (hardware-backed) |
| iOS | Secure Enclave |
| Windows | DPAPI or TPM 2.0 |
| ESP32 | eFuse or NVS with encryption |

### 4.3 Device Identity Binding

```protobuf
message DeviceIdentity {
    // Device ID (SHA-256 of public key)
    bytes device_id = 1;
    // Ed25519 public key (32 bytes)
    bytes public_key = 2;
    // Device type
    DeviceType device_type = 3;
    // Hardware attestation (if available)
    optional bytes attestation = 4;
    // Display name
    optional string display_name = 5;
    // Creation timestamp
    Timestamp created_at = 6;
}
```

---

## 5. Authentication

### 5.1 Challenge-Response Protocol

```
    Prover (joining node)           Verifier (cell member)
           │                              │
           │───── AuthRequest ───────────>│
           │  (device_id, public_key)     │
           │                              │
           │<──── Challenge ──────────────│
           │  (nonce, timestamp)          │
           │                              │
           │───── ChallengeResponse ─────>│
           │  (signature over nonce)      │
           │                              │
           │<──── AuthResult ─────────────│
           │  (success/failure, session)  │
           │                              │
```

### 5.2 Challenge Generation

```rust
pub fn generate_challenge() -> Challenge {
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);

    Challenge {
        nonce: nonce.to_vec(),
        timestamp: current_timestamp(),
        challenger_id: self.device_id.clone(),
    }
}
```

### 5.3 Challenge Response

The prover signs:
```
message = nonce || challenger_id || timestamp
signature = Ed25519_Sign(signing_key, message)
```

### 5.4 Verification

```rust
pub fn verify_response(
    response: &SignedChallengeResponse,
    original_challenge: &Challenge,
) -> Result<DeviceId, SecurityError> {
    // Check timestamp freshness (max 30 seconds)
    if is_expired(&original_challenge.timestamp, 30) {
        return Err(SecurityError::ExpiredChallenge);
    }

    // Reconstruct signed message
    let message = [
        &original_challenge.nonce[..],
        &original_challenge.challenger_id[..],
        &timestamp_bytes(&original_challenge.timestamp)[..],
    ].concat();

    // Verify signature
    let public_key = VerifyingKey::from_bytes(&response.public_key)?;
    let signature = Signature::from_bytes(&response.signature)?;

    public_key.verify(&message, &signature)?;

    // Derive and return device ID
    Ok(DeviceId::from_public_key(&response.public_key))
}
```

### 5.5 Formation Key Authentication

For cell admission, nodes must also prove knowledge of the formation key:

```rust
pub fn verify_formation_key(
    response: &FormationResponse,
    formation_key: &FormationKey,
    challenge: &FormationChallenge,
) -> Result<(), SecurityError> {
    // Compute expected response
    let expected = Hmac::<Sha256>::new_from_slice(&formation_key.0)?
        .chain_update(&challenge.nonce)
        .finalize()
        .into_bytes();

    // Constant-time comparison
    if !constant_time_eq(&response.proof, &expected) {
        return Err(SecurityError::FormationKeyMismatch);
    }

    Ok(())
}
```

---

## 6. Authorization

### 6.1 Role-Based Access Control

```rust
pub enum Role {
    /// Observer with read-only access
    Observer,
    /// Standard member with read/write
    Member,
    /// Operator with human authority
    Operator,
    /// Cell leader with admin rights
    Leader,
    /// Parent cell supervisor
    Supervisor,
}

pub enum Permission {
    /// Read documents
    Read,
    /// Write documents
    Write,
    /// Delete documents
    Delete,
    /// Modify cell membership
    ModifyMembership,
    /// Issue commands
    IssueCommands,
    /// Access classified data
    AccessClassified { level: ClearanceLevel },
}
```

### 6.2 Permission Matrix

| Role | Read | Write | Delete | Membership | Commands | Classified |
|------|------|-------|--------|------------|----------|------------|
| Observer | Yes | No | No | No | No | Own level |
| Member | Yes | Yes | Own | No | No | Own level |
| Operator | Yes | Yes | Yes | No | Yes | Own level |
| Leader | Yes | Yes | Yes | Yes | Yes | Cell level |
| Supervisor | Yes | Yes | Yes | Yes | Yes | Parent level |

### 6.3 Clearance Levels

```rust
pub enum ClearanceLevel {
    /// Unclassified
    Unclassified = 0,
    /// Controlled Unclassified Information
    CUI = 1,
    /// Confidential
    Confidential = 2,
    /// Secret
    Secret = 3,
    /// Top Secret
    TopSecret = 4,
}
```

### 6.4 Authorization Check

```rust
pub fn check_authorization(
    principal: &Principal,
    action: &Action,
    resource: &Resource,
    context: &AuthorizationContext,
) -> Result<(), AuthorizationError> {
    // Check role permission
    if !principal.role.has_permission(&action.permission) {
        return Err(AuthorizationError::InsufficientRole);
    }

    // Check clearance level
    if principal.clearance < resource.classification {
        return Err(AuthorizationError::InsufficientClearance);
    }

    // Check cell membership
    if !context.cell.contains(&principal.device_id) {
        return Err(AuthorizationError::NotCellMember);
    }

    // Check hierarchy (for parent/child access)
    if let Some(required_level) = action.required_hierarchy_level {
        if context.hierarchy_level > required_level {
            return Err(AuthorizationError::HierarchyViolation);
        }
    }

    Ok(())
}
```

---

## 7. Encryption

### 7.1 Algorithms

| Purpose | Algorithm | Key Size |
|---------|-----------|----------|
| Symmetric encryption | ChaCha20-Poly1305 | 256 bits |
| Key exchange | X25519 | 256 bits |
| Signing | Ed25519 | 256 bits |
| Hashing | SHA-256 | 256 bits |
| Key derivation | HKDF-SHA256 | Variable |

### 7.2 Secure Channel Establishment

```
    Initiator                         Responder
        │                                 │
        │──── Ephemeral Public Key ──────>│
        │  (X25519 public key)            │
        │                                 │
        │<─── Ephemeral Public Key ───────│
        │                                 │
        │ (Both compute shared secret)    │
        │                                 │
        │ shared = X25519(my_secret, their_public)
        │                                 │
        │ keys = HKDF(shared, salt, info) │
        │   - initiator_to_responder_key  │
        │   - responder_to_initiator_key  │
        │                                 │
```

### 7.3 Message Encryption

```rust
pub fn encrypt_message(
    plaintext: &[u8],
    key: &SymmetricKey,
) -> Result<EncryptedData, EncryptionError> {
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);

    let cipher = ChaCha20Poly1305::new(key.as_ref().into());
    let ciphertext = cipher.encrypt(&nonce.into(), plaintext)?;

    Ok(EncryptedData {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}

pub fn decrypt_message(
    encrypted: &EncryptedData,
    key: &SymmetricKey,
) -> Result<Vec<u8>, EncryptionError> {
    let cipher = ChaCha20Poly1305::new(key.as_ref().into());
    let nonce = GenericArray::from_slice(&encrypted.nonce);

    cipher.decrypt(nonce, encrypted.ciphertext.as_ref())
        .map_err(|_| EncryptionError::DecryptionFailed)
}
```

### 7.4 Group Encryption

For cell-wide broadcasts, a shared GroupKey is used:

```rust
pub struct GroupKey {
    /// The symmetric key material
    key: [u8; 32],
    /// Key generation/epoch
    generation: u64,
    /// Expiration timestamp
    expires_at: Timestamp,
}
```

---

## 8. Key Management

### 8.1 Key Hierarchy

```
                    ┌───────────────────┐
                    │  Device Master Key │
                    │  (Ed25519 keypair) │
                    └─────────┬─────────┘
                              │
              ┌───────────────┼───────────────┐
              │               │               │
              ▼               ▼               ▼
       ┌──────────┐    ┌──────────┐    ┌──────────┐
       │ Signing  │    │ Identity │    │ Derivation│
       │   Key    │    │   Key    │    │   Key    │
       └──────────┘    └──────────┘    └─────┬────┘
                                             │
                              ┌──────────────┼──────────────┐
                              │              │              │
                              ▼              ▼              ▼
                       ┌──────────┐   ┌──────────┐   ┌──────────┐
                       │ Channel  │   │  Group   │   │ Storage  │
                       │   Keys   │   │   Keys   │   │   Keys   │
                       └──────────┘   └──────────┘   └──────────┘
```

### 8.2 Key Rotation

#### Formation Key Rotation
- **Interval**: SHOULD rotate after any member departure
- **Method**: Leader generates new key, distributes via secure channels
- **Grace period**: 5 minutes for late-arriving updates

#### Group Key Rotation
- **Interval**: Configurable (default: 24 hours or on member change)
- **Method**: MLS tree ratcheting (recommended) or leader distribution
- **Retained epochs**: Keep last 5 keys for late messages

#### Device Key Rotation
- **Interval**: Annually or on suspected compromise
- **Method**: Generate new keypair, re-authenticate to cells
- **Impact**: Requires manual re-provisioning

### 8.3 Key Distribution

```protobuf
message KeyDistribution {
    // Key type being distributed
    KeyType type = 1;
    // Encrypted key material (per recipient)
    repeated EncryptedKeyShare shares = 2;
    // Key generation/epoch
    uint64 generation = 3;
    // Expiration
    Timestamp expires_at = 4;
    // Signature from distributor
    bytes signature = 5;
}

message EncryptedKeyShare {
    bytes recipient_id = 1;
    bytes encrypted_key = 2;  // Encrypted with recipient's public key
    bytes nonce = 3;
}
```

### 8.4 Forward Secrecy

HIVE provides forward secrecy through:
1. **Ephemeral keys**: New X25519 keypair per session
2. **Key ratcheting**: Group keys advance after member removal
3. **MLS integration** (recommended): Full forward secrecy via tree-based key agreement

---

## 9. Audit Logging

### 9.1 Audit Events

```rust
pub enum AuditEventType {
    // Authentication events
    AuthenticationAttempt { device_id: DeviceId, success: bool },
    FormationJoin { device_id: DeviceId, cell_id: CellId },
    FormationLeave { device_id: DeviceId, cell_id: CellId },

    // Authorization events
    AuthorizationCheck { principal: Principal, action: Action, allowed: bool },
    PermissionChange { target: DeviceId, old_role: Role, new_role: Role },

    // Key management events
    KeyRotation { key_type: KeyType, generation: u64 },
    KeyDistribution { recipients: Vec<DeviceId> },

    // Security violations
    SecurityViolation { violation: SecurityViolation, source: DeviceId },
}

pub enum SecurityViolation {
    InvalidSignature,
    ExpiredChallenge,
    ReplayDetected,
    UnauthorizedAccess,
    MalformedMessage,
    RateLimitExceeded,
}
```

### 9.2 Audit Log Entry

```protobuf
message AuditLogEntry {
    // Unique entry ID
    bytes entry_id = 1;
    // Timestamp
    Timestamp timestamp = 2;
    // Event type
    AuditEventType event = 3;
    // Device that logged this entry
    bytes logger_id = 4;
    // Device that triggered the event
    optional bytes actor_id = 5;
    // Human-readable description
    string description = 6;
    // Additional context (JSON)
    optional bytes context = 7;
    // Hash of previous entry (chain integrity)
    bytes previous_hash = 8;
    // Signature over entry
    bytes signature = 9;
}
```

### 9.3 Log Integrity

Audit logs form a hash chain for tamper detection:

```
Entry[n].previous_hash = SHA256(Entry[n-1])
Entry[n].signature = Sign(Entry[n] - signature field)
```

### 9.4 Log Retention

| Log Type | Minimum Retention |
|----------|-------------------|
| Authentication | 90 days |
| Authorization | 30 days |
| Security violations | 1 year |
| Key management | 2 years |

---

## 10. Threat Model

### 10.1 Adversary Capabilities

| Adversary | Capabilities |
|-----------|--------------|
| Passive Eavesdropper | Monitor network traffic |
| Active Attacker | Inject, modify, replay messages |
| Compromised Node | Full control of one cell member |
| Insider Threat | Valid credentials, malicious intent |

### 10.2 Threats and Mitigations

| Threat | Mitigation |
|--------|------------|
| Eavesdropping | TLS 1.3, ChaCha20-Poly1305 encryption |
| Impersonation | Ed25519 device authentication |
| Replay attacks | Timestamp + nonce + sequence numbers |
| Man-in-the-middle | Public key verification, challenge-response |
| Unauthorized access | RBAC, clearance levels |
| Data tampering | Cryptographic signatures |
| Key compromise | Key rotation, forward secrecy |
| Denial of service | Rate limiting, connection limits |

### 10.3 Out of Scope

- Physical access to device
- Side-channel attacks (timing, power analysis)
- Quantum computing attacks (future consideration)

---

## 11. Implementation Requirements

### 11.1 MUST Implement

- Ed25519 device keypair generation and storage
- Challenge-response authentication
- Formation key verification
- ChaCha20-Poly1305 encryption for group messages
- Basic audit logging (auth, security violations)

### 11.2 SHOULD Implement

- X25519 secure channel establishment
- Role-based access control
- Key rotation mechanisms
- Hardware-backed key storage
- Comprehensive audit logging

### 11.3 MAY Implement

- MLS-based group key agreement
- Hardware attestation (TPM, Secure Enclave)
- PUF-derived device identity
- Zero-knowledge membership proofs
- Security clearance enforcement

### 11.4 Cryptographic Library Requirements

Implementations MUST use:
- Constant-time comparison for secrets
- Secure random number generation (OS-provided)
- Approved algorithm implementations (audited libraries)

RECOMMENDED libraries:
- Rust: `ed25519-dalek`, `x25519-dalek`, `chacha20poly1305`
- C: libsodium
- Android: Android Keystore + Tink
- iOS: CryptoKit

---

## Appendix A: References

- RFC 8032: Edwards-Curve Digital Signature Algorithm (Ed25519)
- RFC 7748: Elliptic Curves for Security (X25519)
- RFC 8439: ChaCha20 and Poly1305 for IETF Protocols
- RFC 9420: The Messaging Layer Security (MLS) Protocol
- NIST SP 800-57: Key Management Guidelines
- ADR-006: Security Authentication Authorization
- ADR-044: E2E Encryption and Key Management

## Appendix B: Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-07 | Initial draft |
