//! Zone Coordinator for hierarchical coordination (E5 Phase 2)
//!
//! The ZoneCoordinator manages zone-level operations:
//! - Zone formation and validation
//! - Capability aggregation from cells
//! - Emergent capability detection
//! - Zone readiness assessment

use crate::models::{Capability, CapabilityType, CellState};
use crate::Result;
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument};

/// Zone formation status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneFormationStatus {
    /// Zone is forming (not enough cells yet)
    Forming,
    /// Zone is ready for operations
    Ready,
    /// Zone is degraded (below minimum requirements)
    Degraded,
}

/// Zone formation coordinator
///
/// Manages zone formation, capability aggregation, and readiness assessment.
/// Typically run by the zone coordinator node (a cell leader).
///
/// # Example
/// ```
/// use cap_protocol::hierarchy::ZoneCoordinator;
/// use cap_protocol::models::CellState;
///
/// let mut coordinator = ZoneCoordinator::new("zone_north".to_string(), 2, 0.7);
///
/// // Check if zone is ready
/// let cells: Vec<CellState> = vec![]; // Empty for example
/// let is_complete = coordinator.check_formation_complete(&cells, None).unwrap();
/// ```
pub struct ZoneCoordinator {
    /// Zone identifier
    pub zone_id: String,
    /// Minimum number of cells required
    pub min_cells: usize,
    /// Minimum average readiness score (0.0 - 1.0)
    pub min_readiness: f32,
    /// Current formation status
    pub status: ZoneFormationStatus,
    /// When formation started
    formation_start: Instant,
}

impl ZoneCoordinator {
    /// Create a new zone coordinator
    ///
    /// # Arguments
    /// * `zone_id` - Unique zone identifier
    /// * `min_cells` - Minimum number of cells required for zone readiness
    /// * `min_readiness` - Minimum average cell readiness (0.0 - 1.0)
    ///
    /// # Example
    /// ```
    /// use cap_protocol::hierarchy::ZoneCoordinator;
    ///
    /// let coordinator = ZoneCoordinator::new("zone_1".to_string(), 3, 0.8);
    /// ```
    pub fn new(zone_id: String, min_cells: usize, min_readiness: f32) -> Self {
        Self {
            zone_id,
            min_cells,
            min_readiness: min_readiness.clamp(0.0, 1.0),
            status: ZoneFormationStatus::Forming,
            formation_start: Instant::now(),
        }
    }

    /// Check if zone formation is complete
    ///
    /// Zone is considered complete when:
    /// 1. Minimum number of cells are present
    /// 2. Average cell readiness meets minimum threshold
    /// 3. A coordinator is assigned
    ///
    /// # Arguments
    /// * `cells` - List of cell states in this zone
    /// * `coordinator_id` - Current zone coordinator (if any)
    ///
    /// # Returns
    /// `true` if formation is complete, `false` otherwise
    #[instrument(skip(self, cells))]
    pub fn check_formation_complete(
        &mut self,
        cells: &[CellState],
        coordinator_id: Option<&str>,
    ) -> Result<bool> {
        let cell_count = cells.len();

        // Check minimum cell count
        if cell_count < self.min_cells {
            debug!(
                "Zone {} forming: {}/{} cells",
                self.zone_id, cell_count, self.min_cells
            );
            self.status = ZoneFormationStatus::Forming;
            return Ok(false);
        }

        // Check coordinator assigned
        if coordinator_id.is_none() {
            debug!("Zone {} forming: No coordinator assigned", self.zone_id);
            self.status = ZoneFormationStatus::Forming;
            return Ok(false);
        }

        // Calculate average readiness (based on member count vs min size)
        let avg_readiness = self.calculate_average_readiness(cells);

        if avg_readiness < self.min_readiness {
            debug!(
                "Zone {} forming: Readiness {:.2} < {:.2}",
                self.zone_id, avg_readiness, self.min_readiness
            );
            self.status = ZoneFormationStatus::Forming;
            return Ok(false);
        }

        // Formation complete!
        info!(
            "Zone {} ready: {} cells, {:.2} readiness",
            self.zone_id, cell_count, avg_readiness
        );
        self.status = ZoneFormationStatus::Ready;
        Ok(true)
    }

    /// Calculate average readiness across all cells
    ///
    /// Readiness is calculated as: (member_count / max_size)
    /// This represents how filled each cell is relative to capacity.
    fn calculate_average_readiness(&self, cells: &[CellState]) -> f32 {
        if cells.is_empty() {
            return 0.0;
        }

        let total: f32 = cells
            .iter()
            .map(|cell| {
                let member_count = cell.members.len() as f32;
                let max_size = cell.config.max_size as f32;
                member_count / max_size
            })
            .sum();

        total / cells.len() as f32
    }

