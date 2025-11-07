//! Zone model for hierarchical coordination (E5 Phase 2)
//!
//! Zones represent the highest level of the three-tier hierarchy:
//! - Nodes: Individual platforms with capabilities
//! - Cells: Tactical groups of nodes working together
//! - Zones: Strategic coordination across multiple cells
//!
//! This module implements CRDT-based zone state management with:
//! - Cell membership (OR-Set)
//! - Zone coordinator assignment (LWW-Register)
//! - Capability aggregation (G-Set)

use crate::models::{Capability, CapabilityExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

/// Zone configuration (immutable properties)
///
/// # Example
/// ```
/// use cap_protocol::models::zone::ZoneConfig;
///
/// let config = ZoneConfig::new("zone_north".to_string(), 10);
/// assert_eq!(config.max_cells, 10);
/// assert_eq!(config.min_cells, 2);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZoneConfig {
    /// Unique zone identifier
    pub id: String,
    /// Maximum number of cells in this zone
    pub max_cells: usize,
    /// Minimum number of cells required for valid zone
    pub min_cells: usize,
    /// Creation timestamp
    pub created_at: u64,
}

impl ZoneConfig {
    /// Create a new zone configuration
    ///
    /// # Arguments
    /// * `id` - Unique zone identifier
    /// * `max_cells` - Maximum number of cells allowed in zone
    ///
    /// # Example
    /// ```
    /// use cap_protocol::models::zone::ZoneConfig;
    ///
    /// let config = ZoneConfig::new("zone_alpha".to_string(), 8);
    /// ```
    pub fn new(id: String, max_cells: usize) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id,
            max_cells,
            min_cells: 2, // Default minimum
            created_at,
        }
    }

    /// Create configuration with custom minimum cells
    pub fn with_min_cells(mut self, min_cells: usize) -> Self {
        self.min_cells = min_cells;
        self
    }
}

/// Zone runtime state (CRDT-based)
///
/// Uses multiple CRDT types for distributed consistency:
/// - Commander: LWW-Register (Last-Write-Wins)
/// - Cells: OR-Set (Observed-Remove Set)
/// - Capabilities: G-Set (Grow-only Set)
///
/// # Example
/// ```
/// use cap_protocol::models::zone::{ZoneConfig, ZoneState};
///
/// let config = ZoneConfig::new("zone_1".to_string(), 10);
/// let mut zone = ZoneState::new(config);
///
/// // Add cells to zone
/// zone.add_cell("cell_alpha".to_string());
/// zone.add_cell("cell_beta".to_string());
///
/// assert_eq!(zone.cells.len(), 2);
/// assert!(zone.is_valid()); // Meets minimum cells
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneState {
    /// Zone configuration
    pub config: ZoneConfig,
    /// Zone coordinator node ID (LWW-Register)
    pub coordinator_id: Option<String>,
    /// Cell membership (OR-Set)
    pub cells: HashSet<String>,
    /// Aggregated capabilities from all cells (G-Set)
    pub aggregated_capabilities: Vec<Capability>,
    /// Timestamp for LWW conflict resolution
    pub timestamp: u64,
}

