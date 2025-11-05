//! Ditto backend implementation
//!
//! Wraps the Ditto SDK to implement the `DataSyncBackend` traits,
//! providing a bridge between CAP Protocol's abstraction layer
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
    store: Arc<Mutex<Option<DittoStore>>>,

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

    /// Get a reference to the underlying store (if initialized)
    fn get_store(&self) -> Result<DittoStore> {
        self.store
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| Error::config_error("Backend not initialized", None))
    }
}

impl Default for DittoBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSyncBackend for DittoBackend {
    async fn initialize(&self, config: BackendConfig) -> Result<()> {
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
            tcp_listen_port: config.transport.tcp_listen_port,
            tcp_connect_address: config.transport.tcp_connect_address,
        };

        // Create DittoStore
        let store = DittoStore::new(ditto_config)?;

        // Store it
        *self.store.lock().unwrap() = Some(store);

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
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
    async fn upsert(&self, collection: &str, document: Document) -> Result<DocumentId> {
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
        store.upsert(collection, json_doc).await
    }

    async fn query(&self, collection: &str, query: &Query) -> Result<Vec<Document>> {
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

    async fn remove(&self, collection: &str, doc_id: &DocumentId) -> Result<()> {
        let store = self.get_store()?;
        store.remove(collection, doc_id).await
    }

    fn observe(&self, collection: &str, query: &Query) -> Result<ChangeStream> {
        let store = self.get_store()?;
        let where_clause = query_to_dql(query);
        let dql_query = format!("SELECT * FROM {} WHERE {}", collection, where_clause);

        // Create channel for change events
        let (tx, rx) = mpsc::unbounded_channel();
        let collection = collection.to_string();

        // Register observer with Ditto
        let _observer = store
            .ditto()
            .store()
            .register_observer_v2(&dql_query, move |result| {
                // Convert Ditto result to our ChangeEvent
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

                // Send initial snapshot or update
                let _ = tx.send(crate::sync::ChangeEvent::Initial { documents });
            })
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to register observer: {}", e),
                    "observe",
                    Some(collection.clone()),
                )
            })?;

        // Keep observer alive by leaking it (will be cleaned up on backend shutdown)
        std::mem::forget(_observer);

        Ok(ChangeStream { receiver: rx })
    }

    /// Get a single document by ID
    ///
    /// Override default implementation to use Ditto's _id field
    async fn get(&self, collection: &str, doc_id: &DocumentId) -> Result<Option<Document>> {
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
    async fn start(&self) -> Result<()> {
        // Peer discovery starts automatically when sync starts in Ditto
        // So this is a no-op - actual discovery happens in SyncEngine::start_sync
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // Peer discovery stops when sync stops
        // Actual stop happens in DataSyncBackend::shutdown
        Ok(())
    }

    async fn discovered_peers(&self) -> Result<Vec<PeerInfo>> {
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

    async fn add_peer(&self, address: &str, transport: TransportType) -> Result<()> {
        let store = self.get_store()?;

        // Only TCP transport is supported for explicit peer addition in Ditto
        if transport != TransportType::Tcp {
            return Err(Error::config_error(
                "Only TCP transport supported for explicit peer addition",
                Some("transport".to_string()),
            ));
        }

        // Add TCP server address to Ditto's connect config
        store.ditto().update_transport_config(|config| {
            config.connect.tcp_servers.insert(address.to_string());
        });

        Ok(())
    }

    async fn wait_for_peer(&self, peer_id: &PeerId, timeout: Duration) -> Result<()> {
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
            Ok(None) => Err(Error::storage_error(
                "Peer presence channel closed",
                "wait_for_peer",
                None,
            )),
            Err(_) => Err(Error::storage_error(
                format!("Timeout waiting for peer {}", peer_id_for_error),
                "wait_for_peer",
                None,
            )),
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

    async fn get_peer_info(&self, peer_id: &PeerId) -> Result<Option<PeerInfo>> {
        let peers = self.discovered_peers().await?;
        Ok(peers.into_iter().find(|p| &p.peer_id == peer_id))
    }
}

#[async_trait]
impl SyncEngine for DittoBackend {
    async fn start_sync(&self) -> Result<()> {
        let store = self.get_store()?;
        store.start_sync()
    }

    async fn stop_sync(&self) -> Result<()> {
        let store = self.get_store()?;
        store.stop_sync();
        Ok(())
    }

    async fn subscribe(&self, collection: &str, query: &Query) -> Result<SyncSubscription> {
        let store = self.get_store()?;
        let where_clause = query_to_dql(query);
        let dql_query = format!("SELECT * FROM {} WHERE {}", collection, where_clause);

        // Create Ditto sync subscription
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

        // Wrap in our SyncSubscription abstraction
        Ok(SyncSubscription::new(collection, sync_sub))
    }

    async fn is_syncing(&self) -> Result<bool> {
        // In Ditto, if we have a store and it's initialized, sync is active
        // (it starts when we call start_sync and stops when we call stop_sync)
        Ok(self.store.lock().unwrap().is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::TransportConfig;
    use std::path::PathBuf;

    /// Helper to create test backend config
    fn create_test_config() -> BackendConfig {
        // Load environment for Ditto credentials
        dotenvy::dotenv().ok();

        let app_id = std::env::var("DITTO_APP_ID")
            .ok()
            .and_then(|v| {
                let trimmed = v.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .unwrap_or_else(|| "test-app-id".to_string());

        let shared_key = std::env::var("DITTO_SHARED_KEY")
            .ok()
            .and_then(|v| {
                let trimmed = v.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .unwrap_or_else(|| "test-shared-key".to_string());

        BackendConfig {
            app_id,
            persistence_dir: PathBuf::from(
                tempfile::tempdir()
                    .expect("Failed to create temp dir")
                    .path(),
            ),
            shared_key: Some(shared_key),
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

        // Skip actual initialization if Ditto credentials not available
        if std::env::var("DITTO_OFFLINE_TOKEN").is_ok() {
            let config = create_test_config();
            backend.initialize(config).await.ok();
            assert!(backend.is_ready().await);
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
}
