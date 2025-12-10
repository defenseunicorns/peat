//! Quality of Service (QoS) framework for data prioritization (ADR-019)
//!
//! This module provides the foundational QoS classification system for HIVE Protocol,
//! ensuring critical data (contact reports, commands) reaches commanders before routine
//! telemetry.
//!
//! # Architecture
//!
//! - **QoSClass**: 5-level priority classification (P1 Critical → P5 Bulk)
//! - **QoSPolicy**: Per-data-type policy with latency, TTL, retention parameters
//! - **QoSRegistry**: Maps HIVE data types to their QoS policies
//!
//! # Storage Management (Phase 4)
//!
//! - **RetentionPolicy**: Per-class retention with min/max retention times
//! - **QoSAwareStorage**: Storage tracking with eviction candidate selection
//! - **EvictionController**: Automated eviction with audit logging
//! - **LifecyclePolicy**: Combined QoS + TTL (ADR-016) decision making
//!
//! # Example
//!
//! ```
//! use hive_protocol::qos::{QoSClass, QoSPolicy, DataType, QoSRegistry};
//!
//! // Get default military QoS registry
//! let registry = QoSRegistry::default_military();
//!
//! // Classify a contact report
//! let class = registry.classify(DataType::ContactReport);
//! assert_eq!(class, QoSClass::Critical);
//!
//! // Get full policy with latency constraints
//! let policy = registry.get_policy(DataType::ContactReport);
//! assert_eq!(policy.max_latency_ms, Some(500));
//! ```
//!
//! # Priority Mapping
//!
//! | QoS Class | Description | Max Latency | Bandwidth |
//! |-----------|-------------|-------------|-----------|
//! | P1 Critical | Commands, Contact Reports | 500ms | 40% + preemptive |
//! | P2 High | Mission imagery, retasking | 5s | 30% |
//! | P3 Normal | Health status, capability changes | 60s | 20% |
//! | P4 Low | Position updates, heartbeats | 300s | 8% |
//! | P5 Bulk | Model updates, debug logs | None | 2% |
//!
//! # Storage Management Example
//!
//! ```
//! use hive_protocol::qos::{
//!     QoSClass,
//!     storage::{QoSAwareStorage, StoredDocument},
//!     retention::RetentionPolicies,
//! };
//! use std::sync::Arc;
//!
//! // Create storage manager with 1GB capacity
//! let storage = Arc::new(QoSAwareStorage::new(1024 * 1024 * 1024));
//!
//! // Register a document
//! storage.register_document(StoredDocument::new("doc-123", QoSClass::Normal, 1024));
//!
//! // Check storage pressure
//! let pressure = storage.storage_pressure();
//! ```

pub mod audit;
pub mod bandwidth;
pub mod classification;
pub mod context;
pub mod context_manager;
pub mod deletion;
pub mod eviction;
pub mod lifecycle;
pub mod preemption;
pub mod recovery;
pub mod registry;
pub mod retention;
pub mod storage;
pub mod sync_mode;
pub mod sync_queue;

use serde::{Deserialize, Serialize};
use std::fmt;

// Re-exports
pub use audit::{AuditAction, AuditEntry, AuditSummary, EvictionAuditLog};
pub use bandwidth::{
    BandwidthAllocation, BandwidthConfig, BandwidthPermit, BandwidthQuota, QuotaConfig,
};
pub use classification::DataType;
pub use context::{ContextProfile, MissionContext, QoSClassAdjustment};
pub use context_manager::{ContextChangeListener, ContextChangeLog, ContextManager};
pub use deletion::{
    DeleteResult, DeletionPolicy, DeletionPolicyRegistry, PropagationDirection, Tombstone,
    TombstoneBatch, TombstoneDecodeError, TombstoneSyncMessage,
};
pub use eviction::{EvictionConfig, EvictionController, EvictionResult};
pub use lifecycle::{
    make_lifecycle_decision, LifecycleDecision, LifecyclePolicies, LifecyclePolicy,
};
pub use preemption::{ActiveTransfer, PreemptionController, PreemptionStats, TransferId};
pub use recovery::{RecoveryStats, SyncRecovery, UpdateBatch};
pub use registry::QoSRegistry;
pub use retention::{RetentionPolicies, RetentionPolicy};
pub use storage::{
    ClassStorageMetrics, EvictionCandidate, QoSAwareStorage, StorageMetrics, StoredDocument,
};
pub use sync_mode::{SyncMode, SyncModeRegistry};
pub use sync_queue::{PendingSync, PrioritySyncQueue, QueueStats};

