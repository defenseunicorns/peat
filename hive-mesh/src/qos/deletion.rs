//! Deletion Policy configuration for record deletion and tombstone management (ADR-034)
//!
//! This module addresses the CRDT deletion problem where deleted items can "resurrect"
//! when syncing with offline nodes. We use a hybrid approach with three strategies
//! based on data semantics.
//!
//! # Strategies
//!
//! ```text
//! ImplicitTTL:   beacons, platforms → Auto-superseded by newer, no tombstone
//! Tombstone:    tracks, nodes, alerts → Explicit delete with bounded retention
//! SoftDelete:   contact_reports, commands → Permanent audit trail
//! ```
//!
//! # Example
//!
//! ```
//! use hive_mesh::qos::{DeletionPolicy, DeletionPolicyRegistry};
//! use std::time::Duration;
//!
//! let registry = DeletionPolicyRegistry::with_defaults();
//!
//! // Beacons use implicit TTL (auto-superseded)
//! assert!(registry.get("beacons").is_implicit_ttl());
//!
//! // Commands use soft delete (audit trail)
//! assert!(registry.get("commands").is_soft_delete());
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, SystemTime};

// === Tombstone Sync Protocol Types (ADR-034 Phase 2, Issue #367) ===

/// Propagation direction for tombstones (ADR-034)
///
/// Controls which direction tombstones flow in the hierarchy:
/// - Bidirectional: Both up to parents and down to children
/// - UpOnly: Only to parent cells (e.g., contact_reports)
/// - DownOnly: Only to child cells (e.g., commands)
/// - SystemWide: Propagate to ALL peers (eventually consistent)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PropagationDirection {
    /// Sync bidirectionally (both up and down hierarchy)
    #[default]
    Bidirectional,
    /// Sync only upward to parent cells
    UpOnly,
    /// Sync only downward to child cells
    DownOnly,
    /// Sync to all peers regardless of hierarchy (eventually consistent)
    ///
    /// Use for security-critical deletions that must reach all nodes:
    /// - PII removal (GDPR/privacy compliance)
    /// - Malicious content removal
    /// - Security-revoked credentials
    SystemWide,
}

impl PropagationDirection {
    /// Default propagation direction for a collection
    ///
    /// Per ADR-034 strategy matrix:
    /// - nodes/tracks/alerts: Bidirectional
    /// - cells/contact_reports: Up only
    /// - commands: Down only
    pub fn default_for_collection(collection: &str) -> Self {
        match collection {
            "cells" | "contact_reports" => Self::UpOnly,
            "commands" => Self::DownOnly,
            _ => Self::Bidirectional,
        }
    }

    /// Check if this direction allows syncing to parent
    #[inline]
    pub fn allows_up(&self) -> bool {
        matches!(self, Self::Bidirectional | Self::UpOnly | Self::SystemWide)
    }

    /// Check if this direction allows syncing to children
    #[inline]
    pub fn allows_down(&self) -> bool {
        matches!(
            self,
            Self::Bidirectional | Self::DownOnly | Self::SystemWide
        )
    }

    /// Check if this is a system-wide propagation
    #[inline]
    pub fn is_system_wide(&self) -> bool {
        matches!(self, Self::SystemWide)
    }
}

/// Wire format message for tombstone sync (ADR-034 Phase 2)
///
/// Compact binary format for exchanging tombstones:
/// ```text
/// ┌────────────────┬──────────────┬──────────────┬─────────────┬─────────┬───────────┐
/// │ Collection Len │ Collection   │ Doc ID Len   │ Document ID │ Deleted │ Lamport   │
/// │ (2 bytes)      │ (var)        │ (2 bytes)    │ (var)       │ At (8)  │ (8 bytes) │
/// └────────────────┴──────────────┴──────────────┴─────────────┴─────────┴───────────┘
/// Optionally followed by:
/// ┌────────────────┬──────────────┬────────────────┬─────────────┐
/// │ Deleted By Len │ Deleted By   │ Reason Len     │ Reason      │
/// │ (2 bytes)      │ (var)        │ (2 bytes, 0=N) │ (var, opt)  │
/// └────────────────┴──────────────┴────────────────┴─────────────┘
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TombstoneSyncMessage {
    /// The tombstone being synced
    pub tombstone: Tombstone,
    /// Propagation direction (controls hierarchy flow)
    pub direction: PropagationDirection,
}

impl TombstoneSyncMessage {
    /// Create a new tombstone sync message
    pub fn new(tombstone: Tombstone, direction: PropagationDirection) -> Self {
        Self {
            tombstone,
            direction,
        }
    }

