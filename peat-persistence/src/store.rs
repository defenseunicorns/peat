//! Core DataStore trait for persistence abstraction

use crate::error::Result;
use crate::types::{Document, DocumentId, Query, SubscribeOptions, WriteOptions};
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

/// Core trait for CAP data persistence
///
/// This trait provides a backend-agnostic interface for storing and querying
/// Peat protocol data. Concrete backends implement this trait; the crate
/// itself ships only the abstraction.
///
/// # Example
///
/// ```rust,no_run
/// use peat_persistence::{DataStore, Query};
/// use serde_json::json;
///
/// async fn example(store: &dyn DataStore) -> Result<(), Box<dyn std::error::Error>> {
///     // Save a document
///     let node = json!({
///         "node_id": "node-1",
///         "phase": "discovery"
///     });
///     let id = store.save("node_states", &node).await?;
///
///     // Query documents
///     let nodes = store.query("node_states", Query::all()).await?;
///     println!("Found {} nodes", nodes.len());
///
///     // Subscribe to changes
///     let mut stream = store.observe("node_states", Query::all()).await?;
///     while let Some(event) = stream.recv().await {
///         println!("Change detected: {:?}", event);
///     }
///
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait DataStore: Send + Sync {
    /// Save or update a document
    ///
    /// If the document has an ID and exists, it will be updated.
    /// Otherwise, a new document will be created with a generated ID.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name (e.g., "node_states", "cell_states")
    /// * `document` - Document to save as JSON Value
    ///
    /// # Returns
    ///
    /// Document ID (newly generated or existing)
    async fn save(&self, collection: &str, document: &Value) -> Result<DocumentId>;

    /// Query documents with filtering
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name
    /// * `query` - Query with filters, sorting, pagination
    ///
    /// # Returns
    ///
    /// Vector of matching documents as JSON Values
    async fn query(&self, collection: &str, query: Query) -> Result<Vec<Value>>;

    /// Find a single document by ID
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name
    /// * `id` - Document ID
    ///
    /// # Returns
    ///
    /// Document if found as JSON Value
    async fn find_by_id(&self, collection: &str, id: &DocumentId) -> Result<Value>;

    /// Delete a document
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name
    /// * `id` - Document ID to delete
    async fn delete(&self, collection: &str, id: &DocumentId) -> Result<()>;

    /// Subscribe to live updates
    ///
    /// Returns a channel that receives change events whenever documents
    /// matching the query are added, updated, or deleted.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name
    /// * `query` - Query to watch for changes
    ///
    /// # Returns
    ///
    /// Channel receiver for change events
    async fn observe(
        &self,
        collection: &str,
        query: Query,
    ) -> Result<mpsc::UnboundedReceiver<ChangeEvent>>;

    /// Get store information
    fn store_info(&self) -> StoreInfo;

    // =========================================================================
    // Bypass Integration Methods (ADR-042)
    // =========================================================================

    /// Save or update a document with options
    ///
    /// Extended version of `save()` that supports bypass mode and other options.
    /// When `options.bypass_sync` is `true`, the document is sent via UDP bypass
    /// channel instead of CRDT sync for low-latency delivery.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name
    /// * `document` - Document to save as JSON Value
    /// * `options` - Write options (bypass mode, TTL, priority, etc.)
    ///
    /// # Returns
    ///
    /// Document ID (may be empty for bypass-only writes that aren't persisted)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use peat_persistence::{DataStore, WriteOptions};
    /// use serde_json::json;
    ///
    /// async fn example(store: &dyn DataStore) -> Result<(), Box<dyn std::error::Error>> {
    ///     // High-frequency position update via bypass
    ///     let position = json!({
    ///         "node_id": "node-1",
    ///         "lat": 37.7749,
    ///         "lon": -122.4194,
    ///         "timestamp": 1234567890
    ///     });
    ///     store.save_with_options(
    ///         "position_updates",
    ///         &position,
    ///         WriteOptions::bypass(),
    ///     ).await?;
    ///     Ok(())
    /// }
    /// ```
    async fn save_with_options(
        &self,
        collection: &str,
        document: &Value,
        options: WriteOptions,
    ) -> Result<DocumentId> {
        // Default implementation: ignore bypass flag and use normal save
        // Implementations with bypass channel support should override this
        let _ = options;
        self.save(collection, document).await
    }

    /// Subscribe to live updates with options
    ///
    /// Extended version of `observe()` that supports merging bypass and sync streams.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name
    /// * `query` - Query to watch for changes (applies to CRDT sync only)
    /// * `options` - Subscribe options (include_bypass, include_sync, etc.)
    ///
    /// # Returns
    ///
    /// Channel receiver for change events from selected sources
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use peat_persistence::{DataStore, Query, SubscribeOptions};
    ///
    /// async fn example(store: &dyn DataStore) -> Result<(), Box<dyn std::error::Error>> {
    ///     // Subscribe to high-frequency position updates via bypass only
    ///     let mut stream = store.observe_with_options(
    ///         "position_updates",
    ///         Query::all(),
    ///         SubscribeOptions::bypass_only(),
    ///     ).await?;
    ///
    ///     while let Some(event) = stream.recv().await {
    ///         println!("Position update: {:?}", event);
    ///     }
    ///     Ok(())
    /// }
    /// ```
    async fn observe_with_options(
        &self,
        collection: &str,
        query: Query,
        options: SubscribeOptions,
    ) -> Result<mpsc::UnboundedReceiver<ChangeEvent>> {
        // Default implementation: ignore options and use normal observe
        // Implementations with bypass channel support should override this
        let _ = options;
        self.observe(collection, query).await
    }

    /// Check if a collection is configured for bypass
    ///
    /// Returns `true` if the collection is configured to use the UDP bypass
    /// channel for writes with `bypass_sync: true`.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name to check
    ///
    /// # Returns
    ///
    /// `true` if bypass is configured for this collection
    fn is_bypass_enabled(&self, _collection: &str) -> bool {
        // Default: bypass not enabled
        // Implementations should override if they support bypass
        false
    }
}

/// Change event for document subscriptions
#[derive(Debug, Clone)]
pub enum ChangeEvent {
    /// Document was added or updated
    Upsert {
        /// Document ID
        id: DocumentId,
        /// Updated document data
        document: Document,
    },
    /// Document was deleted
    Delete {
        /// Document ID
        id: DocumentId,
    },
    /// Initial data loaded
    Initial {
        /// Number of documents
        count: usize,
    },
}

/// Information about the storage backend
#[derive(Debug, Clone)]
pub struct StoreInfo {
    /// Backend name (e.g., "Automerge", "SQLite")
    pub name: String,
    /// Backend version
    pub version: String,
    /// Additional backend-specific properties
    pub properties: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_info_creation() {
        let info = StoreInfo {
            name: "TestStore".to_string(),
            version: "1.0.0".to_string(),
            properties: std::collections::HashMap::new(),
        };
        assert_eq!(info.name, "TestStore");
        assert_eq!(info.version, "1.0.0");
    }

    #[test]
    fn test_change_event_creation() {
        let event = ChangeEvent::Initial { count: 10 };
        match event {
            ChangeEvent::Initial { count } => assert_eq!(count, 10),
            _ => panic!("Wrong event type"),
        }
    }
}
