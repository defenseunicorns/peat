//! Hierarchical message router enforcing routing rules
//!
//! This module implements the routing layer for hierarchical operations,
//! enforcing rules about which nodes can message which other nodes based
//! on their position in the node→cell→zone hierarchy.

use crate::cell::messaging::MessagePriority;
use crate::hierarchy::flow_control::{FlowController, RoutingLevel};
use crate::hierarchy::routing_table::RoutingTable;
use crate::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, instrument, warn};

/// Hierarchical message router enforcing routing rules
///
/// This router enforces the hierarchical messaging rules:
/// 1. Nodes can message peers in their own cell
/// 2. Cell leaders can message upward to zone level
/// 3. Non-leaders cannot message cross-cell or to zone level
/// 4. All direct cross-cell messages are rejected
///
/// # Example
/// ```
/// use hive_protocol::hierarchy::{HierarchicalRouter, RoutingTable};
/// use std::sync::Arc;
/// use tokio::sync::RwLock;
///
/// # async fn example() {
/// let mut routing_table = RoutingTable::new();
/// routing_table.assign_node("node1", "cell_alpha", 100);
/// routing_table.assign_node("node2", "cell_alpha", 101);
/// routing_table.set_cell_leader("cell_alpha", "node1", 102);
///
/// let router = HierarchicalRouter::new(
///     "node1".to_string(),
///     Arc::new(RwLock::new(routing_table)),
/// );
///
/// // Check routing rules
/// assert!(router.is_route_valid("node1", "node2").await); // Same cell
/// assert!(!router.is_route_valid("node1", "node_in_other_cell").await); // Cross-cell
/// # }
/// ```
pub struct HierarchicalRouter {
    /// This node's ID
    node_id: String,
    /// Shared routing table (read-mostly, updated on membership changes)
    routing_table: Arc<RwLock<RoutingTable>>,
    /// Optional flow controller for rate limiting and backpressure
    flow_controller: Option<Arc<FlowController>>,
}

impl HierarchicalRouter {
    /// Create a new hierarchical router
    ///
    /// # Arguments
    /// * `node_id` - The ID of this node
    /// * `routing_table` - Shared routing table for the system
    pub fn new(node_id: String, routing_table: Arc<RwLock<RoutingTable>>) -> Self {
        Self {
            node_id,
            routing_table,
            flow_controller: None,
        }
    }

    /// Create a new hierarchical router with flow control
    ///
    /// # Arguments
    /// * `node_id` - The ID of this node
    /// * `routing_table` - Shared routing table for the system
    /// * `flow_controller` - Flow controller for rate limiting
    pub fn with_flow_control(
        node_id: String,
        routing_table: Arc<RwLock<RoutingTable>>,
        flow_controller: Arc<FlowController>,
    ) -> Self {
        Self {
            node_id,
            routing_table,
            flow_controller: Some(flow_controller),
        }
    }

    /// Check if a route from one node to another is valid per hierarchy rules
    ///
    /// # Routing Rules
    /// 1. Node can message cell peers only (same cell)
    /// 2. Cell leader can message cell peers + zone level (upward)
    /// 3. Non-leader cannot message cross-cell or zone level
    /// 4. All direct cross-cell messages are rejected
    ///
    /// # Arguments
    /// * `from` - Source node ID
    /// * `to` - Target node ID
    ///
    /// # Returns
    /// `true` if the route is valid, `false` otherwise
    #[instrument(skip(self))]
    pub async fn is_route_valid(&self, from: &str, to: &str) -> bool {
        let table = self.routing_table.read().await;

        // Get source and target cell assignments
        let from_cell = match table.get_node_cell(from) {
            Some(cell) => cell,
            None => {
                warn!("Source node {} not assigned to any cell", from);
                return false;
            }
        };

        let to_cell = match table.get_node_cell(to) {
            Some(cell) => cell,
            None => {
                // Target might be a cell or zone, check if we're a leader routing upward
                return self.is_upward_route_valid(&table, from, from_cell, to);
            }
        };

        // Same cell routing - always allowed
        if from_cell == to_cell {
            debug!(
                "Allowing same-cell routing: {} → {} (cell: {})",
                from, to, from_cell
            );
            return true;
        }

        // Cross-cell routing - always rejected
        warn!(
            "Rejecting cross-cell routing: {} (cell: {}) → {} (cell: {})",
            from, from_cell, to, to_cell
        );
        false
    }

