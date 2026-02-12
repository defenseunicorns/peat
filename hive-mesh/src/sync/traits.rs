//! Core trait definitions for data synchronization abstraction
//!
//! This module defines the four fundamental traits that any sync backend must implement:
//! - `DocumentStore`: CRUD operations and queries
//! - `PeerDiscovery`: Peer finding and connection management
//! - `SyncEngine`: Synchronization control
//! - `DataSyncBackend`: Lifecycle and composition
//!
//! These traits enable HIVE Protocol to work with multiple sync engines
//! (Ditto, Automerge, custom implementations) without changing business logic.

use crate::sync::types::*;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// Trait 1: Document Storage and Retrieval
///
/// Provides CRUD operations, queries, and live observers for documents.
/// Abstracts over backend-specific storage mechanisms.
#[async_trait]
pub trait DocumentStore: Send + Sync {
    /// Store or update a document
    ///
    /// If `document.id` is None, creates a new document with auto-generated ID.
    /// If `document.id` is Some, updates existing document or creates if not exists.
    ///
    /// Returns the document ID (generated or provided).
    async fn upsert(&self, collection: &str, document: Document) -> Result<DocumentId>;

    /// Retrieve documents matching a query
    ///
    /// Returns all documents in the collection that match the query criteria.
    /// Empty vector if no matches found.
    async fn query(&self, collection: &str, query: &Query) -> Result<Vec<Document>>;

    /// Remove a document by ID
    ///
    /// No-op if document doesn't exist (not an error).
    async fn remove(&self, collection: &str, doc_id: &DocumentId) -> Result<()>;

    /// Register observer for live updates
    ///
    /// Returns a stream that emits change events whenever documents matching
    /// the query are inserted, updated, or removed.
    ///
    /// The stream will first emit an `Initial` event with current matches,
    /// then emit `Updated` or `Removed` events as changes occur.
    fn observe(&self, collection: &str, query: &Query) -> Result<ChangeStream>;

    /// Get a single document by ID
    ///
    /// Convenience method equivalent to `query` with `Eq { field: "id", value }`.
    async fn get(&self, collection: &str, doc_id: &DocumentId) -> Result<Option<Document>> {
        let query = Query::Eq {
            field: "id".to_string(),
            value: Value::String(doc_id.clone()),
        };

        let docs = self.query(collection, &query).await?;
        Ok(docs.into_iter().next())
    }

    /// Count documents matching a query
    ///
    /// Default implementation queries and counts results.
    /// Backends may override with more efficient implementations.
    async fn count(&self, collection: &str, query: &Query) -> Result<usize> {
        let docs = self.query(collection, query).await?;
        Ok(docs.len())
    }

    // === Deletion methods (ADR-034) ===

    /// Delete a document according to collection policy (ADR-034)
    ///
    /// Behavior depends on the collection's DeletionPolicy:
    /// - ImplicitTTL: No-op (documents expire automatically)
    /// - Tombstone: Creates a tombstone record
    /// - SoftDelete: Marks document with _deleted=true
    /// - Immutable: Returns error
    ///
    /// Returns DeleteResult with details about what action was taken.
    async fn delete(
        &self,
        collection: &str,
        doc_id: &DocumentId,
        reason: Option<&str>,
    ) -> Result<crate::qos::DeleteResult> {
        // Default implementation: fall back to remove() with SoftDelete semantics
        let policy = self.deletion_policy(collection);

        if policy.is_immutable() {
            return Ok(crate::qos::DeleteResult::immutable());
        }

        // For non-tombstone policies, just use remove
        self.remove(collection, doc_id).await?;
        let _ = reason; // Unused in default impl

        Ok(crate::qos::DeleteResult::soft_deleted(policy))
    }

    /// Check if a document is deleted (tombstoned or soft-deleted)
    ///
    /// Returns true if:
    /// - Document has a tombstone record, OR
    /// - Document has _deleted=true field (soft delete)
    ///
    /// Returns false if document exists and is not deleted,
    /// or if document doesn't exist.
    async fn is_deleted(&self, collection: &str, doc_id: &DocumentId) -> Result<bool> {
        // Default: check if document exists with _deleted field
        if let Some(doc) = self.get(collection, doc_id).await? {
            if let Some(deleted) = doc.fields.get("_deleted") {
                return Ok(deleted.as_bool().unwrap_or(false));
            }
        }
        Ok(false)
    }