    /// Create from a tombstone with default direction for its collection
    pub fn from_tombstone(tombstone: Tombstone) -> Self {
        let direction = PropagationDirection::default_for_collection(&tombstone.collection);
        Self {
            tombstone,
            direction,
        }
    }

    /// Encode to wire format bytes
    ///
    /// Wire format:
    /// - collection_len (2 bytes, big-endian)
    /// - collection (var bytes)
    /// - doc_id_len (2 bytes, big-endian)
    /// - doc_id (var bytes)
    /// - deleted_at (8 bytes, big-endian, millis since epoch)
    /// - lamport (8 bytes, big-endian)
    /// - deleted_by_len (2 bytes, big-endian)
    /// - deleted_by (var bytes)
    /// - reason_len (2 bytes, big-endian, 0 if None)
    /// - reason (var bytes, optional)
    /// - direction (1 byte)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);

        // Collection
        let collection_bytes = self.tombstone.collection.as_bytes();
        buf.extend_from_slice(&(collection_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(collection_bytes);

        // Document ID
        let doc_id_bytes = self.tombstone.document_id.as_bytes();
        buf.extend_from_slice(&(doc_id_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(doc_id_bytes);

        // Deleted at (millis since epoch)
        let deleted_at_millis = self
            .tombstone
            .deleted_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        buf.extend_from_slice(&deleted_at_millis.to_be_bytes());

        // Lamport timestamp
        buf.extend_from_slice(&self.tombstone.lamport.to_be_bytes());

        // Deleted by
        let deleted_by_bytes = self.tombstone.deleted_by.as_bytes();
        buf.extend_from_slice(&(deleted_by_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(deleted_by_bytes);

        // Reason (optional)
        if let Some(reason) = &self.tombstone.reason {
            let reason_bytes = reason.as_bytes();
            buf.extend_from_slice(&(reason_bytes.len() as u16).to_be_bytes());
            buf.extend_from_slice(reason_bytes);
        } else {
            buf.extend_from_slice(&0u16.to_be_bytes());
        }

        // Direction
        buf.push(match self.direction {
            PropagationDirection::Bidirectional => 0x00,
            PropagationDirection::UpOnly => 0x01,
            PropagationDirection::DownOnly => 0x02,
            PropagationDirection::SystemWide => 0x03,
        });

        buf
    }

    /// Decode from wire format bytes
    pub fn decode(bytes: &[u8]) -> Result<Self, TombstoneDecodeError> {
        let mut pos = 0;

        // Collection
        if bytes.len() < pos + 2 {
            return Err(TombstoneDecodeError::TooShort);
        }
        let collection_len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;

        if bytes.len() < pos + collection_len {
            return Err(TombstoneDecodeError::TooShort);
        }
        let collection = String::from_utf8(bytes[pos..pos + collection_len].to_vec())
            .map_err(|_| TombstoneDecodeError::InvalidUtf8)?;
        pos += collection_len;

        // Document ID
        if bytes.len() < pos + 2 {
            return Err(TombstoneDecodeError::TooShort);
        }
        let doc_id_len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;

        if bytes.len() < pos + doc_id_len {
            return Err(TombstoneDecodeError::TooShort);
        }
        let document_id = String::from_utf8(bytes[pos..pos + doc_id_len].to_vec())
            .map_err(|_| TombstoneDecodeError::InvalidUtf8)?;
        pos += doc_id_len;

        // Deleted at
        if bytes.len() < pos + 8 {
            return Err(TombstoneDecodeError::TooShort);
        }
        let deleted_at_millis = u64::from_be_bytes([
            bytes[pos],
            bytes[pos + 1],
            bytes[pos + 2],
            bytes[pos + 3],
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]);
        let deleted_at =
            SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(deleted_at_millis);
        pos += 8;

        // Lamport
        if bytes.len() < pos + 8 {
            return Err(TombstoneDecodeError::TooShort);
        }
        let lamport = u64::from_be_bytes([
            bytes[pos],
            bytes[pos + 1],
            bytes[pos + 2],
            bytes[pos + 3],
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]);
        pos += 8;

        // Deleted by
        if bytes.len() < pos + 2 {
            return Err(TombstoneDecodeError::TooShort);
        }
        let deleted_by_len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;

        if bytes.len() < pos + deleted_by_len {
            return Err(TombstoneDecodeError::TooShort);
        }
        let deleted_by = String::from_utf8(bytes[pos..pos + deleted_by_len].to_vec())
            .map_err(|_| TombstoneDecodeError::InvalidUtf8)?;
        pos += deleted_by_len;

        // Reason (optional)
        if bytes.len() < pos + 2 {
            return Err(TombstoneDecodeError::TooShort);
        }
        let reason_len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;

        let reason = if reason_len > 0 {
            if bytes.len() < pos + reason_len {
                return Err(TombstoneDecodeError::TooShort);
            }
            let reason_str = String::from_utf8(bytes[pos..pos + reason_len].to_vec())
                .map_err(|_| TombstoneDecodeError::InvalidUtf8)?;
            pos += reason_len;
            Some(reason_str)
        } else {
            None
        };

        // Direction
        if bytes.len() < pos + 1 {
            return Err(TombstoneDecodeError::TooShort);
        }
        let direction = match bytes[pos] {
            0x00 => PropagationDirection::Bidirectional,
            0x01 => PropagationDirection::UpOnly,
            0x02 => PropagationDirection::DownOnly,
            0x03 => PropagationDirection::SystemWide,
            _ => return Err(TombstoneDecodeError::InvalidDirection),
        };

        Ok(Self {
            tombstone: Tombstone {
                document_id,
                collection,
                deleted_at,
                deleted_by,
                lamport,
                reason,
            },
            direction,
        })
    }
}

/// Error decoding a tombstone message
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TombstoneDecodeError {
    /// Message too short
    TooShort,
    /// Invalid UTF-8 string
    InvalidUtf8,
    /// Invalid propagation direction byte
    InvalidDirection,
}

impl std::fmt::Display for TombstoneDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "Tombstone message too short"),
            Self::InvalidUtf8 => write!(f, "Invalid UTF-8 in tombstone message"),
            Self::InvalidDirection => write!(f, "Invalid propagation direction byte"),
        }
    }
}

impl std::error::Error for TombstoneDecodeError {}

/// Batch of tombstones for sync exchange
///
/// Sent during peer connect to exchange all known tombstones.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TombstoneBatch {
    /// Tombstones in this batch
    pub tombstones: Vec<TombstoneSyncMessage>,
}

