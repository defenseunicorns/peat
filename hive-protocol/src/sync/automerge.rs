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
use crate::qos::{DeletionPolicy, DeletionPolicyRegistry, Tombstone};
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

    /// Tombstone storage indexed by collection:doc_id (ADR-034)
    tombstones: Arc<Mutex<HashMap<String, Tombstone>>>,

    /// Deletion policy registry (ADR-034)
    deletion_policy_registry: Arc<DeletionPolicyRegistry>,
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
            tombstones: Arc::new(Mutex::new(HashMap::new())),
            deletion_policy_registry: Arc::new(DeletionPolicyRegistry::new()),
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

            // === Negation query (Issue #357) ===
            Query::Not(inner) => Ok(!self.matches_query(document, inner)?),

            Query::Custom(_) => {
                // Custom queries not supported in initial implementation
                Err(Error::Internal("Custom queries not yet supported".into()))
            }

            // === Spatial queries (Issue #356) ===
            Query::WithinRadius {
                center,
                radius_meters,
                lat_field,
                lon_field,
            } => {
                let lat_key = lat_field.as_deref().unwrap_or("lat");
                let lon_key = lon_field.as_deref().unwrap_or("lon");

                if let (Some(lat_val), Some(lon_val)) = (
                    document.fields.get(lat_key).and_then(|v| v.as_f64()),
                    document.fields.get(lon_key).and_then(|v| v.as_f64()),
                ) {
                    let doc_point = GeoPoint::new(lat_val, lon_val);
                    Ok(doc_point.within_radius(center, *radius_meters))
                } else {
                    Ok(false)
                }
            }

            Query::WithinBounds {
                min,
                max,
                lat_field,
                lon_field,
            } => {
                let lat_key = lat_field.as_deref().unwrap_or("lat");
                let lon_key = lon_field.as_deref().unwrap_or("lon");

                if let (Some(lat_val), Some(lon_val)) = (
                    document.fields.get(lat_key).and_then(|v| v.as_f64()),
                    document.fields.get(lon_key).and_then(|v| v.as_f64()),
                ) {
                    let doc_point = GeoPoint::new(lat_val, lon_val);
                    Ok(doc_point.within_bounds(min, max))
                } else {
                    Ok(false)
                }
            }

            // === Deletion-aware queries (ADR-034, Issue #369) ===
            Query::IncludeDeleted(inner) => {
                // IncludeDeleted wraps another query - run the inner query
                // The soft-delete filter bypass is handled at the query() method level
                self.matches_query(document, inner)
            }

            Query::DeletedOnly => {
                // Only match documents with _deleted=true
                let is_deleted = document
                    .fields
                    .get("_deleted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Ok(is_deleted)
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

            // Apply soft-delete filter (ADR-034, Issue #369)
            // By default, queries exclude documents with _deleted=true
            // IncludeDeleted and DeletedOnly queries override this behavior
            if !query.matches_deletion_state(&document) {
                continue;
            }

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

    // === Deletion methods (ADR-034) ===

    async fn delete(
        &self,
        collection: &str,
        doc_id: &DocumentId,
        reason: Option<&str>,
    ) -> Result<crate::qos::DeleteResult> {
        let policy = self.deletion_policy(collection);

        match policy {
            DeletionPolicy::Immutable => {
                // Cannot delete immutable documents
                Ok(crate::qos::DeleteResult::immutable())
            }
            DeletionPolicy::ImplicitTTL { .. } => {
                // Implicit TTL: no-op, documents expire automatically
                Ok(crate::qos::DeleteResult {
                    deleted: false,
                    tombstone_id: None,
                    expires_at: None,
                    policy: policy.clone(),
                })
            }
            DeletionPolicy::Tombstone {
                tombstone_ttl,
                delete_wins: _,
            } => {
                // Create tombstone
                let tombstone = if let Some(reason_str) = reason {
                    Tombstone::with_reason(
                        doc_id.clone(),
                        collection.to_string(),
                        "local".to_string(), // TODO: Use actual node ID
                        0,                   // TODO: Use actual Lamport timestamp
                        reason_str,
                    )
                } else {
                    Tombstone::new(
                        doc_id.clone(),
                        collection.to_string(),
                        "local".to_string(), // TODO: Use actual node ID
                        0,                   // TODO: Use actual Lamport timestamp
                    )
                };
                let tombstone_id = format!("{}:{}", collection, doc_id);

                // Store tombstone
                self.tombstones
                    .lock()
                    .unwrap()
                    .insert(tombstone_id.clone(), tombstone.clone());

                // Remove the actual document
                self.remove(collection, doc_id).await.ok(); // Ignore if not found

                Ok(crate::qos::DeleteResult {
                    deleted: true,
                    tombstone_id: Some(tombstone_id),
                    expires_at: Some(std::time::SystemTime::now() + tombstone_ttl),
                    policy: policy.clone(),
                })
            }
            DeletionPolicy::SoftDelete {
                include_deleted_default: _,
            } => {
                // Soft delete: mark document with _deleted=true
                if let Some(mut doc) = self.get(collection, doc_id).await? {
                    doc.fields.insert("_deleted".to_string(), Value::Bool(true));
                    doc.fields.insert(
                        "_deleted_at".to_string(),
                        Value::String(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                    );
                    if let Some(reason) = reason {
                        doc.fields.insert(
                            "_deleted_reason".to_string(),
                            Value::String(reason.to_string()),
                        );
                    }
                    self.upsert(collection, doc).await?;

                    Ok(crate::qos::DeleteResult::soft_deleted(policy.clone()))
                } else {
                    // Document not found - still report as deleted
                    Ok(crate::qos::DeleteResult {
                        deleted: false,
                        tombstone_id: None,
                        expires_at: None,
                        policy: policy.clone(),
                    })
                }
            }
        }
    }

    async fn is_deleted(&self, collection: &str, doc_id: &DocumentId) -> Result<bool> {
        let key = format!("{}:{}", collection, doc_id);

        // Check if there's a tombstone
        if self.tombstones.lock().unwrap().contains_key(&key) {
            return Ok(true);
        }

        // Check for soft-delete (_deleted field)
        if let Some(doc) = self.get(collection, doc_id).await? {
            if let Some(deleted) = doc.fields.get("_deleted") {
                return Ok(deleted.as_bool().unwrap_or(false));
            }
        }

        Ok(false)
    }

    fn deletion_policy(&self, collection: &str) -> crate::qos::DeletionPolicy {
        self.deletion_policy_registry.get(collection)
    }

    async fn get_tombstones(&self, collection: &str) -> Result<Vec<crate::qos::Tombstone>> {
        let tombstones = self.tombstones.lock().unwrap();
        let prefix = format!("{}:", collection);

        Ok(tombstones
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .map(|(_, tombstone)| tombstone.clone())
            .collect())
    }

    async fn apply_tombstone(&self, tombstone: &crate::qos::Tombstone) -> Result<()> {
        let key = format!("{}:{}", tombstone.collection, tombstone.document_id);

        // Store the tombstone
        self.tombstones
            .lock()
            .unwrap()
            .insert(key, tombstone.clone());

        // Remove the document if it exists
        self.remove(&tombstone.collection, &tombstone.document_id)
            .await
            .ok();

        Ok(())
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

    /// Get the transport Arc pointer address (for debugging Issue #271)
    ///
    /// This returns the raw pointer address of the transport Arc, which can be used
    /// to verify that cloned backends share the same transport instance.
    /// If two backends show different addresses, they have different transports.
    pub fn transport_arc_ptr(&self) -> *const crate::network::IrohTransport {
        Arc::as_ptr(&self.transport)
    }

    /// Debug method to verify transport sharing (Issue #271)
    ///
    /// Logs the transport Arc pointer address. Call this on original and cloned
    /// backends to verify they share the same transport instance.
    pub fn debug_log_transport_ptr(&self, context: &str) {
        tracing::debug!(
            transport_ptr = ?Arc::as_ptr(&self.transport),
            endpoint_id = %self.transport.endpoint_id(),
            peer_count = self.transport.peer_count(),
            context = context,
            "AutomergeIrohBackend transport instance"
        );
    }

    /// Get this node's endpoint ID
    pub fn endpoint_id(&self) -> iroh::EndpointId {
        self.transport.endpoint_id()
    }

    /// Manually trigger sync for a specific document with all connected peers
    ///
    /// This is useful for testing or for explicit sync triggering when the
    /// automatic sync triggered by upsert may have been blocked by cooldown.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The full document key (e.g., "beacons:edge-sensor-001")
    pub async fn sync_document(&self, doc_key: &str) -> Result<()> {
        self.backend
            .sync_document(doc_key)
            .await
            .map_err(|e| Error::Network {
                message: format!("Failed to sync document {}: {}", doc_key, e),
                peer_id: None,
                source: None,
            })
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

    /// Immediately connect to all discovered peers
    ///
    /// This bypasses the background connection task's periodic interval, allowing
    /// tests to establish connections without waiting 1-7 seconds for the next cycle.
    ///
    /// Returns the number of new connections established.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Add discovery strategy with peer info
    /// backend_a.add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_b]))).await?;
    /// backend_b.add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_a]))).await?;
    ///
    /// // Connect immediately instead of waiting for background task
    /// backend_a.connect_to_discovered_peers_now().await?;
    /// backend_b.connect_to_discovered_peers_now().await?;
    /// ```
    #[cfg(feature = "automerge-backend")]
    pub async fn connect_to_discovered_peers_now(&self) -> Result<usize> {
        use crate::network::formation_handshake::perform_initiator_handshake;
        use crate::network::PeerInfo as NetworkPeerInfo;

        let formation_key = self
            .formation_key
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::config_error("Backend not initialized", None))?;

        // Get discovered peers
        let manager = self.discovery_manager.read().await;
        let discovered_peers = manager.get_peers().await;
        drop(manager);

        let mut new_connections = 0;

        for peer in discovered_peers {
            let network_peer_info = NetworkPeerInfo {
                name: peer.name.clone(),
                node_id: peer.node_id.clone(),
                addresses: peer.addresses.clone(),
                relay_url: peer.relay_url.clone(),
            };

            if let Ok(endpoint_id) = peer.endpoint_id() {
                match self.transport.connect_peer(&network_peer_info).await {
                    Ok(Some(conn)) => {
                        // Issue #346: Give the accept loop a moment to process any
                        // incoming connection from this peer. In symmetric discovery
                        // (both peers have each other in config), both will connect
                        // simultaneously and the accept loop needs time to process
                        // the incoming connection and do conflict resolution.
                        tokio::task::yield_now().await;
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                        // Check if connection was closed by conflict resolution
                        if conn.close_reason().is_some() {
                            tracing::debug!(
                                "Immediate connect: peer {} superseded by accept path",
                                peer.name
                            );
                            continue;
                        }

                        // New connection - perform formation handshake
                        match perform_initiator_handshake(&conn, &formation_key).await {
                            Ok(()) => {
                                tracing::debug!(
                                    "Immediate connect: authenticated with peer {}",
                                    peer.name
                                );
                                // Issue #346: Emit Connected AFTER successful handshake
                                self.transport.emit_peer_connected(endpoint_id);
                                new_connections += 1;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Immediate connect: peer {} failed auth: {}",
                                    peer.name,
                                    e
                                );
                                conn.close(1u32.into(), b"authentication failed");
                                // Issue #346: Don't call disconnect() here - the connection
                                // in the map might be a different one after conflict resolution.
                                // conn.close() is sufficient; close monitor handles cleanup.
                            }
                        }
                    }
                    Ok(None) => {
                        // Accept path is handling connection - no action needed
                        tracing::debug!(
                            "Immediate connect: peer {} handled by accept path",
                            peer.name
                        );
                    }
                    Err(e) => {
                        tracing::debug!(
                            "Immediate connect: failed to connect to {}: {}",
                            peer.name,
                            e
                        );
                    }
                }
            }
        }

        Ok(new_connections)
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

        // Trigger sync to push the document to connected peers
        // The doc_key format is "collection:doc_id"
        let doc_key = format!("{}:{}", collection, doc_id);
        match self.backend.sync_document(&doc_key).await {
            Ok(()) => {
                tracing::debug!("Sync triggered for document {} after upsert", doc_key);
            }
            Err(e) => {
                // Log but don't fail - sync is best-effort
                tracing::debug!("Failed to sync document {} after upsert: {}", doc_key, e);
            }
        }

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

                // Apply soft-delete filter (ADR-034, Issue #369)
                // By default, queries exclude documents with _deleted=true
                // IncludeDeleted and DeletedOnly queries override this behavior
                if !query.matches_deletion_state(&doc) {
                    continue;
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

        // Subscribe to observer notifications from the store (Issue #221, Issue #377)
        // This enables emitting ChangeEvent::Updated when documents sync from peers.
        // Using subscribe_to_observer_changes() instead of subscribe_to_changes() ensures
        // we get notifications for ALL document changes, including remotely synced docs.
        let mut change_rx = self
            .backend
            .automerge_store()
            .subscribe_to_observer_changes();
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
                        // Issue #346: When lagged, re-emit all documents in the collection
                        // to ensure observers don't miss updates. This is critical for
                        // metrics tracking and hierarchical aggregation callbacks.
                        tracing::warn!(
                            "Observer change notification lagged, skipped {} messages - re-emitting all documents",
                            n
                        );

                        // Re-scan collection and emit Updated for all matching documents
                        let coll = backend.collection(collection_prefix.trim_end_matches(':'));
                        if let Ok(all_items) = coll.scan() {
                            for (doc_id, bytes) in all_items {
                                if let Ok(mut doc) = serde_json::from_slice::<Document>(&bytes) {
                                    if doc.id.is_none() {
                                        doc.id = Some(doc_id);
                                    }

                                    // Send event if document matches query
                                    #[allow(clippy::collapsible_if)]
                                    if matches_query(&doc, &query_clone) {
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
    /// Whether the event forwarder task is running (Issue #275)
    event_forwarder_running: Arc<std::sync::atomic::AtomicBool>,
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

                // Issue #346: Track consecutive errors to detect permanent failures
                let mut consecutive_errors = 0u32;
                const MAX_CONSECUTIVE_ERRORS: u32 = 10;

                loop {
                    // Accept incoming connection
                    // Note (Issue #229): accept() returns Option<Connection>
                    // - Some(conn) = new connection that needs authentication
                    // - None = duplicate/transient (already handled or failed QUIC handshake)
                    match transport.accept().await {
                        Ok(Some(conn)) => {
                            consecutive_errors = 0; // Reset on success
                            let peer_id = conn.remote_id();

                            // Perform formation handshake to authenticate peer
                            match perform_responder_handshake(&conn, &formation_key_accept).await {
                                Ok(()) => {
                                    // Issue #346: Emit Connected AFTER successful handshake
                                    transport.emit_peer_connected(peer_id);
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        ?peer_id,
                                        error = %e,
                                        "Formation handshake failed"
                                    );
                                    // Close the unauthenticated connection - connection monitor
                                    // will handle cleanup (Issue #346 stable_id check)
                                    conn.close(1u32.into(), b"authentication failed");
                                }
                            }
                        }
                        Ok(None) => {
                            // Issue #346: This now includes transient errors (failed QUIC handshake)
                            // as well as duplicate connections. Either way, continue accepting.
                            consecutive_errors = 0; // Reset - we're still accepting
                        }
                        Err(e) => {
                            // Issue #346: Only fatal errors (endpoint closed) should stop the loop
                            // But add a circuit breaker for repeated failures
                            consecutive_errors += 1;
                            let error_msg = format!("{}", e);

                            if error_msg.contains("Endpoint closed")
                                || error_msg.contains("no more")
                            {
                                tracing::info!("Accept loop stopped: endpoint closed");
                                break;
                            }

                            if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                tracing::error!(
                                    consecutive_errors,
                                    error = %e,
                                    "Accept loop stopping after {} consecutive errors",
                                    MAX_CONSECUTIVE_ERRORS
                                );
                                break;
                            }

                            tracing::warn!(
                                error = %e,
                                consecutive_errors,
                                "Accept error (will retry, {} more before stopping)",
                                MAX_CONSECUTIVE_ERRORS - consecutive_errors
                            );
                            // Small delay before retrying to avoid tight error loop
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
                tracing::info!("Authenticated accept loop stopped");
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
                                            // Issue #346: Emit Connected AFTER successful handshake
                                            transport.emit_peer_connected(peer_id);
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
                                    // Accept path is handling connection
                                    tracing::debug!(
                                        peer_id = %peer_id,
                                        "mDNS peer connection handled by accept path"
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
                        // - None: Already connected via accept path, no action needed
                        if let Ok(endpoint_id) = peer.endpoint_id() {
                            match transport.connect_peer(&network_peer_info).await {
                                Ok(Some(conn)) => {
                                    // Issue #346: Give the accept loop a moment to process any
                                    // incoming connection from this peer. In symmetric discovery
                                    // (both peers have each other in config), both will connect
                                    // simultaneously and the accept loop needs time to process
                                    // the incoming connection and do conflict resolution.
                                    tokio::task::yield_now().await;
                                    tokio::time::sleep(tokio::time::Duration::from_millis(10))
                                        .await;

                                    // Check if connection was closed by conflict resolution
                                    // (accept path superseded this connection).
                                    if conn.close_reason().is_some() {
                                        tracing::debug!(
                                            "Peer {} connection superseded by accept path",
                                            peer.name
                                        );
                                        continue;
                                    }

                                    // New connection - perform formation handshake
                                    match perform_initiator_handshake(&conn, &formation_key_connect)
                                        .await
                                    {
                                        Ok(()) => {
                                            tracing::info!(
                                                "Connected and authenticated with peer: {}",
                                                peer.name
                                            );
                                            // Issue #346: Emit Connected AFTER successful handshake
                                            transport.emit_peer_connected(endpoint_id);
                                            made_new_connection = true;
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "Peer {} failed authentication: {}. Disconnecting.",
                                                peer.name,
                                                e
                                            );
                                            // Issue #346: Don't call disconnect() here - the connection
                                            // in the map might be a different one after conflict resolution.
                                            // conn.close() is sufficient; close monitor handles cleanup.
                                            conn.close(1u32.into(), b"authentication failed");
                                        }
                                    }
                                }
                                Ok(None) => {
                                    // Accept path is handling connection
                                    tracing::debug!(
                                        "Peer {} connection handled by accept path",
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

        // Connect to peer (conflict resolution handled by transport layer)
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

            let endpoint_id = conn.remote_id();
            if let Err(e) = perform_initiator_handshake(&conn, &formation_key).await {
                // Authentication failed - close the connection
                // Issue #346: Don't call disconnect() here - the connection
                // in the map might be a different one after conflict resolution.
                // conn.close() is sufficient; close monitor handles cleanup.
                conn.close(1u32.into(), b"authentication failed");

                return Err(Error::Network {
                    message: format!("Peer authentication failed: {}", e),
                    peer_id: Some(address.to_string()),
                    source: None,
                });
            }
            // Issue #346: Emit Connected AFTER successful handshake
            self.transport.emit_peer_connected(endpoint_id);
        }
        // If conn_opt is None, accept path is handling the connection

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

        // Start event forwarder on first callback registration (Issue #275)
        // Use compare_exchange to ensure we only start once
        if self
            .event_forwarder_running
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_ok()
        {
            // Subscribe to transport events and forward to callbacks
            let mut rx = self.transport.subscribe_peer_events();
            let callbacks = Arc::clone(&self.peer_callbacks);
            let running = Arc::clone(&self.event_forwarder_running);

            // Spawn the forwarder task using std::thread with a tokio runtime
            // (since on_peer_event is not async)
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create event forwarder runtime");

                rt.block_on(async move {
                    use crate::network::TransportPeerEvent;

                    while running.load(std::sync::atomic::Ordering::SeqCst) {
                        match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
                            .await
                        {
                            Ok(Some(transport_event)) => {
                                // Convert TransportPeerEvent to PeerEvent
                                let peer_event = match transport_event {
                                    TransportPeerEvent::Connected { endpoint_id, .. } => {
                                        PeerEvent::Connected(PeerInfo {
                                            peer_id: format!("{:?}", endpoint_id),
                                            address: None,
                                            transport: TransportType::Tcp, // QUIC maps to TCP for now
                                            connected: true,
                                            last_seen: std::time::SystemTime::now(),
                                            metadata: std::collections::HashMap::new(),
                                        })
                                    }
                                    TransportPeerEvent::Disconnected {
                                        endpoint_id,
                                        reason,
                                    } => PeerEvent::Disconnected {
                                        peer_id: format!("{:?}", endpoint_id),
                                        reason: Some(reason),
                                    },
                                };

                                // Invoke all callbacks
                                if let Ok(cbs) = callbacks.lock() {
                                    for cb in cbs.iter() {
                                        cb(peer_event.clone());
                                    }
                                }
                            }
                            Ok(None) => {
                                // Channel closed, stop forwarder
                                break;
                            }
                            Err(_) => {
                                // Timeout - continue to check running flag
                            }
                        }
                    }
                });
            });
        }
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
    formation_key: Option<crate::security::FormationKey>,
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

        // Issue #346: Removed tie-breaking from sync layer
        //
        // Tie-breaking is handled by the transport layer (IrohTransport::connect).
        // For static configurations (TCP_CONNECT), we should always attempt to connect
        // when explicitly configured. The transport will return Ok(None) if we should
        // wait for the peer to connect to us, which we handle below.
        //
        // Having tie-breaking at BOTH layers caused connections to fail when:
        // - Child node (soldier) has higher EndpointId than parent (squad leader)
        // - Child's TCP_CONNECT says "connect to parent"
        // - Sync layer tie-breaking blocked the connection
        // - Parent doesn't have child in config, so never connects
        // - Result: no connection!
        let our_endpoint_id = self.transport.endpoint_id();
        let our_endpoint_hex = hex::encode(our_endpoint_id.as_bytes());

        tracing::debug!(
            our_endpoint = %our_endpoint_hex,
            peer_endpoint = %endpoint_id_hex,
            addresses = ?addresses,
            "Connecting to peer via static configuration"
        );

        // Create PeerInfo for the transport
        let peer_info = NetworkPeerInfo {
            name: format!("peer-{}", &endpoint_id_hex[..8]),
            node_id: endpoint_id_hex.to_string(),
            addresses: addresses.to_vec(),
            relay_url: None,
        };

        // Issue #346: connect_peer returns Option<Connection>
        // - Some(conn): New connection, we need to do initiator handshake
        // - None: Accept path is handling, no action needed
        match self.transport.connect_peer(&peer_info).await {
            Ok(Some(conn)) => {
                // Issue #346: Check if connection was closed by conflict resolution
                if conn.close_reason().is_some() {
                    tracing::debug!(
                        peer_endpoint = %endpoint_id_hex,
                        "Connection superseded by accept path"
                    );
                    return Ok(false);
                }

                // New connection - perform formation handshake
                if let Some(ref formation_key) = self.formation_key {
                    use crate::network::formation_handshake::perform_initiator_handshake;
                    match perform_initiator_handshake(&conn, formation_key).await {
                        Ok(()) => {
                            tracing::info!(
                                peer_endpoint = %endpoint_id_hex,
                                "Successfully connected to peer and authenticated"
                            );
                            // Issue #378: Emit peer connected event to notify sync handlers
                            if let Ok(peer_id) = peer_info.endpoint_id() {
                                self.transport.emit_peer_connected(peer_id);
                            }
                            Ok(true)
                        }
                        Err(e) => {
                            tracing::warn!(
                                peer_endpoint = %endpoint_id_hex,
                                error = %e,
                                "Peer authentication failed"
                            );
                            // Close the connection on auth failure
                            if let Ok(peer_id) = peer_info.endpoint_id() {
                                conn.close(1u32.into(), b"authentication failed");
                                self.transport.disconnect(&peer_id).ok();
                            }
                            Err(Error::Network {
                                message: format!("Peer authentication failed: {}", e),
                                peer_id: Some(endpoint_id_hex.to_string()),
                                source: None,
                            })
                        }
                    }
                } else {
                    // No formation key - just report connected
                    tracing::info!(
                        peer_endpoint = %endpoint_id_hex,
                        "Successfully connected to peer (no authentication)"
                    );
                    // Issue #378: Emit peer connected event to notify sync handlers
                    if let Ok(peer_id) = peer_info.endpoint_id() {
                        self.transport.emit_peer_connected(peer_id);
                    }
                    Ok(true)
                }
            }
            Ok(None) => {
                // Accept path is handling connection
                tracing::debug!(
                    peer_endpoint = %endpoint_id_hex,
                    "Connection handled by accept path"
                );
                // Return true since a connection will be established via accept path
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
            event_forwarder_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    fn sync_engine(&self) -> Arc<dyn SyncEngine> {
        Arc::new(IrohSyncEngine {
            backend: Arc::clone(&self.backend),
            transport: Arc::clone(&self.transport),
            formation_key: self.formation_key(),
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

        // === Spatial queries (Issue #356) ===
        Query::WithinRadius {
            center,
            radius_meters,
            lat_field,
            lon_field,
        } => {
            let lat_key = lat_field.as_deref().unwrap_or("lat");
            let lon_key = lon_field.as_deref().unwrap_or("lon");

            if let (Some(lat_val), Some(lon_val)) = (
                doc.get(lat_key).and_then(|v| v.as_f64()),
                doc.get(lon_key).and_then(|v| v.as_f64()),
            ) {
                let doc_point = GeoPoint::new(lat_val, lon_val);
                doc_point.within_radius(center, *radius_meters)
            } else {
                false
            }
        }

        Query::WithinBounds {
            min,
            max,
            lat_field,
            lon_field,
        } => {
            let lat_key = lat_field.as_deref().unwrap_or("lat");
            let lon_key = lon_field.as_deref().unwrap_or("lon");

            if let (Some(lat_val), Some(lon_val)) = (
                doc.get(lat_key).and_then(|v| v.as_f64()),
                doc.get(lon_key).and_then(|v| v.as_f64()),
            ) {
                let doc_point = GeoPoint::new(lat_val, lon_val);
                doc_point.within_bounds(min, max)
            } else {
                false
            }
        }

        // === Negation query (Issue #357) ===
        Query::Not(inner) => !matches_query(doc, inner),

        // === Deletion-aware queries (ADR-034, Issue #369) ===
        Query::IncludeDeleted(inner) => {
            // IncludeDeleted wraps another query - run the inner query
            // The soft-delete filter bypass is handled at the query() method level
            matches_query(doc, inner)
        }

        Query::DeletedOnly => {
            // Only match documents with _deleted=true
            doc.fields
                .get("_deleted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        }
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

/// Tests for Issue #271: Verify Clone correctly shares transport instance
#[cfg(all(test, feature = "automerge-backend"))]
mod issue_271_clone_tests {
    use super::*;

    /// Test that cloning AutomergeIrohBackend shares the same transport Arc
    ///
    /// Issue #271: When cloning AutomergeIrohBackend, the transport should be
    /// shared (same Arc pointer), not duplicated. This ensures connections
    /// accumulate correctly across all references to the backend.
    #[tokio::test]
    async fn test_clone_shares_transport_arc() {
        // Create backend components
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::storage::AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(crate::network::IrohTransport::new().await.unwrap());

        let original = AutomergeIrohBackend::from_parts(store, Arc::clone(&transport));
        let cloned = original.clone();

        // Verify transport Arc is shared (same pointer)
        let original_transport_ptr = Arc::as_ptr(&original.transport());
        let cloned_transport_ptr = Arc::as_ptr(&cloned.transport());

        assert_eq!(
            original_transport_ptr, cloned_transport_ptr,
            "Clone should share the same transport Arc, but got different pointers:\n  Original: {:?}\n  Clone: {:?}",
            original_transport_ptr, cloned_transport_ptr
        );

        // Verify both point to the same transport as the original Arc
        let source_transport_ptr = Arc::as_ptr(&transport);
        assert_eq!(
            original_transport_ptr, source_transport_ptr,
            "Original backend transport should be same as source transport Arc"
        );
    }

    /// Test that cloning AutomergeIrohBackend shares the same backend Arc
    #[tokio::test]
    async fn test_clone_shares_backend_arc() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::storage::AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(crate::network::IrohTransport::new().await.unwrap());

        let original = AutomergeIrohBackend::from_parts(store, transport);
        let cloned = original.clone();

        // Verify backend Arc is shared (same pointer)
        // We need to access the internal backend field - using a helper method
        // Since backend is private, we verify via behavior: both should see same endpoint_id
        assert_eq!(
            original.endpoint_id(),
            cloned.endpoint_id(),
            "Clone should have same endpoint_id as original"
        );
    }

    /// Test that transport peer_count is consistent across clone
    ///
    /// This verifies that if connections are managed via one reference,
    /// they are visible via the clone (because they share the same transport).
    #[tokio::test]
    async fn test_clone_shares_peer_count() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::storage::AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(crate::network::IrohTransport::new().await.unwrap());

        let original = AutomergeIrohBackend::from_parts(store, Arc::clone(&transport));
        let cloned = original.clone();

        // Both should report the same peer count (0 initially)
        let original_count = original.transport().peer_count();
        let cloned_count = cloned.transport().peer_count();

        assert_eq!(
            original_count, cloned_count,
            "Original and clone should report same peer_count"
        );
        assert_eq!(original_count, 0, "Initial peer count should be 0");

        // Verify via source transport as well
        assert_eq!(
            transport.peer_count(),
            original_count,
            "Source transport should have same count"
        );
    }

    /// Test that formation_key is shared across clone
    #[tokio::test]
    async fn test_clone_shares_formation_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::storage::AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(crate::network::IrohTransport::new().await.unwrap());

        let original = AutomergeIrohBackend::from_parts(store, transport);

        // Initialize with credentials
        let test_secret = crate::security::FormationKey::generate_secret();
        let config = BackendConfig {
            app_id: "test_formation".to_string(),
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key: Some(test_secret),
            transport: TransportConfig::default(),
            extra: std::collections::HashMap::new(),
        };
        original.initialize(config).await.unwrap();

        // Clone after initialization
        let cloned = original.clone();

        // Both should see the formation key
        let original_key = original.formation_key();
        let cloned_key = cloned.formation_key();

        assert!(original_key.is_some(), "Original should have formation key");
        assert!(cloned_key.is_some(), "Clone should have formation key");
        assert_eq!(
            original_key.as_ref().map(|k| k.formation_id()),
            cloned_key.as_ref().map(|k| k.formation_id()),
            "Clone should share same formation key"
        );
    }

    /// Test that initialized state is shared across clone
    #[tokio::test]
    async fn test_clone_shares_initialized_state() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::storage::AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(crate::network::IrohTransport::new().await.unwrap());

        let original = AutomergeIrohBackend::from_parts(store, transport);

        // Before initialization
        let cloned_before = original.clone();
        assert!(
            !original.is_ready().await,
            "Original should not be ready before init"
        );
        assert!(
            !cloned_before.is_ready().await,
            "Clone should not be ready before init"
        );

        // Initialize original
        let test_secret = crate::security::FormationKey::generate_secret();
        let config = BackendConfig {
            app_id: "test_formation".to_string(),
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key: Some(test_secret),
            transport: TransportConfig::default(),
            extra: std::collections::HashMap::new(),
        };
        original.initialize(config).await.unwrap();

        // Clone created before init should NOW see it as ready
        // (because initialized flag is in shared Arc<Mutex<bool>>)
        assert!(
            original.is_ready().await,
            "Original should be ready after init"
        );
        assert!(
            cloned_before.is_ready().await,
            "Clone (created before init) should also be ready, proving Arc is shared"
        );
    }

    // === Deletion Tests (ADR-034) ===

    fn deletion_test_config() -> BackendConfig {
        let test_secret = crate::security::FormationKey::generate_secret();
        BackendConfig {
            app_id: "deletion_test".to_string(),
            persistence_dir: std::path::PathBuf::from("/tmp/deletion_test"),
            shared_key: Some(test_secret),
            transport: TransportConfig::default(),
            extra: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_soft_delete() {
        let backend = AutomergeBackend::new();
        backend.initialize(deletion_test_config()).await.unwrap();

        // Insert document
        let mut fields = HashMap::new();
        fields.insert("data".to_string(), serde_json::json!("test_value"));
        let doc = Document::new(fields);
        let doc_id = backend
            .document_store()
            .upsert("test_collection", doc)
            .await
            .unwrap();

        // Verify document exists
        let retrieved = backend
            .document_store()
            .get("test_collection", &doc_id)
            .await
            .unwrap();
        assert!(retrieved.is_some());
        assert!(!backend
            .document_store()
            .is_deleted("test_collection", &doc_id)
            .await
            .unwrap());

        // Delete (default policy is SoftDelete)
        let result = backend
            .document_store()
            .delete("test_collection", &doc_id, Some("test deletion"))
            .await
            .unwrap();
        assert!(result.deleted);

        // Document should now be marked as deleted
        assert!(backend
            .document_store()
            .is_deleted("test_collection", &doc_id)
            .await
            .unwrap());

        // Document should still exist (soft delete preserves it)
        let deleted_doc = backend
            .document_store()
            .get("test_collection", &doc_id)
            .await
            .unwrap();
        assert!(deleted_doc.is_some());
        let deleted_doc = deleted_doc.unwrap();
        assert_eq!(
            deleted_doc.fields.get("_deleted"),
            Some(&serde_json::json!(true))
        );
        assert!(deleted_doc.fields.contains_key("_deleted_at"));
        assert_eq!(
            deleted_doc.fields.get("_deleted_reason"),
            Some(&serde_json::json!("test deletion"))
        );
    }

    #[tokio::test]
    async fn test_tombstone_delete() {
        let backend = AutomergeBackend::new();
        backend.initialize(deletion_test_config()).await.unwrap();

        // Configure tombstone policy for this collection
        backend.deletion_policy_registry.set(
            "tombstone_collection",
            crate::qos::DeletionPolicy::Tombstone {
                tombstone_ttl: std::time::Duration::from_secs(3600),
                delete_wins: true,
            },
        );

        // Insert document
        let mut fields = HashMap::new();
        fields.insert("data".to_string(), serde_json::json!("tombstone_test"));
        let doc = Document::new(fields);
        let doc_id = backend
            .document_store()
            .upsert("tombstone_collection", doc)
            .await
            .unwrap();

        // Delete with tombstone policy
        let result = backend
            .document_store()
            .delete("tombstone_collection", &doc_id, Some("removed"))
            .await
            .unwrap();
        assert!(result.deleted);
        assert!(result.tombstone_id.is_some());
        assert!(result.expires_at.is_some());

        // Document should be deleted
        assert!(backend
            .document_store()
            .is_deleted("tombstone_collection", &doc_id)
            .await
            .unwrap());

        // Document should be removed (not just marked)
        let removed_doc = backend
            .document_store()
            .get("tombstone_collection", &doc_id)
            .await
            .unwrap();
        assert!(removed_doc.is_none());

        // Tombstone should exist
        let tombstones = backend
            .document_store()
            .get_tombstones("tombstone_collection")
            .await
            .unwrap();
        assert_eq!(tombstones.len(), 1);
        assert_eq!(tombstones[0].document_id, doc_id);
        assert_eq!(tombstones[0].reason, Some("removed".to_string()));
    }

    #[tokio::test]
    async fn test_deletion_policy() {
        let backend = AutomergeBackend::new();

        // Default policy is SoftDelete
        let policy = backend
            .document_store()
            .deletion_policy("unknown_collection");
        assert!(matches!(
            policy,
            crate::qos::DeletionPolicy::SoftDelete { .. }
        ));

        // Verify default policies for known collections
        assert!(matches!(
            backend.document_store().deletion_policy("beacons"),
            crate::qos::DeletionPolicy::ImplicitTTL { .. }
        ));
        assert!(matches!(
            backend.document_store().deletion_policy("nodes"),
            crate::qos::DeletionPolicy::Tombstone { .. }
        ));
        assert!(matches!(
            backend.document_store().deletion_policy("contact_reports"),
            crate::qos::DeletionPolicy::SoftDelete { .. }
        ));
    }

    #[tokio::test]
    async fn test_apply_tombstone() {
        let backend = AutomergeBackend::new();
        backend.initialize(deletion_test_config()).await.unwrap();

        // Insert document
        let mut fields = HashMap::new();
        fields.insert("data".to_string(), serde_json::json!("to_be_deleted"));
        let doc = Document::new(fields);
        let doc_id = backend
            .document_store()
            .upsert("sync_test", doc)
            .await
            .unwrap();

        // Create a tombstone (simulating receiving from sync)
        let tombstone = crate::qos::Tombstone::with_reason(
            doc_id.clone(),
            "sync_test".to_string(),
            "remote_node".to_string(),
            1, // Lamport timestamp
            "synced deletion",
        );

        // Apply tombstone
        backend
            .document_store()
            .apply_tombstone(&tombstone)
            .await
            .unwrap();

        // Document should be deleted
        assert!(backend
            .document_store()
            .is_deleted("sync_test", &doc_id)
            .await
            .unwrap());

        // Document should be removed
        let removed_doc = backend
            .document_store()
            .get("sync_test", &doc_id)
            .await
            .unwrap();
        assert!(removed_doc.is_none());
    }
}
