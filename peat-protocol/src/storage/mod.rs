//! Storage abstractions and implementations

// Core trait abstractions (ADR-011 E11.2)
pub mod backend;
pub mod capabilities;
pub mod traits;

// Blob storage trait abstraction (ADR-025)
pub mod blob_document_integration;
pub mod blob_traits;
pub mod file_distribution;
pub mod model_distribution;

// Existing implementations
pub mod cell_store;
pub mod node_store;
pub mod throttled_node_store;
pub mod ttl;

#[cfg(feature = "automerge-backend")]
pub mod automerge_backend;
#[cfg(feature = "automerge-backend")]
pub mod automerge_command_storage;
#[cfg(feature = "automerge-backend")]
pub mod automerge_conversion;
#[cfg(feature = "automerge-backend")]
pub mod automerge_store;
#[cfg(feature = "automerge-backend")]
pub mod automerge_summary_storage;
#[cfg(feature = "automerge-backend")]
pub mod automerge_sync;
#[cfg(feature = "automerge-backend")]
pub mod flow_control;
#[cfg(feature = "automerge-backend")]
pub mod geohash_index;
#[cfg(feature = "automerge-backend")]
pub mod iroh_blob_store;
#[cfg(feature = "automerge-backend")]
pub mod negentropy_sync;
#[cfg(feature = "automerge-backend")]
pub mod partition_detection;
#[cfg(feature = "automerge-backend")]
pub mod query;
#[cfg(feature = "automerge-backend")]
pub mod sync_channel;
#[cfg(feature = "automerge-backend")]
pub mod sync_errors;
#[cfg(feature = "automerge-backend")]
pub mod sync_forwarding;
#[cfg(feature = "automerge-backend")]
pub mod sync_persistence;
#[cfg(feature = "automerge-backend")]
pub mod ttl_manager;

pub use cell_store::CellStore;
pub use node_store::NodeStore;
pub use throttled_node_store::{ThrottleStats, ThrottledNodeStore};
pub use ttl::{EvictionStrategy, OfflineRetentionPolicy, TtlConfig};

#[cfg(feature = "automerge-backend")]
pub use automerge_backend::AutomergeBackend;
#[cfg(feature = "automerge-backend")]
pub use automerge_command_storage::AutomergeCommandStorage;
#[cfg(feature = "automerge-backend")]
pub use automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
pub use automerge_summary_storage::AutomergeSummaryStorage;
#[cfg(feature = "automerge-backend")]
pub use automerge_sync::{AutomergeSyncCoordinator, SyncDirection};
#[cfg(feature = "automerge-backend")]
pub use iroh_blob_store::{IrohBlobStore, NetworkedIrohBlobStore};
#[cfg(feature = "automerge-backend")]
pub use partition_detection::{
    PartitionConfig, PartitionDetector, PartitionEvent, PeerHeartbeat, PeerPartitionState,
};
#[cfg(feature = "automerge-backend")]
pub use ttl_manager::TtlManager;

// Query engine (Issue #80 - ADR-011 Phase 4)
#[cfg(feature = "automerge-backend")]
pub use geohash_index::{GeohashIndex, DEFAULT_GEOHASH_PRECISION};
#[cfg(feature = "automerge-backend")]
pub use query::{extract_field, Query, SortOrder, Value};

// Flow control & persistence (Issue #97 - ADR-011 Production Hardening)
#[cfg(feature = "automerge-backend")]
pub use flow_control::{
    BoundedQueue, FlowControlConfig, FlowControlError, FlowControlStats, FlowController,
    PeerResourceTracker, SyncCooldownTracker, TokenBucket,
};
#[cfg(feature = "automerge-backend")]
pub use negentropy_sync::{NegentropyStats, NegentropySync, ReconcileResult, SyncItem};
#[cfg(feature = "automerge-backend")]
pub use sync_channel::{ChannelManagerStats, ChannelState, SyncChannel, SyncChannelManager};
#[cfg(feature = "automerge-backend")]
pub use sync_forwarding::{ForwardingStats, SyncForwarder};
#[cfg(feature = "automerge-backend")]
pub use sync_persistence::{
    Checkpoint, PersistedSyncState, PersistenceStats, SyncStatePersistence,
};

// Trait abstractions (E11.2)
pub use backend::{create_storage_backend, StorageConfig};
pub use capabilities::{
    CrdtCapable, HierarchicalStorageCapable, SyncCapable, SyncStats, TypedCollection,
};
pub use traits::{Collection, DocumentPredicate, StorageBackend};

// Blob storage (ADR-025)
pub use blob_document_integration::{
    BlobDocumentIntegration, BlobReference, BlobReferenceMetadata, ModelProvenance,
    ModelRegistryDocument, ModelVariantBlob,
};
pub use blob_traits::{
    BlobHandle, BlobHash, BlobMetadata, BlobProgress, BlobStorageSummary, BlobStore, BlobStoreExt,
    BlobToken, SharedBlobStore,
};
#[cfg(feature = "automerge-backend")]
pub use file_distribution::IrohFileDistribution;
pub use file_distribution::{
    DistributionHandle, DistributionScope, DistributionStatus, FileDistribution,
    NodeTransferStatus, TransferPriority, TransferState,
};
pub use model_distribution::{
    BlockerReason, ConvergenceBlocker, ModelConvergenceStatus, ModelDeploymentTracker,
    ModelDistribution, ModelDistributionHandle, ModelOperationalStatus, NodeModelStatus,
    VariantSelector,
};

// Legacy compatibility aliases
pub use cell_store::CellStore as SquadStore;
pub use node_store::NodeStore as PlatformStore;
