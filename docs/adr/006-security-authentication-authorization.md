# ADR-006: Security, Authentication, and Authorization for HIVE Protocol

**Status**: Proposed
**Date**: 2025-11-04
**Authors**: Claude, Kit Plummer
**Related**: ADR-005 (Data Sync Abstraction Layer), ADR-004 (Human-Machine Cell Composition)

## Context

HIVE Protocol coordinates autonomous platforms in tactical military environments where security failures can result in:
- **Loss of life** (compromised UAVs, corrupted mission data)
- **Mission failure** (adversary disruption of coordination)
- **Tactical disadvantage** (enemy intelligence gathering)
- **Friendly fire** (spoofed identity or commands)

Current implementation has **no authentication or authorization**. All nodes trust all peers, any node can join any squad, and all data is accessible to all participants. This is acceptable for proof-of-concept but **completely unacceptable** for tactical deployment.

### Threat Model

**Adversaries**:
1. **External attackers** - Enemy attempting to disrupt operations
2. **Compromised nodes** - Captured platforms running adversary code
3. **Insider threats** - Rogue operators or compromised credentials
4. **Network eavesdroppers** - Passive monitoring of communications

**Attack Vectors**:
1. **Identity spoofing** - Pretend to be a legitimate node to join squads
2. **Man-in-the-middle** - Intercept and modify messages between peers
3. **Replay attacks** - Retransmit captured messages to cause confusion
4. **Privilege escalation** - Node attempts to exceed its authorized role
5. **Data exfiltration** - Compromised node leaks tactical information
6. **Denial of service** - Flood network with invalid requests

### Security Requirements

HIVE Protocol must provide:

1. **Device Authentication** - Cryptographically verify device identity
2. **User Authentication** - Verify human operator credentials (for C2 apps)
3. **Application Authentication** - Verify software integrity and authorization
4. **Role-Based Authorization** - Enforce permissions based on role (Leader, Member, Observer)
5. **Hierarchical Authorization** - Enforce access control across organizational levels
6. **Data Confidentiality** - Encrypt all communications and storage
7. **Data Integrity** - Detect tampering with messages and documents
8. **Replay Protection** - Prevent reuse of captured messages
9. **Audit Trail** - Log all security-relevant events for forensics
10. **Graceful Degradation** - Continue operating if some security services fail

### Integration Points

Security must integrate with:
1. **Data Sync Layer** (ADR-005) - Authentication before sync, encrypted transport
2. **Cell Formation** (ADR-001) - Only authorized nodes join squads
3. **Human-in-the-Loop** (ADR-004) - Human operator authentication and approval
4. **Capability Advertisement** - Sign capability claims to prevent spoofing
5. **Hierarchical Aggregation** - Enforce data access by organizational level

## Decision

We will implement a **multi-layer security architecture** with:

### Layer 1: Device Identity and Authentication

Every device has a cryptographic identity verified before joining the mesh.

```rust
/// Device identity backed by PKI
pub struct DeviceIdentity {
    /// Unique device identifier (UUID)
    pub device_id: DeviceId,

    /// Public key for this device
    pub public_key: PublicKey,

    /// Certificate chain proving device authenticity
    pub certificates: Vec<X509Certificate>,

    /// Device type (UAV, ground vehicle, C2 station, etc.)
    pub device_type: DeviceType,

    /// Organizational unit (battalion, company, platoon)
    pub organization: OrganizationUnit,
}

/// Device authentication manager
pub struct DeviceAuthenticator {
    /// This device's identity
    own_identity: DeviceIdentity,

    /// Private key for signing
    private_key: PrivateKey,

    /// Trust store (root CAs, intermediate CAs)
    trust_store: TrustStore,

    /// Known peer identities (cached after first verification)
    peer_cache: Arc<RwLock<HashMap<DeviceId, DeviceIdentity>>>,
}

impl DeviceAuthenticator {
    /// Verify peer's identity during connection establishment
    pub async fn authenticate_peer(
        &self,
        peer_id: &DeviceId,
        challenge_response: &SignedChallenge,
    ) -> Result<DeviceIdentity> {
        // 1. Verify signature on challenge response
        let peer_pubkey = challenge_response.public_key;
        if !challenge_response.verify(&peer_pubkey)? {
            return Err(SecurityError::InvalidSignature);
        }

        // 2. Verify certificate chain
        let certs = &challenge_response.certificates;
        self.trust_store.verify_chain(certs)?;

        // 3. Check certificate validity (not expired, not revoked)
        for cert in certs {
            if cert.is_expired() {
                return Err(SecurityError::ExpiredCertificate);
            }
            if self.is_revoked(&cert)? {
                return Err(SecurityError::RevokedCertificate);
            }
        }

        // 4. Extract device identity from certificate
        let identity = DeviceIdentity::from_certificate(&certs[0])?;

        // 5. Cache for future use
        self.peer_cache.write().await.insert(*peer_id, identity.clone());

        Ok(identity)
    }

    /// Sign a message with this device's private key
    pub fn sign(&self, message: &[u8]) -> Result<Signature> {
        self.private_key.sign(message)
    }

    /// Generate a challenge for peer to prove identity
    pub fn generate_challenge(&self) -> Challenge {
        Challenge {
            nonce: random_bytes(32),
            timestamp: SystemTime::now(),
            challenger_id: self.own_identity.device_id,
        }
    }
}
```

