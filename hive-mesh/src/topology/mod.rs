//! Topology formation module
//!
//! This module provides beacon-driven topology formation capabilities,
//! including parent selection algorithms and topology state management.

mod builder;
mod manager;
pub mod metrics;
mod selection;

pub use builder::{SelectedPeer, TopologyBuilder, TopologyConfig, TopologyEvent, TopologyState};
pub use manager::TopologyManager;
pub use metrics::{
    InMemoryMetricsCollector, MetricsCollector, NoOpMetricsCollector, TopologyMetricsSnapshot,
};
pub use selection::{PeerCandidate, PeerSelector, SelectionConfig};
