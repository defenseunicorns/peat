//! Storage backend trait abstractions and implementations
//!
//! Defines the core traits for the mesh storage layer, enabling runtime
//! backend selection between various storage implementations.

pub mod traits;

// Pure-generic storage types (no feature gate)
pub mod blob_traits;
pub mod geohash_index;
pub mod ttl;

// Automerge backend storage (ADR-049 Phase 3)
#[cfg(feature = "automerge-backend")]
pub mod automerge_store;
#[cfg(feature = "automerge-backend")]
pub mod flow_control;
#[cfg(feature = "automerge-backend")]
pub mod iroh_blob_store;
#[cfg(feature = "automerge-backend")]
pub mod negentropy_sync;
#[cfg(feature = "automerge-backend")]
pub mod partition_detection;
#[cfg(feature = "automerge-backend")]
pub mod query;
#[cfg(feature = "automerge-backend")]
pub mod sync_errors;
#[cfg(feature = "automerge-backend")]
pub mod sync_persistence;
#[cfg(feature = "automerge-backend")]
pub mod ttl_manager;

// Re-export key types (ungated)
pub use traits::{Collection, DocumentPredicate, StorageBackend};
pub use blob_traits::{
    BlobHandle, BlobHash, BlobMetadata, BlobProgress, BlobStorageSummary, BlobStore, BlobStoreExt,
    BlobToken, SharedBlobStore,
};
pub use ttl::{EvictionStrategy, OfflineRetentionPolicy, TtlConfig};
pub use geohash_index::GeohashIndex;

// Re-export key types (feature-gated)
#[cfg(feature = "automerge-backend")]
pub use automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
pub use iroh_blob_store::{IrohBlobStore, NetworkedIrohBlobStore};
#[cfg(feature = "automerge-backend")]
pub use partition_detection::{
    PartitionConfig, PartitionDetector, PartitionEvent, PeerHeartbeat, PeerPartitionState,
};
#[cfg(feature = "automerge-backend")]
pub use query::{extract_field, Query, SortOrder, Value};
#[cfg(feature = "automerge-backend")]
pub use flow_control::{
    BoundedQueue, FlowControlConfig, FlowControlError, FlowControlStats, FlowController,
    PeerResourceTracker, SyncCooldownTracker, TokenBucket,
};
#[cfg(feature = "automerge-backend")]
pub use negentropy_sync::{NegentropyStats, NegentropySync, ReconcileResult, SyncItem};
#[cfg(feature = "automerge-backend")]
pub use sync_persistence::{
    Checkpoint, PersistedSyncState, PersistenceStats, SyncStatePersistence,
};
#[cfg(feature = "automerge-backend")]
pub use ttl_manager::TtlManager;
