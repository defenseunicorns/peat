//! Topology builder for beacon-driven mesh formation
//!
//! This module implements the TopologyBuilder which coordinates topology formation
//! by observing nearby beacons, selecting peers, and maintaining topology state.

use crate::beacon::{BeaconObserver, GeoPosition, GeographicBeacon, HierarchyLevel, NodeProfile};
use crate::hierarchy::{HierarchyStrategy, NodeRole};
use crate::topology::partition::PartitionDetector;
use crate::topology::selection::{PeerSelector, SelectionConfig};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Topology change events
#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)] // "Peer" prefix adds clarity to event names
pub enum TopologyEvent {
    /// Peer selected for the first time
    PeerSelected {
        selected_peer_id: String,
        peer_beacon: GeographicBeacon,
    },
    /// Selected peer changed (peer change occurred)
    PeerChanged {
        old_peer_id: String,
        new_peer_id: String,
        new_peer_beacon: GeographicBeacon,
    },
    /// Selected peer lost (became unavailable)
    PeerLost { lost_peer_id: String },
    /// Linked peer joined under this node (lower hierarchy level)
    PeerAdded { linked_peer_id: String },
    /// Linked peer left (lower hierarchy level)
    PeerRemoved { linked_peer_id: String },
    /// Lateral peer discovered (same hierarchy level)
    LateralPeerDiscovered {
        peer_id: String,
        peer_beacon: GeographicBeacon,
    },
    /// Lateral peer lost (same hierarchy level)
    LateralPeerLost { peer_id: String },
    /// Node role changed within hierarchy level
    RoleChanged {
        old_role: NodeRole,
        new_role: NodeRole,
    },
    /// Node hierarchy level changed
    LevelChanged {
        old_level: HierarchyLevel,
        new_level: HierarchyLevel,
    },
}

/// Current topology state
#[derive(Debug, Clone)]
pub struct TopologyState {
    /// Current selected peer (if any)
    pub selected_peer: Option<SelectedPeer>,
    /// Current linked peers - lower hierarchy level (node_id -> last_seen)
    pub linked_peers: HashMap<String, Instant>,
    /// Current lateral peers - same hierarchy level (node_id -> last_seen)
    pub lateral_peers: HashMap<String, Instant>,
    /// Current node role within hierarchy level
    pub role: NodeRole,
    /// Current hierarchy level
    pub hierarchy_level: HierarchyLevel,
}

impl Default for TopologyState {
    fn default() -> Self {
        Self {
            selected_peer: None,
            linked_peers: HashMap::new(),
            lateral_peers: HashMap::new(),
            role: NodeRole::default(),
            hierarchy_level: HierarchyLevel::Squad, // Default level
        }
    }
}

#[derive(Debug, Clone)]
pub struct SelectedPeer {
    pub node_id: String,
    pub beacon: GeographicBeacon,
    pub selected_at: Instant,
}

/// Configuration for topology builder
#[derive(Debug, Clone)]
pub struct TopologyConfig {
    /// Peer selection configuration
    pub selection: SelectionConfig,
    /// How often to re-evaluate peer selection (None = only on beacon changes)
    pub reevaluation_interval: Option<Duration>,
    /// Minimum time before peer change (prevents thrashing)
    pub peer_change_cooldown: Duration,
    /// Time before considering peer lost if no beacon received
    pub peer_timeout: Duration,
    /// Hierarchy strategy for role and level determination
    pub hierarchy_strategy: Option<Arc<dyn HierarchyStrategy>>,
    /// Maximum lateral peer connections (None = unlimited)
    ///
    /// For backends like Ditto that optimize mesh internally, set to None.
    /// For explicit transports like Iroh, use a reasonable limit (e.g., 10-20)
    /// to avoid O(n²) connection overhead with large groups.
    pub max_lateral_connections: Option<usize>,
    /// Maximum number of connection retry attempts (0 = no retries)
    pub max_retries: u32,
    /// Initial delay before first retry attempt
    pub initial_backoff: Duration,
    /// Maximum backoff delay (caps exponential growth)
    pub max_backoff: Duration,
    /// Exponential backoff multiplier (typically 2.0)
    pub backoff_multiplier: f64,
    /// Maximum number of telemetry packets to buffer during parent transitions (0 = no buffering)
    pub max_telemetry_buffer_size: usize,
    /// Optional metrics collector for observability (None = no metrics collected)
    pub metrics_collector: Option<Arc<dyn crate::topology::metrics::MetricsCollector>>,
    /// Optional partition detector for autonomous operation (None = no partition detection)
    pub partition_detector: Option<Arc<Mutex<PartitionDetector>>>,
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            selection: SelectionConfig::default(),
            reevaluation_interval: Some(Duration::from_secs(30)),
            peer_change_cooldown: Duration::from_secs(60),
            peer_timeout: Duration::from_secs(180), // 3 minutes
            hierarchy_strategy: None, // No strategy by default (uses fixed hierarchy_level)
            max_lateral_connections: Some(10), // Conservative default for explicit transports like Iroh
            max_retries: 3,                    // Retry up to 3 times before giving up
            initial_backoff: Duration::from_secs(1), // Start with 1 second
            max_backoff: Duration::from_secs(60), // Cap at 1 minute
            backoff_multiplier: 2.0,           // Standard exponential backoff (2^n)
            max_telemetry_buffer_size: 100,    // Buffer up to 100 telemetry packets during failover
            metrics_collector: None,           // No metrics collection by default
            partition_detector: None,          // No partition detection by default
        }
    }
}

