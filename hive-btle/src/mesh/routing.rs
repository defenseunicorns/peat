//! Message routing for HIVE-BTLE mesh
//!
//! Handles routing messages through the mesh topology, including:
//! - Upward routing to parent
//! - Downward routing to children
//! - Broadcast to all connected peers

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

use crate::{HierarchyLevel, NodeId};

use super::topology::MeshTopology;

/// Message routing direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteDirection {
    /// Route upward toward the root (parent direction)
    Upward,
    /// Route downward toward leaves (children direction)
    Downward,
    /// Route to all connected peers
    Broadcast,
    /// Route to a specific node
    Targeted(u32), // NodeId as u32 for Copy
}

/// A routing decision for a message
#[derive(Debug, Clone)]
pub struct RouteDecision {
    /// Next hop(s) for the message
    pub next_hops: Vec<NodeId>,
    /// Whether to keep a local copy
    pub local_copy: bool,
    /// Whether routing succeeded
    pub routed: bool,
    /// Reason if routing failed
    pub failure_reason: Option<RouteFailure>,
}

impl RouteDecision {
    /// Create a successful route decision
    pub fn success(next_hops: Vec<NodeId>, local_copy: bool) -> Self {
        Self {
            next_hops,
            local_copy,
            routed: true,
            failure_reason: None,
        }
    }

    /// Create a failed route decision
    pub fn failed(reason: RouteFailure) -> Self {
        Self {
            next_hops: Vec::new(),
            local_copy: false,
            routed: false,
            failure_reason: Some(reason),
        }
    }

    /// Route for local processing only
    pub fn local_only() -> Self {
        Self {
            next_hops: Vec::new(),
            local_copy: true,
            routed: true,
            failure_reason: None,
        }
    }
}

/// Reason for routing failure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteFailure {
    /// No parent available for upward routing
    NoParent,
    /// No children for downward routing
    NoChildren,
    /// Target node not found in topology
    TargetNotFound,
    /// No peers connected
    NoPeers,
    /// TTL expired
    TtlExpired,
    /// Message too large
    MessageTooLarge,
}

/// Router for mesh messages
///
/// Determines where to send messages based on the current topology.
#[derive(Debug, Clone)]
pub struct MeshRouter {
    /// Our node ID
    pub node_id: NodeId,
    /// Our hierarchy level
    pub my_level: HierarchyLevel,
}

impl MeshRouter {
    /// Create a new router
    pub fn new(node_id: NodeId, my_level: HierarchyLevel) -> Self {
        Self { node_id, my_level }
    }

    /// Route a message based on direction
    pub fn route(&self, direction: RouteDirection, topology: &MeshTopology) -> RouteDecision {
        match direction {
            RouteDirection::Upward => self.route_upward(topology),
            RouteDirection::Downward => self.route_downward(topology),
            RouteDirection::Broadcast => self.route_broadcast(topology),
            RouteDirection::Targeted(target_id) => {
                self.route_targeted(&NodeId::new(target_id), topology)
            }
        }
    }

    /// Route upward to parent
    fn route_upward(&self, topology: &MeshTopology) -> RouteDecision {
        match &topology.parent {
            Some(parent_id) => RouteDecision::success(vec![*parent_id], true),
            None => RouteDecision::failed(RouteFailure::NoParent),
        }
    }

    /// Route downward to children
    fn route_downward(&self, topology: &MeshTopology) -> RouteDecision {
        if topology.children.is_empty() {
            RouteDecision::failed(RouteFailure::NoChildren)
        } else {
            RouteDecision::success(topology.children.clone(), true)
        }
    }

    /// Route to all connected peers (broadcast)
    fn route_broadcast(&self, topology: &MeshTopology) -> RouteDecision {
        let all = topology.all_connected();
        if all.is_empty() {
            RouteDecision::failed(RouteFailure::NoPeers)
        } else {
            RouteDecision::success(all, true)
        }
    }

