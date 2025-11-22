//! Flexible hierarchy and role management for mesh topology
//!
//! This module provides pluggable hierarchy strategies that enable both
//! static (organizational) and dynamic (capability-based) role assignment.

mod dynamic_strategy;
mod hybrid_strategy;
mod static_strategy;

pub use dynamic_strategy::{DynamicHierarchyStrategy, ElectionConfig, ElectionWeights};
pub use hybrid_strategy::HybridHierarchyStrategy;
pub use static_strategy::StaticHierarchyStrategy;

use crate::beacon::{GeographicBeacon, HierarchyLevel, NodeProfile};

/// Node role within its hierarchy level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeRole {
    /// Leader: Coordinates peers at same level
    Leader,
    /// Member: Reports to leader at same level
    Member,
    /// Standalone: No same-level coordination
    #[default]
    Standalone,
}

/// Hierarchy strategy trait for pluggable role/level assignment
///
/// Integrators implement this trait to define how nodes determine their
/// hierarchy level and role within the mesh. The protocol provides built-in
/// strategies for common use cases:
///
/// - **StaticHierarchyStrategy**: Fixed assignment from configuration
/// - **DynamicHierarchyStrategy**: Capability-based election
/// - **HybridHierarchyStrategy**: Static baseline with dynamic transitions
///
/// # Example
///
/// ```ignore
/// use hive_mesh::hierarchy::{HierarchyStrategy, StaticHierarchyStrategy, NodeRole};
///
/// let strategy = StaticHierarchyStrategy {
///     assigned_level: HierarchyLevel::Squad,
///     assigned_role: NodeRole::Leader,
/// };
///
/// let level = strategy.determine_level(&node_profile);
/// let role = strategy.determine_role(&node_profile, &nearby_beacons);
/// ```
pub trait HierarchyStrategy: Send + Sync + std::fmt::Debug {
    /// Determine this node's hierarchy level
    ///
    /// # Arguments
    ///
    /// * `node_profile` - This node's capabilities and configuration
    ///
    /// # Returns
    ///
    /// The hierarchy level this node should operate at
    fn determine_level(&self, node_profile: &NodeProfile) -> HierarchyLevel;

    /// Determine this node's role within its level
    ///
    /// # Arguments
    ///
    /// * `node_profile` - This node's capabilities and configuration
    /// * `nearby_peers` - Nearby beacons from peer discovery
    ///
    /// # Returns
    ///
    /// The role this node should assume (Leader, Member, or Standalone)
    fn determine_role(
        &self,
        node_profile: &NodeProfile,
        nearby_peers: &[GeographicBeacon],
    ) -> NodeRole;

    /// Check if this node can transition to a different level
    ///
    /// # Arguments
    ///
    /// * `current_level` - Current hierarchy level
    /// * `new_level` - Proposed new hierarchy level
    ///
    /// # Returns
    ///
    /// `true` if transition is allowed, `false` otherwise
    fn can_transition(&self, current_level: HierarchyLevel, new_level: HierarchyLevel) -> bool;
}