    /// Check if an upward route (to cell or zone) is valid
    ///
    /// Only cell leaders can route upward to zone level
    fn is_upward_route_valid(
        &self,
        table: &RoutingTable,
        from: &str,
        from_cell: &str,
        to: &str,
    ) -> bool {
        // Check if source is the leader of their cell
        if !table.is_cell_leader(from, from_cell) {
            warn!(
                "Rejecting upward routing from non-leader: {} → {}",
                from, to
            );
            return false;
        }

        // Check if target is a valid zone or cell
        // For now, we accept any target if the sender is a leader
        // In future phases, we'll validate zone membership
        debug!(
            "Allowing upward routing from cell leader: {} → {}",
            from, to
        );
        true
    }

    /// Get list of valid message targets for this node
    ///
    /// Returns all nodes in the same cell as this node.
    /// If this node is a cell leader, also includes zone-level targets.
    #[instrument(skip(self))]
    pub async fn valid_targets(&self) -> Vec<String> {
        let table = self.routing_table.read().await;

        let my_cell = match table.get_node_cell(&self.node_id) {
            Some(cell) => cell,
            None => {
                warn!("Node {} not assigned to any cell", self.node_id);
                return Vec::new();
            }
        };

        // Get all nodes in my cell
        let mut targets: Vec<String> = table
            .get_cell_nodes(my_cell)
            .into_iter()
            .filter(|&node| node != self.node_id) // Don't include self
            .map(|s| s.to_string())
            .collect();

        // If I'm the cell leader, I can also message zone level
        if table.is_cell_leader(&self.node_id, my_cell) {
            if let Some(zone_id) = table.get_cell_zone(my_cell) {
                // Add zone as a valid target (for zone coordinator, etc.)
                targets.push(format!("zone:{}", zone_id));
            }
        }

        targets
    }

    /// Update the routing table (called on membership changes)
    ///
    /// Merges the provided routing table with the current one using CRDT semantics.
    #[instrument(skip(self, new_table))]
    pub async fn update_routing_table(&mut self, new_table: RoutingTable) -> Result<()> {
        let mut table = self.routing_table.write().await;
        debug!("Updating routing table for node {}", self.node_id);
        table.merge(&new_table);
        Ok(())
    }

    /// Get statistics about current routing configuration
    pub async fn stats(&self) -> RouterStats {
        let table = self.routing_table.read().await;

        let my_cell = table.get_node_cell(&self.node_id).map(|s| s.to_string());
        let my_zone = table.get_node_zone(&self.node_id).map(|s| s.to_string());

        let is_leader = my_cell
            .as_ref()
            .map(|cell| table.is_cell_leader(&self.node_id, cell))
            .unwrap_or(false);

        let cell_peer_count = my_cell
            .as_ref()
            .map(|cell| table.get_cell_nodes(cell).len().saturating_sub(1)) // Exclude self
            .unwrap_or(0);

        RouterStats {
            node_id: self.node_id.clone(),
            cell_id: my_cell,
            zone_id: my_zone,
            is_cell_leader: is_leader,
            cell_peer_count,
        }
    }

    /// Get the node's cell ID
    pub async fn get_my_cell(&self) -> Option<String> {
        let table = self.routing_table.read().await;
        table.get_node_cell(&self.node_id).map(|s| s.to_string())
    }

    /// Get the node's zone ID
    pub async fn get_my_zone(&self) -> Option<String> {
        let table = self.routing_table.read().await;
        table.get_node_zone(&self.node_id).map(|s| s.to_string())
    }

    /// Check if this node is a cell leader
    pub async fn is_leader(&self) -> bool {
        let table = self.routing_table.read().await;
        if let Some(cell) = table.get_node_cell(&self.node_id) {
            table.is_cell_leader(&self.node_id, cell)
        } else {
            false
        }
    }

    /// Route a message with flow control
    ///
    /// Checks both routing validity and flow control before allowing message.
    /// Returns permit if routing is allowed and flow control permits.
    ///
    /// # Arguments
    /// * `from` - Source node ID
    /// * `to` - Target node ID
    /// * `message_size` - Size of message in bytes
    /// * `priority` - Message priority
    ///
    /// # Returns
    /// * `Ok(Some(permit))` - Message can be sent
    /// * `Ok(None)` - Routing not valid (wrong hierarchy level)
    /// * `Err(_)` - Flow control error
    #[instrument(skip(self))]
    pub async fn route_message(
        &self,
        from: &str,
        to: &str,
        message_size: usize,
        priority: MessagePriority,
    ) -> Result<Option<crate::hierarchy::flow_control::Permit>> {
        // First check if route is valid per hierarchy rules
        if !self.is_route_valid(from, to).await {
            debug!("Route from {} to {} rejected by hierarchy rules", from, to);
            return Ok(None);
        }

        // If flow control is enabled, acquire permit
        if let Some(fc) = &self.flow_controller {
            // Determine routing level based on target
            let table = self.routing_table.read().await;
            let from_cell = table.get_node_cell(from);
            let to_cell = table.get_node_cell(to);

            let level = if from_cell == to_cell && from_cell.is_some() {
                // Same cell = intra-cell routing
                RoutingLevel::Cell
            } else {
                // Cross-level routing (to zone)
                RoutingLevel::Zone
            };

            // Acquire permit (may block if under backpressure)
            let permit = fc.acquire_permit(level, message_size, priority).await?;
            Ok(Some(permit))
        } else {
            // No flow control, routing is valid
            Ok(None) // None means "no permit needed"
        }
    }