impl TombstoneBatch {
    /// Create a new empty batch
    pub fn new() -> Self {
        Self {
            tombstones: Vec::new(),
        }
    }

    /// Create a batch from tombstones
    pub fn from_tombstones(tombstones: Vec<Tombstone>) -> Self {
        Self {
            tombstones: tombstones
                .into_iter()
                .map(TombstoneSyncMessage::from_tombstone)
                .collect(),
        }
    }

    /// Create a batch from TombstoneSyncMessages directly
    pub fn with_messages(messages: Vec<TombstoneSyncMessage>) -> Self {
        Self {
            tombstones: messages,
        }
    }

    /// Add a tombstone to the batch
    pub fn push(&mut self, tombstone: TombstoneSyncMessage) {
        self.tombstones.push(tombstone);
    }

    /// Get the number of tombstones in the batch
    pub fn len(&self) -> usize {
        self.tombstones.len()
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.tombstones.is_empty()
    }

    /// Encode batch to wire format
    ///
    /// Format: [count (4 bytes)][tombstone 1][tombstone 2]...
    /// Each tombstone is: [len (4 bytes)][encoded tombstone bytes]
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.tombstones.len() * 64 + 4);

        // Count
        buf.extend_from_slice(&(self.tombstones.len() as u32).to_be_bytes());

        // Each tombstone with length prefix
        for tombstone in &self.tombstones {
            let encoded = tombstone.encode();
            buf.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
            buf.extend_from_slice(&encoded);
        }

        buf
    }

    /// Decode batch from wire format
    pub fn decode(bytes: &[u8]) -> Result<Self, TombstoneDecodeError> {
        if bytes.len() < 4 {
            return Err(TombstoneDecodeError::TooShort);
        }

        let count = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut pos = 4;
        let mut tombstones = Vec::with_capacity(count);

        for _ in 0..count {
            if bytes.len() < pos + 4 {
                return Err(TombstoneDecodeError::TooShort);
            }
            let len =
                u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
                    as usize;
            pos += 4;

            if bytes.len() < pos + len {
                return Err(TombstoneDecodeError::TooShort);
            }
            let tombstone = TombstoneSyncMessage::decode(&bytes[pos..pos + len])?;
            tombstones.push(tombstone);
            pos += len;
        }

        Ok(Self { tombstones })
    }
}

/// Deletion policy for a collection (ADR-034)
///
/// Determines how documents are deleted and whether tombstones are used.
#[derive(Debug, Clone, PartialEq)]
pub enum DeletionPolicy {
    /// No explicit deletion - documents superseded by newer versions
    ///
    /// Best for high-frequency position data (beacons, platforms).
    /// Documents older than TTL are garbage collected.
    ImplicitTTL {
        /// Maximum age before document is garbage collected
        ttl: Duration,
        /// Key field for supersession (e.g., "node_id" for beacons)
        supersession_key: Option<String>,
    },

