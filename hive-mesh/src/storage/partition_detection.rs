//! Network partition detection for Automerge+Iroh sync
//!
//! This module provides partition detection mechanisms to identify when peers
//! become unreachable and track partition lifecycle events.
//!
//! # Conceptual Model
//!
//! Automerge CRDTs already provide "recovery" via eventual consistency guarantees.
//! This module implements the **operational mechanisms** around partition handling:
//!
//! 1. **Detection**: Identify when peers are unreachable (heartbeat/timeout)
//! 2. **State Management**: Track partition state per peer
//! 3. **Observability**: Emit partition lifecycle events
//!
//! The CRDT (Automerge) handles the correctness guarantees (no data loss, automatic merge).
//! This module provides the operational layer (detection, metrics, efficiency).

use iroh::EndpointId;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use tracing::{debug, info, warn};

/// Partition lifecycle event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionEvent {
    /// Partition detected for a peer
    PartitionDetected {
        peer_id: EndpointId,
        consecutive_failures: u64,
    },
    /// Partition healed - peer started recovering
    PartitionHealed {
        peer_id: EndpointId,
        partition_duration: Duration,
    },
    /// Peer fully recovered - returned to Connected state
    PeerRecovered { peer_id: EndpointId },
    /// Heartbeat success recorded
    HeartbeatSuccess { peer_id: EndpointId },
    /// Heartbeat failure recorded (but partition not yet detected)
    HeartbeatFailure {
        peer_id: EndpointId,
        consecutive_failures: u64,
    },
}

/// Partition state for a peer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerPartitionState {
    /// Peer is connected and responsive
    Connected,
    /// Peer is partitioned (heartbeat timeout exceeded)
    Partitioned,
    /// Peer is recovering from partition (first heartbeat after partition)
    Recovering,
}

/// Heartbeat tracker for a single peer
#[derive(Debug, Clone)]
pub struct PeerHeartbeat {
    /// Current partition state
    pub state: PeerPartitionState,
    /// Last successful heartbeat timestamp
    pub last_heartbeat: SystemTime,
    /// When the partition was detected (None if not partitioned)
    pub partition_detected_at: Option<SystemTime>,
    /// Number of consecutive heartbeat failures
    pub consecutive_failures: u64,
}

impl PeerHeartbeat {
    /// Create a new heartbeat tracker for a peer
    pub fn new() -> Self {
        Self {
            state: PeerPartitionState::Connected,
            last_heartbeat: SystemTime::now(),
            partition_detected_at: None,
            consecutive_failures: 0,
        }
    }

    /// Record a successful heartbeat
    ///
    /// Returns an optional event if a state transition occurred
    pub fn record_success(&mut self, peer_id: EndpointId) -> Option<PartitionEvent> {
        let now = SystemTime::now();
        let was_partitioned = self.state == PeerPartitionState::Partitioned;

        self.last_heartbeat = now;
        self.consecutive_failures = 0;

        if was_partitioned {
            // Transition from Partitioned → Recovering
            let partition_duration = self
                .partition_detected_at
                .and_then(|detected_at| now.duration_since(detected_at).ok())
                .unwrap_or(Duration::from_secs(0));

            self.state = PeerPartitionState::Recovering;
            self.partition_detected_at = None;

            info!(
                peer_id = ?peer_id,
                partition_duration_secs = partition_duration.as_secs(),
                "Partition healed - peer recovering"
            );

            return Some(PartitionEvent::PartitionHealed {
                peer_id,
                partition_duration,
            });
        } else if self.state == PeerPartitionState::Recovering {
            // Transition from Recovering → Connected
            self.state = PeerPartitionState::Connected;

            info!(peer_id = ?peer_id, "Peer fully recovered");

            return Some(PartitionEvent::PeerRecovered { peer_id });
        }

        debug!(peer_id = ?peer_id, "Heartbeat success");
        Some(PartitionEvent::HeartbeatSuccess { peer_id })
    }

    /// Record a heartbeat failure
    ///
    /// Returns an optional event if a partition was detected
    pub fn record_failure(
        &mut self,
        peer_id: EndpointId,
        timeout_threshold: u64,
    ) -> Option<PartitionEvent> {
        self.consecutive_failures += 1;

        // Detect partition if we've exceeded the threshold
        if self.consecutive_failures >= timeout_threshold
            && self.state != PeerPartitionState::Partitioned
        {
            self.state = PeerPartitionState::Partitioned;
            self.partition_detected_at = Some(SystemTime::now());

            warn!(
                peer_id = ?peer_id,
                consecutive_failures = self.consecutive_failures,
                "Partition detected"
            );

            return Some(PartitionEvent::PartitionDetected {
                peer_id,
                consecutive_failures: self.consecutive_failures,
            });
        }

        // Still connected but experiencing failures
        if self.state == PeerPartitionState::Connected {
            debug!(
                peer_id = ?peer_id,
                consecutive_failures = self.consecutive_failures,
                threshold = timeout_threshold,
                "Heartbeat failure"
            );

            return Some(PartitionEvent::HeartbeatFailure {
                peer_id,
                consecutive_failures: self.consecutive_failures,
            });
        }

        None
    }

