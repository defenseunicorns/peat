//! Storage abstractions and implementations

// Core trait abstractions (ADR-011 E11.2)
pub mod backend;
pub mod capabilities;
pub mod traits;

// Blob storage trait abstraction (ADR-025)
pub mod blob_document_integration;
pub mod blob_traits;
pub mod ditto_blob_store;
pub mod file_distribution;

// Backend implementations (E11.2)
pub mod ditto_backend;

// Existing implementations
pub mod cell_store;
pub mod ditto_command_storage;
pub mod ditto_store;
pub mod ditto_summary_storage;
pub mod node_store;
pub mod throttled_node_store;
pub mod ttl;

#[cfg(feature = "automerge-backend")]
pub mod automerge_backend;
#[cfg(feature = "automerge-backend")]
pub mod automerge_conversion;
#[cfg(feature = "automerge-backend")]
pub mod automerge_store;
#[cfg(feature = "automerge-backend")]
pub mod automerge_sync;
#[cfg(feature = "automerge-backend")]
pub mod iroh_blob_store;
#[cfg(feature = "automerge-backend")]
pub mod partition_detection;
#[cfg(feature = "automerge-backend")]
pub mod sync_errors;
#[cfg(feature = "automerge-backend")]
pub mod ttl_manager;

pub use cell_store::CellStore;
pub use ditto_command_storage::DittoCommandStorage;
pub use ditto_store::DittoStore;
pub use ditto_summary_storage::DittoSummaryStorage;
pub use node_store::NodeStore;
pub use throttled_node_store::{ThrottleStats, ThrottledNodeStore};
pub use ttl::{EvictionStrategy, OfflineRetentionPolicy, TtlConfig};

#[cfg(feature = "automerge-backend")]
pub use automerge_backend::AutomergeBackend;
#[cfg(feature = "automerge-backend")]
pub use automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
pub use automerge_sync::AutomergeSyncCoordinator;
#[cfg(feature = "automerge-backend")]
pub use iroh_blob_store::IrohBlobStore;
#[cfg(feature = "automerge-backend")]
pub use partition_detection::{
    PartitionConfig, PartitionDetector, PartitionEvent, PeerHeartbeat, PeerPartitionState,
};
#[cfg(feature = "automerge-backend")]
pub use ttl_manager::TtlManager;

// Trait abstractions (E11.2)
pub use backend::{create_storage_backend, StorageConfig};
pub use capabilities::{CrdtCapable, SyncCapable, SyncStats, TypedCollection};
pub use ditto_backend::DittoBackend;
pub use traits::{Collection, DocumentPredicate, StorageBackend};

// Blob storage (ADR-025)
pub use blob_document_integration::{
    BlobDocumentIntegration, BlobReference, BlobReferenceMetadata, DittoBlobDocumentIntegration,
    ModelProvenance, ModelRegistryDocument, ModelVariantBlob,
};
pub use blob_traits::{
    BlobHandle, BlobHash, BlobMetadata, BlobProgress, BlobStorageSummary, BlobStore, BlobStoreExt,
    BlobToken, SharedBlobStore,
};
pub use ditto_blob_store::DittoBlobStore;
pub use file_distribution::{
    DistributionHandle, DistributionScope, DistributionStatus, DittoFileDistribution,
    FileDistribution, NodeTransferStatus, TransferPriority, TransferState,
};

// Legacy compatibility aliases
pub use cell_store::CellStore as SquadStore;
pub use node_store::NodeStore as PlatformStore;
