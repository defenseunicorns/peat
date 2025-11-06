//! Data Synchronization Abstraction Layer
//!
//! This module provides a unified interface for CRDT-based data synchronization,
//! enabling CAP Protocol to work with multiple sync engines without changing
//! business logic.
//!
//! ## Architecture
//!
//! The abstraction consists of four core traits:
//!
//! 1. **`DocumentStore`** - CRUD operations, queries, and live observers
//! 2. **`PeerDiscovery`** - Finding and connecting to other nodes
//! 3. **`SyncEngine`** - Controlling synchronization behavior
//! 4. **`DataSyncBackend`** - Lifecycle management and trait composition
//!
//! ## Supported Backends
//!
//! - **Ditto** - Current production backend (proprietary CBOR-based)
//! - **Automerge** - Future open-source backend (columnar storage) [planned]
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use cap_protocol::sync::{DataSyncBackend, BackendConfig};
//! use cap_protocol::sync::ditto::DittoBackend;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create backend
//! let backend = DittoBackend::new();
//!
//! // Initialize with config
//! let config = BackendConfig {
//!     app_id: "my-app".to_string(),
//!     persistence_dir: "/tmp/data".into(),
//!     // ...
//! };
//! backend.initialize(config).await?;
//!
//! // Start peer discovery and sync
//! backend.peer_discovery().start().await?;
//! backend.sync_engine().start_sync().await?;
//!
//! // Use document store
//! let doc_store = backend.document_store();
//! let doc = Document::new(fields);
//! doc_store.upsert("collection", doc).await?;
//!
//! // Cleanup
//! backend.shutdown().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Testing
//!
//! The abstraction enables testing without real backends:
//!
//! ```rust,ignore
//! use cap_protocol::sync::mock::MockBackend;
//!
//! # async fn test_example() {
//! let backend = MockBackend::new();
//! // Test CAP protocol logic with mock backend
//! # }
//! ```
//!
//! ## Design Rationale
//!
//! See ADR-005 for full context on why this abstraction exists:
//! - Eliminate vendor lock-in with proprietary sync engines
//! - Enable open-source alternatives for DoD/NATO deployments
//! - Support multiple sync strategies (Ditto, Automerge, custom)
//! - Simplify testing (mocks, no real Ditto instances needed)

pub mod traits;
pub mod types;

// Backend implementations
#[cfg(feature = "automerge-backend")]
pub mod automerge; // Automerge CRDT backend (E8 evaluation)
pub mod ditto; // Wraps existing Ditto SDK

// Re-export core types and traits for convenience
pub use traits::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_compiles() {
        // Sanity check that all types and traits are accessible
        let _: Option<Document> = None;
        let _: Option<Query> = None;
        let _: Option<BackendConfig> = None;
    }
}
