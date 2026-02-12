//! Storage backend trait abstraction
//!
//! This module defines the core traits for HIVE Protocol's storage layer,
//! enabling runtime backend selection between Ditto, Automerge, RocksDB, etc.
//!
//! # Design Philosophy
//!
//! - **Backend agnostic**: Business logic doesn't depend on specific storage implementation
//! - **Type-safe**: Rust's type system enforces correct usage
//! - **Flexible**: Easy to add new backends (Redb, LMDB, etc.)
//! - **Testable**: Mock implementations for testing without real storage
//!
//! # Example
//!
//! ```ignore
//! use hive_protocol::storage::{StorageBackend, create_storage_backend};
//!
//! // Create backend from configuration
//! let config = StorageConfig::from_env()?;
//! let storage = create_storage_backend(&config)?;
//!
//! // Get collection and perform operations
//! let cells = storage.collection("cells");
//! cells.upsert("cell-1", serialize(&cell_state)?)?;
//!
//! let cell_bytes = cells.get("cell-1")?.unwrap();
//! let cell = deserialize::<CellState>(&cell_bytes)?;
//! ```

use anyhow::Result;
use std::sync::Arc;

/// Type alias for document predicate functions
///
/// Used in `Collection::find()` to filter documents by their serialized bytes.
pub type DocumentPredicate = Box<dyn Fn(&[u8]) -> bool + Send>;

/// Main storage backend trait
///
/// Implementations provide access to collections and manage the underlying storage.
/// All implementations must be thread-safe (Send + Sync).
///
/// # Implementations
///
/// - **DittoBackend**: Wraps existing Ditto SDK (proprietary, production-ready)
/// - **AutomergeInMemoryBackend**: In-memory Automerge (POC, testing)
/// - **RocksDbBackend**: RocksDB persistence (production target)
///
/// # Thread Safety
///
/// All methods are safe to call from multiple threads. Implementations should use
/// appropriate synchronization (Arc, RwLock, etc.) as needed.
pub trait StorageBackend: Send + Sync {
    /// Get or create a collection by name
    ///
    /// Collections are logical groupings of documents (e.g., "cells", "nodes").
    /// Multiple calls with the same name return references to the same collection.
    ///
    /// # Arguments
    ///
    /// * `name` - Collection name (e.g., "cells", "nodes", "capabilities")
    ///
    /// # Returns
    ///
    /// A thread-safe collection handle
    ///
    /// # Example
    ///
    /// ```ignore
    /// let cells = storage.collection("cells");
    /// let nodes = storage.collection("nodes");
    /// ```
    fn collection(&self, name: &str) -> Arc<dyn Collection>;

    /// List all collection names
    ///
    /// Returns names of all collections that have been created or contain documents.
    ///
    /// # Returns
    ///
    /// Vector of collection names (may be empty)
    fn list_collections(&self) -> Vec<String>;

    /// Flush any pending writes to disk
    ///
    /// For in-memory backends, this is a no-op. For persistent backends (RocksDB),
    /// this ensures all writes are durable.
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if flush fails
    fn flush(&self) -> Result<()>;

    /// Close the storage backend cleanly
    ///
    /// Implementations should flush pending writes and release resources.
    /// After calling close(), the backend should not be used.
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if close fails
    fn close(self) -> Result<()>;
}

/// Collection trait for storing and querying documents
///
/// A collection is a logical grouping of documents (key-value pairs).
/// Documents are stored as raw bytes (typically serialized protobuf).
///
/// # Document Storage Format
///
/// - **Key**: String document ID (e.g., "cell-1", "node-abc123")
/// - **Value**: Raw bytes (serialized protobuf message)
///
/// # Thread Safety
///
/// All operations are thread-safe. Multiple threads can read/write concurrently.
///
/// # Example
///
/// ```ignore
/// let cells = storage.collection("cells");
///
/// // Create and store a cell
/// let cell = CellState { id: "cell-1".to_string(), ..Default::default() };
/// let bytes = cell.encode_to_vec();
/// cells.upsert("cell-1", bytes)?;
///
/// // Retrieve the cell
/// if let Some(stored) = cells.get("cell-1")? {
///     let cell = CellState::decode(&stored[..])?;
///     println!("Retrieved cell: {}", cell.id);
/// }
///
/// // Query all cells
/// for (id, bytes) in cells.scan()? {
///     let cell = CellState::decode(&bytes[..])?;
///     println!("Cell {}: {:?}", id, cell);
/// }
/// ```
pub trait Collection: Send + Sync {
    /// Insert or update a document
    ///
    /// If a document with the given ID exists, it is replaced. Otherwise, a new
    /// document is created.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Unique document identifier
    /// * `data` - Serialized document bytes (typically protobuf)
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if upsert fails
    ///
    /// # Example
    ///
    /// ```ignore
    /// let cell = CellState { id: "cell-1".to_string(), ..Default::default() };
    /// cells.upsert("cell-1", cell.encode_to_vec())?;
    /// ```
    fn upsert(&self, doc_id: &str, data: Vec<u8>) -> Result<()>;

