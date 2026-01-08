# ADR-044: End-to-End Encryption and Key Management

**Status**: Proposed
**Date**: 2025-01-07
**Authors**: Codex, Kit Plummer
**Related**: ADR-006 (Security Architecture), ADR-042 (UDP Bypass), ADR-005 (Data Sync)

## Context

HIVE Protocol has a solid security foundation (ADR-006) with:
- Device identity (Ed25519 keypairs)
- Peer-to-peer encryption (X25519 + ChaCha20-Poly1305)
- Formation keys (pre-shared secret authentication)
- Group keys (cell broadcast encryption with generation tracking)

However, critical gaps remain for production tactical deployment:

### Gap 1: Key Distribution Protocol

The current `GroupKey` implementation can generate and rotate keys, but has no protocol for:
- Securely distributing keys to new cell members
- Ensuring removed members cannot read future messages (forward secrecy)
- Synchronizing key state across an asynchronous mesh network

### Gap 2: CRDT-Specific Threats

Kerkour's research notes highlight a critical insight:
> "Removing server-side validation creates vulnerability where malicious clients could introduce invalid mutations, compromising data structure integrity."

In HIVE's mesh topology:
- Any node can propose CRDT mutations
- Compromised nodes could inject malformed documents
- Replay attacks could revert document state
- No mechanism to attribute mutations to authenticated sources

### Gap 3: Bypass Channel Authentication

ADR-042's UDP bypass channel (implemented in Phases 1-4) currently has:
- 12-byte header with collection hash
- No authentication or encryption
- Vulnerable to spoofing, replay, and eavesdropping

### Gap 4: Offline Key Agreement

Tactical DDIL (Denied, Degraded, Intermittent, Limited) environments require:
- Key agreement without real-time communication
- Graceful handling of network partitions during key rotation
- Recovery when nodes rejoin after extended offline periods

## Decision

### Adopt MLS (RFC 9420) for Group Key Agreement

We will integrate the Messaging Layer Security protocol for cell-level key management.

**Why MLS?**
- Standardized (IETF RFC 9420)
- O(log n) complexity for add/remove operations
- Forward secrecy and post-compromise security
- Asynchronous design (supports offline key packages)
- Rust implementations available (OpenMLS, mls-rs)

**Library Choice: OpenMLS**

| Criteria | OpenMLS | mls-rs |
|----------|---------|--------|
| Maturity | More production usage | Newer |
| Ciphersuite | ChaCha20-Poly1305 ✓ | Multiple options |
| License | MIT | Apache-2.0 |
| X.509 Support | Limited | Full |
| Audit Status | Partial | None |

OpenMLS aligns with HIVE's existing crypto (ChaCha20-Poly1305, Ed25519) and has more real-world deployment experience.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Application Layer                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────┐  │
│  │   CRDT      │    │   Bypass    │    │   Cell Membership   │  │
│  │   Sync      │    │   Channel   │    │   Management        │  │
│  └──────┬──────┘    └──────┬──────┘    └──────────┬──────────┘  │
│         │                  │                       │             │
│         ▼                  ▼                       ▼             │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                 E2E Encryption Layer                         │ │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │ │
│  │  │ Document    │  │ Message     │  │ MLS Group           │  │ │
│  │  │ Encryption  │  │ Signing     │  │ Key Agreement       │  │ │
│  │  └─────────────┘  └─────────────┘  └─────────────────────┘  │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                              │                                    │
│                              ▼                                    │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │              Existing Security Layer (ADR-006)               │ │
│  │  DeviceKeypair │ FormationKey │ SecureChannel │ GroupKey    │ │
│  └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Component 1: MLS-Based Cell Key Manager