    /// Check if backpressure is active
    pub async fn has_backpressure(&self) -> bool {
        if let Some(fc) = &self.flow_controller {
            fc.has_backpressure().await
        } else {
            false
        }
    }

    /// Get flow controller (if enabled)
    pub fn flow_controller(&self) -> Option<Arc<FlowController>> {
        self.flow_controller.clone()
    }
}

// Implement SyncRouter trait for HierarchicalRouter (hive-mesh abstraction)
#[cfg(feature = "automerge-backend")]
#[async_trait::async_trait]
impl hive_mesh::storage::sync_transport::SyncRouter for HierarchicalRouter {
    async fn get_targets(
        &self,
        direction: hive_mesh::storage::automerge_sync::SyncDirection,
        connected: &[iroh::EndpointId],
    ) -> Vec<iroh::EndpointId> {
        use hive_mesh::storage::automerge_sync::SyncDirection;

        match direction {
            SyncDirection::Broadcast => connected.to_vec(),
            SyncDirection::Lateral => {
                let valid = self.valid_targets().await;
                let has_cell_peers = valid.iter().any(|t| !t.starts_with("zone:"));
                if has_cell_peers {
                    connected.to_vec()
                } else {
                    connected.to_vec()
                }
            }
            SyncDirection::Upward => {
                if self.is_leader().await {
                    let valid = self.valid_targets().await;
                    let has_zone_targets = valid.iter().any(|t| t.starts_with("zone:"));
                    if has_zone_targets {
                        tracing::debug!("Upward sync from leader - zone sync not yet implemented");
                        Vec::new()
                    } else {
                        connected.to_vec()
                    }
                } else {
                    connected.to_vec()
                }
            }
            SyncDirection::Downward => {
                if self.is_leader().await {
                    connected.to_vec()
                } else {
                    tracing::debug!(
                        "Non-leader ignoring downward sync (commands flow from leader)"
                    );
                    Vec::new()
                }
            }
        }
    }

    async fn is_leader(&self) -> bool {
        self.is_leader().await
    }
}

/// Statistics about the router's current state
#[derive(Debug, Clone)]
pub struct RouterStats {
    /// This node's ID
    pub node_id: String,
    /// Cell this node is assigned to (if any)
    pub cell_id: Option<String>,
    /// Zone this node is assigned to (if any)
    pub zone_id: Option<String>,
    /// Whether this node is a cell leader
    pub is_cell_leader: bool,
    /// Number of peers in the same cell
    pub cell_peer_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_routing_table() -> RoutingTable {
        let mut table = RoutingTable::new();

        // Cell Alpha: node1 (leader), node2, node3
        table.assign_node("node1", "cell_alpha", 100);
        table.assign_node("node2", "cell_alpha", 101);
        table.assign_node("node3", "cell_alpha", 102);
        table.set_cell_leader("cell_alpha", "node1", 103);

        // Cell Beta: node4 (leader), node5
        table.assign_node("node4", "cell_beta", 104);
        table.assign_node("node5", "cell_beta", 105);
        table.set_cell_leader("cell_beta", "node4", 106);

        // Assign cells to zones
        table.assign_cell("cell_alpha", "zone_north", 107);
        table.assign_cell("cell_beta", "zone_south", 108);

        table
    }

    #[tokio::test]
    async fn test_router_creation() {
        let table = setup_routing_table().await;
        let router = HierarchicalRouter::new("node1".to_string(), Arc::new(RwLock::new(table)));

        let stats = router.stats().await;
        assert_eq!(stats.node_id, "node1");
        assert_eq!(stats.cell_id, Some("cell_alpha".to_string()));
        assert_eq!(stats.zone_id, Some("zone_north".to_string()));
        assert!(stats.is_cell_leader);
        assert_eq!(stats.cell_peer_count, 2); // node2 and node3
    }