### Layer 2: User Authentication (for Human Operators)

Human operators (C2 tablet, mission planning tools) authenticate separately from devices.

```rust
/// User identity for human operators
pub struct UserIdentity {
    /// Username (e.g., call sign)
    pub username: String,

    /// Full name and rank
    pub display_name: String,
    pub rank: MilitaryRank,

    /// Clearance level
    pub clearance: SecurityClearance,

    /// Organizational unit
    pub unit: OrganizationUnit,

    /// Roles (mission commander, operator, observer)
    pub roles: HashSet<UserRole>,
}

/// User authentication methods
pub enum AuthMethod {
    /// Password + TOTP (tactical environments)
    PasswordMFA { password_hash: PasswordHash, totp_secret: TotpSecret },

    /// CAC/PIV card (DoD standard)
    SmartCard { card_id: String, pin_hash: PasswordHash },

    /// Biometric (fingerprint, facial recognition)
    Biometric { biometric_template: Vec<u8> },

    /// Certificate-based (PKI)
    Certificate { certificate: X509Certificate },
}

/// User authentication manager
pub struct UserAuthenticator {
    /// User database (may be local or remote)
    user_store: Box<dyn UserStore>,

    /// Session manager (tracks logged-in users)
    sessions: Arc<RwLock<HashMap<SessionId, UserSession>>>,
}

impl UserAuthenticator {
    /// Authenticate user and create session
    pub async fn authenticate(
        &self,
        username: &str,
        credential: &Credential,
    ) -> Result<UserSession> {
        // 1. Lookup user
        let user = self.user_store
            .get_user(username)
            .await
            .ok_or(SecurityError::UserNotFound)?;

        // 2. Verify credential
        match (&user.auth_method, credential) {
            (AuthMethod::PasswordMFA { password_hash, totp_secret },
             Credential::PasswordMFA { password, totp_code }) => {
                // Verify password
                if !password_hash.verify(password)? {
                    return Err(SecurityError::InvalidCredential);
                }
                // Verify TOTP code
                if !totp_secret.verify(totp_code, SystemTime::now())? {
                    return Err(SecurityError::InvalidMFA);
                }
            }
            (AuthMethod::SmartCard { pin_hash, .. }, Credential::SmartCard { pin, .. }) => {
                if !pin_hash.verify(pin)? {
                    return Err(SecurityError::InvalidCredential);
                }
            }
            _ => return Err(SecurityError::UnsupportedAuthMethod),
        }

        // 3. Create session
        let session = UserSession {
            session_id: SessionId::new(),
            user_identity: user.identity,
            device_id: self.get_current_device_id(),
            created_at: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_hours(8),
        };

        // 4. Store session
        self.sessions.write().await.insert(session.session_id, session.clone());

        Ok(session)
    }

    /// Verify session is still valid
    pub async fn verify_session(&self, session_id: &SessionId) -> Result<UserSession> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or(SecurityError::InvalidSession)?;

        if session.expires_at < SystemTime::now() {
            return Err(SecurityError::SessionExpired);
        }

        Ok(session.clone())
    }
}
```

### Layer 3: Application Authentication

Verify that running software is authorized and unmodified.

