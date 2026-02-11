//! MeshRouter facade combining routing, aggregation, and transport
//!
//! This module provides a unified interface for mesh data routing that combines:
//! - SelectiveRouter for routing decisions
//! - Aggregator for hierarchical telemetry summarization
//! - MeshTransport for packet delivery
//! - Message deduplication for loop prevention
//!
//! # Architecture
//!
//! The MeshRouter acts as a facade over the existing components, providing a
//! simple API for sending and receiving data through the mesh hierarchy.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                       MeshRouter                            │
//! │  ┌─────────────────┐  ┌─────────────────┐                  │
//! │  │ SelectiveRouter │  │ Aggregator│                  │
//! │  │  (decisions)    │  │  (aggregation)  │                  │
//! │  └────────┬────────┘  └────────┬────────┘                  │
//! │           │                    │                           │
//! │           └────────────────────┘                           │
//! │                       │                                    │
//! │              ┌────────▼────────┐                           │
//! │              │  TopologyState  │                           │
//! │              │  (mesh state)   │                           │
//! │              └────────┬────────┘                           │
//! │                       │                                    │
//! │              ┌────────▼────────┐                           │
//! │              │  MeshTransport  │                           │
//! │              │  (delivery)     │                           │
//! │              └─────────────────┘                           │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use hive_mesh::routing::{MeshRouter, MeshRouterConfig, DataPacket};
//! use hive_mesh::topology::TopologyState;
//!
//! // Create mesh router with transport
//! let router = MeshRouter::new(
//!     MeshRouterConfig::default(),
//!     "my-node-id".to_string(),
//! );
//!
//! // Send telemetry (routing handled automatically)
//! let packet = DataPacket::telemetry("my-node-id", telemetry_bytes);
//! router.send(packet, &topology_state).await?;
//!
//! // Receive and route incoming packet
//! let decision = router.receive(incoming_packet, &topology_state);
//! ```

use super::aggregator::{Aggregator, NoOpAggregator};
use super::packet::DataPacket;
use super::router::{DeduplicationConfig, RoutingDecision, SelectiveRouter};
use crate::topology::TopologyState;
use std::sync::Arc;
use tracing::{debug, info};

/// Configuration for MeshRouter
#[derive(Debug, Clone)]
pub struct MeshRouterConfig {
    /// This node's ID
    pub node_id: String,
    /// Deduplication configuration
    pub deduplication: DeduplicationConfig,
    /// Whether to enable automatic aggregation
    pub auto_aggregate: bool,
    /// Minimum packets before triggering aggregation
    pub aggregation_threshold: usize,
    /// Enable verbose logging
    pub verbose: bool,
}

impl Default for MeshRouterConfig {
    fn default() -> Self {
        Self {
            node_id: String::new(),
            deduplication: DeduplicationConfig::default(),
            auto_aggregate: true,
            aggregation_threshold: 3,
            verbose: false,
        }
    }
}

impl MeshRouterConfig {
    /// Create configuration with a specific node ID
    pub fn with_node_id(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            ..Default::default()
        }
    }
}

/// Result of processing an incoming packet
#[derive(Debug)]
pub struct RouteResult {
    /// The routing decision made
    pub decision: RoutingDecision,
    /// Whether the packet should be aggregated
    pub should_aggregate: bool,
    /// Next hop(s) for forwarding (if any)
    pub forward_to: Vec<String>,
}

/// Unified mesh router combining routing, aggregation, and transport
///
/// MeshRouter provides a high-level API for routing data through the mesh
/// hierarchy while handling deduplication and aggregation automatically.
///
/// The type parameter `A` determines which aggregation strategy is used.
/// Use [`NoOpAggregator`] (the default) when aggregation is not needed.
pub struct MeshRouter<A: Aggregator = NoOpAggregator> {
    /// Configuration
    config: MeshRouterConfig,
    /// Selective router for routing decisions
    router: SelectiveRouter,
    /// Packet aggregator for telemetry summarization
    aggregator: A,
    /// Pending telemetry packets for aggregation (squad_id -> packets)
    pending_aggregation: Arc<std::sync::RwLock<Vec<DataPacket>>>,
}

impl MeshRouter<NoOpAggregator> {
    /// Create a new mesh router with no aggregation
    pub fn new(config: MeshRouterConfig) -> Self {
        Self::with_aggregator(config, NoOpAggregator)
    }

    /// Create with default configuration and node ID (no aggregation)
    pub fn with_node_id(node_id: impl Into<String>) -> Self {
        Self::new(MeshRouterConfig::with_node_id(node_id))
    }
}

impl<A: Aggregator> MeshRouter<A> {
    /// Create a new mesh router with a specific aggregator
    pub fn with_aggregator(config: MeshRouterConfig, aggregator: A) -> Self {
        let router = if config.deduplication.enabled {
            SelectiveRouter::new_with_deduplication(config.deduplication.clone())
        } else {
            SelectiveRouter::new()
        };

        Self {
            config,
            router,
            aggregator,
            pending_aggregation: Arc::new(std::sync::RwLock::new(Vec::new())),
        }
    }

