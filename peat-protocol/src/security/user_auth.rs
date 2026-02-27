//! User Authentication for Human Operators.
//!
//! Implements ADR-006 Layer 2: User Authentication.
//!
//! # Overview
//!
//! This module provides authentication for human operators using C2 tablets
//! and mission planning tools. It supports:
//!
//! - Password + TOTP (Time-based One-Time Password) for tactical environments
//! - Session management with configurable expiry (default 8 hours)
//! - Local user database for offline operation
//!
//! # Security Properties
//!
//! - Passwords are stored using Argon2id (memory-hard, resistant to GPU attacks)
//! - TOTP uses HMAC-SHA256 with 6-digit codes
//! - Sessions are identified by cryptographically random UUIDs
//! - Failed login attempts are logged (not rate-limited in this implementation)
//!
//! # Example
//!
//! ```ignore
//! use peat_protocol::security::{UserAuthenticator, LocalUserStore, Credential};
//!
//! // Create authenticator with local user store
//! let store = LocalUserStore::new();
//! let authenticator = UserAuthenticator::new(Box::new(store));
//!
//! // Register a new user
//! authenticator.register_user(
//!     "alpha_6",
//!     "password123",
//!     UserIdentity {
//!         username: "alpha_6".to_string(),
//!         display_name: "CPT Smith".to_string(),
//!         rank: MilitaryRank::Captain,
//!         clearance: SecurityClearance::Secret,
//!         unit: OrganizationUnit::new("1st Plt", "A Co"),
//!         roles: HashSet::from([Role::Commander]),
//!     },
//! ).await?;
//!
//! // Authenticate user
//! let session = authenticator.authenticate(
//!     "alpha_6",
//!     &Credential::PasswordMfa {
//!         password: "password123".to_string(),
//!         totp_code: "123456".to_string(),
//!     },
//! ).await?;
//!
//! // Validate session
//! let identity = authenticator.validate_session(&session.session_id).await?;
//! ```

use super::authorization::Role;
use super::device_id::DeviceId;
use super::error::SecurityError;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use hmac::{Hmac, Mac};
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Default session expiry in hours.
pub const DEFAULT_SESSION_EXPIRY_HOURS: u64 = 8;

/// TOTP time step in seconds (standard is 30).
pub const TOTP_TIME_STEP_SECS: u64 = 30;

/// TOTP code length (standard is 6 digits).
pub const TOTP_CODE_LENGTH: usize = 6;

/// Number of time steps to allow for clock drift (1 step = 30 seconds).
pub const TOTP_CLOCK_DRIFT_STEPS: i64 = 1;

/// Military ranks (simplified for Peat Protocol).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MilitaryRank {
    // Enlisted
    Private,
    Specialist,
    Corporal,
    Sergeant,
    StaffSergeant,
    SergeantFirstClass,
    MasterSergeant,
    FirstSergeant,
    SergeantMajor,

    // Warrant Officers
    WarrantOfficer1,
    ChiefWarrantOfficer2,
    ChiefWarrantOfficer3,
    ChiefWarrantOfficer4,
    ChiefWarrantOfficer5,

    // Commissioned Officers
    SecondLieutenant,
    FirstLieutenant,
    Captain,
    Major,
    LieutenantColonel,
    Colonel,
    BrigadierGeneral,
    MajorGeneral,
    LieutenantGeneral,
    General,

    // Civilian
    Civilian,
}

impl std::fmt::Display for MilitaryRank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MilitaryRank::Private => write!(f, "PVT"),
            MilitaryRank::Specialist => write!(f, "SPC"),
            MilitaryRank::Corporal => write!(f, "CPL"),
            MilitaryRank::Sergeant => write!(f, "SGT"),
            MilitaryRank::StaffSergeant => write!(f, "SSG"),
            MilitaryRank::SergeantFirstClass => write!(f, "SFC"),
            MilitaryRank::MasterSergeant => write!(f, "MSG"),
            MilitaryRank::FirstSergeant => write!(f, "1SG"),
            MilitaryRank::SergeantMajor => write!(f, "SGM"),
            MilitaryRank::WarrantOfficer1 => write!(f, "WO1"),
            MilitaryRank::ChiefWarrantOfficer2 => write!(f, "CW2"),
            MilitaryRank::ChiefWarrantOfficer3 => write!(f, "CW3"),
            MilitaryRank::ChiefWarrantOfficer4 => write!(f, "CW4"),
            MilitaryRank::ChiefWarrantOfficer5 => write!(f, "CW5"),
            MilitaryRank::SecondLieutenant => write!(f, "2LT"),
            MilitaryRank::FirstLieutenant => write!(f, "1LT"),
            MilitaryRank::Captain => write!(f, "CPT"),
            MilitaryRank::Major => write!(f, "MAJ"),
            MilitaryRank::LieutenantColonel => write!(f, "LTC"),
            MilitaryRank::Colonel => write!(f, "COL"),
            MilitaryRank::BrigadierGeneral => write!(f, "BG"),
            MilitaryRank::MajorGeneral => write!(f, "MG"),
            MilitaryRank::LieutenantGeneral => write!(f, "LTG"),
            MilitaryRank::General => write!(f, "GEN"),
            MilitaryRank::Civilian => write!(f, "CIV"),
        }
    }
}

