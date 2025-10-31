//! Routing table for hierarchical message routing
//!
//! This module provides the core routing table that manages the hierarchical
//! relationships between nodes, cells, and zones. It uses CRDT semantics
//! (Last-Write-Wins) for conflict resolution in distributed scenarios.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, instrument};

/// Routing table managing node→cell→zone hierarchy
///
/// This structure maintains the routing information for the entire system:
/// - Which cell each node belongs to
/// - Which zone each cell belongs to
/// - Which node is the leader of each cell
///
/// All assignments use Last-Write-Wins (LWW) CRDT semantics for distributed
/// conflict resolution.
///
/// # Example
/// ```
/// use cap_protocol::hierarchy::RoutingTable;
///
/// let mut table = RoutingTable::new();
///
/// // Assign nodes to cells
/// table.assign_node("node1", "cell_alpha", 100);
/// table.assign_node("node2", "cell_alpha", 101);
///
/// // Assign cell to zone
/// table.assign_cell("cell_alpha", "zone_north", 102);
///
/// // Set cell leader
/// table.set_cell_leader("cell_alpha", "node1", 103);
///
/// // Query routing
/// assert_eq!(table.get_node_cell("node1"), Some("cell_alpha"));
/// assert_eq!(table.get_cell_zone("cell_alpha"), Some("zone_north"));
/// assert_eq!(table.get_node_zone("node1"), Some("zone_north"));
/// assert!(table.is_cell_leader("node1", "cell_alpha"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingTable {
    /// Maps node_id → (cell_id, timestamp)
    node_to_cell: HashMap<String, (String, u64)>,
    /// Maps cell_id → (zone_id, timestamp)
    cell_to_zone: HashMap<String, (String, u64)>,
    /// Maps cell_id → (leader_node_id, timestamp)
    cell_leaders: HashMap<String, (String, u64)>,
}

impl RoutingTable {
    /// Create a new empty routing table
    pub fn new() -> Self {
        Self {
            node_to_cell: HashMap::new(),
            cell_to_zone: HashMap::new(),
            cell_leaders: HashMap::new(),
        }
    }

    /// Assign a node to a cell (LWW-Register operation)
    ///
    /// # Arguments
    /// * `node_id` - The node to assign
    /// * `cell_id` - The cell to assign to
    /// * `timestamp` - Logical timestamp for LWW conflict resolution
    ///
    /// # Returns
    /// `true` if the assignment was applied, `false` if rejected due to older timestamp
    #[instrument(skip(self))]
    pub fn assign_node(&mut self, node_id: &str, cell_id: &str, timestamp: u64) -> bool {
        if let Some((_, existing_ts)) = self.node_to_cell.get(node_id) {
            if timestamp <= *existing_ts {
                debug!(
                    "Rejected node assignment (old timestamp): {} → {} @ {}",
                    node_id, cell_id, timestamp
                );
                return false;
            }
        }

        debug!(
            "Assigning node to cell: {} → {} @ {}",
            node_id, cell_id, timestamp
        );
        self.node_to_cell
            .insert(node_id.to_string(), (cell_id.to_string(), timestamp));
        true
    }

    /// Remove a node from its cell (LWW-Register deletion)
    ///
    /// # Arguments
    /// * `node_id` - The node to remove
    /// * `timestamp` - Logical timestamp for LWW conflict resolution
    ///
    /// # Returns
    /// `true` if the removal was applied, `false` if rejected due to older timestamp
    #[instrument(skip(self))]
    pub fn remove_node(&mut self, node_id: &str, timestamp: u64) -> bool {
        if let Some((_, existing_ts)) = self.node_to_cell.get(node_id) {
            if timestamp < *existing_ts {
                debug!(
                    "Rejected node removal (old timestamp): {} @ {}",
                    node_id, timestamp
                );
                return false;
            }
        }

        debug!("Removing node from cell: {} @ {}", node_id, timestamp);
        self.node_to_cell.remove(node_id);
        true
    }