    /// Explicit tombstones with bounded retention
    ///
    /// Best for data requiring explicit deletion but not permanent audit trail.
    /// Tombstones are garbage collected after TTL.
    Tombstone {
        /// How long to retain tombstones before garbage collection
        tombstone_ttl: Duration,
        /// Conflict resolution: true = delete wins over concurrent update
        delete_wins: bool,
    },

    /// Soft delete with permanent audit trail
    ///
    /// Best for audit-required data (contact_reports, commands).
    /// Documents are marked deleted but never removed.
    SoftDelete {
        /// Whether to include soft-deleted docs in queries by default
        include_deleted_default: bool,
    },

    /// No deletion allowed for this collection
    Immutable,
}

impl DeletionPolicy {
    /// Check if this policy uses implicit TTL
    #[inline]
    pub fn is_implicit_ttl(&self) -> bool {
        matches!(self, Self::ImplicitTTL { .. })
    }

    /// Check if this policy uses tombstones
    #[inline]
    pub fn is_tombstone(&self) -> bool {
        matches!(self, Self::Tombstone { .. })
    }

    /// Check if this policy uses soft delete
    #[inline]
    pub fn is_soft_delete(&self) -> bool {
        matches!(self, Self::SoftDelete { .. })
    }

    /// Check if this policy is immutable
    #[inline]
    pub fn is_immutable(&self) -> bool {
        matches!(self, Self::Immutable)
    }

    /// Get the TTL for implicit TTL policy
    pub fn implicit_ttl(&self) -> Option<Duration> {
        match self {
            Self::ImplicitTTL { ttl, .. } => Some(*ttl),
            _ => None,
        }
    }

    /// Get the tombstone TTL for tombstone policy
    pub fn tombstone_ttl(&self) -> Option<Duration> {
        match self {
            Self::Tombstone { tombstone_ttl, .. } => Some(*tombstone_ttl),
            _ => None,
        }
    }

    /// Check if delete wins over concurrent updates (for tombstone policy)
    pub fn delete_wins(&self) -> Option<bool> {
        match self {
            Self::Tombstone { delete_wins, .. } => Some(*delete_wins),
            _ => None,
        }
    }

    /// Default deletion policy for a collection name
    ///
    /// Returns appropriate defaults based on collection semantics:
    /// - beacons, platforms → ImplicitTTL (position data, superseded)
    /// - tracks → Tombstone with 1hr TTL
    /// - nodes, cells → Tombstone with 24hr TTL
    /// - contact_reports, commands, audit_logs → SoftDelete
    /// - alerts → Tombstone with 4hr TTL
    pub fn default_for_collection(collection: &str) -> Self {
        match collection {
            // Position/state data - auto-superseded by newer
            "beacons" | "platforms" => Self::ImplicitTTL {
                ttl: Duration::from_secs(3600), // 1 hour
                supersession_key: Some("node_id".to_string()),
            },

            // Tracks - explicit delete with short TTL
            "tracks" => Self::Tombstone {
                tombstone_ttl: Duration::from_secs(3600), // 1 hour
                delete_wins: true,
            },

            // Network membership - tombstone with longer TTL
            "nodes" | "cells" => Self::Tombstone {
                tombstone_ttl: Duration::from_secs(86400), // 24 hours
                delete_wins: true,
            },

            // Alerts - tombstone with medium TTL, update-wins
            "alerts" => Self::Tombstone {
                tombstone_ttl: Duration::from_secs(14400), // 4 hours
                delete_wins: false, // Update wins: alert update cancels delete
            },

            // Audit-required data - soft delete forever
            "contact_reports" | "commands" | "audit_logs" => Self::SoftDelete {
                include_deleted_default: false,
            },

            // Default: soft delete for safety
            _ => Self::SoftDelete {
                include_deleted_default: false,
            },
        }
    }
}

impl Default for DeletionPolicy {
    fn default() -> Self {
        Self::SoftDelete {
            include_deleted_default: false,
        }
    }
}

impl std::fmt::Display for DeletionPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplicitTTL { ttl, .. } => write!(f, "ImplicitTTL({}s)", ttl.as_secs()),
            Self::Tombstone { tombstone_ttl, .. } => {
                write!(f, "Tombstone({}s)", tombstone_ttl.as_secs())
            }
            Self::SoftDelete { .. } => write!(f, "SoftDelete"),
            Self::Immutable => write!(f, "Immutable"),
        }
    }
}

/// Tombstone record for deleted documents (ADR-034)
///
/// Represents a deletion marker that syncs alongside documents.
/// Tombstones have a TTL after which they are garbage collected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tombstone {
    /// ID of the deleted document
    pub document_id: String,
    /// Collection the document belonged to
    pub collection: String,
    /// When deletion occurred
    pub deleted_at: SystemTime,
    /// Node that initiated deletion
    pub deleted_by: String,
    /// Lamport timestamp for ordering
    pub lamport: u64,
    /// Optional reason for deletion
    pub reason: Option<String>,
}

