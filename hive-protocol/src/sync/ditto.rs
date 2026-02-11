//! Ditto backend implementation
//!
//! Wraps the Ditto SDK to implement the `DataSyncBackend` traits,
//! providing a bridge between HIVE Protocol's abstraction layer
//! and Ditto's proprietary CRDT sync engine.

use crate::storage::ditto_store::{DittoConfig, DittoStore};
use crate::sync::{
    BackendConfig, BackendInfo, ChangeStream, DataSyncBackend, Document, DocumentId, DocumentStore,
    PeerDiscovery, PeerEvent, PeerId, PeerInfo, Query, SyncEngine, SyncSubscription, TransportType,
    Value,
};
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;

/// Type alias for peer event callbacks to simplify complex type
type PeerCallbacks = Arc<Mutex<Vec<Box<dyn Fn(PeerEvent) + Send + Sync>>>>;

/// Ditto backend implementation
///
/// Wraps the existing DittoStore to provide trait-based abstraction.
pub struct DittoBackend {
    /// Underlying Ditto store (None until initialized)
    /// Wrapped in Arc so all operations use the same instance
    store: Arc<Mutex<Option<Arc<DittoStore>>>>,

    /// Peer event callbacks
    peer_callbacks: PeerCallbacks,
}

impl DittoBackend {
    /// Create a new Ditto backend
    ///
    /// Note: Must call `initialize()` before use.
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(None)),
            peer_callbacks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a Ditto backend from an existing DittoStore
    ///
    /// This is useful for tests that create stores directly.
    pub fn from_store(store: DittoStore) -> Self {
        Self {
            store: Arc::new(Mutex::new(Some(Arc::new(store)))),
            peer_callbacks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get a reference to the underlying store (if initialized)
    ///
    /// Returns Arc to ensure all operations use the same DittoStore instance.
    /// This is critical for peer discovery to work correctly.
    fn get_store(&self) -> Result<Arc<DittoStore>> {
        self.store
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| Error::config_error("Backend not initialized", None))
    }

    /// Get a reference to the underlying DittoStore for testing/debugging
    ///
    /// Exposes the Arc-wrapped DittoStore for tests that need direct access.
    pub fn get_ditto_store(&self) -> Result<Arc<DittoStore>> {
        self.get_store()
    }
}

impl Default for DittoBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSyncBackend for DittoBackend {
    async fn initialize(&self, config: BackendConfig) -> anyhow::Result<()> {
        // Get offline_token from extra config or environment (via HiveCredentials)
        let offline_token = config.extra.get("offline_token").cloned()
            .or_else(|| {
                crate::credentials::HiveCredentials::try_from_env()
                    .and_then(|c| c.offline_token().map(|s| s.to_string()))
            })
            .ok_or_else(|| {
                Error::config_error(
                    "offline_token required for Ditto backend (set HIVE_OFFLINE_TOKEN or DITTO_OFFLINE_TOKEN)",
                    Some("offline_token".to_string()),
                )
            })?;

        // Map abstraction BackendConfig to DittoConfig
        let ditto_config = DittoConfig {
            app_id: config.app_id,
            persistence_dir: config.persistence_dir,
            shared_key: config.shared_key.ok_or_else(|| {
                Error::config_error(
                    "shared_key required for Ditto backend",
                    Some("shared_key".to_string()),
                )
            })?,
            offline_token,
            tcp_listen_port: config.transport.tcp_listen_port,
            tcp_connect_address: config.transport.tcp_connect_address,
        };

        // Create DittoStore wrapped in Arc
        // Arc ensures all trait method calls use the same instance for peer discovery
        let store = Arc::new(DittoStore::new(ditto_config)?);

        // Store it
        *self.store.lock().unwrap() = Some(store);

        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(store) = self.store.lock().unwrap().take() {
            store.stop_sync();
            drop(store);
        }
        Ok(())
    }

    fn document_store(&self) -> Arc<dyn DocumentStore> {
        Arc::new(self.clone())
    }

    fn peer_discovery(&self) -> Arc<dyn PeerDiscovery> {
        Arc::new(self.clone())
    }

    fn sync_engine(&self) -> Arc<dyn SyncEngine> {
        Arc::new(self.clone())
    }

    async fn is_ready(&self) -> bool {
        self.store.lock().unwrap().is_some()
    }

    fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            name: "Ditto".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Clone for DittoBackend {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            peer_callbacks: self.peer_callbacks.clone(),
        }
    }
}

#[async_trait]
impl DocumentStore for DittoBackend {
    async fn upsert(&self, collection: &str, document: Document) -> anyhow::Result<DocumentId> {
        let store = self.get_store()?;

        // Convert Document to serde_json::Value
        let mut json_doc = serde_json::json!(document.fields);

        // If document has an ID, include it in the fields
        if let Some(ref id) = document.id {
            if let Some(obj) = json_doc.as_object_mut() {
                obj.insert("_id".to_string(), Value::String(id.clone()));
            }
        }

        // Use DittoStore's upsert method
        Ok(store.upsert(collection, json_doc).await?)
    }

    async fn query(&self, collection: &str, query: &Query) -> anyhow::Result<Vec<Document>> {
        let store = self.get_store()?;

        // Convert Query to DQL where clause
        let where_clause = query_to_dql(query);

        // Execute query
        let results = store.query(collection, &where_clause).await?;

        // Convert serde_json::Value results to Document
        Ok(results
            .into_iter()
            .map(|json_val| {
                let mut fields = HashMap::new();
                let mut doc_id = None;

                if let Some(obj) = json_val.as_object() {
                    for (key, value) in obj {
                        if key == "_id" {
                            doc_id = value.as_str().map(|s| s.to_string());
                        } else {
                            fields.insert(key.clone(), value.clone());
                        }
                    }
                }

                Document {
                    id: doc_id,
                    fields,
                    updated_at: SystemTime::now(),
                }
            })
            .collect())
    }

    async fn remove(&self, collection: &str, doc_id: &DocumentId) -> anyhow::Result<()> {
        let store = self.get_store()?;
        Ok(store.remove(collection, doc_id).await?)
    }

    fn observe(&self, collection: &str, query: &Query) -> anyhow::Result<ChangeStream> {
        let store = self.get_store()?;
        let where_clause = query_to_dql(query);
        let dql_query = format!("SELECT * FROM {} WHERE {}", collection, where_clause);

        // Create channel for change events
        let (tx, rx) = mpsc::unbounded_channel();
        let collection_name = collection.to_string();
        let collection_for_closure = collection_name.clone();
        let collection_for_error = collection_name.clone();

        // Track previous document IDs to detect changes
        let previous_doc_ids = Arc::new(Mutex::new(std::collections::HashSet::<String>::new()));
        let is_initial = Arc::new(Mutex::new(true));

        let prev_ids = previous_doc_ids.clone();
        let initial_flag = is_initial.clone();

        // Register observer with Ditto
        let _observer = store
            .ditto()
            .store()
            .register_observer_v2(&dql_query, move |result| {
                // Convert Ditto result to documents
                let documents: Vec<Document> = result
                    .iter()
                    .map(|item| {
                        let json_str = item.json_string();
                        let json_val: serde_json::Value =
                            serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);

                        let mut fields = HashMap::new();
                        let mut doc_id = None;

                        if let Some(obj) = json_val.as_object() {
                            for (key, value) in obj {
                                if key == "_id" {
                                    doc_id = value.as_str().map(|s| s.to_string());
                                } else {
                                    fields.insert(key.clone(), value.clone());
                                }
                            }
                        }

                        Document {
                            id: doc_id,
                            fields,
                            updated_at: SystemTime::now(),
                        }
                    })
                    .collect();

                let mut is_first = initial_flag.lock().unwrap();
                let mut prev = prev_ids.lock().unwrap();

                if *is_first {
                    // First callback - send initial snapshot
                    let _ = tx.send(crate::sync::ChangeEvent::Initial {
                        documents: documents.clone(),
                    });

                    // Track document IDs
                    prev.clear();
                    for doc in &documents {
                        if let Some(ref id) = doc.id {
                            prev.insert(id.clone());
                        }
                    }

                    *is_first = false;
                } else {
                    // Subsequent callback - detect changes
                    let mut current_ids = std::collections::HashSet::new();

                    // Send Updated events for new or modified documents
                    for doc in documents {
                        if let Some(ref id) = doc.id {
                            current_ids.insert(id.clone());

                            // Send update event (could be insert or update)
                            let _ = tx.send(crate::sync::ChangeEvent::Updated {
                                collection: collection_for_closure.clone(),
                                document: doc.clone(),
                            });
                        }
                    }

                    // Send Removed events for documents no longer in results
                    for old_id in prev.iter() {
                        if !current_ids.contains(old_id) {
                            let _ = tx.send(crate::sync::ChangeEvent::Removed {
                                collection: collection_for_closure.clone(),
                                doc_id: old_id.clone(),
                            });
                        }
                    }

                    // Update tracked IDs
                    *prev = current_ids;
                }
            })
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to register observer: {}", e),
                    "observe",
                    Some(collection_for_error),
                )
            })?;

        // Keep observer alive by leaking it (will be cleaned up on backend shutdown)
        std::mem::forget(_observer);

        Ok(ChangeStream { receiver: rx })
    }

    /// Get a single document by ID
    ///
    /// Override default implementation to use Ditto's _id field
    async fn get(&self, collection: &str, doc_id: &DocumentId) -> anyhow::Result<Option<Document>> {
        let query = Query::Eq {
            field: "_id".to_string(), // Ditto uses _id, not id
            value: Value::String(doc_id.clone()),
        };

        let docs = self.query(collection, &query).await?;
        Ok(docs.into_iter().next())
    }
}