    /// Assign a cell to a zone (LWW-Register operation)
    ///
    /// # Arguments
    /// * `cell_id` - The cell to assign
    /// * `zone_id` - The zone to assign to
    /// * `timestamp` - Logical timestamp for LWW conflict resolution
    ///
    /// # Returns
    /// `true` if the assignment was applied, `false` if rejected due to older timestamp
    #[instrument(skip(self))]
    pub fn assign_cell(&mut self, cell_id: &str, zone_id: &str, timestamp: u64) -> bool {
        if let Some((_, existing_ts)) = self.cell_to_zone.get(cell_id) {
            if timestamp <= *existing_ts {
                debug!(
                    "Rejected cell assignment (old timestamp): {} → {} @ {}",
                    cell_id, zone_id, timestamp
                );
                return false;
            }
        }

        debug!(
            "Assigning cell to zone: {} → {} @ {}",
            cell_id, zone_id, timestamp
        );
        self.cell_to_zone
            .insert(cell_id.to_string(), (zone_id.to_string(), timestamp));
        true
    }

    /// Remove a cell from its zone (LWW-Register deletion)
    ///
    /// # Arguments
    /// * `cell_id` - The cell to remove
    /// * `timestamp` - Logical timestamp for LWW conflict resolution
    ///
    /// # Returns
    /// `true` if the removal was applied, `false` if rejected due to older timestamp
    #[instrument(skip(self))]
    pub fn remove_cell(&mut self, cell_id: &str, timestamp: u64) -> bool {
        if let Some((_, existing_ts)) = self.cell_to_zone.get(cell_id) {
            if timestamp < *existing_ts {
                debug!(
                    "Rejected cell removal (old timestamp): {} @ {}",
                    cell_id, timestamp
                );
                return false;
            }
        }

        debug!("Removing cell from zone: {} @ {}", cell_id, timestamp);
        self.cell_to_zone.remove(cell_id);
        true
    }

    /// Set the leader of a cell (LWW-Register operation)
    ///
    /// # Arguments
    /// * `cell_id` - The cell
    /// * `leader_node_id` - The node to designate as leader
    /// * `timestamp` - Logical timestamp for LWW conflict resolution
    ///
    /// # Returns
    /// `true` if the assignment was applied, `false` if rejected due to older timestamp
    #[instrument(skip(self))]
    pub fn set_cell_leader(&mut self, cell_id: &str, leader_node_id: &str, timestamp: u64) -> bool {
        if let Some((_, existing_ts)) = self.cell_leaders.get(cell_id) {
            if timestamp <= *existing_ts {
                debug!(
                    "Rejected leader assignment (old timestamp): {} → {} @ {}",
                    cell_id, leader_node_id, timestamp
                );
                return false;
            }
        }

        debug!(
            "Setting cell leader: {} → {} @ {}",
            cell_id, leader_node_id, timestamp
        );
        self.cell_leaders
            .insert(cell_id.to_string(), (leader_node_id.to_string(), timestamp));
        true
    }

    /// Remove the leader designation from a cell (LWW-Register deletion)
    ///
    /// # Arguments
    /// * `cell_id` - The cell to remove leader from
    /// * `timestamp` - Logical timestamp for LWW conflict resolution
    ///
    /// # Returns
    /// `true` if the removal was applied, `false` if rejected due to older timestamp
    #[instrument(skip(self))]
    pub fn remove_cell_leader(&mut self, cell_id: &str, timestamp: u64) -> bool {
        if let Some((_, existing_ts)) = self.cell_leaders.get(cell_id) {
            if timestamp < *existing_ts {
                debug!(
                    "Rejected leader removal (old timestamp): {} @ {}",
                    cell_id, timestamp
                );
                return false;
            }
        }

        debug!("Removing cell leader: {} @ {}", cell_id, timestamp);
        self.cell_leaders.remove(cell_id);
        true
    }

    /// Get the cell ID for a given node
    ///
    /// Returns None if the node is not assigned to any cell.
    pub fn get_node_cell(&self, node_id: &str) -> Option<&str> {
        self.node_to_cell
            .get(node_id)
            .map(|(cell_id, _)| cell_id.as_str())
    }

    /// Get the zone ID for a given cell
    ///
    /// Returns None if the cell is not assigned to any zone.
    pub fn get_cell_zone(&self, cell_id: &str) -> Option<&str> {
        self.cell_to_zone
            .get(cell_id)
            .map(|(zone_id, _)| zone_id.as_str())
    }

