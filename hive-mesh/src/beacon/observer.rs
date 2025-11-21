use super::types::GeographicBeacon;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Observes and tracks nearby geographic beacons
///
/// BeaconObserver subscribes to beacon updates from the storage backend
/// and maintains a cache of nearby beacons based on geohash proximity.
pub struct BeaconObserver {
    my_geohash: String,
    nearby_beacons: Arc<RwLock<HashMap<String, GeographicBeacon>>>,
    running: Arc<RwLock<bool>>,
}

impl BeaconObserver {
    /// Create a new beacon observer
    pub fn new(my_geohash: String) -> Self {
        Self {
            my_geohash,
            nearby_beacons: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start observing beacons
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            debug!("Beacon observer already running");
            return;
        }
        *running = true;
        drop(running);

        info!("Starting beacon observer for geohash {}", self.my_geohash);

        // TODO: Subscribe to beacon collection changes from storage backend
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
    #[allow(dead_code)]
    fn is_nearby(&self, other_geohash: &str) -> bool {
        use geohash::Direction;

        if self.my_geohash == other_geohash {
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
            if let Ok(neighbor) = geohash::neighbor(&self.my_geohash, *dir) {
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

    #[tokio::test]
    async fn test_observer_lifecycle() {
        let observer = BeaconObserver::new("9q8yy9m".to_string());

        observer.start().await;
        assert!(*observer.running.read().await);

        observer.stop().await;
        assert!(!*observer.running.read().await);
    }

    #[test]
    fn test_is_nearby() {
        let observer = BeaconObserver::new("9q8yy9m".to_string());

        // Same geohash should be nearby
        assert!(observer.is_nearby("9q8yy9m"));

        // Adjacent geohashes should be nearby
        assert!(observer.is_nearby(&geohash::neighbor("9q8yy9m", geohash::Direction::N).unwrap()));

        // Distant geohash should not be nearby
        assert!(!observer.is_nearby("u4pruyd")); // Sydney, Australia
    }
}
