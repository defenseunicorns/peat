#[cfg(feature = "broker")]
pub mod broker;

pub mod beacon;
pub mod config;
pub mod discovery;
pub mod flat_mesh;
pub mod hierarchy;
pub mod mesh;
pub mod qos;
pub mod routing;
pub mod security;
pub mod storage;
pub mod sync;
pub mod topology;
pub mod transport;

// Re-export main types
pub use beacon::{
    BeaconBroadcaster, BeaconJanitor, BeaconObserver, GeoPosition, GeographicBeacon, HierarchyLevel,
};
pub use flat_mesh::FlatMeshCoordinator;
pub use hierarchy::{
    DynamicHierarchyStrategy, ElectionConfig, ElectionWeights, HierarchyStrategy,
    HybridHierarchyStrategy, NodeRole, StaticHierarchyStrategy,
};
pub use routing::{
    AggregationError, Aggregator, DataDirection, DataPacket, DataType, DeduplicationConfig,
    MeshRouter, NoOpAggregator, RoutingDecision, SelectiveRouter,
};
pub use topology::{
    AutonomousOperationHandler, AutonomousState, InMemoryMetricsCollector, MetricsCollector,
    NoOpMetricsCollector, PartitionConfig, PartitionDetector, PartitionEvent, PartitionHandler,
    PeerCandidate, PeerSelector, SelectedPeer, SelectionConfig, TopologyBuilder, TopologyConfig,
    TopologyEvent, TopologyMetricsSnapshot, TopologyState,
};
pub use transport::{
    ConnectionHealth, ConnectionState, DisconnectReason, MeshConnection, MeshTransport, NodeId,
    PeerEvent, PeerEventReceiver, TransportError,
};

// Phase 7 facade re-exports
pub use config::{MeshConfig, MeshDiscoveryConfig, SecurityConfig};
pub use mesh::{HiveMesh, HiveMeshBuilder, HiveMeshEvent, MeshError, MeshState, MeshStatus};