impl ZoneState {
    /// Create a new zone state from configuration
    ///
    /// # Example
    /// ```
    /// use cap_protocol::models::zone::{ZoneConfig, ZoneState};
    ///
    /// let config = ZoneConfig::new("zone_north".to_string(), 5);
    /// let zone = ZoneState::new(config);
    /// ```
    pub fn new(config: ZoneConfig) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            config,
            coordinator_id: None,
            cells: HashSet::new(),
            aggregated_capabilities: Vec::new(),
            timestamp,
        }
    }

    /// Add a cell to the zone (OR-Set add operation)
    ///
    /// Returns `true` if cell was added, `false` if already present or zone is full.
    ///
    /// # Example
    /// ```
    /// use cap_protocol::models::zone::{ZoneConfig, ZoneState};
    ///
    /// let config = ZoneConfig::new("zone_1".to_string(), 3);
    /// let mut zone = ZoneState::new(config);
    ///
    /// assert!(zone.add_cell("cell_1".to_string()));
    /// assert!(!zone.add_cell("cell_1".to_string())); // Already present
    /// ```
    pub fn add_cell(&mut self, cell_id: String) -> bool {
        if self.is_full() {
            return false;
        }

        if self.cells.insert(cell_id) {
            self.update_timestamp();
            true
        } else {
            false
        }
    }

    /// Remove a cell from the zone (OR-Set remove operation)
    ///
    /// Returns `true` if cell was removed, `false` if not present.
    pub fn remove_cell(&mut self, cell_id: &str) -> bool {
        if self.cells.remove(cell_id) {
            self.update_timestamp();
            true
        } else {
            false
        }
    }

    /// Set the zone coordinator (LWW-Register operation)
    ///
    /// The coordinator must be a leader of a cell within this zone.
    ///
    /// # Arguments
    /// * `coordinator_id` - Node ID of the coordinator
    /// * `timestamp` - Logical timestamp for conflict resolution
    ///
    /// # Returns
    /// `true` if assignment was applied, `false` if rejected due to older timestamp
    pub fn set_coordinator(&mut self, coordinator_id: String, timestamp: u64) -> bool {
        if timestamp < self.timestamp {
            return false;
        }

        self.coordinator_id = Some(coordinator_id);
        self.timestamp = timestamp;
        true
    }

    /// Remove the zone coordinator (LWW-Register deletion)
    pub fn remove_coordinator(&mut self, timestamp: u64) -> bool {
        if timestamp < self.timestamp {
            return false;
        }

        self.coordinator_id = None;
        self.timestamp = timestamp;
        true
    }

    /// Add an aggregated capability (G-Set add operation)
    ///
    /// Capabilities are grow-only - once added, they cannot be removed.
    pub fn add_capability(&mut self, capability: Capability) {
        // Check if capability already exists
        if !self
            .aggregated_capabilities
            .iter()
            .any(|c| c.get_capability_type() == capability.get_capability_type())
        {
            self.aggregated_capabilities.push(capability);
            self.update_timestamp();
        }
    }

    /// Check if zone meets minimum cell requirement
    pub fn is_valid(&self) -> bool {
        self.cells.len() >= self.config.min_cells
    }

    /// Check if zone is at maximum capacity
    pub fn is_full(&self) -> bool {
        self.cells.len() >= self.config.max_cells
    }

    /// Get the number of cells in this zone
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Check if a specific cell is a member of this zone
    pub fn contains_cell(&self, cell_id: &str) -> bool {
        self.cells.contains(cell_id)
    }

    /// Merge another zone state into this one (CRDT merge)
    ///
    /// Applies CRDT semantics for each component:
    /// - Coordinator: LWW based on timestamp
    /// - Cells: OR-Set union
    /// - Capabilities: G-Set union
    ///
    /// # Panics
    /// Panics if attempting to merge zones with different IDs
    pub fn merge(&mut self, other: &ZoneState) {
        assert_eq!(
            self.config.id, other.config.id,
            "Cannot merge zones with different IDs"
        );

        // LWW-Register merge for coordinator
        if other.timestamp > self.timestamp {
            self.coordinator_id = other.coordinator_id.clone();
            self.timestamp = other.timestamp;
        }

        // OR-Set merge for cells
        for cell_id in &other.cells {
            self.cells.insert(cell_id.clone());
        }

        // G-Set merge for capabilities
        for capability in &other.aggregated_capabilities {
            if !self
                .aggregated_capabilities
                .iter()
                .any(|c| c.get_capability_type() == capability.get_capability_type())
            {
                self.aggregated_capabilities.push(capability.clone());
            }
        }
    }

    /// Update timestamp to current time
    fn update_timestamp(&mut self) {
        self.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Get zone statistics
    pub fn stats(&self) -> ZoneStats {
        ZoneStats {
            zone_id: self.config.id.clone(),
            cell_count: self.cells.len(),
            coordinator: self.coordinator_id.clone(),
            capability_count: self.aggregated_capabilities.len(),
            is_valid: self.is_valid(),
            is_full: self.is_full(),
        }
    }
}

/// Statistics about a zone's current state
#[derive(Debug, Clone)]
pub struct ZoneStats {
    /// Zone identifier
    pub zone_id: String,
    /// Number of cells in zone
    pub cell_count: usize,
    /// Current coordinator (if any)
    pub coordinator: Option<String>,
    /// Number of aggregated capabilities
    pub capability_count: usize,
    /// Whether zone meets minimum requirements
    pub is_valid: bool,
    /// Whether zone is at capacity
    pub is_full: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CapabilityType;

