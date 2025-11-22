//! Topology builder for beacon-driven mesh formation
//!
//! This module implements the TopologyBuilder which coordinates topology formation
//! by observing nearby beacons, selecting peers, and maintaining topology state.

use crate::beacon::{BeaconObserver, GeoPosition, GeographicBeacon, HierarchyLevel, NodeProfile};
use crate::topology::selection::{PeerSelector, SelectionConfig};
use std::collections::HashMap;
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
    /// Linked peer joined under this node
    PeerAdded { linked_peer_id: String },
    /// Linked peer left
    PeerRemoved { linked_peer_id: String },
}

/// Current topology state
#[derive(Debug, Clone, Default)]
pub struct TopologyState {
    /// Current selected peer (if any)
    pub selected_peer: Option<SelectedPeer>,
    /// Current linked peers (node_id -> last_seen)
    pub linked_peers: HashMap<String, Instant>,
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
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            selection: SelectionConfig::default(),
            reevaluation_interval: Some(Duration::from_secs(30)),
            peer_change_cooldown: Duration::from_secs(60),
            peer_timeout: Duration::from_secs(180), // 3 minutes
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

                // Check current peer status
                let mut state_lock = state.lock().unwrap();
                let needs_peer =
                    Self::check_peer_status(&mut state_lock, &config, &nearby, &event_tx);

                if needs_peer {
                    // Select new peer
                    if let Some(candidate) = selector.select_peer(&nearby) {
                        Self::update_selected_peer(&mut state_lock, &event_tx, candidate.beacon);
                    }
                }

                drop(state_lock);
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
}