impl Tombstone {
    /// Create a new tombstone
    pub fn new(
        document_id: impl Into<String>,
        collection: impl Into<String>,
        deleted_by: impl Into<String>,
        lamport: u64,
    ) -> Self {
        Self {
            document_id: document_id.into(),
            collection: collection.into(),
            deleted_at: SystemTime::now(),
            deleted_by: deleted_by.into(),
            lamport,
            reason: None,
        }
    }

    /// Create a tombstone with a reason
    pub fn with_reason(
        document_id: impl Into<String>,
        collection: impl Into<String>,
        deleted_by: impl Into<String>,
        lamport: u64,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            document_id: document_id.into(),
            collection: collection.into(),
            deleted_at: SystemTime::now(),
            deleted_by: deleted_by.into(),
            lamport,
            reason: Some(reason.into()),
        }
    }

    /// Check if this tombstone has expired based on TTL
    ///
    /// A tombstone is expired if its age is greater than or equal to the TTL.
    /// A TTL of zero means immediate expiration.
    pub fn is_expired(&self, ttl: Duration) -> bool {
        match SystemTime::now().duration_since(self.deleted_at) {
            Ok(age) => age >= ttl,
            Err(_) => false, // Clock went backwards, not expired
        }
    }

    /// Get the age of this tombstone
    pub fn age(&self) -> Option<Duration> {
        SystemTime::now().duration_since(self.deleted_at).ok()
    }

    /// Unique key for this tombstone
    pub fn key(&self) -> String {
        format!("{}:{}", self.collection, self.document_id)
    }
}

/// Result of a delete operation
#[derive(Debug, Clone)]
pub struct DeleteResult {
    /// Whether the document was deleted
    pub deleted: bool,
    /// Tombstone ID if one was created
    pub tombstone_id: Option<String>,
    /// When the tombstone expires (if applicable)
    pub expires_at: Option<SystemTime>,
    /// The deletion policy used
    pub policy: DeletionPolicy,
}

impl DeleteResult {
    /// Create a successful delete result with tombstone
    pub fn tombstoned(
        tombstone_id: String,
        expires_at: SystemTime,
        policy: DeletionPolicy,
    ) -> Self {
        Self {
            deleted: true,
            tombstone_id: Some(tombstone_id),
            expires_at: Some(expires_at),
            policy,
        }
    }

    /// Create a successful soft delete result
    pub fn soft_deleted(policy: DeletionPolicy) -> Self {
        Self {
            deleted: true,
            tombstone_id: None,
            expires_at: None,
            policy,
        }
    }

    /// Create a result for immutable collection (delete not allowed)
    pub fn immutable() -> Self {
        Self {
            deleted: false,
            tombstone_id: None,
            expires_at: None,
            policy: DeletionPolicy::Immutable,
        }
    }

    /// Create a result for document not found
    pub fn not_found(policy: DeletionPolicy) -> Self {
        Self {
            deleted: false,
            tombstone_id: None,
            expires_at: None,
            policy,
        }
    }
}

/// Registry for per-collection deletion policy configuration
///
/// Allows runtime configuration of deletion policies per collection,
/// with sensible defaults based on collection names.
#[derive(Debug, Default)]
pub struct DeletionPolicyRegistry {
    /// Per-collection deletion policy overrides
    overrides: RwLock<HashMap<String, DeletionPolicy>>,
}

impl DeletionPolicyRegistry {
    /// Create a new empty registry (uses defaults for all collections)
    pub fn new() -> Self {
        Self {
            overrides: RwLock::new(HashMap::new()),
        }
    }

    /// Create a registry with standard defaults pre-configured
    pub fn with_defaults() -> Self {
        let registry = Self::new();

        let defaults = [
            (
                "beacons",
                DeletionPolicy::ImplicitTTL {
                    ttl: Duration::from_secs(3600),
                    supersession_key: Some("node_id".to_string()),
                },
            ),
            (
                "platforms",
                DeletionPolicy::ImplicitTTL {
                    ttl: Duration::from_secs(3600),
                    supersession_key: Some("node_id".to_string()),
                },
            ),
            (
                "tracks",
                DeletionPolicy::Tombstone {
                    tombstone_ttl: Duration::from_secs(3600),
                    delete_wins: true,
                },
            ),
            (
                "nodes",
                DeletionPolicy::Tombstone {
                    tombstone_ttl: Duration::from_secs(86400),
                    delete_wins: true,
                },
            ),
            (
                "cells",
                DeletionPolicy::Tombstone {
                    tombstone_ttl: Duration::from_secs(86400),
                    delete_wins: true,
                },
            ),
            (
                "alerts",
                DeletionPolicy::Tombstone {
                    tombstone_ttl: Duration::from_secs(14400),
                    delete_wins: false,
                },
            ),
            (
                "contact_reports",
                DeletionPolicy::SoftDelete {
                    include_deleted_default: false,
                },
            ),
            (
                "commands",
                DeletionPolicy::SoftDelete {
                    include_deleted_default: false,
                },
            ),
            (
                "audit_logs",
                DeletionPolicy::SoftDelete {
                    include_deleted_default: false,
                },
            ),
        ];

        {
            let mut overrides = registry.overrides.write().unwrap();
            for (collection, policy) in defaults {
                overrides.insert(collection.to_string(), policy);
            }
        }

        registry
    }