/// Convert Query abstraction to DQL where clause
fn query_to_dql(query: &Query) -> String {
    match query {
        Query::Eq { field, value } => format!("{} == {}", field, value_to_dql(value)),
        Query::Lt { field, value } => format!("{} < {}", field, value_to_dql(value)),
        Query::Gt { field, value } => format!("{} > {}", field, value_to_dql(value)),
        Query::And(queries) => {
            let clauses: Vec<String> = queries.iter().map(query_to_dql).collect();
            format!("({})", clauses.join(" AND "))
        }
        Query::Or(queries) => {
            let clauses: Vec<String> = queries.iter().map(query_to_dql).collect();
            format!("({})", clauses.join(" OR "))
        }
        Query::All => "true".to_string(),
        Query::Custom(dql) => dql.clone(),

        // === Spatial queries (Issue #356) ===
        // Note: Ditto has native spatial query support. For real Ditto integration,
        // these would use Ditto's GEO_DISTANCE and bounding box functions.
        // This mock implementation generates DQL-like syntax for testing.
        Query::WithinRadius {
            center,
            radius_meters,
            lat_field,
            lon_field,
        } => {
            let lat_key = lat_field.as_deref().unwrap_or("lat");
            let lon_key = lon_field.as_deref().unwrap_or("lon");
            // Ditto DQL spatial syntax (mock)
            format!(
                "GEO_DISTANCE({}, {}, {}, {}) <= {}",
                lat_key, lon_key, center.lat, center.lon, radius_meters
            )
        }

        Query::WithinBounds {
            min,
            max,
            lat_field,
            lon_field,
        } => {
            let lat_key = lat_field.as_deref().unwrap_or("lat");
            let lon_key = lon_field.as_deref().unwrap_or("lon");
            // Ditto DQL bounding box syntax (mock)
            format!(
                "({} >= {} AND {} <= {} AND {} >= {} AND {} <= {})",
                lat_key, min.lat, lat_key, max.lat, lon_key, min.lon, lon_key, max.lon
            )
        }

        // === Negation query (Issue #357) ===
        Query::Not(inner) => format!("NOT ({})", query_to_dql(inner)),

        // === Deletion-aware queries (ADR-034, Issue #369) ===
        // For Ditto, these translate to DQL conditions on the _deleted field
        Query::IncludeDeleted(inner) => {
            // Include deleted: just process the inner query without soft-delete filter
            query_to_dql(inner)
        }
        Query::DeletedOnly => {
            // Only deleted documents
            "_deleted == true".to_string()
        }
    }
}

/// Convert Value to DQL literal
fn value_to_dql(value: &Value) -> String {
    match value {
        Value::String(s) => format!("'{}'", s.replace('\'', "\\'")),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => {
            // For complex types, convert to JSON string
            serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
        }
    }
}

#[async_trait]
impl PeerDiscovery for DittoBackend {
    async fn start(&self) -> anyhow::Result<()> {
        // Peer discovery starts automatically when sync starts in Ditto
        // So this is a no-op - actual discovery happens in SyncEngine::start_sync
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        // Peer discovery stops when sync stops
        // Actual stop happens in DataSyncBackend::shutdown
        Ok(())
    }