/// Security clearance levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SecurityClearance {
    /// Unclassified - no clearance required
    Unclassified,
    /// Controlled Unclassified Information
    Cui,
    /// Confidential clearance
    Confidential,
    /// Secret clearance
    Secret,
    /// Top Secret clearance
    TopSecret,
    /// Top Secret / Sensitive Compartmented Information
    TopSecretSci,
}

impl std::fmt::Display for SecurityClearance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityClearance::Unclassified => write!(f, "UNCLASSIFIED"),
            SecurityClearance::Cui => write!(f, "CUI"),
            SecurityClearance::Confidential => write!(f, "CONFIDENTIAL"),
            SecurityClearance::Secret => write!(f, "SECRET"),
            SecurityClearance::TopSecret => write!(f, "TOP SECRET"),
            SecurityClearance::TopSecretSci => write!(f, "TS/SCI"),
        }
    }
}

/// Organizational unit information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationUnit {
    /// Primary unit (e.g., "1st Platoon", "Alpha Company")
    pub name: String,

    /// Parent organization (e.g., "2nd Battalion", "1st Brigade")
    pub parent: Option<String>,

    /// Unit identifier code (UIC)
    pub uic: Option<String>,
}

impl OrganizationUnit {
    /// Create a new organization unit.
    pub fn new(name: impl Into<String>, parent: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parent: Some(parent.into()),
            uic: None,
        }
    }

    /// Create a top-level organization unit.
    pub fn top_level(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parent: None,
            uic: None,
        }
    }

    /// Set the Unit Identifier Code.
    pub fn with_uic(mut self, uic: impl Into<String>) -> Self {
        self.uic = Some(uic.into());
        self
    }
}

impl std::fmt::Display for OrganizationUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(parent) = &self.parent {
            write!(f, "{}, {}", self.name, parent)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

/// Complete user identity for human operators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIdentity {
    /// Username (e.g., call sign like "alpha_6")
    pub username: String,

    /// Display name (e.g., "CPT John Smith")
    pub display_name: String,

    /// Military rank
    pub rank: MilitaryRank,

    /// Security clearance level
    pub clearance: SecurityClearance,

    /// Organizational unit
    pub unit: OrganizationUnit,

    /// Roles for RBAC authorization
    pub roles: HashSet<Role>,
}

impl UserIdentity {
    /// Create a builder for UserIdentity.
    pub fn builder(username: impl Into<String>) -> UserIdentityBuilder {
        UserIdentityBuilder::new(username)
    }
}

/// Builder for UserIdentity.
pub struct UserIdentityBuilder {
    username: String,
    display_name: Option<String>,
    rank: MilitaryRank,
    clearance: SecurityClearance,
    unit: Option<OrganizationUnit>,
    roles: HashSet<Role>,
}

impl UserIdentityBuilder {
    /// Create a new builder.
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            display_name: None,
            rank: MilitaryRank::Civilian,
            clearance: SecurityClearance::Unclassified,
            unit: None,
            roles: HashSet::new(),
        }
    }

    /// Set the display name.
    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the rank.
    pub fn rank(mut self, rank: MilitaryRank) -> Self {
        self.rank = rank;
        self
    }

    /// Set the clearance.
    pub fn clearance(mut self, clearance: SecurityClearance) -> Self {
        self.clearance = clearance;
        self
    }

    /// Set the organizational unit.
    pub fn unit(mut self, unit: OrganizationUnit) -> Self {
        self.unit = Some(unit);
        self
    }

    /// Add a role.
    pub fn role(mut self, role: Role) -> Self {
        self.roles.insert(role);
        self
    }

    /// Add multiple roles.
    pub fn roles(mut self, roles: impl IntoIterator<Item = Role>) -> Self {
        self.roles.extend(roles);
        self
    }

    /// Build the UserIdentity.
    pub fn build(self) -> UserIdentity {
        UserIdentity {
            username: self.username.clone(),
            display_name: self.display_name.unwrap_or_else(|| self.username.clone()),
            rank: self.rank,
            clearance: self.clearance,
            unit: self
                .unit
                .unwrap_or_else(|| OrganizationUnit::top_level("Unknown")),
            roles: self.roles,
        }
    }
}

/// Stored user record with authentication credentials.
#[derive(Debug, Clone)]
pub struct UserRecord {
    /// User identity
    pub identity: UserIdentity,

    /// Authentication method
    pub auth_method: AuthMethod,

    /// Account status
    pub status: AccountStatus,

    /// Account creation time
    pub created_at: SystemTime,

