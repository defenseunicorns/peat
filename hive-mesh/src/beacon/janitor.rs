use super::types::GeographicBeacon;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Cleans up expired beacons based on TTL
///
/// BeaconJanitor runs periodically to remove beacons that have exceeded
/// their time-to-live, ensuring the beacon cache stays fresh.
pub struct BeaconJanitor {
    nearby_beacons: Arc<RwLock<HashMap<String, GeographicBeacon>>>,
    ttl: Duration,
    cleanup_interval: Duration,
    running: Arc<RwLock<bool>>,
}

impl BeaconJanitor {
    /// Create a new beacon janitor
    ///
    /// # Arguments
    /// * `nearby_beacons` - Shared beacon cache to clean
    /// * `ttl` - Time-to-live for beacons (e.g., 30 seconds)
    /// * `cleanup_interval` - How often to run cleanup (e.g., 5 seconds)
    pub fn new(
        nearby_beacons: Arc<RwLock<HashMap<String, GeographicBeacon>>>,
        ttl: Duration,
        cleanup_interval: Duration,
    ) -> Self {
        Self {
            nearby_beacons,
            ttl,
            cleanup_interval,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start the janitor cleanup loop
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            debug!("Beacon janitor already running");
            return;
        }
        *running = true;
        drop(running);

        info!(
            "Starting beacon janitor with TTL {:?} and cleanup interval {:?}",
            self.ttl, self.cleanup_interval
        );

        let nearby_beacons = self.nearby_beacons.clone();
        let ttl_secs = self.ttl.as_secs();
        let cleanup_interval = self.cleanup_interval;
        let running_clone = self.running.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);

            while *running_clone.read().await {
                interval.tick().await;

                // Cleanup expired beacons
                let mut beacons = nearby_beacons.write().await;
                let initial_count = beacons.len();

                beacons.retain(|_, beacon| !beacon.is_expired(ttl_secs));

                let removed_count = initial_count - beacons.len();
                if removed_count > 0 {
                    debug!("Removed {} expired beacons", removed_count);
                }
            }

            info!("Beacon janitor stopped");
        });
    }

    /// Stop the janitor
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Stopped beacon janitor");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::types::{GeoPosition, HierarchyLevel};

    #[tokio::test]
    async fn test_janitor_lifecycle() {
        let beacons = Arc::new(RwLock::new(HashMap::new()));
        let janitor =
            BeaconJanitor::new(beacons, Duration::from_secs(30), Duration::from_millis(100));

        janitor.start().await;
        assert!(*janitor.running.read().await);

        tokio::time::sleep(Duration::from_millis(50)).await;

        janitor.stop().await;
        assert!(!*janitor.running.read().await);
    }

    #[tokio::test]
    async fn test_janitor_cleanup() {
        let beacons = Arc::new(RwLock::new(HashMap::new()));

        // Add a fresh beacon
        let fresh_beacon = GeographicBeacon::new(
            "fresh".to_string(),
            GeoPosition::new(37.0, -122.0),
            HierarchyLevel::Platform,
        );
        beacons
            .write()
            .await
            .insert("fresh".to_string(), fresh_beacon);

        // Add an expired beacon (simulate by setting old timestamp)
        let mut expired_beacon = GeographicBeacon::new(
            "expired".to_string(),
            GeoPosition::new(37.0, -122.0),
            HierarchyLevel::Platform,
        );
        expired_beacon.timestamp = 0; // Very old timestamp
        beacons
            .write()
            .await
            .insert("expired".to_string(), expired_beacon);

        assert_eq!(beacons.read().await.len(), 2);

        // Start janitor with short TTL
        let janitor = BeaconJanitor::new(
            beacons.clone(),
            Duration::from_secs(5),
            Duration::from_millis(50),
        );

        janitor.start().await;

        // Wait for cleanup to run
        tokio::time::sleep(Duration::from_millis(200)).await;

        janitor.stop().await;

        // Expired beacon should be removed
        assert_eq!(beacons.read().await.len(), 1);
        assert!(beacons.read().await.contains_key("fresh"));
        assert!(!beacons.read().await.contains_key("expired"));
    }
}