    #[tokio::test]
    async fn test_same_cell_routing() {
        let table = setup_routing_table().await;
        let router = HierarchicalRouter::new("node1".to_string(), Arc::new(RwLock::new(table)));

        // Same cell routing should be allowed
        assert!(router.is_route_valid("node1", "node2").await);
        assert!(router.is_route_valid("node2", "node3").await);
        assert!(router.is_route_valid("node3", "node1").await);
    }

    #[tokio::test]
    async fn test_cross_cell_routing_rejected() {
        let table = setup_routing_table().await;
        let router = HierarchicalRouter::new("node1".to_string(), Arc::new(RwLock::new(table)));

        // Cross-cell routing should be rejected
        assert!(!router.is_route_valid("node1", "node4").await);
        assert!(!router.is_route_valid("node2", "node5").await);
        assert!(!router.is_route_valid("node4", "node1").await);
    }

    #[tokio::test]
    async fn test_leader_upward_routing() {
        let table = setup_routing_table().await;
        let router = HierarchicalRouter::new("node1".to_string(), Arc::new(RwLock::new(table)));

        // Leader can route to zone (upward)
        assert!(router.is_route_valid("node1", "zone_coordinator").await);

        // Non-leader cannot route to zone
        assert!(!router.is_route_valid("node2", "zone_coordinator").await);
    }

    #[tokio::test]
    async fn test_valid_targets() {
        let table = setup_routing_table().await;

        // Non-leader node
        let router2 =
            HierarchicalRouter::new("node2".to_string(), Arc::new(RwLock::new(table.clone())));
        let mut targets = router2.valid_targets().await;
        targets.sort();
        assert_eq!(targets, vec!["node1", "node3"]); // Other nodes in cell_alpha

        // Leader node
        let router1 = HierarchicalRouter::new("node1".to_string(), Arc::new(RwLock::new(table)));
        let mut targets = router1.valid_targets().await;
        targets.sort();
        // Should include cell peers + zone target
        assert!(targets.contains(&"node2".to_string()));
        assert!(targets.contains(&"node3".to_string()));
        assert!(targets.contains(&"zone:zone_north".to_string()));
        assert_eq!(targets.len(), 3);
    }

    #[tokio::test]
    async fn test_unassigned_node() {
        let table = setup_routing_table().await;
        let router =
            HierarchicalRouter::new("node_unassigned".to_string(), Arc::new(RwLock::new(table)));

        // Unassigned node cannot route to anyone
        assert!(!router.is_route_valid("node_unassigned", "node1").await);

        // No valid targets
        let targets = router.valid_targets().await;
        assert_eq!(targets.len(), 0);

        let stats = router.stats().await;
        assert_eq!(stats.cell_id, None);
        assert!(!stats.is_cell_leader);
    }

    #[tokio::test]
    async fn test_routing_table_update() {
        let table = setup_routing_table().await;
        let mut router = HierarchicalRouter::new("node1".to_string(), Arc::new(RwLock::new(table)));

        // Initially node1 is in cell_alpha
        assert_eq!(router.get_my_cell().await, Some("cell_alpha".to_string()));

        // Create update that moves node1 to cell_beta
        let mut update = RoutingTable::new();
        update.assign_node("node1", "cell_beta", 200); // Higher timestamp wins

        router.update_routing_table(update).await.unwrap();

        // Now node1 should be in cell_beta
        assert_eq!(router.get_my_cell().await, Some("cell_beta".to_string()));
    }

    #[tokio::test]
    async fn test_leader_check() {
        let table = setup_routing_table().await;

        let router1 =
            HierarchicalRouter::new("node1".to_string(), Arc::new(RwLock::new(table.clone())));
        assert!(router1.is_leader().await); // node1 is leader

        let router2 = HierarchicalRouter::new("node2".to_string(), Arc::new(RwLock::new(table)));
        assert!(!router2.is_leader().await); // node2 is not leader
    }

    // ===== Flow Control Integration Tests =====

    #[tokio::test]
    async fn test_router_with_flow_control() {
        use crate::hierarchy::flow_control::{BandwidthLimit, MessageDropPolicy};

        let table = setup_routing_table().await;
        let fc = Arc::new(FlowController::new(
            BandwidthLimit::new(10, 1000),
            BandwidthLimit::new(5, 500),
            MessageDropPolicy::DropLowPriority,
        ));

        let router = HierarchicalRouter::with_flow_control(
            "node1".to_string(),
            Arc::new(RwLock::new(table)),
            fc.clone(),
        );

        // Verify flow controller is set
        assert!(router.flow_controller().is_some());
        assert!(!router.has_backpressure().await);
    }

