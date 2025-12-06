//! Automerge-based implementation of DataSyncBackend
//!
//! This module provides a CRDT backend using the Automerge library (v0.7.1),
//! enabling eventual consistency without requiring Ditto SDK.
//!
//! # Architecture
//!
//! - **Documents**: Stored as Automerge CRDTs indexed by collection:id
//! - **Sync Protocol**: Uses Automerge's built-in sync state machine
//! - **Query Engine**: In-memory filtering on exported JSON
//!
//! # Example
//!
//! ```text
//! use hive_protocol::sync::automerge::AutomergeBackend;
//! use hive_protocol::sync::traits::*;
//! use hive_protocol::sync::types::*;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = AutomergeBackend::new();
//! backend.initialize(BackendConfig::default()).await?;
//!
//! let doc_store = backend.document_store();
//! // Use DocumentStore API...
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use automerge::{sync, sync::SyncDoc, transaction::Transactable, Automerge};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::error::{Error, Result};
use crate::sync::traits::*;
use crate::sync::types::*;

/// Automerge-based backend for CRDT synchronization
///
/// This backend implements all DataSyncBackend traits using Automerge as the
/// underlying CRDT library, providing an alternative to Ditto.
#[derive(Clone)]
pub struct AutomergeBackend {
    /// Automerge documents indexed by collection:id key
    documents: Arc<Mutex<HashMap<String, Automerge>>>,

    /// Sync states for each peer:document pair
    sync_states: Arc<Mutex<HashMap<String, sync::State>>>,

    /// Configuration
    config: Arc<Mutex<Option<BackendConfig>>>,

    /// Initialized flag
    initialized: Arc<Mutex<bool>>,

    /// Change notification channels for observers
    observers: Arc<Mutex<Vec<mpsc::UnboundedSender<ChangeEvent>>>>,
}

impl AutomergeBackend {
    /// Create new AutomergeBackend
    ///
    /// # Example
    ///
    /// ```
    /// use hive_protocol::sync::automerge::AutomergeBackend;
    ///
    /// let backend = AutomergeBackend::new();
    /// ```
    pub fn new() -> Self {
        Self {
            documents: Arc::new(Mutex::new(HashMap::new())),
            sync_states: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(None)),
            initialized: Arc::new(Mutex::new(false)),
            observers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Helper: Generate document key from collection and ID
    fn doc_key(collection: &str, doc_id: &DocumentId) -> String {
        format!("{}:{}", collection, doc_id)
    }

    /// Helper: Convert Automerge document to our Document type
    ///
    /// For Phase 1, we read directly from the Automerge document using map_range.
    fn automerge_to_document(doc: &Automerge, doc_id: DocumentId) -> Result<Document> {
        use automerge::ReadDoc;

        let mut fields = HashMap::new();

        // Try to read from the root/root path
        if let Ok(Some((automerge::Value::Object(automerge::ObjType::Map), obj_id))) =
            doc.get(automerge::ROOT, "root")
        {
            // Iterate over the map entries
            for item in doc.map_range(&obj_id, ..) {
                let key_str = item.key.to_string();
                if let Ok(Some((value, _))) = doc.get(&obj_id, &item.key as &str) {
                    // Convert the Automerge value to serde_json::Value
                    if let Some(json_val) = Self::automerge_scalar_to_json(&value) {
                        fields.insert(key_str, json_val);
                    }
                }
            }
        }

        Ok(Document {
            id: Some(doc_id),
            fields,
            updated_at: std::time::SystemTime::now(),
        })
    }

    /// Helper: Convert Automerge scalar value to serde_json::Value
    fn automerge_scalar_to_json(value: &automerge::Value) -> Option<Value> {
        match value {
            automerge::Value::Scalar(scalar) => {
                let json_val = match scalar.as_ref() {
                    automerge::ScalarValue::Str(s) => Value::String(s.to_string()),
                    automerge::ScalarValue::Int(i) => Value::Number(serde_json::Number::from(*i)),
                    automerge::ScalarValue::Uint(u) => Value::Number(serde_json::Number::from(*u)),
                    automerge::ScalarValue::F64(f) => serde_json::Number::from_f64(*f)
                        .map(Value::Number)
                        .unwrap_or(Value::Null),
                    automerge::ScalarValue::Boolean(b) => Value::Bool(*b),
                    automerge::ScalarValue::Null => Value::Null,
                    automerge::ScalarValue::Bytes(bytes) => {
                        // Encode bytes as array of numbers
                        let byte_array: Vec<serde_json::Value> = bytes
                            .iter()
                            .map(|b| Value::Number(serde_json::Number::from(*b)))
                            .collect();
                        Value::Array(byte_array)
                    }
                    automerge::ScalarValue::Counter(_c) => {
                        // Counters don't have a simple get method, just convert to i64
                        // The Counter type doesn't expose the value directly in 0.7.1
                        // So we'll just return 0 as a placeholder
                        Value::Number(serde_json::Number::from(0))
                    }
                    automerge::ScalarValue::Timestamp(ts) => {
                        Value::Number(serde_json::Number::from(*ts))
                    }
                    automerge::ScalarValue::Unknown { .. } => Value::Null,
                };
                Some(json_val)
            }
            automerge::Value::Object(_) => {
                // For nested objects, return null for Phase 1
                // In production, you'd recursively convert
                Some(Value::Null)
            }
        }
    }

    /// Helper: Apply Document fields to Automerge doc
    fn apply_document_to_automerge(doc: &mut Automerge, document: &Document) -> Result<()> {
        doc.transact::<_, _, automerge::AutomergeError>(|tx| {
            // Create or get root map
            let root = tx.put_object(automerge::ROOT, "root", automerge::ObjType::Map)?;

            for (key, value) in &document.fields {
                // Convert serde_json::Value to Automerge scalar
                Self::put_json_value(tx, &root, key, value)?;
            }

            Ok(())
        })
        .map_err(|e| Error::Internal(format!("Transaction failed: {:?}", e)))?;

        Ok(())
    }

    /// Helper: Put JSON value into Automerge
    fn put_json_value(
        tx: &mut automerge::transaction::Transaction,
        obj: &automerge::ObjId,
        key: &str,
        value: &serde_json::Value,
    ) -> std::result::Result<(), automerge::AutomergeError> {
        use serde_json::Value;

        match value {
            Value::String(s) => {
                tx.put(obj, key, s.clone())?;
            }
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    tx.put(obj, key, i)?;
                } else if let Some(f) = n.as_f64() {
                    tx.put(obj, key, f)?;
                }
            }
            Value::Bool(b) => {
                tx.put(obj, key, *b)?;
            }
            Value::Null => {
                // Skip null values
            }
            Value::Array(arr) => {
                // Create list
                let list = tx.put_object(obj, key, automerge::ObjType::List)?;
                for (idx, item) in arr.iter().enumerate() {
                    Self::insert_json_value(tx, &list, idx, item)?;
                }
            }
            Value::Object(map) => {
                // Create nested map
                let nested = tx.put_object(obj, key, automerge::ObjType::Map)?;
                for (k, v) in map {
                    Self::put_json_value(tx, &nested, k, v)?;
                }
            }
        }

