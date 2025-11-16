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
use super::automerge_conversion::{automerge_to_message, message_to_automerge};
#[cfg(feature = "automerge-backend")]
use super::automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
use super::automerge_sync::AutomergeSyncCoordinator;
#[cfg(feature = "automerge-backend")]
use super::capabilities::{CrdtCapable, SyncCapable, SyncStats, TypedCollection};
#[cfg(feature = "automerge-backend")]
use super::traits::{Collection, StorageBackend};
#[cfg(feature = "automerge-backend")]
use crate::network::iroh_transport::IrohTransport;
#[cfg(feature = "automerge-backend")]
use anyhow::Result;
#[cfg(feature = "automerge-backend")]
use prost::Message as ProstMessage;
#[cfg(feature = "automerge-backend")]
use serde::{de::DeserializeOwned, Serialize};
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::marker::PhantomData;
#[cfg(feature = "automerge-backend")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "automerge-backend")]
use tokio::task::JoinHandle;

/// Automerge backend adapter implementing StorageBackend trait
///
/// Wraps AutomergeStore to provide trait-based interface for backend-agnostic code.
///
/// # Complete Implementation (Phases 1-6)
///
/// - ✅ RocksDB persistence with LRU cache
/// - ✅ CRDT field-level semantics via Automerge
/// - ✅ P2P sync via Iroh QUIC transport
/// - ✅ Background sync coordination
/// - ✅ SyncCapable trait for lifecycle management
/// - ✅ Incoming sync handler (Phase 6.2)
#[cfg(feature = "automerge-backend")]
pub struct AutomergeBackend {
    /// Underlying AutomergeStore instance
    store: Arc<AutomergeStore>,
    /// Cache of known collection names
    collections: Arc<RwLock<HashMap<String, Arc<dyn Collection>>>>,
    /// Optional Iroh transport for P2P sync (Phase 5)
    transport: Option<Arc<IrohTransport>>,
    /// Optional sync coordinator (Phase 5)
    sync_coordinator: Option<Arc<AutomergeSyncCoordinator>>,
    /// Sync state tracking
    sync_active: Arc<AtomicBool>,
    /// Bytes sent counter
    bytes_sent: Arc<AtomicU64>,
    /// Bytes received counter
    bytes_received: Arc<AtomicU64>,
    /// Incoming sync handler task handle (Phase 6.2)
    incoming_handler_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    /// Automatic sync task handle (Phase 6.3)
    auto_sync_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeBackend {
    /// Create a new Automerge backend from an existing AutomergeStore
    ///
    /// This creates a backend without P2P sync capabilities.
    /// For sync support, use `with_transport()` instead.
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
            transport: None,
            sync_coordinator: None,
            sync_active: Arc::new(AtomicBool::new(false)),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            incoming_handler_task: Arc::new(RwLock::new(None)),
            auto_sync_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new Automerge backend with P2P sync capabilities
    ///
    /// # Arguments
    ///
    /// * `store` - Configured AutomergeStore instance
    /// * `transport` - IrohTransport for P2P networking
    ///
    /// # Example
    ///
    /// ```ignore
    /// use cap_protocol::storage::{AutomergeBackend, AutomergeStore};
    /// use cap_protocol::network::IrohTransport;
    /// use std::sync::Arc;
    ///
    /// let store = Arc::new(AutomergeStore::open("./data/automerge")?);
    /// let transport = Arc::new(IrohTransport::new().await?);
    /// let backend = AutomergeBackend::with_transport(store, transport);
    /// ```
    pub fn with_transport(store: Arc<AutomergeStore>, transport: Arc<IrohTransport>) -> Self {
        let coordinator = Arc::new(AutomergeSyncCoordinator::new(
            Arc::clone(&store),
            Arc::clone(&transport),
        ));

        Self {
            store,
            collections: Arc::new(RwLock::new(HashMap::new())),
            transport: Some(transport),
            sync_coordinator: Some(coordinator),
            sync_active: Arc::new(AtomicBool::new(false)),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            incoming_handler_task: Arc::new(RwLock::new(None)),
            auto_sync_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Get access to underlying AutomergeStore for store-specific operations
    ///
    /// This provides an escape hatch for features not yet abstracted by the trait.
    pub fn automerge_store(&self) -> &AutomergeStore {
        &self.store
    }

    /// Manually trigger sync for a specific document with all connected peers
    ///
    /// This is useful for testing or for explicit sync triggering.
    /// In production, the background sync task will handle this automatically.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The full document key (e.g., "nodes:node-1")
    pub async fn sync_document(&self, doc_key: &str) -> Result<()> {
        if let Some(coordinator) = &self.sync_coordinator {
            coordinator.sync_document_with_all_peers(doc_key).await
        } else {
            anyhow::bail!("Cannot sync: backend created without transport")
        }
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

/// Typed collection for Automerge backend with CRDT semantics
///
/// Stores protobuf messages as Automerge CRDT documents with field-level merging.
#[cfg(feature = "automerge-backend")]
pub struct AutomergeTypedCollection<M> {
    store: Arc<AutomergeStore>,
    prefix: String,
    _phantom: PhantomData<M>,
}

#[cfg(feature = "automerge-backend")]
impl<M> AutomergeTypedCollection<M>
where
    M: ProstMessage + Serialize + DeserializeOwned + Default + Clone,
{
    fn new(store: Arc<AutomergeStore>, collection_name: &str) -> Self {
        Self {
            store,
            prefix: format!("{}:", collection_name),
            _phantom: PhantomData,
        }
    }

    fn prefixed_key(&self, doc_id: &str) -> String {
        format!("{}{}", self.prefix, doc_id)
    }

    fn strip_prefix<'a>(&self, key: &'a str) -> Option<&'a str> {
        key.strip_prefix(&self.prefix)
    }
}

#[cfg(feature = "automerge-backend")]
impl<M> TypedCollection<M> for AutomergeTypedCollection<M>
where
    M: ProstMessage + Serialize + DeserializeOwned + Default + Clone,
{
    fn upsert(&self, doc_id: &str, message: &M) -> Result<()> {
        // Convert message to Automerge document with CRDT semantics
        let doc = message_to_automerge(message)?;
        self.store.put(&self.prefixed_key(doc_id), &doc)
    }

    fn get(&self, doc_id: &str) -> Result<Option<M>> {
        match self.store.get(&self.prefixed_key(doc_id))? {
            Some(doc) => {
                let message = automerge_to_message(&doc)?;
                Ok(Some(message))
            }
            None => Ok(None),
        }
    }

    fn delete(&self, doc_id: &str) -> Result<()> {
        self.store.delete(&self.prefixed_key(doc_id))
    }

    fn scan(&self) -> Result<Vec<(String, M)>> {
        let docs = self.store.scan_prefix(&self.prefix)?;
        let mut results = Vec::new();

        for (key, doc) in docs {
            if let Some(doc_id) = self.strip_prefix(&key) {
                let message = automerge_to_message(&doc)?;
                results.push((doc_id.to_string(), message));
            }
        }

        Ok(results)
    }

    fn find(&self, predicate: Box<dyn Fn(&M) -> bool + Send>) -> Result<Vec<(String, M)>> {
        let all_docs = self.scan()?;
        Ok(all_docs
            .into_iter()
            .filter(|(_, msg)| predicate(msg))
            .collect())
    }

    fn count(&self) -> Result<usize> {
        Ok(self.scan()?.len())
    }
}

/// Implement CrdtCapable trait to provide typed collections with CRDT semantics
#[cfg(feature = "automerge-backend")]
impl CrdtCapable for AutomergeBackend {
    fn typed_collection<M>(&self, name: &str) -> Arc<dyn TypedCollection<M>>
    where
        M: ProstMessage + Serialize + DeserializeOwned + Default + Clone + 'static,
    {
        Arc::new(AutomergeTypedCollection::new(Arc::clone(&self.store), name))
    }
}

/// Implement SyncCapable trait for background synchronization
///
/// Phase 5: Provides lifecycle management for P2P sync with Iroh transport.
#[cfg(feature = "automerge-backend")]
impl SyncCapable for AutomergeBackend {
    fn start_sync(&self) -> Result<()> {
        // Check if transport is available
        if self.transport.is_none() || self.sync_coordinator.is_none() {
            anyhow::bail!(
                "Cannot start sync: backend created without transport (use with_transport())"
            );
        }

        // Check if already syncing
        if self
            .sync_active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            anyhow::bail!("Sync already active");
        }

        // Phase 6.1: Start accept loop to receive incoming connections
        if let Some(transport) = &self.transport {
            // Only start if not already running (may have been started by initialize())
            if !transport.is_accept_loop_running() {
                transport.start_accept_loop()?;
            }
        }

        // Phase 6.2: Spawn incoming sync handler task
        //
        // Note: For Phase 6.2, we rely on manual sync triggering via sync_document().
        // The incoming handler is invoked per-stream when a peer sends us a sync message.
        // The accept loop in IrohTransport accepts connections, and we need a stream
        // accept loop for each connection to handle incoming sync messages.
        //
        // For now, we'll implement a simple polling-based handler that checks for
        // incoming streams on all connected peers.
        let transport = self.transport.clone().unwrap();
        let coordinator = self.sync_coordinator.clone().unwrap();
        let sync_active = Arc::clone(&self.sync_active);

        let task = tokio::spawn(async move {
            while sync_active.load(Ordering::Relaxed) {
                // Get all connected peers
                let peer_ids = transport.connected_peers();

                for peer_id in peer_ids {
                    // Get connection for this peer
                    if let Some(conn) = transport.get_connection(&peer_id) {
                        // Clone for the async block
                        let coordinator_clone = Arc::clone(&coordinator);
                        let conn_clone = conn.clone();

                        // Spawn a task to accept incoming streams on this connection
                        tokio::spawn(async move {
                            if let Err(e) = coordinator_clone.handle_incoming_sync(conn_clone).await
                            {
                                tracing::debug!("Error handling incoming sync: {}", e);
                            }
                        });
                    }
                }

                // Small delay to avoid busy loop
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }

            tracing::debug!("Incoming sync handler stopped");
        });

        *self.incoming_handler_task.write().unwrap() = Some(task);

        // Phase 6.3: Spawn automatic sync task for outgoing sync
        //
        // Subscribe to document change notifications and automatically sync
        // changed documents with all connected peers.
        if let Some(mut change_rx) = self.store.subscribe_to_changes() {
            let coordinator = self.sync_coordinator.clone().unwrap();
            let sync_active = Arc::clone(&self.sync_active);

            let auto_task = tokio::spawn(async move {
                tracing::debug!("Automatic sync task started");

                while sync_active.load(Ordering::Relaxed) {
                    // Wait for change notification
                    match change_rx.recv().await {
                        Some(doc_key) => {
                            tracing::debug!("Document changed: {}, triggering sync", doc_key);

                            // Sync with all connected peers
                            if let Err(e) = coordinator.sync_document_with_all_peers(&doc_key).await
                            {
                                tracing::warn!(
                                    "Failed to sync document {} after change: {}",
                                    doc_key,
                                    e
                                );
                            }
                        }
                        None => {
                            // Channel closed
                            tracing::debug!("Change notification channel closed");
                            break;
                        }
                    }
                }

                tracing::debug!("Automatic sync task stopped");
            });

            *self.auto_sync_task.write().unwrap() = Some(auto_task);
        } else {
            tracing::warn!("Could not subscribe to store changes - automatic sync disabled");
        }

        Ok(())
    }

    fn stop_sync(&self) -> Result<()> {
        // Mark as inactive
        if !self.sync_active.swap(false, Ordering::SeqCst) {
            anyhow::bail!("Sync is not active");
        }

        // Phase 6.1: Stop accept loop
        if let Some(transport) = &self.transport {
            // Ignore error if accept loop already stopped
            let _ = transport.stop_accept_loop();
        }

        // TODO Phase 6.2: Signal background sync task to stop and wait for completion

        Ok(())
    }

    fn sync_stats(&self) -> Result<SyncStats> {
        let peer_count = self.transport.as_ref().map(|t| t.peer_count()).unwrap_or(0);

        Ok(SyncStats {
            peer_count,
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            last_sync: None, // TODO Phase 6: Track last sync timestamp in coordinator
        })
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

    // Phase 2: CRDT Integration Tests
    use cap_schema::common::v1::Position;
    use cap_schema::node::v1::NodeState;

    #[test]
    fn test_typed_collection_crdt_upsert_get() {
        use crate::storage::capabilities::CrdtCapable;

        let (backend, _temp) = create_test_backend();
        let nodes: Arc<dyn TypedCollection<NodeState>> = backend.typed_collection("nodes");

        let node = NodeState {
            position: Some(Position {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: 100.0,
            }),
            fuel_minutes: 60,
            health: 1,
            phase: 1,
            cell_id: Some("cell-1".to_string()),
            zone_id: None,
            timestamp: None,
        };

        nodes.upsert("node-1", &node).unwrap();
        let retrieved = nodes.get("node-1").unwrap().unwrap();

        assert_eq!(retrieved.fuel_minutes, 60);
        assert_eq!(retrieved.cell_id, Some("cell-1".to_string()));
        assert!(retrieved.position.is_some());
    }

    #[test]
    fn test_typed_collection_crdt_scan() {
        use crate::storage::capabilities::CrdtCapable;

        let (backend, _temp) = create_test_backend();
        let nodes: Arc<dyn TypedCollection<NodeState>> = backend.typed_collection("nodes");

        let node1 = NodeState {
            fuel_minutes: 60,
            health: 1,
            phase: 1,
            cell_id: Some("cell-1".to_string()),
            ..Default::default()
        };

        let node2 = NodeState {
            fuel_minutes: 45,
            health: 1,
            phase: 2,
            cell_id: Some("cell-2".to_string()),
            ..Default::default()
        };

        nodes.upsert("node-1", &node1).unwrap();
        nodes.upsert("node-2", &node2).unwrap();

        let results = nodes.scan().unwrap();
        assert_eq!(results.len(), 2);

        let ids: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();
        assert!(ids.contains(&"node-1".to_string()));
        assert!(ids.contains(&"node-2".to_string()));
    }

    #[test]
    fn test_typed_collection_crdt_find_with_predicate() {
        use crate::storage::capabilities::CrdtCapable;

        let (backend, _temp) = create_test_backend();
        let nodes: Arc<dyn TypedCollection<NodeState>> = backend.typed_collection("nodes");

        let node1 = NodeState {
            fuel_minutes: 60,
            health: 1,
            phase: 1,
            cell_id: Some("cell-1".to_string()),
            ..Default::default()
        };

        let node2 = NodeState {
            fuel_minutes: 30,
            health: 1,
            phase: 1,
            cell_id: Some("cell-1".to_string()),
            ..Default::default()
        };

        let node3 = NodeState {
            fuel_minutes: 45,
            health: 1,
            phase: 1,
            cell_id: Some("cell-2".to_string()),
            ..Default::default()
        };

        nodes.upsert("node-1", &node1).unwrap();
        nodes.upsert("node-2", &node2).unwrap();
        nodes.upsert("node-3", &node3).unwrap();

        // Find nodes with low fuel
        let low_fuel_nodes = nodes.find(Box::new(|node| node.fuel_minutes < 40)).unwrap();

        assert_eq!(low_fuel_nodes.len(), 1);
        assert_eq!(low_fuel_nodes[0].1.fuel_minutes, 30);
    }

    #[test]
    fn test_typed_collection_delete() {
        use crate::storage::capabilities::CrdtCapable;

        let (backend, _temp) = create_test_backend();
        let nodes: Arc<dyn TypedCollection<NodeState>> = backend.typed_collection("nodes");

        let node = NodeState {
            fuel_minutes: 60,
            ..Default::default()
        };

        nodes.upsert("node-1", &node).unwrap();
        assert!(nodes.get("node-1").unwrap().is_some());

        nodes.delete("node-1").unwrap();
        assert!(nodes.get("node-1").unwrap().is_none());
    }

    // Phase 5: SyncCapable Trait Tests

    #[tokio::test]
    async fn test_backend_without_transport_cannot_sync() {
        use crate::storage::capabilities::SyncCapable;

        let (backend, _temp) = create_test_backend();

        // Should fail - no transport configured
        let result = backend.start_sync();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("without transport"));
    }

    #[tokio::test]
    async fn test_backend_with_transport_sync_lifecycle() {
        use crate::network::IrohTransport;
        use crate::storage::capabilities::SyncCapable;

        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(IrohTransport::new().await.unwrap());
        let backend = AutomergeBackend::with_transport(store, transport);

        // Start sync should succeed
        assert!(backend.start_sync().is_ok());

        // Starting again should fail
        let result = backend.start_sync();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already active"));

        // Stop should succeed
        assert!(backend.stop_sync().is_ok());

        // Stopping again should fail
        let result = backend.stop_sync();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not active"));
    }

    #[tokio::test]
    async fn test_sync_stats_without_transport() {
        use crate::storage::capabilities::SyncCapable;

        let (backend, _temp) = create_test_backend();

        let stats = backend.sync_stats().unwrap();
        assert_eq!(stats.peer_count, 0);
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
        assert!(stats.last_sync.is_none());
    }

    #[tokio::test]
    async fn test_sync_stats_with_transport() {
        use crate::network::IrohTransport;
        use crate::storage::capabilities::SyncCapable;

        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(IrohTransport::new().await.unwrap());
        let backend = AutomergeBackend::with_transport(store, transport);

        let stats = backend.sync_stats().unwrap();
        assert_eq!(stats.peer_count, 0); // No peers connected yet
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
    }
}