    #[tokio::test]
    async fn test_route_message_with_flow_control() {
        use crate::hierarchy::flow_control::{BandwidthLimit, MessageDropPolicy};

        let table = setup_routing_table().await;
        let fc = Arc::new(FlowController::new(
            BandwidthLimit::new(100, 10000),
            BandwidthLimit::new(50, 5000),
            MessageDropPolicy::DropLowPriority,
        ));

        let router = HierarchicalRouter::with_flow_control(
            "node1".to_string(),
            Arc::new(RwLock::new(table)),
            fc.clone(),
        );

        // Route a message from node1 to node2 (same cell)
        let result = router
            .route_message("node1", "node2", 100, MessagePriority::Normal)
            .await
            .unwrap();

        // Should get a permit (Some) because routing is valid
        assert!(result.is_some());

        // Verify flow control metrics updated
        let metrics = fc.get_metrics();
        assert_eq!(metrics.cell_messages_sent, 1);
        assert_eq!(metrics.cell_bytes_sent, 100);
    }

    #[tokio::test]
    async fn test_route_message_cross_cell_rejected() {
        use crate::hierarchy::flow_control::{BandwidthLimit, MessageDropPolicy};

        let table = setup_routing_table().await;
        let fc = Arc::new(FlowController::new(
            BandwidthLimit::new(100, 10000),
            BandwidthLimit::new(50, 5000),
            MessageDropPolicy::DropLowPriority,
        ));

        let router = HierarchicalRouter::with_flow_control(
            "node1".to_string(),
            Arc::new(RwLock::new(table)),
            fc.clone(),
        );

        // Try to route from node1 (cell_alpha) to node4 (cell_beta)
        let result = router
            .route_message("node1", "node4", 100, MessagePriority::Normal)
            .await
            .unwrap();

        // Should be None because routing is invalid (cross-cell)
        assert!(result.is_none());

        // Flow control should not have been invoked
        let metrics = fc.get_metrics();
        assert_eq!(metrics.cell_messages_sent, 0);
    }

    #[tokio::test]
    async fn test_route_message_leader_to_zone() {
        use crate::hierarchy::flow_control::{BandwidthLimit, MessageDropPolicy};

        let table = setup_routing_table().await;
        let fc = Arc::new(FlowController::new(
            BandwidthLimit::new(100, 10000),
            BandwidthLimit::new(50, 5000),
            MessageDropPolicy::DropLowPriority,
        ));

        let router = HierarchicalRouter::with_flow_control(
            "node1".to_string(),
            Arc::new(RwLock::new(table)),
            fc.clone(),
        );

        // Leader routes to zone coordinator
        let result = router
            .route_message("node1", "zone_coordinator", 200, MessagePriority::High)
            .await
            .unwrap();

        // Should get permit (upward routing from leader is valid)
        assert!(result.is_some());

        // Should use zone-level flow control
        let metrics = fc.get_metrics();
        assert_eq!(metrics.zone_messages_sent, 1);
        assert_eq!(metrics.zone_bytes_sent, 200);
    }

    #[tokio::test]
    async fn test_priority_affects_flow_control() {
        use crate::hierarchy::flow_control::{BandwidthLimit, MessageDropPolicy};

        let table = setup_routing_table().await;
        let fc = Arc::new(FlowController::new(
            BandwidthLimit::new(10, 1000),
            BandwidthLimit::new(5, 500),
            MessageDropPolicy::DropLowPriority,
        ));

        let router = HierarchicalRouter::with_flow_control(
            "node1".to_string(),
            Arc::new(RwLock::new(table)),
            fc.clone(),
        );

        // Send critical priority message (consumes 0.5x tokens)
        let _r1 = router
            .route_message("node1", "node2", 100, MessagePriority::Critical)
            .await
            .unwrap();

        // Send low priority message (consumes 1.5x tokens)
        let _r2 = router
            .route_message("node2", "node3", 100, MessagePriority::Low)
            .await
            .unwrap();

        // Both should succeed, metrics should show both messages
        let metrics = fc.get_metrics();
        assert_eq!(metrics.cell_messages_sent, 2);
    }

    #[tokio::test]
    async fn test_router_without_flow_control() {
        let table = setup_routing_table().await;
        let router = HierarchicalRouter::new("node1".to_string(), Arc::new(RwLock::new(table)));

        // Route a message without flow control
        let result = router
            .route_message("node1", "node2", 100, MessagePriority::Normal)
            .await
            .unwrap();

        // Should be None (no permit needed, but routing is valid)
        assert!(result.is_none());

        // No backpressure without flow controller
        assert!(!router.has_backpressure().await);
    }
}