```rust
/// Cell key manager using MLS for group key agreement
pub struct CellKeyManager {
    /// MLS group state
    mls_group: MlsGroup,

    /// Current epoch's group key (derived from MLS)
    current_key: GroupKey,

    /// Previous epoch keys (for decrypting in-flight messages)
    previous_keys: VecDeque<GroupKey>,

    /// Pending key packages for offline members
    pending_welcomes: HashMap<DeviceId, MlsMessage>,
}

impl CellKeyManager {
    /// Add a member to the cell
    /// Returns Welcome message for new member + Commit for existing members
    pub async fn add_member(&mut self, key_package: KeyPackage)
        -> Result<(MlsMessage, MlsMessage)>;

    /// Remove a member from the cell
    /// Returns Commit that rotates key material (forward secrecy)
    pub async fn remove_member(&mut self, member_id: &DeviceId)
        -> Result<MlsMessage>;

    /// Process incoming MLS message (Welcome, Commit, Proposal)
    pub async fn process_message(&mut self, msg: MlsMessage)
        -> Result<ProcessedMessage>;

    /// Get current encryption key for cell broadcast
    pub fn current_key(&self) -> &GroupKey;

    /// Attempt decryption with current and previous keys
    pub fn decrypt_with_fallback(&self, msg: &EncryptedCellMessage)
        -> Result<Vec<u8>>;
}
```

### Component 2: Signed CRDT Operations

```rust
/// Signed mutation for CRDT operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMutation {
    /// The CRDT operation (Automerge change)
    pub operation: Vec<u8>,

    /// Device that created this mutation
    pub author: DeviceId,

    /// Signature over (operation || author || timestamp || nonce)
    pub signature: [u8; 64],

    /// Timestamp (for ordering and replay detection)
    pub timestamp: u64,

    /// Unique nonce (replay protection)
    pub nonce: [u8; 16],
}

impl SignedMutation {
    /// Create and sign a new mutation
    pub fn new(operation: Vec<u8>, keypair: &DeviceKeypair) -> Self;

    /// Verify signature and check for replay
    pub fn verify(&self, pubkey: &PublicKey, seen_nonces: &mut HashSet<[u8; 16]>)
        -> Result<()>;
}
```

### Component 3: Authenticated Bypass Messages

Extend `BypassHeader` for optional authentication:

```rust
/// Extended bypass header with optional authentication (16 bytes base + 64 bytes sig)
pub struct AuthenticatedBypassHeader {
    /// Base header (12 bytes)
    pub header: BypassHeader,

    /// Flags indicating auth mode
    pub auth_flags: u8,

    /// Reserved
    pub reserved: [u8; 3],

    /// Ed25519 signature (optional, 64 bytes)
    /// Signs: header || payload
    pub signature: Option<[u8; 64]>,
}

/// Authentication mode for bypass messages
#[derive(Debug, Clone, Copy)]
pub enum BypassAuthMode {
    /// No authentication (backward compatible)
    None,
    /// Ed25519 signature
    Signed,
    /// Signed + encrypted with cell key
    SignedEncrypted,
}
```

### Component 4: Encrypted Document Store

```rust
/// Document store with transparent E2E encryption
pub struct EncryptedDocumentStore<S: DocumentStore> {
    /// Underlying store
    inner: S,

    /// Cell key manager for encryption
    key_manager: Arc<CellKeyManager>,

    /// Device keypair for signing
    device_keypair: DeviceKeypair,
}

impl<S: DocumentStore> EncryptedDocumentStore<S> {
    /// Upsert with automatic encryption and signing
    pub async fn upsert(&mut self, collection: &str, doc: Document)
        -> Result<DocumentId> {
        // 1. Serialize document
        // 2. Sign the mutation
        // 3. Encrypt with current cell key
        // 4. Store encrypted blob
    }

    /// Query with automatic decryption and verification
    pub async fn query(&self, collection: &str, query: &Query)
        -> Result<Vec<Document>> {
        // 1. Query encrypted documents
        // 2. Decrypt with key fallback
        // 3. Verify signatures
        // 4. Return verified documents
    }
}
```

## Threat Model Updates

### New Threats Addressed

| Threat | Mitigation |
|--------|------------|
| Compromised node reads future messages | MLS forward secrecy via tree ratcheting |
| Removed member continues to decrypt | Key rotation on remove (new epoch) |
| Replay of old CRDT mutations | Nonce tracking per device |
| Spoofed bypass messages | Ed25519 signatures |
| Eavesdropping on bypass channel | Cell key encryption |
| Malformed CRDT injection | Signature verification before merge |

### Residual Risks

| Risk | Mitigation | Notes |
|------|------------|-------|
| Long-term key compromise | Update/Commit refreshes keys | Requires periodic updates |
| Partition during key rotation | Multiple epoch key retention | Configure retention window |
| Denial of service via invalid proposals | Rate limiting, proposal validation | Application-level |
| Side-channel attacks | Constant-time crypto | Rely on OpenMLS |