    /// Route to a specific target node
    fn route_targeted(&self, target: &NodeId, topology: &MeshTopology) -> RouteDecision {
        // Check if target is directly connected
        if topology.is_connected(target) {
            return RouteDecision::success(vec![*target], false);
        }

        // Not directly connected - need to route through topology
        // For now, use simple strategy:
        // - If we have a parent, route upward (parent has broader view)
        // - Otherwise, route to all children (flood downward)

        if let Some(ref parent) = topology.parent {
            // Route to parent, it will figure out where to send
            RouteDecision::success(vec![*parent], false)
        } else if !topology.children.is_empty() {
            // Flood to all children
            RouteDecision::success(topology.children.clone(), false)
        } else {
            RouteDecision::failed(RouteFailure::TargetNotFound)
        }
    }

    /// Determine routing for a received message
    ///
    /// Given the source and destination, decide what to do with a message.
    pub fn handle_received(
        &self,
        source: &NodeId,
        destination: Option<&NodeId>,
        direction: RouteDirection,
        topology: &MeshTopology,
    ) -> RouteDecision {
        // Check if we are the destination
        if let Some(dest) = destination {
            if dest == &self.node_id {
                return RouteDecision::local_only();
            }
        }

        // Determine role of source (may be useful for advanced routing)
        let _source_role = topology.get_role(source);

        match direction {
            RouteDirection::Upward => {
                // Message going upward - forward to parent
                self.route_upward(topology)
            }
            RouteDirection::Downward => {
                // Message going downward - forward to children
                // Exclude the source if it's one of our children
                let mut children = topology.children.clone();
                children.retain(|c| c != source);
                if children.is_empty() {
                    RouteDecision::local_only()
                } else {
                    RouteDecision::success(children, true)
                }
            }
            RouteDirection::Broadcast => {
                // Broadcast - forward to all except source
                let mut all = topology.all_connected();
                all.retain(|n| n != source);
                RouteDecision::success(all, true)
            }
            RouteDirection::Targeted(target_id) => {
                let target = NodeId::new(target_id);
                if target == self.node_id {
                    RouteDecision::local_only()
                } else {
                    self.route_targeted(&target, topology)
                }
            }
        }
    }

    /// Get the best route for aggregation (data flowing upward)
    ///
    /// For HIVE-Lite nodes, this is always the parent.
    pub fn aggregation_route(&self, topology: &MeshTopology) -> Option<NodeId> {
        topology.parent
    }

    /// Get routes for dissemination (data flowing downward)
    ///
    /// Returns all children that should receive the data.
    pub fn dissemination_routes(&self, topology: &MeshTopology) -> Vec<NodeId> {
        topology.children.clone()
    }
}

/// Message hop tracking for loop prevention
#[derive(Debug, Clone)]
pub struct HopTracker {
    /// Maximum allowed hops
    pub max_hops: u8,
    /// Nodes this message has visited
    pub visited: Vec<NodeId>,
}

impl HopTracker {
    /// Create a new hop tracker
    pub fn new(max_hops: u8) -> Self {
        Self {
            max_hops,
            visited: Vec::new(),
        }
    }

    /// Record visiting a node
    pub fn visit(&mut self, node_id: NodeId) -> bool {
        if self.visited.contains(&node_id) {
            return false; // Loop detected
        }
        if self.visited.len() >= self.max_hops as usize {
            return false; // TTL expired
        }
        self.visited.push(node_id);
        true
    }

    /// Check if we've visited a node
    pub fn has_visited(&self, node_id: &NodeId) -> bool {
        self.visited.contains(node_id)
    }

    /// Get remaining hops
    pub fn remaining_hops(&self) -> u8 {
        self.max_hops.saturating_sub(self.visited.len() as u8)
    }

