pub mod beacon;
pub mod flat_mesh;
pub mod hierarchy;
pub mod routing;
pub mod topology;

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
    AggregationError, DataDirection, DataPacket, DataType, PacketAggregator, RoutingDecision,
    SelectiveRouter, TelemetryPayload,
};
pub use topology::{
    AutonomousOperationHandler, AutonomousState, InMemoryMetricsCollector, MetricsCollector,
    NoOpMetricsCollector, PartitionConfig, PartitionDetector, PartitionEvent, PartitionHandler,
    PeerCandidate, PeerSelector, SelectedPeer, SelectionConfig, TopologyBuilder, TopologyConfig,
    TopologyEvent, TopologyMetricsSnapshot, TopologyState,
};
