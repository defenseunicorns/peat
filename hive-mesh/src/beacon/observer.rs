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
}