## Implementation Phases

### Phase 1: OpenMLS Integration (Issue #538 Scope Expansion)

- Add OpenMLS dependency
- Implement `CellKeyManager` wrapper
- Key package generation and storage
- Basic add/remove member operations
- Unit tests with in-memory provider

### Phase 2: CRDT Signing

- Implement `SignedMutation`
- Integrate with Automerge change creation
- Verify signatures on incoming changes
- Nonce tracking for replay detection

### Phase 3: Bypass Authentication

- Add `AuthenticatedBypassHeader`
- Implement `BypassAuthMode` configuration
- Signature generation/verification
- Optional encryption with cell key

### Phase 4: Encrypted Document Store

- Implement `EncryptedDocumentStore` wrapper
- Transparent encryption/decryption
- Key fallback for in-flight messages during rotation
- Integration with existing `DocumentStore` trait

### Phase 5: Key Distribution Protocol

- Welcome message delivery via mesh
- Offline key package caching
- Partition recovery protocol
- Key rotation scheduling

## Alternatives Considered

### Alternative 1: Custom Key Distribution

Build our own key distribution on top of existing `GroupKey`:

**Pros**: No new dependencies, simpler
**Cons**: Reinventing MLS poorly, no formal security analysis

**Rejected**: MLS provides battle-tested key agreement.

### Alternative 2: Signal Protocol (libsignal)

Use Signal's Double Ratchet for pairwise keys, Sender Keys for group:

**Pros**: Widely deployed, well-analyzed
**Cons**: Designed for 1:1 with group as extension, less efficient for mesh

**Rejected**: MLS is designed for groups from the start.

### Alternative 3: Keyhive (Ink & Switch)

Use Keyhive's CRDT-native key management:

**Pros**: Designed specifically for CRDTs
**Cons**: Research project, not production-ready, P2P only

**Considered for future**: May revisit when mature.

### Alternative 4: mls-rs Instead of OpenMLS

**Pros**: More crypto providers, X.509 support
**Cons**: Less mature, no audit

**Decision**: Start with OpenMLS, but design abstraction to allow swap.

## Dependencies

```toml
[dependencies]
openmls = "0.5"
openmls_rust_crypto = "0.2"  # ChaCha20-Poly1305 provider
openmls_traits = "0.2"
```

## Test Plan

1. **Unit tests**: Key package creation, add/remove, epoch transitions
2. **Integration tests**: Multi-node cell formation with key agreement
3. **Interop tests**: OpenMLS test vectors
4. **Chaos tests**: Network partitions during key rotation
5. **Performance tests**: Measure overhead vs. unencrypted baseline

## Configuration

All security parameters are runtime-configurable with sensible defaults:

### Cell Key Configuration

```rust
/// Configuration for cell-level key management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellKeyConfig {
    /// Key rotation interval (default: 1 hour)
    #[serde(default = "default_rotation_interval")]
    pub rotation_interval: Duration,

    /// Previous epoch key retention for decrypting in-flight messages (default: 24 hours)
    #[serde(default = "default_retention_window")]
    pub key_retention_window: Duration,

    /// Automatically rotate key when member is removed (default: true)
    #[serde(default = "default_true")]
    pub rotate_on_remove: bool,

    /// Maximum epochs to retain (default: 48, roughly 2 days at hourly rotation)
    #[serde(default = "default_max_epochs")]
    pub max_retained_epochs: usize,
}
```

### Per-Collection Bypass Authentication

Extends existing `BypassCollectionConfig`:

```rust
pub struct BypassCollectionConfig {
    pub collection: String,
    pub transport: BypassTransport,
    pub encoding: MessageEncoding,
    pub ttl_ms: u64,
    pub priority: MessagePriority,

    /// Authentication mode for this collection (default: None for backward compat)
    #[serde(default)]
    pub auth_mode: BypassAuthMode,
}

/// Authentication mode for bypass messages
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum BypassAuthMode {
    /// No authentication (backward compatible, fastest)
    #[default]
    None,
    /// Ed25519 signature only (authenticity, no confidentiality)
    /// +64 bytes overhead, ~50μs signing cost
    Signed,
    /// Signed + encrypted with cell key (full protection)
    /// +64 bytes sig + 16 bytes auth tag, ~100μs total cost
    SignedEncrypted,
}
```

