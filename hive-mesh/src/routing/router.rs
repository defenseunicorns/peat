//! Selective router implementation for hierarchical data routing
//!
//! This module implements the core routing logic that determines:
//! - Whether data should be consumed (processed) by this node
//! - Whether data should be forwarded to other nodes
//! - Which peer should receive forwarded data

use super::packet::{DataDirection, DataPacket};
use crate::beacon::HierarchyLevel;
use crate::hierarchy::NodeRole;
use crate::topology::TopologyState;
use tracing::{debug, trace, warn};

/// Routing decision result
#[derive(Debug, Clone, PartialEq)]
pub enum RoutingDecision {
    /// Consume (process) the data locally
    Consume,

    /// Forward the data to a specific peer
    Forward { next_hop: String },

    /// Consume locally AND forward to peer
    ConsumeAndForward { next_hop: String },

    /// Drop the packet (reached max hops or no route)
    Drop,
}

/// Selective router for hierarchical mesh networks
///
/// Makes intelligent routing decisions based on:
/// - Node's position in hierarchy (level and role)
/// - Data direction (upward/downward/lateral)
/// - Topology state (selected peer, linked peers, lateral peers)
///
/// # Example
///
/// ```ignore
/// use hive_mesh::routing::{SelectiveRouter, DataPacket};
/// use hive_mesh::topology::TopologyState;
///
/// let router = SelectiveRouter::new();
/// let state = get_topology_state();
/// let packet = DataPacket::telemetry("node-123", vec![1, 2, 3]);
///
/// // Check if we should consume this telemetry
/// if router.should_consume(&packet, &state) {
///     process_telemetry(&packet);
/// }
///
/// // Check if we should forward it upward
/// if router.should_forward(&packet, &state) {
///     if let Some(next) = router.next_hop(&packet, &state) {
///         send_to_peer(&next, &packet);
///     }
/// }
/// ```
pub struct SelectiveRouter {
    /// Enable verbose logging for debugging
    verbose: bool,
}

impl SelectiveRouter {
    /// Create a new selective router
    pub fn new() -> Self {
        Self { verbose: false }
    }

    /// Create a new selective router with verbose logging
    pub fn new_verbose() -> Self {
        Self { verbose: true }
    }

    /// Make a complete routing decision for a packet
    ///
    /// This is the primary entry point that combines should_consume,
    /// should_forward, and next_hop into a single decision.
    ///
    /// # Arguments
    ///
    /// * `packet` - The data packet to route
    /// * `state` - Current topology state
    /// * `this_node_id` - This node's identifier
    ///
    /// # Returns
    ///
    /// RoutingDecision indicating what to do with the packet
    pub fn route(
        &self,
        packet: &DataPacket,
        state: &TopologyState,
        this_node_id: &str,
    ) -> RoutingDecision {
        // Check if packet has reached max hops
        if packet.at_max_hops() {
            if self.verbose {
                warn!(
                    "Packet {} reached max hops ({}), dropping",
                    packet.packet_id, packet.max_hops
                );
            }
            return RoutingDecision::Drop;
        }

        // Check if we're the source (don't route our own packets back to us)
        if packet.source_node_id == this_node_id {
            if self.verbose {
                trace!(
                    "Packet {} originated from us, not routing",
                    packet.packet_id
                );
            }
            return RoutingDecision::Drop;
        }

        let should_consume = self.should_consume(packet, state, this_node_id);
        let should_forward = self.should_forward(packet, state);

        if should_consume && should_forward {
            // Both consume and forward
            if let Some(next_hop) = self.next_hop(packet, state) {
                if self.verbose {
                    debug!(
                        "Packet {}: Consume and forward to {}",
                        packet.packet_id, next_hop
                    );
                }
                RoutingDecision::ConsumeAndForward { next_hop }
            } else {
                // Can't forward without next hop, just consume
                if self.verbose {
                    debug!("Packet {}: Consume only (no next hop)", packet.packet_id);
                }
                RoutingDecision::Consume
            }
        } else if should_consume {
            if self.verbose {
                debug!("Packet {}: Consume only", packet.packet_id);
            }
            RoutingDecision::Consume
        } else if should_forward {
            if let Some(next_hop) = self.next_hop(packet, state) {
                if self.verbose {
                    debug!("Packet {}: Forward to {}", packet.packet_id, next_hop);
                }
                RoutingDecision::Forward { next_hop }
            } else {
                if self.verbose {
                    warn!(
                        "Packet {}: Should forward but no next hop, dropping",
                        packet.packet_id
                    );
                }
                RoutingDecision::Drop
            }
        } else {
            if self.verbose {
                debug!("Packet {}: Drop (not for us)", packet.packet_id);
            }
            RoutingDecision::Drop
        }
    }