    /// Get the zone ID for a given node (by following node→cell→zone)
    ///
    /// Returns None if the node is not in a cell, or the cell is not in a zone.
    pub fn get_node_zone(&self, node_id: &str) -> Option<&str> {
        let cell_id = self.get_node_cell(node_id)?;
        self.get_cell_zone(cell_id)
    }

    /// Check if a node is the leader of a given cell
    ///
    /// Returns true if the node is designated as the cell's leader.
    pub fn is_cell_leader(&self, node_id: &str, cell_id: &str) -> bool {
        self.cell_leaders
            .get(cell_id)
            .map(|(leader_id, _)| leader_id == node_id)
            .unwrap_or(false)
    }

    /// Get the leader node ID for a given cell
    ///
    /// Returns None if the cell has no designated leader.
    pub fn get_cell_leader(&self, cell_id: &str) -> Option<&str> {
        self.cell_leaders
            .get(cell_id)
            .map(|(leader_id, _)| leader_id.as_str())
    }

    /// Merge another routing table into this one (CRDT merge)
    ///
    /// Applies Last-Write-Wins semantics for all entries. Higher timestamps win.
    #[instrument(skip(self, other))]
    pub fn merge(&mut self, other: &RoutingTable) {
        debug!("Merging routing tables");

        // Merge node→cell mappings
        for (node_id, (cell_id, timestamp)) in &other.node_to_cell {
            self.assign_node(node_id, cell_id, *timestamp);
        }

        // Merge cell→zone mappings
        for (cell_id, (zone_id, timestamp)) in &other.cell_to_zone {
            self.assign_cell(cell_id, zone_id, *timestamp);
        }

        // Merge cell leaders
        for (cell_id, (leader_id, timestamp)) in &other.cell_leaders {
            self.set_cell_leader(cell_id, leader_id, *timestamp);
        }
    }

    /// Get statistics about the routing table
    pub fn stats(&self) -> RoutingTableStats {
        RoutingTableStats {
            node_count: self.node_to_cell.len(),
            cell_count: self.cell_to_zone.len(),
            leader_count: self.cell_leaders.len(),
        }
    }

    /// Get all nodes in a specific cell
    pub fn get_cell_nodes(&self, cell_id: &str) -> Vec<&str> {
        self.node_to_cell
            .iter()
            .filter(|(_, (cid, _))| cid == cell_id)
            .map(|(node_id, _)| node_id.as_str())
            .collect()
    }

