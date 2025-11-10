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
//! ```no_run
//! use cap_protocol::sync::automerge::AutomergeBackend;
//! use cap_protocol::sync::traits::*;
//! use cap_protocol::sync::types::*;
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
}

impl AutomergeBackend {
    /// Create new AutomergeBackend
    ///
    /// # Example
    ///
    /// ```
    /// use cap_protocol::sync::automerge::AutomergeBackend;
    ///
    /// let backend = AutomergeBackend::new();
    /// ```
    pub fn new() -> Self {
        Self {
            documents: Arc::new(Mutex::new(HashMap::new())),
            sync_states: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(None)),
            initialized: Arc::new(Mutex::new(false)),
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

    fn observe(&self, _collection: &str, _query: &Query) -> Result<ChangeStream> {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Helper: Create test BackendConfig
    fn test_config() -> BackendConfig {
        BackendConfig {
            app_id: "test_app".to_string(),
            persistence_dir: PathBuf::from("/tmp/automerge_test"),
            shared_key: None,
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