    /// Get the deletion policy for a collection
    ///
    /// Returns the configured DeletionPolicy for this collection.
    /// Default implementation returns SoftDelete for all collections.
    fn deletion_policy(&self, _collection: &str) -> crate::qos::DeletionPolicy {
        crate::qos::DeletionPolicy::default()
    }

    /// Get all tombstones for a collection
    ///
    /// Returns tombstones that haven't expired yet.
    /// Used for sync protocol to exchange deletion markers.
    async fn get_tombstones(&self, collection: &str) -> Result<Vec<crate::qos::Tombstone>> {
        // Default: no tombstones (backends override)
        let _ = collection;
        Ok(vec![])
    }

    /// Apply a tombstone received from sync
    ///
    /// Used by sync protocol to apply remote deletions.
    async fn apply_tombstone(&self, tombstone: &crate::qos::Tombstone) -> Result<()> {
        // Default: just remove the document
        self.remove(&tombstone.collection, &tombstone.document_id)
            .await
    }
}

/// Trait 2: Peer Discovery and Connection Management
///
/// Handles finding and connecting to other nodes in the mesh network.
/// Abstracts over different discovery mechanisms (mDNS, TCP, Bluetooth, etc).
#[async_trait]
pub trait PeerDiscovery: Send + Sync {
    /// Start discovery mechanism
    ///
    /// Begins advertising this node and listening for other nodes.
    /// Must be called before any peers can be discovered.
    async fn start(&self) -> Result<()>;

    /// Stop discovery
    ///
    /// Stops advertising and peer discovery.
    async fn stop(&self) -> Result<()>;

    /// Get list of discovered peers
    ///
    /// Returns all peers currently known (discovered and/or connected).
    async fn discovered_peers(&self) -> Result<Vec<PeerInfo>>;

    /// Manually add a peer by address
    ///
    /// Useful for connecting to known peers (e.g., TCP address).
    /// Complements automatic discovery.
    async fn add_peer(&self, address: &str, transport: TransportType) -> Result<()>;

    /// Wait for a specific peer to connect
    ///
    /// Blocks until the specified peer is connected or timeout occurs.
    /// Used in tests to wait for mesh formation.
    async fn wait_for_peer(&self, peer_id: &PeerId, timeout: Duration) -> Result<()>;

    /// Register callback for peer events
    ///
    /// Callback will be invoked whenever peers are discovered, connected,
    /// disconnected, or lost.
    ///
    /// Note: Callback must be Send + Sync as it may be called from any thread.
    fn on_peer_event(&self, callback: Box<dyn Fn(PeerEvent) + Send + Sync>);

    /// Get information about a specific peer
    async fn get_peer_info(&self, peer_id: &PeerId) -> Result<Option<PeerInfo>>;

    /// Check if a specific peer is currently connected
    async fn is_peer_connected(&self, peer_id: &PeerId) -> Result<bool> {
        Ok(self
            .get_peer_info(peer_id)
            .await?
            .map(|info| info.connected)
            .unwrap_or(false))
    }
}

/// Trait 3: Synchronization Control
///
/// Controls when and how documents are synchronized between peers.
/// Abstracts over different sync strategies and protocols.
#[async_trait]
pub trait SyncEngine: Send + Sync {
    /// Start synchronization with discovered peers
    ///
    /// Begins exchanging documents with connected peers.
    /// Discovery must be started first via `PeerDiscovery::start()`.
    async fn start_sync(&self) -> Result<()>;

    /// Stop synchronization
    ///
    /// Stops exchanging documents but maintains peer connections.
    async fn stop_sync(&self) -> Result<()>;

    /// Create sync subscription for a collection
    ///
    /// Tells the sync engine to actively synchronize documents in this collection.
    /// Without a subscription, documents may not sync (backend-dependent).
    ///
    /// The subscription keeps sync active while the returned handle is alive.
    /// Drop the handle to unsubscribe.
    async fn subscribe(&self, collection: &str, query: &Query) -> Result<SyncSubscription>;

    /// Set sync priority for a collection (optional)
    ///
    /// Backends that support priority-based sync can use this to prioritize
    /// certain collections over others.
    ///
    /// Default implementation is a no-op.
    async fn set_priority(&self, collection: &str, priority: Priority) -> Result<()> {
        // Default: no-op (not all backends support priority)
        let _ = (collection, priority);
        Ok(())
    }