        Ok(())
    }

    /// Helper: Insert JSON value into Automerge list
    fn insert_json_value(
        tx: &mut automerge::transaction::Transaction,
        list: &automerge::ObjId,
        index: usize,
        value: &serde_json::Value,
    ) -> std::result::Result<(), automerge::AutomergeError> {
        use serde_json::Value;

        match value {
            Value::String(s) => {
                tx.insert(list, index, s.clone())?;
            }
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    tx.insert(list, index, i)?;
                } else if let Some(f) = n.as_f64() {
                    tx.insert(list, index, f)?;
                }
            }
            Value::Bool(b) => {
                tx.insert(list, index, *b)?;
            }
            Value::Null => {
                // Skip null values
            }
            Value::Array(_) | Value::Object(_) => {
                // For complex nested types, serialize as JSON string
                let json_str =
                    serde_json::to_string(value).map_err(|_| automerge::AutomergeError::Fail)?;
                tx.insert(list, index, json_str)?;
            }
        }

        Ok(())
    }

    /// Helper: Check if document matches query
    fn matches_query(&self, document: &Document, query: &Query) -> Result<bool> {
        match query {
            Query::All => Ok(true),

            Query::Eq { field, value } => {
                if let Some(doc_value) = document.fields.get(field) {
                    Ok(doc_value == value)
                } else {
                    Ok(false)
                }
            }

            Query::Lt { field, value } => {
                if let Some(doc_value) = document.fields.get(field) {
                    Ok(self.compare_values(doc_value, value)? < 0)
                } else {
                    Ok(false)
                }
            }

            Query::Gt { field, value } => {
                if let Some(doc_value) = document.fields.get(field) {
                    Ok(self.compare_values(doc_value, value)? > 0)
                } else {
                    Ok(false)
                }
            }

            Query::And(queries) => {
                for q in queries {
                    if !self.matches_query(document, q)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }

            Query::Or(queries) => {
                for q in queries {
                    if self.matches_query(document, q)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }

            Query::Custom(_) => {
                // Custom queries not supported in initial implementation
                Err(Error::Internal("Custom queries not yet supported".into()))
            }
        }
    }

    /// Helper: Compare two JSON values
    fn compare_values(&self, a: &serde_json::Value, b: &serde_json::Value) -> Result<i8> {
        use serde_json::Value;

        match (a, b) {
            (Value::Number(a_num), Value::Number(b_num)) => {
                if let (Some(a_f), Some(b_f)) = (a_num.as_f64(), b_num.as_f64()) {
                    if a_f < b_f {
                        Ok(-1)
                    } else if a_f > b_f {
                        Ok(1)
                    } else {
                        Ok(0)
                    }
                } else {
                    Err(Error::Internal("Number comparison failed".into()))
                }
            }
            (Value::String(a_str), Value::String(b_str)) => {
                if a_str < b_str {
                    Ok(-1)
                } else if a_str > b_str {
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            _ => Err(Error::Internal("Unsupported value comparison".into())),
        }
    }

    /// Generate sync message for a document
    ///
    /// This uses Automerge's sync protocol to generate a message containing
    /// the changes needed to sync with a peer.
    pub fn generate_sync_message(
        &self,
        collection: &str,
        doc_id: &DocumentId,
        peer_id: &str,
    ) -> Result<Vec<u8>> {
        let key = Self::doc_key(collection, doc_id);
        let docs = self.documents.lock().unwrap();

        let automerge_doc = docs.get(&key).ok_or_else(|| Error::NotFound {
            resource_type: "Document".to_string(),
            id: doc_id.clone(),
        })?;

        // Get or create sync state for this peer
        let mut sync_states = self.sync_states.lock().unwrap();
        let sync_state = sync_states
            .entry(format!("{}:{}", peer_id, key))
            .or_default();

        // Generate sync message
        let message = automerge_doc.generate_sync_message(sync_state);

        // Encode message (handle Option)
        match message {
            Some(msg) => Ok(msg.encode()),
            None => Ok(Vec::new()), // No changes to sync
        }
    }

    /// Receive and apply sync message
    ///
    /// This applies changes from a peer's sync message to our local document.
    pub fn receive_sync_message(
        &self,
        collection: &str,
        doc_id: &DocumentId,
        peer_id: &str,
        message: &[u8],
    ) -> Result<()> {
        let key = Self::doc_key(collection, doc_id);
        let mut docs = self.documents.lock().unwrap();

        let automerge_doc = docs.get_mut(&key).ok_or_else(|| Error::NotFound {
            resource_type: "Document".to_string(),
            id: doc_id.clone(),
        })?;

        // Decode message
        let sync_message = sync::Message::decode(message)
            .map_err(|e| Error::Internal(format!("Message decode failed: {:?}", e)))?;

        // Get sync state
        let mut sync_states = self.sync_states.lock().unwrap();
        let sync_state = sync_states
            .entry(format!("{}:{}", peer_id, key))
            .or_default();

        // Apply sync message
        automerge_doc
            .receive_sync_message(sync_state, sync_message)
            .map_err(|e| Error::Internal(format!("Sync message apply failed: {:?}", e)))?;

        Ok(())
    }
}

impl Default for AutomergeBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// DocumentStore Trait Implementation
// ============================================================================

#[async_trait]
impl DocumentStore for AutomergeBackend {
    async fn upsert(&self, collection: &str, mut document: Document) -> Result<DocumentId> {
        // Generate ID if not present
        let doc_id = document
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let key = Self::doc_key(collection, &doc_id);
        let mut docs = self.documents.lock().unwrap();

        if let Some(existing_doc) = docs.get_mut(&key) {
            // Update existing document
            Self::apply_document_to_automerge(existing_doc, &document)?;
        } else {
            // Create new document
            let mut automerge_doc = Automerge::new();
            Self::apply_document_to_automerge(&mut automerge_doc, &document)?;
            docs.insert(key, automerge_doc);
        }

        document.id = Some(doc_id.clone());

        // Notify observers
        drop(docs); // Release lock before notifying
        let observers = self.observers.lock().unwrap();
        for observer in observers.iter() {
            let _ = observer.send(ChangeEvent::Updated {
                collection: collection.to_string(),
                document: document.clone(),
            });
        }
        drop(observers);

        Ok(doc_id)
    }

    async fn query(&self, collection: &str, query: &Query) -> Result<Vec<Document>> {
        let docs = self.documents.lock().unwrap();
        let mut results = Vec::new();

        // Iterate all documents in collection
        for (key, automerge_doc) in docs.iter() {
            if !key.starts_with(&format!("{}:", collection)) {
                continue;
            }

            // Extract document ID from key
            let doc_id = key.split(':').nth(1).unwrap_or("").to_string();

            // Convert to our Document type
            let document = Self::automerge_to_document(automerge_doc, doc_id)?;

            // Apply query filter
            if self.matches_query(&document, query)? {
                results.push(document);
            }
        }

        Ok(results)
    }

    async fn remove(&self, collection: &str, doc_id: &DocumentId) -> Result<()> {
        let key = Self::doc_key(collection, doc_id);
        let mut docs = self.documents.lock().unwrap();

        docs.remove(&key).ok_or_else(|| Error::NotFound {
            resource_type: "Document".to_string(),
            id: doc_id.clone(),
        })?;

        // Notify observers
        drop(docs); // Release lock before notifying
        let observers = self.observers.lock().unwrap();
        for observer in observers.iter() {
            let _ = observer.send(ChangeEvent::Removed {
                collection: collection.to_string(),
                doc_id: doc_id.clone(),
            });
        }
        drop(observers);

        Ok(())
    }

    async fn get(&self, collection: &str, doc_id: &DocumentId) -> Result<Option<Document>> {
        let key = Self::doc_key(collection, doc_id);
        let docs = self.documents.lock().unwrap();

        if let Some(automerge_doc) = docs.get(&key) {
            let document = Self::automerge_to_document(automerge_doc, doc_id.clone())?;
            Ok(Some(document))
        } else {
            Ok(None)
        }
    }

    async fn count(&self, collection: &str, query: &Query) -> Result<usize> {
        let results = self.query(collection, query).await?;
        Ok(results.len())
    }

    fn observe(&self, collection: &str, query: &Query) -> Result<ChangeStream> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        // Send initial snapshot of matching documents
        let docs = self.documents.lock().unwrap();
        let mut initial_docs = Vec::new();

        for (key, automerge_doc) in docs.iter() {
            if !key.starts_with(&format!("{}:", collection)) {
                continue;
            }

            let doc_id = key.split(':').nth(1).unwrap_or("").to_string();
            if let Ok(document) = Self::automerge_to_document(automerge_doc, doc_id) {
                if self.matches_query(&document, query).unwrap_or(false) {
                    initial_docs.push(document);
                }
            }
        }

        drop(docs); // Release lock

        // Send initial snapshot
        let _ = tx.send(ChangeEvent::Initial {
            documents: initial_docs,
        });

        // Register this observer for future updates
        self.observers.lock().unwrap().push(tx.clone());

        Ok(ChangeStream { receiver: rx })
    }
}

// ============================================================================
// PeerDiscovery Trait Implementation
// ============================================================================

#[async_trait]
impl PeerDiscovery for AutomergeBackend {
    async fn start(&self) -> Result<()> {
        // Manual peer discovery only for initial implementation
        // Full implementation would support mDNS, etc.
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn discovered_peers(&self) -> Result<Vec<PeerInfo>> {
        // Return empty - manual configuration required
        Ok(Vec::new())
    }

    async fn add_peer(&self, _address: &str, _transport: TransportType) -> Result<()> {
        // Manual peer addition not implemented in initial version
        Ok(())
    }

    async fn wait_for_peer(&self, _peer_id: &PeerId, _timeout: Duration) -> Result<()> {
        // Peer waiting not implemented in initial version
        Err(Error::Internal("wait_for_peer not implemented".into()))
    }

    fn on_peer_event(&self, _callback: Box<dyn Fn(PeerEvent) + Send + Sync>) {
        // Callback registration not implemented in initial version
        // Would store in a Vec for future notifications
    }

    async fn get_peer_info(&self, _peer_id: &PeerId) -> Result<Option<PeerInfo>> {
        // Peer info lookup not implemented in initial version
        Ok(None)
    }
}

// ============================================================================
// SyncEngine Trait Implementation
// ============================================================================

#[async_trait]
impl SyncEngine for AutomergeBackend {
    async fn start_sync(&self) -> Result<()> {
        // For Automerge, sync is pull-based via generate/receive_sync_message
        // This method indicates we're ready to sync
        Ok(())
    }

    async fn stop_sync(&self) -> Result<()> {
        // Clean up sync states
        self.sync_states.lock().unwrap().clear();
        Ok(())
    }

    async fn subscribe(&self, collection: &str, _query: &Query) -> Result<SyncSubscription> {
        // Create subscription handle
        // For Automerge, subscriptions are logical - we track interest
        Ok(SyncSubscription::new(
            collection.to_string(),
            Box::new(AutomergeSubscriptionHandle {
                collection: collection.to_string(),
            }),
        ))
    }

    async fn is_syncing(&self) -> Result<bool> {
        // Always ready to sync with Automerge
        Ok(self.is_ready().await)
    }
}

/// Subscription handle for Automerge
struct AutomergeSubscriptionHandle {
    #[allow(dead_code)]
    collection: String,
}

// ============================================================================
// DataSyncBackend Trait Implementation
// ============================================================================

#[async_trait]
impl DataSyncBackend for AutomergeBackend {
    async fn initialize(&self, config: BackendConfig) -> Result<()> {
        let mut initialized = self.initialized.lock().unwrap();
        if *initialized {
            return Err(Error::Internal("Already initialized".into()));
        }

        *self.config.lock().unwrap() = Some(config);
        *initialized = true;

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        self.stop_sync().await?;
        self.documents.lock().unwrap().clear();
        self.sync_states.lock().unwrap().clear();
        *self.initialized.lock().unwrap() = false;

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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn is_ready(&self) -> bool {
        *self.initialized.lock().unwrap()
    }

    fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            name: "Automerge".to_string(),
            version: "0.7.1".to_string(),
        }
    }
}

// ============================================================================
// AutomergeIroh Backend Adapter (Phase 7: Lab Integration)
// ============================================================================

/// Type alias for peer event callback list
type PeerCallbacks = Arc<Mutex<Vec<Box<dyn Fn(PeerEvent) + Send + Sync>>>>;

/// DataSyncBackend adapter for storage::AutomergeBackend
///
/// This adapter wraps the storage::AutomergeBackend (RocksDB + Iroh + Automerge)
/// to provide DataSyncBackend trait compatibility for cap_sim_node.rs
#[derive(Clone)]
pub struct AutomergeIrohBackend {
    /// The underlying Automerge+Iroh backend
    backend: Arc<crate::storage::AutomergeBackend>,

    /// Reference to the transport for peer discovery
    transport: Arc<crate::network::IrohTransport>,

    /// Peer event callbacks
    peer_callbacks: PeerCallbacks,

    /// Initialization state
    initialized: Arc<Mutex<bool>>,

    /// Formation key for peer authentication (ADR-030)
    /// Peers must share the same app_id and secret_key to connect
    formation_key: Arc<std::sync::RwLock<Option<crate::security::FormationKey>>>,

    /// Peer discovery manager (ADR-011 Phase 3)
    #[cfg(feature = "automerge-backend")]
    discovery_manager: Arc<tokio::sync::RwLock<crate::discovery::peer::DiscoveryManager>>,
}

impl AutomergeIrohBackend {
    /// Create a new adapter
    pub fn new(
        backend: Arc<crate::storage::AutomergeBackend>,
        transport: Arc<crate::network::IrohTransport>,
    ) -> Self {
        Self {
            backend,
            transport,
            peer_callbacks: Arc::new(Mutex::new(Vec::new())),
            initialized: Arc::new(Mutex::new(false)),
            formation_key: Arc::new(std::sync::RwLock::new(None)),
            #[cfg(feature = "automerge-backend")]
            discovery_manager: Arc::new(tokio::sync::RwLock::new(
                crate::discovery::peer::DiscoveryManager::default(),
            )),
        }
    }

    /// Get the formation key (if initialized with credentials)
    pub fn formation_key(&self) -> Option<crate::security::FormationKey> {
        self.formation_key.read().unwrap().clone()
    }

    /// Get the formation ID (app_id used as formation identifier)
    pub fn formation_id(&self) -> Option<String> {
        self.formation_key
            .read()
            .unwrap()
            .as_ref()
            .map(|k| k.formation_id().to_string())
    }

    /// Create from store and transport (convenience method)
    pub fn from_parts(
        store: Arc<crate::storage::AutomergeStore>,
        transport: Arc<crate::network::IrohTransport>,
    ) -> Self {
        let backend = Arc::new(crate::storage::AutomergeBackend::with_transport(
            store,
            Arc::clone(&transport),
        ));
        Self::new(backend, transport)
    }

    /// Get the transport (for testing/advanced usage)
    pub fn transport(&self) -> Arc<crate::network::IrohTransport> {
        Arc::clone(&self.transport)
    }

    /// Get this node's endpoint ID
    pub fn endpoint_id(&self) -> iroh::EndpointId {
        self.transport.endpoint_id()
    }

    /// Add a discovery strategy to the peer discovery manager
    ///
    /// This allows configuring static peers, mDNS discovery, etc.
    #[cfg(feature = "automerge-backend")]
    pub async fn add_discovery_strategy(
        &self,
        strategy: Box<dyn crate::discovery::peer::DiscoveryStrategy>,
    ) -> Result<()> {
        let mut manager = self.discovery_manager.write().await;
        manager.add_strategy(strategy);
        Ok(())
    }

    /// Get access to the peer discovery information
    ///
    /// Returns a handle for querying discovered peers.
    #[cfg(feature = "automerge-backend")]
    pub fn get_peer_discovery(&self) -> PeerDiscoveryHandle {
        PeerDiscoveryHandle {
            manager: Arc::clone(&self.discovery_manager),
        }
    }
}

/// Handle for accessing peer discovery information
#[cfg(feature = "automerge-backend")]
pub struct PeerDiscoveryHandle {
    manager: Arc<tokio::sync::RwLock<crate::discovery::peer::DiscoveryManager>>,
}

#[cfg(feature = "automerge-backend")]
impl PeerDiscoveryHandle {
    /// Get all discovered peers
    ///
    /// Queries all discovery strategies and returns their currently cached peers.
    /// Strategies update their caches asynchronously, so this is a fast read operation.
    pub async fn discovered_peers(&self) -> Result<Vec<crate::discovery::peer::PeerInfo>> {
        let manager = self.manager.read().await;
        manager
            .discovered_peers()
            .await
            .map_err(|e| Error::Discovery {
                message: e.to_string(),
                source: None,
            })
    }

    /// Get the number of discovered peers
    ///
    /// Queries all discovery strategies and counts their currently cached peers.
    pub async fn peer_count(&self) -> usize {
        let manager = self.manager.read().await;
        manager.peer_count().await
    }
}

// DocumentStore implementation for AutomergeIrohBackend
struct IrohDocumentStore {
    backend: Arc<crate::storage::AutomergeBackend>,
}

#[async_trait]
impl DocumentStore for IrohDocumentStore {
    async fn upsert(&self, collection: &str, document: Document) -> Result<DocumentId> {
        use crate::storage::traits::StorageBackend;

        // Generate ID if not provided
        let doc_id = document.id.clone().unwrap_or_else(|| {
            use std::time::SystemTime;
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            format!("doc-{}", timestamp)
        });

        // Serialize document to JSON bytes
        let json_bytes = serde_json::to_vec(&document)?;

        // Get collection and upsert
        let coll = self.backend.collection(collection);
        coll.upsert(&doc_id, json_bytes)
            .map_err(|e| Error::Storage {
                message: e.to_string(),
                operation: Some("upsert".to_string()),
                key: Some(doc_id.clone()),
                source: None,
            })?;

        Ok(doc_id)
    }

    async fn query(&self, collection: &str, query: &Query) -> Result<Vec<Document>> {
        use crate::storage::traits::StorageBackend;

        let coll = self.backend.collection(collection);
        let all_items = coll.scan().map_err(|e| Error::Storage {
            message: e.to_string(),
            operation: Some("scan".to_string()),
            key: None,
            source: None,
        })?;

        // Deserialize and filter
        let mut results = Vec::new();
        for (doc_id, bytes) in all_items {
            if let Ok(mut doc) = serde_json::from_slice::<Document>(&bytes) {
                // Set the ID from the key if not already set
                if doc.id.is_none() {
                    doc.id = Some(doc_id);
                }

                if matches_query(&doc, query) {
                    results.push(doc);
                }
            }
        }

        Ok(results)
    }

    async fn remove(&self, collection: &str, doc_id: &DocumentId) -> Result<()> {
        use crate::storage::traits::StorageBackend;

        let coll = self.backend.collection(collection);
        coll.delete(doc_id).map_err(|e| Error::Storage {
            message: e.to_string(),
            operation: Some("delete".to_string()),
            key: Some(doc_id.clone()),
            source: None,
        })?;
        Ok(())
    }

    fn observe(&self, collection: &str, query: &Query) -> Result<ChangeStream> {
        use crate::storage::traits::StorageBackend;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        // Get initial snapshot
        let coll = self.backend.collection(collection);
        let all_items = coll.scan().map_err(|e| Error::Storage {
            message: e.to_string(),
            operation: Some("scan".to_string()),
            key: None,
            source: None,
        })?;

        let mut initial_docs = Vec::new();
        for (doc_id, bytes) in all_items {
            if let Ok(mut doc) = serde_json::from_slice::<Document>(&bytes) {
                if doc.id.is_none() {
                    doc.id = Some(doc_id);
                }

                if matches_query(&doc, query) {
                    initial_docs.push(doc);
                }
            }
        }

        // Send initial snapshot
        let _ = tx.send(ChangeEvent::Initial {
            documents: initial_docs,
        });

        // Subscribe to change notifications from the store (Issue #221)
        // This enables emitting ChangeEvent::Updated when documents sync from peers
        let mut change_rx = self.backend.automerge_store().subscribe_to_changes();
        let collection_name = collection.to_string();
        let collection_prefix = format!("{}:", collection);
        let query_clone = query.clone();
        let backend = Arc::clone(&self.backend);
        let tx_clone = tx.clone();

        // Spawn background task to listen for changes and emit Updated events
        tokio::spawn(async move {
            loop {
                match change_rx.recv().await {
                    Ok(doc_key) => {
                        // Check if this change is for our collection
                        if !doc_key.starts_with(&collection_prefix) {
                            continue;
                        }

                        // Extract doc_id from key (format: "collection:doc_id")
                        let doc_id = match doc_key.strip_prefix(&collection_prefix) {
                            Some(id) => id.to_string(),
                            None => continue,
                        };

                        // Fetch the updated document
                        let coll = backend.collection(collection_prefix.trim_end_matches(':'));
                        if let Ok(Some(bytes)) = coll.get(&doc_id) {
                            if let Ok(mut doc) = serde_json::from_slice::<Document>(&bytes) {
                                if doc.id.is_none() {
                                    doc.id = Some(doc_id);
                                }

                                // Check if document matches query
                                if matches_query(&doc, &query_clone) {
                                    // Emit Updated event
                                    if tx_clone
                                        .send(ChangeEvent::Updated {
                                            collection: collection_name.clone(),
                                            document: doc,
                                        })
                                        .is_err()
                                    {
                                        // Receiver dropped, stop listening
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "Observer change notification lagged, skipped {} messages",
                            n
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        // Channel closed, stop listening
                        break;
                    }
                }
            }
        });

        Ok(ChangeStream { receiver: rx })
    }
}

// PeerDiscovery implementation for AutomergeIrohBackend
struct IrohPeerDiscovery {
    transport: Arc<crate::network::IrohTransport>,
    peer_callbacks: PeerCallbacks,
    #[cfg(feature = "automerge-backend")]
    discovery_manager: Arc<tokio::sync::RwLock<crate::discovery::peer::DiscoveryManager>>,
    /// Formation key for peer authentication (required for secure connections)
    #[cfg(feature = "automerge-backend")]
    formation_key: Arc<std::sync::RwLock<Option<crate::security::FormationKey>>>,
}

#[async_trait]
impl PeerDiscovery for IrohPeerDiscovery {
    async fn start(&self) -> Result<()> {
        // Get formation key for authentication (required)
        let formation_key = self
            .formation_key
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Internal("Formation key not initialized".to_string()))?;

        // Start authenticated accept loop (replaces simple start_accept_loop)
        // This spawns a background task that accepts connections and performs handshake
        //
        // IMPORTANT (Issue #229): We MUST mark the accept loop as managed BEFORE spawning
        // our custom loop. This prevents AutomergeBackend::start_sync() from starting a
        // duplicate accept loop via transport.start_accept_loop(), which would cause
        // competing loops where one might accept connections without doing the handshake.
        #[cfg(feature = "automerge-backend")]
        {
            // Mark accept loop as externally managed to prevent duplicate loops
            self.transport.mark_accept_loop_managed().map_err(|e| {
                Error::Internal(format!("Failed to mark accept loop as managed: {}", e))
            })?;

            let transport = Arc::clone(&self.transport);
            let formation_key_accept = formation_key.clone();

            tokio::spawn(async move {
                use crate::network::formation_handshake::perform_responder_handshake;

                loop {
                    // Accept incoming connection
                    // Note (Issue #229): accept() returns Option<Connection>
                    // - Some(conn) = new connection that needs authentication
                    // - None = duplicate connection (already have one to this peer), skip handshake
                    match transport.accept().await {
                        Ok(Some(conn)) => {
                            let peer_id = conn.remote_id();
                            tracing::debug!("Accepted connection from: {:?}", peer_id);

                            // Perform formation handshake to authenticate peer
                            match perform_responder_handshake(&conn, &formation_key_accept).await {
                                Ok(()) => {
                                    tracing::info!("Peer {:?} authenticated successfully", peer_id);
                                    // Connection is already stored by transport.accept()
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Peer {:?} failed authentication: {}. Closing connection.",
                                        peer_id,
                                        e
                                    );
                                    // Close the unauthenticated connection
                                    conn.close(1u32.into(), b"authentication failed");
                                    transport.disconnect(&peer_id).ok();
                                }
                            }
                        }
                        Ok(None) => {
                            // Duplicate connection - already have one to this peer (Issue #229)
                            // Skip handshake since the existing connection is already authenticated
                            tracing::debug!("Duplicate connection closed, using existing");
                        }
                        Err(e) => {
                            // Accept loop stopped or endpoint closed
                            tracing::debug!("Accept loop ended: {}", e);
                            break;
                        }
                    }
                }
                tracing::debug!("Authenticated accept loop stopped");
            });
        }

        // Start discovery manager
        #[cfg(feature = "automerge-backend")]
        {
            let mut manager = self.discovery_manager.write().await;
            manager.start().await.map_err(|e| {
                Error::Internal(format!("Failed to start discovery manager: {}", e))
            })?;
        }

        // Spawn mDNS discovery event handler (Issue #233)
        // This subscribes to Iroh's MdnsDiscovery stream and connects to newly discovered peers.
        // Without this, mDNS only populates the address book but doesn't trigger connections.
        #[cfg(feature = "automerge-backend")]
        if let Some(mdns) = self.transport.mdns_discovery() {
            use futures_lite::StreamExt;
            use iroh::discovery::mdns::DiscoveryEvent;

            let mdns = mdns.clone();
            let transport = Arc::clone(&self.transport);
            let formation_key_mdns = formation_key.clone();

            tokio::spawn(async move {
                use crate::network::formation_handshake::perform_initiator_handshake;

                tracing::info!("Starting mDNS discovery event handler");
                let mut stream = mdns.subscribe().await;

                while let Some(event) = stream.next().await {
                    match event {
                        DiscoveryEvent::Discovered { endpoint_info, .. } => {
                            let peer_id = endpoint_info.endpoint_id;
                            tracing::info!(
                                peer_id = %peer_id,
                                "mDNS discovered peer, attempting connection"
                            );

                            // Check if already connected
                            if transport.get_connection(&peer_id).is_some() {
                                tracing::debug!(
                                    peer_id = %peer_id,
                                    "Already connected to mDNS-discovered peer"
                                );
                                continue;
                            }

                            // Connect using just the EndpointId (addresses from mDNS discovery)
                            match transport.connect_by_id(peer_id).await {
                                Ok(Some(conn)) => {
                                    // New connection - perform formation handshake
                                    match perform_initiator_handshake(&conn, &formation_key_mdns)
                                        .await
                                    {
                                        Ok(()) => {
                                            tracing::info!(
                                                peer_id = %peer_id,
                                                "mDNS peer connected and authenticated"
                                            );
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                peer_id = %peer_id,
                                                error = %e,
                                                "mDNS peer failed authentication"
                                            );
                                            conn.close(1u32.into(), b"authentication failed");
                                            transport.disconnect(&peer_id).ok();
                                        }
                                    }
                                }
                                Ok(None) => {
                                    // Already connected (they initiated) - per tie-breaking
                                    tracing::debug!(
                                        peer_id = %peer_id,
                                        "mDNS peer already connected (they initiated)"
                                    );
                                }
                                Err(e) => {
                                    tracing::debug!(
                                        peer_id = %peer_id,
                                        error = %e,
                                        "Failed to connect to mDNS-discovered peer"
                                    );
                                }
                            }
                        }
                        DiscoveryEvent::Expired { endpoint_id } => {
                            tracing::debug!(
                                peer_id = %endpoint_id,
                                "mDNS peer expired (no longer advertising)"
                            );
                            // Note: We don't disconnect immediately since the peer might still
                            // be reachable. The connection will fail naturally if unreachable.
                        }
                    }
                }
                tracing::debug!("mDNS discovery event handler stopped");
            });
        }

        // Spawn background task to connect to discovered peers (with authentication)
        #[cfg(feature = "automerge-backend")]
        {
            let discovery_manager = Arc::clone(&self.discovery_manager);
            let transport = Arc::clone(&self.transport);
            let formation_key_connect = formation_key;

            tokio::spawn(async move {
                use crate::network::formation_handshake::perform_initiator_handshake;
                use crate::network::PeerInfo as NetworkPeerInfo;

                // Adaptive interval: start fast (1s), slow down once mesh is stable (up to 5s)
                let mut interval_secs = 1u64;
                let mut consecutive_no_new_connections = 0u32;

                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;

                    // Get discovered peers
                    let manager = discovery_manager.read().await;
                    let discovered_peers = manager.get_peers().await;
                    drop(manager);

                    // Try to connect to each discovered peer
                    let mut made_new_connection = false;
                    for peer in discovered_peers {
                        // Convert discovery::peer::PeerInfo to network::PeerInfo
                        let network_peer_info = NetworkPeerInfo {
                            name: peer.name.clone(),
                            node_id: peer.node_id.clone(),
                            addresses: peer.addresses.clone(),
                            relay_url: peer.relay_url.clone(),
                        };

                        // Try to connect to the peer
                        // connect_peer() returns Option<Connection> (Issue #229):
                        // - Some(conn): New connection, we need to do initiator handshake
                        // - None: Already connected via their initiative, no handshake needed
                        if let Ok(endpoint_id) = peer.endpoint_id() {
                            match transport.connect_peer(&network_peer_info).await {
                                Ok(Some(conn)) => {
                                    // New connection - perform formation handshake to authenticate
                                    match perform_initiator_handshake(&conn, &formation_key_connect)
                                        .await
                                    {
                                        Ok(()) => {
                                            tracing::info!(
                                                "Connected and authenticated with peer: {}",
                                                peer.name
                                            );
                                            made_new_connection = true;
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "Peer {} failed authentication: {}. Disconnecting.",
                                                peer.name,
                                                e
                                            );
                                            // Disconnect unauthenticated peer
                                            conn.close(1u32.into(), b"authentication failed");
                                            transport.disconnect(&endpoint_id).ok();
                                        }
                                    }
                                }
                                Ok(None) => {
                                    // Already connected - they are the initiator (Issue #229)
                                    // Their accept loop will handle the handshake
                                    tracing::debug!(
                                        "Already connected to peer {} (they initiated)",
                                        peer.name
                                    );
                                }
                                Err(e) => {
                                    tracing::debug!(
                                        "Failed to connect to discovered peer {}: {}",
                                        peer.name,
                                        e
                                    );
                                }
                            }
                        }
                    }

                    // Adaptive backoff: stay fast while forming mesh, slow down once stable
                    if made_new_connection {
                        // Reset to fast polling when we're actively connecting
                        interval_secs = 1;
                        consecutive_no_new_connections = 0;
                    } else {
                        consecutive_no_new_connections += 1;
                        // After 3 cycles with no new connections, increase interval (max 5s)
                        if consecutive_no_new_connections >= 3 && interval_secs < 5 {
                            interval_secs = (interval_secs * 2).min(5);
                            tracing::debug!(
                                "Mesh stable, increasing connect interval to {}s",
                                interval_secs
                            );
                        }
                    }
                }
            });
        }

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn discovered_peers(&self) -> Result<Vec<PeerInfo>> {
        let mut peers = Vec::new();

        // Get connected peers from transport
        let peer_ids = self.transport.connected_peers();
        for peer_id in peer_ids {
            if self.transport.get_connection(&peer_id).is_some() {
                peers.push(PeerInfo {
                    peer_id: hex::encode(peer_id.as_bytes()),
                    address: None,
                    transport: TransportType::Custom,
                    connected: true,
                    last_seen: std::time::SystemTime::now(),
                    metadata: HashMap::new(),
                });
            }
        }

        // Add discovered but not yet connected peers from discovery manager
        #[cfg(feature = "automerge-backend")]
        {
            let manager = self.discovery_manager.read().await;
            for discovered_peer in manager.get_peers().await {
                // Check if already connected
                if !peers.iter().any(|p| p.peer_id == discovered_peer.node_id) {
                    peers.push(PeerInfo {
                        peer_id: discovered_peer.node_id.clone(),
                        address: discovered_peer.addresses.first().cloned(),
                        transport: TransportType::Custom,
                        connected: false,
                        last_seen: std::time::SystemTime::now(),
                        metadata: HashMap::new(),
                    });
                }
            }
        }

        Ok(peers)
    }

    async fn add_peer(&self, address: &str, _transport: TransportType) -> Result<()> {
        use crate::network::iroh_transport::IrohTransport;
        use crate::network::PeerInfo as NetworkPeerInfo;

        // Get formation key for authentication
        let formation_key = self
            .formation_key
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Internal("Formation key not initialized".to_string()))?;

        // Parse address format (Issue #226):
        // Format 1: "seed|hostname:port" - Derives EndpointId from seed (for containerlab)
        // Format 2: "hex_node_id" - Raw hex EndpointId (legacy static config)
        //
        // Example: "alpha-formation/node-1|192.168.1.100:9000"
        let (node_id, socket_addr) = if address.contains('|') {
            // Seed-based format: "seed|address"
            let parts: Vec<&str> = address.splitn(2, '|').collect();
            if parts.len() != 2 {
                return Err(Error::Internal(format!(
                    "Invalid address format: {}. Expected 'seed|host:port'",
                    address
                )));
            }
            let seed = parts[0];
            let addr = parts[1];

            // Derive EndpointId from seed using deterministic key generation
            let endpoint_id = IrohTransport::endpoint_id_from_seed(seed);
            let node_id_hex = hex::encode(endpoint_id.as_bytes());

            tracing::debug!(
                seed = seed,
                node_id = %node_id_hex,
                address = addr,
                "Derived EndpointId from seed for add_peer"
            );

            (node_id_hex, addr.to_string())
        } else {
            // Legacy format: assume address is a hex-encoded EndpointId
            // (for backwards compatibility with existing static configs)
            (address.to_string(), address.to_string())
        };

        let peer_info = NetworkPeerInfo {
            name: "manual-peer".to_string(),
            node_id,
            addresses: vec![socket_addr],
            relay_url: None,
        };

        // Connect to peer (Issue #229: returns Option<Connection>)
        // - Some(conn): New connection, we need to do initiator handshake
        // - None: Already connected via their initiative, no handshake needed
        let conn_opt =
            self.transport
                .connect_peer(&peer_info)
                .await
                .map_err(|e| Error::Network {
                    message: format!("Failed to connect to peer: {}", e),
                    peer_id: None,
                    source: None,
                })?;

        // Perform formation handshake to authenticate (only if we got a new connection)
        #[cfg(feature = "automerge-backend")]
        if let Some(conn) = conn_opt {
            use crate::network::formation_handshake::perform_initiator_handshake;

            if let Err(e) = perform_initiator_handshake(&conn, &formation_key).await {
                // Authentication failed - disconnect
                let endpoint_id = conn.remote_id();
                conn.close(1u32.into(), b"authentication failed");
                self.transport.disconnect(&endpoint_id).ok();

                return Err(Error::Network {
                    message: format!("Peer authentication failed: {}", e),
                    peer_id: Some(address.to_string()),
                    source: None,
                });
            }
        }

        Ok(())
    }

    async fn wait_for_peer(&self, peer_id: &PeerId, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();

        loop {
            let peers = self.discovered_peers().await?;
            if peers.iter().any(|p| &p.peer_id == peer_id) {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(Error::Network {
                    message: format!("Timeout waiting for peer: {}", peer_id),
                    peer_id: Some(peer_id.clone()),
                    source: None,
                });
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn on_peer_event(&self, callback: Box<dyn Fn(PeerEvent) + Send + Sync>) {
        self.peer_callbacks.lock().unwrap().push(callback);
    }

    async fn get_peer_info(&self, peer_id: &PeerId) -> Result<Option<PeerInfo>> {
        let peers = self.discovered_peers().await?;
        Ok(peers.into_iter().find(|p| &p.peer_id == peer_id))
    }
}

// SyncEngine implementation for AutomergeIrohBackend
struct IrohSyncEngine {
    backend: Arc<crate::storage::AutomergeBackend>,
    transport: Arc<crate::network::IrohTransport>,
}

#[async_trait]
impl SyncEngine for IrohSyncEngine {
    async fn start_sync(&self) -> Result<()> {
        use crate::storage::capabilities::SyncCapable;
        self.backend.start_sync().map_err(|e| Error::Storage {
            message: format!("Failed to start sync: {}", e),
            operation: Some("start_sync".to_string()),
            key: None,
            source: None,
        })?;
        Ok(())
    }

    async fn stop_sync(&self) -> Result<()> {
        use crate::storage::capabilities::SyncCapable;
        self.backend.stop_sync().map_err(|e| Error::Storage {
            message: format!("Failed to stop sync: {}", e),
            operation: Some("stop_sync".to_string()),
            key: None,
            source: None,
        })?;
        Ok(())
    }

    async fn subscribe(&self, collection: &str, _query: &Query) -> Result<SyncSubscription> {
        Ok(SyncSubscription::new(collection, ()))
    }

    async fn is_syncing(&self) -> Result<bool> {
        use crate::storage::capabilities::SyncCapable;
        let stats = self.backend.sync_stats().map_err(|e| Error::Storage {
            message: format!("Failed to get sync stats: {}", e),
            operation: Some("sync_stats".to_string()),
            key: None,
            source: None,
        })?;
        Ok(stats.peer_count > 0)
    }

    /// Connect to a peer using their EndpointId and addresses (Issue #235)
    ///
    /// This enables static peer configuration in containerlab and similar environments
    /// where mDNS discovery may not work across network namespaces.
    async fn connect_to_peer(&self, endpoint_id_hex: &str, addresses: &[String]) -> Result<bool> {
        use crate::network::PeerInfo as NetworkPeerInfo;

        // Parse the endpoint ID from hex
        let endpoint_id_bytes = hex::decode(endpoint_id_hex)
            .map_err(|e| Error::Internal(format!("Invalid endpoint_id_hex: {}", e)))?;

        if endpoint_id_bytes.len() != 32 {
            return Err(Error::Internal(format!(
                "Invalid endpoint_id_hex length: expected 32 bytes, got {}",
                endpoint_id_bytes.len()
            )));
        }

        // Tie-breaking: only the peer with the lower EndpointId initiates the connection
        // This prevents duplicate connections where both peers try to connect to each other
        let our_endpoint_id = self.transport.endpoint_id();
        let our_endpoint_hex = hex::encode(our_endpoint_id.as_bytes());

        if our_endpoint_hex.as_str() > endpoint_id_hex {
            // We have the higher EndpointId, so we should wait for them to connect to us
            tracing::debug!(
                our_endpoint = %our_endpoint_hex,
                peer_endpoint = %endpoint_id_hex,
                "Tie-breaking: peer has lower EndpointId, waiting for them to connect"
            );
            return Ok(false);
        }

        tracing::debug!(
            our_endpoint = %our_endpoint_hex,
            peer_endpoint = %endpoint_id_hex,
            addresses = ?addresses,
            "Tie-breaking: we have lower EndpointId, initiating connection"
        );

        // Create PeerInfo for the transport
        let peer_info = NetworkPeerInfo {
            name: format!("peer-{}", &endpoint_id_hex[..8]),
            node_id: endpoint_id_hex.to_string(),
            addresses: addresses.to_vec(),
            relay_url: None,
        };

        // Attempt to connect via transport
        // Returns Some(conn) if new connection, None if already connected
        match self.transport.connect_peer(&peer_info).await {
            Ok(Some(_conn)) => {
                tracing::info!(
                    peer_endpoint = %endpoint_id_hex,
                    "Successfully connected to peer"
                );
                Ok(true)
            }
            Ok(None) => {
                // Already connected (they initiated)
                tracing::debug!(
                    peer_endpoint = %endpoint_id_hex,
                    "Already connected to peer (they initiated)"
                );
                Ok(true)
            }
            Err(e) => {
                tracing::warn!(
                    peer_endpoint = %endpoint_id_hex,
                    error = %e,
                    "Failed to connect to peer"
                );
                Err(Error::Network {
                    message: format!("Failed to connect to peer: {}", e),
                    peer_id: Some(endpoint_id_hex.to_string()),
                    source: None,
                })
            }
        }
    }
}

// DataSyncBackend implementation
#[async_trait]
impl DataSyncBackend for AutomergeIrohBackend {
    async fn initialize(&self, config: BackendConfig) -> Result<()> {
        // Require shared_key for peer authentication
        let shared_key = config.shared_key.as_ref().ok_or_else(|| {
            Error::config_error(
                "AutomergeIroh backend requires HIVE_SECRET_KEY (or DITTO_SHARED_KEY) for peer authentication",
                Some("shared_key".to_string()),
            )
        })?;

        // Create FormationKey from app_id (formation_id) and shared_key
        // This ensures only peers with matching credentials can sync
        let formation_key = crate::security::FormationKey::from_base64(&config.app_id, shared_key)
            .map_err(|e| {
                Error::config_error(
                    format!(
                        "Invalid shared_key format: {}. Expected base64-encoded 32-byte key.",
                        e
                    ),
                    Some("shared_key".to_string()),
                )
            })?;

        // Store the formation key for peer authentication
        *self.formation_key.write().unwrap() = Some(formation_key);

        *self.initialized.lock().unwrap() = true;
        self.peer_discovery().start().await?;
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        if self.is_ready().await {
            let _ = self.sync_engine().stop_sync().await;
            let _ = self.peer_discovery().stop().await;
        }
        *self.initialized.lock().unwrap() = false;
        Ok(())
    }

    fn document_store(&self) -> Arc<dyn DocumentStore> {
        Arc::new(IrohDocumentStore {
            backend: Arc::clone(&self.backend),
        })
    }

    fn peer_discovery(&self) -> Arc<dyn PeerDiscovery> {
        Arc::new(IrohPeerDiscovery {
            transport: Arc::clone(&self.transport),
            peer_callbacks: Arc::clone(&self.peer_callbacks),
            #[cfg(feature = "automerge-backend")]
            discovery_manager: Arc::clone(&self.discovery_manager),
            #[cfg(feature = "automerge-backend")]
            formation_key: Arc::clone(&self.formation_key),
        })
    }

    fn sync_engine(&self) -> Arc<dyn SyncEngine> {
        Arc::new(IrohSyncEngine {
            backend: Arc::clone(&self.backend),
            transport: Arc::clone(&self.transport),
        })
    }

    async fn is_ready(&self) -> bool {
        *self.initialized.lock().unwrap()
    }

    fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            name: "AutomergeIroh".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// Implement HierarchicalStorageCapable for AutomergeIrohBackend
// This enables hive-sim hierarchical mode with Automerge backend
#[cfg(feature = "automerge-backend")]
impl crate::storage::HierarchicalStorageCapable for AutomergeIrohBackend {
    fn summary_storage(&self) -> Arc<dyn crate::hierarchy::SummaryStorage> {
        // Delegate to the underlying storage::AutomergeBackend
        crate::storage::HierarchicalStorageCapable::summary_storage(self.backend.as_ref())
    }

    fn command_storage(&self) -> Arc<dyn crate::command::CommandStorage> {
        // Delegate to the underlying storage::AutomergeBackend
        crate::storage::HierarchicalStorageCapable::command_storage(self.backend.as_ref())
    }
}

// Helper function for query matching
fn matches_query(doc: &Document, query: &Query) -> bool {
    match query {
        Query::All => true,
        Query::Eq { field, value } => {
            // Special case for "id" field - check doc.id instead of doc.fields
            if field == "id" {
                if let Some(ref doc_id) = doc.id {
                    if let Some(value_str) = value.as_str() {
                        return doc_id == value_str;
                    }
                }
                return false;
            }
            doc.get(field) == Some(value)
        }
        Query::Lt { field, value } => {
            if let Some(doc_val) = doc.get(field) {
                compare_values(doc_val, value) < 0
            } else {
                false
            }
        }
        Query::Gt { field, value } => {
            if let Some(doc_val) = doc.get(field) {
                compare_values(doc_val, value) > 0
            } else {
                false
            }
        }
        Query::And(queries) => queries.iter().all(|q| matches_query(doc, q)),
        Query::Or(queries) => queries.iter().any(|q| matches_query(doc, q)),
        Query::Custom(_) => false,
    }
}

fn compare_values(a: &serde_json::Value, b: &serde_json::Value) -> i32 {
    use serde_json::Value as V;

    match (a, b) {
        (V::Number(n1), V::Number(n2)) => {
            if let (Some(f1), Some(f2)) = (n1.as_f64(), n2.as_f64()) {
                if f1 < f2 {
                    -1
                } else if f1 > f2 {
                    1
                } else {
                    0
                }
            } else {
                0
            }
        }
        (V::String(s1), V::String(s2)) => s1.cmp(s2) as i32,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Helper: Create test BackendConfig with valid credentials
    fn test_config() -> BackendConfig {
        // Generate a valid test secret key (base64-encoded 32 bytes)
        let test_secret = crate::security::FormationKey::generate_secret();
        BackendConfig {
            app_id: "test_app".to_string(),
            persistence_dir: PathBuf::from("/tmp/automerge_test"),
            shared_key: Some(test_secret),
            transport: TransportConfig::default(),
            extra: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_automerge_backend_creation() {
        let backend = AutomergeBackend::new();
        assert!(!backend.is_ready().await);
    }

    #[tokio::test]
    async fn test_document_upsert() {
        let backend = AutomergeBackend::new();
        backend.initialize(test_config()).await.unwrap();

        let mut fields = HashMap::new();
        fields.insert("name".to_string(), serde_json::json!("test"));
        fields.insert("value".to_string(), serde_json::json!(42));

        let doc = Document::new(fields);
        let doc_id = backend
            .document_store()
            .upsert("test_collection", doc)
            .await
            .unwrap();

        assert!(!doc_id.is_empty());
    }

    #[tokio::test]
    async fn test_document_query() {
        let backend = AutomergeBackend::new();
        backend.initialize(test_config()).await.unwrap();

        // Insert test document
        let mut fields = HashMap::new();
        fields.insert("status".to_string(), serde_json::json!("active"));
        let doc = Document::new(fields);
        backend
            .document_store()
            .upsert("test_collection", doc)
            .await
            .unwrap();

        // Query
        let query = Query::Eq {
            field: "status".to_string(),
            value: serde_json::json!("active"),
        };

        let results = backend
            .document_store()
            .query("test_collection", &query)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_document_get() {
        let backend = AutomergeBackend::new();
        backend.initialize(test_config()).await.unwrap();

        // Insert document
        let mut fields = HashMap::new();
        fields.insert("data".to_string(), serde_json::json!("test_value"));
        let doc = Document::new(fields);
        let doc_id = backend
            .document_store()
            .upsert("test_coll", doc)
            .await
            .unwrap();

        // Get document
        let retrieved = backend
            .document_store()
            .get("test_coll", &doc_id)
            .await
            .unwrap();

        assert!(retrieved.is_some());
        let retrieved_doc = retrieved.unwrap();
        assert_eq!(
            retrieved_doc.fields.get("data").unwrap(),
            &serde_json::json!("test_value")
        );
    }

    #[tokio::test]
    async fn test_document_remove() {
        let backend = AutomergeBackend::new();
        backend.initialize(test_config()).await.unwrap();

        // Insert document
        let mut fields = HashMap::new();
        fields.insert("temp".to_string(), serde_json::json!(true));
        let doc = Document::new(fields);
        let doc_id = backend
            .document_store()
            .upsert("temp_coll", doc)
            .await
            .unwrap();

        // Remove document
        backend
            .document_store()
            .remove("temp_coll", &doc_id)
            .await
            .unwrap();

        // Verify removed
        let retrieved = backend
            .document_store()
            .get("temp_coll", &doc_id)
            .await
            .unwrap();

        assert!(retrieved.is_none());
    }
}

/// Tests for AutomergeIrohBackend credential requirements
#[cfg(all(test, feature = "automerge-backend"))]
mod iroh_credential_tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Test that AutomergeIrohBackend initialization fails without shared_key
    #[tokio::test]
    async fn test_automerge_iroh_requires_credentials() {
        // Create backend components
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::storage::AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(crate::network::IrohTransport::new().await.unwrap());

        let backend = AutomergeIrohBackend::from_parts(store, transport);

        // Config without shared_key should fail
        let config = BackendConfig {
            app_id: "test_app".to_string(),
            persistence_dir: PathBuf::from("/tmp/test"),
            shared_key: None, // Missing!
            transport: TransportConfig::default(),
            extra: HashMap::new(),
        };

        let result = backend.initialize(config).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        let error_msg = error.to_string();
        assert!(
            error_msg.contains("HIVE_SECRET_KEY") || error_msg.contains("shared_key"),
            "Error should mention missing credentials: {}",
            error_msg
        );
    }

    /// Test that AutomergeIrohBackend initializes successfully with valid credentials
    #[tokio::test]
    async fn test_automerge_iroh_with_valid_credentials() {
        // Create backend components
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::storage::AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(crate::network::IrohTransport::new().await.unwrap());

        let backend = AutomergeIrohBackend::from_parts(store, transport);

        // Generate valid test credentials
        let test_secret = crate::security::FormationKey::generate_secret();

        let config = BackendConfig {
            app_id: "test_formation".to_string(),
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key: Some(test_secret),
            transport: TransportConfig::default(),
            extra: HashMap::new(),
        };

        let result = backend.initialize(config).await;
        assert!(result.is_ok(), "Should initialize with valid credentials");

        // Verify formation key was stored
        let formation_key = backend.formation_key();
        assert!(
            formation_key.is_some(),
            "Formation key should be set after initialization"
        );
        assert_eq!(
            formation_key.unwrap().formation_id(),
            "test_formation",
            "Formation ID should match app_id"
        );
    }

    /// Test that invalid shared_key format is rejected
    #[tokio::test]
    async fn test_automerge_iroh_rejects_invalid_key_format() {
        // Create backend components
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::storage::AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(crate::network::IrohTransport::new().await.unwrap());

        let backend = AutomergeIrohBackend::from_parts(store, transport);

        // Invalid base64 key
        let config = BackendConfig {
            app_id: "test_app".to_string(),
            persistence_dir: PathBuf::from("/tmp/test"),
            shared_key: Some("not-valid-base64!!!".to_string()),
            transport: TransportConfig::default(),
            extra: HashMap::new(),
        };

        let result = backend.initialize(config).await;
        assert!(result.is_err(), "Should reject invalid base64 key");

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Invalid shared_key format"),
            "Error should mention invalid format: {}",
            error_msg
        );
    }
}