    /// Last login time
    pub last_login: Option<SystemTime>,

    /// Failed login attempt count (reset on successful login)
    pub failed_attempts: u32,
}

/// Account status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    /// Account is active
    Active,
    /// Account is locked (too many failed attempts)
    Locked,
    /// Account is disabled by admin
    Disabled,
    /// Account is pending activation
    Pending,
}

/// Authentication methods supported.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// Password + TOTP (Time-based One-Time Password)
    PasswordMfa {
        /// Argon2id password hash
        password_hash: String,
        /// TOTP secret (base32 encoded)
        totp_secret: Vec<u8>,
    },

    /// Smart card (CAC/PIV) - placeholder for future implementation
    SmartCard {
        /// Card serial number
        card_id: String,
        /// PIN hash
        pin_hash: String,
    },

    /// Certificate-based authentication - placeholder for future implementation
    Certificate {
        /// X.509 certificate fingerprint
        certificate_fingerprint: String,
    },
}

/// Credential provided during authentication.
#[derive(Debug, Clone)]
pub enum Credential {
    /// Password + TOTP code
    PasswordMfa {
        /// Plain text password
        password: String,
        /// 6-digit TOTP code
        totp_code: String,
    },

    /// Smart card PIN
    SmartCard {
        /// Card serial number
        card_id: String,
        /// PIN
        pin: String,
    },

    /// Certificate-based (certificate presented during TLS handshake)
    Certificate {
        /// Certificate fingerprint
        fingerprint: String,
    },
}

/// User session token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    /// Unique session identifier
    pub session_id: SessionId,

    /// Authenticated user identity
    pub identity: UserIdentity,

    /// Device this session is bound to (if any)
    pub device_id: Option<DeviceId>,

    /// Session creation time
    pub created_at: SystemTime,

    /// Session expiration time
    pub expires_at: SystemTime,
}

impl UserSession {
    /// Check if the session is expired.
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires_at
    }

    /// Get remaining session time.
    pub fn remaining_time(&self) -> Option<Duration> {
        self.expires_at.duration_since(SystemTime::now()).ok()
    }
}

/// Session identifier (UUID v4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl SessionId {
    /// Generate a new random session ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// User store trait for database abstraction.
pub trait UserStore: Send + Sync {
    /// Get a user by username.
    fn get_user(&self, username: &str) -> Option<UserRecord>;

    /// Store a user record.
    fn store_user(&self, record: UserRecord) -> Result<(), SecurityError>;

    /// Update a user record.
    fn update_user(&self, record: UserRecord) -> Result<(), SecurityError>;

    /// Delete a user.
    fn delete_user(&self, username: &str) -> Result<(), SecurityError>;

    /// List all usernames.
    fn list_users(&self) -> Vec<String>;
}

/// Local in-memory user store for offline tactical use.
#[derive(Debug, Default)]
pub struct LocalUserStore {
    users: RwLock<HashMap<String, UserRecord>>,
}

impl LocalUserStore {
    /// Create a new empty local user store.
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
        }
    }

    /// Create a local user store with pre-provisioned users.
    pub fn with_users(users: Vec<UserRecord>) -> Self {
        let store = Self::new();
        {
            let mut map = store.users.write().unwrap();
            for user in users {
                map.insert(user.identity.username.clone(), user);
            }
        }
        store
    }
}

impl UserStore for LocalUserStore {
    fn get_user(&self, username: &str) -> Option<UserRecord> {
        self.users.read().unwrap().get(username).cloned()
    }

    fn store_user(&self, record: UserRecord) -> Result<(), SecurityError> {
        let mut users = self.users.write().unwrap();
        if users.contains_key(&record.identity.username) {
            return Err(SecurityError::UserAlreadyExists {
                username: record.identity.username,
            });
        }
        users.insert(record.identity.username.clone(), record);
        Ok(())
    }

    fn update_user(&self, record: UserRecord) -> Result<(), SecurityError> {
        let mut users = self.users.write().unwrap();
        if !users.contains_key(&record.identity.username) {
            return Err(SecurityError::UserNotFound {
                username: record.identity.username,
            });
        }
        users.insert(record.identity.username.clone(), record);
        Ok(())
    }

    fn delete_user(&self, username: &str) -> Result<(), SecurityError> {
        let mut users = self.users.write().unwrap();
        if users.remove(username).is_none() {
            return Err(SecurityError::UserNotFound {
                username: username.to_string(),
            });
        }
        Ok(())
    }

    fn list_users(&self) -> Vec<String> {
        self.users.read().unwrap().keys().cloned().collect()
    }
}

/// User authentication manager.
pub struct UserAuthenticator {
    /// User database
    user_store: Box<dyn UserStore>,

    /// Active sessions
    sessions: Arc<RwLock<HashMap<SessionId, UserSession>>>,

    /// Session expiry duration
    session_expiry: Duration,

    /// Maximum failed login attempts before lockout
    max_failed_attempts: u32,
}

