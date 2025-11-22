//! Topology builder for beacon-driven mesh formation
//!
//! This module implements the TopologyBuilder which coordinates topology formation
//! by observing nearby beacons, selecting parents, and maintaining hierarchy state.

use crate::beacon::{BeaconObserver, GeoPosition, GeographicBeacon, HierarchyLevel, NodeProfile};
use crate::topology::selection::{ParentSelector, SelectionConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Topology change events
#[derive(Debug, Clone)]
pub enum TopologyEvent {
    /// Parent selected for the first time
    ParentSelected {
        parent_id: String,
        parent_beacon: GeographicBeacon,
    },
    /// Parent changed (re-parenting occurred)
    ParentChanged {
        old_parent_id: String,
        new_parent_id: String,
        new_parent_beacon: GeographicBeacon,
    },
    /// Parent lost (became unavailable)
    ParentLost { parent_id: String },
    /// Child node joined under this node as parent
    ChildAdded { child_id: String },
    /// Child node left
    ChildRemoved { child_id: String },
}

/// Current topology state
#[derive(Debug, Clone, Default)]
pub struct TopologyState {
    /// Current parent node (if any)
    pub parent: Option<ParentInfo>,
    /// Current children nodes (node_id -> last_seen)
    pub children: HashMap<String, Instant>,
}

#[derive(Debug, Clone)]
pub struct ParentInfo {
    pub node_id: String,
    pub beacon: GeographicBeacon,
    pub selected_at: Instant,
}

/// Configuration for topology builder
#[derive(Debug, Clone)]
pub struct TopologyConfig {
    /// Parent selection configuration
    pub selection: SelectionConfig,
    /// How often to re-evaluate parent selection (None = only on beacon changes)
    pub reevaluation_interval: Option<Duration>,
    /// Minimum time before re-parenting (prevents thrashing)
    pub reparent_cooldown: Duration,
    /// Time before considering parent lost if no beacon received
    pub parent_timeout: Duration,
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            selection: SelectionConfig::default(),
            reevaluation_interval: Some(Duration::from_secs(30)),
            reparent_cooldown: Duration::from_secs(60),
            parent_timeout: Duration::from_secs(180), // 3 minutes
        }
    }
}

/// Topology builder
///
/// Coordinates topology formation by:
/// - Observing nearby beacons
/// - Selecting optimal parents
/// - Managing parent/child relationships
/// - Handling dynamic re-parenting
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
                    ParentSelector::new(config.selection.clone(), current_pos, hierarchy_level);

                // Get nearby beacons
                let nearby = observer.get_nearby_beacons().await;

                // Check current parent status
                let mut state_lock = state.lock().unwrap();
                let needs_parent = Self::check_parent_status(&mut state_lock, &config, &nearby);

                if needs_parent {
                    // Select new parent
                    if let Some(candidate) = selector.select_parent(&nearby) {
                        Self::update_parent(&mut state_lock, &event_tx, candidate.beacon);
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

    /// Get current parent
    pub fn get_parent(&self) -> Option<ParentInfo> {
        self.state.lock().unwrap().parent.clone()
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

    /// Force immediate re-evaluation of parent selection
    pub async fn reevaluate_parent(&self) {
        let current_pos = *self.position.lock().unwrap();
        let selector = ParentSelector::new(
            self.config.selection.clone(),
            current_pos,
            self.hierarchy_level,
        );

        let nearby = self.observer.get_nearby_beacons().await;
        let mut state_lock = self.state.lock().unwrap();

        if let Some(candidate) = selector.select_parent(&nearby) {
            // Check if this is better than current parent
            let should_switch = if let Some(ref current) = state_lock.parent {
                // Only switch if cooldown period has passed
                let elapsed = current.selected_at.elapsed();
                if elapsed < self.config.reparent_cooldown {
                    false
                } else {
                    // Re-score current parent and compare
                    let current_score = if let Some(current_beacon) =
                        nearby.iter().find(|b| b.node_id == current.node_id)
                    {
                        selector
                            .select_parent(std::slice::from_ref(current_beacon))
                            .map(|c| c.score)
                            .unwrap_or(0.0)
                    } else {
                        0.0 // Current parent not visible anymore
                    };

                    candidate.score > current_score * 1.1 // 10% hysteresis
                }
            } else {
                true // No current parent, definitely select
            };

            if should_switch {
                Self::update_parent(&mut state_lock, &self.event_tx, candidate.beacon);
            }
        }
    }

    /// Check parent status and determine if new parent needed
    fn check_parent_status(
        state: &mut TopologyState,
        config: &TopologyConfig,
        nearby: &[GeographicBeacon],
    ) -> bool {
        if let Some(ref parent) = state.parent {
            // Check if parent is still visible
            if nearby.iter().any(|b| b.node_id == parent.node_id) {
                // Parent still visible
                false
            } else {
                // Check timeout
                if parent.selected_at.elapsed() > config.parent_timeout {
                    // Parent lost
                    state.parent = None;
                    true
                } else {
                    false
                }
            }
        } else {
            // No parent, need one
            true
        }
    }

    /// Update current parent
    fn update_parent(
        state: &mut TopologyState,
        event_tx: &mpsc::UnboundedSender<TopologyEvent>,
        new_parent_beacon: GeographicBeacon,
    ) {
        let new_parent_id = new_parent_beacon.node_id.clone();

        let event = if let Some(ref current) = state.parent {
            TopologyEvent::ParentChanged {
                old_parent_id: current.node_id.clone(),
                new_parent_id: new_parent_id.clone(),
                new_parent_beacon: new_parent_beacon.clone(),
            }
        } else {
            TopologyEvent::ParentSelected {
                parent_id: new_parent_id.clone(),
                parent_beacon: new_parent_beacon.clone(),
            }
        };

        state.parent = Some(ParentInfo {
            node_id: new_parent_id,
            beacon: new_parent_beacon,
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
        assert!(state.parent.is_none());
        assert!(state.children.is_empty());
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
