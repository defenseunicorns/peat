pub mod beacon;
pub mod hierarchy;
pub mod topology;

// Re-export main types
pub use beacon::{
    BeaconBroadcaster, BeaconJanitor, BeaconObserver, GeoPosition, GeographicBeacon, HierarchyLevel,
};
pub use hierarchy::{
    DynamicHierarchyStrategy, ElectionConfig, ElectionWeights, HierarchyStrategy,
    HybridHierarchyStrategy, NodeRole, StaticHierarchyStrategy,
};
pub use topology::{
    PeerCandidate, PeerSelector, SelectedPeer, SelectionConfig, TopologyBuilder, TopologyConfig,
    TopologyEvent, TopologyState,
};
