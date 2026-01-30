# ADR-048: Membership Certificates and Tactical Trust

**Status**: Proposed
**Date**: 2025-01-29
**Authors**: Codex, Kit Plummer
**Related**: ADR-006 (Security Architecture), ADR-044 (E2E Encryption), ADR-039 (hive-btle Mesh Transport)

## Context

ADR-006 describes a full PKI model with X509 certificates and external CA hierarchies. While appropriate for enterprise deployments, tactical field operations require a simpler, self-contained trust model that:

- Works offline (no external CA connectivity)
- Bootstraps quickly (QR code, NFC tap)
- Has mesh-local authority (squad leader's device)
- Includes callsign binding directly
- Supports time-limited credentials with re-authentication

### Current Problem

Mesh encryption (ChaCha20-Poly1305 via hive-btle) proves a sender has the mesh key, but does NOT authenticate *which* mesh member sent a message. This enables spoofing:

1. Attacker joins mesh (has mesh key)
2. Attacker sends CannedMessage with `sourceNodeId` = victim's nodeId
3. Receiver displays "CHECK IN from DINO" but DINO never sent it

Additionally:
- Callsigns are self-declared (no authority binding)
- No enrollment process for new devices
- No certificate expiration or re-authentication
- No trust hierarchy (all mesh members are equal)

## Decision

### Tactical Trust Model

Implement a lightweight, mesh-local trust hierarchy:

```
MeshGenesis (root of trust)
    │
    ├── mesh_seed (256-bit, derives encryption key)
    ├── mesh_id (8 hex chars)
    ├── auth_interval_hours (e.g., 24)
    ├── grace_period_hours (e.g., 4)
    │
    └── creator_public_key (authority)
            │
            └── signs ──► MembershipCertificate
                              ├── member_public_key
                              ├── member_node_id (derived)
                              ├── callsign (authority-assigned)
                              ├── mesh_id
                              ├── issued_at_ms
                              ├── expires_at_ms
                              ├── permissions
                              └── issuer_signature
```

### MembershipCertificate Structure

```rust
pub struct MembershipCertificate {
    /// Device's Ed25519 public key
    pub member_public_key: [u8; 32],

    /// Authority-assigned callsign (max 16 UTF-8 bytes)
    pub callsign: String,

    /// Mesh identifier (8 hex chars)
    pub mesh_id: String,

    /// Timestamp when issued (ms since Unix epoch)
    pub issued_at_ms: u64,

    /// Expiration timestamp (issued_at + mesh.auth_interval)
    pub expires_at_ms: u64,

    /// Permission flags
    pub permissions: MemberPermissions,

    /// Issuer's public key (for delegation chains)
    pub issuer_public_key: [u8; 32],

    /// Ed25519 signature over all above fields
    pub issuer_signature: [u8; 64],
}

bitflags! {
    pub struct MemberPermissions: u8 {
        const RELAY      = 0b0000_0001;  // Can relay messages
        const EMERGENCY  = 0b0000_0010;  // Can trigger emergencies
        const ENROLL     = 0b0000_0100;  // Can enroll new members (delegation)
        const ADMIN      = 0b1000_0000;  // Full authority
    }
}
```

Wire format: ~145 bytes (32 + 1 + callsign + 8 + 8 + 8 + 1 + 32 + 64)

### Mesh-Level Auth Policy

Extend MeshGenesis with authentication policy:

```rust
pub struct MeshGenesis {
    // ... existing fields ...

    /// Re-authentication interval (0 = no expiry)
    pub auth_interval_hours: u16,

    /// Grace period after expiration before hard cutoff
    pub grace_period_hours: u16,
}
```

**Recommended defaults**: 24-hour auth interval, 4-hour grace period.

### Callsign Assignment

Authority has two options:
1. **Manual entry** - Type callsign directly
2. **Randomize** - Generate NATO phonetic + 2-digit (e.g., ALPHA-01, ZULU-47)

NATO + 2-digit provides 2,600 unique callsigns with familiar, radio-clear format.

### Graceful Degradation

**During grace period** (cert expired but within grace):
- Device shows prominent "AUTH EXPIRED" warning
- Device continues to operate (send/receive)
- Other devices see this device flagged as "stale cert"
- Authority can still re-issue certificate

**After grace period**:
- Hard cutoff - device cannot participate in mesh
- Must re-enroll from scratch (or authority manually extends)

### Message Authentication

All messages include Ed25519 signature from sender:

```
[marker:1][payload:N][signature:64]
```

Receive flow:
1. Decrypt with mesh key (proves mesh membership)
2. Lookup `source_node_id` in certificate cache
3. If unknown → reject message
4. Verify signature using cached public key
5. Display with verified callsign from certificate

### Enrollment Flows

#### ATAK Tablet (Authority)

Creates mesh and issues certificates:

```
1. Create Mesh
   - Enter mesh name
   - Set auth_interval (default: 24h)
   - Set grace_period (default: 4h)
   - Genesis created, tablet becomes authority

2. Enroll Device ("Add Device")
   - Display QR: [mesh_id + enrollment_token]
   - Wait for BLE connection from new device
   - Receive device's IdentityAttestation
   - Assign callsign (manual entry or randomize)
   - Sign and send MembershipCertificate + MeshCredentials
```

#### WearTAK Watch (Enrolled Device)

```
1. Scan QR from authority (camera)
2. Connect to authority via BLE
3. Send IdentityAttestation + enrollment_token
4. Receive MeshCredentials + MembershipCertificate
5. Store credentials and begin operation
```

#### NPE/ESP32 (Pre-provisioned)

```
Option A: NFC tap from authority tablet
Option B: USB/serial provisioning during staging
Option C: Pre-loaded certificates (enterprise deployment)
```

### Re-Authentication Flow

```
Timeline:
├── T-0: Certificate issued
├── T-23h: Warning "Re-auth in 1 hour"
├── T-24h: Expiration (grace period starts)
├── T-28h: Hard cutoff (if grace = 4h)
└── Re-auth succeeds: New cert issued, timer resets

Protocol:
1. Device connects to authority
2. Sends current certificate + fresh IdentityAttestation
3. Authority verifies device is still authorized
4. Authority issues new certificate (same callsign, new expiration)
5. Device stores new certificate
```

### Certificate Exchange During Discovery

When two mesh nodes discover each other via BLE:

1. Exchange IdentityAttestations (108 bytes each) - existing protocol
2. Exchange MembershipCertificates (~145 bytes each) - new
3. Each node verifies other's certificate against `genesis.creator_public_key`
4. Store in peer registry: `node_id → (public_key, callsign, certificate)`

Unknown or invalid certificates → reject peer, do not sync.

## Consequences

### Positive

- **Offline operation**: No external CA needed, mesh-local authority
- **Fast enrollment**: QR scan + BLE exchange, <30 seconds
- **Spoofing prevention**: Messages cryptographically bound to sender identity
- **Callsign integrity**: Authority controls who uses which callsign
- **Automatic cleanup**: Expired certs drop compromised/lost devices
- **Simple key management**: Ed25519 only, no X509 complexity

### Negative

- **Single point of authority**: If authority device is lost, no new enrollments
  - Mitigation: Delegation via ENROLL permission flag
- **Certificate size**: 145 bytes per peer, adds to discovery overhead
  - Acceptable for BLE mesh (<10 peers typical)
- **Breaking change**: New wire formats not backward compatible
  - Mitigation: Version negotiation, legacy mode for testing

### Neutral

- Coexists with ADR-006 PKI model for enterprise deployments
- hive-btle provides crypto primitives, HIVE implements policy

## Implementation

### Layer Responsibilities

| Layer | Responsibility |
|-------|----------------|
| **hive-btle** | Ed25519 sign/verify, IdentityAttestation, peer pubkey registry |
| **hive-lite** | Signed wire formats (86-byte CannedMessage) |
| **HIVE** | MembershipCertificate, enrollment protocol, auth policy, UX |
| **Apps** | Enrollment UI, QR display/scan, callsign assignment |

### Related Issues

- **hive-btle** `cac154cc`: Crypto primitives (sign/verify utilities)
- **hive-lite** `2140b82a`: Signed CannedMessage wire format
- **HIVE** `23505348`: Membership certificates, enrollment, trust hierarchy

## References

- hive-btle `src/security/identity.rs`: DeviceIdentity, IdentityAttestation
- hive-btle `src/security/genesis.rs`: MeshGenesis, MembershipPolicy
- ADR-006: Security, Authentication, Authorization (PKI model)
- ADR-044: E2E Encryption and Key Management (MLS, group keys)