    #[test]
    fn test_zone_config_creation() {
        let config = ZoneConfig::new("zone_test".to_string(), 10);
        assert_eq!(config.id, "zone_test");
        assert_eq!(config.max_cells, 10);
        assert_eq!(config.min_cells, 2);
    }

    #[test]
    fn test_zone_config_custom_min() {
        let config = ZoneConfig::new("zone_test".to_string(), 10).with_min_cells(3);
        assert_eq!(config.min_cells, 3);
    }

    #[test]
    fn test_zone_state_creation() {
        let config = ZoneConfig::new("zone_1".to_string(), 5);
        let zone = ZoneState::new(config);

        assert_eq!(zone.config.id, "zone_1");
        assert_eq!(zone.cells.len(), 0);
        assert!(zone.coordinator_id.is_none());
        assert!(!zone.is_valid()); // Not enough cells
    }

    #[test]
    fn test_add_cell() {
        let config = ZoneConfig::new("zone_1".to_string(), 5);
        let mut zone = ZoneState::new(config);

        assert!(zone.add_cell("cell_1".to_string()));
        assert!(zone.add_cell("cell_2".to_string()));
        assert_eq!(zone.cell_count(), 2);

        // Duplicate add should return false
        assert!(!zone.add_cell("cell_1".to_string()));
        assert_eq!(zone.cell_count(), 2);
    }

    #[test]
    fn test_remove_cell() {
        let config = ZoneConfig::new("zone_1".to_string(), 5);
        let mut zone = ZoneState::new(config);

        zone.add_cell("cell_1".to_string());
        zone.add_cell("cell_2".to_string());

        assert!(zone.remove_cell("cell_1"));
        assert_eq!(zone.cell_count(), 1);

        // Removing non-existent cell should return false
        assert!(!zone.remove_cell("cell_3"));
    }

    #[test]
    fn test_zone_capacity() {
        let config = ZoneConfig::new("zone_1".to_string(), 3);
        let mut zone = ZoneState::new(config);

        assert!(zone.add_cell("cell_1".to_string()));
        assert!(zone.add_cell("cell_2".to_string()));
        assert!(zone.add_cell("cell_3".to_string()));

        assert!(zone.is_full());
        assert!(!zone.add_cell("cell_4".to_string())); // Should fail - at capacity
    }

    #[test]
    fn test_zone_validity() {
        let config = ZoneConfig::new("zone_1".to_string(), 5).with_min_cells(2);
        let mut zone = ZoneState::new(config);

        assert!(!zone.is_valid()); // 0 cells

        zone.add_cell("cell_1".to_string());
        assert!(!zone.is_valid()); // 1 cell, needs 2

        zone.add_cell("cell_2".to_string());
        assert!(zone.is_valid()); // 2 cells, meets minimum
    }

    #[test]
    fn test_set_coordinator() {
        let config = ZoneConfig::new("zone_1".to_string(), 5);
        let mut zone = ZoneState::new(config);

        let initial_ts = zone.timestamp;
        let ts1 = initial_ts + 100;
        assert!(zone.set_coordinator("node_1".to_string(), ts1));
        assert_eq!(zone.coordinator_id, Some("node_1".to_string()));

        // Older timestamp should be rejected
        assert!(!zone.set_coordinator("node_2".to_string(), initial_ts + 50));
        assert_eq!(zone.coordinator_id, Some("node_1".to_string()));

        // Newer timestamp should win
        assert!(zone.set_coordinator("node_2".to_string(), ts1 + 100));
        assert_eq!(zone.coordinator_id, Some("node_2".to_string()));
    }

    #[test]
    fn test_remove_coordinator() {
        let config = ZoneConfig::new("zone_1".to_string(), 5);
        let mut zone = ZoneState::new(config);

        let initial_ts = zone.timestamp;
        zone.set_coordinator("node_1".to_string(), initial_ts + 100);

        // Old timestamp should be rejected
        assert!(!zone.remove_coordinator(initial_ts + 50));
        assert_eq!(zone.coordinator_id, Some("node_1".to_string()));

        // Newer timestamp should succeed
        assert!(zone.remove_coordinator(initial_ts + 200));
        assert_eq!(zone.coordinator_id, None);
    }

