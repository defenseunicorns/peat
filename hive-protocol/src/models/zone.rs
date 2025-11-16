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
//!
//! ## Protobuf Integration
//!
//! This module uses protobuf types from `hive_schema::zone::v1` for multi-transport
//! support and cross-language compatibility. Extension traits provide CRDT semantics
//! and helper methods on top of the protobuf types.

use crate::models::{Capability, CapabilityExt};
use std::time::{SystemTime, UNIX_EPOCH};

// Re-export protobuf types
pub use hive_schema::zone::v1::{ZoneConfig, ZoneState, ZoneStats};

/// Extension trait for ZoneConfig helper methods
///
/// # Example
/// ```
/// use hive_protocol::models::zone::{ZoneConfig, ZoneConfigExt};
///
/// let config = ZoneConfig::new("zone_north".to_string(), 10);
/// assert_eq!(config.max_cells, 10);
/// assert_eq!(config.min_cells, 2);
/// ```
pub trait ZoneConfigExt {
    /// Create a new zone configuration
    ///
    /// # Arguments
    /// * `id` - Unique zone identifier
    /// * `max_cells` - Maximum number of cells allowed in zone
    ///
    /// # Example
    /// ```
    /// use hive_protocol::models::zone::{ZoneConfig, ZoneConfigExt};
    ///
    /// let config = ZoneConfig::new("zone_alpha".to_string(), 8);
    /// ```
    fn new(id: String, max_cells: u32) -> Self;

    /// Create configuration with custom minimum cells
    fn with_min_cells(self, min_cells: u32) -> Self;
}

