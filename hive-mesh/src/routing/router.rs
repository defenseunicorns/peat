//! Selective router implementation for hierarchical data routing
//!
//! This module implements the core routing logic that determines:
//! - Whether data should be consumed (processed) by this node
//! - Whether data should be forwarded to other nodes
//! - Which peer should receive forwarded data
//!
//! ## Message Deduplication
//!
//! The router includes optional message deduplication to prevent routing loops.
//! When enabled, each packet's ID is tracked and duplicate packets are automatically
//! dropped. The deduplication cache uses a time-based eviction strategy.

use super::packet::{DataDirection, DataPacket};
use crate::beacon::HierarchyLevel;
use crate::hierarchy::NodeRole;
use crate::topology::TopologyState;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
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

    /// Forward the data to multiple peers (multicast/broadcast)
    ForwardMulticast { next_hops: Vec<String> },

    /// Consume locally AND forward to multiple peers (multicast/broadcast)
    ConsumeAndForwardMulticast { next_hops: Vec<String> },

    /// Drop the packet (reached max hops or no route)
    Drop,
}

/// Configuration for message deduplication
#[derive(Debug, Clone)]
pub struct DeduplicationConfig {
    /// Whether deduplication is enabled
    pub enabled: bool,
    /// How long to remember seen packet IDs (default: 5 minutes)
    pub ttl: Duration,
    /// Maximum number of packet IDs to track (default: 10000)
    pub max_entries: usize,
}

impl Default for DeduplicationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl: Duration::from_secs(300), // 5 minutes
            max_entries: 10000,
        }
    }
}

/// Entry in the deduplication cache
#[derive(Debug, Clone)]
struct DeduplicationEntry {
    /// When this packet was first seen
    first_seen: Instant,
}

/// Selective router for hierarchical mesh networks
///
/// Makes intelligent routing decisions based on:
/// - Node's position in hierarchy (level and role)
/// - Data direction (upward/downward/lateral)
/// - Topology state (selected peer, linked peers, lateral peers)
///
/// # Message Deduplication
///
/// The router can optionally track seen packet IDs to prevent routing loops.
/// Use `new_with_deduplication()` to enable this feature.
///
/// # Example
///
/// ```ignore
/// use hive_mesh::routing::{SelectiveRouter, DataPacket, DeduplicationConfig};
/// use hive_mesh::topology::TopologyState;
///
/// // Create router with deduplication enabled
/// let router = SelectiveRouter::new_with_deduplication(DeduplicationConfig::default());
/// let state = get_topology_state();
/// let packet = DataPacket::telemetry("node-123", vec![1, 2, 3]);
///
/// // Route will automatically deduplicate
/// let decision = router.route(&packet, &state, "this-node");
///
/// // Second call with same packet returns Drop (duplicate)
/// let decision2 = router.route(&packet, &state, "this-node");
/// assert_eq!(decision2, RoutingDecision::Drop);
/// ```
pub struct SelectiveRouter {
    /// Enable verbose logging for debugging
    verbose: bool,
    /// Deduplication configuration
    dedup_config: DeduplicationConfig,
    /// Cache of seen packet IDs (packet_id -> entry)
    seen_packets: Arc<RwLock<HashMap<String, DeduplicationEntry>>>,
}

