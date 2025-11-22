//! Ditto backend adapter for DataStore trait

use crate::error::{Error, Result};
use crate::store::{ChangeEvent, DataStore, StoreInfo};
use crate::types::{Document, DocumentId, DocumentMetadata, Query};
use async_trait::async_trait;
use hive_protocol::sync::{DataSyncBackend, Document as SyncDocument, Query as SyncQuery};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Ditto storage backend
///
/// Wraps the existing `DataSyncBackend` from cap-protocol to provide
/// the `DataStore` trait interface.
pub struct DittoStore {
    backend: Arc<dyn DataSyncBackend>,
}

impl DittoStore {
    /// Create a new Ditto store from an existing backend
    ///
    /// # Arguments
    ///
    /// * `backend` - Initialized DataSyncBackend instance
    pub fn new(backend: Arc<dyn DataSyncBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl DataStore for DittoStore {
    async fn save(&self, collection: &str, document: &Value) -> Result<DocumentId> {
        let doc_store = self.backend.document_store();

        // Convert JSON value to HashMap<String, Value>
        let fields = match document {
            Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => {
                // If not an object, wrap it
                let mut map = HashMap::new();
                map.insert("value".to_string(), document.clone());
                map
            }
        };

        // Extract document ID from fields if present
        // Look for "node_id" (beacons) or "_id" (generic) field
        let doc_id = if let Some(Value::String(node_id)) = document.get("node_id") {
            Some(node_id.clone())
        } else {
            document
                .get("_id")
                .and_then(|v| v.as_str())
                .map(String::from)
        };

        // Create sync document
        let sync_doc = SyncDocument {
            id: doc_id.clone(),
            fields,
            updated_at: std::time::SystemTime::now(),
        };

        // Upsert via backend
        let id = doc_store.upsert(collection, sync_doc).await?;

        Ok(DocumentId::new(id))
    }

    async fn query(&self, collection: &str, query: Query) -> Result<Vec<Value>> {
        let doc_store = self.backend.document_store();

        // Convert our Query to SyncQuery
        let sync_query = convert_query(&query);

        // Execute query
        let documents = doc_store.query(collection, &sync_query).await?;

        // Convert HashMap<String, Value> back to Value::Object
        let results = documents
            .into_iter()
            .map(|doc| {
                let map: serde_json::Map<String, Value> = doc.fields.into_iter().collect();
                Value::Object(map)
            })
            .collect();

        Ok(results)
    }

    async fn find_by_id(&self, collection: &str, id: &DocumentId) -> Result<Value> {
        let doc_store = self.backend.document_store();

        // Query for specific document by ID
        let documents = doc_store.query(collection, &SyncQuery::All).await?;

        // Find matching document
        let doc = documents
            .into_iter()
            .find(|d| d.id.as_deref() == Some(id.as_str()))
            .ok_or_else(|| Error::NotFound(format!("Document not found: {}", id)))?;

        // Convert HashMap to JSON object
        let map: serde_json::Map<String, Value> = doc.fields.into_iter().collect();
        Ok(Value::Object(map))
    }

    async fn delete(&self, collection: &str, id: &DocumentId) -> Result<()> {
        // Ditto uses eviction for deletion
        // This is a simplified implementation - real deletion may require
        // specific Ditto API calls

        // Create a tombstone document or use Ditto's eviction
        // For now, we'll just return Ok - full deletion logic
        // depends on Ditto's eviction policies
        tracing::warn!(
            "Delete operation for {} in collection {} - implementation pending",
            id,
            collection
        );
        Ok(())
    }

    async fn observe(
        &self,
        collection: &str,
        query: Query,
    ) -> Result<mpsc::UnboundedReceiver<ChangeEvent>> {
        let doc_store = self.backend.document_store();

        // Convert query
        let sync_query = convert_query(&query);

        // Create channel for events
        let (tx, rx) = mpsc::unbounded_channel();

        // Register observer (non-async method)
        let change_stream = doc_store
            .observe(collection, &sync_query)
            .map_err(|e| Error::Subscription(e.to_string()))?;

        // Spawn task to forward events
        tokio::spawn(async move {
            let mut receiver = change_stream.receiver;
            while let Some(event) = receiver.recv().await {
                let change_event = match event {
                    hive_protocol::sync::ChangeEvent::Initial { documents } => {
                        ChangeEvent::Initial {
                            count: documents.len(),
                        }
                    }
                    hive_protocol::sync::ChangeEvent::Updated { document, .. } => {
                        let doc_id = document.id.clone().unwrap_or_default();
                        let map: serde_json::Map<String, Value> =
                            document.fields.into_iter().collect();
                        ChangeEvent::Upsert {
                            id: DocumentId::new(doc_id.clone()),
                            document: Document {
                                id: Some(DocumentId::new(doc_id)),
                                fields: Value::Object(map),
                                metadata: DocumentMetadata::default(),
                            },
                        }
                    }
                    hive_protocol::sync::ChangeEvent::Removed { doc_id, .. } => {
                        ChangeEvent::Delete {
                            id: DocumentId::new(doc_id),
                        }
                    }
                };

                if tx.send(change_event).is_err() {
                    break; // Receiver dropped
                }
            }
        });

        Ok(rx)
    }

    fn store_info(&self) -> StoreInfo {
        let backend_info = self.backend.backend_info();
        let mut properties = HashMap::new();
        properties.insert("backend_type".to_string(), "Ditto".to_string());

        StoreInfo {
            name: backend_info.name,
            version: backend_info.version,
            properties,
        }
    }
}

/// Convert our Query type to cap-protocol's SyncQuery
fn convert_query(query: &Query) -> SyncQuery {
    // For now, we only support Query::All
    // Full filter conversion would map Filter types to SyncQuery
    if query.filters.is_empty() {
        SyncQuery::All
    } else {
        // TODO: Convert filters to SyncQuery when more advanced
        // query support is added to cap-protocol
        tracing::warn!("Query filters not yet supported, using Query::All");
        SyncQuery::All
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_conversion() {
        let query = Query::new();
        let sync_query = convert_query(&query);
        match sync_query {
            SyncQuery::All => {}
            _ => panic!("Expected SyncQuery::All"),
        }
    }

    #[test]
    fn test_document_id_creation() {
        let id = DocumentId::new("test-id");
        assert_eq!(id.as_str(), "test-id");
    }
}