impl UserAuthenticator {
    /// Create a new user authenticator.
    pub fn new(user_store: Box<dyn UserStore>) -> Self {
        Self {
            user_store,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_expiry: Duration::from_secs(DEFAULT_SESSION_EXPIRY_HOURS * 3600),
            max_failed_attempts: 5,
        }
    }

    /// Create authenticator with custom session expiry.
    pub fn with_session_expiry(mut self, expiry: Duration) -> Self {
        self.session_expiry = expiry;
        self
    }

    /// Create authenticator with custom max failed attempts.
    pub fn with_max_failed_attempts(mut self, max: u32) -> Self {
        self.max_failed_attempts = max;
        self
    }

    /// Register a new user with Password + TOTP authentication.
    ///
    /// Returns the TOTP secret (base32 encoded) for the user to configure their authenticator app.
    /// The `username` parameter is used as the lookup key - it should match identity.username.
    pub fn register_user(
        &self,
        _username: &str,
        password: &str,
        identity: UserIdentity,
    ) -> Result<String, SecurityError> {
        // Hash password with Argon2id
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| SecurityError::PasswordHashError {
                message: e.to_string(),
            })?
            .to_string();

        // Generate TOTP secret (20 bytes = 160 bits, standard for TOTP)
        let totp_secret: Vec<u8> = (0..20).map(|_| rand_core::OsRng.next_u64() as u8).collect();

        // Base32 encode the secret for display
        let totp_secret_b32 = base32_encode(&totp_secret);

        let record = UserRecord {
            identity,
            auth_method: AuthMethod::PasswordMfa {
                password_hash,
                totp_secret,
            },
            status: AccountStatus::Active,
            created_at: SystemTime::now(),
            last_login: None,
            failed_attempts: 0,
        };

        self.user_store.store_user(record)?;