    /// Route an incoming packet and determine what to do with it
    ///
    /// This is the main entry point for processing received packets.
    /// It handles deduplication, routing decisions, and aggregation checks.
    ///
    /// # Arguments
    ///
    /// * `packet` - The incoming data packet
    /// * `state` - Current topology state
    ///
    /// # Returns
    ///
    /// RouteResult containing the routing decision and forwarding targets
    pub fn route(&self, packet: &DataPacket, state: &TopologyState) -> RouteResult {
        let decision = self.router.route(packet, state, &self.config.node_id);

        let should_aggregate = self.router.should_aggregate(packet, &decision, state);

        let forward_to = match &decision {
            RoutingDecision::Forward { next_hop } => vec![next_hop.clone()],
            RoutingDecision::ConsumeAndForward { next_hop } => vec![next_hop.clone()],
            RoutingDecision::ForwardMulticast { next_hops } => next_hops.clone(),
            RoutingDecision::ConsumeAndForwardMulticast { next_hops } => next_hops.clone(),
            _ => vec![],
        };

        RouteResult {
            decision,
            should_aggregate,
            forward_to,
        }
    }

    /// Add a telemetry packet to the aggregation buffer
    ///
    /// If aggregation threshold is reached, returns aggregated packet.
    /// Otherwise returns None.
    pub fn add_for_aggregation(&self, packet: DataPacket, squad_id: &str) -> Option<DataPacket> {
        if !self.config.auto_aggregate {
            return None;
        }

        let mut pending = self.pending_aggregation.write().unwrap();
        pending.push(packet);

        if pending.len() >= self.config.aggregation_threshold {
            // Aggregate and return
            let packets: Vec<DataPacket> = pending.drain(..).collect();
            match self
                .aggregator
                .aggregate_telemetry(squad_id, &self.config.node_id, packets)
            {
                Ok(aggregated) => {
                    if self.config.verbose {
                        info!(
                            "Aggregated {} packets into squad summary for {}",
                            self.config.aggregation_threshold, squad_id
                        );
                    }
                    Some(aggregated)
                }
                Err(e) => {
                    debug!("Aggregation failed: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Get the number of packets pending aggregation
    pub fn pending_aggregation_count(&self) -> usize {
        self.pending_aggregation.read().unwrap().len()
    }

    /// Force aggregation of pending packets (even if below threshold)
    pub fn flush_aggregation(&self, squad_id: &str) -> Option<DataPacket> {
        let mut pending = self.pending_aggregation.write().unwrap();
        if pending.is_empty() {
            return None;
        }

        let packets: Vec<DataPacket> = pending.drain(..).collect();
        match self
            .aggregator
            .aggregate_telemetry(squad_id, &self.config.node_id, packets)
        {
            Ok(aggregated) => Some(aggregated),
            Err(e) => {
                debug!("Flush aggregation failed: {}", e);
                None
            }
        }
    }

    /// Get the underlying router for advanced operations
    pub fn router(&self) -> &SelectiveRouter {
        &self.router
    }

    /// Get the underlying aggregator for advanced operations
    pub fn aggregator(&self) -> &A {
        &self.aggregator
    }

    /// Get the node ID this router is configured for
    pub fn node_id(&self) -> &str {
        &self.config.node_id
    }

    /// Get deduplication cache size
    pub fn dedup_cache_size(&self) -> usize {
        self.router.dedup_cache_size()
    }

    /// Clear deduplication cache
    pub fn clear_dedup_cache(&self) {
        self.router.clear_dedup_cache();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{GeoPosition, GeographicBeacon, HierarchyLevel};
    use crate::hierarchy::NodeRole;
    use crate::topology::SelectedPeer;
    use std::collections::HashMap;
    use std::time::Instant;

    fn create_test_state() -> TopologyState {
        TopologyState {
            selected_peer: Some(SelectedPeer {
                node_id: "parent-node".to_string(),
                beacon: GeographicBeacon::new(
                    "parent-node".to_string(),
                    GeoPosition::new(37.7749, -122.4194),
                    HierarchyLevel::Platoon,
                ),
                selected_at: Instant::now(),
            }),
            linked_peers: HashMap::new(),
            lateral_peers: HashMap::new(),
            role: NodeRole::Member,
            hierarchy_level: HierarchyLevel::Squad,
        }
    }

    #[test]
    fn test_mesh_router_creation() {
        let router = MeshRouter::with_node_id("test-node");
        assert_eq!(router.node_id(), "test-node");
        assert_eq!(router.pending_aggregation_count(), 0);
    }

    #[test]
    fn test_mesh_router_routing() {
        let router = MeshRouter::with_node_id("test-node");
        let state = create_test_state();
        let packet = DataPacket::telemetry("other-node", vec![1, 2, 3]);

        let result = router.route(&packet, &state);

        // Should consume and forward upward
        assert!(!result.forward_to.is_empty());
        assert_eq!(result.forward_to[0], "parent-node");
    }

    #[test]
    fn test_mesh_router_deduplication() {
        let config = MeshRouterConfig {
            node_id: "test-node".to_string(),
            deduplication: DeduplicationConfig {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let router = MeshRouter::new(config);
        let state = create_test_state();
        let packet = DataPacket::telemetry("other-node", vec![1, 2, 3]);

        // First route should succeed
        let result1 = router.route(&packet, &state);
        assert!(!matches!(result1.decision, RoutingDecision::Drop));
        assert_eq!(router.dedup_cache_size(), 1);

        // Second route of same packet should be dropped (duplicate)
        let result2 = router.route(&packet, &state);
        assert_eq!(result2.decision, RoutingDecision::Drop);
    }

    #[test]
    fn test_mesh_router_aggregation_disabled() {
        let config = MeshRouterConfig {
            node_id: "test-node".to_string(),
            auto_aggregate: false,
            ..Default::default()
        };
        let router = MeshRouter::new(config);
        let packet = DataPacket::telemetry("sensor-1", vec![1, 2, 3]);

        let result = router.add_for_aggregation(packet, "squad-1");
        assert!(result.is_none());
        assert_eq!(router.pending_aggregation_count(), 0);
    }
}