**Guidance**: Use `Signed` for high-frequency telemetry (position updates), `SignedEncrypted` for commands and sensitive data.

## Credential Strategy: Ed25519 vs X.509

| Aspect | Ed25519-Only | X.509 Certificates |
|--------|--------------|-------------------|
| **Simplicity** | Just a keypair | Certificate chains, CAs, validity periods |
| **Key size** | 32 bytes public | ~1-2KB per cert |
| **Revocation** | Manual tracking | CRL/OCSP infrastructure |
| **Interop** | HIVE-specific | DoD PKI, NATO systems |
| **Metadata** | None built-in | Org unit, clearance, role in cert |
| **Offline validation** | Always works | Needs cached CRLs |

**Decision**: Start with **Ed25519-only** for simplicity. Design credential abstraction to allow X.509 integration later if DoD PKI requirements emerge. OpenMLS supports both via its credential trait.

```rust
/// Abstraction over credential types
pub enum HiveCredential {
    /// Simple Ed25519 public key (current implementation)
    Ed25519(DeviceKeypair),
    /// X.509 certificate chain (future, if needed)
    X509(Vec<Certificate>),
}
```

## Hardware Root of Trust

For tactical deployment, software-only keys are insufficient. Captured devices could have keys extracted. HIVE should support hardware-backed identity where available.

### Physical Unclonable Functions (PUFs)

PUFs exploit manufacturing variations in silicon to create unique, unclonable device fingerprints. Keys are derived at runtime from physics, not stored in flash.

**Properties:**
- **Unclonable**: Manufacturing variations can't be reproduced
- **No key storage**: Keys derived on-demand (nothing to extract)
- **Tamper-evident**: Physical probing changes PUF response
- **Cost-effective**: ~$0.50 vs $2-5 for discrete TPM

**Enrollment vs Runtime:**

```
Enrollment (provisioning):
  PUF Challenge → Response → Helper Data (stored) + Public Key (registered)

Runtime (each boot):
  PUF Challenge → Noisy Response + Helper Data → Clean Response → Derive Keys
```

**Why it matters for tactical:**

| Threat | Without PUF | With PUF |
|--------|-------------|----------|
| Enemy captures UAV | Extract keys from flash | No keys stored |
| Clone device identity | Copy key material | Can't clone silicon |
| Firmware dump | Reveals secrets | Secrets derived at runtime |

### Key Storage Abstraction

```rust
/// Hardware-backed key storage options
pub enum KeyStorage {
    /// Software-only (works everywhere, least secure)
    Software(SoftwareKey),

    /// PUF-derived keys (no storage, unclonable)
    /// Supported: NXP i.MX RT, Microchip ATECC608, Infineon OPTIGA
    Puf(PufDerivedKey),

    /// TPM 2.0 backed (hardware-bound, attestation capable)
    /// Supported: Most x86 platforms, some ARM
    Tpm(TpmKey),

    /// Secure enclave (ARM TrustZone, Apple Secure Enclave)
    SecureEnclave(EnclaveKey),
}

/// PUF provider abstraction
pub trait PufProvider: Send + Sync {
    /// Get PUF response for challenge
    fn get_response(&self, challenge: &[u8]) -> Result<[u8; 32]>;

    /// Get helper data for error correction
    fn get_helper_data(&self) -> Result<Vec<u8>>;

    /// Reconstruct stable response using helper data
    fn reconstruct(&self, challenge: &[u8], helper: &[u8]) -> Result<[u8; 32]>;
}

/// PUF-derived device identity
pub struct PufDeviceIdentity {
    /// Signing keypair (derived from PUF at boot)
    signing_key: DeviceKeypair,

    /// Encryption keypair (derived from PUF at boot)
    encryption_key: EncryptionKeypair,
}

impl PufDeviceIdentity {
    pub fn from_puf(puf: &dyn PufProvider) -> Result<Self> {
        let response = puf.reconstruct(
            HIVE_PUF_CHALLENGE,
            &puf.get_helper_data()?
        )?;

        // Derive separate keys for signing and encryption
        let signing_seed = hkdf_expand(&response, b"hive-signing-v1");
        let encryption_seed = hkdf_expand(&response, b"hive-encryption-v1");

        Ok(Self {
            signing_key: DeviceKeypair::from_seed(&signing_seed),
            encryption_key: EncryptionKeypair::from_seed(&encryption_seed),
        })
    }
}
```