```rust
/// Application identity (software being executed)
pub struct ApplicationIdentity {
    /// Application name and version
    pub app_name: String,
    pub version: semver::Version,

    /// Code signing certificate
    pub code_signature: CodeSignature,

    /// Hash of executable for integrity check
    pub executable_hash: Hash,

    /// Permissions this app is allowed to request
    pub declared_permissions: HashSet<Permission>,
}

/// Application authenticator using code signing
pub struct ApplicationAuthenticator {
    /// Trust store for code signing certificates
    code_signing_trust: TrustStore,

    /// Runtime integrity checker
    integrity_monitor: IntegrityMonitor,
}

impl ApplicationAuthenticator {
    /// Verify application integrity at startup
    pub fn verify_application(&self) -> Result<ApplicationIdentity> {
        // 1. Locate executable
        let exe_path = std::env::current_exe()?;

        // 2. Read executable and compute hash
        let exe_bytes = std::fs::read(&exe_path)?;
        let computed_hash = Hash::sha256(&exe_bytes);

        // 3. Extract embedded signature
        let signature = CodeSignature::extract_from_binary(&exe_bytes)?;

        // 4. Verify signature
        self.code_signing_trust.verify_code_signature(&signature)?;

        // 5. Check signature matches executable
        if signature.signed_hash != computed_hash {
            return Err(SecurityError::TamperedExecutable);
        }

        // 6. Extract identity from signature
        let identity = ApplicationIdentity::from_signature(&signature)?;

        // 7. Start runtime integrity monitoring
        self.integrity_monitor.start_monitoring()?;

        Ok(identity)
    }

    /// Periodic integrity check (detect runtime tampering)
    pub async fn check_runtime_integrity(&self) -> Result<()> {
        // Check for code injection, memory tampering, etc.
        self.integrity_monitor.check()?;
        Ok(())
    }
}
```

### Layer 4: Role-Based Authorization (RBAC)

Control what each authenticated entity can do.