    /// Aggregate capabilities from all cells
    ///
    /// Combines capabilities from all cells into a zone-level capability set.
    /// De-duplicates by capability type, keeping the highest value.
    ///
    /// # Arguments
    /// * `cells` - List of cell states to aggregate from
    ///
    /// # Returns
    /// Vector of aggregated capabilities
    #[instrument(skip(self, cells))]
    pub fn aggregate_capabilities(&self, cells: &[CellState]) -> Vec<Capability> {
        use std::collections::HashMap;

        let mut capability_map: HashMap<CapabilityType, Capability> = HashMap::new();

        for cell in cells {
            for cap in &cell.capabilities {
                capability_map
                    .entry(cap.capability_type)
                    .and_modify(|existing| {
                        // Keep capability with higher confidence
                        if cap.confidence > existing.confidence {
                            *existing = cap.clone();
                        }
                    })
                    .or_insert_with(|| cap.clone());
            }
        }

        let mut aggregated: Vec<Capability> = capability_map.into_values().collect();
        aggregated.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        debug!(
            "Zone {} aggregated {} capabilities from {} cells",
            self.zone_id,
            aggregated.len(),
            cells.len()
        );

        aggregated
    }

    /// Detect emergent zone-level capabilities
    ///
    /// Identifies capabilities that emerge from the composition of multiple cells.
    /// For example, if we have both Sensor and Compute capabilities across cells,
    /// we might have an emergent ISR (Intelligence, Surveillance, Reconnaissance) capability.
    ///
    /// # Arguments
    /// * `cells` - List of cell states to analyze
    ///
    /// # Returns
    /// Vector of emergent capabilities detected
    pub fn detect_emergent_capabilities(&self, cells: &[CellState]) -> Vec<Capability> {
        use std::collections::HashSet;

        if cells.len() < 2 {
            // Need at least 2 cells for emergent capabilities
            return Vec::new();
        }

        // Collect all unique capability types across cells
        let mut capability_types = HashSet::new();
        for cell in cells {
            for cap in &cell.capabilities {
                capability_types.insert(cap.capability_type);
            }
        }

        let mut emergent = Vec::new();

        // Detect specific emergent patterns
        // Pattern 1: Sensor + Compute = Enhanced ISR
        if capability_types.contains(&CapabilityType::Sensor)
            && capability_types.contains(&CapabilityType::Compute)
        {
            emergent.push(Capability::new(
                format!("{}_emergent_isr", self.zone_id),
                "Enhanced ISR Capability".to_string(),
                CapabilityType::Emergent,
                0.85, // High confidence for well-established pattern
            ));
        }

        // Pattern 2: Communication + Mobility = Relay Network
        if capability_types.contains(&CapabilityType::Communication)
            && capability_types.contains(&CapabilityType::Mobility)
            && cells.len() >= 3
        {
            emergent.push(Capability::new(
                format!("{}_emergent_relay", self.zone_id),
                "Mobile Relay Network".to_string(),
                CapabilityType::Emergent,
                0.80,
            ));
        }

        // Pattern 3: Sensor + Payload = Strike Package
        if capability_types.contains(&CapabilityType::Sensor)
            && capability_types.contains(&CapabilityType::Payload)
        {
            emergent.push(Capability::new(
                format!("{}_emergent_strike", self.zone_id),
                "Coordinated Strike Package".to_string(),
                CapabilityType::Emergent,
                0.75,
            ));
        }

        if !emergent.is_empty() {
            info!(
                "Zone {} detected {} emergent capabilities",
                self.zone_id,
                emergent.len()
            );
        }

        emergent
    }

    /// Check if zone can transition to operations phase
    ///
    /// Returns true if the zone is in Ready status and has been stable
    /// for a minimum duration.
    pub fn can_transition_to_operations(&self) -> bool {
        self.status == ZoneFormationStatus::Ready
            && self.formation_start.elapsed() >= Duration::from_secs(5)
    }

    /// Get formation metrics
    ///
    /// Returns current statistics about the zone formation.
    pub fn get_metrics(&self, cells: &[CellState]) -> ZoneMetrics {
        let total_nodes: usize = cells.iter().map(|cell| cell.members.len()).sum();

        let aggregated_caps = self.aggregate_capabilities(cells);
        let emergent_caps = self.detect_emergent_capabilities(cells);

        ZoneMetrics {
            cell_count: cells.len(),
            total_nodes,
            average_cell_readiness: self.calculate_average_readiness(cells),
            capability_count: aggregated_caps.len(),
            emergent_capability_count: emergent_caps.len(),
            formation_time_ms: self.formation_start.elapsed().as_millis() as u64,
            status: self.status,
        }
    }

