//! Cell formation module (Phase 2)
//!
//! Implements cell-level coordination, leader election, and capability aggregation.

pub mod aggregation;
pub mod capability_aggregation;
pub mod coordinator;
pub mod election_policy;
pub mod leader_election;
pub mod messaging;

// Re-exports
pub use capability_aggregation::{AggregatedCapability, CapabilityAggregator};
pub use coordinator::{CellCoordinator, FormationStatus};
pub use leader_election::{LeaderElectionManager, LeadershipScore};
pub use messaging::{CellMessage, CellMessageBus, MessagePriority, RoutingContext};