    async fn discovered_peers(&self) -> anyhow::Result<Vec<PeerInfo>> {
        let store = self.get_store()?;
        let presence_graph = store.ditto().presence().graph();

        // Convert Ditto peers to our PeerInfo abstraction
        let peers: Vec<PeerInfo> = presence_graph
            .remote_peers
            .iter()
            .map(|peer| {
                let mut metadata = HashMap::new();
                // device_name is a String, not Option<String>
                metadata.insert("device_name".to_string(), peer.device_name.clone());

                // Determine transport type from connections
                // For now, use a simplified mapping since Ditto's ConnectionType
                // doesn't directly expose the variants we need
                let transport = if !peer.connections.is_empty() {
                    // Default to Custom for now - the specific transport type
                    // is not critical for the abstraction layer
                    TransportType::Custom
                } else {
                    TransportType::Custom
                };

                PeerInfo {
                    peer_id: peer.peer_key_string.clone(),
                    address: peer.connections.first().map(|c| c.id.clone()),
                    transport,
                    connected: peer.is_connected_to_ditto_cloud || !peer.connections.is_empty(),
                    last_seen: SystemTime::now(), // Ditto doesn't expose last_seen, use current time
                    metadata,
                }
            })
            .collect();

        Ok(peers)
    }

    async fn add_peer(&self, address: &str, transport: TransportType) -> anyhow::Result<()> {
        let store = self.get_store()?;

        // Only TCP transport is supported for explicit peer addition in Ditto
        if transport != TransportType::Tcp {
            return Err(Error::config_error(
                "Only TCP transport supported for explicit peer addition",
                Some("transport".to_string()),
            )
            .into());
        }

        // Add TCP server address to Ditto's connect config
        store.ditto().update_transport_config(|config| {
            config.connect.tcp_servers.insert(address.to_string());
        });

        Ok(())
    }

    async fn wait_for_peer(&self, peer_id: &PeerId, timeout: Duration) -> anyhow::Result<()> {
        let store = self.get_store()?;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let peer_id_clone = peer_id.clone();
        let peer_id_for_error = peer_id.clone();

        // Register presence observer
        let observer = store.ditto().presence().observe(move |graph| {
            // Check if the peer we're waiting for is present
            let found = graph
                .remote_peers
                .iter()
                .any(|p| p.peer_key_string == peer_id_clone);

            if found {
                let _ = tx.send(());
            }
        });

        // Wait with timeout
        let result = tokio::time::timeout(timeout, rx.recv()).await;

        drop(observer);

        match result {
            Ok(Some(())) => Ok(()),
            Ok(None) => {
                Err(
                    Error::storage_error("Peer presence channel closed", "wait_for_peer", None)
                        .into(),
                )
            }
            Err(_) => Err(Error::storage_error(
                format!("Timeout waiting for peer {}", peer_id_for_error),
                "wait_for_peer",
                None,
            )
            .into()),
        }
    }

    fn on_peer_event(&self, callback: Box<dyn Fn(PeerEvent) + Send + Sync>) {
        self.peer_callbacks.lock().unwrap().push(callback);

        // Register presence observer to trigger callbacks
        if let Ok(store) = self.get_store() {
            let callbacks = self.peer_callbacks.clone();

            let _observer = store.ditto().presence().observe(move |graph| {
                // For now, send a simple Connected event for each remote peer
                // A more sophisticated implementation would track state changes
                for peer in &graph.remote_peers {
                    let peer_info = PeerInfo {
                        peer_id: peer.peer_key_string.clone(),
                        address: peer.connections.first().map(|c| c.id.clone()),
                        transport: TransportType::Custom,
                        connected: !peer.connections.is_empty(),
                        last_seen: SystemTime::now(),
                        metadata: HashMap::new(),
                    };

                    let callbacks = callbacks.lock().unwrap();
                    for callback in callbacks.iter() {
                        callback(PeerEvent::Connected(peer_info.clone()));
                    }
                }
            });

            // Keep observer alive by leaking it
            std::mem::forget(_observer);
        }
    }

    async fn get_peer_info(&self, peer_id: &PeerId) -> anyhow::Result<Option<PeerInfo>> {
        let peers = self.discovered_peers().await?;
        Ok(peers.into_iter().find(|p| &p.peer_id == peer_id))
    }
}