impl SelectiveRouter {
    /// Create a new selective router (deduplication disabled by default)
    pub fn new() -> Self {
        Self {
            verbose: false,
            dedup_config: DeduplicationConfig {
                enabled: false,
                ..Default::default()
            },
            seen_packets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new selective router with verbose logging
    pub fn new_verbose() -> Self {
        Self {
            verbose: true,
            dedup_config: DeduplicationConfig {
                enabled: false,
                ..Default::default()
            },
            seen_packets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new selective router with deduplication enabled
    pub fn new_with_deduplication(config: DeduplicationConfig) -> Self {
        Self {
            verbose: false,
            dedup_config: config,
            seen_packets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a packet has been seen before (for deduplication)
    ///
    /// Returns `true` if this is a duplicate packet that should be dropped.
    /// If the packet is new, it's added to the seen cache.
    fn is_duplicate(&self, packet_id: &str) -> bool {
        if !self.dedup_config.enabled {
            return false;
        }

        let now = Instant::now();

        // Try to insert into cache
        let mut cache = self.seen_packets.write().unwrap();

        // Check if already seen and not expired
        if let Some(entry) = cache.get(packet_id) {
            if now.duration_since(entry.first_seen) < self.dedup_config.ttl {
                if self.verbose {
                    debug!("Duplicate packet detected: {}", packet_id);
                }
                return true;
            }
            // Entry expired, will be replaced below
        }

        // Evict expired entries if cache is getting full
        if cache.len() >= self.dedup_config.max_entries {
            self.evict_expired(&mut cache, now);

            // If still full after eviction, remove oldest entry
            if cache.len() >= self.dedup_config.max_entries {
                if let Some(oldest_key) = cache
                    .iter()
                    .min_by_key(|(_, entry)| entry.first_seen)
                    .map(|(k, _)| k.clone())
                {
                    cache.remove(&oldest_key);
                }
            }
        }

        // Record this packet
        cache.insert(
            packet_id.to_string(),
            DeduplicationEntry { first_seen: now },
        );

        false
    }

    /// Evict expired entries from the cache
    fn evict_expired(&self, cache: &mut HashMap<String, DeduplicationEntry>, now: Instant) {
        cache.retain(|_, entry| now.duration_since(entry.first_seen) < self.dedup_config.ttl);
    }

    /// Get the number of entries in the deduplication cache
    pub fn dedup_cache_size(&self) -> usize {
        self.seen_packets.read().unwrap().len()
    }

    /// Clear the deduplication cache
    pub fn clear_dedup_cache(&self) {
        self.seen_packets.write().unwrap().clear();
    }

    /// Make a complete routing decision for a packet
    ///
    /// This is the primary entry point that combines should_consume,
    /// should_forward, and next_hop into a single decision.
    ///
    /// If deduplication is enabled, duplicate packets are automatically dropped.
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
        // Check for duplicate packet (if deduplication enabled)
        if self.is_duplicate(&packet.packet_id) {
            if self.verbose {
                debug!("Packet {} is a duplicate, dropping", packet.packet_id);
            }
            return RoutingDecision::Drop;
        }

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
            // Both consume and forward - check if multicast needed
            let next_hops = self.next_hops(packet, state);
            if next_hops.is_empty() {
                // Can't forward without next hop, just consume
                if self.verbose {
                    debug!("Packet {}: Consume only (no next hop)", packet.packet_id);
                }
                RoutingDecision::Consume
            } else if next_hops.len() == 1 {
                // Single next hop - use unicast variant
                if self.verbose {
                    debug!(
                        "Packet {}: Consume and forward to {}",
                        packet.packet_id, next_hops[0]
                    );
                }
                RoutingDecision::ConsumeAndForward {
                    next_hop: next_hops[0].clone(),
                }
            } else {
                // Multiple next hops - use multicast variant
                if self.verbose {
                    debug!(
                        "Packet {}: Consume and multicast to {} peers",
                        packet.packet_id,
                        next_hops.len()
                    );
                }
                RoutingDecision::ConsumeAndForwardMulticast { next_hops }
            }
        } else if should_consume {
            if self.verbose {
                debug!("Packet {}: Consume only", packet.packet_id);
            }
            RoutingDecision::Consume
        } else if should_forward {
            let next_hops = self.next_hops(packet, state);
            if next_hops.is_empty() {
                if self.verbose {
                    warn!(
                        "Packet {}: Should forward but no next hop, dropping",
                        packet.packet_id
                    );
                }
                RoutingDecision::Drop
            } else if next_hops.len() == 1 {
                // Single next hop - use unicast variant
                if self.verbose {
                    debug!("Packet {}: Forward to {}", packet.packet_id, next_hops[0]);
                }
                RoutingDecision::Forward {
                    next_hop: next_hops[0].clone(),
                }
            } else {
                // Multiple next hops - use multicast variant
                if self.verbose {
                    debug!(
                        "Packet {}: Multicast to {} peers",
                        packet.packet_id,
                        next_hops.len()
                    );
                }
                RoutingDecision::ForwardMulticast { next_hops }
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

                // Otherwise, return first linked peer for backward compatibility
                // For multicast/broadcast, use next_hops() instead
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

                // Otherwise, route to first lateral peer for backward compatibility
                state.lateral_peers.keys().next().cloned()
            }
        }
    }

    /// Determine all next hops for multicast/broadcast forwarding
    ///
    /// Returns all appropriate peers for scenarios requiring multicast:
    /// - Downward command dissemination to all children
    /// - Lateral coordination broadcast to all peers at same level
    ///
    /// # Next Hops Selection
    ///
    /// **Upward**: Returns selected_peer (parent) as single-element vector
    /// **Downward**: Returns all linked_peers (children) for broadcast
    /// **Lateral**: Returns all lateral_peers for broadcast
    ///
    /// # Arguments
    ///
    /// * `packet` - The data packet
    /// * `state` - Current topology state
    ///
    /// # Returns
    ///
    /// Vector of node IDs to forward to (empty if no valid hops)
    pub fn next_hops(&self, packet: &DataPacket, state: &TopologyState) -> Vec<String> {
        match packet.direction {
            DataDirection::Upward => {
                // Upward: Return selected peer (parent) as single-element vector
                state
                    .selected_peer
                    .as_ref()
                    .map(|peer| vec![peer.node_id.clone()])
                    .unwrap_or_default()
            }

            DataDirection::Downward => {
                // Downward: Route to all linked peers (children) for broadcast
                // If addressed to specific child, route only there
                if let Some(ref dest) = packet.destination_node_id {
                    if state.linked_peers.contains_key(dest) {
                        return vec![dest.clone()];
                    }
                }

                // Otherwise, route to ALL linked peers (multicast/broadcast)
                state.linked_peers.keys().cloned().collect()
            }

            DataDirection::Lateral => {
                // Lateral: Route to all lateral peers for broadcast
                if let Some(ref dest) = packet.destination_node_id {
                    // Route to specific lateral peer if we track them
                    if state.lateral_peers.contains_key(dest) {
                        return vec![dest.clone()];
                    }
                }

                // Otherwise, route to ALL lateral peers (broadcast)
                state.lateral_peers.keys().cloned().collect()
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
    /// # Integration with Aggregator
    ///
    /// When this returns true, the application should:
    /// 1. Collect telemetry packets from squad members (batching)
    /// 2. Use `Aggregator::aggregate_telemetry()` to create aggregated packet
    /// 3. Route the aggregated packet upward using this router
    ///
    /// # Example
    ///
    /// ```ignore
    /// use hive_mesh::routing::{SelectiveRouter, Aggregator, DataPacket};
    ///
    /// let router = SelectiveRouter::new();
    /// // let aggregator = MyAggregator::new();
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
    use crate::routing::packet::{DataDirection, DataType};
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

    // ============================================================================
    // Week 10: Multicast/Broadcast Routing Tests
    // ============================================================================

    #[test]
    fn test_next_hops_upward_single_parent() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        // Upward should return selected_peer as single-element vector
        let next_hops = router.next_hops(&packet, &state);
        assert_eq!(next_hops.len(), 1);
        assert_eq!(next_hops[0], "parent-node");
    }

    #[test]
    fn test_next_hops_upward_no_parent() {
        let router = SelectiveRouter::new();
        // HQ node with no parent
        let state = create_test_state(HierarchyLevel::Company, NodeRole::Leader, false, 3, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        // No parent means empty vector
        let next_hops = router.next_hops(&packet, &state);
        assert_eq!(next_hops.len(), 0);
    }

    #[test]
    fn test_next_hops_downward_multicast() {
        let router = SelectiveRouter::new();
        // Platoon leader with 5 linked peers (children)
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 5, 0);
        // Create broadcast command packet (no specific destination)
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "hq".to_string(),
            destination_node_id: None, // Broadcast
            data_type: DataType::Command,
            direction: DataDirection::Downward,
            hop_count: 0,
            max_hops: 10,
            payload: vec![4, 5, 6],
        };

        // Downward broadcast should return ALL linked peers
        let next_hops = router.next_hops(&packet, &state);
        assert_eq!(next_hops.len(), 5);
        for i in 0..5 {
            assert!(next_hops.contains(&format!("linked-peer-{}", i)));
        }
    }

    #[test]
    fn test_next_hops_downward_targeted() {
        let router = SelectiveRouter::new();
        // Platoon leader with 3 linked peers
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 3, 0);
        let packet = DataPacket::command("hq", "linked-peer-1", vec![4, 5, 6]);

        // Targeted downward should return only the specific child
        let next_hops = router.next_hops(&packet, &state);
        assert_eq!(next_hops.len(), 1);
        assert_eq!(next_hops[0], "linked-peer-1");
    }

    #[test]
    fn test_next_hops_lateral_multicast() {
        let router = SelectiveRouter::new();
        // Leader with 4 lateral peers
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 2, 4);
        // Create broadcast coordination packet (no specific destination)
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "platoon-1".to_string(),
            destination_node_id: None, // Broadcast
            data_type: DataType::Coordination,
            direction: DataDirection::Lateral,
            hop_count: 0,
            max_hops: 3,
            payload: vec![7, 8, 9],
        };

        // Lateral broadcast should return ALL lateral peers
        let next_hops = router.next_hops(&packet, &state);
        assert_eq!(next_hops.len(), 4);
        for i in 0..4 {
            assert!(next_hops.contains(&format!("lateral-peer-{}", i)));
        }
    }

    #[test]
    fn test_next_hops_lateral_targeted() {
        let router = SelectiveRouter::new();
        // Leader with 3 lateral peers
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 2, 3);
        let packet = DataPacket::coordination("platoon-1", "lateral-peer-2", vec![7, 8, 9]);

        // Targeted lateral should return only specific peer
        let next_hops = router.next_hops(&packet, &state);
        assert_eq!(next_hops.len(), 1);
        assert_eq!(next_hops[0], "lateral-peer-2");
    }

    #[test]
    fn test_route_downward_multicast() {
        let router = SelectiveRouter::new();
        // HQ node (Leader) with 3 children, broadcasting command
        let state = create_test_state(HierarchyLevel::Company, NodeRole::Leader, false, 3, 0);
        // Create broadcast command packet (no specific destination)
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "hq".to_string(),
            destination_node_id: None, // Broadcast
            data_type: DataType::Command,
            direction: DataDirection::Downward,
            hop_count: 0,
            max_hops: 10,
            payload: vec![4, 5, 6],
        };

        let decision = router.route(&packet, &state, "hq-node");

        // Leaders should consume broadcast commands AND multicast to all children
        match decision {
            RoutingDecision::ConsumeAndForwardMulticast { next_hops } => {
                assert_eq!(next_hops.len(), 3);
                assert!(next_hops.contains(&"linked-peer-0".to_string()));
                assert!(next_hops.contains(&"linked-peer-1".to_string()));
                assert!(next_hops.contains(&"linked-peer-2".to_string()));
            }
            _ => panic!("Expected ConsumeAndForwardMulticast, got {:?}", decision),
        }
    }

