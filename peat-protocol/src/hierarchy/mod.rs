//! Hierarchical operations module (Phase 3)
//!
//! This module implements the hierarchical coordination layer for E5,
//! including zone management and hierarchical message routing.

pub mod aggregation_coordinator;
pub mod deltas;
pub mod flow_control;
pub mod maintenance;
pub mod router;
pub mod routing_table;
pub mod state_aggregation;
pub mod storage_trait;
pub mod zone_coordinator;

pub use aggregation_coordinator::HierarchicalAggregator;
pub use deltas::{
    current_timestamp_us, CompanyDelta, CompanyFieldUpdate, PlatoonDelta, PlatoonFieldUpdate,
    SquadDelta, SquadFieldUpdate,
};
pub use flow_control::{
    BandwidthLimit, CapacityInfo, FlowController, FlowMetrics, MessageDropPolicy, Permit,
    RoutingLevel,
};
pub use maintenance::{
    HierarchyMaintainer, MaintenanceMetrics, RebalanceAction, RebalancingCoordinator,
};
pub use router::{HierarchicalRouter, RouterStats};
pub use routing_table::{RoutingTable, RoutingTableStats};
pub use state_aggregation::StateAggregator;
pub use storage_trait::{DocumentMetrics, SummaryStorage};
pub use zone_coordinator::{ZoneCoordinator, ZoneFormationStatus, ZoneMetrics};
