//! Quality of Service (QoS) framework for data prioritization (ADR-019)
//!
//! This module provides the foundational QoS classification system for Peat Protocol,
//! ensuring critical data (contact reports, commands) reaches commanders before routine
//! telemetry.
//!
//! # Architecture
//!
//! - **QoSClass**: 5-level priority classification (P1 Critical → P5 Bulk)
//! - **QoSPolicy**: Per-data-type policy with latency, TTL, retention parameters
//! - **QoSRegistry**: Maps PEAT data types to their QoS policies
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
//! use peat_protocol::qos::{QoSClass, QoSPolicy, DataType, QoSRegistry};
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
//! use peat_protocol::qos::{
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

// ============================================================================
// Re-exported modules from peat-mesh (generic QoS framework)
// ============================================================================

pub mod audit;
pub mod bandwidth;
pub mod deletion;
pub mod eviction;
pub mod garbage_collection;
pub mod lifecycle;
pub mod preemption;
pub mod retention;
pub mod storage;
pub mod sync_mode;

// ============================================================================
// PEAT-specific modules (depend on military domain types)
// ============================================================================

pub mod classification;
pub mod context;
pub mod context_manager;
pub mod recovery;
pub mod registry;
pub mod sync_queue;

// ============================================================================
// Re-export QoSClass and QoSPolicy from peat-mesh
// ============================================================================

pub use peat_mesh::qos::{QoSClass, QoSPolicy};

// ============================================================================
// Re-exports from peat-mesh QoS submodules
// ============================================================================

pub use audit::{AuditAction, AuditEntry, AuditSummary, EvictionAuditLog};
pub use bandwidth::{
    BandwidthAllocation, BandwidthConfig, BandwidthPermit, BandwidthQuota, QuotaConfig,
};
pub use deletion::{
    DeleteResult, DeletionPolicy, DeletionPolicyRegistry, PropagationDirection, Tombstone,
    TombstoneBatch, TombstoneDecodeError, TombstoneSyncMessage,
};
pub use eviction::{EvictionConfig, EvictionController, EvictionResult};
pub use garbage_collection::{
    start_periodic_gc, GarbageCollector, GcConfig, GcResult, GcStats, GcStore, ResurrectionPolicy,
};
pub use lifecycle::{
    make_lifecycle_decision, LifecycleDecision, LifecyclePolicies, LifecyclePolicy,
};
pub use preemption::{ActiveTransfer, PreemptionController, PreemptionStats, TransferId};
pub use retention::{RetentionPolicies, RetentionPolicy};
pub use storage::{
    ClassStorageMetrics, EvictionCandidate, QoSAwareStorage, StorageMetrics, StoredDocument,
};
pub use sync_mode::{SyncMode, SyncModeRegistry};

// ============================================================================
// PEAT-specific re-exports
// ============================================================================

pub use classification::DataType;
pub use context::{ContextProfile, MissionContext, QoSClassAdjustment};
pub use context_manager::{ContextChangeListener, ContextChangeLog, ContextManager};
pub use recovery::{RecoveryStats, SyncRecovery, UpdateBatch};
pub use registry::QoSRegistry;
pub use sync_queue::{PendingSync, PrioritySyncQueue, QueueStats};

// ============================================================================
// Conversions to/from existing priority types (PEAT-specific)
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
}
