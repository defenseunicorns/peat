//! Hierarchical operations module (Phase 3)
//!
//! This module implements the hierarchical coordination layer for E5,
//! including zone management and hierarchical message routing.

pub mod flow_control;
pub mod router;
pub mod routing_cache;
pub mod routing_table;
pub mod zone_coordinator;

pub use flow_control::{
    BandwidthLimit, CapacityInfo, FlowController, FlowMetrics, MessageDropPolicy, Permit,
    RoutingLevel,
};
pub use router::{HierarchicalRouter, RouterStats};
pub use routing_cache::{CacheStats, RoutingCache};
pub use routing_table::{RoutingTable, RoutingTableStats};
pub use zone_coordinator::{ZoneCoordinator, ZoneFormationStatus, ZoneMetrics};