    /// Update zone status based on current cell states
    ///
    /// Checks if zone has degraded below minimum requirements.
    pub fn update_status(&mut self, cells: &[CellState]) {
        let cell_count = cells.len();

        if cell_count < self.min_cells {
            if self.status == ZoneFormationStatus::Ready {
                info!(
                    "Zone {} degraded: {} < {} cells",
                    self.zone_id, cell_count, self.min_cells
                );
                self.status = ZoneFormationStatus::Degraded;
            }
            return;
        }

        let avg_readiness = self.calculate_average_readiness(cells);
        if avg_readiness < self.min_readiness && self.status == ZoneFormationStatus::Ready {
            info!(
                "Zone {} degraded: {:.2} < {:.2} readiness",
                self.zone_id, avg_readiness, self.min_readiness
            );
            self.status = ZoneFormationStatus::Degraded;
        }
    }
}

/// Zone formation metrics
#[derive(Debug, Clone)]
pub struct ZoneMetrics {
    /// Number of cells in zone
    pub cell_count: usize,
    /// Total number of nodes across all cells
    pub total_nodes: usize,
    /// Average readiness score across cells
    pub average_cell_readiness: f32,
    /// Number of aggregated capabilities
    pub capability_count: usize,
    /// Number of emergent capabilities detected
    pub emergent_capability_count: usize,
    /// Time since formation started (milliseconds)
    pub formation_time_ms: u64,
    /// Current formation status
    pub status: ZoneFormationStatus,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CellConfig;

    fn create_test_cell(id: &str, member_count: usize) -> CellState {
        let mut config = CellConfig::new(10);
        config.id = id.to_string();
        let mut cell = CellState::new(config);

        // Add the specified number of members
        for i in 0..member_count {
            cell.add_member(format!("{}_{}", id, i));
        }

        cell
    }

    fn add_capability(cell: &mut CellState, cap_type: CapabilityType, confidence: f32) {
        cell.capabilities.push(Capability::new(
            format!("{}_{:?}", cell.config.id, cap_type),
            format!("{:?} Capability", cap_type),
            cap_type,
            confidence,
        ));
    }

    #[test]
    fn test_coordinator_creation() {
        let coordinator = ZoneCoordinator::new("zone_1".to_string(), 3, 0.8);
        assert_eq!(coordinator.zone_id, "zone_1");
        assert_eq!(coordinator.min_cells, 3);
        assert_eq!(coordinator.min_readiness, 0.8);
        assert_eq!(coordinator.status, ZoneFormationStatus::Forming);
    }

    #[test]
    fn test_formation_incomplete_not_enough_cells() {
        let mut coordinator = ZoneCoordinator::new("zone_1".to_string(), 3, 0.5);

        let cells = vec![create_test_cell("cell_1", 5), create_test_cell("cell_2", 5)];

        let is_complete = coordinator
            .check_formation_complete(&cells, Some("node_1"))
            .unwrap();

        assert!(!is_complete);
        assert_eq!(coordinator.status, ZoneFormationStatus::Forming);
    }

    #[test]
    fn test_formation_incomplete_no_coordinator() {
        let mut coordinator = ZoneCoordinator::new("zone_1".to_string(), 2, 0.5);

        let cells = vec![create_test_cell("cell_1", 5), create_test_cell("cell_2", 5)];

        let is_complete = coordinator.check_formation_complete(&cells, None).unwrap();

        assert!(!is_complete);
        assert_eq!(coordinator.status, ZoneFormationStatus::Forming);
    }

    #[test]
    fn test_formation_incomplete_low_readiness() {
        let mut coordinator = ZoneCoordinator::new("zone_1".to_string(), 2, 0.8);

        let cells = vec![
            create_test_cell("cell_1", 9), // 90% full
            create_test_cell("cell_2", 3), // 30% full - brings average to 60%
        ];

        let is_complete = coordinator
            .check_formation_complete(&cells, Some("node_1"))
            .unwrap();

        assert!(!is_complete); // Average readiness (60%) < min (80%)
        assert_eq!(coordinator.status, ZoneFormationStatus::Forming);
    }

    #[test]
    fn test_formation_complete() {
        let mut coordinator = ZoneCoordinator::new("zone_1".to_string(), 2, 0.4);

        let cells = vec![
            create_test_cell("cell_1", 5), // 0.5
            create_test_cell("cell_2", 5), // 0.5
            create_test_cell("cell_3", 4), // 0.4
        ];
        // Average readiness: (0.5 + 0.5 + 0.4) / 3 = 0.467 > 0.4

        let is_complete = coordinator
            .check_formation_complete(&cells, Some("node_1"))
            .unwrap();

        assert!(is_complete);
        assert_eq!(coordinator.status, ZoneFormationStatus::Ready);
    }