```rust
/// Roles in HIVE Protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Squad/cell leader - can command cell, set objectives
    Leader,

    /// Squad/cell member - participates in missions
    Member,

    /// Observer - can view but not command
    Observer,

    /// Mission commander - can direct multiple cells
    Commander,

    /// Administrator - can configure system
    Admin,
}

/// Permissions that can be checked
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    // Cell operations
    JoinCell,
    LeaveCell,
    CreateCell,
    DisbandCell,
    SetCellLeader,
    SetCellObjective,

    // Capability operations
    AdvertiseCapability,
    RequestCapability,

    // Data access
    ReadCellState,
    WriteCellState,
    ReadNodeState,
    WriteNodeState,
    ReadTelemetry,

    // Hierarchical operations
    FormPlatoon,
    AggregateToCompany,

    // Human-in-the-loop
    ApproveFormation,
    VetoCommand,

    // Administration
    ConfigureNetwork,
    ManageKeys,
    ViewAuditLog,
}

/// Role-based authorization controller
pub struct AuthorizationController {
    /// Policy defining role → permissions mapping
    policy: AuthorizationPolicy,

    /// Audit logger for authorization decisions
    audit_log: AuditLogger,
}

impl AuthorizationController {
    /// Check if entity has permission
    pub fn check_permission(
        &self,
        entity: &AuthenticatedEntity,
        permission: Permission,
        context: &AuthorizationContext,
    ) -> Result<()> {
        // 1. Get roles for entity
        let roles = self.get_roles(entity, context)?;

        // 2. Check if any role grants permission
        let granted = roles.iter().any(|role| {
            self.policy.role_has_permission(*role, permission)
        });

        if !granted {
            // Log denial
            self.audit_log.log_denial(entity, permission, context);
            return Err(SecurityError::PermissionDenied {
                permission,
                entity_id: entity.id(),
            });
        }

        // 3. Log grant
        self.audit_log.log_grant(entity, permission, context);

        Ok(())
    }

    /// Get roles for entity in given context
    fn get_roles(
        &self,
        entity: &AuthenticatedEntity,
        context: &AuthorizationContext,
    ) -> Result<HashSet<Role>> {
        let mut roles = HashSet::new();

        match entity {
            AuthenticatedEntity::Device(device) => {
                // Devices get roles based on cell membership
                if let Some(cell_id) = context.cell_id {
                    let cell = context.get_cell(cell_id)?;

                    if cell.leader_id == Some(device.device_id.to_string()) {
                        roles.insert(Role::Leader);
                    } else if cell.members.contains(&device.device_id.to_string()) {
                        roles.insert(Role::Member);
                    } else {
                        roles.insert(Role::Observer);
                    }
                }
            }
            AuthenticatedEntity::User(user) => {
                // Users have explicit roles
                roles = user.identity.roles.clone();
            }
        }

        Ok(roles)
    }
}

/// Authorization context provides situational information
pub struct AuthorizationContext {
    /// Cell being accessed (if applicable)
    pub cell_id: Option<CellId>,

    /// Organizational level
    pub hierarchy_level: Option<HierarchyLevel>,

    /// Time of access
    pub timestamp: SystemTime,

    /// Access to data stores for context lookups
    pub cell_store: Arc<dyn CellStoreReader>,
}

/// Default authorization policy for HIVE Protocol
impl AuthorizationPolicy {
    pub fn default_policy() -> Self {
        let mut policy = AuthorizationPolicy::new();

        // Leader permissions
        policy.grant_role(Role::Leader, Permission::SetCellObjective);
        policy.grant_role(Role::Leader, Permission::SetCellLeader);
        policy.grant_role(Role::Leader, Permission::RequestCapability);
        policy.grant_role(Role::Leader, Permission::ReadCellState);
        policy.grant_role(Role::Leader, Permission::WriteCellState);

        // Member permissions
        policy.grant_role(Role::Member, Permission::JoinCell);
        policy.grant_role(Role::Member, Permission::LeaveCell);
        policy.grant_role(Role::Member, Permission::AdvertiseCapability);
        policy.grant_role(Role::Member, Permission::ReadCellState);
        policy.grant_role(Role::Member, Permission::WriteNodeState);

        // Observer permissions (read-only)
        policy.grant_role(Role::Observer, Permission::ReadCellState);
        policy.grant_role(Role::Observer, Permission::ReadNodeState);
        policy.grant_role(Role::Observer, Permission::ReadTelemetry);

        // Commander permissions (hierarchical)
        policy.grant_role(Role::Commander, Permission::FormPlatoon);
        policy.grant_role(Role::Commander, Permission::ApproveFormation);
        policy.grant_role(Role::Commander, Permission::VetoCommand);

        // Admin permissions (system-wide)
        policy.grant_role(Role::Admin, Permission::ConfigureNetwork);
        policy.grant_role(Role::Admin, Permission::ManageKeys);
        policy.grant_role(Role::Admin, Permission::ViewAuditLog);

        policy
    }
}
```

### Layer 5: Data Encryption

Encrypt all data in transit and at rest.

```rust
/// Encryption manager for HIVE Protocol
pub struct EncryptionManager {
    /// Device's encryption keypair
    keypair: EncryptionKeypair,

    /// Symmetric keys for peer-to-peer encryption
    peer_keys: Arc<RwLock<HashMap<PeerId, SymmetricKey>>>,

    /// Cell-level group keys for broadcast encryption
    cell_keys: Arc<RwLock<HashMap<CellId, GroupKey>>>,
}

impl EncryptionManager {
    /// Establish encrypted channel with peer
    pub async fn establish_secure_channel(
        &self,
        peer_id: &PeerId,
        peer_pubkey: &PublicKey,
    ) -> Result<SecureChannel> {
        // 1. Perform Diffie-Hellman key exchange
        let shared_secret = self.keypair.dh_exchange(peer_pubkey)?;

        // 2. Derive symmetric key using HKDF
        let symmetric_key = hkdf_derive(&shared_secret, b"hive-protocol-v1")?;

        // 3. Store key for this peer
        self.peer_keys.write().await.insert(*peer_id, symmetric_key.clone());

        // 4. Return secure channel
        Ok(SecureChannel {
            peer_id: *peer_id,
            symmetric_key,
            cipher: ChaCha20Poly1305::new(&symmetric_key),
        })
    }

    /// Encrypt document for storage
    pub fn encrypt_document(&self, document: &Document) -> Result<EncryptedDocument> {
        // Use device's own key for at-rest encryption
        let plaintext = serde_json::to_vec(document)?;
        let nonce = random_nonce();
        let ciphertext = self.keypair.encrypt(&plaintext, &nonce)?;

        Ok(EncryptedDocument {
            ciphertext,
            nonce,
            encrypted_by: self.keypair.public_key(),
        })
    }

    /// Encrypt message for cell broadcast
    pub async fn encrypt_for_cell(
        &self,
        cell_id: &CellId,
        message: &[u8],
    ) -> Result<EncryptedMessage> {
        // Get or create group key for cell
        let cell_keys = self.cell_keys.read().await;
        let group_key = cell_keys
            .get(cell_id)
            .ok_or(SecurityError::NoGroupKey)?;

        // Encrypt with group key
        let nonce = random_nonce();
        let ciphertext = group_key.encrypt(message, &nonce)?;

        Ok(EncryptedMessage {
            cell_id: *cell_id,
            ciphertext,
            nonce,
        })
    }

    /// Rotate cell group key (e.g., when member leaves)
    pub async fn rotate_cell_key(&self, cell_id: &CellId) -> Result<()> {
        // Generate new group key
        let new_key = GroupKey::generate();

        // Store new key
        self.cell_keys.write().await.insert(*cell_id, new_key.clone());

        // Distribute to all current cell members (encrypted per-peer)
        // This requires peer_keys to be established first

        Ok(())
    }
}
```

