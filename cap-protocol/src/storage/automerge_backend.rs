//! Automerge backend adapter for trait abstraction
//!
//! This module provides an adapter between the StorageBackend trait
//! and the AutomergeStore implementation. It enables backend-agnostic business
//! logic with fully open-source CRDT storage.
//!
//! # Architecture
//!
//! ```text
//! Business Logic (Coordinators)
//!         ↓
//! StorageBackend trait (backend-agnostic)
//!         ↓
//! AutomergeBackend (adapter) ← This module
//!         ↓
//! AutomergeStore (Automerge + RocksDB)
//!         ↓
//! ┌────────────────┐  ┌────────────┐
//! │ Automerge 0.7  │  │ RocksDB    │
//! │ (CRDT engine)  │  │ (persist)  │
//! └────────────────┘  └────────────┘
//! ```
//!
//! # Phase 1 Limitations
//!
//! **Current**: Phase 1 stores raw bytes in Automerge documents (simple blob storage).
//! - Documents stored as: `Automerge { "data": bytes }`
//! - No field-level CRDT semantics yet
//! - Provides Collection trait interface for backend-agnostic code
//!
//! **Future** (Phase 2): Protobuf → JSON → Automerge conversion for CRDT benefits.
//! - Field-level merging (OR-Set for arrays, LWW-Register for scalars)
//! - Delta sync (only changed fields transmitted)
//! - See `automerge_conversion.rs` for conversion utilities
//!
//! # Usage Examples
//!
//! ## Create backend with persistence
//!
//! ```ignore
//! use cap_protocol::storage::{AutomergeBackend, AutomergeStore};
//! use std::sync::Arc;
//!
//! let store = Arc::new(AutomergeStore::open("./data/automerge")?);
//! let backend = AutomergeBackend::new(store);
//!
//! // Use via StorageBackend trait
//! let cells = backend.collection("cells");
//! cells.upsert("cell-1", cell_state.encode_to_vec())?;
//! ```
//!
//! ## Backend comparison
//!
//! | Feature              | DittoBackend         | AutomergeBackend (Phase 1) | AutomergeBackend (Phase 2) |
//! |----------------------|----------------------|----------------------------|----------------------------|
//! | CRDT Support         | ✅ (built-in)        | ❌ (blob storage)          | ✅ (field-level)           |
//! | Persistence          | ✅ (SQLite)          | ✅ (RocksDB)               | ✅ (RocksDB)               |
//! | Network Sync         | ✅ (multi-transport) | ⏭ (Phase 4: Iroh)         | ⏭ (Phase 4: Iroh)         |
//! | License              | Proprietary          | MIT/Apache 2.0             | MIT/Apache 2.0             |
//! | Backend-agnostic API | ✅                   | ✅                         | ✅                         |

#[cfg(feature = "automerge-backend")]
use super::automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
use super::traits::{Collection, StorageBackend};
#[cfg(feature = "automerge-backend")]
use anyhow::Result;
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};

/// Automerge backend adapter implementing StorageBackend trait
///
/// Wraps AutomergeStore to provide trait-based interface for backend-agnostic code.
///
/// # Phase 1 Implementation
///
/// - Stores raw bytes in Automerge documents (blob storage)
/// - Provides Collection abstraction with namespace isolation
/// - RocksDB persistence with LRU cache
/// - No network sync yet (Phase 4)
#[cfg(feature = "automerge-backend")]
pub struct AutomergeBackend {
    /// Underlying AutomergeStore instance
    store: Arc<AutomergeStore>,
    /// Cache of known collection names
    collections: Arc<RwLock<HashMap<String, Arc<dyn Collection>>>>,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeBackend {
    /// Create a new Automerge backend from an existing AutomergeStore
    ///
    /// # Arguments
    ///
    /// * `store` - Configured AutomergeStore instance
    ///
    /// # Example
    ///
    /// ```ignore
    /// use cap_protocol::storage::{AutomergeBackend, AutomergeStore};
    /// use std::sync::Arc;
    ///
    /// let store = Arc::new(AutomergeStore::open("./data/automerge")?);
    /// let backend = AutomergeBackend::new(store);
    /// ```
    pub fn new(store: Arc<AutomergeStore>) -> Self {
        Self {
            store,
            collections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get access to underlying AutomergeStore for store-specific operations
    ///
    /// This provides an escape hatch for features not yet abstracted by the trait.
    pub fn automerge_store(&self) -> &AutomergeStore {
        &self.store
    }
}

#[cfg(feature = "automerge-backend")]
impl StorageBackend for AutomergeBackend {
    fn collection(&self, name: &str) -> Arc<dyn Collection> {
        // Check cache first
        {
            let collections = self.collections.read().unwrap();
            if let Some(collection) = collections.get(name) {
                return Arc::clone(collection);
            }
        }

        // Create new collection and cache it
        let collection = self.store.collection(name);
        self.collections
            .write()
            .unwrap()
            .insert(name.to_string(), Arc::clone(&collection));

        collection
    }

    fn list_collections(&self) -> Vec<String> {
        // Return known collections from cache
        let collections = self.collections.read().unwrap();
        collections.keys().cloned().collect()
    }

    fn flush(&self) -> Result<()> {
        // RocksDB handles durability automatically via write-ahead log
        // No explicit flush needed for Phase 1
        Ok(())
    }

    fn close(self) -> Result<()> {
        // RocksDB will be closed when AutomergeStore is dropped
        // No explicit cleanup needed for Phase 1
        // Phase 4 will need to stop sync here
        Ok(())
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_backend() -> (AutomergeBackend, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
        let backend = AutomergeBackend::new(store);
        (backend, temp_dir)
    }

    #[test]
    fn test_backend_collection_creation() {
        let (backend, _temp) = create_test_backend();

        let collection = backend.collection("test");
        assert!(collection.count().unwrap() == 0);
    }

    #[test]
    fn test_backend_collection_caching() {
        let (backend, _temp) = create_test_backend();

        let coll1 = backend.collection("test");
        let coll2 = backend.collection("test");

        // Both should point to the same cached collection
        assert_eq!(Arc::as_ptr(&coll1), Arc::as_ptr(&coll2));
    }

    #[test]
    fn test_backend_list_collections() {
        let (backend, _temp) = create_test_backend();

        assert_eq!(backend.list_collections().len(), 0);

        backend.collection("coll1");
        backend.collection("coll2");

        let collections = backend.list_collections();
        assert_eq!(collections.len(), 2);
        assert!(collections.contains(&"coll1".to_string()));
        assert!(collections.contains(&"coll2".to_string()));
    }

    #[test]
    fn test_backend_operations_via_trait() {
        let (backend, _temp) = create_test_backend();

        let collection = backend.collection("test");

        // Test CRUD via trait interface
        collection.upsert("doc1", b"data1".to_vec()).unwrap();
        let retrieved = collection.get("doc1").unwrap().unwrap();
        assert_eq!(retrieved, b"data1");

        collection.delete("doc1").unwrap();
        assert!(collection.get("doc1").unwrap().is_none());
    }

    #[test]
    fn test_backend_flush_and_close() {
        let (backend, _temp) = create_test_backend();

        // Flush should succeed (no-op in Phase 1)
        assert!(backend.flush().is_ok());

        // Close should succeed
        assert!(backend.close().is_ok());
    }
}