#[async_trait]
impl SyncEngine for DittoBackend {
    async fn start_sync(&self) -> anyhow::Result<()> {
        let store = self.get_store()?;
        eprintln!("DittoBackend::start_sync - Ditto ptr: {:p}", store.ditto());
        Ok(store.start_sync()?)
    }

    async fn stop_sync(&self) -> anyhow::Result<()> {
        let store = self.get_store()?;
        store.stop_sync();
        Ok(())
    }

    async fn subscribe(&self, collection: &str, query: &Query) -> anyhow::Result<SyncSubscription> {
        let store = self.get_store()?;
        let where_clause = query_to_dql(query);
        let dql_query = format!("SELECT * FROM {} WHERE {}", collection, where_clause);

        eprintln!("DittoBackend::subscribe - Ditto ptr: {:p}", store.ditto());

        // Use Sync API register_subscription_v2 (as per Ditto docs)
        let sync_sub = store
            .ditto()
            .sync()
            .register_subscription_v2(&dql_query)
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to create sync subscription: {}", e),
                    "subscribe",
                    Some(collection.to_string()),
                )
            })?;

        eprintln!(
            "DittoBackend: Created subscription for query: {}",
            dql_query
        );

        // Wrap in our SyncSubscription abstraction
        Ok(SyncSubscription::new(collection, sync_sub))
    }

    async fn is_syncing(&self) -> anyhow::Result<bool> {
        // In Ditto, if we have a store and it's initialized, sync is active
        // (it starts when we call start_sync and stops when we call stop_sync)
        Ok(self.store.lock().unwrap().is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::HiveCredentials;
    use crate::sync::TransportConfig;
    use std::path::PathBuf;

    /// Helper to create test backend config
    ///
    /// Uses HiveCredentials to load credentials (supports both HIVE_* and DITTO_* env vars)
    fn create_test_config() -> BackendConfig {
        // Load environment for credentials
        dotenvy::dotenv().ok();

        // Use HiveCredentials to load app_id and shared_key (with DITTO_* fallback)
        let (app_id, shared_key) = if let Ok(creds) = HiveCredentials::from_env() {
            (
                creds.app_id().to_string(),
                creds.secret_key().map(|s| s.to_string()),
            )
        } else {
            (
                "test-app-id".to_string(),
                Some("test-shared-key".to_string()),
            )
        };

        BackendConfig {
            app_id,
            persistence_dir: PathBuf::from(
                tempfile::tempdir()
                    .expect("Failed to create temp dir")
                    .path(),
            ),
            shared_key,
            transport: TransportConfig::default(),
            extra: HashMap::new(),
        }
    }

    #[test]
    fn test_backend_creation() {
        let backend = DittoBackend::new();
        assert!(!backend.store.lock().unwrap().is_some());
    }

    #[tokio::test]
    async fn test_backend_info() {
        let backend = DittoBackend::new();
        let info = backend.backend_info();
        assert_eq!(info.name, "Ditto");
        assert!(!info.version.is_empty());
    }

    #[tokio::test]
    async fn test_is_ready() {
        let backend = DittoBackend::new();
        assert!(!backend.is_ready().await);

        // Skip actual initialization if credentials not available
        // HiveCredentials checks for HIVE_OFFLINE_TOKEN with fallback to DITTO_OFFLINE_TOKEN
        if let Ok(creds) = HiveCredentials::from_env() {
            if creds.has_offline_token() {
                let config = create_test_config();
                backend.initialize(config).await.ok();
                assert!(backend.is_ready().await);
            }
        }
    }

    #[tokio::test]
    async fn test_query_to_dql() {
        // Test simple equality
        let query = Query::Eq {
            field: "name".to_string(),
            value: Value::String("test".to_string()),
        };
        assert_eq!(query_to_dql(&query), "name == 'test'");

        // Test less than
        let query = Query::Lt {
            field: "age".to_string(),
            value: Value::Number(serde_json::Number::from(42)),
        };
        assert_eq!(query_to_dql(&query), "age < 42");

        // Test greater than
        let query = Query::Gt {
            field: "score".to_string(),
            value: Value::Number(serde_json::Number::from(100)),
        };
        assert_eq!(query_to_dql(&query), "score > 100");

        // Test AND
        let query = Query::And(vec![
            Query::Eq {
                field: "active".to_string(),
                value: Value::Bool(true),
            },
            Query::Gt {
                field: "score".to_string(),
                value: Value::Number(serde_json::Number::from(50)),
            },
        ]);
        assert_eq!(query_to_dql(&query), "(active == true AND score > 50)");

        // Test OR
        let query = Query::Or(vec![
            Query::Eq {
                field: "role".to_string(),
                value: Value::String("admin".to_string()),
            },
            Query::Eq {
                field: "role".to_string(),
                value: Value::String("moderator".to_string()),
            },
        ]);
        assert_eq!(
            query_to_dql(&query),
            "(role == 'admin' OR role == 'moderator')"
        );

        // Test All
        let query = Query::All;
        assert_eq!(query_to_dql(&query), "true");

        // Test Custom
        let query = Query::Custom("custom_field LIKE '%pattern%'".to_string());
        assert_eq!(query_to_dql(&query), "custom_field LIKE '%pattern%'");
    }

    #[tokio::test]
    async fn test_value_to_dql() {
        // Test string
        assert_eq!(value_to_dql(&Value::String("hello".to_string())), "'hello'");

        // Test string with quotes (should escape)
        assert_eq!(
            value_to_dql(&Value::String("O'Brien".to_string())),
            "'O\\'Brien'"
        );

        // Test number
        assert_eq!(
            value_to_dql(&Value::Number(serde_json::Number::from(42))),
            "42"
        );

        // Test boolean
        assert_eq!(value_to_dql(&Value::Bool(true)), "true");
        assert_eq!(value_to_dql(&Value::Bool(false)), "false");

        // Test null
        assert_eq!(value_to_dql(&Value::Null), "null");
    }

    #[tokio::test]
    async fn test_document_to_json_conversion() {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), Value::String("test".to_string()));
        fields.insert(
            "count".to_string(),
            Value::Number(serde_json::Number::from(42)),
        );

        let doc = Document {
            id: Some("doc123".to_string()),
            fields,
            updated_at: SystemTime::now(),
        };

        // Convert to JSON (simulating what upsert does)
        let mut json_doc = serde_json::json!(doc.fields);
        if let Some(ref id) = doc.id {
            if let Some(obj) = json_doc.as_object_mut() {
                obj.insert("_id".to_string(), Value::String(id.clone()));
            }
        }

        // Verify conversion
        assert_eq!(json_doc["name"], "test");
        assert_eq!(json_doc["count"], 42);
        assert_eq!(json_doc["_id"], "doc123");
    }

    #[tokio::test]
    async fn test_trait_implementation() {
        // Verify all traits are implemented by checking trait objects work
        let backend = DittoBackend::new();

        let _: Arc<dyn DataSyncBackend> = Arc::new(backend.clone());
        let _: Arc<dyn DocumentStore> = Arc::new(backend.clone());
        let _: Arc<dyn PeerDiscovery> = Arc::new(backend.clone());
        let _: Arc<dyn SyncEngine> = Arc::new(backend.clone());
    }

    #[tokio::test]
    async fn test_backend_requires_shared_key() {
        let backend = DittoBackend::new();
        let mut config = create_test_config();
        config.shared_key = None;

        let result = backend.initialize(config).await;
        assert!(result.is_err());
        // Verify error message contains expected text
        if let Err(e) = result {
            assert!(e.to_string().contains("shared_key required"));
        }
    }

    #[tokio::test]
    async fn test_get_store_before_init() {
        let backend = DittoBackend::new();
        let result = backend.get_store();
        assert!(result.is_err());
        // Verify error message contains expected text
        if let Err(e) = result {
            assert!(e.to_string().contains("Backend not initialized"));
        }
    }

    #[tokio::test]
    async fn test_trait_two_node_sync() {
        use crate::sync::{BackendConfig, TransportConfig};
        use std::collections::HashMap;
        use tempfile::tempdir;
        use tokio::time::{sleep, Duration};

        dotenvy::dotenv().ok();

        // Skip test if credentials not available (checks HIVE_* with DITTO_* fallback)
        let credentials = match HiveCredentials::from_env() {
            Ok(creds) if creds.has_secret_key() => creds,
            _ => {
                eprintln!("Skipping test: credentials not available (need HIVE_APP_ID/HIVE_SECRET_KEY or DITTO_APP_ID/DITTO_SHARED_KEY)");
                return;
            }
        };

        let app_id = credentials.app_id().to_string();
        let shared_key = credentials.secret_key().unwrap().to_string();

        // Create temp directories
        let temp_dir1 = tempdir().expect("Failed to create temp dir 1");
        let temp_dir2 = tempdir().expect("Failed to create temp dir 2");

        // Create two backends using concrete types (not boxed traits)
        let backend1 = DittoBackend::new();
        let backend2 = DittoBackend::new();

        let tcp_port: u16 = 12346; // Different port from DittoStore test

        // Configure backend1 (listener)
        let config1 = BackendConfig {
            app_id: app_id.clone(),
            persistence_dir: temp_dir1.path().to_path_buf(),
            shared_key: Some(shared_key.clone()),
            transport: TransportConfig {
                tcp_listen_port: Some(tcp_port),
                tcp_connect_address: None,
                enable_mdns: false,
                enable_bluetooth: false,
                enable_websocket: false,
                custom: HashMap::new(),
            },
            extra: HashMap::new(),
        };

        // Configure backend2 (connector)
        let config2 = BackendConfig {
            app_id,
            persistence_dir: temp_dir2.path().to_path_buf(),
            shared_key: Some(shared_key),
            transport: TransportConfig {
                tcp_listen_port: None,
                tcp_connect_address: Some(format!("127.0.0.1:{}", tcp_port)),
                enable_mdns: false,
                enable_bluetooth: false,
                enable_websocket: false,
                custom: HashMap::new(),
            },
            extra: HashMap::new(),
        };

        // Initialize backends
        println!("Initializing backends via trait...");
        backend1
            .initialize(config1)
            .await
            .expect("Failed to init backend1");
        backend2
            .initialize(config2)
            .await
            .expect("Failed to init backend2");

        // Get sync engines via trait abstraction
        let sync1 = backend1.sync_engine();
        let sync2 = backend2.sync_engine();

        // Start sync via trait
        println!("Starting sync via trait...");
        sync1.start_sync().await.expect("Failed to start sync1");
        sync2.start_sync().await.expect("Failed to start sync2");

        // Create subscriptions via trait
        // IMPORTANT: Keep subscription handles alive to maintain Ditto sync
        println!("Creating subscriptions via trait...");
        let _sub1 = sync1
            .subscribe("trait_sync_test", &Query::All)
            .await
            .expect("Failed to create subscription on backend1");
        let _sub2 = sync2
            .subscribe("trait_sync_test", &Query::All)
            .await
            .expect("Failed to create subscription on backend2");

        // Prevent subscription handles from being optimized away
        // They must stay alive until shutdown for Ditto sync to work
        let _ = (&_sub1, &_sub2);

        // Wait for peer connection
        println!("Waiting for peer connection...");
        sleep(Duration::from_secs(5)).await;

        // Check discovered peers via trait
        let peers1 = backend1
            .peer_discovery()
            .discovered_peers()
            .await
            .expect("Failed to get peers from backend1");
        let peers2 = backend2
            .peer_discovery()
            .discovered_peers()
            .await
            .expect("Failed to get peers from backend2");

        println!("Backend1 discovered {} peers", peers1.len());
        println!("Backend2 discovered {} peers", peers2.len());

        // Insert document via trait
        println!("Inserting document via trait...");
        let mut fields = HashMap::new();
        fields.insert(
            "test_field".to_string(),
            Value::String("trait_test_value".to_string()),
        );
        let doc = Document::with_id("trait_test_001", fields);

        backend1
            .document_store()
            .upsert("trait_sync_test", doc)
            .await
            .expect("Failed to insert document");

        // Poll for document via trait
        println!("Waiting for document sync...");
        let mut synced = false;
        for attempt in 1..=20 {
            sleep(Duration::from_millis(500)).await;

            let query = Query::Eq {
                field: "_id".to_string(),
                value: Value::String("trait_test_001".to_string()),
            };

            let docs = backend2
                .document_store()
                .query("trait_sync_test", &query)
                .await
                .expect("Failed to query");

            if !docs.is_empty() {
                println!("✓ Document synced after {} attempts", attempt);
                synced = true;
                break;
            }
        }

        // Shutdown
        backend1.shutdown().await.ok();
        backend2.shutdown().await.ok();
        sleep(Duration::from_millis(100)).await;

        assert!(
            synced,
            "Document failed to sync between backends using trait abstraction"
        );
    }
}