    /// Determine if this node should consume (process) the packet
    ///
    /// # Consumption Rules
    ///
    /// **Upward (Telemetry)**
    /// - Always consume telemetry for local processing/aggregation
    ///
    /// **Downward (Commands)**
    /// - Consume if packet is addressed to us
    /// - Leaders consume commands for their squad
    ///
    /// **Lateral (Coordination)**
    /// - Leaders consume coordination messages
    /// - Members typically don't consume lateral messages
    ///
    /// # Arguments
    ///
    /// * `packet` - The data packet
    /// * `state` - Current topology state
    /// * `this_node_id` - This node's identifier
    ///
    /// # Returns
    ///
    /// `true` if this node should process the packet
    pub fn should_consume(
        &self,
        packet: &DataPacket,
        state: &TopologyState,
        this_node_id: &str,
    ) -> bool {
        match packet.direction {
            DataDirection::Upward => {
                // Upward data (telemetry, status): Always consume for aggregation
                // Every node in the path can aggregate/process
                true
            }

            DataDirection::Downward => {
                // Downward data (commands, config): Consume if targeted at us
                if let Some(ref dest) = packet.destination_node_id {
                    if dest == this_node_id {
                        return true;
                    }
                }

                // Leaders consume commands even if not directly targeted
                // (they may need to disseminate to squad members)
                matches!(state.role, NodeRole::Leader)
            }

            DataDirection::Lateral => {
                // Lateral data (coordination): Only Leaders typically consume
                if let Some(ref dest) = packet.destination_node_id {
                    // Consume only if directly addressed to us
                    dest == this_node_id
                } else {
                    // No specific destination (broadcast): Leaders consume
                    matches!(state.role, NodeRole::Leader)
                }
            }
        }
    }

    /// Determine if this node should forward the packet
    ///
    /// # Forwarding Rules
    ///
    /// **Upward (Telemetry)**
    /// - Forward if we have a selected peer (parent in hierarchy)
    /// - Don't forward if we're at HQ level (no parent)
    ///
    /// **Downward (Commands)**
    /// - Forward if we have linked peers (children) that need this data
    /// - Don't forward if we're a leaf node (no children)
    ///
    /// **Lateral (Coordination)**
    /// - Forward if addressed to a lateral peer we track
    /// - Leaders may forward to other Leaders at same level
    ///
    /// # Arguments
    ///
    /// * `packet` - The data packet
    /// * `state` - Current topology state
    ///
    /// # Returns
    ///
    /// `true` if packet should be forwarded to another peer
    pub fn should_forward(&self, packet: &DataPacket, state: &TopologyState) -> bool {
        match packet.direction {
            DataDirection::Upward => {
                // Forward upward if we have a selected peer (parent)
                state.selected_peer.is_some()
            }

            DataDirection::Downward => {
                // Forward downward if we have linked peers (children)
                !state.linked_peers.is_empty()
            }

            DataDirection::Lateral => {
                // Forward laterally if addressed to a peer we know
                if let Some(ref dest) = packet.destination_node_id {
                    state.lateral_peers.contains_key(dest)
                } else {
                    // Broadcast lateral messages if we're a Leader with lateral peers
                    matches!(state.role, NodeRole::Leader) && !state.lateral_peers.is_empty()
                }
            }
        }
    }