/// 5-level priority classification (ADR-019)
///
/// Critical messages are handled with highest priority and may preempt lower
/// priority transfers. The numeric values enable `PartialOrd` comparisons
/// where lower values indicate higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum QoSClass {
    /// P1: Mission-critical, immediate sync
    ///
    /// Contact reports, emergency alerts, abort commands.
    /// Never preempted, may preempt all lower priorities.
    Critical = 1,

    /// P2: Important, sync within seconds
    ///
    /// Target imagery, audio intercepts, mission retasking.
    /// May be preempted only by Critical.
    High = 2,

    /// P3: Standard operational data
    ///
    /// Health status, capability changes, formation updates.
    /// Default priority for most operational messages.
    #[default]
    #[serde(alias = "default")]
    Normal = 3,

    /// P4: Routine telemetry
    ///
    /// Position updates, heartbeats, sensor readings.
    /// May be delayed during bandwidth constraints.
    Low = 4,

    /// P5: Archival/historical data
    ///
    /// Model updates, debug logs, historical data.
    /// Background transfer, lowest priority.
    Bulk = 5,
}

impl QoSClass {
    /// Returns the numeric priority value (1-5)
    ///
    /// Lower values indicate higher priority.
    #[inline]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Returns true if this class can preempt the other class
    ///
    /// A class can preempt another if it has strictly higher priority (lower numeric value).
    #[inline]
    pub fn can_preempt(&self, other: &QoSClass) -> bool {
        self.as_u8() < other.as_u8()
    }

    /// Returns true if this is a critical priority message
    #[inline]
    pub fn is_critical(&self) -> bool {
        matches!(self, Self::Critical)
    }

    /// Returns the bandwidth allocation percentage for this class
    ///
    /// Based on ADR-019 bandwidth allocation table.
    pub fn bandwidth_allocation_percent(&self) -> u8 {
        match self {
            Self::Critical => 40,
            Self::High => 30,
            Self::Normal => 20,
            Self::Low => 8,
            Self::Bulk => 2,
        }
    }

    /// Returns all QoS classes in priority order (highest first)
    pub fn all_by_priority() -> &'static [QoSClass] {
        &[
            QoSClass::Critical,
            QoSClass::High,
            QoSClass::Normal,
            QoSClass::Low,
            QoSClass::Bulk,
        ]
    }
}

impl PartialOrd for QoSClass {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QoSClass {
    /// Lower numeric value = higher priority
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse the comparison so Critical (1) > Bulk (5) in priority ordering
        other.as_u8().cmp(&self.as_u8())
    }
}

impl fmt::Display for QoSClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Critical => write!(f, "P1:Critical"),
            Self::High => write!(f, "P2:High"),
            Self::Normal => write!(f, "P3:Normal"),
            Self::Low => write!(f, "P4:Low"),
            Self::Bulk => write!(f, "P5:Bulk"),
        }
    }
}

/// Per-data-type QoS policy
///
/// Defines the handling characteristics for a specific data type,
/// including latency constraints, time-to-live, and preemption behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QoSPolicy {
    /// Base priority class for this data type
    pub base_class: QoSClass,

    /// Maximum acceptable latency in milliseconds
    ///
    /// Messages exceeding this latency may trigger alerts or be dropped
    /// depending on the drop policy. `None` means no latency constraint.
    pub max_latency_ms: Option<u64>,

    /// Maximum message size in bytes
    ///
    /// Messages exceeding this size may be rejected or chunked.
    /// `None` means no size constraint.
    pub max_size_bytes: Option<usize>,

    /// Time-to-live in seconds
    ///
    /// Messages older than TTL may be automatically discarded.
    /// `None` means no TTL (persist indefinitely).
    pub ttl_seconds: Option<u64>,

    /// Retention priority (1-5, lower = evict first)
    ///
    /// Used by QoS-aware storage eviction. Priority 5 items are
    /// evicted last, priority 1 items evicted first.
    pub retention_priority: u8,

    /// Whether this data type can be preempted by higher priority data
    ///
    /// If false, transfer will complete even if higher priority data arrives.
    /// Critical data is typically non-preemptable.
    pub preemptable: bool,
}

impl Default for QoSPolicy {
    fn default() -> Self {
        Self {
            base_class: QoSClass::Normal,
            max_latency_ms: Some(60_000), // 60 seconds
            max_size_bytes: None,
            ttl_seconds: None,
            retention_priority: 3,
            preemptable: true,
        }
    }
}

impl QoSPolicy {
    /// Create a critical priority policy
    pub fn critical() -> Self {
        Self {
            base_class: QoSClass::Critical,
            max_latency_ms: Some(500),
            max_size_bytes: Some(64 * 1024), // 64KB max for critical
            ttl_seconds: None,               // Never expire
            retention_priority: 5,           // Never evict
            preemptable: false,
        }
    }