### Layer 6: Integration with Data Sync Abstraction

Security must integrate with the abstraction layer from ADR-005.

```rust
/// Extend DataSyncBackend trait with security
pub trait SecureDataSyncBackend: DataSyncBackend {
    /// Get security manager
    fn security(&self) -> &dyn SecurityManager;
}

/// Security manager trait
pub trait SecurityManager: Send + Sync {
    /// Authenticate a peer before allowing sync
    async fn authenticate_peer(&self, peer_id: &PeerId) -> Result<DeviceIdentity>;

    /// Authorize an operation
    fn authorize(
        &self,
        entity: &AuthenticatedEntity,
        permission: Permission,
        context: &AuthorizationContext,
    ) -> Result<()>;

    /// Encrypt data before sending
    fn encrypt(&self, data: &[u8], recipient: &PeerId) -> Result<Vec<u8>>;

    /// Decrypt data after receiving
    fn decrypt(&self, data: &[u8], sender: &PeerId) -> Result<Vec<u8>>;

    /// Get audit logger
    fn audit_log(&self) -> &dyn AuditLogger;
}

/// Secure wrapper for CellStore
impl<B: SecureDataSyncBackend> CellStore<B> {
    /// Store cell with authorization check
    pub async fn store_cell_secure(
        &self,
        cell: &CellState,
        entity: &AuthenticatedEntity,
    ) -> Result<String> {
        // 1. Check authorization
        let context = AuthorizationContext {
            cell_id: Some(CellId::from_str(&cell.config.id)?),
            hierarchy_level: Some(HierarchyLevel::Squad),
            timestamp: SystemTime::now(),
            cell_store: self.as_reader(),
        };

        self.backend.security().authorize(
            entity,
            Permission::WriteCellState,
            &context,
        )?;

        // 2. Store cell (encryption handled by backend)
        let doc_id = self.store_cell(cell).await?;

        // 3. Audit log
        self.backend.security().audit_log().log_operation(
            entity,
            "store_cell",
            &cell.config.id,
            true,
        );

        Ok(doc_id)
    }

    /// Set cell leader with authorization check
    pub async fn set_leader_secure(
        &self,
        cell_id: &str,
        leader_id: String,
        entity: &AuthenticatedEntity,
    ) -> Result<()> {
        // 1. Check authorization
        let context = AuthorizationContext {
            cell_id: Some(CellId::from_str(cell_id)?),
            hierarchy_level: Some(HierarchyLevel::Squad),
            timestamp: SystemTime::now(),
            cell_store: self.as_reader(),
        };

        self.backend.security().authorize(
            entity,
            Permission::SetCellLeader,
            &context,
        )?;

        // 2. Execute operation
        self.set_leader(cell_id, leader_id.clone()).await?;

        // 3. Audit log
        self.backend.security().audit_log().log_operation(
            entity,
            "set_leader",
            &format!("{} -> {}", cell_id, leader_id),
            true,
        );

        Ok(())
    }
}
```

### Layer 7: Audit Logging

Track all security-relevant events for forensics.

