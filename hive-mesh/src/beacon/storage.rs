use super::types::GeographicBeacon;
use async_trait::async_trait;
use std::error::Error as StdError;
use std::fmt;

/// Error type for beacon storage operations
#[derive(Debug)]
pub enum StorageError {
    /// Failed to save beacon
    SaveFailed(String),

    /// Failed to query beacons
    QueryFailed(String),

    /// Failed to subscribe to beacon updates
    SubscribeFailed(String),

    /// Generic storage error
    Other(Box<dyn StdError + Send + Sync>),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::SaveFailed(msg) => write!(f, "Save failed: {}", msg),
            StorageError::QueryFailed(msg) => write!(f, "Query failed: {}", msg),
            StorageError::SubscribeFailed(msg) => write!(f, "Subscribe failed: {}", msg),
            StorageError::Other(err) => write!(f, "Storage error: {}", err),
        }
    }
}

impl StdError for StorageError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            StorageError::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Storage abstraction for geographic beacons
///
/// This trait defines the storage operations needed by the beacon system
/// without coupling to any specific backend implementation. Implementations
/// can use Ditto, in-memory storage, or any other CRDT-based system.
///
/// # Design Principles
///
/// - **Backend Agnostic**: No direct dependency on storage backends
/// - **Testable**: Easy to mock for unit tests
/// - **Async**: All operations are async for non-blocking I/O
/// - **CRDT-Friendly**: Designed for eventual consistency
#[async_trait]
pub trait BeaconStorage: Send + Sync {
    /// Save a beacon to storage
    ///
    /// This operation should be idempotent - saving the same beacon
    /// multiple times should result in the same state.
    ///
    /// # Arguments
    ///
    /// * `beacon` - The geographic beacon to save
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Beacon saved successfully
    /// * `Err(StorageError)` - Save operation failed
    async fn save_beacon(&self, beacon: &GeographicBeacon) -> Result<()>;

    /// Query beacons by geohash prefix
    ///
    /// Retrieves all beacons matching the given geohash prefix.
    /// This allows for proximity-based queries.
    ///
    /// # Arguments
    ///
    /// * `geohash_prefix` - Geohash prefix to match (e.g., "9q8" for San Francisco area)
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<GeographicBeacon>)` - List of matching beacons
    /// * `Err(StorageError)` - Query operation failed
    async fn query_by_geohash(&self, geohash_prefix: &str) -> Result<Vec<GeographicBeacon>>;

    /// Query all beacons
    ///
    /// Retrieves all beacons in the system. Use with caution in
    /// large deployments.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<GeographicBeacon>)` - List of all beacons
    /// * `Err(StorageError)` - Query operation failed
    async fn query_all(&self) -> Result<Vec<GeographicBeacon>>;

    /// Subscribe to beacon changes
    ///
    /// Returns a stream of beacon updates (inserts, updates, deletes).
    /// Implementations should use CRDT change notifications to provide
    /// real-time updates.
    ///
    /// # Returns
    ///
    /// * `Ok(BeaconChangeStream)` - Stream of beacon changes
    /// * `Err(StorageError)` - Subscribe operation failed
    async fn subscribe(&self) -> Result<BeaconChangeStream>;
}

/// Type alias for beacon change stream
///
/// This represents a stream of beacon change events that observers
/// can consume to track beacon updates in real-time.
pub type BeaconChangeStream = Box<dyn futures::Stream<Item = BeaconChangeEvent> + Send + Unpin>;

/// Beacon change event
///
/// Represents a change to a beacon in the storage system.
#[derive(Debug, Clone)]
pub enum BeaconChangeEvent {
    /// A new beacon was inserted
    Inserted(GeographicBeacon),

    /// An existing beacon was updated
    Updated(GeographicBeacon),

    /// A beacon was removed (expired or explicitly deleted)
    Removed { node_id: String },
}