    /// Check if sync is currently active
    async fn is_syncing(&self) -> Result<bool>;

    /// Force a sync round (push pending changes)
    ///
    /// Most backends sync automatically, but this can force immediate sync.
    /// Useful for testing or ensuring critical updates are sent.
    ///
    /// Default implementation is a no-op.
    async fn force_sync(&self) -> Result<()> {
        // Default: no-op (most backends sync automatically)
        Ok(())
    }

    /// Connect to a peer using their EndpointId and addresses (Issue #235)
    ///
    /// Establishes a connection to a peer with a known EndpointId and network addresses.
    /// Used for static peer configuration in containerlab and similar environments.
    ///
    /// # Arguments
    ///
    /// * `endpoint_id_hex` - The peer's EndpointId as a hex string (64 chars)
    /// * `addresses` - List of socket addresses (e.g., "192.168.1.1:12345")
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Connection established successfully
    /// * `Ok(false)` - Tie-breaking: peer will connect to us instead
    /// * `Err(e)` - Connection failed
    ///
    /// Default implementation returns Ok(false) for backends that don't support this.
    async fn connect_to_peer(&self, endpoint_id_hex: &str, addresses: &[String]) -> Result<bool> {
        let _ = (endpoint_id_hex, addresses);
        Ok(false)
    }
}

/// Trait 4: Lifecycle Management and Composition
///
/// Top-level trait that composes the other three traits and manages
/// backend initialization and shutdown.
#[async_trait]
pub trait DataSyncBackend: Send + Sync {
    /// Initialize backend with configuration
    ///
    /// Must be called before using any other methods.
    /// Sets up storage, networking, and prepares for sync.
    async fn initialize(&self, config: BackendConfig) -> Result<()>;

    /// Shutdown gracefully
    ///
    /// Stops sync, closes connections, flushes data to disk.
    /// Should be called before dropping the backend.
    async fn shutdown(&self) -> Result<()>;

    /// Get reference to document store implementation
    fn document_store(&self) -> Arc<dyn DocumentStore>;

    /// Get reference to peer discovery implementation
    fn peer_discovery(&self) -> Arc<dyn PeerDiscovery>;

    /// Get reference to sync engine implementation
    fn sync_engine(&self) -> Arc<dyn SyncEngine>;

    /// Check if backend is ready (initialized and not shut down)
    async fn is_ready(&self) -> bool {
        // Default: assume ready if this method can be called
        // Backends can override with more sophisticated checks
        true
    }

    /// Get backend name/version for debugging
    fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            name: "Unknown".to_string(),
            version: "0.0.0".to_string(),
        }
    }

    /// Get backend as Any for downcasting to concrete types
    ///
    /// Allows accessing backend-specific functionality not exposed through the trait.
    /// Used primarily for testing and advanced scenarios.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Information about a backend implementation
#[derive(Debug, Clone)]
pub struct BackendInfo {
    /// Backend name (e.g., "Ditto", "Automerge")
    pub name: String,

    /// Backend version
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::types::TransportConfig;
    use std::collections::HashMap;
    use std::time::SystemTime;

    // Test that traits are object-safe (can be used as trait objects)
    #[test]
    fn test_trait_object_safety() {
        // These should compile if traits are object-safe
        fn _takes_document_store(_: &dyn DocumentStore) {}
        fn _takes_peer_discovery(_: &dyn PeerDiscovery) {}
        fn _takes_sync_engine(_: &dyn SyncEngine) {}
        fn _takes_backend(_: &dyn DataSyncBackend) {}
    }

    #[test]
    fn test_backend_info_default() {
        // The default backend_info() method on DataSyncBackend
        struct Stub;
        impl Stub {
            fn backend_info(&self) -> BackendInfo {
                BackendInfo {
                    name: "Unknown".to_string(),
                    version: "0.0.0".to_string(),
                }
            }
        }
        let info = Stub.backend_info();
        assert_eq!(info.name, "Unknown");
        assert_eq!(info.version, "0.0.0");
    }

    #[test]
    fn test_backend_info_debug_clone() {
        let info = BackendInfo {
            name: "Automerge".to_string(),
            version: "0.7.0".to_string(),
        };
        let cloned = info.clone();
        assert_eq!(cloned.name, "Automerge");
        assert_eq!(format!("{:?}", info), format!("{:?}", cloned));
    }