    /// Create a high priority policy
    pub fn high() -> Self {
        Self {
            base_class: QoSClass::High,
            max_latency_ms: Some(5_000),            // 5 seconds
            max_size_bytes: Some(10 * 1024 * 1024), // 10MB
            ttl_seconds: Some(3600),                // 1 hour
            retention_priority: 4,
            preemptable: true,
        }
    }

    /// Create a low priority policy
    pub fn low() -> Self {
        Self {
            base_class: QoSClass::Low,
            max_latency_ms: Some(300_000), // 5 minutes
            max_size_bytes: None,
            ttl_seconds: Some(86400), // 24 hours
            retention_priority: 2,
            preemptable: true,
        }
    }

    /// Create a bulk priority policy
    pub fn bulk() -> Self {
        Self {
            base_class: QoSClass::Bulk,
            max_latency_ms: None,      // No latency constraint
            max_size_bytes: None,      // No size limit
            ttl_seconds: Some(604800), // 1 week
            retention_priority: 1,
            preemptable: true,
        }
    }

    /// Check if a message with given latency exceeds the policy limit
    pub fn exceeds_latency(&self, latency_ms: u64) -> bool {
        self.max_latency_ms
            .map(|max| latency_ms > max)
            .unwrap_or(false)
    }

    /// Check if a message with given size exceeds the policy limit
    pub fn exceeds_size(&self, size_bytes: usize) -> bool {
        self.max_size_bytes
            .map(|max| size_bytes > max)
            .unwrap_or(false)
    }
}

// ============================================================================
// Conversions to/from existing priority types
// ============================================================================

use crate::cell::messaging::MessagePriority;
use crate::storage::file_distribution::TransferPriority;

impl From<QoSClass> for MessagePriority {
    fn from(qos: QoSClass) -> Self {
        match qos {
            QoSClass::Critical => MessagePriority::Critical,
            QoSClass::High => MessagePriority::High,
            QoSClass::Normal => MessagePriority::Normal,
            QoSClass::Low | QoSClass::Bulk => MessagePriority::Low,
        }
    }
}

impl From<MessagePriority> for QoSClass {
    fn from(priority: MessagePriority) -> Self {
        match priority {
            MessagePriority::Critical => QoSClass::Critical,
            MessagePriority::High => QoSClass::High,
            MessagePriority::Normal => QoSClass::Normal,
            MessagePriority::Low => QoSClass::Low,
        }
    }
}

impl From<QoSClass> for TransferPriority {
    fn from(qos: QoSClass) -> Self {
        match qos {
            QoSClass::Critical => TransferPriority::Critical,
            QoSClass::High => TransferPriority::High,
            QoSClass::Normal => TransferPriority::Normal,
            QoSClass::Low | QoSClass::Bulk => TransferPriority::Low,
        }
    }
}

