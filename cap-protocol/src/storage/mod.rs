//! Storage abstractions and implementations

// Core trait abstractions (ADR-011 E11.2)
pub mod traits;

// Existing implementations
pub mod cell_store;
pub mod ditto_store;
pub mod node_store;
pub mod throttled_node_store;
pub mod ttl;

#[cfg(feature = "automerge-backend")]
pub mod automerge_conversion;
#[cfg(feature = "automerge-backend")]
pub mod automerge_store;

pub use cell_store::CellStore;
pub use ditto_store::DittoStore;
pub use node_store::NodeStore;
pub use throttled_node_store::{ThrottleStats, ThrottledNodeStore};
pub use ttl::{EvictionStrategy, OfflineRetentionPolicy, TtlConfig};

#[cfg(feature = "automerge-backend")]
pub use automerge_store::AutomergeStore;

// Trait abstractions (E11.2)
pub use traits::{Collection, DocumentPredicate, StorageBackend};

// Legacy compatibility aliases
pub use cell_store::CellStore as SquadStore;
pub use node_store::NodeStore as PlatformStore;