    #[test]
    fn test_add_capability() {
        let config = ZoneConfig::new("zone_1".to_string(), 5);
        let mut zone = ZoneState::new(config);

        let cap1 = Capability::new(
            "cap_1".to_string(),
            "Sensor Capability".to_string(),
            CapabilityType::Sensor,
            0.9,
        );

        zone.add_capability(cap1.clone());
        assert_eq!(zone.aggregated_capabilities.len(), 1);

        // Adding same capability type again should not duplicate
        zone.add_capability(cap1.clone());
        assert_eq!(zone.aggregated_capabilities.len(), 1);

        let cap2 = Capability::new(
            "cap_2".to_string(),
            "Payload Capability".to_string(),
            CapabilityType::Payload,
            0.8,
        );

        zone.add_capability(cap2);
        assert_eq!(zone.aggregated_capabilities.len(), 2);
    }

    #[test]
    fn test_contains_cell() {
        let config = ZoneConfig::new("zone_1".to_string(), 5);
        let mut zone = ZoneState::new(config);

        zone.add_cell("cell_1".to_string());
        assert!(zone.contains_cell("cell_1"));
        assert!(!zone.contains_cell("cell_2"));
    }

    #[test]
    fn test_merge_zones() {
        let config1 = ZoneConfig::new("zone_1".to_string(), 10);
        let mut zone1 = ZoneState::new(config1);

        let initial_ts = zone1.timestamp;

        zone1.add_cell("cell_1".to_string());
        zone1.add_cell("cell_2".to_string());
        zone1.set_coordinator("node_1".to_string(), initial_ts + 100);

        let config2 = ZoneConfig::new("zone_1".to_string(), 10);
        let mut zone2 = ZoneState::new(config2);

        zone2.add_cell("cell_2".to_string()); // Duplicate
        zone2.add_cell("cell_3".to_string()); // New
        zone2.set_coordinator("node_2".to_string(), initial_ts + 200); // Newer

        zone1.merge(&zone2);

        // Should have union of cells
        assert_eq!(zone1.cell_count(), 3);
        assert!(zone1.contains_cell("cell_1"));
        assert!(zone1.contains_cell("cell_2"));
        assert!(zone1.contains_cell("cell_3"));

        // Should have newer coordinator
        assert_eq!(zone1.coordinator_id, Some("node_2".to_string()));
    }

    #[test]
    fn test_merge_capabilities() {
        let config1 = ZoneConfig::new("zone_1".to_string(), 10);
        let mut zone1 = ZoneState::new(config1);

        let cap1 = Capability::new(
            "cap_1".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        zone1.add_capability(cap1);

        let config2 = ZoneConfig::new("zone_1".to_string(), 10);
        let mut zone2 = ZoneState::new(config2);

        let cap2 = Capability::new(
            "cap_2".to_string(),
            "Payload".to_string(),
            CapabilityType::Payload,
            0.8,
        );
        zone2.add_capability(cap2);

        zone1.merge(&zone2);

        // Should have both capabilities
        assert_eq!(zone1.aggregated_capabilities.len(), 2);
    }

    #[test]
    #[should_panic(expected = "Cannot merge zones with different IDs")]
    fn test_merge_different_zones_panics() {
        let config1 = ZoneConfig::new("zone_1".to_string(), 10);
        let mut zone1 = ZoneState::new(config1);

        let config2 = ZoneConfig::new("zone_2".to_string(), 10);
        let zone2 = ZoneState::new(config2);

        zone1.merge(&zone2); // Should panic
    }

    #[test]
    fn test_zone_stats() {
        let config = ZoneConfig::new("zone_test".to_string(), 5).with_min_cells(2);
        let mut zone = ZoneState::new(config);

        let initial_ts = zone.timestamp;

        zone.add_cell("cell_1".to_string());
        zone.add_cell("cell_2".to_string());
        zone.set_coordinator("node_1".to_string(), initial_ts + 100);

        let stats = zone.stats();
        assert_eq!(stats.zone_id, "zone_test");
        assert_eq!(stats.cell_count, 2);
        assert_eq!(stats.coordinator, Some("node_1".to_string()));
        assert!(stats.is_valid);
        assert!(!stats.is_full);
    }
}