### Platform Attestation

For high-security deployments, nodes can prove their hardware/software integrity before joining a cell:

```rust
/// Platform attestation for cell join
pub struct PlatformAttestation {
    /// Hardware identity (PUF response hash or TPM EK)
    pub hardware_id: [u8; 32],

    /// Firmware measurement (hash of boot chain)
    pub firmware_hash: [u8; 32],

    /// TPM quote or PUF challenge-response
    pub attestation_proof: AttestationProof,

    /// Timestamp (prevents replay)
    pub timestamp: u64,

    /// Signature over all fields
    pub signature: [u8; 64],
}

pub enum AttestationProof {
    /// TPM 2.0 quote (signed PCR values)
    TpmQuote { pcrs: Vec<u8>, signature: Vec<u8> },

    /// PUF challenge-response
    PufResponse { challenge: [u8; 32], response: [u8; 32] },

    /// Software-only (hash of known binary)
    SoftwareOnly { binary_hash: [u8; 32] },
}
```

### Hardware Support Matrix

| Platform | PUF | TPM | Secure Enclave | HIVE Target |
|----------|-----|-----|----------------|-------------|
| NXP i.MX RT | ✅ SRAM PUF | ❌ | ✅ TrustZone | UAVs, edge |
| Microchip ATECC608 | ✅ Built-in | ❌ | ✅ Secure element | Small UAS, sensors |
| Infineon OPTIGA | ✅ Hardware | ❌ | ✅ | High-security nodes |
| Raspberry Pi | ❌ | ❌ | ❌ | Dev/test only |
| x86 (Intel/AMD) | ❌ | ✅ | ✅ SGX/SEV | Ground vehicles, C2 |
| ESP32-S3 | ⚠️ eFuse HMAC | ❌ | ❌ | Constrained sensors |

### Implementation Priority

| Capability | Priority | Rationale |
|------------|----------|-----------|
| `KeyStorage` abstraction | **High** | Enables all hardware options |
| PUF support (NXP/Microchip) | **High** | Primary UAV platforms |
| Software fallback | **High** | Dev/test, legacy hardware |
| TPM support | **Medium** | Ground vehicles, C2 |
| Platform attestation | **Medium** | High-security cells |
| SGX enclaves | **Low** | Server-side only |

### Zero-Knowledge Extensions (Future)

For scenarios requiring anonymous cell membership:

```rust
/// ZK proof of cell membership without revealing device identity
pub struct ZkMembershipProof {
    /// Proof that prover knows a valid credential
    pub proof: Vec<u8>,

    /// Public inputs (cell ID, epoch)
    pub public_inputs: ZkPublicInputs,
}
```

Use cases:
- Prove "I'm authorized for this cell" without revealing which device
- Prove "I have required clearance" without revealing exact level
- Prove "I'm within operational area" without exact position

**Status**: Research/future. ZK proof generation overhead (~100ms+) limits real-time applicability. Consider for cell join authorization, not per-message auth.

## Open Questions

1. ~~Key rotation frequency~~ → **Resolved**: Configurable via `CellKeyConfig::rotation_interval`
2. ~~Partition recovery~~ → **Resolved**: Configurable via `CellKeyConfig::key_retention_window`
3. ~~Bypass encryption overhead~~ → **Resolved**: Per-collection `BypassAuthMode` configuration
4. ~~X.509 integration~~ → **Resolved**: Ed25519 first, X.509 via abstraction later

**Remaining questions for team:**
- Default rotation interval: 1 hour reasonable for tactical ops?
- Should `rotate_on_remove` be default true (more secure) or false (less churn)?

## References

- [RFC 9420: The Messaging Layer Security (MLS) Protocol](https://datatracker.ietf.org/doc/html/rfc9420)
- [OpenMLS Book](https://book.openmls.tech/)
- [OpenMLS GitHub](https://github.com/openmls/openmls)
- [mls-rs GitHub](https://github.com/awslabs/mls-rs)
- [Kerkour: CRDT + E2E Encryption Research Notes](https://kerkour.com/crdt-end-to-end-encryption-research-notes)
- [Martin Kleppmann: Secure Group Messaging for CRDTs](https://speakerdeck.com/ept/adapting-secure-group-messaging-for-encrypted-crdts)