        Ok(totp_secret_b32)
    }

    /// Authenticate a user and create a session.
    pub fn authenticate(
        &self,
        username: &str,
        credential: &Credential,
    ) -> Result<UserSession, SecurityError> {
        // Look up user
        let mut user =
            self.user_store
                .get_user(username)
                .ok_or_else(|| SecurityError::UserNotFound {
                    username: username.to_string(),
                })?;

        // Check account status
        match user.status {
            AccountStatus::Locked => {
                return Err(SecurityError::AccountLocked {
                    username: username.to_string(),
                })
            }
            AccountStatus::Disabled => {
                return Err(SecurityError::AccountDisabled {
                    username: username.to_string(),
                })
            }
            AccountStatus::Pending => {
                return Err(SecurityError::AccountPending {
                    username: username.to_string(),
                })
            }
            AccountStatus::Active => {}
        }

        // Verify credentials
        let verified = self.verify_credentials(&user.auth_method, credential)?;

        if !verified {
            // Increment failed attempts
            user.failed_attempts += 1;
            if user.failed_attempts >= self.max_failed_attempts {
                user.status = AccountStatus::Locked;
            }
            let _ = self.user_store.update_user(user);

            return Err(SecurityError::InvalidCredential {
                username: username.to_string(),
            });
        }

        // Reset failed attempts and update last login
        user.failed_attempts = 0;
        user.last_login = Some(SystemTime::now());
        let _ = self.user_store.update_user(user.clone());

        // Create session
        let now = SystemTime::now();
        let session = UserSession {
            session_id: SessionId::new(),
            identity: user.identity,
            device_id: None,
            created_at: now,
            expires_at: now + self.session_expiry,
        };

        // Store session
        self.sessions
            .write()
            .unwrap()
            .insert(session.session_id, session.clone());

        Ok(session)
    }

    /// Authenticate without TOTP (for testing or initial setup).
    ///
    /// WARNING: This should only be used in development/testing or when TOTP
    /// is being initially configured.
    pub fn authenticate_password_only(
        &self,
        username: &str,
        password: &str,
    ) -> Result<UserSession, SecurityError> {
        // Look up user
        let mut user =
            self.user_store
                .get_user(username)
                .ok_or_else(|| SecurityError::UserNotFound {
                    username: username.to_string(),
                })?;

        // Check account status
        match user.status {
            AccountStatus::Locked => {
                return Err(SecurityError::AccountLocked {
                    username: username.to_string(),
                })
            }
            AccountStatus::Disabled => {
                return Err(SecurityError::AccountDisabled {
                    username: username.to_string(),
                })
            }
            AccountStatus::Pending => {
                return Err(SecurityError::AccountPending {
                    username: username.to_string(),
                })
            }
            AccountStatus::Active => {}
        }

        // Verify password only
        match &user.auth_method {
            AuthMethod::PasswordMfa { password_hash, .. } => {
                let parsed_hash = PasswordHash::new(password_hash).map_err(|e| {
                    SecurityError::PasswordHashError {
                        message: e.to_string(),
                    }
                })?;

                if Argon2::default()
                    .verify_password(password.as_bytes(), &parsed_hash)
                    .is_err()
                {
                    user.failed_attempts += 1;
                    if user.failed_attempts >= self.max_failed_attempts {
                        user.status = AccountStatus::Locked;
                    }
                    let _ = self.user_store.update_user(user);

                    return Err(SecurityError::InvalidCredential {
                        username: username.to_string(),
                    });
                }
            }
            _ => {
                return Err(SecurityError::UnsupportedAuthMethod {
                    method: "non-password".to_string(),
                })
            }
        }

        // Reset failed attempts and update last login
        user.failed_attempts = 0;
        user.last_login = Some(SystemTime::now());
        let _ = self.user_store.update_user(user.clone());

        // Create session
        let now = SystemTime::now();
        let session = UserSession {
            session_id: SessionId::new(),
            identity: user.identity,
            device_id: None,
            created_at: now,
            expires_at: now + self.session_expiry,
        };

        // Store session
        self.sessions
            .write()
            .unwrap()
            .insert(session.session_id, session.clone());

        Ok(session)
    }

    /// Validate a session and return the user identity.
    pub fn validate_session(&self, session_id: &SessionId) -> Result<UserIdentity, SecurityError> {
        let sessions = self.sessions.read().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or(SecurityError::SessionNotFound)?;

        if session.is_expired() {
            drop(sessions);
            self.invalidate_session(session_id);
            return Err(SecurityError::SessionExpired);
        }

        Ok(session.identity.clone())
    }

    /// Get a session by ID.
    pub fn get_session(&self, session_id: &SessionId) -> Option<UserSession> {
        self.sessions.read().unwrap().get(session_id).cloned()
    }

    /// Invalidate (logout) a session.
    pub fn invalidate_session(&self, session_id: &SessionId) {
        self.sessions.write().unwrap().remove(session_id);
    }

    /// Invalidate all sessions for a user.
    pub fn invalidate_user_sessions(&self, username: &str) {
        let mut sessions = self.sessions.write().unwrap();
        sessions.retain(|_, session| session.identity.username != username);
    }

    /// Clean up expired sessions.
    pub fn cleanup_expired_sessions(&self) {
        let now = SystemTime::now();
        let mut sessions = self.sessions.write().unwrap();
        sessions.retain(|_, session| session.expires_at > now);
    }

    /// Get count of active sessions.
    pub fn active_session_count(&self) -> usize {
        self.sessions.read().unwrap().len()
    }

    /// Bind a session to a device.
    pub fn bind_session_to_device(
        &self,
        session_id: &SessionId,
        device_id: DeviceId,
    ) -> Result<(), SecurityError> {
        let mut sessions = self.sessions.write().unwrap();
        let session = sessions
            .get_mut(session_id)
            .ok_or(SecurityError::SessionNotFound)?;
        session.device_id = Some(device_id);
        Ok(())
    }

    /// Verify credentials against the stored auth method.
    fn verify_credentials(
        &self,
        auth_method: &AuthMethod,
        credential: &Credential,
    ) -> Result<bool, SecurityError> {
        match (auth_method, credential) {
            (
                AuthMethod::PasswordMfa {
                    password_hash,
                    totp_secret,
                },
                Credential::PasswordMfa {
                    password,
                    totp_code,
                },
            ) => {
                // Verify password
                let parsed_hash = PasswordHash::new(password_hash).map_err(|e| {
                    SecurityError::PasswordHashError {
                        message: e.to_string(),
                    }
                })?;

                if Argon2::default()
                    .verify_password(password.as_bytes(), &parsed_hash)
                    .is_err()
                {
                    return Ok(false);
                }

                // Verify TOTP
                if !verify_totp(totp_secret, totp_code)? {
                    return Err(SecurityError::InvalidMfaCode);
                }

                Ok(true)
            }

            (
                AuthMethod::SmartCard {
                    card_id: stored_id,
                    pin_hash,
                },
                Credential::SmartCard { card_id, pin },
            ) => {
                // Verify card ID matches
                if stored_id != card_id {
                    return Ok(false);
                }

                // Verify PIN
                let parsed_hash =
                    PasswordHash::new(pin_hash).map_err(|e| SecurityError::PasswordHashError {
                        message: e.to_string(),
                    })?;

                Ok(Argon2::default()
                    .verify_password(pin.as_bytes(), &parsed_hash)
                    .is_ok())
            }

            (
                AuthMethod::Certificate {
                    certificate_fingerprint: stored_fp,
                },
                Credential::Certificate { fingerprint },
            ) => {
                // Compare certificate fingerprints
                Ok(stored_fp == fingerprint)
            }

            _ => Err(SecurityError::UnsupportedAuthMethod {
                method: "mismatched auth method and credential".to_string(),
            }),
        }
    }

    /// Unlock a locked account (admin operation).
    pub fn unlock_account(&self, username: &str) -> Result<(), SecurityError> {
        let mut user =
            self.user_store
                .get_user(username)
                .ok_or_else(|| SecurityError::UserNotFound {
                    username: username.to_string(),
                })?;

        user.status = AccountStatus::Active;
        user.failed_attempts = 0;
        self.user_store.update_user(user)
    }

    /// Disable an account (admin operation).
    pub fn disable_account(&self, username: &str) -> Result<(), SecurityError> {
        let mut user =
            self.user_store
                .get_user(username)
                .ok_or_else(|| SecurityError::UserNotFound {
                    username: username.to_string(),
                })?;

        user.status = AccountStatus::Disabled;
        self.user_store.update_user(user)?;

        // Also invalidate all sessions
        self.invalidate_user_sessions(username);
        Ok(())
    }

    /// Change a user's password.
    pub fn change_password(
        &self,
        user_name: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), SecurityError> {
        let mut user =
            self.user_store
                .get_user(user_name)
                .ok_or_else(|| SecurityError::UserNotFound {
                    username: user_name.to_string(),
                })?;

        // Verify old password
        match &user.auth_method {
            AuthMethod::PasswordMfa {
                password_hash,
                totp_secret,
            } => {
                let parsed_hash = PasswordHash::new(password_hash).map_err(|e| {
                    SecurityError::PasswordHashError {
                        message: e.to_string(),
                    }
                })?;

                if Argon2::default()
                    .verify_password(old_password.as_bytes(), &parsed_hash)
                    .is_err()
                {
                    return Err(SecurityError::InvalidCredential {
                        username: user_name.to_string(),
                    });
                }

                // Hash new password
                let salt = SaltString::generate(&mut OsRng);
                let argon2 = Argon2::default();
                let new_hash = argon2
                    .hash_password(new_password.as_bytes(), &salt)
                    .map_err(|e| SecurityError::PasswordHashError {
                        message: e.to_string(),
                    })?
                    .to_string();

                user.auth_method = AuthMethod::PasswordMfa {
                    password_hash: new_hash,
                    totp_secret: totp_secret.clone(),
                };
            }
            _ => {
                return Err(SecurityError::UnsupportedAuthMethod {
                    method: "non-password".to_string(),
                })
            }
        }

        self.user_store.update_user(user)?;

        // Invalidate all sessions after password change
        self.invalidate_user_sessions(user_name);
        Ok(())
    }
}