    /// Get a document by ID
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Document identifier to retrieve
    ///
    /// # Returns
    ///
    /// - `Ok(Some(bytes))` if document exists
    /// - `Ok(None)` if document not found
    /// - `Err` if query fails
    ///
    /// # Example
    ///
    /// ```ignore
    /// match cells.get("cell-1")? {
    ///     Some(bytes) => {
    ///         let cell = CellState::decode(&bytes[..])?;
    ///         println!("Found cell: {}", cell.id);
    ///     }
    ///     None => println!("Cell not found"),
    /// }
    /// ```
    fn get(&self, doc_id: &str) -> Result<Option<Vec<u8>>>;

    /// Delete a document by ID
    ///
    /// If the document doesn't exist, this is a no-op (not an error).
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Document identifier to delete
    ///
    /// # Returns
    ///
    /// Ok(()) on success (whether or not document existed), Err if delete fails
    ///
    /// # Example
    ///
    /// ```ignore
    /// cells.delete("cell-1")?;
    /// ```
    fn delete(&self, doc_id: &str) -> Result<()>;

    /// Scan all documents in the collection
    ///
    /// Returns all documents as (id, bytes) tuples. Order is implementation-defined.
    ///
    /// # Performance
    ///
    /// This loads all documents into memory. For large collections, consider
    /// streaming or pagination (future enhancement).
    ///
    /// # Returns
    ///
    /// Vector of (document_id, document_bytes) tuples
    ///
    /// # Example
    ///
    /// ```ignore
    /// for (id, bytes) in cells.scan()? {
    ///     let cell = CellState::decode(&bytes[..])?;
    ///     println!("Cell {}: {:?}", id, cell);
    /// }
    /// ```
    fn scan(&self) -> Result<Vec<(String, Vec<u8>)>>;

    /// Find documents matching a predicate
    ///
    /// Filters documents by applying a predicate function to their serialized bytes.
    /// Less efficient than indexed queries, but flexible.
    ///
    /// # Arguments
    ///
    /// * `predicate` - Function that returns true for documents to include
    ///
    /// # Returns
    ///
    /// Vector of matching (document_id, document_bytes) tuples
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Find all cells with status "active"
    /// let active_cells = cells.find(Box::new(|bytes| {
    ///     if let Ok(cell) = CellState::decode(bytes) {
    ///         cell.status == CellStatus::Active as i32
    ///     } else {
    ///         false
    ///     }
    /// }))?;
    /// ```
    fn find(&self, predicate: DocumentPredicate) -> Result<Vec<(String, Vec<u8>)>>;

    /// Query documents by geohash prefix (proximity queries)
    ///
    /// Geohash is a hierarchical spatial index. Documents with the same prefix
    /// are geographically close. Longer prefixes = smaller areas.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Geohash prefix (e.g., "9q8y" for San Francisco area)
    ///
    /// # Returns
    ///
    /// Vector of matching (document_id, document_bytes) tuples
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Find all nodes within ~5km of a location
    /// let nearby_nodes = nodes.query_geohash_prefix("9q8yy")?;
    /// ```
    ///
    /// # Implementation Notes
    ///
    /// - For Ditto: Uses geohash index (efficient)
    /// - For RocksDB: Uses prefix scan (requires geohash in key)
    /// - For in-memory: Scans all documents (inefficient for large datasets)
    fn query_geohash_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>>;

    /// Count documents in the collection
    ///
    /// # Returns
    ///
    /// Number of documents in collection
    ///
    /// # Example
    ///
    /// ```ignore
    /// let cell_count = cells.count()?;
    /// println!("Total cells: {}", cell_count);
    /// ```
    fn count(&self) -> Result<usize>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;

    // Test that traits are object-safe (can be used as trait objects)
    #[test]
    fn test_storage_backend_is_object_safe() {
        fn _assert_object_safe(_: &dyn StorageBackend) {}
    }

    #[test]
    fn test_collection_is_object_safe() {
        fn _assert_object_safe(_: &dyn Collection) {}
    }

    // --- In-memory Collection for testing ---

    struct InMemCollection {
        data: RwLock<HashMap<String, Vec<u8>>>,
    }

    impl InMemCollection {
        fn new() -> Self {
            Self {
                data: RwLock::new(HashMap::new()),
            }
        }
    }

    impl Collection for InMemCollection {
        fn upsert(&self, doc_id: &str, data: Vec<u8>) -> Result<()> {
            self.data.write().unwrap().insert(doc_id.to_string(), data);
            Ok(())
        }

        fn get(&self, doc_id: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.data.read().unwrap().get(doc_id).cloned())
        }

