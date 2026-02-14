//! Integration test: dual-active transport (Iroh + BLE concurrent)
//!
//! Validates the combined flow as used from FFI: build config with collection
//! routes + PACE policy, create manager, register both transport instances,
//! route collections, verify decisions.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::mpsc;

use hive_mesh::transport::{
    CollectionRouteConfig, CollectionRouteTable, CollectionTransportRoute, MeshConnection,
    MeshTransport, MessagePriority, MessageRequirements, NodeId, PeerEventReceiver, RouteDecision,
    Transport, TransportCapabilities, TransportInstance, TransportManager, TransportManagerConfig,
    TransportPolicy, TransportType,
};
use hive_mesh::transport::{Result, TransportError};

// =============================================================================
// Mock Transport (self-contained, matches manager.rs pattern)
// =============================================================================

struct MockTransport {
    caps: TransportCapabilities,
    available: bool,
    reachable_peers: Vec<NodeId>,
}

impl MockTransport {
    fn new(caps: TransportCapabilities) -> Self {
        Self {
            caps,
            available: true,
            reachable_peers: vec![],
        }
    }

    fn with_peer(mut self, peer: NodeId) -> Self {
        self.reachable_peers.push(peer);
        self
    }

    fn unavailable(mut self) -> Self {
        self.available = false;
        self
    }
}

struct MockConnection {
    peer_id: NodeId,
    connected_at: Instant,
}

impl MeshConnection for MockConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        true
    }

    fn connected_at(&self) -> Instant {
        self.connected_at
    }
}

#[async_trait]
impl MeshTransport for MockTransport {
    async fn start(&self) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>> {
        if self.reachable_peers.contains(peer_id) {
            Ok(Box::new(MockConnection {
                peer_id: peer_id.clone(),
                connected_at: Instant::now(),
            }))
        } else {
            Err(TransportError::PeerNotFound(peer_id.to_string()))
        }
    }

    async fn disconnect(&self, _peer_id: &NodeId) -> Result<()> {
        Ok(())
    }

    fn get_connection(&self, _peer_id: &NodeId) -> Option<Box<dyn MeshConnection>> {
        None
    }

    fn peer_count(&self) -> usize {
        0
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        vec![]
    }

    fn subscribe_peer_events(&self) -> PeerEventReceiver {
        let (_tx, rx) = mpsc::channel(1);
        rx
    }
}

impl Transport for MockTransport {
    fn capabilities(&self) -> &TransportCapabilities {
        &self.caps
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn signal_quality(&self) -> Option<u8> {
        None
    }

    fn can_reach(&self, peer_id: &NodeId) -> bool {
        self.reachable_peers.contains(peer_id)
    }
}

// =============================================================================
// Helper: build a standard dual-active config
// =============================================================================

fn dual_active_config() -> TransportManagerConfig {
    let policy = TransportPolicy::new("tactical")
        .primary(vec!["iroh-primary"])
        .alternate(vec!["ble-primary"]);

    let routes = CollectionRouteTable::new()
        .with_collection(CollectionRouteConfig {
            collection: "documents".to_string(),
            route: CollectionTransportRoute::Fixed {
                transport_type: TransportType::Quic,
            },
            priority: MessagePriority::High,
        })
        .with_collection(CollectionRouteConfig {
            collection: "canned_msgs".to_string(),
            route: CollectionTransportRoute::Fixed {
                transport_type: TransportType::BluetoothLE,
            },
            priority: MessagePriority::Normal,
        })
        .with_collection(CollectionRouteConfig {
            collection: "beacons".to_string(),
            route: CollectionTransportRoute::Pace {
                policy_override: None,
            },
            priority: MessagePriority::Normal,
        })
        .with_collection(CollectionRouteConfig {
            collection: "telemetry".to_string(),
            route: CollectionTransportRoute::Pace {
                policy_override: None,
            },
            priority: MessagePriority::Background,
        });

    TransportManagerConfig {
        default_policy: Some(policy),
        collection_routes: routes,
        ..Default::default()
    }
}

fn default_requirements() -> MessageRequirements {
    MessageRequirements::default()
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn test_dual_active_setup_from_ffi_config() {
    let peer = NodeId::new("peer-1".to_string());
    let config = dual_active_config();
    let mut manager = TransportManager::new(config);

    // Register legacy transports (used by Fixed routes)
    let quic = Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
    let ble =
        Arc::new(MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()));
    manager.register(quic);
    manager.register(ble);

    // Register PACE instances
    let iroh_instance = TransportInstance::new(
        "iroh-primary",
        TransportType::Quic,
        TransportCapabilities::quic(),
    );
    let ble_instance = TransportInstance::new(
        "ble-primary",
        TransportType::BluetoothLE,
        TransportCapabilities::bluetooth_le(),
    );
    let iroh_transport: Arc<dyn Transport> =
        Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
    let ble_transport: Arc<dyn Transport> =
        Arc::new(MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()));
    manager.register_instance(iroh_instance, iroh_transport);
    manager.register_instance(ble_instance, ble_transport);

