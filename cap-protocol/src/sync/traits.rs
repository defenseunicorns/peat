//! Core trait definitions for data synchronization abstraction
//!
//! This module defines the four fundamental traits that any sync backend must implement:
//! - `DocumentStore`: CRUD operations and queries
//! - `PeerDiscovery`: Peer finding and connection management
//! - `SyncEngine`: Synchronization control
//! - `DataSyncBackend`: Lifecycle and composition
//!
//! These traits enable CAP Protocol to work with multiple sync engines
//! (Ditto, Automerge, custom implementations) without changing business logic.

use crate::sync::types::*;
use crate::Result;
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

    // Test that traits are object-safe (can be used as trait objects)
    #[test]
    fn test_trait_object_safety() {
        // These should compile if traits are object-safe
        fn _takes_document_store(_: &dyn DocumentStore) {}
        fn _takes_peer_discovery(_: &dyn PeerDiscovery) {}
        fn _takes_sync_engine(_: &dyn SyncEngine) {}
        fn _takes_backend(_: &dyn DataSyncBackend) {}
    }
}
