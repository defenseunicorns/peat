//! Topology formation module
//!
//! This module provides beacon-driven topology formation capabilities,
//! including parent selection algorithms, topology state management, and
//! network partition detection and autonomous operation.

mod autonomous;
mod builder;
mod manager;
pub mod metrics;
mod partition;
mod selection;

pub use autonomous::{AutonomousOperationHandler, AutonomousState};
pub use builder::{SelectedPeer, TopologyBuilder, TopologyConfig, TopologyEvent, TopologyState};
pub use manager::TopologyManager;
pub use metrics::{
    InMemoryMetricsCollector, MetricsCollector, NoOpMetricsCollector, TopologyMetricsSnapshot,
};
pub use partition::{PartitionConfig, PartitionDetector, PartitionEvent, PartitionHandler};
pub use selection::{PeerCandidate, PeerSelector, SelectionConfig};
