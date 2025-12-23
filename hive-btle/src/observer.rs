//! Observer pattern for HIVE mesh events
//!
//! This module provides the event types and observer trait for receiving
//! notifications about mesh state changes. Platform implementations register
//! observers to receive callbacks when peers are discovered, connected,
//! disconnected, or when documents are synced.
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::observer::{HiveEvent, HiveObserver};
//!
//! struct MyObserver;
//!
//! impl HiveObserver for MyObserver {
//!     fn on_event(&self, event: HiveEvent) {
//!         match event {
//!             HiveEvent::PeerDiscovered { peer } => {
//!                 println!("Discovered: {}", peer.display_name());
//!             }
//!             HiveEvent::EmergencyReceived { from_node } => {
//!                 println!("EMERGENCY from {:08X}", from_node.as_u32());
//!             }
//!             _ => {}
//!         }
//!     }
//! }
//! ```

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, sync::Arc, vec::Vec};
#[cfg(feature = "std")]
use std::sync::Arc;

use crate::peer::HivePeer;
use crate::sync::crdt::EventType;
use crate::NodeId;

/// Events emitted by the HIVE mesh
///
/// These events notify observers about changes in mesh state, peer lifecycle,
/// and document synchronization.
#[derive(Debug, Clone)]
pub enum HiveEvent {
    // ==================== Peer Lifecycle Events ====================
    /// A new peer was discovered via BLE scanning
    PeerDiscovered {
        /// The discovered peer
        peer: HivePeer,
    },

    /// A peer connected to us (either direction)
    PeerConnected {
        /// Node ID of the connected peer
        node_id: NodeId,
    },

    /// A peer disconnected
    PeerDisconnected {
        /// Node ID of the disconnected peer
        node_id: NodeId,
        /// Reason for disconnection
        reason: DisconnectReason,
    },

    /// A peer was removed due to timeout (stale)
    PeerLost {
        /// Node ID of the lost peer
        node_id: NodeId,
    },

    // ==================== Mesh Events ====================
    /// An emergency event was received from a peer
    EmergencyReceived {
        /// Node ID that sent the emergency
        from_node: NodeId,
    },

    /// An ACK event was received from a peer
    AckReceived {
        /// Node ID that sent the ACK
        from_node: NodeId,
    },

    /// A generic event was received from a peer
    EventReceived {
        /// Node ID that sent the event
        from_node: NodeId,
        /// Type of event
        event_type: EventType,
    },

    /// A document was synced with a peer
    DocumentSynced {
        /// Node ID that we synced with
        from_node: NodeId,
        /// Updated total counter value
        total_count: u64,
    },

    // ==================== Mesh State Events ====================
    /// Mesh state changed (peer count, connected count)
    MeshStateChanged {
        /// Total number of known peers
        peer_count: usize,
        /// Number of connected peers
        connected_count: usize,
    },

    /// All peers have acknowledged an emergency
    AllPeersAcked {
        /// Number of peers that acknowledged
        ack_count: usize,
    },
}

impl HiveEvent {
    /// Create a peer discovered event
    pub fn peer_discovered(peer: HivePeer) -> Self {
        Self::PeerDiscovered { peer }
    }

    /// Create a peer connected event
    pub fn peer_connected(node_id: NodeId) -> Self {
        Self::PeerConnected { node_id }
    }

    /// Create a peer disconnected event
    pub fn peer_disconnected(node_id: NodeId, reason: DisconnectReason) -> Self {
        Self::PeerDisconnected { node_id, reason }
    }

    /// Create a peer lost event (timeout)
    pub fn peer_lost(node_id: NodeId) -> Self {
        Self::PeerLost { node_id }
    }

    /// Create an emergency received event
    pub fn emergency_received(from_node: NodeId) -> Self {
        Self::EmergencyReceived { from_node }
    }

    /// Create an ACK received event
    pub fn ack_received(from_node: NodeId) -> Self {
        Self::AckReceived { from_node }
    }

    /// Create a generic event received
    pub fn event_received(from_node: NodeId, event_type: EventType) -> Self {
        Self::EventReceived {
            from_node,
            event_type,
        }
    }

    /// Create a document synced event
    pub fn document_synced(from_node: NodeId, total_count: u64) -> Self {
        Self::DocumentSynced {
            from_node,
            total_count,
        }
    }
}