```rust
/// Audit logger for security events
pub trait AuditLogger: Send + Sync {
    /// Log authentication event
    fn log_authentication(
        &self,
        entity: &AuthenticatedEntity,
        success: bool,
        reason: Option<&str>,
    );

    /// Log authorization grant
    fn log_grant(
        &self,
        entity: &AuthenticatedEntity,
        permission: Permission,
        context: &AuthorizationContext,
    );

    /// Log authorization denial
    fn log_denial(
        &self,
        entity: &AuthenticatedEntity,
        permission: Permission,
        context: &AuthorizationContext,
    );

    /// Log operation execution
    fn log_operation(
        &self,
        entity: &AuthenticatedEntity,
        operation: &str,
        target: &str,
        success: bool,
    );

    /// Log security violation
    fn log_violation(
        &self,
        entity: &AuthenticatedEntity,
        violation_type: SecurityViolation,
        details: &str,
    );
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// Timestamp
    pub timestamp: SystemTime,

    /// Entity performing action
    pub entity: String,

    /// Event type
    pub event_type: AuditEventType,

    /// Success or failure
    pub success: bool,

    /// Details
    pub details: String,

    /// Context (cell ID, hierarchy level, etc.)
    pub context: HashMap<String, String>,
}

/// Audit event types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AuditEventType {
    Authentication,
    Authorization,
    DataAccess,
    DataModification,
    KeyExchange,
    CellFormation,
    LeaderElection,
    SecurityViolation,
}

/// File-based audit logger
pub struct FileAuditLogger {
    log_file: Arc<Mutex<File>>,
}

impl AuditLogger for FileAuditLogger {
    fn log_operation(
        &self,
        entity: &AuthenticatedEntity,
        operation: &str,
        target: &str,
        success: bool,
    ) {
        let entry = AuditLogEntry {
            timestamp: SystemTime::now(),
            entity: entity.id().to_string(),
            event_type: AuditEventType::DataModification,
            success,
            details: format!("{} on {}", operation, target),
            context: HashMap::new(),
        };

        // Write to log file (append-only)
        let mut file = self.log_file.lock().unwrap();
        writeln!(file, "{}", serde_json::to_string(&entry).unwrap()).ok();
        file.flush().ok();
    }

    // ... other methods
}
```

## Integration with HIVE Protocol Phases

### Phase 1: Discovery → Requires Device Authentication

```rust
// Before discovery, authenticate device
let device_auth = DeviceAuthenticator::new(config)?;
let device_identity = device_auth.verify_application()?;

// Discovery protocol includes signed beacon
let beacon = Beacon {
    device_id: device_identity.device_id,
    capabilities: my_capabilities,
    signature: device_auth.sign(&beacon_payload)?,
};

// Receiving node verifies beacon signature
if !peer_device_auth.verify_beacon(&beacon)? {
    warn!("Ignoring beacon from untrusted device");
    return;
}
```

### Phase 2: Cell Formation → Requires Authorization

```rust
// Human commander approves squad formation (ADR-004)
let user_auth = UserAuthenticator::new();
let user_session = user_auth.authenticate("commander_callsign", &credential).await?;

// Check authorization
let context = AuthorizationContext {
    cell_id: Some(proposed_cell_id),
    hierarchy_level: Some(HierarchyLevel::Squad),
    timestamp: SystemTime::now(),
    cell_store: cell_store.as_reader(),
};

authz.check_permission(
    &AuthenticatedEntity::User(user_session),
    Permission::ApproveFormation,
    &context,
)?;

// Form cell with encrypted group key
cell_store.store_cell_secure(&cell_state, &AuthenticatedEntity::User(user_session)).await?;
```

### Phase 3: Hierarchical Operations → Hierarchical Authorization

```rust
// Only commanders can aggregate cells into platoons
authz.check_permission(
    entity,
    Permission::FormPlatoon,
    &context,
)?;

// Platoon-level data only accessible to platoon members and above
let user = entity.as_user()?;
if !user.has_clearance_for_level(HierarchyLevel::Platoon) {
    return Err(SecurityError::InsufficientClearance);
}
```

## Deployment Scenarios

### Scenario 1: Tactical Edge (Fully Offline)

**Challenge**: No connection to PKI infrastructure or authentication servers

**Solution**:
- Pre-provision devices with certificates before deployment
- Use offline Certificate Revocation Lists (CRLs) synchronized during planning
- Local user database on mission commander's tablet
- Audit logs stored locally, uploaded post-mission