    /// Determine the next hop for forwarding the packet
    ///
    /// # Next Hop Selection
    ///
    /// **Upward**: selected_peer (parent in hierarchy)
    /// **Downward**: linked_peers (children) - for now, return first child
    /// **Lateral**: lateral_peers - specific peer if addressed, or first if broadcast
    ///
    /// # Arguments
    ///
    /// * `packet` - The data packet
    /// * `state` - Current topology state
    ///
    /// # Returns
    ///
    /// Node ID of the next hop, or None if no valid next hop
    pub fn next_hop(&self, packet: &DataPacket, state: &TopologyState) -> Option<String> {
        match packet.direction {
            DataDirection::Upward => {
                // Upward: Route to selected peer (parent)
                state
                    .selected_peer
                    .as_ref()
                    .map(|peer| peer.node_id.clone())
            }

            DataDirection::Downward => {
                // Downward: Route to linked peers (children)
                // If addressed to specific child, route there
                if let Some(ref dest) = packet.destination_node_id {
                    if state.linked_peers.contains_key(dest) {
                        return Some(dest.clone());
                    }
                }

                // Otherwise, route to all linked peers (broadcast)
                // For now, return first linked peer
                // TODO: In Week 10, implement multicast/broadcast forwarding
                state.linked_peers.keys().next().cloned()
            }

            DataDirection::Lateral => {
                // Lateral: Route to lateral peers
                if let Some(ref dest) = packet.destination_node_id {
                    // Route to specific lateral peer if we track them
                    if state.lateral_peers.contains_key(dest) {
                        return Some(dest.clone());
                    }
                }

                // Otherwise, route to first lateral peer (broadcast)
                state.lateral_peers.keys().next().cloned()
            }
        }
    }

    /// Check if this node is at the hierarchy level that should aggregate
    ///
    /// HQ nodes (Company level) should aggregate and consume
    /// without further forwarding.
    #[allow(dead_code)]
    fn is_hq_level(&self, level: HierarchyLevel) -> bool {
        matches!(level, HierarchyLevel::Company)
    }

    /// Check if a packet should be aggregated before forwarding
    ///
    /// Aggregation is appropriate when:
    /// - Packet data type requires aggregation (Telemetry, Status)
    /// - Routing decision is ConsumeAndForward (intermediate node)
    /// - Node is a Leader (squad leader aggregating member data)
    ///
    /// # Integration with PacketAggregator
    ///
    /// When this returns true, the application should:
    /// 1. Collect telemetry packets from squad members (batching)
    /// 2. Use PacketAggregator::aggregate_telemetry() to create aggregated packet
    /// 3. Route the aggregated packet upward using this router
    ///
    /// # Example
    ///
    /// ```ignore
    /// use hive_mesh::routing::{SelectiveRouter, PacketAggregator, DataPacket};
    ///
    /// let router = SelectiveRouter::new();
    /// let aggregator = PacketAggregator::new();
    ///
    /// // Collect telemetry from squad members
    /// let mut squad_telemetry = Vec::new();
    /// for packet in incoming_packets {
    ///     let decision = router.route(&packet, &state, "platoon-leader");
    ///     if router.should_aggregate(&packet, &decision, &state) {
    ///         squad_telemetry.push(packet);
    ///     }
    /// }
    ///
    /// // Aggregate when we have enough data
    /// if squad_telemetry.len() >= 3 {
    ///     let aggregated = aggregator.aggregate_telemetry(
    ///         "squad-1",
    ///         "platoon-leader",
    ///         squad_telemetry,
    ///     )?;
    ///
    ///     // Route aggregated packet upward
    ///     let decision = router.route(&aggregated, &state, "platoon-leader");
    ///     // ... forward to parent
    /// }
    /// ```
    ///
    /// # Arguments
    ///
    /// * `packet` - The data packet to check
    /// * `decision` - The routing decision for this packet
    /// * `state` - Current topology state
    ///
    /// # Returns
    ///
    /// `true` if this packet should be aggregated before forwarding
    pub fn should_aggregate(
        &self,
        packet: &DataPacket,
        decision: &RoutingDecision,
        state: &TopologyState,
    ) -> bool {
        // Only aggregate if we're consuming and forwarding (intermediate node)
        if !matches!(decision, RoutingDecision::ConsumeAndForward { .. }) {
            return false;
        }

        // Only aggregate data types that require it
        if !packet.data_type.requires_aggregation() {
            return false;
        }

        // Only Leaders aggregate squad member data
        matches!(state.role, NodeRole::Leader)
    }
}

