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

    // Test that traits are object-safe (can be used as trait objects)
    #[test]
    fn test_storage_backend_is_object_safe() {
        // This test just needs to compile - it verifies object safety
        fn _assert_object_safe(_: &dyn StorageBackend) {}
    }

    #[test]
    fn test_collection_is_object_safe() {
        // This test just needs to compile - it verifies object safety
        fn _assert_object_safe(_: &dyn Collection) {}
    }
}
