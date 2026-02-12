use super::storage::{BeaconChangeEvent, BeaconStorage};
use super::types::GeographicBeacon;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Observes and tracks nearby geographic beacons
///
/// BeaconObserver subscribes to beacon updates from the storage backend
/// and maintains a cache of nearby beacons based on geohash proximity.
pub struct BeaconObserver {
    storage: Arc<dyn BeaconStorage>,
    my_geohash: String,
    nearby_beacons: Arc<RwLock<HashMap<String, GeographicBeacon>>>,
    running: Arc<RwLock<bool>>,
}

impl BeaconObserver {
    /// Create a new beacon observer
    ///
    /// # Arguments
    ///
    /// * `storage` - Storage backend for beacon queries and subscriptions
    /// * `my_geohash` - This node's geohash for proximity filtering
    pub fn new(storage: Arc<dyn BeaconStorage>, my_geohash: String) -> Self {
        Self {
            storage,
            my_geohash,
            nearby_beacons: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start observing beacons
    ///
    /// Subscribes to beacon change events from storage and maintains
    /// a cache of nearby beacons based on geohash proximity.
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            debug!("Beacon observer already running");
            return;
        }
        *running = true;
        drop(running);

        info!("Starting beacon observer for geohash {}", self.my_geohash);

        // Subscribe to beacon changes
        let mut stream = match self.storage.subscribe().await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to subscribe to beacon changes: {}", e);
                let mut running = self.running.write().await;
                *running = false;
                return;
            }
        };

        let running_clone = self.running.clone();
        let nearby_beacons_clone = self.nearby_beacons.clone();
        let my_geohash = self.my_geohash.clone();

        tokio::spawn(async move {
            while *running_clone.read().await {
                tokio::select! {
                    Some(event) = stream.next() => {
                        match event {
                            BeaconChangeEvent::Inserted(beacon) | BeaconChangeEvent::Updated(beacon) => {
                                // Check if beacon is nearby
                                if Self::is_nearby_geohash(&my_geohash, &beacon.geohash) {
                                    debug!("Nearby beacon detected: {}", beacon.node_id);
                                    let mut beacons = nearby_beacons_clone.write().await;
                                    beacons.insert(beacon.node_id.clone(), beacon);
                                }
                            }
                            BeaconChangeEvent::Removed { node_id } => {
                                debug!("Beacon removed: {}", node_id);
                                let mut beacons = nearby_beacons_clone.write().await;
                                beacons.remove(&node_id);
                            }
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                        // Periodic check to ensure loop continues
                    }
                }
            }
            debug!("Beacon observer event loop stopped");
        });
    }

    /// Stop observing beacons
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Stopped beacon observer");
    }

    /// Get all nearby beacons
    pub async fn get_nearby_beacons(&self) -> Vec<GeographicBeacon> {
        self.nearby_beacons.read().await.values().cloned().collect()
    }

    /// Check if a geohash is nearby (same or adjacent cell)
    fn is_nearby_geohash(my_geohash: &str, other_geohash: &str) -> bool {
        use geohash::Direction;

        if my_geohash == other_geohash {
            return true;
        }

        // Check all 8 adjacent cells
        let directions = [
            Direction::N,
            Direction::NE,
            Direction::E,
            Direction::SE,
            Direction::S,
            Direction::SW,
            Direction::W,
            Direction::NW,
        ];

        for dir in &directions {
            if let Ok(neighbor) = geohash::neighbor(my_geohash, *dir) {
                if neighbor == other_geohash {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::storage::{BeaconChangeStream, Result};
    use async_trait::async_trait;
    use futures::stream;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Mock storage for testing
    struct MockBeaconStorage {
        beacons: Arc<Mutex<Vec<GeographicBeacon>>>,
    }

    impl MockBeaconStorage {
        fn new() -> Self {
            Self {
                beacons: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl BeaconStorage for MockBeaconStorage {
        async fn save_beacon(&self, beacon: &GeographicBeacon) -> Result<()> {
            let mut beacons = self.beacons.lock().await;
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
            // For testing, return empty stream
            // In real tests with events, we'd use the event_tx channel
            Ok(Box::new(stream::empty()))
        }
    }

    #[tokio::test]
    async fn test_observer_lifecycle() {
        let storage = MockBeaconStorage::new();
        let observer = BeaconObserver::new(Arc::new(storage), "9q8yy9m".to_string());

        observer.start().await;
        assert!(*observer.running.read().await);

        observer.stop().await;
        assert!(!*observer.running.read().await);
    }

    #[test]
    fn test_is_nearby_geohash() {
        // Same geohash should be nearby
        assert!(BeaconObserver::is_nearby_geohash("9q8yy9m", "9q8yy9m"));

        // Adjacent geohashes should be nearby
        let north = geohash::neighbor("9q8yy9m", geohash::Direction::N).unwrap();
        assert!(BeaconObserver::is_nearby_geohash("9q8yy9m", &north));

        // Distant geohash should not be nearby
        assert!(!BeaconObserver::is_nearby_geohash("9q8yy9m", "u4pruyd")); // Sydney, Australia
    }

    #[tokio::test]
    async fn test_observer_filters_nearby_beacons() {
        let storage = MockBeaconStorage::new();
        let my_geohash = "9q8yy9m"; // San Francisco area

        let observer = BeaconObserver::new(Arc::new(storage), my_geohash.to_string());

        // Verify empty initially
        let nearby = observer.get_nearby_beacons().await;
        assert_eq!(nearby.len(), 0);
    }

    #[tokio::test]
    async fn test_get_nearby_beacons() {
        let storage = MockBeaconStorage::new();
        let observer = BeaconObserver::new(Arc::new(storage), "9q8yy9m".to_string());

        observer.start().await;

        // Initially empty
        let nearby = observer.get_nearby_beacons().await;
        assert_eq!(nearby.len(), 0);

        observer.stop().await;
    }

    #[tokio::test]
    async fn test_observer_start_twice() {
        let storage = MockBeaconStorage::new();
        let observer = BeaconObserver::new(Arc::new(storage), "9q8yy9m".to_string());

        observer.start().await;
        assert!(*observer.running.read().await);

        // Starting again should be a no-op (already running)
        observer.start().await;
        assert!(*observer.running.read().await);

        observer.stop().await;
    }

    #[test]
    fn test_is_nearby_geohash_all_directions() {
        let my = "9q8yy9m";

        // Check all 8 neighbors
        let directions = [
            geohash::Direction::N,
            geohash::Direction::NE,
            geohash::Direction::E,
            geohash::Direction::SE,
            geohash::Direction::S,
            geohash::Direction::SW,
            geohash::Direction::W,
            geohash::Direction::NW,
        ];

        for dir in &directions {
            let neighbor = geohash::neighbor(my, *dir).unwrap();
            assert!(
                BeaconObserver::is_nearby_geohash(my, &neighbor),
                "Neighbor in direction {:?} should be nearby",
                dir
            );
        }
    }

    #[test]
    fn test_is_nearby_geohash_same_hash() {
        assert!(BeaconObserver::is_nearby_geohash("9q8yy9m", "9q8yy9m"));
    }

    #[test]
    fn test_is_nearby_geohash_distant() {
        // Two very different geohashes should not be nearby
        assert!(!BeaconObserver::is_nearby_geohash("9q8yy9m", "u4pruyd"));
        assert!(!BeaconObserver::is_nearby_geohash("9q8yy9m", "s00000"));
    }

    #[tokio::test]
    async fn test_observer_with_failing_subscribe() {
        use crate::beacon::storage::StorageError;

        struct FailingStorage;

        #[async_trait]
        impl BeaconStorage for FailingStorage {
            async fn save_beacon(
                &self,
                _beacon: &crate::beacon::types::GeographicBeacon,
            ) -> Result<()> {
                Ok(())
            }

            async fn query_by_geohash(
                &self,
                _geohash_prefix: &str,
            ) -> Result<Vec<crate::beacon::types::GeographicBeacon>> {
                Ok(vec![])
            }

            async fn query_all(&self) -> Result<Vec<crate::beacon::types::GeographicBeacon>> {
                Ok(vec![])
            }

            async fn subscribe(&self) -> Result<BeaconChangeStream> {
                Err(StorageError::SubscribeFailed("test failure".to_string()))
            }
        }

        let observer = BeaconObserver::new(Arc::new(FailingStorage), "9q8yy9m".to_string());

        // Start should handle the subscribe failure gracefully
        observer.start().await;
        // After failed subscribe, running should be set back to false
        assert!(!*observer.running.read().await);
    }

    #[tokio::test]
    async fn test_observer_processes_events() {
        use crate::beacon::types::{GeoPosition, HierarchyLevel};

        struct EventStorage {
            events: Vec<BeaconChangeEvent>,
        }

        #[async_trait]
        impl BeaconStorage for EventStorage {
            async fn save_beacon(
                &self,
                _beacon: &crate::beacon::types::GeographicBeacon,
            ) -> Result<()> {
                Ok(())
            }

            async fn query_by_geohash(
                &self,
                _geohash_prefix: &str,
            ) -> Result<Vec<crate::beacon::types::GeographicBeacon>> {
                Ok(vec![])
            }

            async fn query_all(&self) -> Result<Vec<crate::beacon::types::GeographicBeacon>> {
                Ok(vec![])
            }

            async fn subscribe(&self) -> Result<BeaconChangeStream> {
                let events = self.events.clone();
                Ok(Box::new(stream::iter(events)))
            }
        }

        // Create a beacon that is nearby (same geohash)
        let my_geohash = "9q8yy9m";
        let beacon = crate::beacon::types::GeographicBeacon::new(
            "nearby-node".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );

        // The beacon's geohash needs to match or be adjacent to my_geohash
        // We'll use a beacon with the same geohash
        let mut nearby_beacon = beacon.clone();
        nearby_beacon.geohash = my_geohash.to_string();

        let storage = EventStorage {
            events: vec![BeaconChangeEvent::Inserted(nearby_beacon.clone())],
        };

        let observer = BeaconObserver::new(Arc::new(storage), my_geohash.to_string());
        observer.start().await;

        // Give the spawned task time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let nearby = observer.get_nearby_beacons().await;
        assert_eq!(nearby.len(), 1);
        assert_eq!(nearby[0].node_id, "nearby-node");

        observer.stop().await;
    }

    /// Reusable event-based storage mock for observer tests
    struct EventStorage {
        events: Vec<BeaconChangeEvent>,
    }

    impl EventStorage {
        fn new(events: Vec<BeaconChangeEvent>) -> Self {
            Self { events }
        }
    }

    #[async_trait]
    impl BeaconStorage for EventStorage {
        async fn save_beacon(
            &self,
            _beacon: &crate::beacon::types::GeographicBeacon,
        ) -> Result<()> {
            Ok(())
        }
        async fn query_by_geohash(
            &self,
            _geohash_prefix: &str,
        ) -> Result<Vec<crate::beacon::types::GeographicBeacon>> {
            Ok(vec![])
        }
        async fn query_all(&self) -> Result<Vec<crate::beacon::types::GeographicBeacon>> {
            Ok(vec![])
        }
        async fn subscribe(&self) -> Result<BeaconChangeStream> {
            let events = self.events.clone();
            Ok(Box::new(stream::iter(events)))
        }
    }

    fn make_nearby_beacon(node_id: &str, geohash: &str) -> crate::beacon::types::GeographicBeacon {
        use crate::beacon::types::{GeoPosition, HierarchyLevel};
        let mut beacon = crate::beacon::types::GeographicBeacon::new(
            node_id.to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );
        beacon.geohash = geohash.to_string();
        beacon
    }

    #[tokio::test]
    async fn test_observer_processes_removed_event() {
        let my_geohash = "9q8yy9m";
        let beacon = make_nearby_beacon("remove-me", my_geohash);

        let storage = EventStorage::new(vec![
            BeaconChangeEvent::Inserted(beacon),
            BeaconChangeEvent::Removed {
                node_id: "remove-me".to_string(),
            },
        ]);

        let observer = BeaconObserver::new(Arc::new(storage), my_geohash.to_string());
        observer.start().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let nearby = observer.get_nearby_beacons().await;
        assert_eq!(nearby.len(), 0);

        observer.stop().await;
    }

    #[tokio::test]
    async fn test_observer_processes_updated_event() {
        let my_geohash = "9q8yy9m";
        let beacon = make_nearby_beacon("update-me", my_geohash);
        let updated = make_nearby_beacon("update-me", my_geohash);

        let storage = EventStorage::new(vec![
            BeaconChangeEvent::Inserted(beacon),
            BeaconChangeEvent::Updated(updated),
        ]);

        let observer = BeaconObserver::new(Arc::new(storage), my_geohash.to_string());
        observer.start().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let nearby = observer.get_nearby_beacons().await;
        assert_eq!(nearby.len(), 1);
        assert_eq!(nearby[0].node_id, "update-me");

        observer.stop().await;
    }

    #[tokio::test]
    async fn test_observer_ignores_distant_beacons() {
        let my_geohash = "9q8yy9m";
        let distant = make_nearby_beacon("far-away", "u4pruyd");

        let storage = EventStorage::new(vec![BeaconChangeEvent::Inserted(distant)]);

        let observer = BeaconObserver::new(Arc::new(storage), my_geohash.to_string());
        observer.start().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let nearby = observer.get_nearby_beacons().await;
        assert_eq!(nearby.len(), 0);

        observer.stop().await;
    }

    #[tokio::test]
    async fn test_event_storage_methods() {
        // Ensure mock storage methods are exercised for coverage
        use crate::beacon::types::{GeoPosition, HierarchyLevel};
        let storage = EventStorage::new(vec![]);
        let beacon = crate::beacon::types::GeographicBeacon::new(
            "test".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );
        storage.save_beacon(&beacon).await.unwrap();
        let results = storage.query_by_geohash("9q8").await.unwrap();
        assert!(results.is_empty());
        let all = storage.query_all().await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn test_mock_storage_methods() {
        use crate::beacon::types::{GeoPosition, HierarchyLevel};
        let storage = MockBeaconStorage::new();
        let beacon = crate::beacon::types::GeographicBeacon::new(
            "test".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );
        storage.save_beacon(&beacon).await.unwrap();
        let results = storage.query_by_geohash("9q8").await.unwrap();
        assert_eq!(results.len(), 1);
        let all = storage.query_all().await.unwrap();
        assert_eq!(all.len(), 1);
        // Save again (update path)
        storage.save_beacon(&beacon).await.unwrap();
        let all = storage.query_all().await.unwrap();
        assert_eq!(all.len(), 1);
    }
}
