//! Beacon storage adapter for peat-mesh
//!
//! This adapter implements the BeaconStorage trait from peat-mesh using
//! the DataStore backend, enabling beacon data to be persisted and queried
//! through any backend that implements `DataStore`.

use crate::store::{ChangeEvent, DataStore};
use crate::types::Query;
use crate::{Error, Result};
use async_trait::async_trait;
use peat_mesh::beacon::{
    BeaconChangeEvent, BeaconChangeStream, BeaconStorage, GeographicBeacon, StorageError,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

const BEACON_COLLECTION: &str = "beacons";

/// Adapter that implements BeaconStorage using a DataStore backend
///
/// This allows the beacon system from peat-mesh to persist data through
/// any CRDT backend that implements the DataStore trait.
pub struct PersistentBeaconStorage {
    store: Arc<dyn DataStore>,
}

impl PersistentBeaconStorage {
    /// Create a new persistent beacon storage adapter
    pub fn new(store: Arc<dyn DataStore>) -> Self {
        Self { store }
    }

    /// Convert a GeographicBeacon to a JSON document
    fn beacon_to_document(beacon: &GeographicBeacon) -> Result<Value> {
        serde_json::to_value(beacon).map_err(Error::Serialization)
    }

    /// Convert a JSON document to a GeographicBeacon
    fn document_to_beacon(doc: &Value) -> Result<GeographicBeacon> {
        serde_json::from_value(doc.clone()).map_err(Error::Serialization)
    }

    /// Map DataStore Error to StorageError
    fn map_error(err: Error) -> StorageError {
        match err {
            Error::NotFound(msg) => StorageError::QueryFailed(msg),
            Error::Serialization(e) => StorageError::SaveFailed(format!("Serialization: {}", e)),
            Error::Backend(e) => StorageError::SaveFailed(format!("Backend: {}", e)),
            Error::InvalidQuery(msg) => StorageError::QueryFailed(msg),
            Error::Subscription(msg) => StorageError::SubscribeFailed(msg),
            Error::Transaction(msg) => StorageError::SaveFailed(format!("Transaction: {}", msg)),
            Error::Internal(msg) => StorageError::SaveFailed(format!("Internal: {}", msg)),
        }
    }
}

#[async_trait]
impl BeaconStorage for PersistentBeaconStorage {
    async fn save_beacon(
        &self,
        beacon: &GeographicBeacon,
    ) -> std::result::Result<(), StorageError> {
        debug!(
            "Saving beacon for node {} at geohash {}",
            beacon.node_id, beacon.geohash
        );

        let doc = Self::beacon_to_document(beacon).map_err(Self::map_error)?;

        // Save with node_id as the document ID for idempotent updates
        self.store
            .save(BEACON_COLLECTION, &doc)
            .await
            .map_err(Self::map_error)?;

        Ok(())
    }

    async fn query_by_geohash(
        &self,
        geohash_prefix: &str,
    ) -> std::result::Result<Vec<GeographicBeacon>, StorageError> {
        debug!("Querying beacons by geohash prefix: {}", geohash_prefix);

        // Build a query that filters by geohash prefix
        // Note: The exact query syntax depends on the backend's capabilities
        // For now, we query all and filter in memory
        let all_docs = self
            .store
            .query(BEACON_COLLECTION, Query::all())
            .await
            .map_err(Self::map_error)?;

        let mut beacons = Vec::new();
        for doc in all_docs {
            match Self::document_to_beacon(&doc) {
                Ok(beacon) => {
                    if beacon.geohash.starts_with(geohash_prefix) {
                        beacons.push(beacon);
                    }
                }
                Err(e) => {
                    warn!("Failed to deserialize beacon: {}", e);
                }
            }
        }

        debug!("Found {} beacons for geohash prefix", beacons.len());
        Ok(beacons)
    }

    async fn query_all(&self) -> std::result::Result<Vec<GeographicBeacon>, StorageError> {
        debug!("Querying all beacons");

        let all_docs = self
            .store
            .query(BEACON_COLLECTION, Query::all())
            .await
            .map_err(Self::map_error)?;

        let mut beacons = Vec::new();
        for doc in all_docs {
            match Self::document_to_beacon(&doc) {
                Ok(beacon) => beacons.push(beacon),
                Err(e) => {
                    warn!("Failed to deserialize beacon: {}", e);
                }
            }
        }

        debug!("Found {} total beacons", beacons.len());
        Ok(beacons)
    }

    async fn subscribe(&self) -> std::result::Result<BeaconChangeStream, StorageError> {
        debug!("Subscribing to beacon changes");

        let mut store_stream = self
            .store
            .observe(BEACON_COLLECTION, Query::all())
            .await
            .map_err(Self::map_error)?;

        // Create a channel to forward mapped events
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn a task to map ChangeEvents to BeaconChangeEvents
        tokio::spawn(async move {
            while let Some(event) = store_stream.recv().await {
                let beacon_event = match event {
                    ChangeEvent::Upsert { document, .. } => {
                        // Extract the Value from the Document struct
                        match serde_json::from_value::<GeographicBeacon>(document.fields.clone()) {
                            Ok(beacon) => {
                                // Determine if this is an insert or update based on whether
                                // we've seen this node_id before. For simplicity, treat all
                                // upserts as updates since the beacon system treats them
                                // the same way.
                                Some(BeaconChangeEvent::Updated(beacon))
                            }
                            Err(e) => {
                                error!("Failed to deserialize beacon from change event: {}", e);
                                None
                            }
                        }
                    }
                    ChangeEvent::Delete { id } => {
                        // Extract node_id from document ID
                        Some(BeaconChangeEvent::Removed {
                            node_id: id.to_string(),
                        })
                    }
                    ChangeEvent::Initial { count } => {
                        debug!("Received initial snapshot with {} beacons", count);
                        None // Don't forward initial events to beacon observers
                    }
                };

                if let Some(event) = beacon_event {
                    if tx.send(event).is_err() {
                        debug!("Beacon change stream receiver dropped, stopping forwarding");
                        break;
                    }
                }
            }
        });

        // Convert the receiver into a Stream
        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoreInfo;
    use crate::types::{Document, DocumentId, DocumentMetadata};
    use futures::StreamExt;
    use peat_mesh::beacon::{GeoPosition, GeographicBeacon, HierarchyLevel};
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    /// Mock DataStore for testing the adapter
    struct MockDataStore {
        documents: Arc<RwLock<HashMap<String, Value>>>,
        observers: Arc<RwLock<Vec<mpsc::UnboundedSender<ChangeEvent>>>>,
    }

    impl MockDataStore {
        fn new() -> Self {
            Self {
                documents: Arc::new(RwLock::new(HashMap::new())),
                observers: Arc::new(RwLock::new(Vec::new())),
            }
        }

        async fn notify_observers(&self, event: ChangeEvent) {
            let observers = self.observers.read().await;
            for tx in observers.iter() {
                let _ = tx.send(event.clone());
            }
        }
    }

    #[async_trait]
    impl DataStore for MockDataStore {
        async fn save(&self, collection: &str, document: &Value) -> Result<DocumentId> {
            assert_eq!(collection, BEACON_COLLECTION);

            let node_id = document["node_id"]
                .as_str()
                .ok_or_else(|| Error::Internal("Missing node_id".to_string()))?
                .to_string();

            let id = DocumentId::from(node_id.clone());
            self.documents
                .write()
                .await
                .insert(node_id.clone(), document.clone());

            self.notify_observers(ChangeEvent::Upsert {
                id: id.clone(),
                document: Document {
                    id: Some(id.clone()),
                    fields: document.clone(),
                    metadata: DocumentMetadata::default(),
                },
            })
            .await;

            Ok(id)
        }

        async fn query(&self, collection: &str, _query: Query) -> Result<Vec<Value>> {
            assert_eq!(collection, BEACON_COLLECTION);
            Ok(self.documents.read().await.values().cloned().collect())
        }

        async fn find_by_id(&self, _collection: &str, _id: &DocumentId) -> Result<Value> {
            unimplemented!("Not needed for beacon tests")
        }

        async fn delete(&self, _collection: &str, _id: &DocumentId) -> Result<()> {
            unimplemented!("Not needed for beacon tests")
        }

        async fn observe(
            &self,
            collection: &str,
            _query: Query,
        ) -> Result<mpsc::UnboundedReceiver<ChangeEvent>> {
            assert_eq!(collection, BEACON_COLLECTION);

            let (tx, rx) = mpsc::unbounded_channel();
            self.observers.write().await.push(tx);

            Ok(rx)
        }

        fn store_info(&self) -> StoreInfo {
            StoreInfo {
                name: "mock".to_string(),
                version: "0.1.0".to_string(),
                properties: std::collections::HashMap::new(),
            }
        }
    }

    fn create_test_beacon(node_id: &str) -> GeographicBeacon {
        GeographicBeacon::new(
            node_id.to_string(),
            GeoPosition::new(37.7749, -122.4194), // San Francisco
            HierarchyLevel::Platform,
        )
    }

    #[tokio::test]
    async fn test_save_and_query_beacon() {
        let mock_store = Arc::new(MockDataStore::new());
        let storage = PersistentBeaconStorage::new(mock_store);

        // Use different positions to get different geohashes
        let beacon1 = GeographicBeacon::new(
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194), // San Francisco
            HierarchyLevel::Platform,
        );
        let beacon2 = GeographicBeacon::new(
            "node-2".to_string(),
            GeoPosition::new(34.0522, -118.2437), // Los Angeles
            HierarchyLevel::Platform,
        );

        // Save beacons
        storage
            .save_beacon(&beacon1)
            .await
            .expect("Failed to save beacon1");
        storage
            .save_beacon(&beacon2)
            .await
            .expect("Failed to save beacon2");

        // Query all
        let all = storage.query_all().await.expect("Failed to query all");
        assert_eq!(all.len(), 2);

        // Query by geohash prefix using beacon1's geohash
        let geohash_prefix = &beacon1.geohash[..5]; // Use first 5 chars as prefix
        let filtered = storage
            .query_by_geohash(geohash_prefix)
            .await
            .expect("Failed to query by geohash");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].node_id, "node-1");
    }

    #[tokio::test]
    async fn test_beacon_change_stream() {
        let mock_store = Arc::new(MockDataStore::new());
        let storage = PersistentBeaconStorage::new(mock_store.clone());

        // Subscribe before saving
        let mut stream = storage.subscribe().await.expect("Failed to subscribe");

        // Save a beacon
        let beacon = create_test_beacon("node-1");
        storage
            .save_beacon(&beacon)
            .await
            .expect("Failed to save beacon");

        // Should receive an update event
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            if let Some(event) = stream.next().await {
                match event {
                    BeaconChangeEvent::Updated(b) => {
                        assert_eq!(b.node_id, "node-1");
                    }
                    _ => panic!("Expected Updated event"),
                }
            } else {
                panic!("Expected to receive an event");
            }
        })
        .await
        .expect("Timeout waiting for event");
    }

    #[tokio::test]
    async fn test_idempotent_beacon_saves() {
        let mock_store = Arc::new(MockDataStore::new());
        let storage = PersistentBeaconStorage::new(mock_store);

        let beacon = create_test_beacon("node-1");

        // Save same beacon twice
        storage
            .save_beacon(&beacon)
            .await
            .expect("Failed to save beacon first time");
        storage
            .save_beacon(&beacon)
            .await
            .expect("Failed to save beacon second time");

        // Should still only have one beacon
        let all = storage.query_all().await.expect("Failed to query all");
        assert_eq!(all.len(), 1);
    }
}
