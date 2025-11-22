use super::config::NodeProfile;
use super::storage::BeaconStorage;
use super::types::{GeoPosition, GeographicBeacon, HierarchyLevel};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Broadcasts geographic beacons periodically
///
/// BeaconBroadcaster is responsible for periodically creating and broadcasting
/// this node's presence to the mesh network via the storage backend.
pub struct BeaconBroadcaster {
    storage: Arc<dyn BeaconStorage>,
    beacon: Arc<RwLock<GeographicBeacon>>,
    broadcast_interval: Duration,
    running: Arc<RwLock<bool>>,
}

impl BeaconBroadcaster {
    /// Create a new beacon broadcaster
    ///
    /// # Arguments
    ///
    /// * `storage` - Storage backend for beacon persistence
    /// * `node_id` - Unique identifier for this node
    /// * `position` - Geographic position of this node
    /// * `hierarchy_level` - Position in military hierarchy
    /// * `profile` - Optional node profile (mobility, resources, parenting capability)
    /// * `broadcast_interval` - How often to broadcast beacons
    pub fn new(
        storage: Arc<dyn BeaconStorage>,
        node_id: String,
        position: GeoPosition,
        hierarchy_level: HierarchyLevel,
        profile: Option<NodeProfile>,
        broadcast_interval: Duration,
    ) -> Self {
        // Create initial beacon
        let mut beacon = GeographicBeacon::new(node_id, position, hierarchy_level);

        // Apply profile if provided
        if let Some(profile) = profile {
            beacon.mobility = Some(profile.mobility);
            beacon.can_parent = profile.can_parent;
            beacon.parent_priority = profile.parent_priority;
            beacon.resources = Some(profile.resources);
        }

        Self {
            storage,
            beacon: Arc::new(RwLock::new(beacon)),
            broadcast_interval,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start broadcasting beacons
    ///
    /// This will run indefinitely until stop() is called, broadcasting
    /// the node's beacon at regular intervals via the storage backend.
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            debug!("Beacon broadcaster already running");
            return;
        }
        *running = true;
        drop(running);

        let node_id = self.beacon.read().await.node_id.clone();
        info!(
            "Starting beacon broadcaster for node {} with interval {:?}",
            node_id, self.broadcast_interval
        );

        let mut interval = tokio::time::interval(self.broadcast_interval);
        let running_clone = self.running.clone();
        let beacon_clone = self.beacon.clone();
        let storage_clone = self.storage.clone();

        tokio::spawn(async move {
            while *running_clone.read().await {
                interval.tick().await;

                // Update beacon timestamp
                let beacon = {
                    let mut beacon = beacon_clone.write().await;
                    beacon.update_timestamp();
                    beacon.clone()
                };

                // Broadcast via storage backend
                if let Err(e) = storage_clone.save_beacon(&beacon).await {
                    error!("Failed to broadcast beacon: {}", e);
                } else {
                    debug!("Beacon broadcast for node {}", beacon.node_id);
                }
            }
        });
    }

    /// Stop broadcasting beacons
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        let node_id = self.beacon.read().await.node_id.clone();
        info!("Stopped beacon broadcaster for node {}", node_id);
    }

    /// Update the beacon's position
    ///
    /// This is useful for mobile nodes that change location over time.
    pub async fn update_position(&self, new_position: GeoPosition) {
        use geohash::{encode, Coord};

        let mut beacon = self.beacon.write().await;
        beacon.position = new_position;

        // Recalculate geohash
        beacon.geohash = encode(
            Coord {
                x: new_position.lon,
                y: new_position.lat,
            },
            7,
        )
        .unwrap_or_else(|_| String::from("invalid"));
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

    /// Mock in-memory beacon storage for testing
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
            Ok(Box::new(stream::empty()))
        }
    }

    #[tokio::test]
    async fn test_broadcaster_lifecycle() {
        let storage = Arc::new(MockBeaconStorage::new());
        let position = GeoPosition::new(37.7749, -122.4194);

        let broadcaster = BeaconBroadcaster::new(
            storage,
            "test-node".to_string(),
            position,
            HierarchyLevel::Platform,
            None,
            Duration::from_millis(100),
        );

        broadcaster.start().await;
        assert!(*broadcaster.running.read().await);

        tokio::time::sleep(Duration::from_millis(50)).await;

        broadcaster.stop().await;
        assert!(!*broadcaster.running.read().await);
    }

    #[tokio::test]
    async fn test_broadcaster_saves_beacons() {
        let storage = Arc::new(MockBeaconStorage::new());
        let position = GeoPosition::new(37.7749, -122.4194);

        let broadcaster = BeaconBroadcaster::new(
            storage.clone(),
            "test-node".to_string(),
            position,
            HierarchyLevel::Platform,
            None,
            Duration::from_millis(50),
        );

        broadcaster.start().await;

        // Wait for a few broadcasts
        tokio::time::sleep(Duration::from_millis(200)).await;

        broadcaster.stop().await;

        // Verify beacon was saved
        let beacons = storage.query_all().await.unwrap();
        assert_eq!(beacons.len(), 1);
        assert_eq!(beacons[0].node_id, "test-node");
        assert_eq!(beacons[0].hierarchy_level, HierarchyLevel::Platform);
    }

    #[tokio::test]
    async fn test_broadcaster_with_profile() {
        let storage = Arc::new(MockBeaconStorage::new());
        let position = GeoPosition::new(37.7749, -122.4194);

        let resources = crate::beacon::config::NodeResources {
            cpu_cores: 4,
            memory_mb: 2048,
            bandwidth_mbps: 100,
            cpu_usage_percent: 30,
            memory_usage_percent: 40,
            battery_percent: None,
        };

        let profile = crate::beacon::config::NodeProfile::static_node(resources);

        let broadcaster = BeaconBroadcaster::new(
            storage.clone(),
            "static-node".to_string(),
            position,
            HierarchyLevel::Squad,
            Some(profile),
            Duration::from_millis(50),
        );

        broadcaster.start().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        broadcaster.stop().await;

        // Verify beacon has profile attributes
        let beacons = storage.query_all().await.unwrap();
        assert_eq!(beacons.len(), 1);
        assert!(beacons[0].can_parent);
        assert_eq!(beacons[0].parent_priority, 255);
        assert!(beacons[0].mobility.is_some());
    }

    #[tokio::test]
    async fn test_update_position() {
        let storage = Arc::new(MockBeaconStorage::new());
        let initial_position = GeoPosition::new(37.7749, -122.4194);

        let broadcaster = BeaconBroadcaster::new(
            storage.clone(),
            "mobile-node".to_string(),
            initial_position,
            HierarchyLevel::Platform,
            None,
            Duration::from_millis(50),
        );

        // Start broadcasting
        broadcaster.start().await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Update position
        let new_position = GeoPosition::new(37.7750, -122.4195);
        broadcaster.update_position(new_position).await;

        // Wait for next broadcast
        tokio::time::sleep(Duration::from_millis(100)).await;
        broadcaster.stop().await;

        // Verify new position was broadcast
        let beacons = storage.query_all().await.unwrap();
        assert_eq!(beacons.len(), 1);
        assert_eq!(beacons[0].position.lat, 37.7750);
        assert_eq!(beacons[0].position.lon, -122.4195);
    }
}