    /// Get the deletion policy for a collection
    pub fn get(&self, collection: &str) -> DeletionPolicy {
        self.overrides
            .read()
            .unwrap()
            .get(collection)
            .cloned()
            .unwrap_or_else(|| DeletionPolicy::default_for_collection(collection))
    }

    /// Set the deletion policy for a collection
    pub fn set(&self, collection: &str, policy: DeletionPolicy) {
        self.overrides
            .write()
            .unwrap()
            .insert(collection.to_string(), policy);
    }

    /// Remove a collection override (will use default)
    pub fn remove(&self, collection: &str) -> Option<DeletionPolicy> {
        self.overrides.write().unwrap().remove(collection)
    }

    /// Check if deletion is allowed for a collection
    pub fn allows_delete(&self, collection: &str) -> bool {
        !self.get(collection).is_immutable()
    }

    /// Check if a collection uses tombstones
    pub fn uses_tombstones(&self, collection: &str) -> bool {
        self.get(collection).is_tombstone()
    }

    /// Check if a collection uses soft delete
    pub fn uses_soft_delete(&self, collection: &str) -> bool {
        self.get(collection).is_soft_delete()
    }
}

impl Clone for DeletionPolicyRegistry {
    fn clone(&self) -> Self {
        Self {
            overrides: RwLock::new(self.overrides.read().unwrap().clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deletion_policy_defaults() {
        // Position data should be ImplicitTTL
        assert!(DeletionPolicy::default_for_collection("beacons").is_implicit_ttl());
        assert!(DeletionPolicy::default_for_collection("platforms").is_implicit_ttl());

        // Tracks should be Tombstone
        assert!(DeletionPolicy::default_for_collection("tracks").is_tombstone());
        assert!(DeletionPolicy::default_for_collection("nodes").is_tombstone());

        // Audit data should be SoftDelete
        assert!(DeletionPolicy::default_for_collection("commands").is_soft_delete());
        assert!(DeletionPolicy::default_for_collection("contact_reports").is_soft_delete());

        // Unknown should default to SoftDelete
        assert!(DeletionPolicy::default_for_collection("unknown").is_soft_delete());
    }

    #[test]
    fn test_deletion_policy_accessors() {
        let implicit = DeletionPolicy::ImplicitTTL {
            ttl: Duration::from_secs(3600),
            supersession_key: Some("node_id".to_string()),
        };
        assert_eq!(implicit.implicit_ttl(), Some(Duration::from_secs(3600)));
        assert_eq!(implicit.tombstone_ttl(), None);

        let tombstone = DeletionPolicy::Tombstone {
            tombstone_ttl: Duration::from_secs(7200),
            delete_wins: true,
        };
        assert_eq!(tombstone.tombstone_ttl(), Some(Duration::from_secs(7200)));
        assert_eq!(tombstone.delete_wins(), Some(true));
        assert_eq!(tombstone.implicit_ttl(), None);
    }

    #[test]
    fn test_deletion_policy_display() {
        let implicit = DeletionPolicy::ImplicitTTL {
            ttl: Duration::from_secs(3600),
            supersession_key: None,
        };
        assert_eq!(implicit.to_string(), "ImplicitTTL(3600s)");

        let tombstone = DeletionPolicy::Tombstone {
            tombstone_ttl: Duration::from_secs(86400),
            delete_wins: true,
        };
        assert_eq!(tombstone.to_string(), "Tombstone(86400s)");

        assert_eq!(
            DeletionPolicy::SoftDelete {
                include_deleted_default: false
            }
            .to_string(),
            "SoftDelete"
        );
        assert_eq!(DeletionPolicy::Immutable.to_string(), "Immutable");
    }

    #[test]
    fn test_tombstone_creation() {
        let tombstone = Tombstone::new("doc-123", "tracks", "node-alpha", 42);

        assert_eq!(tombstone.document_id, "doc-123");
        assert_eq!(tombstone.collection, "tracks");
        assert_eq!(tombstone.deleted_by, "node-alpha");
        assert_eq!(tombstone.lamport, 42);
        assert!(tombstone.reason.is_none());
        assert_eq!(tombstone.key(), "tracks:doc-123");
    }

    #[test]
    fn test_tombstone_with_reason() {
        let tombstone =
            Tombstone::with_reason("doc-456", "alerts", "node-beta", 100, "User dismissed");

        assert_eq!(tombstone.reason, Some("User dismissed".to_string()));
    }

    #[test]
    fn test_tombstone_expiration() {
        let tombstone = Tombstone::new("doc-123", "tracks", "node-alpha", 42);

        // Should not be expired immediately with 1 hour TTL
        assert!(!tombstone.is_expired(Duration::from_secs(3600)));

        // Should be expired with 0 TTL
        assert!(tombstone.is_expired(Duration::ZERO));
    }

    #[test]
    fn test_tombstone_age() {
        let tombstone = Tombstone::new("doc-123", "tracks", "node-alpha", 42);

        // Age should be very small (just created)
        let age = tombstone.age().unwrap();
        assert!(age < Duration::from_secs(1));
    }

    #[test]
    fn test_delete_result() {
        let policy = DeletionPolicy::Tombstone {
            tombstone_ttl: Duration::from_secs(3600),
            delete_wins: true,
        };

        let result = DeleteResult::tombstoned(
            "tomb-123".to_string(),
            SystemTime::now() + Duration::from_secs(3600),
            policy.clone(),
        );
        assert!(result.deleted);
        assert_eq!(result.tombstone_id, Some("tomb-123".to_string()));
        assert!(result.expires_at.is_some());

        let soft = DeleteResult::soft_deleted(DeletionPolicy::SoftDelete {
            include_deleted_default: false,
        });
        assert!(soft.deleted);
        assert!(soft.tombstone_id.is_none());

        let immutable = DeleteResult::immutable();
        assert!(!immutable.deleted);
    }

    #[test]
    fn test_deletion_policy_registry() {
        let registry = DeletionPolicyRegistry::with_defaults();

        // Check defaults
        assert!(registry.get("beacons").is_implicit_ttl());
        assert!(registry.get("commands").is_soft_delete());
        assert!(registry.get("tracks").is_tombstone());

        // Override
        registry.set("beacons", DeletionPolicy::Immutable);
        assert!(registry.get("beacons").is_immutable());

        // Remove override
        registry.remove("beacons");
        assert!(registry.get("beacons").is_implicit_ttl());
    }

    #[test]
    fn test_deletion_policy_registry_helpers() {
        let registry = DeletionPolicyRegistry::with_defaults();

        assert!(registry.allows_delete("beacons"));
        assert!(registry.allows_delete("commands"));

        registry.set("special", DeletionPolicy::Immutable);
        assert!(!registry.allows_delete("special"));

        assert!(registry.uses_tombstones("tracks"));
        assert!(!registry.uses_tombstones("commands"));

        assert!(registry.uses_soft_delete("commands"));
        assert!(!registry.uses_soft_delete("tracks"));
    }

    #[test]
    fn test_tombstone_serialization() {
        let tombstone = Tombstone::with_reason("doc-123", "tracks", "node-alpha", 42, "Test");

        let json = serde_json::to_string(&tombstone).unwrap();
        let deserialized: Tombstone = serde_json::from_str(&json).unwrap();

        assert_eq!(tombstone.document_id, deserialized.document_id);
        assert_eq!(tombstone.collection, deserialized.collection);
        assert_eq!(tombstone.lamport, deserialized.lamport);
        assert_eq!(tombstone.reason, deserialized.reason);
    }

    // === Phase 2 Tests (Issue #367) ===

    #[test]
    fn test_propagation_direction_defaults() {
        // Bidirectional by default
        assert_eq!(
            PropagationDirection::default_for_collection("tracks"),
            PropagationDirection::Bidirectional
        );
        assert_eq!(
            PropagationDirection::default_for_collection("nodes"),
            PropagationDirection::Bidirectional
        );

        // Up-only for contact reports and cells
        assert_eq!(
            PropagationDirection::default_for_collection("contact_reports"),
            PropagationDirection::UpOnly
        );
        assert_eq!(
            PropagationDirection::default_for_collection("cells"),
            PropagationDirection::UpOnly
        );

        // Down-only for commands
        assert_eq!(
            PropagationDirection::default_for_collection("commands"),
            PropagationDirection::DownOnly
        );
    }

    #[test]
    fn test_propagation_direction_allows() {
        assert!(PropagationDirection::Bidirectional.allows_up());
        assert!(PropagationDirection::Bidirectional.allows_down());

        assert!(PropagationDirection::UpOnly.allows_up());
        assert!(!PropagationDirection::UpOnly.allows_down());

        assert!(!PropagationDirection::DownOnly.allows_up());
        assert!(PropagationDirection::DownOnly.allows_down());

        // SystemWide allows both
        assert!(PropagationDirection::SystemWide.allows_up());
        assert!(PropagationDirection::SystemWide.allows_down());
        assert!(PropagationDirection::SystemWide.is_system_wide());
    }

    #[test]
    fn test_tombstone_sync_message_encode_decode() {
        let tombstone = Tombstone::with_reason("doc-456", "alerts", "node-beta", 100, "Dismissed");
        let msg = TombstoneSyncMessage::new(tombstone, PropagationDirection::Bidirectional);

        let encoded = msg.encode();
        let decoded = TombstoneSyncMessage::decode(&encoded).unwrap();

        assert_eq!(msg.tombstone.document_id, decoded.tombstone.document_id);
        assert_eq!(msg.tombstone.collection, decoded.tombstone.collection);
        assert_eq!(msg.tombstone.deleted_by, decoded.tombstone.deleted_by);
        assert_eq!(msg.tombstone.lamport, decoded.tombstone.lamport);
        assert_eq!(msg.tombstone.reason, decoded.tombstone.reason);
        assert_eq!(msg.direction, decoded.direction);
    }

    #[test]
    fn test_tombstone_sync_message_no_reason() {
        let tombstone = Tombstone::new("doc-789", "tracks", "node-gamma", 50);
        let msg = TombstoneSyncMessage::new(tombstone, PropagationDirection::UpOnly);

        let encoded = msg.encode();
        let decoded = TombstoneSyncMessage::decode(&encoded).unwrap();

        assert!(decoded.tombstone.reason.is_none());
        assert_eq!(decoded.direction, PropagationDirection::UpOnly);
    }

    #[test]
    fn test_tombstone_sync_message_system_wide() {
        let tombstone = Tombstone::with_reason("pii-doc", "users", "admin", 999, "GDPR deletion");
        let msg = TombstoneSyncMessage::new(tombstone, PropagationDirection::SystemWide);

        let encoded = msg.encode();
        let decoded = TombstoneSyncMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.direction, PropagationDirection::SystemWide);
        assert!(decoded.direction.is_system_wide());
    }

    #[test]
    fn test_tombstone_sync_message_from_tombstone() {
        // from_tombstone uses default direction for collection
        let tombstone = Tombstone::new("doc-123", "commands", "node-delta", 75);
        let msg = TombstoneSyncMessage::from_tombstone(tombstone);

        // commands should default to DownOnly
        assert_eq!(msg.direction, PropagationDirection::DownOnly);
    }

    #[test]
    fn test_tombstone_batch_encode_decode() {
        let tombstones = vec![
            Tombstone::new("doc-1", "tracks", "node-a", 10),
            Tombstone::with_reason("doc-2", "alerts", "node-b", 20, "Expired"),
            Tombstone::new("doc-3", "nodes", "node-c", 30),
        ];

        let batch = TombstoneBatch::from_tombstones(tombstones);
        assert_eq!(batch.len(), 3);
        assert!(!batch.is_empty());

        let encoded = batch.encode();
        let decoded = TombstoneBatch::decode(&encoded).unwrap();

        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded.tombstones[0].tombstone.document_id, "doc-1");
        assert_eq!(decoded.tombstones[1].tombstone.document_id, "doc-2");
        assert_eq!(decoded.tombstones[2].tombstone.document_id, "doc-3");
    }

    #[test]
    fn test_tombstone_batch_empty() {
        let batch = TombstoneBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);

        let encoded = batch.encode();
        let decoded = TombstoneBatch::decode(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_tombstone_decode_error_too_short() {
        let result = TombstoneSyncMessage::decode(&[0x00]);
        assert_eq!(result.unwrap_err(), TombstoneDecodeError::TooShort);
    }

    #[test]
    fn test_tombstone_decode_error_invalid_direction() {
        // Create a valid tombstone, then corrupt the direction byte
        let tombstone = Tombstone::new("doc", "col", "node", 1);
        let msg = TombstoneSyncMessage::new(tombstone, PropagationDirection::Bidirectional);
        let mut encoded = msg.encode();

        // Corrupt the last byte (direction) to invalid value
        let len = encoded.len();
        encoded[len - 1] = 0xFF;

        let result = TombstoneSyncMessage::decode(&encoded);
        assert_eq!(result.unwrap_err(), TombstoneDecodeError::InvalidDirection);
    }
}