    /// Check if peer should be considered partitioned based on time elapsed
    pub fn is_timeout(&self, timeout: Duration) -> bool {
        SystemTime::now()
            .duration_since(self.last_heartbeat)
            .map(|elapsed| elapsed > timeout)
            .unwrap_or(false)
    }

    /// Get partition duration (if partitioned)
    pub fn partition_duration(&self) -> Option<Duration> {
        self.partition_detected_at
            .and_then(|detected_at| SystemTime::now().duration_since(detected_at).ok())
    }
}

impl Default for PeerHeartbeat {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for partition detection
#[derive(Debug, Clone)]
pub struct PartitionConfig {
    /// Heartbeat interval (default: 5 seconds)
    pub heartbeat_interval: Duration,
    /// Heartbeat timeout (default: 3x heartbeat interval = 15 seconds)
    pub heartbeat_timeout: Duration,
    /// Number of consecutive failures before partition detection (default: 3)
    pub failure_threshold: u64,
}

impl Default for PartitionConfig {
    fn default() -> Self {
        let heartbeat_interval = Duration::from_secs(5);
        Self {
            heartbeat_interval,
            heartbeat_timeout: heartbeat_interval * 3,
            failure_threshold: 3,
        }
    }
}

/// Partition detection coordinator
///
/// Tracks heartbeat state for all peers and detects network partitions.
pub struct PartitionDetector {
    /// Heartbeat state per peer
    heartbeats: Arc<RwLock<HashMap<EndpointId, PeerHeartbeat>>>,
    /// Configuration
    config: PartitionConfig,
}

impl PartitionDetector {
    /// Create a new partition detector with default config
    pub fn new() -> Self {
        Self::with_config(PartitionConfig::default())
    }