impl From<TransferPriority> for QoSClass {
    fn from(priority: TransferPriority) -> Self {
        match priority {
            TransferPriority::Critical => QoSClass::Critical,
            TransferPriority::High => QoSClass::High,
            TransferPriority::Normal => QoSClass::Normal,
            TransferPriority::Low => QoSClass::Low,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qos_class_ordering() {
        // Critical > High > Normal > Low > Bulk
        assert!(QoSClass::Critical > QoSClass::High);
        assert!(QoSClass::High > QoSClass::Normal);
        assert!(QoSClass::Normal > QoSClass::Low);
        assert!(QoSClass::Low > QoSClass::Bulk);

        // Verify sorting puts critical first
        let mut classes = [
            QoSClass::Bulk,
            QoSClass::Critical,
            QoSClass::Low,
            QoSClass::High,
            QoSClass::Normal,
        ];
        classes.sort();
        classes.reverse(); // Sort is by priority, reverse to get highest first
        assert_eq!(classes[0], QoSClass::Critical);
        assert_eq!(classes[4], QoSClass::Bulk);
    }

    #[test]
    fn test_qos_class_can_preempt() {
        assert!(QoSClass::Critical.can_preempt(&QoSClass::High));
        assert!(QoSClass::Critical.can_preempt(&QoSClass::Bulk));
        assert!(QoSClass::High.can_preempt(&QoSClass::Normal));
        assert!(!QoSClass::Low.can_preempt(&QoSClass::High));
        assert!(!QoSClass::Normal.can_preempt(&QoSClass::Normal));
    }

    #[test]
    fn test_qos_class_bandwidth_allocation() {
        assert_eq!(QoSClass::Critical.bandwidth_allocation_percent(), 40);
        assert_eq!(QoSClass::High.bandwidth_allocation_percent(), 30);
        assert_eq!(QoSClass::Normal.bandwidth_allocation_percent(), 20);
        assert_eq!(QoSClass::Low.bandwidth_allocation_percent(), 8);
        assert_eq!(QoSClass::Bulk.bandwidth_allocation_percent(), 2);

        // Total should be 100%
        let total: u8 = QoSClass::all_by_priority()
            .iter()
            .map(|c| c.bandwidth_allocation_percent())
            .sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_qos_class_display() {
        assert_eq!(QoSClass::Critical.to_string(), "P1:Critical");
        assert_eq!(QoSClass::High.to_string(), "P2:High");
        assert_eq!(QoSClass::Normal.to_string(), "P3:Normal");
        assert_eq!(QoSClass::Low.to_string(), "P4:Low");
        assert_eq!(QoSClass::Bulk.to_string(), "P5:Bulk");
    }

    #[test]
    fn test_qos_policy_defaults() {
        let policy = QoSPolicy::default();
        assert_eq!(policy.base_class, QoSClass::Normal);
        assert_eq!(policy.max_latency_ms, Some(60_000));
        assert!(policy.preemptable);
    }

    #[test]
    fn test_qos_policy_critical() {
        let policy = QoSPolicy::critical();
        assert_eq!(policy.base_class, QoSClass::Critical);
        assert_eq!(policy.max_latency_ms, Some(500));
        assert!(!policy.preemptable);
        assert_eq!(policy.retention_priority, 5);
    }

    #[test]
    fn test_qos_policy_latency_check() {
        let policy = QoSPolicy::critical();
        assert!(!policy.exceeds_latency(400));
        assert!(policy.exceeds_latency(600));

        let bulk_policy = QoSPolicy::bulk();
        assert!(!bulk_policy.exceeds_latency(1_000_000)); // No limit
    }

    #[test]
    fn test_qos_policy_size_check() {
        let policy = QoSPolicy::critical();
        assert!(!policy.exceeds_size(60 * 1024));
        assert!(policy.exceeds_size(70 * 1024));

        let bulk_policy = QoSPolicy::bulk();
        assert!(!bulk_policy.exceeds_size(100_000_000)); // No limit
    }

    #[test]
    fn test_message_priority_conversion() {
        // QoSClass -> MessagePriority
        assert_eq!(
            MessagePriority::from(QoSClass::Critical),
            MessagePriority::Critical
        );
        assert_eq!(MessagePriority::from(QoSClass::High), MessagePriority::High);
        assert_eq!(
            MessagePriority::from(QoSClass::Normal),
            MessagePriority::Normal
        );
        assert_eq!(MessagePriority::from(QoSClass::Low), MessagePriority::Low);
        assert_eq!(MessagePriority::from(QoSClass::Bulk), MessagePriority::Low);

        // MessagePriority -> QoSClass
        assert_eq!(
            QoSClass::from(MessagePriority::Critical),
            QoSClass::Critical
        );
        assert_eq!(QoSClass::from(MessagePriority::High), QoSClass::High);
        assert_eq!(QoSClass::from(MessagePriority::Normal), QoSClass::Normal);
        assert_eq!(QoSClass::from(MessagePriority::Low), QoSClass::Low);
    }

    #[test]
    fn test_transfer_priority_conversion() {
        // QoSClass -> TransferPriority
        assert_eq!(
            TransferPriority::from(QoSClass::Critical),
            TransferPriority::Critical
        );
        assert_eq!(
            TransferPriority::from(QoSClass::High),
            TransferPriority::High
        );
        assert_eq!(
            TransferPriority::from(QoSClass::Normal),
            TransferPriority::Normal
        );
        assert_eq!(TransferPriority::from(QoSClass::Low), TransferPriority::Low);
        assert_eq!(
            TransferPriority::from(QoSClass::Bulk),
            TransferPriority::Low
        );

        // TransferPriority -> QoSClass
        assert_eq!(
            QoSClass::from(TransferPriority::Critical),
            QoSClass::Critical
        );
        assert_eq!(QoSClass::from(TransferPriority::High), QoSClass::High);
        assert_eq!(QoSClass::from(TransferPriority::Normal), QoSClass::Normal);
        assert_eq!(QoSClass::from(TransferPriority::Low), QoSClass::Low);
    }

    #[test]
    fn test_qos_class_serialization() {
        let class = QoSClass::Critical;
        let json = serde_json::to_string(&class).unwrap();
        assert_eq!(json, "\"Critical\"");

        let deserialized: QoSClass = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, QoSClass::Critical);
    }

    #[test]
    fn test_qos_policy_serialization() {
        let policy = QoSPolicy::critical();
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: QoSPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, policy);
    }
}
