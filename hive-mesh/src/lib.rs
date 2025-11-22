pub mod beacon;
pub mod topology;

// Re-export main types
pub use beacon::{
    BeaconBroadcaster, BeaconJanitor, BeaconObserver, GeoPosition, GeographicBeacon, HierarchyLevel,
};
pub use topology::{
    ParentCandidate, ParentInfo, ParentSelector, SelectionConfig, TopologyBuilder, TopologyConfig,
    TopologyEvent, TopologyState,
};