impl ZoneConfigExt for ZoneConfig {
    fn new(id: String, max_cells: u32) -> Self {
        Self {
            id,
            max_cells,
            min_cells: 2, // Default minimum
            created_at: Some(hive_schema::common::v1::Timestamp {
                seconds: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        }
    }

    fn with_min_cells(mut self, min_cells: u32) -> Self {
        self.min_cells = min_cells;
        self
    }
}

/// Extension trait for ZoneState with CRDT operations
///
/// Uses multiple CRDT types for distributed consistency:
/// - Commander: LWW-Register (Last-Write-Wins)
/// - Cells: OR-Set (Observed-Remove Set)
/// - Capabilities: G-Set (Grow-only Set)
///
/// # Example
/// ```
/// use hive_protocol::models::zone::{ZoneConfig, ZoneConfigExt, ZoneState, ZoneStateExt};
///
/// let config = ZoneConfig::new("zone_1".to_string(), 10);
/// let mut zone = ZoneState::new(config);
///
/// // Add cells to zone
/// zone.add_cell("cell_alpha".to_string());
/// zone.add_cell("cell_beta".to_string());
///
/// assert_eq!(zone.cell_count(), 2);
/// assert!(zone.is_valid()); // Meets minimum cells
/// ```
pub trait ZoneStateExt {
    /// Create a new zone state from configuration
    ///
    /// # Example
    /// ```
    /// use hive_protocol::models::zone::{ZoneConfig, ZoneConfigExt, ZoneState, ZoneStateExt};
    ///
    /// let config = ZoneConfig::new("zone_north".to_string(), 5);
    /// let zone = ZoneState::new(config);
    /// ```
    fn new(config: ZoneConfig) -> Self;

    /// Add a cell to the zone (OR-Set add operation)
    ///
    /// Returns `true` if cell was added, `false` if already present or zone is full.
    ///
    /// # Example
    /// ```
    /// use hive_protocol::models::zone::{ZoneConfig, ZoneConfigExt, ZoneState, ZoneStateExt};
    ///
    /// let config = ZoneConfig::new("zone_1".to_string(), 3);
    /// let mut zone = ZoneState::new(config);
    ///
    /// assert!(zone.add_cell("cell_1".to_string()));
    /// assert!(!zone.add_cell("cell_1".to_string())); // Already present
    /// ```
    fn add_cell(&mut self, cell_id: String) -> bool;

    /// Remove a cell from the zone (OR-Set remove operation)
    ///
    /// Returns `true` if cell was removed, `false` if not present.
    fn remove_cell(&mut self, cell_id: &str) -> bool;

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
    fn set_coordinator(&mut self, coordinator_id: String, timestamp: u64) -> bool;

    /// Remove the zone coordinator (LWW-Register deletion)
    fn remove_coordinator(&mut self, timestamp: u64) -> bool;

    /// Add an aggregated capability (G-Set add operation)
    ///
    /// Capabilities are grow-only - once added, they cannot be removed.
    fn add_capability(&mut self, capability: Capability);

    /// Check if zone meets minimum cell requirement
    fn is_valid(&self) -> bool;

    /// Check if zone is at maximum capacity
    fn is_full(&self) -> bool;

    /// Get the number of cells in this zone
    fn cell_count(&self) -> usize;

    /// Check if a specific cell is a member of this zone
    fn contains_cell(&self, cell_id: &str) -> bool;

    /// Merge another zone state into this one (CRDT merge)
    ///
    /// Applies CRDT semantics for each component:
    /// - Coordinator: LWW based on timestamp
    /// - Cells: OR-Set union
    /// - Capabilities: G-Set union
    ///
    /// # Panics
    /// Panics if attempting to merge zones with different IDs
    fn merge(&mut self, other: &ZoneState);

    /// Update timestamp to current time
    fn update_timestamp(&mut self);

    /// Get zone statistics
    fn stats(&self) -> ZoneStats;
}

impl ZoneStateExt for ZoneState {
    fn new(config: ZoneConfig) -> Self {
        Self {
            config: Some(config),
            coordinator_id: None,
            cells: Vec::new(),
            aggregated_capabilities: Vec::new(),
            timestamp: Some(hive_schema::common::v1::Timestamp {
                seconds: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        }
    }

    fn add_cell(&mut self, cell_id: String) -> bool {
        if self.is_full() {
            return false;
        }

        // Check if already present (protobuf uses Vec instead of HashSet)
        if self.cells.iter().any(|id| id == &cell_id) {
            return false;
        }

        self.cells.push(cell_id);
        self.update_timestamp();
        true
    }

    fn remove_cell(&mut self, cell_id: &str) -> bool {
        if let Some(pos) = self.cells.iter().position(|id| id == cell_id) {
            self.cells.remove(pos);
            self.update_timestamp();
            true
        } else {
            false
        }
    }

    fn set_coordinator(&mut self, coordinator_id: String, timestamp: u64) -> bool {
        let current_ts = self.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);

        if timestamp < current_ts {
            return false;
        }

        self.coordinator_id = Some(coordinator_id);
        self.timestamp = Some(hive_schema::common::v1::Timestamp {
            seconds: timestamp,
            nanos: 0,
        });
        true
    }

    fn remove_coordinator(&mut self, timestamp: u64) -> bool {
        let current_ts = self.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);

        if timestamp < current_ts {
            return false;
        }

        self.coordinator_id = None;
        self.timestamp = Some(hive_schema::common::v1::Timestamp {
            seconds: timestamp,
            nanos: 0,
        });
        true
    }

    fn add_capability(&mut self, capability: Capability) {
        // Check if capability already exists (by type)
        if !self
            .aggregated_capabilities
            .iter()
            .any(|c| c.get_capability_type() == capability.get_capability_type())
        {
            self.aggregated_capabilities.push(capability);
            self.update_timestamp();
        }
    }

    fn is_valid(&self) -> bool {
        if let Some(ref config) = self.config {
            self.cells.len() >= config.min_cells as usize
        } else {
            false
        }
    }

    fn is_full(&self) -> bool {
        if let Some(ref config) = self.config {
            self.cells.len() >= config.max_cells as usize
        } else {
            false
        }
    }

    fn cell_count(&self) -> usize {
        self.cells.len()
    }

    fn contains_cell(&self, cell_id: &str) -> bool {
        self.cells.iter().any(|id| id == cell_id)
    }

    fn merge(&mut self, other: &ZoneState) {
        // Verify we're merging the same zone
        let self_id = self.config.as_ref().map(|c| &c.id);
        let other_id = other.config.as_ref().map(|c| &c.id);

        assert_eq!(self_id, other_id, "Cannot merge zones with different IDs");

        // LWW-Register merge for coordinator
        let self_ts = self.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
        let other_ts = other.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);

        if other_ts > self_ts {
            self.coordinator_id = other.coordinator_id.clone();
            self.timestamp = other.timestamp;
        }

        // OR-Set merge for cells (union, avoiding duplicates)
        for cell_id in &other.cells {
            if !self.cells.iter().any(|id| id == cell_id) {
                self.cells.push(cell_id.clone());
            }
        }

        // G-Set merge for capabilities (union by type)
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

    fn update_timestamp(&mut self) {
        self.timestamp = Some(hive_schema::common::v1::Timestamp {
            seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        });
    }

    fn stats(&self) -> ZoneStats {
        let zone_id = self
            .config
            .as_ref()
            .map(|c| c.id.clone())
            .unwrap_or_default();

        ZoneStats {
            zone_id,
            cell_count: self.cells.len() as u32,
            total_nodes: 0, // This would need to be calculated from actual cell data
            unique_capability_count: self.aggregated_capabilities.len() as u32,
            is_valid: self.is_valid(),
            is_full: self.is_full(),
            calculated_at: Some(hive_schema::common::v1::Timestamp {
                seconds: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        }
    }
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

        assert_eq!(zone.config.as_ref().unwrap().id, "zone_1");
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

        let initial_ts = zone.timestamp.as_ref().unwrap().seconds;
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

        let initial_ts = zone.timestamp.as_ref().unwrap().seconds;
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

        let initial_ts = zone1.timestamp.as_ref().unwrap().seconds;

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

        let initial_ts = zone.timestamp.as_ref().unwrap().seconds;

        zone.add_cell("cell_1".to_string());
        zone.add_cell("cell_2".to_string());
        zone.set_coordinator("node_1".to_string(), initial_ts + 100);

        let stats = zone.stats();
        assert_eq!(stats.zone_id, "zone_test");
        assert_eq!(stats.cell_count, 2);
        assert!(stats.is_valid);
        assert!(!stats.is_full);
    }
}