        fn delete(&self, doc_id: &str) -> Result<()> {
            self.data.write().unwrap().remove(doc_id);
            Ok(())
        }

        fn scan(&self) -> Result<Vec<(String, Vec<u8>)>> {
            Ok(self
                .data
                .read()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }

        fn find(&self, predicate: DocumentPredicate) -> Result<Vec<(String, Vec<u8>)>> {
            Ok(self
                .data
                .read()
                .unwrap()
                .iter()
                .filter(|(_, v)| predicate(v))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }

        fn query_geohash_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
            Ok(self
                .data
                .read()
                .unwrap()
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }

        fn count(&self) -> Result<usize> {
            Ok(self.data.read().unwrap().len())
        }
    }

    #[test]
    fn test_collection_upsert_and_get() {
        let col = InMemCollection::new();
        col.upsert("doc-1", vec![1, 2, 3]).unwrap();

        let result = col.get("doc-1").unwrap();
        assert_eq!(result, Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_collection_get_missing() {
        let col = InMemCollection::new();
        assert_eq!(col.get("nonexistent").unwrap(), None);
    }

    #[test]
    fn test_collection_upsert_overwrite() {
        let col = InMemCollection::new();
        col.upsert("doc-1", vec![1]).unwrap();
        col.upsert("doc-1", vec![2]).unwrap();

        assert_eq!(col.get("doc-1").unwrap(), Some(vec![2]));
        assert_eq!(col.count().unwrap(), 1);
    }

    #[test]
    fn test_collection_delete() {
        let col = InMemCollection::new();
        col.upsert("doc-1", vec![1]).unwrap();
        col.delete("doc-1").unwrap();

        assert_eq!(col.get("doc-1").unwrap(), None);
        assert_eq!(col.count().unwrap(), 0);
    }

    #[test]
    fn test_collection_delete_nonexistent_noop() {
        let col = InMemCollection::new();
        col.delete("nonexistent").unwrap(); // should not error
    }

    #[test]
    fn test_collection_scan() {
        let col = InMemCollection::new();
        col.upsert("a", vec![1]).unwrap();
        col.upsert("b", vec![2]).unwrap();

        let mut results = col.scan().unwrap();
        results.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ("a".to_string(), vec![1]));
        assert_eq!(results[1], ("b".to_string(), vec![2]));
    }

    #[test]
    fn test_collection_find() {
        let col = InMemCollection::new();
        col.upsert("big", vec![100, 200]).unwrap();
        col.upsert("small", vec![1]).unwrap();

        let results = col.find(Box::new(|bytes| bytes.len() > 1)).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "big");
    }

    #[test]
    fn test_collection_query_geohash_prefix() {
        let col = InMemCollection::new();
        col.upsert("9q8yy_a", vec![1]).unwrap();
        col.upsert("9q8yy_b", vec![2]).unwrap();
        col.upsert("u4pru_c", vec![3]).unwrap();

        let results = col.query_geohash_prefix("9q8yy").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_collection_count() {
        let col = InMemCollection::new();
        assert_eq!(col.count().unwrap(), 0);

        col.upsert("a", vec![1]).unwrap();
        col.upsert("b", vec![2]).unwrap();
        assert_eq!(col.count().unwrap(), 2);
    }

    // --- In-memory StorageBackend for testing ---

    struct InMemBackend {
        collections: RwLock<HashMap<String, Arc<InMemCollection>>>,
    }

    impl InMemBackend {
        fn new() -> Self {
            Self {
                collections: RwLock::new(HashMap::new()),
            }
        }
    }

    impl StorageBackend for InMemBackend {
        fn collection(&self, name: &str) -> Arc<dyn Collection> {
            let mut cols = self.collections.write().unwrap();
            cols.entry(name.to_string())
                .or_insert_with(|| Arc::new(InMemCollection::new()))
                .clone()
        }

        fn list_collections(&self) -> Vec<String> {
            self.collections.read().unwrap().keys().cloned().collect()
        }

        fn flush(&self) -> Result<()> {
            Ok(())
        }

        fn close(self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_backend_collection_create_and_reuse() {
        let backend = InMemBackend::new();
        let col1 = backend.collection("cells");
        col1.upsert("c1", vec![1]).unwrap();

        let col2 = backend.collection("cells");
        assert_eq!(col2.get("c1").unwrap(), Some(vec![1]));
    }

    #[test]
    fn test_backend_list_collections() {
        let backend = InMemBackend::new();
        let _ = backend.collection("cells");
        let _ = backend.collection("nodes");

        let mut names = backend.list_collections();
        names.sort();
        assert_eq!(names, vec!["cells", "nodes"]);
    }

    #[test]
    fn test_backend_flush() {
        let backend = InMemBackend::new();
        assert!(backend.flush().is_ok());
    }

    #[test]
    fn test_backend_close() {
        let backend = InMemBackend::new();
        assert!(backend.close().is_ok());
    }
}