    #[test]
    fn test_route_downward_consume_and_multicast() {
        let router = SelectiveRouter::new();
        // Platoon leader with 4 children, receiving command addressed to them
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 4, 0);
        let packet = DataPacket::command("hq", "platoon-leader", vec![4, 5, 6]);

        let decision = router.route(&packet, &state, "platoon-leader");

        // Should consume (addressed to us) AND multicast to all children
        match decision {
            RoutingDecision::ConsumeAndForwardMulticast { next_hops } => {
                assert_eq!(next_hops.len(), 4);
                for i in 0..4 {
                    assert!(next_hops.contains(&format!("linked-peer-{}", i)));
                }
            }
            _ => panic!("Expected ConsumeAndForwardMulticast, got {:?}", decision),
        }
    }

    #[test]
    fn test_route_lateral_multicast() {
        let router = SelectiveRouter::new();
        // Leader with 3 lateral peers, broadcasting coordination
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 0, 3);
        // Create broadcast coordination packet (no specific destination)
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "platoon-1".to_string(),
            destination_node_id: None, // Broadcast
            data_type: DataType::Coordination,
            direction: DataDirection::Lateral,
            hop_count: 0,
            max_hops: 3,
            payload: vec![7, 8, 9],
        };

        let decision = router.route(&packet, &state, "platoon-3");

        // Leaders should consume broadcast coordination AND forward to all lateral peers
        match decision {
            RoutingDecision::ConsumeAndForwardMulticast { next_hops } => {
                assert_eq!(next_hops.len(), 3);
                assert!(next_hops.contains(&"lateral-peer-0".to_string()));
                assert!(next_hops.contains(&"lateral-peer-1".to_string()));
                assert!(next_hops.contains(&"lateral-peer-2".to_string()));
            }
            _ => panic!("Expected ConsumeAndForwardMulticast, got {:?}", decision),
        }
    }

    #[test]
    fn test_route_downward_single_child_unicast() {
        let router = SelectiveRouter::new();
        // Leader with only 1 child
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, false, 1, 0);
        // Create broadcast command packet (no specific destination)
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "hq".to_string(),
            destination_node_id: None, // Broadcast
            data_type: DataType::Command,
            direction: DataDirection::Downward,
            hop_count: 0,
            max_hops: 10,
            payload: vec![4, 5, 6],
        };

        let decision = router.route(&packet, &state, "platoon-leader");

        // Leaders consume broadcast commands, and with only 1 child use unicast variant
        match decision {
            RoutingDecision::ConsumeAndForward { next_hop } => {
                assert_eq!(next_hop, "linked-peer-0");
            }
            _ => panic!("Expected ConsumeAndForward (unicast), got {:?}", decision),
        }
    }

    #[test]
    fn test_route_lateral_single_peer_unicast() {
        let router = SelectiveRouter::new();
        // Leader with only 1 lateral peer
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 0, 1);
        // Create broadcast coordination packet (no specific destination)
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "platoon-1".to_string(),
            destination_node_id: None, // Broadcast
            data_type: DataType::Coordination,
            direction: DataDirection::Lateral,
            hop_count: 0,
            max_hops: 3,
            payload: vec![7, 8, 9],
        };

        let decision = router.route(&packet, &state, "platoon-3");

        // Leaders consume broadcast coordination, and with only 1 lateral peer use unicast variant
        match decision {
            RoutingDecision::ConsumeAndForward { next_hop } => {
                assert_eq!(next_hop, "lateral-peer-0");
            }
            _ => panic!("Expected ConsumeAndForward (unicast), got {:?}", decision),
        }
    }

    #[test]
    fn test_route_downward_no_children_drop() {
        let router = SelectiveRouter::new();
        // Leaf node with no children
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        // Create broadcast command packet (no specific destination)
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "hq".to_string(),
            destination_node_id: None, // Broadcast
            data_type: DataType::Command,
            direction: DataDirection::Downward,
            hop_count: 0,
            max_hops: 10,
            payload: vec![4, 5, 6],
        };

        let decision = router.route(&packet, &state, "squad-member");

        // With no children, downward broadcast should drop (or consume if addressed)
        // In this case, not addressed to us, so should drop
        assert_eq!(decision, RoutingDecision::Drop);
    }

    #[test]
    fn test_multicast_preserves_backward_compatibility() {
        let router = SelectiveRouter::new();
        // Intermediate node with parent (upward routing)
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 3, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        let decision = router.route(&packet, &state, "platoon-leader");

        // Upward routing should still use ConsumeAndForward (unicast)
        match decision {
            RoutingDecision::ConsumeAndForward { next_hop } => {
                assert_eq!(next_hop, "parent-node");
            }
            _ => panic!(
                "Expected ConsumeAndForward (backward compat), got {:?}",
                decision
            ),
        }

        // next_hop() should still work for backward compatibility
        let next_hop = router.next_hop(&packet, &state);
        assert_eq!(next_hop, Some("parent-node".to_string()));
    }

    // ============================================================================
    // Message Deduplication Tests
    // ============================================================================

    #[test]
    fn test_deduplication_disabled_by_default() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        // Route same packet twice - should NOT be dropped (dedup disabled)
        let decision1 = router.route(&packet, &state, "this-node");
        let decision2 = router.route(&packet, &state, "this-node");

        // Both should route normally (ConsumeAndForward)
        assert!(matches!(
            decision1,
            RoutingDecision::ConsumeAndForward { .. }
        ));
        assert!(matches!(
            decision2,
            RoutingDecision::ConsumeAndForward { .. }
        ));
        assert_eq!(router.dedup_cache_size(), 0);
    }

    #[test]
    fn test_deduplication_enabled() {
        let router = SelectiveRouter::new_with_deduplication(DeduplicationConfig::default());
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        // First route should succeed
        let decision1 = router.route(&packet, &state, "this-node");
        assert!(matches!(
            decision1,
            RoutingDecision::ConsumeAndForward { .. }
        ));
        assert_eq!(router.dedup_cache_size(), 1);

        // Second route of same packet should be dropped
        let decision2 = router.route(&packet, &state, "this-node");
        assert_eq!(decision2, RoutingDecision::Drop);
        assert_eq!(router.dedup_cache_size(), 1); // No new entry added
    }

    #[test]
    fn test_deduplication_different_packets() {
        let router = SelectiveRouter::new_with_deduplication(DeduplicationConfig::default());
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet1 = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);
        let packet2 = DataPacket::telemetry("sensor-2", vec![4, 5, 6]);

        // Route two different packets - both should succeed
        let decision1 = router.route(&packet1, &state, "this-node");
        let decision2 = router.route(&packet2, &state, "this-node");

        assert!(matches!(
            decision1,
            RoutingDecision::ConsumeAndForward { .. }
        ));
        assert!(matches!(
            decision2,
            RoutingDecision::ConsumeAndForward { .. }
        ));
        assert_eq!(router.dedup_cache_size(), 2);
    }

    #[test]
    fn test_deduplication_cache_clear() {
        let router = SelectiveRouter::new_with_deduplication(DeduplicationConfig::default());
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        // Route packet
        let _ = router.route(&packet, &state, "this-node");
        assert_eq!(router.dedup_cache_size(), 1);

        // Clear cache
        router.clear_dedup_cache();
        assert_eq!(router.dedup_cache_size(), 0);

        // Should be able to route same packet again
        let decision = router.route(&packet, &state, "this-node");
        assert!(matches!(
            decision,
            RoutingDecision::ConsumeAndForward { .. }
        ));
    }

    #[test]
    fn test_deduplication_config_defaults() {
        let config = DeduplicationConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ttl, Duration::from_secs(300));
        assert_eq!(config.max_entries, 10000);
    }

    #[test]
    fn test_verbose_router() {
        let router = SelectiveRouter::new_verbose();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        // Should work the same as non-verbose, just with logging
        let decision = router.route(&packet, &state, "this-node");
        assert!(matches!(
            decision,
            RoutingDecision::ConsumeAndForward { .. }
        ));
    }

    #[test]
    fn test_verbose_max_hops_drop() {
        let router = SelectiveRouter::new_verbose();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let mut packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);
        for _ in 0..10 {
            packet.increment_hop();
        }
        let decision = router.route(&packet, &state, "this-node");
        assert_eq!(decision, RoutingDecision::Drop);
    }

    #[test]
    fn test_verbose_own_packet_drop() {
        let router = SelectiveRouter::new_verbose();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("this-node", vec![1, 2, 3]);
        let decision = router.route(&packet, &state, "this-node");
        assert_eq!(decision, RoutingDecision::Drop);
    }

    #[test]
    fn test_verbose_consume_only() {
        let router = SelectiveRouter::new_verbose();
        let state = create_test_state(HierarchyLevel::Company, NodeRole::Leader, false, 3, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);
        let decision = router.route(&packet, &state, "hq-node");
        assert_eq!(decision, RoutingDecision::Consume);
    }

    #[test]
    fn test_verbose_consume_and_forward() {
        let router = SelectiveRouter::new_verbose();
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 3, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);
        let decision = router.route(&packet, &state, "platoon-leader");
        assert!(matches!(
            decision,
            RoutingDecision::ConsumeAndForward { .. }
        ));
    }

    #[test]
    fn test_verbose_consume_and_multicast() {
        let router = SelectiveRouter::new_verbose();
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 4, 0);
        let packet = DataPacket::command("hq", "platoon-leader", vec![4, 5, 6]);
        let decision = router.route(&packet, &state, "platoon-leader");
        assert!(matches!(
            decision,
            RoutingDecision::ConsumeAndForwardMulticast { .. }
        ));
    }

    #[test]
    fn test_verbose_forward_multicast() {
        let router = SelectiveRouter::new_verbose();
        // Member (not leader) with lateral peers and broadcast lateral packet
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Member, true, 3, 0);
        // Downward broadcast from hq - member doesn't consume, should forward to children
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "hq".to_string(),
            destination_node_id: None,
            data_type: DataType::Command,
            direction: DataDirection::Downward,
            hop_count: 0,
            max_hops: 10,
            payload: vec![4, 5, 6],
        };
        let decision = router.route(&packet, &state, "member-node");
        assert!(matches!(
            decision,
            RoutingDecision::ForwardMulticast { .. }
        ));
    }

    #[test]
    fn test_verbose_forward_unicast() {
        let router = SelectiveRouter::new_verbose();
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Member, true, 1, 0);
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "hq".to_string(),
            destination_node_id: None,
            data_type: DataType::Command,
            direction: DataDirection::Downward,
            hop_count: 0,
            max_hops: 10,
            payload: vec![4, 5, 6],
        };
        let decision = router.route(&packet, &state, "member-node");
        assert!(matches!(decision, RoutingDecision::Forward { .. }));
    }

    #[test]
    fn test_verbose_forward_no_next_hop_drop() {
        let router = SelectiveRouter::new_verbose();
        // Member with lateral peer but packet directed to unknown lateral
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Member, false, 0, 1);
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "other".to_string(),
            destination_node_id: Some("lateral-peer-0".to_string()),
            data_type: DataType::Coordination,
            direction: DataDirection::Lateral,
            hop_count: 0,
            max_hops: 3,
            payload: vec![7, 8, 9],
        };
        // Member doesn't consume lateral (not addressed to us), but does forward
        let decision = router.route(&packet, &state, "member-node");
        assert!(matches!(decision, RoutingDecision::Forward { .. }));
    }

    #[test]
    fn test_verbose_drop_not_for_us() {
        let router = SelectiveRouter::new_verbose();
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, false, 0, 0);
        // Lateral packet not addressed to us, no lateral peers
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "other".to_string(),
            destination_node_id: Some("someone-else".to_string()),
            data_type: DataType::Coordination,
            direction: DataDirection::Lateral,
            hop_count: 0,
            max_hops: 3,
            payload: vec![7, 8, 9],
        };
        let decision = router.route(&packet, &state, "member-node");
        assert_eq!(decision, RoutingDecision::Drop);
    }

    #[test]
    fn test_verbose_dedup_drop() {
        let config = DeduplicationConfig {
            enabled: true,
            ttl: Duration::from_secs(300),
            max_entries: 100,
        };
        let router = SelectiveRouter {
            verbose: true,
            dedup_config: config,
            seen_packets: Arc::new(RwLock::new(HashMap::new())),
        };
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        let _ = router.route(&packet, &state, "this-node");
        let decision2 = router.route(&packet, &state, "this-node");
        assert_eq!(decision2, RoutingDecision::Drop);
    }

    #[test]
    fn test_forward_only_no_consume_member_downward() {
        // Member with children receiving non-addressed broadcast command
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Member, true, 2, 0);
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "hq".to_string(),
            destination_node_id: None,
            data_type: DataType::Command,
            direction: DataDirection::Downward,
            hop_count: 0,
            max_hops: 10,
            payload: vec![4, 5, 6],
        };
        let decision = router.route(&packet, &state, "member-node");
        // Member doesn't consume broadcast commands, just forwards
        assert!(matches!(
            decision,
            RoutingDecision::ForwardMulticast { .. }
        ));
    }

    #[test]
    fn test_should_forward_no_next_hop_returns_drop() {
        let router = SelectiveRouter::new();
        // Member with no linked peers, no lateral peers, downward packet addressed to unknown
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, false, 0, 0);
        let packet = DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: "hq".to_string(),
            destination_node_id: None,
            data_type: DataType::Command,
            direction: DataDirection::Downward,
            hop_count: 0,
            max_hops: 10,
            payload: vec![4, 5, 6],
        };
        let decision = router.route(&packet, &state, "squad-member");
        assert_eq!(decision, RoutingDecision::Drop);
    }

    #[test]
    fn test_next_hop_downward_targeted_not_found() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 3, 0);
        // Addressed to a peer not in linked_peers
        let packet = DataPacket::command("hq", "unknown-child", vec![4, 5, 6]);

        let next_hop = router.next_hop(&packet, &state);
        // Should return first linked peer as fallback
        assert!(next_hop.is_some());
        assert!(next_hop.unwrap().starts_with("linked-peer-"));
    }

    #[test]
    fn test_next_hop_lateral_unknown_peer() {
        let router = SelectiveRouter::new();
        let state = create_test_state(HierarchyLevel::Platoon, NodeRole::Leader, true, 0, 3);
        let packet = DataPacket::coordination("source", "unknown-lateral", vec![7, 8, 9]);

        let next_hop = router.next_hop(&packet, &state);
        // Should return first lateral peer as fallback
        assert!(next_hop.is_some());
        assert!(next_hop.unwrap().starts_with("lateral-peer-"));
    }

    #[test]
    fn test_default_router() {
        let router = SelectiveRouter::default();
        assert_eq!(router.dedup_cache_size(), 0);
    }

    #[test]
    fn test_deduplication_max_entries_eviction() {
        let config = DeduplicationConfig {
            enabled: true,
            ttl: Duration::from_secs(300),
            max_entries: 3, // Very small for testing
        };
        let router = SelectiveRouter::new_with_deduplication(config);
        let state = create_test_state(HierarchyLevel::Squad, NodeRole::Member, true, 0, 0);

        // Route 5 packets
        for i in 0..5 {
            let packet = DataPacket::telemetry(format!("sensor-{}", i), vec![i as u8]);
            let _ = router.route(&packet, &state, "this-node");
        }

        // Cache should be limited to max_entries
        assert!(router.dedup_cache_size() <= 3);
    }
}