    /// Get all cells in a specific zone
    pub fn get_zone_cells(&self, zone_id: &str) -> Vec<&str> {
        self.cell_to_zone
            .iter()
            .filter(|(_, (zid, _))| zid == zone_id)
            .map(|(cell_id, _)| cell_id.as_str())
            .collect()
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the routing table
#[derive(Debug, Clone)]
pub struct RoutingTableStats {
    /// Number of nodes with cell assignments
    pub node_count: usize,
    /// Number of cells with zone assignments
    pub cell_count: usize,
    /// Number of cells with designated leaders
    pub leader_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_table_creation() {
        let table = RoutingTable::new();
        let stats = table.stats();
        assert_eq!(stats.node_count, 0);
        assert_eq!(stats.cell_count, 0);
        assert_eq!(stats.leader_count, 0);
    }

    #[test]
    fn test_node_assignment() {
        let mut table = RoutingTable::new();

        // Assign node to cell
        assert!(table.assign_node("node1", "cell_alpha", 100));
        assert_eq!(table.get_node_cell("node1"), Some("cell_alpha"));

        // Assign another node to same cell
        assert!(table.assign_node("node2", "cell_alpha", 101));
        assert_eq!(table.get_node_cell("node2"), Some("cell_alpha"));

        // Check stats
        let stats = table.stats();
        assert_eq!(stats.node_count, 2);
    }

    #[test]
    fn test_lww_semantics_node_assignment() {
        let mut table = RoutingTable::new();

        // Initial assignment
        assert!(table.assign_node("node1", "cell_alpha", 100));
        assert_eq!(table.get_node_cell("node1"), Some("cell_alpha"));

        // Later assignment should win
        assert!(table.assign_node("node1", "cell_beta", 200));
        assert_eq!(table.get_node_cell("node1"), Some("cell_beta"));

        // Earlier assignment should be rejected
        assert!(!table.assign_node("node1", "cell_gamma", 150));
        assert_eq!(table.get_node_cell("node1"), Some("cell_beta"));

        // Equal timestamp should be rejected
        assert!(!table.assign_node("node1", "cell_delta", 200));
        assert_eq!(table.get_node_cell("node1"), Some("cell_beta"));
    }

    #[test]
    fn test_cell_assignment() {
        let mut table = RoutingTable::new();

        // Assign cell to zone
        assert!(table.assign_cell("cell_alpha", "zone_north", 100));
        assert_eq!(table.get_cell_zone("cell_alpha"), Some("zone_north"));

        let stats = table.stats();
        assert_eq!(stats.cell_count, 1);
    }

    #[test]
    fn test_lww_semantics_cell_assignment() {
        let mut table = RoutingTable::new();

        // Initial assignment
        assert!(table.assign_cell("cell_alpha", "zone_north", 100));
        assert_eq!(table.get_cell_zone("cell_alpha"), Some("zone_north"));

        // Later assignment should win
        assert!(table.assign_cell("cell_alpha", "zone_south", 200));
        assert_eq!(table.get_cell_zone("cell_alpha"), Some("zone_south"));

        // Earlier assignment should be rejected
        assert!(!table.assign_cell("cell_alpha", "zone_east", 150));
        assert_eq!(table.get_cell_zone("cell_alpha"), Some("zone_south"));
    }

    #[test]
    fn test_cell_leader() {
        let mut table = RoutingTable::new();

        // Assign nodes to cell
        table.assign_node("node1", "cell_alpha", 100);
        table.assign_node("node2", "cell_alpha", 101);

        // Set leader
        assert!(table.set_cell_leader("cell_alpha", "node1", 102));
        assert!(table.is_cell_leader("node1", "cell_alpha"));
        assert!(!table.is_cell_leader("node2", "cell_alpha"));
        assert_eq!(table.get_cell_leader("cell_alpha"), Some("node1"));

        let stats = table.stats();
        assert_eq!(stats.leader_count, 1);
    }

    #[test]
    fn test_lww_semantics_leader() {
        let mut table = RoutingTable::new();

        // Initial leader
        assert!(table.set_cell_leader("cell_alpha", "node1", 100));
        assert!(table.is_cell_leader("node1", "cell_alpha"));

        // Later assignment should win
        assert!(table.set_cell_leader("cell_alpha", "node2", 200));
        assert!(!table.is_cell_leader("node1", "cell_alpha"));
        assert!(table.is_cell_leader("node2", "cell_alpha"));

        // Earlier assignment should be rejected
        assert!(!table.set_cell_leader("cell_alpha", "node3", 150));
        assert!(table.is_cell_leader("node2", "cell_alpha"));
    }

    #[test]
    fn test_node_zone_lookup() {
        let mut table = RoutingTable::new();

        // Set up hierarchy: node1 → cell_alpha → zone_north
        table.assign_node("node1", "cell_alpha", 100);
        table.assign_cell("cell_alpha", "zone_north", 101);

        // Should traverse the hierarchy
        assert_eq!(table.get_node_zone("node1"), Some("zone_north"));

        // Node without cell
        assert_eq!(table.get_node_zone("node2"), None);

        // Node in cell without zone
        table.assign_node("node3", "cell_beta", 102);
        assert_eq!(table.get_node_zone("node3"), None);
    }

    #[test]
    fn test_get_cell_nodes() {
        let mut table = RoutingTable::new();

        table.assign_node("node1", "cell_alpha", 100);
        table.assign_node("node2", "cell_alpha", 101);
        table.assign_node("node3", "cell_beta", 102);

        let mut alpha_nodes = table.get_cell_nodes("cell_alpha");
        alpha_nodes.sort();
        assert_eq!(alpha_nodes, vec!["node1", "node2"]);

        let beta_nodes = table.get_cell_nodes("cell_beta");
        assert_eq!(beta_nodes, vec!["node3"]);

        let empty_nodes = table.get_cell_nodes("cell_gamma");
        assert_eq!(empty_nodes.len(), 0);
    }

    #[test]
    fn test_get_zone_cells() {
        let mut table = RoutingTable::new();

        table.assign_cell("cell_alpha", "zone_north", 100);
        table.assign_cell("cell_beta", "zone_north", 101);
        table.assign_cell("cell_gamma", "zone_south", 102);

        let mut north_cells = table.get_zone_cells("zone_north");
        north_cells.sort();
        assert_eq!(north_cells, vec!["cell_alpha", "cell_beta"]);

        let south_cells = table.get_zone_cells("zone_south");
        assert_eq!(south_cells, vec!["cell_gamma"]);

        let empty_cells = table.get_zone_cells("zone_east");
        assert_eq!(empty_cells.len(), 0);
    }

    #[test]
    fn test_node_removal() {
        let mut table = RoutingTable::new();

        table.assign_node("node1", "cell_alpha", 100);
        assert_eq!(table.get_node_cell("node1"), Some("cell_alpha"));

        // Remove node with old timestamp should fail
        assert!(!table.remove_node("node1", 50));
        assert_eq!(table.get_node_cell("node1"), Some("cell_alpha"));

        // Remove node with newer timestamp should succeed
        assert!(table.remove_node("node1", 200));
        assert_eq!(table.get_node_cell("node1"), None);
    }

    #[test]
    fn test_cell_removal() {
        let mut table = RoutingTable::new();

        table.assign_cell("cell_alpha", "zone_north", 100);
        assert_eq!(table.get_cell_zone("cell_alpha"), Some("zone_north"));

        // Remove cell with old timestamp should fail
        assert!(!table.remove_cell("cell_alpha", 50));
        assert_eq!(table.get_cell_zone("cell_alpha"), Some("zone_north"));

        // Remove cell with newer timestamp should succeed
        assert!(table.remove_cell("cell_alpha", 200));
        assert_eq!(table.get_cell_zone("cell_alpha"), None);
    }

    #[test]
    fn test_leader_removal() {
        let mut table = RoutingTable::new();

        table.set_cell_leader("cell_alpha", "node1", 100);
        assert_eq!(table.get_cell_leader("cell_alpha"), Some("node1"));

        // Remove leader with old timestamp should fail
        assert!(!table.remove_cell_leader("cell_alpha", 50));
        assert_eq!(table.get_cell_leader("cell_alpha"), Some("node1"));

        // Remove leader with newer timestamp should succeed
        assert!(table.remove_cell_leader("cell_alpha", 200));
        assert_eq!(table.get_cell_leader("cell_alpha"), None);
    }

    #[test]
    fn test_merge() {
        let mut table1 = RoutingTable::new();
        let mut table2 = RoutingTable::new();

        // Table 1 has older data
        table1.assign_node("node1", "cell_alpha", 100);
        table1.assign_cell("cell_alpha", "zone_north", 100);

        // Table 2 has newer data for node1 and different data for node2
        table2.assign_node("node1", "cell_beta", 200);
        table2.assign_node("node2", "cell_gamma", 150);
        table2.assign_cell("cell_beta", "zone_south", 200);

        // Merge table2 into table1
        table1.merge(&table2);

        // Newer timestamp should win for node1
        assert_eq!(table1.get_node_cell("node1"), Some("cell_beta"));

        // Node2 should be added
        assert_eq!(table1.get_node_cell("node2"), Some("cell_gamma"));

        // Cell assignments should be merged
        assert_eq!(table1.get_cell_zone("cell_beta"), Some("zone_south"));
        assert_eq!(table1.get_cell_zone("cell_alpha"), Some("zone_north"));
    }

    #[test]
    fn test_merge_leaders() {
        let mut table1 = RoutingTable::new();
        let mut table2 = RoutingTable::new();

        table1.set_cell_leader("cell_alpha", "node1", 100);
        table2.set_cell_leader("cell_alpha", "node2", 200);

        table1.merge(&table2);

        // Newer timestamp should win
        assert!(table1.is_cell_leader("node2", "cell_alpha"));
        assert!(!table1.is_cell_leader("node1", "cell_alpha"));
    }
}