/// Generate a TOTP code for the current time.
#[allow(dead_code)]
pub fn generate_totp(secret: &[u8]) -> Result<String, SecurityError> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SecurityError::TotpError {
            message: "System time before UNIX epoch".to_string(),
        })?
        .as_secs();

    let counter = time / TOTP_TIME_STEP_SECS;
    generate_hotp(secret, counter)
}

/// Generate an HOTP code for a given counter.
fn generate_hotp(secret: &[u8], counter: u64) -> Result<String, SecurityError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).map_err(|e| SecurityError::TotpError {
        message: format!("HMAC error: {}", e),
    })?;

    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();

    // Dynamic truncation (RFC 4226)
    let offset = (result[result.len() - 1] & 0x0f) as usize;
    let binary = ((result[offset] & 0x7f) as u32) << 24
        | (result[offset + 1] as u32) << 16
        | (result[offset + 2] as u32) << 8
        | (result[offset + 3] as u32);

    let otp = binary % 10u32.pow(TOTP_CODE_LENGTH as u32);
    Ok(format!("{:0width$}", otp, width = TOTP_CODE_LENGTH))
}

/// Verify a TOTP code (allows for clock drift).
pub fn verify_totp(secret: &[u8], code: &str) -> Result<bool, SecurityError> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SecurityError::TotpError {
            message: "System time before UNIX epoch".to_string(),
        })?
        .as_secs();

    let counter = (time / TOTP_TIME_STEP_SECS) as i64;

    // Check current time step and adjacent steps for clock drift
    for offset in -TOTP_CLOCK_DRIFT_STEPS..=TOTP_CLOCK_DRIFT_STEPS {
        let check_counter = (counter + offset) as u64;
        let expected = generate_hotp(secret, check_counter)?;
        if constant_time_compare(code.as_bytes(), expected.as_bytes()) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

/// Base32 encode bytes (RFC 4648).
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    let mut result = String::new();
    let mut buffer: u64 = 0;
    let mut bits_left = 0;

    for &byte in data {
        buffer = (buffer << 8) | (byte as u64);
        bits_left += 8;

        while bits_left >= 5 {
            bits_left -= 5;
            let index = ((buffer >> bits_left) & 0x1f) as usize;
            result.push(ALPHABET[index] as char);
        }
    }

    if bits_left > 0 {
        let index = ((buffer << (5 - bits_left)) & 0x1f) as usize;
        result.push(ALPHABET[index] as char);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_military_rank_display() {
        assert_eq!(MilitaryRank::Captain.to_string(), "CPT");
        assert_eq!(MilitaryRank::Sergeant.to_string(), "SGT");
        assert_eq!(MilitaryRank::Colonel.to_string(), "COL");
    }

    #[test]
    fn test_security_clearance_ordering() {
        assert!(SecurityClearance::Secret > SecurityClearance::Confidential);
        assert!(SecurityClearance::TopSecret > SecurityClearance::Secret);
        assert!(SecurityClearance::TopSecretSci > SecurityClearance::TopSecret);
    }

    #[test]
    fn test_organization_unit() {
        let unit = OrganizationUnit::new("1st Platoon", "Alpha Company");
        assert_eq!(unit.to_string(), "1st Platoon, Alpha Company");

        let top = OrganizationUnit::top_level("Battalion HQ");
        assert_eq!(top.to_string(), "Battalion HQ");
    }

    #[test]
    fn test_user_identity_builder() {
        let identity = UserIdentity::builder("alpha_6")
            .display_name("CPT John Smith")
            .rank(MilitaryRank::Captain)
            .clearance(SecurityClearance::Secret)
            .unit(OrganizationUnit::new("1st Plt", "A Co"))
            .role(Role::Commander)
            .build();

        assert_eq!(identity.username, "alpha_6");
        assert_eq!(identity.rank, MilitaryRank::Captain);
        assert!(identity.roles.contains(&Role::Commander));
    }

    #[test]
    fn test_session_id_generation() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_local_user_store() {
        let store = LocalUserStore::new();

        let identity = UserIdentity::builder("test_user")
            .rank(MilitaryRank::Sergeant)
            .build();

        let record = UserRecord {
            identity,
            auth_method: AuthMethod::PasswordMfa {
                password_hash: "hash".to_string(),
                totp_secret: vec![1, 2, 3],
            },
            status: AccountStatus::Active,
            created_at: SystemTime::now(),
            last_login: None,
            failed_attempts: 0,
        };

        // Store user
        store.store_user(record.clone()).unwrap();

        // Retrieve user
        let retrieved = store.get_user("test_user").unwrap();
        assert_eq!(retrieved.identity.username, "test_user");

        // List users
        let users = store.list_users();
        assert_eq!(users.len(), 1);
        assert!(users.contains(&"test_user".to_string()));

        // Delete user
        store.delete_user("test_user").unwrap();
        assert!(store.get_user("test_user").is_none());
    }

    #[test]
    fn test_user_store_duplicate_prevention() {
        let store = LocalUserStore::new();

        let identity = UserIdentity::builder("test_user").build();
        let record = UserRecord {
            identity: identity.clone(),
            auth_method: AuthMethod::PasswordMfa {
                password_hash: "hash".to_string(),
                totp_secret: vec![1, 2, 3],
            },
            status: AccountStatus::Active,
            created_at: SystemTime::now(),
            last_login: None,
            failed_attempts: 0,
        };

        store.store_user(record.clone()).unwrap();

        // Attempt to store duplicate
        let result = store.store_user(record);
        assert!(matches!(
            result,
            Err(SecurityError::UserAlreadyExists { .. })
        ));
    }

    #[test]
    fn test_hotp_generation() {
        // Test vector from RFC 4226
        let secret = b"12345678901234567890";
        let code = generate_hotp(secret, 0).unwrap();
        assert_eq!(code.len(), 6);
    }

    #[test]
    fn test_totp_generation_and_verification() {
        let secret = b"test_secret_key_1234";

        // Generate a code
        let code = generate_totp(secret).unwrap();
        assert_eq!(code.len(), 6);

        // Verify the code
        assert!(verify_totp(secret, &code).unwrap());

        // Wrong code should fail
        assert!(!verify_totp(secret, "000000").unwrap());
    }

    #[test]
    fn test_base32_encode() {
        // Test vectors
        assert_eq!(base32_encode(b""), "");
        assert_eq!(base32_encode(b"f"), "MY");
        assert_eq!(base32_encode(b"fo"), "MZXQ");
        assert_eq!(base32_encode(b"foo"), "MZXW6");
        assert_eq!(base32_encode(b"foob"), "MZXW6YQ");
        assert_eq!(base32_encode(b"fooba"), "MZXW6YTB");
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
    }

    #[test]
    fn test_constant_time_compare() {
        assert!(constant_time_compare(b"abc", b"abc"));
        assert!(!constant_time_compare(b"abc", b"abd"));
        assert!(!constant_time_compare(b"abc", b"ab"));
    }

    #[test]
    fn test_user_registration_and_password_auth() {
        let store = LocalUserStore::new();
        let authenticator = UserAuthenticator::new(Box::new(store));

        let identity = UserIdentity::builder("test_commander")
            .display_name("MAJ Test")
            .rank(MilitaryRank::Major)
            .clearance(SecurityClearance::Secret)
            .role(Role::Commander)
            .build();

        // Register user
        let totp_secret = authenticator
            .register_user("test_commander", "secure_password_123", identity)
            .unwrap();

        assert!(!totp_secret.is_empty());

        // Password-only auth should work
        let session = authenticator
            .authenticate_password_only("test_commander", "secure_password_123")
            .unwrap();

        assert_eq!(session.identity.username, "test_commander");
        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_validation() {
        let store = LocalUserStore::new();
        let authenticator = UserAuthenticator::new(Box::new(store));

        let identity = UserIdentity::builder("session_test")
            .role(Role::Observer)
            .build();

        authenticator
            .register_user("session_test", "password123", identity)
            .unwrap();

        let session = authenticator
            .authenticate_password_only("session_test", "password123")
            .unwrap();

        // Validate session
        let validated = authenticator.validate_session(&session.session_id).unwrap();
        assert_eq!(validated.username, "session_test");

        // Invalidate session
        authenticator.invalidate_session(&session.session_id);

        // Should fail now
        assert!(matches!(
            authenticator.validate_session(&session.session_id),
            Err(SecurityError::SessionNotFound)
        ));
    }

    #[test]
    fn test_account_lockout() {
        let store = LocalUserStore::new();
        let authenticator = UserAuthenticator::new(Box::new(store)).with_max_failed_attempts(3);

        let identity = UserIdentity::builder("lockout_test").build();

        authenticator
            .register_user("lockout_test", "correct_password", identity)
            .unwrap();

        // Fail 3 times
        for _ in 0..3 {
            let _ = authenticator.authenticate_password_only("lockout_test", "wrong_password");
        }

        // Now even correct password should fail
        let result = authenticator.authenticate_password_only("lockout_test", "correct_password");
        assert!(matches!(result, Err(SecurityError::AccountLocked { .. })));

        // Unlock account
        authenticator.unlock_account("lockout_test").unwrap();

        // Should work now
        let session = authenticator
            .authenticate_password_only("lockout_test", "correct_password")
            .unwrap();
        assert_eq!(session.identity.username, "lockout_test");
    }

    #[test]
    fn test_password_change() {
        let store = LocalUserStore::new();
        let authenticator = UserAuthenticator::new(Box::new(store));

        let identity = UserIdentity::builder("pwd_change_test").build();

        authenticator
            .register_user("pwd_change_test", "old_password", identity)
            .unwrap();

        // Change password
        authenticator
            .change_password("pwd_change_test", "old_password", "new_password")
            .unwrap();

        // Old password should fail
        let result = authenticator.authenticate_password_only("pwd_change_test", "old_password");
        assert!(result.is_err());

        // New password should work
        let session = authenticator
            .authenticate_password_only("pwd_change_test", "new_password")
            .unwrap();
        assert_eq!(session.identity.username, "pwd_change_test");
    }

    #[test]
    fn test_session_count_and_cleanup() {
        let store = LocalUserStore::new();
        let authenticator =
            UserAuthenticator::new(Box::new(store)).with_session_expiry(Duration::from_millis(10)); // Very short expiry

        let identity = UserIdentity::builder("cleanup_test").build();

        authenticator
            .register_user("cleanup_test", "password", identity)
            .unwrap();

        // Create a session
        let _session = authenticator
            .authenticate_password_only("cleanup_test", "password")
            .unwrap();

        assert_eq!(authenticator.active_session_count(), 1);

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(20));

        // Cleanup should remove it
        authenticator.cleanup_expired_sessions();
        assert_eq!(authenticator.active_session_count(), 0);
    }

    #[test]
    fn test_disable_account() {
        let store = LocalUserStore::new();
        let authenticator = UserAuthenticator::new(Box::new(store));

        let identity = UserIdentity::builder("disable_test").build();

        authenticator
            .register_user("disable_test", "password", identity)
            .unwrap();

        // Create session
        let session = authenticator
            .authenticate_password_only("disable_test", "password")
            .unwrap();

        // Disable account
        authenticator.disable_account("disable_test").unwrap();

        // Session should be invalidated
        assert!(authenticator.validate_session(&session.session_id).is_err());

        // New login should fail
        let result = authenticator.authenticate_password_only("disable_test", "password");
        assert!(matches!(result, Err(SecurityError::AccountDisabled { .. })));
    }

    #[test]
    fn test_bind_session_to_device() {
        let store = LocalUserStore::new();
        let authenticator = UserAuthenticator::new(Box::new(store));

        let identity = UserIdentity::builder("device_bind_test").build();

        authenticator
            .register_user("device_bind_test", "password", identity)
            .unwrap();

        let session = authenticator
            .authenticate_password_only("device_bind_test", "password")
            .unwrap();

        assert!(session.device_id.is_none());

        // Bind to device
        let keypair = crate::security::DeviceKeypair::generate();
        let device_id = keypair.device_id();

        authenticator
            .bind_session_to_device(&session.session_id, device_id)
            .unwrap();

        // Verify binding
        let updated_session = authenticator.get_session(&session.session_id).unwrap();
        assert!(updated_session.device_id.is_some());
    }
}
