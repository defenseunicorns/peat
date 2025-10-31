//! Squad formation phase implementation (Phase 2)
//!
//! Implements intra-squad communication, leader election, and capability aggregation.

pub mod aggregation;
pub mod capability_aggregation;
pub mod coordinator;
pub mod election_policy;
pub mod leader_election;
pub mod messaging;

// Re-exports
pub use capability_aggregation::{AggregatedCapability, CapabilityAggregator};
pub use coordinator::{FormationStatus, SquadCoordinator};
pub use election_policy::{ElectionContext, ElectionPolicyConfig, LeadershipPolicy};