impl Default for SelectiveRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{GeoPosition, GeographicBeacon};
    use crate::topology::SelectedPeer;
    use std::collections::HashMap;
    use std::time::Instant;

    fn create_test_state(
        hierarchy_level: HierarchyLevel,
        role: NodeRole,
        has_selected_peer: bool,
        num_linked_peers: usize,
        num_lateral_peers: usize,
    ) -> TopologyState {
        let selected_peer = if has_selected_peer {
            Some(SelectedPeer {
                node_id: "parent-node".to_string(),
                beacon: GeographicBeacon::new(
                    "parent-node".to_string(),
                    GeoPosition::new(37.7749, -122.4194),
                    HierarchyLevel::Platoon,
                ),
                selected_at: Instant::now(),
            })
        } else {
            None
        };

        let mut linked_peers = HashMap::new();
        for i in 0..num_linked_peers {
            linked_peers.insert(format!("linked-peer-{}", i), Instant::now());
        }

        let mut lateral_peers = HashMap::new();
        for i in 0..num_lateral_peers {
            lateral_peers.insert(format!("lateral-peer-{}", i), Instant::now());
        }

        TopologyState {
            selected_peer,
            linked_peers,
            lateral_peers,
            role,
            hierarchy_level,
        }
    }

    #[test]
    fn test_upward_telemetry_leaf_node() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        // Leaf node should consume telemetry
        assert!(router.should_consume(&packet, &state, "this-node"));

        // Leaf node with parent should forward
        assert!(router.should_forward(&packet, &state));

        // Next hop should be parent
        let next_hop = router.next_hop(&packet, &state);
        assert_eq!(next_hop, Some("parent-node".to_string()));
    }

    #[test]
    fn test_upward_telemetry_hq_node() {
        let router = SelectiveRouter::new();
        // HQ node (no selected peer = highest level)
        let state = create_test_state(HierarchyLevel::Company, NodeRole::Leader, false, 3, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        // HQ should consume telemetry
        assert!(router.should_consume(&packet, &state, "hq-node"));

        // HQ should NOT forward (no parent)
        assert!(!router.should_forward(&packet, &state));
    }

    #[test]
    fn test_downward_command_to_leader() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 3, 0);
        let packet = DataPacket::command("hq", "platoon-leader", vec![4, 5, 6]);

        // Leader should consume command addressed to them
        assert!(router.should_consume(&packet, &state, "platoon-leader"));

        // Leader with children should forward
        assert!(router.should_forward(&packet, &state));

        // Next hop should be one of the linked peers (children)
        let next_hop = router.next_hop(&packet, &state);
        assert!(next_hop.is_some());
        assert!(next_hop.unwrap().starts_with("linked-peer-"));
    }

    #[test]
    fn test_downward_command_to_leaf() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::command("hq", "squad-member", vec![4, 5, 6]);

        // Member should consume command addressed to them
        assert!(router.should_consume(&packet, &state, "squad-member"));

        // Leaf node should NOT forward (no children)
        assert!(!router.should_forward(&packet, &state));
    }

    #[test]
    fn test_lateral_coordination_between_leaders() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 2, 3);
        let packet = DataPacket::coordination("platoon-1", "lateral-peer-0", vec![7, 8, 9]);

        // Leader should NOT consume lateral coordination if not addressed to them
        assert!(!router.should_consume(&packet, &state, "platoon-3"));

        // Should forward if addressed to a lateral peer we track
        let state_with_target =
            create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 2, 3);
        assert!(router.should_forward(&packet, &state_with_target));
    }

    #[test]
    fn test_max_hops_drop() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let mut packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        // Increment hops to max
        for _ in 0..10 {
            packet.increment_hop();
        }

        // Routing should return Drop when at max hops
        let decision = router.route(&packet, &state, "this-node");
        assert_eq!(decision, RoutingDecision::Drop);
    }

    #[test]
    fn test_routing_decision_consume_and_forward() {
        let router = SelectiveRouter::new();
        // Intermediate node with parent and children
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 3, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        let decision = router.route(&packet, &state, "platoon-leader");

        // Should consume and forward
        match decision {
            RoutingDecision::ConsumeAndForward { next_hop } => {
                assert_eq!(next_hop, "parent-node");
            }
            _ => panic!("Expected ConsumeAndForward, got {:?}", decision),
        }
    }

    #[test]
    fn test_routing_decision_consume_only() {
        let router = SelectiveRouter::new();
        // HQ node (no parent)
        let state = create_test_state(HierarchyLevel::Company, NodeRole::Leader, false, 3, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        let decision = router.route(&packet, &state, "hq-node");

        // Should consume only (no forwarding)
        assert_eq!(decision, RoutingDecision::Consume);
    }

    #[test]
    fn test_dont_route_own_packets() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("this-node", vec![1, 2, 3]);

        // Should not route our own packets back to us
        let decision = router.route(&packet, &state, "this-node");
        assert_eq!(decision, RoutingDecision::Drop);
    }

    #[test]
    fn test_should_aggregate_intermediate_leader() {
        let router = SelectiveRouter::new();
        // Intermediate Leader node (has parent and children)
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 3, 0);
        let packet = DataPacket::telemetry("squad-member-1", vec![1, 2, 3]);

        let decision = router.route(&packet, &state, "platoon-leader");

        // Should aggregate: Leader with ConsumeAndForward decision
        assert!(router.should_aggregate(&packet, &decision, &state));
    }

    #[test]
    fn test_should_not_aggregate_hq_node() {
        let router = SelectiveRouter::new();
        // HQ node (no parent, just consumes)
        let state = create_test_state(HierarchyLevel::Company, NodeRole::Leader, false, 3, 0);
        let packet = DataPacket::telemetry("platoon-1", vec![1, 2, 3]);

        let decision = router.route(&packet, &state, "hq-node");

        // Should NOT aggregate: Decision is Consume only (not ConsumeAndForward)
        assert!(!router.should_aggregate(&packet, &decision, &state));
    }

    #[test]
    fn test_should_not_aggregate_non_leader() {
        let router = SelectiveRouter::new();
        // Member node (not a Leader)
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        let decision = router.route(&packet, &state, "squad-member");

        // Should NOT aggregate: Not a Leader
        assert!(!router.should_aggregate(&packet, &decision, &state));
    }

    #[test]
    fn test_should_not_aggregate_command_packet() {
        let router = SelectiveRouter::new();
        // Leader node
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 3, 0);
        let packet = DataPacket::command("hq", "platoon-leader", vec![4, 5, 6]);

        let decision = router.route(&packet, &state, "platoon-leader");

        // Should NOT aggregate: Command packets don't require aggregation
        assert!(!router.should_aggregate(&packet, &decision, &state));
    }
}
