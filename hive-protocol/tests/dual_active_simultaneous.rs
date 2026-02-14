//! Simultaneous Iroh + BLE transport proof
//!
//! Proves that a **real** IrohMeshTransport (with QUIC accept loop) runs
//! alongside a mock BLE transport in the same TransportManager. This is the
//! M4 dual-active integration proof: real async QUIC endpoint + mock BLE
//! coexist, route correctly, and PACE fallback works when Iroh stops.
//!
//! Previous tests proved:
//! - `dual_active_transport_e2e.rs`: all-mock routing logic
//! - `canned_message_sync.rs`: CannedMessage over encrypted BLE
//! - Pi-to-Pi functional test: real BLE sync (277 ms)
//!
//! This test adds: real QUIC transport lifecycle in the same manager as BLE.

#![cfg(feature = "automerge-backend")]

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::mpsc;

use hive_protocol::network::iroh_transport::IrohTransport;
use hive_protocol::network::peer_config::PeerConfig;
use hive_protocol::transport::iroh::IrohMeshTransport;
use hive_protocol::transport::{
    CollectionRouteConfig, CollectionRouteTable, CollectionTransportRoute, MeshConnection,
    MeshTransport, MessagePriority, MessageRequirements, NodeId, PeerEventReceiver, RouteDecision,
    Transport, TransportCapabilities, TransportInstance, TransportManager, TransportManagerConfig,
    TransportPolicy, TransportType,
};

// =============================================================================
// Mock BLE Transport (self-contained, Transport trait only)
// =============================================================================

struct MockBleTransport {
    caps: TransportCapabilities,
    reachable_peers: Vec<NodeId>,
}

impl MockBleTransport {
    fn new(peers: Vec<NodeId>) -> Self {
        Self {
            caps: TransportCapabilities::bluetooth_le(),
            reachable_peers: peers,
        }
    }
}

struct MockBleConnection {
    peer_id: NodeId,
    connected_at: Instant,
}