    // Verify both legacy transports registered
    assert!(manager.get_transport(TransportType::Quic).is_some());
    assert!(manager.get_transport(TransportType::BluetoothLE).is_some());

    // Verify both PACE instances registered
    let ids = manager.registered_instance_ids();
    assert!(ids.contains(&"iroh-primary".to_string()));
    assert!(ids.contains(&"ble-primary".to_string()));
    assert_eq!(ids.len(), 2);
}

#[test]
fn test_collection_routing_fixed_to_quic() {
    let peer = NodeId::new("peer-1".to_string());
    let config = dual_active_config();
    let mut manager = TransportManager::new(config);

    let quic = Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
    manager.register(quic);

    let decision = manager.route_collection("documents", &peer, &default_requirements());
    assert_eq!(decision, RouteDecision::Transport(TransportType::Quic));
}

#[test]
fn test_collection_routing_fixed_to_ble() {
    let peer = NodeId::new("peer-1".to_string());
    let config = dual_active_config();
    let mut manager = TransportManager::new(config);

    let ble =
        Arc::new(MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()));
    manager.register(ble);

    let decision = manager.route_collection("canned_msgs", &peer, &default_requirements());
    assert_eq!(
        decision,
        RouteDecision::Transport(TransportType::BluetoothLE)
    );
}

#[test]
fn test_collection_routing_pace_selects_primary() {
    let peer = NodeId::new("peer-1".to_string());
    let config = dual_active_config();
    let manager = TransportManager::new(config);

    // Register both PACE instances — both available and can reach peer
    let iroh_transport: Arc<dyn Transport> =
        Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
    let ble_transport: Arc<dyn Transport> =
        Arc::new(MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()));

    manager.register_instance(
        TransportInstance::new(
            "iroh-primary",
            TransportType::Quic,
            TransportCapabilities::quic(),
        ),
        iroh_transport,
    );
    manager.register_instance(
        TransportInstance::new(
            "ble-primary",
            TransportType::BluetoothLE,
            TransportCapabilities::bluetooth_le(),
        ),
        ble_transport,
    );

    let decision = manager.route_collection("beacons", &peer, &default_requirements());
    assert_eq!(
        decision,
        RouteDecision::TransportInstance("iroh-primary".to_string())
    );
}

#[test]
fn test_collection_routing_pace_falls_back_to_alternate() {
    let peer = NodeId::new("peer-1".to_string());
    let config = dual_active_config();
    let manager = TransportManager::new(config);

    // QUIC instance unavailable
    let iroh_transport: Arc<dyn Transport> = Arc::new(
        MockTransport::new(TransportCapabilities::quic())
            .with_peer(peer.clone())
            .unavailable(),
    );
    // BLE instance available
    let ble_transport: Arc<dyn Transport> =
        Arc::new(MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()));

    manager.register_instance(
        TransportInstance::new(
            "iroh-primary",
            TransportType::Quic,
            TransportCapabilities::quic(),
        ),
        iroh_transport,
    );
    manager.register_instance(
        TransportInstance::new(
            "ble-primary",
            TransportType::BluetoothLE,
            TransportCapabilities::bluetooth_le(),
        ),
        ble_transport,
    );

    let decision = manager.route_collection("beacons", &peer, &default_requirements());
    assert_eq!(
        decision,
        RouteDecision::TransportInstance("ble-primary".to_string())
    );
}

#[test]
fn test_unlisted_collection_falls_through_to_legacy() {
    let peer = NodeId::new("peer-1".to_string());
    let config = dual_active_config();
    let mut manager = TransportManager::new(config);

    // Register a QUIC transport so legacy scoring can find it
    let quic = Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
    manager.register(quic);

    // "unknown_collection" is not in the route table — falls through to route_message()
    let decision = manager.route_collection("unknown_collection", &peer, &default_requirements());
    // Legacy scoring should select QUIC (the only registered transport)
    assert_eq!(decision, RouteDecision::Transport(TransportType::Quic));
}