/// Reason for peer disconnection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisconnectReason {
    /// Local initiated disconnect
    LocalRequest,
    /// Remote peer initiated disconnect
    RemoteRequest,
    /// Connection timed out
    Timeout,
    /// BLE link lost
    LinkLoss,
    /// Connection failed
    ConnectionFailed,
    /// Unknown reason
    #[default]
    Unknown,
}

/// Observer trait for receiving HIVE mesh events
///
/// Implement this trait to receive callbacks when mesh events occur.
/// Observers must be thread-safe (Send + Sync) as they may be called
/// from any thread.
///
/// ## Platform Notes
///
/// - **iOS/macOS**: Wrap in a Swift class that conforms to this protocol via UniFFI
/// - **Android**: Implement via JNI callback interface
/// - **ESP32**: Use direct Rust implementation with static callbacks
pub trait HiveObserver: Send + Sync {
    /// Called when a mesh event occurs
    ///
    /// This method should return quickly to avoid blocking the mesh.
    /// If heavy processing is needed, dispatch to another thread.
    fn on_event(&self, event: HiveEvent);
}

/// A simple observer that collects events into a vector (useful for testing)
#[cfg(feature = "std")]
#[derive(Debug, Default)]
pub struct CollectingObserver {
    events: std::sync::Mutex<Vec<HiveEvent>>,
}

#[cfg(feature = "std")]
impl CollectingObserver {
    /// Create a new collecting observer
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Get all collected events
    pub fn events(&self) -> Vec<HiveEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Clear collected events
    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    /// Get count of collected events
    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

#[cfg(feature = "std")]
impl HiveObserver for CollectingObserver {
    fn on_event(&self, event: HiveEvent) {
        self.events.lock().unwrap().push(event);
    }
}

/// Helper to manage multiple observers
#[cfg(feature = "std")]
pub struct ObserverManager {
    observers: std::sync::RwLock<Vec<Arc<dyn HiveObserver>>>,
}

#[cfg(feature = "std")]
impl Default for ObserverManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "std")]
impl ObserverManager {
    /// Create a new observer manager
    pub fn new() -> Self {
        Self {
            observers: std::sync::RwLock::new(Vec::new()),
        }
    }

    /// Add an observer
    pub fn add(&self, observer: Arc<dyn HiveObserver>) {
        self.observers.write().unwrap().push(observer);
    }

    /// Remove an observer (by Arc pointer equality)
    pub fn remove(&self, observer: &Arc<dyn HiveObserver>) {
        self.observers
            .write()
            .unwrap()
            .retain(|o| !Arc::ptr_eq(o, observer));
    }

    /// Notify all observers of an event
    pub fn notify(&self, event: HiveEvent) {
        // Use try_read to avoid panicking on poisoned locks
        if let Ok(observers) = self.observers.try_read() {
            for observer in observers.iter() {
                observer.on_event(event.clone());
            }
        }
    }

    /// Get the number of registered observers
    pub fn count(&self) -> usize {
        self.observers.read().unwrap().len()
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_collecting_observer() {
        let observer = CollectingObserver::new();

        observer.on_event(HiveEvent::peer_connected(NodeId::new(0x12345678)));
        observer.on_event(HiveEvent::emergency_received(NodeId::new(0x87654321)));

        assert_eq!(observer.count(), 2);

        let events = observer.events();
        assert!(matches!(events[0], HiveEvent::PeerConnected { .. }));
        assert!(matches!(events[1], HiveEvent::EmergencyReceived { .. }));

        observer.clear();
        assert_eq!(observer.count(), 0);
    }

    #[test]
    fn test_observer_manager() {
        let manager = ObserverManager::new();

        // Keep concrete references for count checks
        let obs1_concrete = Arc::new(CollectingObserver::new());
        let obs2_concrete = Arc::new(CollectingObserver::new());
        let observer1: Arc<dyn HiveObserver> = obs1_concrete.clone();
        let observer2: Arc<dyn HiveObserver> = obs2_concrete.clone();

        manager.add(observer1.clone());
        manager.add(observer2.clone());

        assert_eq!(manager.count(), 2);

        manager.notify(HiveEvent::peer_connected(NodeId::new(0x12345678)));

        assert_eq!(obs1_concrete.count(), 1);
        assert_eq!(obs2_concrete.count(), 1);

        manager.remove(&observer1);
        assert_eq!(manager.count(), 1);

        manager.notify(HiveEvent::peer_lost(NodeId::new(0x12345678)));

        assert_eq!(obs1_concrete.count(), 1); // Not notified
        assert_eq!(obs2_concrete.count(), 2); // Got both events
    }
}