impl MeshConnection for MockBleConnection {
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
impl MeshTransport for MockBleTransport {
    async fn start(&self) -> hive_protocol::transport::Result<()> {
        Ok(())
    }
    async fn stop(&self) -> hive_protocol::transport::Result<()> {
        Ok(())
    }
    async fn connect(
        &self,
        peer_id: &NodeId,
    ) -> hive_protocol::transport::Result<Box<dyn MeshConnection>> {
        Ok(Box::new(MockBleConnection {
            peer_id: peer_id.clone(),
            connected_at: Instant::now(),
        }))
    }
    async fn disconnect(&self, _peer_id: &NodeId) -> hive_protocol::transport::Result<()> {
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

impl Transport for MockBleTransport {
    fn capabilities(&self) -> &TransportCapabilities {
        &self.caps
    }

    fn is_available(&self) -> bool {
        true
    }

    fn signal_quality(&self) -> Option<u8> {
        Some(80)
    }

    fn can_reach(&self, peer_id: &NodeId) -> bool {
        self.reachable_peers.contains(peer_id)
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Build the standard dual-active config: Iroh primary, BLE alternate,
/// with collection routes for documents (QUIC), canned_msgs (BLE),
/// beacons (PACE).
fn simultaneous_config() -> TransportManagerConfig {
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
        });

    TransportManagerConfig {
        default_policy: Some(policy),
        collection_routes: routes,
        ..Default::default()
    }
}

// =============================================================================
// Tests
// =============================================================================

/// Main proof: real Iroh QUIC + mock BLE both active simultaneously
/// in the same TransportManager with correct routing.
#[tokio::test]
async fn test_iroh_and_ble_simultaneously_active() {
    let peer = NodeId::new("peer-1".to_string());
    let config = simultaneous_config();
    let mut manager = TransportManager::new(config);

    // --- Real Iroh transport ---
    let iroh_transport = Arc::new(IrohTransport::new().await.unwrap());
    let iroh_mesh = Arc::new(IrohMeshTransport::new(
        Arc::clone(&iroh_transport),
        PeerConfig::empty(),
    ));
    // Register the peer so can_reach() returns true
    iroh_mesh.register_peer(peer.clone(), iroh_transport.endpoint_id());

    // --- Mock BLE transport ---
    let mock_ble = Arc::new(MockBleTransport::new(vec![peer.clone()]));

    // Register legacy transports (for Fixed routes)
    manager.register(Arc::clone(&iroh_mesh) as Arc<dyn Transport>);
    manager.register(Arc::clone(&mock_ble) as Arc<dyn Transport>);

    // Register PACE instances
    manager.register_instance(
        TransportInstance::new(
            "iroh-primary",
            TransportType::Quic,
            TransportCapabilities::quic(),
        ),
        Arc::clone(&iroh_mesh) as Arc<dyn Transport>,
    );
    manager.register_instance(
        TransportInstance::new(
            "ble-primary",
            TransportType::BluetoothLE,
            TransportCapabilities::bluetooth_le(),
        ),
        Arc::clone(&mock_ble) as Arc<dyn Transport>,
    );

    // Start real Iroh transport (spawns QUIC accept loop)
    iroh_mesh.start().await.unwrap();

    // ---- Assert both available simultaneously ----
    let available = manager.available_instance_ids();
    assert_eq!(
        available.len(),
        2,
        "Expected 2 available instances, got {:?}",
        available
    );
    assert!(available.contains("iroh-primary"));
    assert!(available.contains("ble-primary"));

    // Assert real QUIC accept loop is running
    assert!(
        iroh_transport.is_accept_loop_running(),
        "Iroh QUIC accept loop should be running"
    );

    // ---- Assert routing ----
    let reqs = MessageRequirements::default();

    // documents → Fixed QUIC
    assert_eq!(
        manager.route_collection("documents", &peer, &reqs),
        RouteDecision::Transport(TransportType::Quic),
    );

    // canned_msgs → Fixed BLE
    assert_eq!(
        manager.route_collection("canned_msgs", &peer, &reqs),
        RouteDecision::Transport(TransportType::BluetoothLE),
    );

    // beacons → PACE selects iroh-primary (primary)
    assert_eq!(
        manager.route_collection("beacons", &peer, &reqs),
        RouteDecision::TransportInstance("iroh-primary".to_string()),
    );

    // ---- Stop Iroh, verify PACE fallback ----
    iroh_mesh.stop().await.unwrap();
    assert!(
        !iroh_transport.is_accept_loop_running(),
        "Iroh accept loop should be stopped"
    );

    // Iroh is_available() returns false → PACE falls back to BLE
    assert_eq!(
        manager.route_collection("beacons", &peer, &reqs),
        RouteDecision::TransportInstance("ble-primary".to_string()),
        "PACE should fall back to BLE when Iroh is stopped"
    );

    // BLE is still available
    assert!(mock_ble.is_available());
}

/// Iroh lifecycle (start/stop/restart) does not interfere with BLE availability.
#[tokio::test]
async fn test_iroh_lifecycle_doesnt_affect_ble() {
    let peer = NodeId::new("peer-1".to_string());
    let config = simultaneous_config();
    let manager = TransportManager::new(config);

    // Real Iroh
    let iroh_transport = Arc::new(IrohTransport::new().await.unwrap());
    let iroh_mesh = Arc::new(IrohMeshTransport::new(
        Arc::clone(&iroh_transport),
        PeerConfig::empty(),
    ));
    iroh_mesh.register_peer(peer.clone(), iroh_transport.endpoint_id());

    // Mock BLE
    let mock_ble = Arc::new(MockBleTransport::new(vec![peer.clone()]));

    // Register PACE instances
    manager.register_instance(
        TransportInstance::new(
            "iroh-primary",
            TransportType::Quic,
            TransportCapabilities::quic(),
        ),
        Arc::clone(&iroh_mesh) as Arc<dyn Transport>,
    );
    manager.register_instance(
        TransportInstance::new(
            "ble-primary",
            TransportType::BluetoothLE,
            TransportCapabilities::bluetooth_le(),
        ),
        Arc::clone(&mock_ble) as Arc<dyn Transport>,
    );

    // Phase 1: Start Iroh → both available
    iroh_mesh.start().await.unwrap();
    assert_eq!(manager.available_instance_ids().len(), 2);
    assert!(mock_ble.is_available(), "BLE should be available");

    // Phase 2: Stop Iroh → only BLE available
    iroh_mesh.stop().await.unwrap();
    let available = manager.available_instance_ids();
    assert_eq!(
        available.len(),
        1,
        "Only BLE should be available after Iroh stops"
    );
    assert!(available.contains("ble-primary"));
    assert!(
        mock_ble.is_available(),
        "BLE must remain available after Iroh stops"
    );

    // Phase 3: Restart Iroh → both available again
    iroh_mesh.start().await.unwrap();
    assert_eq!(
        manager.available_instance_ids().len(),
        2,
        "Both should be available after Iroh restarts"
    );
    assert!(
        mock_ble.is_available(),
        "BLE still available after Iroh restarts"
    );

    // Cleanup
    iroh_mesh.stop().await.unwrap();
}

/// Routing decisions for multiple collections in rapid succession all resolve
/// to the correct transport — proves no interference between routes.
#[tokio::test]
async fn test_simultaneous_routing_decisions() {
    let peer = NodeId::new("peer-1".to_string());
    let config = simultaneous_config();
    let mut manager = TransportManager::new(config);

    // Real Iroh
    let iroh_transport = Arc::new(IrohTransport::new().await.unwrap());
    let iroh_mesh = Arc::new(IrohMeshTransport::new(
        Arc::clone(&iroh_transport),
        PeerConfig::empty(),
    ));
    iroh_mesh.register_peer(peer.clone(), iroh_transport.endpoint_id());

    // Mock BLE
    let mock_ble = Arc::new(MockBleTransport::new(vec![peer.clone()]));

    // Register legacy + PACE
    manager.register(Arc::clone(&iroh_mesh) as Arc<dyn Transport>);
    manager.register(Arc::clone(&mock_ble) as Arc<dyn Transport>);
    manager.register_instance(
        TransportInstance::new(
            "iroh-primary",
            TransportType::Quic,
            TransportCapabilities::quic(),
        ),
        Arc::clone(&iroh_mesh) as Arc<dyn Transport>,
    );
    manager.register_instance(
        TransportInstance::new(
            "ble-primary",
            TransportType::BluetoothLE,
            TransportCapabilities::bluetooth_le(),
        ),
        Arc::clone(&mock_ble) as Arc<dyn Transport>,
    );

    // Start Iroh
    iroh_mesh.start().await.unwrap();

    let reqs = MessageRequirements::default();

    // Rapid-fire routing decisions — each should go to the right transport
    let collections_and_expected = [
        ("documents", RouteDecision::Transport(TransportType::Quic)),
        (
            "canned_msgs",
            RouteDecision::Transport(TransportType::BluetoothLE),
        ),
        (
            "beacons",
            RouteDecision::TransportInstance("iroh-primary".to_string()),
        ),
        ("documents", RouteDecision::Transport(TransportType::Quic)),
        (
            "canned_msgs",
            RouteDecision::Transport(TransportType::BluetoothLE),
        ),
        (
            "beacons",
            RouteDecision::TransportInstance("iroh-primary".to_string()),
        ),
    ];

    for (collection, expected) in &collections_and_expected {
        let decision = manager.route_collection(collection, &peer, &reqs);
        assert_eq!(
            &decision, expected,
            "Collection '{}' routed to {:?}, expected {:?}",
            collection, decision, expected
        );
    }

    // Cleanup
    iroh_mesh.stop().await.unwrap();
}