    /// Create a new partition detector with custom config
    pub fn with_config(config: PartitionConfig) -> Self {
        Self {
            heartbeats: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get partition config
    pub fn config(&self) -> &PartitionConfig {
        &self.config
    }

    /// Register a new peer for heartbeat tracking
    pub fn register_peer(&self, peer_id: EndpointId) {
        self.heartbeats.write().unwrap().entry(peer_id).or_default();
    }

    /// Remove a peer from heartbeat tracking
    pub fn unregister_peer(&self, peer_id: &EndpointId) {
        self.heartbeats.write().unwrap().remove(peer_id);
    }

    /// Record a successful heartbeat for a peer
    ///
    /// Returns an optional partition event if a state transition occurred
    pub fn record_heartbeat_success(&self, peer_id: &EndpointId) -> Option<PartitionEvent> {
        self.heartbeats
            .write()
            .unwrap()
            .get_mut(peer_id)
            .and_then(|hb| hb.record_success(*peer_id))
    }

    /// Record a heartbeat failure for a peer
    ///
    /// Returns an optional partition event if partition was detected or failure occurred
    pub fn record_heartbeat_failure(&self, peer_id: &EndpointId) -> Option<PartitionEvent> {
        self.heartbeats
            .write()
            .unwrap()
            .get_mut(peer_id)
            .and_then(|hb| hb.record_failure(*peer_id, self.config.failure_threshold))
    }

    /// Get the partition state for a peer
    pub fn get_peer_state(&self, peer_id: &EndpointId) -> Option<PeerPartitionState> {
        self.heartbeats
            .read()
            .unwrap()
            .get(peer_id)
            .map(|hb| hb.state)
    }

    /// Get heartbeat info for a peer
    pub fn get_peer_heartbeat(&self, peer_id: &EndpointId) -> Option<PeerHeartbeat> {
        self.heartbeats.read().unwrap().get(peer_id).cloned()
    }

    /// Get all partitioned peers
    pub fn get_partitioned_peers(&self) -> Vec<EndpointId> {
        self.heartbeats
            .read()
            .unwrap()
            .iter()
            .filter(|(_, hb)| hb.state == PeerPartitionState::Partitioned)
            .map(|(peer_id, _)| *peer_id)
            .collect()
    }

    /// Check all peers for timeout-based partition detection
    ///
    /// Returns a list of partition events for newly partitioned peers
    pub fn check_timeouts(&self) -> Vec<PartitionEvent> {
        let mut events = Vec::new();

        let mut heartbeats = self.heartbeats.write().unwrap();
        for (peer_id, hb) in heartbeats.iter_mut() {
            if hb.state != PeerPartitionState::Partitioned
                && hb.is_timeout(self.config.heartbeat_timeout)
            {
                hb.state = PeerPartitionState::Partitioned;
                hb.partition_detected_at = Some(SystemTime::now());

                warn!(
                    peer_id = ?peer_id,
                    timeout_secs = self.config.heartbeat_timeout.as_secs(),
                    "Partition detected via timeout"
                );

                events.push(PartitionEvent::PartitionDetected {
                    peer_id: *peer_id,
                    consecutive_failures: hb.consecutive_failures,
                });
            }
        }

        events
    }

    /// Get number of tracked peers
    pub fn peer_count(&self) -> usize {
        self.heartbeats.read().unwrap().len()
    }
}

impl Default for PartitionDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_heartbeat_success_resets_failures() {
        let mut hb = PeerHeartbeat::new();
        hb.consecutive_failures = 2;

        // Create a dummy endpoint ID for testing
        let mut rng = rand::rng();
        let secret_key = iroh::SecretKey::generate(&mut rng);
        let peer_id = secret_key.public();

        let event = hb.record_success(peer_id);

        assert_eq!(hb.consecutive_failures, 0);
        assert_eq!(hb.state, PeerPartitionState::Connected);
        assert_eq!(event, Some(PartitionEvent::HeartbeatSuccess { peer_id }));
    }

    #[test]
    fn test_peer_heartbeat_partition_detection() {
        let mut hb = PeerHeartbeat::new();
        let mut rng = rand::rng();
        let secret_key = iroh::SecretKey::generate(&mut rng);
        let peer_id = secret_key.public();

        // First 2 failures don't trigger partition
        let event1 = hb.record_failure(peer_id, 3);
        assert_eq!(hb.state, PeerPartitionState::Connected);
        assert!(matches!(
            event1,
            Some(PartitionEvent::HeartbeatFailure { .. })
        ));

        let event2 = hb.record_failure(peer_id, 3);
        assert_eq!(hb.state, PeerPartitionState::Connected);
        assert!(matches!(
            event2,
            Some(PartitionEvent::HeartbeatFailure { .. })
        ));

        // 3rd failure triggers partition
        let event3 = hb.record_failure(peer_id, 3);
        assert_eq!(hb.state, PeerPartitionState::Partitioned);
        assert!(hb.partition_detected_at.is_some());
        assert!(matches!(
            event3,
            Some(PartitionEvent::PartitionDetected { .. })
        ));
    }

    #[test]
    fn test_peer_heartbeat_recovery() {
        let mut hb = PeerHeartbeat::new();
        let mut rng = rand::rng();
        let secret_key = iroh::SecretKey::generate(&mut rng);
        let peer_id = secret_key.public();

        // Trigger partition
        hb.record_failure(peer_id, 3);
        hb.record_failure(peer_id, 3);
        hb.record_failure(peer_id, 3);
        assert_eq!(hb.state, PeerPartitionState::Partitioned);

        // First success after partition → Recovering
        let event1 = hb.record_success(peer_id);
        assert_eq!(hb.state, PeerPartitionState::Recovering);
        assert!(hb.partition_detected_at.is_none());
        assert!(matches!(
            event1,
            Some(PartitionEvent::PartitionHealed { .. })
        ));

        // Second success → Connected
        let event2 = hb.record_success(peer_id);
        assert_eq!(hb.state, PeerPartitionState::Connected);
        assert!(matches!(event2, Some(PartitionEvent::PeerRecovered { .. })));
    }

    #[test]
    fn test_partition_config_defaults() {
        let config = PartitionConfig::default();
        assert_eq!(config.heartbeat_interval, Duration::from_secs(5));
        assert_eq!(config.heartbeat_timeout, Duration::from_secs(15));
        assert_eq!(config.failure_threshold, 3);
    }

    #[test]
    fn test_partition_detector_creation() {
        let detector = PartitionDetector::new();
        assert_eq!(detector.peer_count(), 0);
        assert_eq!(detector.config().heartbeat_interval, Duration::from_secs(5));
    }

    // NOTE: Tests involving EndpointId require IrohTransport which is async.
    // These tests are covered in integration tests (automerge_iroh_partition_e2e.rs)
}