    #[test]
    fn test_capability_aggregation() {
        let coordinator = ZoneCoordinator::new("zone_1".to_string(), 2, 0.5);

        let mut cell1 = create_test_cell("cell_1", 5);
        add_capability(&mut cell1, CapabilityType::Sensor, 0.9);
        add_capability(&mut cell1, CapabilityType::Compute, 0.8);

        let mut cell2 = create_test_cell("cell_2", 5);
        add_capability(&mut cell2, CapabilityType::Sensor, 0.7); // Lower confidence
        add_capability(&mut cell2, CapabilityType::Communication, 0.85);

        let cells = vec![cell1, cell2];
        let aggregated = coordinator.aggregate_capabilities(&cells);

        // Should have 3 unique capability types
        assert_eq!(aggregated.len(), 3);

        // Sensor should have higher confidence (0.9 from cell1)
        let sensor_cap = aggregated
            .iter()
            .find(|c| c.capability_type == CapabilityType::Sensor)
            .unwrap();
        assert_eq!(sensor_cap.confidence, 0.9);
    }

    #[test]
    fn test_emergent_capability_isr() {
        let coordinator = ZoneCoordinator::new("zone_1".to_string(), 2, 0.5);

        let mut cell1 = create_test_cell("cell_1", 5);
        add_capability(&mut cell1, CapabilityType::Sensor, 0.9);

        let mut cell2 = create_test_cell("cell_2", 5);
        add_capability(&mut cell2, CapabilityType::Compute, 0.8);

        let cells = vec![cell1, cell2];
        let emergent = coordinator.detect_emergent_capabilities(&cells);

        // Should detect ISR emergent capability
        assert_eq!(emergent.len(), 1);
        assert_eq!(emergent[0].capability_type, CapabilityType::Emergent);
        assert!(emergent[0].name.contains("ISR"));
    }

    #[test]
    fn test_emergent_capability_relay_network() {
        let coordinator = ZoneCoordinator::new("zone_1".to_string(), 3, 0.5);

        let mut cell1 = create_test_cell("cell_1", 5);
        add_capability(&mut cell1, CapabilityType::Communication, 0.9);

        let mut cell2 = create_test_cell("cell_2", 5);
        add_capability(&mut cell2, CapabilityType::Mobility, 0.8);

        let mut cell3 = create_test_cell("cell_3", 4);
        add_capability(&mut cell3, CapabilityType::Communication, 0.85);

        let cells = vec![cell1, cell2, cell3];
        let emergent = coordinator.detect_emergent_capabilities(&cells);

        // Should detect relay network
        assert!(!emergent.is_empty());
        assert!(emergent
            .iter()
            .any(|c| c.name.contains("Relay") || c.name.contains("Network")));
    }

    #[test]
    fn test_zone_metrics() {
        let coordinator = ZoneCoordinator::new("zone_1".to_string(), 2, 0.5);

        let mut cell1 = create_test_cell("cell_1", 5);
        add_capability(&mut cell1, CapabilityType::Sensor, 0.9);

        let mut cell2 = create_test_cell("cell_2", 4);
        add_capability(&mut cell2, CapabilityType::Compute, 0.8);

        let cells = vec![cell1, cell2];
        let metrics = coordinator.get_metrics(&cells);

        assert_eq!(metrics.cell_count, 2);
        assert_eq!(metrics.total_nodes, 9); // 5 + 4
        assert_eq!(metrics.average_cell_readiness, 0.45); // (5/10 + 4/10) / 2 = 0.45
        assert_eq!(metrics.capability_count, 2);
        assert_eq!(metrics.emergent_capability_count, 1); // ISR
    }

    #[test]
    fn test_zone_degradation() {
        let mut coordinator = ZoneCoordinator::new("zone_1".to_string(), 3, 0.4);

        // Start with 3 cells - should be ready
        let cells = vec![
            create_test_cell("cell_1", 5),
            create_test_cell("cell_2", 5),
            create_test_cell("cell_3", 4),
        ];

        coordinator
            .check_formation_complete(&cells, Some("node_1"))
            .unwrap();
        assert_eq!(coordinator.status, ZoneFormationStatus::Ready);

        // Drop to 2 cells - should degrade
        let degraded_cells = vec![create_test_cell("cell_1", 5), create_test_cell("cell_2", 5)];

        coordinator.update_status(&degraded_cells);
        assert_eq!(coordinator.status, ZoneFormationStatus::Degraded);
    }

    #[test]
    fn test_can_transition_to_operations() {
        let coordinator = ZoneCoordinator::new("zone_1".to_string(), 2, 0.7);

        // Initially can't transition (status is Forming)
        assert!(!coordinator.can_transition_to_operations());

        // Even after setting to Ready, need to wait for stability duration
        // This would require mocking time or waiting in test
    }
}