    // --- Mock DocumentStore to test default methods ---

    struct MockDocStore {
        docs: std::sync::Mutex<HashMap<String, Vec<Document>>>,
    }

    impl MockDocStore {
        fn new() -> Self {
            Self {
                docs: std::sync::Mutex::new(HashMap::new()),
            }
        }

        fn insert(&self, collection: &str, doc: Document) {
            let mut docs = self.docs.lock().unwrap();
            docs.entry(collection.to_string()).or_default().push(doc);
        }
    }

    #[async_trait]
    impl DocumentStore for MockDocStore {
        async fn upsert(&self, collection: &str, document: Document) -> anyhow::Result<DocumentId> {
            let id = document.id.clone().unwrap_or_else(|| "auto-id".to_string());
            let mut doc = document;
            doc.id = Some(id.clone());
            self.insert(collection, doc);
            Ok(id)
        }

        async fn query(&self, collection: &str, query: &Query) -> anyhow::Result<Vec<Document>> {
            let docs = self.docs.lock().unwrap();
            let col = docs.get(collection).cloned().unwrap_or_default();
            match query {
                Query::All => Ok(col),
                Query::Eq { field, value } => Ok(col
                    .into_iter()
                    .filter(|d| {
                        if field == "id" {
                            d.id.as_deref() == value.as_str()
                        } else {
                            d.fields.get(field.as_str()) == Some(value)
                        }
                    })
                    .collect()),
                _ => Ok(col),
            }
        }

        async fn remove(&self, collection: &str, doc_id: &DocumentId) -> anyhow::Result<()> {
            let mut docs = self.docs.lock().unwrap();
            if let Some(col) = docs.get_mut(collection) {
                col.retain(|d| d.id.as_deref() != Some(doc_id.as_str()));
            }
            Ok(())
        }

        fn observe(&self, _collection: &str, _query: &Query) -> anyhow::Result<ChangeStream> {
            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
            Ok(ChangeStream { receiver: rx })
        }
    }