    /// Check if TTL is expired
    pub fn is_expired(&self) -> bool {
        self.visited.len() >= self.max_hops as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_topology_with_parent() -> MeshTopology {
        let mut topology = MeshTopology::new(HierarchyLevel::Platform, 3, 7);
        topology.set_parent(NodeId::new(0x1000));
        topology.add_child(NodeId::new(0x0001));
        topology.add_child(NodeId::new(0x0002));
        topology
    }

    #[test]
    fn test_route_upward() {
        let router = MeshRouter::new(NodeId::new(0x1234), HierarchyLevel::Platform);
        let topology = create_topology_with_parent();

        let decision = router.route(RouteDirection::Upward, &topology);
        assert!(decision.routed);
        assert_eq!(decision.next_hops.len(), 1);
        assert_eq!(decision.next_hops[0].as_u32(), 0x1000);
    }

    #[test]
    fn test_route_upward_no_parent() {
        let router = MeshRouter::new(NodeId::new(0x1234), HierarchyLevel::Platform);
        let topology = MeshTopology::new(HierarchyLevel::Platform, 3, 7);

        let decision = router.route(RouteDirection::Upward, &topology);
        assert!(!decision.routed);
        assert_eq!(decision.failure_reason, Some(RouteFailure::NoParent));
    }

    #[test]
    fn test_route_downward() {
        let router = MeshRouter::new(NodeId::new(0x1234), HierarchyLevel::Squad);
        let topology = create_topology_with_parent();

        let decision = router.route(RouteDirection::Downward, &topology);
        assert!(decision.routed);
        assert_eq!(decision.next_hops.len(), 2);
    }

    #[test]
    fn test_route_broadcast() {
        let router = MeshRouter::new(NodeId::new(0x1234), HierarchyLevel::Platform);
        let topology = create_topology_with_parent();

        let decision = router.route(RouteDirection::Broadcast, &topology);
        assert!(decision.routed);
        assert_eq!(decision.next_hops.len(), 3); // Parent + 2 children
    }

    #[test]
    fn test_route_targeted_direct() {
        let router = MeshRouter::new(NodeId::new(0x1234), HierarchyLevel::Platform);
        let topology = create_topology_with_parent();

        // Target is a child (directly connected)
        let decision = router.route(RouteDirection::Targeted(0x0001), &topology);
        assert!(decision.routed);
        assert_eq!(decision.next_hops.len(), 1);
        assert_eq!(decision.next_hops[0].as_u32(), 0x0001);
    }

    #[test]
    fn test_route_targeted_via_parent() {
        let router = MeshRouter::new(NodeId::new(0x1234), HierarchyLevel::Platform);
        let topology = create_topology_with_parent();

        // Target is not directly connected - should route to parent
        let decision = router.route(RouteDirection::Targeted(0x9999), &topology);
        assert!(decision.routed);
        assert_eq!(decision.next_hops.len(), 1);
        assert_eq!(decision.next_hops[0].as_u32(), 0x1000); // Parent
    }

    #[test]
    fn test_handle_received_for_us() {
        let router = MeshRouter::new(NodeId::new(0x1234), HierarchyLevel::Platform);
        let topology = create_topology_with_parent();

        let decision = router.handle_received(
            &NodeId::new(0x1000),
            Some(&NodeId::new(0x1234)), // Destination is us
            RouteDirection::Downward,
            &topology,
        );

        assert!(decision.local_copy);
        assert!(decision.next_hops.is_empty());
    }

    #[test]
    fn test_hop_tracker() {
        let mut tracker = HopTracker::new(3);

        assert!(tracker.visit(NodeId::new(0x0001)));
        assert!(tracker.visit(NodeId::new(0x0002)));
        assert!(tracker.visit(NodeId::new(0x0003)));
        assert!(!tracker.visit(NodeId::new(0x0004))); // TTL expired

        assert!(tracker.has_visited(&NodeId::new(0x0001)));
        assert!(!tracker.has_visited(&NodeId::new(0x0004)));
    }

    #[test]
    fn test_hop_tracker_loop_detection() {
        let mut tracker = HopTracker::new(10);

        assert!(tracker.visit(NodeId::new(0x0001)));
        assert!(tracker.visit(NodeId::new(0x0002)));
        assert!(!tracker.visit(NodeId::new(0x0001))); // Loop detected
    }

    #[test]
    fn test_aggregation_route() {
        let router = MeshRouter::new(NodeId::new(0x1234), HierarchyLevel::Platform);
        let topology = create_topology_with_parent();

        let route = router.aggregation_route(&topology);
        assert_eq!(route, Some(NodeId::new(0x1000)));
    }

    #[test]
    fn test_dissemination_routes() {
        let router = MeshRouter::new(NodeId::new(0x1234), HierarchyLevel::Squad);
        let topology = create_topology_with_parent();

        let routes = router.dissemination_routes(&topology);
        assert_eq!(routes.len(), 2);
    }
}