### Scenario 2: Contested Environment (Intermittent Connectivity)

**Challenge**: Network disruption, potential adversary interference

**Solution**:
- Use short-lived session tokens (8-hour expiry)
- Certificate stapling to reduce PKI dependencies
- Local authorization decisions with eventual consistency
- Cryptographic replay protection with time windows

### Scenario 3: Garrison/Training (Full Connectivity)

**Challenge**: Integration with existing DoD infrastructure

**Solution**:
- OCSP for real-time certificate validation
- CAC/PIV integration for user authentication
- Centralized audit log aggregation
- Integration with DoD PKI hierarchy

## Implementation Roadmap

### Phase 1: Foundation (Weeks 1-4)

- [ ] Define security traits and types
- [ ] Implement device identity and PKI verification
- [ ] Basic encryption (ChaCha20-Poly1305)
- [ ] File-based audit logging
- [ ] **Milestone**: Two devices can authenticate and establish encrypted channel

### Phase 2: Authorization (Weeks 5-8)

- [ ] Implement RBAC policy engine
- [ ] Authorization checks in CellStore/NodeStore
- [ ] Context-aware permission checking
- [ ] **Milestone**: Only authorized nodes can join cells and set leaders

### Phase 3: User Authentication (Weeks 9-12)

- [ ] Password + TOTP authentication
- [ ] Session management
- [ ] CAC/PIV integration (if available)
- [ ] **Milestone**: Human commanders can approve cell formations

### Phase 4: Advanced Features (Weeks 13-16)

- [ ] Group key management for cells
- [ ] Key rotation protocols
- [ ] Certificate revocation checking
- [ ] **Milestone**: Complete security for offline tactical deployment

## Security Best Practices

1. **Defense in Depth**: Multiple layers (device, user, app, network, data)
2. **Principle of Least Privilege**: Minimal permissions by default
3. **Zero Trust**: Verify every request, don't trust network position
4. **Fail Securely**: Deny access when in doubt
5. **Audit Everything**: Log all security-relevant events
6. **Graceful Degradation**: Continue operating if some security services fail

## Compliance Considerations

HIVE Protocol security must align with:

- **NIST SP 800-53** - Security and Privacy Controls for Information Systems
- **DoD 8500 Series** - Cybersecurity for DoD Information Systems
- **FIPS 140-2/3** - Cryptographic Module Validation (for tactical systems)
- **Common Criteria EAL** - Evaluation Assurance Level for security evaluation

## Open Questions

1. **How to handle certificate distribution in disconnected environments?**
   - Pre-provisioning before deployment?
   - Secure transfer via physical media?

2. **What's the certificate revocation strategy without network?**
   - Offline CRLs updated during mission planning?
   - Time-limited certificates with short validity?

3. **How to handle compromised devices in the field?**
   - Manual removal from trust store?
   - Automatic detection and isolation?

4. **Should we support different security levels for different data?**
   - Unclassified, Secret, Top Secret handling?
   - Multi-level security (MLS) architecture?

5. **How to handle human-in-the-loop approval latency?**
   - Timeout policies?
   - Automated fallback for time-critical scenarios?

## References

- [NIST SP 800-53](https://csrc.nist.gov/publications/detail/sp/800-53/rev-5/final) - Security Controls
- [DoD Zero Trust Reference Architecture](https://dodcio.defense.gov/Portals/0/Documents/Library/(U)ZT_RA_v2.0(U)_Sep22.pdf)
- [FIPS 140-3](https://csrc.nist.gov/publications/detail/fips/140/3/final) - Cryptographic Standards
- [RFC 5280](https://datatracker.ietf.org/doc/html/rfc5280) - X.509 PKI Certificate Profile
- [RFC 6749](https://datatracker.ietf.org/doc/html/rfc6749) - OAuth 2.0 Authorization Framework (adapted for military use)

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-11-04 | Proposed multi-layer security architecture | Comprehensive defense for tactical systems |
| TBD | Approved/Rejected | After team and security review |

---

**Next Steps**:
1. Security review by DoD cybersecurity experts
2. Threat modeling workshop with red team
3. Prototype device authentication with test PKI
4. Integrate with ADR-005 abstraction layer
5. Define cryptographic cipher suites and key sizes