    #[tokio::test]
    async fn test_document_store_get_found() {
        let store = MockDocStore::new();
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), Value::String("test".to_string()));
        let doc = Document::with_id("doc1", fields);
        store.insert("col", doc);

        let result = store.get("col", &"doc1".to_string()).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, Some("doc1".to_string()));
    }

    #[tokio::test]
    async fn test_document_store_get_not_found() {
        let store = MockDocStore::new();
        let result = store.get("col", &"missing".to_string()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_document_store_count() {
        let store = MockDocStore::new();
        store.insert("col", Document::with_id("a", HashMap::new()));
        store.insert("col", Document::with_id("b", HashMap::new()));

        let count = store.count("col", &Query::All).await.unwrap();
        assert_eq!(count, 2);

        let count = store.count("empty", &Query::All).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_document_store_delete_default() {
        let store = MockDocStore::new();
        store.insert("col", Document::with_id("d1", HashMap::new()));
        let result = store
            .delete("col", &"d1".to_string(), Some("test reason"))
            .await
            .unwrap();
        // Default deletion_policy is SoftDelete, so delete should succeed
        assert!(result.deleted);
    }

    #[tokio::test]
    async fn test_document_store_is_deleted_false() {
        let store = MockDocStore::new();
        store.insert("col", Document::with_id("d1", HashMap::new()));

        let deleted = store.is_deleted("col", &"d1".to_string()).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_document_store_is_deleted_true() {
        let store = MockDocStore::new();
        let mut fields = HashMap::new();
        fields.insert("_deleted".to_string(), Value::Bool(true));
        store.insert("col", Document::with_id("d1", fields));

        let deleted = store.is_deleted("col", &"d1".to_string()).await.unwrap();
        assert!(deleted);
    }

    #[tokio::test]
    async fn test_document_store_is_deleted_nonexistent() {
        let store = MockDocStore::new();
        let deleted = store
            .is_deleted("col", &"missing".to_string())
            .await
            .unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_document_store_get_tombstones_default() {
        let store = MockDocStore::new();
        let tombstones = store.get_tombstones("col").await.unwrap();
        assert!(tombstones.is_empty());
    }

    #[tokio::test]
    async fn test_document_store_apply_tombstone_default() {
        let store = MockDocStore::new();
        store.insert("col", Document::with_id("d1", HashMap::new()));

        let tombstone = crate::qos::Tombstone {
            collection: "col".to_string(),
            document_id: "d1".to_string(),
            deleted_at: SystemTime::now(),
            deleted_by: "user-1".to_string(),
            lamport: 1,
            reason: None,
        };
        store.apply_tombstone(&tombstone).await.unwrap();

        // Doc should be removed
        let result = store.get("col", &"d1".to_string()).await.unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_deletion_policy_default() {
        let store = MockDocStore::new();
        let policy = store.deletion_policy("any_collection");
        assert!(policy.is_soft_delete());
    }

    // --- Mock SyncEngine to test default methods ---

    struct MockSyncEngine;

    #[async_trait]
    impl SyncEngine for MockSyncEngine {
        async fn start_sync(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn stop_sync(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn subscribe(
            &self,
            _collection: &str,
            _query: &Query,
        ) -> anyhow::Result<SyncSubscription> {
            Ok(SyncSubscription::new("test", ()))
        }
        async fn is_syncing(&self) -> anyhow::Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn test_sync_engine_set_priority_default_noop() {
        let engine = MockSyncEngine;
        let result = engine.set_priority("col", Priority::High).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sync_engine_force_sync_default_noop() {
        let engine = MockSyncEngine;
        let result = engine.force_sync().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sync_engine_connect_to_peer_default() {
        let engine = MockSyncEngine;
        let result = engine
            .connect_to_peer("abcd1234", &["192.168.1.1:5000".to_string()])
            .await
            .unwrap();
        assert!(!result); // Default returns false
    }

    // --- Mock PeerDiscovery to test default method ---

    fn make_peer_info(peer_id: &str, connected: bool) -> PeerInfo {
        PeerInfo {
            peer_id: peer_id.to_string(),
            address: None,
            transport: TransportType::Tcp,
            connected,
            last_seen: SystemTime::now(),
            metadata: HashMap::new(),
        }
    }

    struct MockPeerDiscovery;

    #[async_trait]
    impl PeerDiscovery for MockPeerDiscovery {
        async fn start(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn stop(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn discovered_peers(&self) -> anyhow::Result<Vec<PeerInfo>> {
            Ok(vec![make_peer_info("peer-1", true)])
        }
        async fn add_peer(&self, _address: &str, _transport: TransportType) -> anyhow::Result<()> {
            Ok(())
        }
        async fn wait_for_peer(&self, _peer_id: &PeerId, _timeout: Duration) -> anyhow::Result<()> {
            Ok(())
        }
        fn on_peer_event(&self, _callback: Box<dyn Fn(PeerEvent) + Send + Sync>) {}
        async fn get_peer_info(&self, peer_id: &PeerId) -> anyhow::Result<Option<PeerInfo>> {
            if peer_id == "peer-1" {
                Ok(Some(make_peer_info("peer-1", true)))
            } else if peer_id == "peer-disconnected" {
                Ok(Some(make_peer_info("peer-disconnected", false)))
            } else {
                Ok(None)
            }
        }
    }

    #[tokio::test]
    async fn test_peer_discovery_is_connected_true() {
        let disc = MockPeerDiscovery;
        let connected = disc.is_peer_connected(&"peer-1".to_string()).await.unwrap();
        assert!(connected);
    }

    #[tokio::test]
    async fn test_peer_discovery_is_connected_false_disconnected() {
        let disc = MockPeerDiscovery;
        let connected = disc
            .is_peer_connected(&"peer-disconnected".to_string())
            .await
            .unwrap();
        assert!(!connected);
    }

    #[tokio::test]
    async fn test_peer_discovery_is_connected_false_unknown() {
        let disc = MockPeerDiscovery;
        let connected = disc
            .is_peer_connected(&"unknown".to_string())
            .await
            .unwrap();
        assert!(!connected);
    }

    // --- DataSyncBackend default methods ---

    struct MockBackend;

    #[async_trait]
    impl DataSyncBackend for MockBackend {
        async fn initialize(&self, _config: BackendConfig) -> anyhow::Result<()> {
            Ok(())
        }
        async fn shutdown(&self) -> anyhow::Result<()> {
            Ok(())
        }
        fn document_store(&self) -> Arc<dyn DocumentStore> {
            Arc::new(MockDocStore::new())
        }
        fn peer_discovery(&self) -> Arc<dyn PeerDiscovery> {
            Arc::new(MockPeerDiscovery)
        }
        fn sync_engine(&self) -> Arc<dyn SyncEngine> {
            Arc::new(MockSyncEngine)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[tokio::test]
    async fn test_data_sync_backend_is_ready_default() {
        let backend = MockBackend;
        assert!(backend.is_ready().await);
    }

    #[test]
    fn test_data_sync_backend_backend_info_default() {
        let backend = MockBackend;
        let info = backend.backend_info();
        assert_eq!(info.name, "Unknown");
        assert_eq!(info.version, "0.0.0");
    }

    #[test]
    fn test_data_sync_backend_as_any() {
        let backend = MockBackend;
        let any = backend.as_any();
        assert!(any.downcast_ref::<MockBackend>().is_some());
    }

    #[tokio::test]
    async fn test_data_sync_backend_accessors() {
        let backend = MockBackend;
        let _store = backend.document_store();
        let _disc = backend.peer_discovery();
        let _engine = backend.sync_engine();
    }

    #[tokio::test]
    async fn test_document_store_delete_immutable_policy() {
        #[allow(dead_code)]
        struct ImmutableDocStore {
            docs: std::sync::Mutex<HashMap<String, Vec<Document>>>,
        }

        impl ImmutableDocStore {
            fn new() -> Self {
                Self {
                    docs: std::sync::Mutex::new(HashMap::new()),
                }
            }
        }

        #[async_trait]
        impl DocumentStore for ImmutableDocStore {
            async fn upsert(
                &self,
                _collection: &str,
                _document: Document,
            ) -> anyhow::Result<DocumentId> {
                Ok("id".to_string())
            }
            async fn query(
                &self,
                _collection: &str,
                _query: &Query,
            ) -> anyhow::Result<Vec<Document>> {
                Ok(vec![])
            }
            async fn remove(
                &self,
                _collection: &str,
                _doc_id: &DocumentId,
            ) -> anyhow::Result<()> {
                Ok(())
            }
            fn observe(
                &self,
                _collection: &str,
                _query: &Query,
            ) -> anyhow::Result<ChangeStream> {
                let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
                Ok(ChangeStream { receiver: rx })
            }
            fn deletion_policy(&self, _collection: &str) -> crate::qos::DeletionPolicy {
                crate::qos::DeletionPolicy::Immutable
            }
        }

        let store = ImmutableDocStore::new();
        let result = store
            .delete("col", &"doc1".to_string(), None)
            .await
            .unwrap();
        assert!(!result.deleted);
    }

    #[tokio::test]
    async fn test_document_store_upsert_auto_id() {
        let store = MockDocStore::new();
        // Upsert with no ID → generates "auto-id"
        let doc = Document {
            id: None,
            fields: HashMap::new(),
            updated_at: SystemTime::now(),
        };
        let id = store.upsert("col", doc).await.unwrap();
        assert_eq!(id, "auto-id");

        // Verify stored
        let result = store.get("col", &"auto-id".to_string()).await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_document_store_query_field_match() {
        let store = MockDocStore::new();
        let mut fields = HashMap::new();
        fields.insert("status".to_string(), Value::String("active".to_string()));
        store.insert("col", Document::with_id("d1", fields.clone()));

        let mut fields2 = HashMap::new();
        fields2.insert("status".to_string(), Value::String("inactive".to_string()));
        store.insert("col", Document::with_id("d2", fields2));

        // Query by field value (non-id field)
        let results = store
            .query(
                "col",
                &Query::Eq {
                    field: "status".to_string(),
                    value: Value::String("active".to_string()),
                },
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, Some("d1".to_string()));
    }

    #[tokio::test]
    async fn test_document_store_query_other_variant() {
        let store = MockDocStore::new();
        store.insert("col", Document::with_id("d1", HashMap::new()));
        store.insert("col", Document::with_id("d2", HashMap::new()));

        // Use a query variant other than All or Eq (catches the _ arm in mock)
        let results = store
            .query(
                "col",
                &Query::Gt {
                    field: "x".to_string(),
                    value: serde_json::json!(0),
                },
            )
            .await
            .unwrap();
        // The _ arm returns all docs
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_document_store_delete_with_none_reason() {
        let store = MockDocStore::new();
        store.insert("col", Document::with_id("d1", HashMap::new()));
        let result = store
            .delete("col", &"d1".to_string(), None)
            .await
            .unwrap();
        assert!(result.deleted);
    }

    #[tokio::test]
    async fn test_document_store_remove_nonexistent() {
        let store = MockDocStore::new();
        // Remove from nonexistent collection
        store.remove("col", &"missing".to_string()).await.unwrap();
    }

    #[tokio::test]
    async fn test_document_store_is_deleted_with_non_bool_field() {
        let store = MockDocStore::new();
        let mut fields = HashMap::new();
        fields.insert("_deleted".to_string(), Value::String("not-a-bool".to_string()));
        store.insert("col", Document::with_id("d1", fields));

        // Value is not a bool, so should return false
        let deleted = store.is_deleted("col", &"d1".to_string()).await.unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_mock_doc_store_observe() {
        let store = MockDocStore::new();
        let stream = store.observe("col", &Query::All);
        assert!(stream.is_ok());
    }

    #[tokio::test]
    async fn test_mock_peer_discovery_methods() {
        let disc = MockPeerDiscovery;
        disc.start().await.unwrap();
        disc.stop().await.unwrap();

        let peers = disc.discovered_peers().await.unwrap();
        assert_eq!(peers.len(), 1);

        disc.add_peer("10.0.0.1:5000", TransportType::Tcp)
            .await
            .unwrap();
        disc.wait_for_peer(&"peer-1".to_string(), Duration::from_secs(1))
            .await
            .unwrap();
        disc.on_peer_event(Box::new(|_| {}));

        let info = disc.get_peer_info(&"peer-1".to_string()).await.unwrap();
        assert!(info.is_some());
    }

    #[tokio::test]
    async fn test_mock_sync_engine_methods() {
        let engine = MockSyncEngine;
        engine.start_sync().await.unwrap();
        engine.stop_sync().await.unwrap();
        let sub = engine.subscribe("col", &Query::All).await.unwrap();
        assert_eq!(sub.collection(), "test");
        assert!(engine.is_syncing().await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_backend_lifecycle() {
        let backend = MockBackend;
        backend
            .initialize(BackendConfig {
                app_id: "test-app".to_string(),
                persistence_dir: std::path::PathBuf::from("/tmp/test"),
                shared_key: None,
                transport: TransportConfig::default(),
                extra: HashMap::new(),
            })
            .await
            .unwrap();
        backend.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_immutable_doc_store_methods() {
        #[allow(dead_code)]
        struct ImmutableStore {
            docs: std::sync::Mutex<HashMap<String, Vec<Document>>>,
        }

        impl ImmutableStore {
            fn new() -> Self {
                Self {
                    docs: std::sync::Mutex::new(HashMap::new()),
                }
            }
        }

        #[async_trait]
        impl DocumentStore for ImmutableStore {
            async fn upsert(
                &self,
                _collection: &str,
                _document: Document,
            ) -> anyhow::Result<DocumentId> {
                Ok("id".to_string())
            }
            async fn query(
                &self,
                _collection: &str,
                _query: &Query,
            ) -> anyhow::Result<Vec<Document>> {
                Ok(vec![])
            }
            async fn remove(
                &self,
                _collection: &str,
                _doc_id: &DocumentId,
            ) -> anyhow::Result<()> {
                Ok(())
            }
            fn observe(
                &self,
                _collection: &str,
                _query: &Query,
            ) -> anyhow::Result<ChangeStream> {
                let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
                Ok(ChangeStream { receiver: rx })
            }
            fn deletion_policy(&self, _collection: &str) -> crate::qos::DeletionPolicy {
                crate::qos::DeletionPolicy::Immutable
            }
        }

        let store = ImmutableStore::new();
        // Exercise all methods
        let id = store.upsert("col", Document::with_id("x", HashMap::new())).await.unwrap();
        assert_eq!(id, "id");
        let docs = store.query("col", &Query::All).await.unwrap();
        assert!(docs.is_empty());
        store.remove("col", &"x".to_string()).await.unwrap();
        let _stream = store.observe("col", &Query::All).unwrap();

        // Test immutable delete
        let result = store.delete("col", &"doc1".to_string(), None).await.unwrap();
        assert!(!result.deleted);
    }
}