#[cfg(test)]
pub use tests::MockBeaconStorage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::types::{GeoPosition, HierarchyLevel};
    use futures::stream;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Mock in-memory beacon storage for testing
    pub struct MockBeaconStorage {
        beacons: Arc<Mutex<Vec<GeographicBeacon>>>,
    }

    impl Default for MockBeaconStorage {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockBeaconStorage {
        pub fn new() -> Self {
            Self {
                beacons: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl BeaconStorage for MockBeaconStorage {
        async fn save_beacon(&self, beacon: &GeographicBeacon) -> Result<()> {
            let mut beacons = self.beacons.lock().await;

            // Update existing or insert new
            if let Some(existing) = beacons.iter_mut().find(|b| b.node_id == beacon.node_id) {
                *existing = beacon.clone();
            } else {
                beacons.push(beacon.clone());
            }

            Ok(())
        }

        async fn query_by_geohash(&self, geohash_prefix: &str) -> Result<Vec<GeographicBeacon>> {
            let beacons = self.beacons.lock().await;
            Ok(beacons
                .iter()
                .filter(|b| b.geohash.starts_with(geohash_prefix))
                .cloned()
                .collect())
        }

        async fn query_all(&self) -> Result<Vec<GeographicBeacon>> {
            let beacons = self.beacons.lock().await;
            Ok(beacons.clone())
        }

        async fn subscribe(&self) -> Result<BeaconChangeStream> {
            // Return empty stream for mock
            Ok(Box::new(stream::empty()))
        }
    }

    #[tokio::test]
    async fn test_mock_storage_save_and_query() {
        let storage = MockBeaconStorage::new();

        let beacon = GeographicBeacon::new(
            "test-node".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Platform,
        );

        // Save beacon
        storage.save_beacon(&beacon).await.unwrap();

        // Query all
        let all_beacons = storage.query_all().await.unwrap();
        assert_eq!(all_beacons.len(), 1);
        assert_eq!(all_beacons[0].node_id, "test-node");

        // Query by geohash prefix
        let nearby = storage.query_by_geohash("9q8").await.unwrap();
        assert_eq!(nearby.len(), 1);
    }

    #[tokio::test]
    async fn test_mock_storage_idempotent_save() {
        let storage = MockBeaconStorage::new();

        let beacon = GeographicBeacon::new(
            "test-node".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Platform,
        );

        // Save same beacon twice
        storage.save_beacon(&beacon).await.unwrap();
        storage.save_beacon(&beacon).await.unwrap();

        // Should only have one beacon
        let all_beacons = storage.query_all().await.unwrap();
        assert_eq!(all_beacons.len(), 1);
    }

    #[test]
    fn test_storage_error_display() {
        let err = StorageError::SaveFailed("disk full".to_string());
        assert_eq!(err.to_string(), "Save failed: disk full");

        let err = StorageError::QueryFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Query failed: timeout");

        let err = StorageError::SubscribeFailed("connection lost".to_string());
        assert_eq!(err.to_string(), "Subscribe failed: connection lost");

        let inner = std::io::Error::new(std::io::ErrorKind::Other, "io error");
        let err = StorageError::Other(Box::new(inner));
        assert!(err.to_string().contains("Storage error"));
    }

    #[test]
    fn test_storage_error_source() {
        // SaveFailed, QueryFailed, SubscribeFailed have no source
        let err = StorageError::SaveFailed("test".to_string());
        assert!(err.source().is_none());

        let err = StorageError::QueryFailed("test".to_string());
        assert!(err.source().is_none());

        let err = StorageError::SubscribeFailed("test".to_string());
        assert!(err.source().is_none());

        // Other has a source
        let inner = std::io::Error::new(std::io::ErrorKind::Other, "io error");
        let err = StorageError::Other(Box::new(inner));
        assert!(err.source().is_some());
    }

    #[test]
    fn test_storage_error_debug() {
        let err = StorageError::SaveFailed("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("SaveFailed"));
    }

    #[test]
    fn test_beacon_change_event_variants() {
        let beacon = GeographicBeacon::new(
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );

        let inserted = BeaconChangeEvent::Inserted(beacon.clone());
        let updated = BeaconChangeEvent::Updated(beacon);
        let removed = BeaconChangeEvent::Removed {
            node_id: "node-1".to_string(),
        };

        // Verify Debug works
        let _ = format!("{:?}", inserted);
        let _ = format!("{:?}", updated);
        let _ = format!("{:?}", removed);
    }

    #[tokio::test]
    async fn test_mock_storage_query_by_geohash_no_match() {
        let storage = MockBeaconStorage::new();

        let beacon = GeographicBeacon::new(
            "test-node".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Platform,
        );
        storage.save_beacon(&beacon).await.unwrap();

        // Query with a non-matching prefix
        let result = storage.query_by_geohash("xyz").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_mock_storage_multiple_beacons() {
        let storage = MockBeaconStorage::new();

        let beacon1 = GeographicBeacon::new(
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );
        let beacon2 = GeographicBeacon::new(
            "node-2".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Squad,
        );
        let beacon3 = GeographicBeacon::new(
            "node-3".to_string(),
            GeoPosition::new(40.7128, -74.0060), // New York
            HierarchyLevel::Platoon,
        );

        storage.save_beacon(&beacon1).await.unwrap();
        storage.save_beacon(&beacon2).await.unwrap();
        storage.save_beacon(&beacon3).await.unwrap();

        let all = storage.query_all().await.unwrap();
        assert_eq!(all.len(), 3);

        // Both SF beacons start with "9q8"
        let sf_beacons = storage.query_by_geohash("9q8").await.unwrap();
        assert_eq!(sf_beacons.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_storage_subscribe_returns_empty() {
        let storage = MockBeaconStorage::new();
        let stream = storage.subscribe().await;
        assert!(stream.is_ok());
    }

    #[test]
    fn test_mock_beacon_storage_default() {
        let storage = MockBeaconStorage::default();
        // Should work the same as new()
        let _ = format!("{:?}", "created storage");
        assert!(storage.beacons.try_lock().unwrap().is_empty());
    }
}
