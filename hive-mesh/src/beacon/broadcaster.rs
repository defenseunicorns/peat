use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Broadcasts geographic beacons periodically
///
/// BeaconBroadcaster is responsible for periodically creating and broadcasting
/// this node's presence to the mesh network via the storage backend.
pub struct BeaconBroadcaster {
    node_id: String,
    broadcast_interval: Duration,
    running: Arc<RwLock<bool>>,
}

impl BeaconBroadcaster {
    /// Create a new beacon broadcaster
    pub fn new(node_id: String, broadcast_interval: Duration) -> Self {
        Self {
            node_id,
            broadcast_interval,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start broadcasting beacons
    ///
    /// This will run indefinitely until stop() is called
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            debug!("Beacon broadcaster already running");
            return;
        }
        *running = true;
        drop(running);

        info!(
            "Starting beacon broadcaster for node {} with interval {:?}",
            self.node_id, self.broadcast_interval
        );

        let mut interval = tokio::time::interval(self.broadcast_interval);
        let running_clone = self.running.clone();

        tokio::spawn(async move {
            while *running_clone.read().await {
                interval.tick().await;
                // TODO: Broadcast beacon via storage backend
                debug!("Beacon broadcast tick");
            }
        });
    }

    /// Stop broadcasting beacons
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Stopped beacon broadcaster for node {}", self.node_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broadcaster_lifecycle() {
        let broadcaster =
            BeaconBroadcaster::new("test-node".to_string(), Duration::from_millis(100));

        broadcaster.start().await;
        assert!(*broadcaster.running.read().await);

        tokio::time::sleep(Duration::from_millis(50)).await;

        broadcaster.stop().await;
        assert!(!*broadcaster.running.read().await);
    }
}