/// Topology builder
///
/// Coordinates topology formation by:
/// - Observing nearby beacons
/// - Selecting optimal peers
/// - Managing peer relationships
/// - Handling dynamic peer changes
pub struct TopologyBuilder {
    config: TopologyConfig,
    #[allow(dead_code)]
    node_id: String,
    position: Arc<Mutex<GeoPosition>>,
    hierarchy_level: HierarchyLevel,
    #[allow(dead_code)]
    profile: Option<NodeProfile>,
    observer: Arc<BeaconObserver>,
    state: Arc<Mutex<TopologyState>>,
    event_tx: mpsc::UnboundedSender<TopologyEvent>,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<TopologyEvent>>>,
    task_handle: Mutex<Option<JoinHandle<()>>>,
}

impl Clone for TopologyBuilder {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            node_id: self.node_id.clone(),
            position: self.position.clone(),
            hierarchy_level: self.hierarchy_level,
            profile: self.profile.clone(),
            observer: self.observer.clone(),
            state: self.state.clone(),
            event_tx: self.event_tx.clone(),
            event_rx: Mutex::new(None), // Don't clone receiver, only sender clones
            task_handle: Mutex::new(None), // Don't clone task handle
        }
    }
}

impl TopologyBuilder {
    /// Create a new topology builder
    pub fn new(
        config: TopologyConfig,
        node_id: String,
        position: GeoPosition,
        hierarchy_level: HierarchyLevel,
        profile: Option<NodeProfile>,
        observer: Arc<BeaconObserver>,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            config,
            node_id,
            position: Arc::new(Mutex::new(position)),
            hierarchy_level,
            profile,
            observer,
            state: Arc::new(Mutex::new(TopologyState::default())),
            event_tx,
            event_rx: Mutex::new(Some(event_rx)),
            task_handle: Mutex::new(None),
        }
    }

    /// Start topology formation
    pub async fn start(&self) {
        let mut handle_guard = self.task_handle.lock().unwrap();
        if handle_guard.is_some() {
            return; // Already running
        }

        let config = self.config.clone();
        let position = self.position.clone();
        let hierarchy_level = self.hierarchy_level;
        let profile = self.profile.clone();
        let observer = self.observer.clone();
        let state = self.state.clone();
        let event_tx = self.event_tx.clone();

        let handle = tokio::spawn(async move {
            let mut interval = config.reevaluation_interval.map(tokio::time::interval);

            loop {
                // Wait for either interval or shutdown signal
                if let Some(ref mut int) = interval {
                    int.tick().await;
                } else {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }

                // Evaluate topology
                let current_pos = *position.lock().unwrap();
                let selector =
                    PeerSelector::new(config.selection.clone(), current_pos, hierarchy_level);

                // Get nearby beacons
                let nearby = observer.get_nearby_beacons().await;

                // Evaluate hierarchy strategy if provided
                let mut state_lock = state.lock().unwrap();
                let current_hierarchy_level = if let (Some(strategy), Some(prof)) =
                    (config.hierarchy_strategy.as_ref(), profile.as_ref())
                {
                    // Determine level and role using strategy
                    let new_level = strategy.determine_level(prof);
                    let new_role = strategy.determine_role(prof, &nearby);

                    // Check for level change
                    if new_level != state_lock.hierarchy_level {
                        let old_level = state_lock.hierarchy_level;
                        state_lock.hierarchy_level = new_level;
                        let _ = event_tx.send(TopologyEvent::LevelChanged {
                            old_level,
                            new_level,
                        });
                    }

                    // Check for role change
                    if new_role != state_lock.role {
                        let old_role = state_lock.role;
                        state_lock.role = new_role;
                        let _ = event_tx.send(TopologyEvent::RoleChanged { old_role, new_role });
                    }

                    new_level
                } else {
                    // No strategy provided, use fixed hierarchy_level
                    hierarchy_level
                };

                // Check current peer status
                let needs_peer =
                    Self::check_peer_status(&mut state_lock, &config, &nearby, &event_tx);

                if needs_peer {
                    // Select new peer
                    if let Some(candidate) = selector.select_peer(&nearby) {
                        Self::update_selected_peer(&mut state_lock, &event_tx, candidate.beacon);
                    }
                }

                // Track linked peers (peers that could select us)
                Self::update_linked_peers(
                    &mut state_lock,
                    &config,
                    &nearby,
                    current_hierarchy_level,
                    &event_tx,
                );

                // Track lateral peers (peers at same hierarchy level)
                Self::update_lateral_peers(
                    &mut state_lock,
                    &config,
                    &nearby,
                    current_hierarchy_level,
                    &event_tx,
                );

                drop(state_lock);

                // Check for network partition if detector is configured
                if let Some(ref partition_detector) = config.partition_detector {
                    // Convert Vec to HashMap for PartitionDetector
                    let beacons_map: HashMap<String, GeographicBeacon> = nearby
                        .iter()
                        .map(|beacon| (beacon.node_id.clone(), beacon.clone()))
                        .collect();

                    // Check partition and emit events if state changed
                    if let Some(_event) = partition_detector
                        .lock()
                        .unwrap()
                        .check_partition(&beacons_map)
                    {
                        // PartitionDetector already notifies the PartitionHandler
                        // No additional action needed here
                    }
                }
            }
        });

        *handle_guard = Some(handle);
    }

    /// Stop topology formation
    pub async fn stop(&self) {
        if let Some(handle) = self.task_handle.lock().unwrap().take() {
            handle.abort();
        }
    }

    /// Get current topology state
    pub fn get_state(&self) -> TopologyState {
        self.state.lock().unwrap().clone()
    }

    /// Get current selected peer
    pub fn get_selected_peer(&self) -> Option<SelectedPeer> {
        self.state.lock().unwrap().selected_peer.clone()
    }

    /// Get topology configuration
    pub fn config(&self) -> &TopologyConfig {
        &self.config
    }

    /// Get event receiver for topology changes
    ///
    /// Can only be called once. Returns None if already taken.
    pub fn subscribe(&self) -> Option<mpsc::UnboundedReceiver<TopologyEvent>> {
        self.event_rx.lock().unwrap().take()
    }

    /// Update node position (for mobile nodes)
    pub fn update_position(&self, position: GeoPosition) {
        *self.position.lock().unwrap() = position;
    }

    /// Force immediate re-evaluation of peer selection
    pub async fn reevaluate_peer(&self) {
        let current_pos = *self.position.lock().unwrap();
        let selector = PeerSelector::new(
            self.config.selection.clone(),
            current_pos,
            self.hierarchy_level,
        );

        let nearby = self.observer.get_nearby_beacons().await;
        let mut state_lock = self.state.lock().unwrap();

        if let Some(candidate) = selector.select_peer(&nearby) {
            // Check if this is better than current selected peer
            let should_switch = if let Some(ref current) = state_lock.selected_peer {
                // Only switch if cooldown period has passed
                let elapsed = current.selected_at.elapsed();
                if elapsed < self.config.peer_change_cooldown {
                    false
                } else {
                    // Re-score current selected peer and compare
                    let current_score = if let Some(current_beacon) =
                        nearby.iter().find(|b| b.node_id == current.node_id)
                    {
                        selector
                            .select_peer(std::slice::from_ref(current_beacon))
                            .map(|c| c.score)
                            .unwrap_or(0.0)
                    } else {
                        0.0 // Current selected peer not visible anymore
                    };

                    candidate.score > current_score * 1.1 // 10% hysteresis
                }
            } else {
                true // No current selected peer, definitely select
            };

            if should_switch {
                Self::update_selected_peer(&mut state_lock, &self.event_tx, candidate.beacon);
            }
        }
    }

    /// Check peer status and determine if new peer needed
    fn check_peer_status(
        state: &mut TopologyState,
        config: &TopologyConfig,
        nearby: &[GeographicBeacon],
        event_tx: &mpsc::UnboundedSender<TopologyEvent>,
    ) -> bool {
        if let Some(ref selected_peer) = state.selected_peer {
            // Check if selected peer is still visible
            if nearby.iter().any(|b| b.node_id == selected_peer.node_id) {
                // Selected peer still visible
                false
            } else {
                // Check timeout
                if selected_peer.selected_at.elapsed() > config.peer_timeout {
                    // Selected peer lost - emit event before clearing state
                    let lost_peer_id = selected_peer.node_id.clone();
                    state.selected_peer = None;
                    let _ = event_tx.send(TopologyEvent::PeerLost { lost_peer_id });
                    true
                } else {
                    false
                }
            }
        } else {
            // No selected peer, need one
            true
        }
    }

    /// Update current selected peer
    fn update_selected_peer(
        state: &mut TopologyState,
        event_tx: &mpsc::UnboundedSender<TopologyEvent>,
        new_peer_beacon: GeographicBeacon,
    ) {
        let new_peer_id = new_peer_beacon.node_id.clone();

        let event = if let Some(ref current) = state.selected_peer {
            TopologyEvent::PeerChanged {
                old_peer_id: current.node_id.clone(),
                new_peer_id: new_peer_id.clone(),
                new_peer_beacon: new_peer_beacon.clone(),
            }
        } else {
            TopologyEvent::PeerSelected {
                selected_peer_id: new_peer_id.clone(),
                peer_beacon: new_peer_beacon.clone(),
            }
        };

        state.selected_peer = Some(SelectedPeer {
            node_id: new_peer_id,
            beacon: new_peer_beacon,
            selected_at: Instant::now(),
        });

        let _ = event_tx.send(event);
    }

    /// Update linked peers (peers that could select us as their peer)
    fn update_linked_peers(
        state: &mut TopologyState,
        config: &TopologyConfig,
        nearby: &[GeographicBeacon],
        own_level: HierarchyLevel,
        event_tx: &mpsc::UnboundedSender<TopologyEvent>,
    ) {
        let now = Instant::now();

        // Identify potential linked peers (peers at lower hierarchy level that could select us)
        let potential_linked: Vec<&GeographicBeacon> = nearby
            .iter()
            .filter(|beacon| {
                // Peer must be at lower hierarchy level (could select us)
                own_level.can_be_parent_of(&beacon.hierarchy_level)
            })
            .collect();

        // Update last_seen for existing linked peers that are still visible
        for beacon in &potential_linked {
            if let Some(last_seen) = state.linked_peers.get_mut(&beacon.node_id) {
                *last_seen = now;
            } else {
                // New linked peer discovered
                state.linked_peers.insert(beacon.node_id.clone(), now);
                let _ = event_tx.send(TopologyEvent::PeerAdded {
                    linked_peer_id: beacon.node_id.clone(),
                });
            }
        }

        // Check for expired linked peers (not seen recently)
        let potential_linked_ids: HashSet<_> =
            potential_linked.iter().map(|b| &b.node_id).collect();

        let mut expired_peers = Vec::new();
        for (peer_id, last_seen) in &state.linked_peers {
            // Peer is expired if:
            // 1. Not in current nearby beacons
            // 2. Last seen longer than peer_timeout ago
            if !potential_linked_ids.contains(peer_id) && last_seen.elapsed() > config.peer_timeout
            {
                expired_peers.push(peer_id.clone());
            }
        }

        // Remove expired linked peers
        for peer_id in expired_peers {
            state.linked_peers.remove(&peer_id);
            let _ = event_tx.send(TopologyEvent::PeerRemoved {
                linked_peer_id: peer_id,
            });
        }
    }

    /// Update lateral peers (same hierarchy level)
    ///
    /// Tracks peers at the same hierarchy level for potential coordination.
    /// Emits LateralPeerDiscovered/Lost events as peers appear and disappear.
    fn update_lateral_peers(
        state: &mut TopologyState,
        config: &TopologyConfig,
        nearby: &[GeographicBeacon],
        own_level: HierarchyLevel,
        event_tx: &mpsc::UnboundedSender<TopologyEvent>,
    ) {
        let now = Instant::now();

        // Identify potential lateral peers (same hierarchy level)
        let potential_lateral: Vec<&GeographicBeacon> = nearby
            .iter()
            .filter(|beacon| beacon.hierarchy_level == own_level)
            .collect();

        // Update last_seen for existing lateral peers that are still visible
        for beacon in &potential_lateral {
            if let Some(last_seen) = state.lateral_peers.get_mut(&beacon.node_id) {
                *last_seen = now;
            } else {
                // New lateral peer discovered
                state.lateral_peers.insert(beacon.node_id.clone(), now);
                let _ = event_tx.send(TopologyEvent::LateralPeerDiscovered {
                    peer_id: beacon.node_id.clone(),
                    peer_beacon: (*beacon).clone(),
                });
            }
        }

        // Check for expired lateral peers (not seen recently)
        let potential_lateral_ids: HashSet<_> =
            potential_lateral.iter().map(|b| &b.node_id).collect();

        let mut expired_peers = Vec::new();
        for (peer_id, last_seen) in &state.lateral_peers {
            // Peer is expired if:
            // 1. Not in current nearby beacons
            // 2. Last seen longer than peer_timeout ago
            if !potential_lateral_ids.contains(peer_id) && last_seen.elapsed() > config.peer_timeout
            {
                expired_peers.push(peer_id.clone());
            }
        }

        // Remove expired lateral peers
        for peer_id in expired_peers {
            state.lateral_peers.remove(&peer_id);
            let _ = event_tx.send(TopologyEvent::LateralPeerLost { peer_id });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::MockBeaconStorage;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_topology_builder_creation() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer_geohash = "9q8yy".to_string();
        let observer = Arc::new(BeaconObserver::new(storage, observer_geohash));

        let builder = TopologyBuilder::new(
            TopologyConfig::default(),
            "test-node".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        let state = builder.get_state();
        assert!(state.selected_peer.is_none());
        assert!(state.linked_peers.is_empty());
    }

    #[tokio::test]
    async fn test_subscribe_returns_receiver() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer_geohash = "9q8yy".to_string();
        let observer = Arc::new(BeaconObserver::new(storage, observer_geohash));

        let builder = TopologyBuilder::new(
            TopologyConfig::default(),
            "test-node".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        let rx = builder.subscribe();
        assert!(rx.is_some());

        // Second call should return None
        let rx2 = builder.subscribe();
        assert!(rx2.is_none());
    }

    #[tokio::test]
    async fn test_update_position() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer_geohash = "9q8yy".to_string();
        let observer = Arc::new(BeaconObserver::new(storage, observer_geohash));

        let builder = TopologyBuilder::new(
            TopologyConfig::default(),
            "test-node".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        let new_pos = GeoPosition::new(37.8000, -122.4000);
        builder.update_position(new_pos);

        let updated_pos = *builder.position.lock().unwrap();
        assert_eq!(updated_pos.lat, 37.8000);
        assert_eq!(updated_pos.lon, -122.4000);
    }

    #[test]
    fn test_linked_peer_tracking() {
        use crate::beacon::GeoPosition;

        // Create test beacons
        let mut nearby_beacons = Vec::new();

        // Beacon from lower hierarchy level (Platform < Platoon)
        // This should be tracked as a linked peer
        let mut linked_beacon = GeographicBeacon::new(
            "linked-peer".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Platform,
        );
        linked_beacon.can_parent = false; // Lower level nodes typically don't parent
        nearby_beacons.push(linked_beacon);

        // Beacon from same hierarchy level
        // This should NOT be tracked (not a valid linked peer)
        let mut same_level_beacon = GeographicBeacon::new(
            "same-level".to_string(),
            GeoPosition::new(37.7751, -122.4196),
            HierarchyLevel::Platoon,
        );
        same_level_beacon.can_parent = true;
        nearby_beacons.push(same_level_beacon);

        let mut state = TopologyState::default();
        let config = TopologyConfig::default();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Initial update - should add linked peer
        TopologyBuilder::update_linked_peers(
            &mut state,
            &config,
            &nearby_beacons,
            HierarchyLevel::Platoon, // We are Platoon level
            &event_tx,
        );

        // Check state
        assert_eq!(state.linked_peers.len(), 1);
        assert!(state.linked_peers.contains_key("linked-peer"));

        // Check event
        let event = event_rx.try_recv().unwrap();
        match event {
            TopologyEvent::PeerAdded { linked_peer_id } => {
                assert_eq!(linked_peer_id, "linked-peer");
            }
            _ => panic!("Expected PeerAdded event"),
        }
    }

    #[test]
    fn test_linked_peer_expiry() {
        use std::time::Duration;

        let mut state = TopologyState::default();
        let config = TopologyConfig {
            peer_timeout: Duration::from_millis(100), // Short timeout for test
            ..Default::default()
        };

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Manually add a linked peer with old timestamp
        let old_time = Instant::now() - Duration::from_millis(200);
        state
            .linked_peers
            .insert("stale-peer".to_string(), old_time);

        // Update with empty nearby beacons (peer disappeared)
        let nearby_beacons = Vec::new();

        TopologyBuilder::update_linked_peers(
            &mut state,
            &config,
            &nearby_beacons,
            HierarchyLevel::Platoon,
            &event_tx,
        );

        // Check state - stale peer should be removed
        assert_eq!(state.linked_peers.len(), 0);

        // Check event
        let event = event_rx.try_recv().unwrap();
        match event {
            TopologyEvent::PeerRemoved { linked_peer_id } => {
                assert_eq!(linked_peer_id, "stale-peer");
            }
            _ => panic!("Expected PeerRemoved event"),
        }
    }

    // --- Additional coverage tests ---

    #[tokio::test]
    async fn test_get_state_initial_values() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer = Arc::new(BeaconObserver::new(storage, "9q8yy".to_string()));

        let builder = TopologyBuilder::new(
            TopologyConfig::default(),
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        let state = builder.get_state();
        assert!(state.selected_peer.is_none());
        assert!(state.linked_peers.is_empty());
        assert!(state.lateral_peers.is_empty());
        assert_eq!(state.hierarchy_level, HierarchyLevel::Squad);
        assert_eq!(state.role, crate::hierarchy::NodeRole::Standalone);
    }

    #[tokio::test]
    async fn test_get_selected_peer_returns_none_initially() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer = Arc::new(BeaconObserver::new(storage, "9q8yy".to_string()));

        let builder = TopologyBuilder::new(
            TopologyConfig::default(),
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        assert!(builder.get_selected_peer().is_none());
    }

    #[tokio::test]
    async fn test_config_returns_the_config() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer = Arc::new(BeaconObserver::new(storage, "9q8yy".to_string()));

        let config = TopologyConfig {
            peer_change_cooldown: Duration::from_secs(120),
            peer_timeout: Duration::from_secs(300),
            max_retries: 5,
            ..Default::default()
        };

        let builder = TopologyBuilder::new(
            config,
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        let cfg = builder.config();
        assert_eq!(cfg.peer_change_cooldown, Duration::from_secs(120));
        assert_eq!(cfg.peer_timeout, Duration::from_secs(300));
        assert_eq!(cfg.max_retries, 5);
    }

    #[tokio::test]
    async fn test_stop_aborts_task_handle() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer = Arc::new(BeaconObserver::new(storage, "9q8yy".to_string()));

        let builder = TopologyBuilder::new(
            TopologyConfig::default(),
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        // Stop before start should be a no-op (no panic)
        builder.stop().await;

        // Start, then stop
        builder.start().await;
        // task_handle should be set
        assert!(builder.task_handle.lock().unwrap().is_some());

        builder.stop().await;
        // task_handle should be taken (None)
        assert!(builder.task_handle.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn test_start_is_idempotent() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer = Arc::new(BeaconObserver::new(storage, "9q8yy".to_string()));

        let builder = TopologyBuilder::new(
            TopologyConfig::default(),
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        builder.start().await;
        // Calling start a second time should be a no-op (not spawn a second task)
        builder.start().await;
        assert!(builder.task_handle.lock().unwrap().is_some());

        builder.stop().await;
    }

    #[tokio::test]
    async fn test_reevaluate_peer_no_candidates() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer = Arc::new(BeaconObserver::new(storage, "9q8yy".to_string()));

        let builder = TopologyBuilder::new(
            TopologyConfig::default(),
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        // No beacons in observer, so reevaluate should not select any peer
        builder.reevaluate_peer().await;
        assert!(builder.get_selected_peer().is_none());
    }

    #[tokio::test]
    async fn test_reevaluate_peer_cooldown_not_elapsed() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer = Arc::new(BeaconObserver::new(storage, "9q8yy".to_string()));

        let config = TopologyConfig {
            peer_change_cooldown: Duration::from_secs(3600), // Very long cooldown
            ..Default::default()
        };

        let builder = TopologyBuilder::new(
            config,
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        // Manually set a selected peer via update_selected_peer
        let beacon = create_test_beacon("peer-A", HierarchyLevel::Platoon);
        {
            let mut state = builder.state.lock().unwrap();
            TopologyBuilder::update_selected_peer(&mut state, &builder.event_tx, beacon);
        }
        assert_eq!(builder.get_selected_peer().unwrap().node_id, "peer-A");

        // Even if reevaluate is called, cooldown hasn't elapsed so peer should not change
        builder.reevaluate_peer().await;
        assert_eq!(builder.get_selected_peer().unwrap().node_id, "peer-A");
    }

    #[test]
    fn test_check_peer_status_no_selected_peer() {
        let mut state = TopologyState::default();
        let config = TopologyConfig::default();
        let (event_tx, _event_rx) = mpsc::unbounded_channel();

        let nearby: Vec<GeographicBeacon> = vec![];

        // With no selected peer, check_peer_status should return true (needs peer)
        let needs_peer =
            TopologyBuilder::check_peer_status(&mut state, &config, &nearby, &event_tx);
        assert!(needs_peer);
    }

    #[test]
    fn test_check_peer_status_peer_still_visible() {
        let mut state = TopologyState::default();
        let config = TopologyConfig::default();
        let (event_tx, _event_rx) = mpsc::unbounded_channel();

        // Set a selected peer
        let beacon = create_test_beacon("peer-A", HierarchyLevel::Platoon);
        state.selected_peer = Some(SelectedPeer {
            node_id: "peer-A".to_string(),
            beacon: beacon.clone(),
            selected_at: Instant::now(),
        });

        // Peer is still visible in nearby beacons
        let nearby = vec![beacon];
        let needs_peer =
            TopologyBuilder::check_peer_status(&mut state, &config, &nearby, &event_tx);
        assert!(!needs_peer);
        assert!(state.selected_peer.is_some()); // Still selected
    }

    #[test]
    fn test_check_peer_status_peer_lost_after_timeout() {
        let mut state = TopologyState::default();
        let config = TopologyConfig {
            peer_timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Set a selected peer with an old selected_at time
        let beacon = create_test_beacon("peer-A", HierarchyLevel::Platoon);
        state.selected_peer = Some(SelectedPeer {
            node_id: "peer-A".to_string(),
            beacon,
            selected_at: Instant::now() - Duration::from_millis(200),
        });

        // Peer is NOT in nearby beacons and timeout has passed
        let nearby: Vec<GeographicBeacon> = vec![];
        let needs_peer =
            TopologyBuilder::check_peer_status(&mut state, &config, &nearby, &event_tx);
        assert!(needs_peer);
        assert!(state.selected_peer.is_none()); // Cleared

        // Should have emitted PeerLost event
        let event = event_rx.try_recv().unwrap();
        match event {
            TopologyEvent::PeerLost { lost_peer_id } => {
                assert_eq!(lost_peer_id, "peer-A");
            }
            _ => panic!("Expected PeerLost event"),
        }
    }

    #[test]
    fn test_check_peer_status_peer_not_visible_but_not_timed_out() {
        let mut state = TopologyState::default();
        let config = TopologyConfig {
            peer_timeout: Duration::from_secs(3600), // Very long timeout
            ..Default::default()
        };
        let (event_tx, _event_rx) = mpsc::unbounded_channel();

        // Set a recently selected peer
        let beacon = create_test_beacon("peer-A", HierarchyLevel::Platoon);
        state.selected_peer = Some(SelectedPeer {
            node_id: "peer-A".to_string(),
            beacon,
            selected_at: Instant::now(),
        });

        // Peer is NOT in nearby beacons, but timeout has not elapsed
        let nearby: Vec<GeographicBeacon> = vec![];
        let needs_peer =
            TopologyBuilder::check_peer_status(&mut state, &config, &nearby, &event_tx);
        assert!(!needs_peer); // Still within timeout, don't need new peer yet
        assert!(state.selected_peer.is_some()); // Not cleared
    }

    #[test]
    fn test_update_selected_peer_first_selection() {
        let mut state = TopologyState::default();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        let beacon = create_test_beacon("peer-A", HierarchyLevel::Platoon);
        TopologyBuilder::update_selected_peer(&mut state, &event_tx, beacon);

        // State should have the selected peer
        let selected = state.selected_peer.as_ref().unwrap();
        assert_eq!(selected.node_id, "peer-A");

        // Should have emitted PeerSelected event
        let event = event_rx.try_recv().unwrap();
        match event {
            TopologyEvent::PeerSelected {
                selected_peer_id,
                peer_beacon,
            } => {
                assert_eq!(selected_peer_id, "peer-A");
                assert_eq!(peer_beacon.node_id, "peer-A");
            }
            _ => panic!("Expected PeerSelected event"),
        }
    }

    #[test]
    fn test_update_selected_peer_change_event() {
        let mut state = TopologyState::default();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Set initial peer
        let beacon_a = create_test_beacon("peer-A", HierarchyLevel::Platoon);
        TopologyBuilder::update_selected_peer(&mut state, &event_tx, beacon_a);
        let _ = event_rx.try_recv(); // Consume PeerSelected event

        // Now change to a different peer
        let beacon_b = create_test_beacon("peer-B", HierarchyLevel::Platoon);
        TopologyBuilder::update_selected_peer(&mut state, &event_tx, beacon_b);

        // State should have the new peer
        assert_eq!(state.selected_peer.as_ref().unwrap().node_id, "peer-B");

        // Should have emitted PeerChanged event
        let event = event_rx.try_recv().unwrap();
        match event {
            TopologyEvent::PeerChanged {
                old_peer_id,
                new_peer_id,
                new_peer_beacon,
            } => {
                assert_eq!(old_peer_id, "peer-A");
                assert_eq!(new_peer_id, "peer-B");
                assert_eq!(new_peer_beacon.node_id, "peer-B");
            }
            _ => panic!("Expected PeerChanged event"),
        }
    }

    #[test]
    fn test_update_lateral_peers_discovery() {
        let mut state = TopologyState::default();
        let config = TopologyConfig::default();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Create beacons at the same level (Platoon) as our own level
        let lateral_beacon = create_test_beacon("lateral-1", HierarchyLevel::Platoon);

        // Also create a beacon at a different level (should be ignored)
        let different_level_beacon = create_test_beacon("lower-1", HierarchyLevel::Squad);

        let nearby = vec![lateral_beacon.clone(), different_level_beacon];

        TopologyBuilder::update_lateral_peers(
            &mut state,
            &config,
            &nearby,
            HierarchyLevel::Platoon,
            &event_tx,
        );

        // Only the same-level peer should be tracked
        assert_eq!(state.lateral_peers.len(), 1);
        assert!(state.lateral_peers.contains_key("lateral-1"));

        // Check LateralPeerDiscovered event
        let event = event_rx.try_recv().unwrap();
        match event {
            TopologyEvent::LateralPeerDiscovered {
                peer_id,
                peer_beacon,
            } => {
                assert_eq!(peer_id, "lateral-1");
                assert_eq!(peer_beacon.node_id, "lateral-1");
            }
            _ => panic!("Expected LateralPeerDiscovered event"),
        }

        // No more events
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn test_update_lateral_peers_refresh_existing() {
        let mut state = TopologyState::default();
        let config = TopologyConfig::default();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Pre-insert a lateral peer with an old timestamp
        let old_time = Instant::now() - Duration::from_secs(60);
        state
            .lateral_peers
            .insert("lateral-1".to_string(), old_time);

        // Beacon is still visible
        let lateral_beacon = create_test_beacon("lateral-1", HierarchyLevel::Platoon);
        let nearby = vec![lateral_beacon];

        TopologyBuilder::update_lateral_peers(
            &mut state,
            &config,
            &nearby,
            HierarchyLevel::Platoon,
            &event_tx,
        );

        // Peer should still exist and last_seen should be refreshed (more recent than old_time)
        assert_eq!(state.lateral_peers.len(), 1);
        let last_seen = state.lateral_peers.get("lateral-1").unwrap();
        assert!(*last_seen > old_time);

        // No discovery event since peer was already known
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn test_update_lateral_peers_expiry() {
        let mut state = TopologyState::default();
        let config = TopologyConfig {
            peer_timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Pre-insert a lateral peer with an old timestamp (past timeout)
        let old_time = Instant::now() - Duration::from_millis(200);
        state
            .lateral_peers
            .insert("stale-lateral".to_string(), old_time);

        // Empty nearby beacons
        let nearby: Vec<GeographicBeacon> = vec![];

        TopologyBuilder::update_lateral_peers(
            &mut state,
            &config,
            &nearby,
            HierarchyLevel::Platoon,
            &event_tx,
        );

        // Stale peer should be removed
        assert!(state.lateral_peers.is_empty());

        // Check LateralPeerLost event
        let event = event_rx.try_recv().unwrap();
        match event {
            TopologyEvent::LateralPeerLost { peer_id } => {
                assert_eq!(peer_id, "stale-lateral");
            }
            _ => panic!("Expected LateralPeerLost event"),
        }
    }

    #[test]
    fn test_update_lateral_peers_not_expired_within_timeout() {
        let mut state = TopologyState::default();
        let config = TopologyConfig {
            peer_timeout: Duration::from_secs(3600), // Very long timeout
            ..Default::default()
        };
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Pre-insert a lateral peer with a recent timestamp
        state
            .lateral_peers
            .insert("recent-lateral".to_string(), Instant::now());

        // Empty nearby beacons (peer not visible), but timeout not elapsed
        let nearby: Vec<GeographicBeacon> = vec![];

        TopologyBuilder::update_lateral_peers(
            &mut state,
            &config,
            &nearby,
            HierarchyLevel::Platoon,
            &event_tx,
        );

        // Peer should NOT be removed (within timeout)
        assert_eq!(state.lateral_peers.len(), 1);
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn test_topology_config_default_values() {
        let config = TopologyConfig::default();

        assert_eq!(config.reevaluation_interval, Some(Duration::from_secs(30)));
        assert_eq!(config.peer_change_cooldown, Duration::from_secs(60));
        assert_eq!(config.peer_timeout, Duration::from_secs(180));
        assert!(config.hierarchy_strategy.is_none());
        assert_eq!(config.max_lateral_connections, Some(10));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_backoff, Duration::from_secs(1));
        assert_eq!(config.max_backoff, Duration::from_secs(60));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.max_telemetry_buffer_size, 100);
        assert!(config.metrics_collector.is_none());
        assert!(config.partition_detector.is_none());
    }

    #[test]
    fn test_topology_state_default_values() {
        let state = TopologyState::default();

        assert!(state.selected_peer.is_none());
        assert!(state.linked_peers.is_empty());
        assert!(state.lateral_peers.is_empty());
        assert_eq!(state.role, crate::hierarchy::NodeRole::Standalone);
        assert_eq!(state.hierarchy_level, HierarchyLevel::Squad);
    }

    #[test]
    fn test_topology_event_enum_variants() {
        // Verify all event variants can be constructed and debug-printed
        let beacon = create_test_beacon("peer-1", HierarchyLevel::Platoon);

        let events: Vec<TopologyEvent> = vec![
            TopologyEvent::PeerSelected {
                selected_peer_id: "peer-1".to_string(),
                peer_beacon: beacon.clone(),
            },
            TopologyEvent::PeerChanged {
                old_peer_id: "peer-old".to_string(),
                new_peer_id: "peer-new".to_string(),
                new_peer_beacon: beacon.clone(),
            },
            TopologyEvent::PeerLost {
                lost_peer_id: "peer-lost".to_string(),
            },
            TopologyEvent::PeerAdded {
                linked_peer_id: "linked-1".to_string(),
            },
            TopologyEvent::PeerRemoved {
                linked_peer_id: "linked-1".to_string(),
            },
            TopologyEvent::LateralPeerDiscovered {
                peer_id: "lateral-1".to_string(),
                peer_beacon: beacon.clone(),
            },
            TopologyEvent::LateralPeerLost {
                peer_id: "lateral-1".to_string(),
            },
            TopologyEvent::RoleChanged {
                old_role: crate::hierarchy::NodeRole::Standalone,
                new_role: crate::hierarchy::NodeRole::Leader,
            },
            TopologyEvent::LevelChanged {
                old_level: HierarchyLevel::Squad,
                new_level: HierarchyLevel::Platoon,
            },
        ];

        // All variants should be Debug-printable and Clone-able
        for event in &events {
            let debug_str = format!("{:?}", event);
            assert!(!debug_str.is_empty());
            let _cloned = event.clone();
        }
        assert_eq!(events.len(), 9);
    }

    #[test]
    fn test_selected_peer_struct() {
        let beacon = create_test_beacon("peer-1", HierarchyLevel::Platoon);
        let now = Instant::now();

        let selected = SelectedPeer {
            node_id: "peer-1".to_string(),
            beacon: beacon.clone(),
            selected_at: now,
        };

        assert_eq!(selected.node_id, "peer-1");
        assert_eq!(selected.beacon.node_id, "peer-1");
        // selected_at should be very recent
        assert!(selected.selected_at.elapsed() < Duration::from_secs(1));

        // Verify Clone and Debug
        let cloned = selected.clone();
        assert_eq!(cloned.node_id, "peer-1");
        let debug_str = format!("{:?}", selected);
        assert!(debug_str.contains("peer-1"));
    }

    #[test]
    fn test_linked_peer_refresh_existing() {
        let mut state = TopologyState::default();
        let config = TopologyConfig::default();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Pre-insert a linked peer with an old timestamp
        let old_time = Instant::now() - Duration::from_secs(60);
        state
            .linked_peers
            .insert("linked-peer".to_string(), old_time);

        // Create a beacon that qualifies as a linked peer (lower level than ours)
        let mut beacon = GeographicBeacon::new(
            "linked-peer".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Platform,
        );
        beacon.can_parent = false;
        let nearby = vec![beacon];

        TopologyBuilder::update_linked_peers(
            &mut state,
            &config,
            &nearby,
            HierarchyLevel::Platoon, // We are Platoon, beacon is Platform
            &event_tx,
        );

        // Peer should still be tracked, but with refreshed last_seen
        assert_eq!(state.linked_peers.len(), 1);
        let last_seen = state.linked_peers.get("linked-peer").unwrap();
        assert!(*last_seen > old_time);

        // No event should be emitted (peer was already known)
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn test_linked_peer_not_expired_within_timeout() {
        let mut state = TopologyState::default();
        let config = TopologyConfig {
            peer_timeout: Duration::from_secs(3600),
            ..Default::default()
        };
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Pre-insert a linked peer with a recent timestamp
        state
            .linked_peers
            .insert("recent-linked".to_string(), Instant::now());

        // Empty nearby (peer disappeared) but timeout not elapsed
        let nearby: Vec<GeographicBeacon> = vec![];

        TopologyBuilder::update_linked_peers(
            &mut state,
            &config,
            &nearby,
            HierarchyLevel::Platoon,
            &event_tx,
        );

        // Peer should NOT be removed (within timeout)
        assert_eq!(state.linked_peers.len(), 1);
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn test_linked_peers_higher_level_not_tracked() {
        let mut state = TopologyState::default();
        let config = TopologyConfig::default();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Create a beacon at a HIGHER level than ours -- should NOT be tracked as linked peer
        let mut beacon = GeographicBeacon::new(
            "higher-peer".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Company,
        );
        beacon.can_parent = true;
        let nearby = vec![beacon];

        TopologyBuilder::update_linked_peers(
            &mut state,
            &config,
            &nearby,
            HierarchyLevel::Platoon, // We are Platoon, beacon is Company (higher)
            &event_tx,
        );

        // Should not be tracked as a linked peer
        assert!(state.linked_peers.is_empty());
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_clone_topology_builder() {
        let storage = Arc::new(MockBeaconStorage::new());
        let observer = Arc::new(BeaconObserver::new(storage, "9q8yy".to_string()));

        let builder = TopologyBuilder::new(
            TopologyConfig::default(),
            "node-1".to_string(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
            None,
            observer,
        );

        let cloned = builder.clone();

        // Cloned builder should share the same state (Arc)
        assert!(cloned.get_selected_peer().is_none());
        assert!(cloned.get_state().linked_peers.is_empty());

        // Clone's event_rx should be None (not cloned)
        assert!(cloned.subscribe().is_none());

        // Original's subscribe should still work (if not already taken)
        let rx = builder.subscribe();
        assert!(rx.is_some());
    }

    #[test]
    fn test_update_lateral_peers_multiple_peers() {
        let mut state = TopologyState::default();
        let config = TopologyConfig::default();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        let lateral_a = create_test_beacon("lateral-A", HierarchyLevel::Platoon);
        let lateral_b = create_test_beacon("lateral-B", HierarchyLevel::Platoon);
        let nearby = vec![lateral_a, lateral_b];

        TopologyBuilder::update_lateral_peers(
            &mut state,
            &config,
            &nearby,
            HierarchyLevel::Platoon,
            &event_tx,
        );

        assert_eq!(state.lateral_peers.len(), 2);
        assert!(state.lateral_peers.contains_key("lateral-A"));
        assert!(state.lateral_peers.contains_key("lateral-B"));

        // Two discovery events
        let mut discovered_ids: Vec<String> = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            match event {
                TopologyEvent::LateralPeerDiscovered { peer_id, .. } => {
                    discovered_ids.push(peer_id);
                }
                _ => panic!("Expected LateralPeerDiscovered events"),
            }
        }
        discovered_ids.sort();
        assert_eq!(discovered_ids, vec!["lateral-A", "lateral-B"]);
    }

    #[test]
    fn test_update_linked_peers_multiple_additions() {
        let mut state = TopologyState::default();
        let config = TopologyConfig::default();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Two beacons at lower levels
        let mut beacon_a = GeographicBeacon::new(
            "linked-A".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Platform,
        );
        beacon_a.can_parent = false;

        let mut beacon_b = GeographicBeacon::new(
            "linked-B".to_string(),
            GeoPosition::new(37.7751, -122.4196),
            HierarchyLevel::Squad,
        );
        beacon_b.can_parent = false;

        let nearby = vec![beacon_a, beacon_b];

        TopologyBuilder::update_linked_peers(
            &mut state,
            &config,
            &nearby,
            HierarchyLevel::Company, // We are Company, both Platform and Squad are lower
            &event_tx,
        );

        assert_eq!(state.linked_peers.len(), 2);

        let mut added_ids: Vec<String> = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            match event {
                TopologyEvent::PeerAdded { linked_peer_id } => {
                    added_ids.push(linked_peer_id);
                }
                _ => panic!("Expected PeerAdded events"),
            }
        }
        added_ids.sort();
        assert_eq!(added_ids, vec!["linked-A", "linked-B"]);
    }

    /// Helper to create a test beacon with sensible defaults
    fn create_test_beacon(node_id: &str, level: HierarchyLevel) -> GeographicBeacon {
        let mut beacon = GeographicBeacon::new(
            node_id.to_string(),
            GeoPosition::new(37.7750, -122.4195),
            level,
        );
        beacon.can_parent = true;
        beacon.parent_priority = 100;
        beacon
    }
}
