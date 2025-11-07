//! Hierarchy maintenance and dynamic rebalancing
//!
//! This module implements cell merge/split operations and automatic rebalancing
//! to maintain optimal hierarchy structure as nodes join and leave the system.
//!
//! # Architecture
//!
//! The hierarchy maintainer monitors cell and zone sizes and triggers
//! rebalancing operations when thresholds are crossed:
//!
//! - **Cell too small** (< min_size): Merge with neighbor
//! - **Cell too large** (> max_size): Split into two cells
//! - **Zone imbalanced**: Redistribute cells
//!
//! ## Merge Strategy
//!
//! When a cell falls below minimum size:
//! 1. Find nearest cell with capacity
//! 2. Transfer all members to target cell
//! 3. Update routing table
//! 4. Dissolve empty cell
//!
//! ## Split Strategy
//!
//! When a cell exceeds maximum size:
//! 1. Partition members into two balanced groups
//! 2. Create new cell with half the members
//! 3. Update routing table
//! 4. Elect leaders for both cells
//!
//! # Example
//!
//! ```
//! use cap_protocol::hierarchy::maintenance::{HierarchyMaintainer, RebalanceAction};
//! use cap_protocol::models::cell::CellState;
//!
//! # fn example() -> cap_protocol::Result<()> {
//! let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
//!
//! // Check if cell needs rebalancing
//! # let cell = CellState::new(cap_protocol::models::cell::CellConfig::new(10));
//! let action = maintainer.needs_rebalance(&cell);
//!
//! match action {
//!     RebalanceAction::Merge => {
//!         // Cell too small, find merge candidate
//!     }
//!     RebalanceAction::Split => {
//!         // Cell too large, split it
//!     }
//!     RebalanceAction::None => {
//!         // Cell size is optimal
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use crate::models::cell::{CellConfig, CellConfigExt, CellState, CellStateExt};
use crate::models::zone::ZoneState;
use crate::Result;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Rebalance action recommendation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebalanceAction {
    /// Cell size is within acceptable range
    None,
    /// Cell is too small, should merge with another
    Merge,
    /// Cell is too large, should split
    Split,
}

/// Hierarchy maintenance coordinator
///
/// Monitors and maintains optimal hierarchy structure through
/// cell merge/split operations.
pub struct HierarchyMaintainer {
    /// Minimum cell size (below this triggers merge)
    pub min_cell_size: usize,
    /// Maximum cell size (above this triggers split)
    pub max_cell_size: usize,
    /// Minimum cells per zone
    pub min_zone_cells: usize,
    /// Maximum cells per zone
    pub max_zone_cells: usize,
    /// Maintenance metrics
    metrics: Arc<MaintenanceMetricsInner>,
}

/// Internal metrics tracking
struct MaintenanceMetricsInner {
    merge_count: AtomicUsize,
    split_count: AtomicUsize,
    rebalance_disruptions: AtomicUsize,
}

impl HierarchyMaintainer {
    /// Create a new hierarchy maintainer
    ///
    /// # Arguments
    /// * `min_cell_size` - Minimum members per cell (triggers merge if below)
    /// * `max_cell_size` - Maximum members per cell (triggers split if above)
    /// * `min_zone_cells` - Minimum cells per zone
    /// * `max_zone_cells` - Maximum cells per zone
    pub fn new(
        min_cell_size: usize,
        max_cell_size: usize,
        min_zone_cells: usize,
        max_zone_cells: usize,
    ) -> Self {
        assert!(min_cell_size > 0, "min_cell_size must be > 0");
        assert!(
            max_cell_size > min_cell_size,
            "max_cell_size must be > min_cell_size"
        );
        assert!(min_zone_cells > 0, "min_zone_cells must be > 0");
        assert!(
            max_zone_cells >= min_zone_cells,
            "max_zone_cells must be >= min_zone_cells"
        );

        Self {
            min_cell_size,
            max_cell_size,
            min_zone_cells,
            max_zone_cells,
            metrics: Arc::new(MaintenanceMetricsInner {
                merge_count: AtomicUsize::new(0),
                split_count: AtomicUsize::new(0),
                rebalance_disruptions: AtomicUsize::new(0),
            }),
        }
    }

    /// Check if a cell needs rebalancing
    ///
    /// Returns the recommended action based on cell size.
    pub fn needs_rebalance(&self, cell: &CellState) -> RebalanceAction {
        let size = cell.members.len();

        if size < self.min_cell_size {
            debug!(
                "Cell {} needs merge: {} members < {} min",
                cell.get_id().unwrap_or("<unknown>"),
                size,
                self.min_cell_size
            );
            RebalanceAction::Merge
        } else if size > self.max_cell_size {
            debug!(
                "Cell {} needs split: {} members > {} max",
                cell.get_id().unwrap_or("<unknown>"),
                size,
                self.max_cell_size
            );
            RebalanceAction::Split
        } else {
            RebalanceAction::None
        }
    }

    /// Merge two cells into one
    ///
    /// Combines all members from both cells into a single cell.
    /// The merged cell inherits:
    /// - All members from both cells
    /// - Combined capabilities
    /// - Higher timestamp
    /// - Larger max_size to accommodate
    ///
    /// # Arguments
    /// * `cell1` - First cell to merge
    /// * `cell2` - Second cell to merge
    ///
    /// # Returns
    /// New merged cell state
    pub fn merge_cells(&self, cell1: &CellState, cell2: &CellState) -> Result<CellState> {
        info!(
            "Merging cells {} ({} members) and {} ({} members)",
            cell1.get_id().unwrap_or("<unknown>"),
            cell1.members.len(),
            cell2.get_id().unwrap_or("<unknown>"),
            cell2.members.len()
        );

        // Create new config with combined capacity
        let total_members = cell1.members.len() + cell2.members.len();
        let max_size = (self.max_cell_size as u32).max(total_members as u32);
        let mut new_config = CellConfig::new(max_size);
        new_config.id = Uuid::new_v4().to_string();
        new_config.min_size = self.min_cell_size as u32;

        // Create new cell state
        let mut merged = CellState::new(new_config);

        // Combine members (OR-Set union) - use add_member to avoid duplicates
        for member in cell1.members.iter().chain(cell2.members.iter()) {
            merged.add_member(member.clone());
        }

        // Combine capabilities (G-Set union, deduplicate by ID)
        let mut capability_ids = HashSet::new();
        for cap in cell1.capabilities.iter().chain(cell2.capabilities.iter()) {
            if capability_ids.insert(cap.id.clone()) {
                merged.capabilities.push(cap.clone());
            }
        }

        // Use latest timestamp (LWW merge)
        let cell1_ts = cell1.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
        let cell2_ts = cell2.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
        if cell2_ts > cell1_ts {
            merged.timestamp = cell2.timestamp;
        } else {
            merged.timestamp = cell1.timestamp;
        }

        // Inherit platoon from either cell (prefer non-None)
        merged.platoon_id = cell1
            .platoon_id
            .clone()
            .or_else(|| cell2.platoon_id.clone());

        // Leader will be re-elected by the merged cell
        merged.leader_id = None;

        // Update metrics
        self.metrics.merge_count.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .rebalance_disruptions
            .fetch_add(1, Ordering::Relaxed);

        info!(
            "Merge complete: new cell {} with {} members",
            merged.get_id().unwrap_or("<unknown>"),
            merged.members.len()
        );

        Ok(merged)
    }

    /// Split an oversized cell into two cells
    ///
    /// Partitions members roughly evenly between two new cells.
    /// Strategy:
    /// - First half of members go to cell A
    /// - Second half go to cell B
    /// - Capabilities are duplicated to both cells
    /// - Leaders will be re-elected
    ///
    /// # Arguments
    /// * `cell` - Cell to split
    ///
    /// # Returns
    /// Tuple of (cell_a, cell_b)
    pub fn split_cell(&self, cell: &CellState) -> Result<(CellState, CellState)> {
        let member_count = cell.members.len();

        if member_count < 2 {
            warn!("Cannot split cell with < 2 members");
            return Err(crate::Error::Internal(
                "Cell too small to split".to_string(),
            ));
        }

        info!(
            "Splitting cell {} with {} members",
            cell.get_id().unwrap_or("<unknown>"),
            member_count
        );

        // Calculate split point
        let split_point = member_count / 2;

        // Create two new cells
        let mut config_a = CellConfig::new(self.max_cell_size as u32);
        config_a.id = Uuid::new_v4().to_string();
        config_a.min_size = self.min_cell_size as u32;

        let mut config_b = CellConfig::new(self.max_cell_size as u32);
        config_b.id = Uuid::new_v4().to_string();
        config_b.min_size = self.min_cell_size as u32;

        let mut cell_a = CellState::new(config_a);
        let mut cell_b = CellState::new(config_b);

        // Partition members
        let members: Vec<_> = cell.members.iter().collect();
        for (i, member) in members.iter().enumerate() {
            if i < split_point {
                cell_a.add_member((*member).clone());
            } else {
                cell_b.add_member((*member).clone());
            }
        }

        // Both cells get all capabilities (they can prune later)
        cell_a.capabilities = cell.capabilities.clone();
        cell_b.capabilities = cell.capabilities.clone();

        // Inherit platoon
        cell_a.platoon_id = cell.platoon_id.clone();
        cell_b.platoon_id = cell.platoon_id.clone();

        // Inherit timestamp
        cell_a.timestamp = cell.timestamp;
        cell_b.timestamp = cell.timestamp;

        // Leaders will be re-elected
        cell_a.leader_id = None;
        cell_b.leader_id = None;

        // Update metrics
        self.metrics.split_count.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .rebalance_disruptions
            .fetch_add(1, Ordering::Relaxed);

        info!(
            "Split complete: {} members -> cell_a {} ({} members), cell_b {} ({} members)",
            member_count,
            cell_a.get_id().unwrap_or("<unknown>"),
            cell_a.members.len(),
            cell_b.get_id().unwrap_or("<unknown>"),
            cell_b.members.len()
        );

        Ok((cell_a, cell_b))
    }

    /// Find a merge candidate for an undersized cell
    ///
    /// Selection criteria (in priority order):
    /// 1. Must have capacity for the undersized cell's members
    /// 2. Prefer cells in same zone (minimize cross-zone merges)
    /// 3. Prefer smaller cells (better load balance)
    ///
    /// # Arguments
    /// * `cell` - Undersized cell needing merge
    /// * `candidates` - Available cells to merge with
    ///
    /// # Returns
    /// ID of best merge candidate, or None if no suitable candidate
    pub fn find_merge_candidate(
        &self,
        cell: &CellState,
        candidates: &[CellState],
    ) -> Option<String> {
        let cell_size = cell.members.len();

        let mut best_candidate: Option<(&CellState, usize)> = None;

        for candidate in candidates {
            // Skip self
            if candidate.get_id().unwrap_or("<unknown>") == cell.get_id().unwrap_or("<unknown>") {
                continue;
            }

            let candidate_size = candidate.members.len();
            let combined_size = cell_size + candidate_size;

            // Must have capacity
            if combined_size > self.max_cell_size {
                continue;
            }

            // Calculate priority score (lower is better)
            let mut score = candidate_size; // Prefer smaller cells

            // Bonus for same zone
            if cell.platoon_id.is_some()
                && candidate.platoon_id.is_some()
                && cell.platoon_id == candidate.platoon_id
            {
                score = score.saturating_sub(100); // Strong preference for same zone
            }

            // Update best if this is better
            if best_candidate.is_none() || score < best_candidate.unwrap().1 {
                best_candidate = Some((candidate, score));
            }
        }

        best_candidate.map(|(c, _)| c.get_id().unwrap_or("<unknown>").to_string())
    }

    /// Check if a zone needs rebalancing
    ///
    /// Returns true if:
    /// - Zone has too few cells (< min_zone_cells)
    /// - Zone has too many cells (> max_zone_cells)
    pub fn needs_zone_rebalance(&self, zone: &ZoneState) -> bool {
        let cell_count = zone.cells.len();

        if cell_count < self.min_zone_cells {
            debug!(
                "Zone {} needs cells: {} < {} min",
                zone.config.id, cell_count, self.min_zone_cells
            );
            true
        } else if cell_count > self.max_zone_cells {
            debug!(
                "Zone {} has too many cells: {} > {} max",
                zone.config.id, cell_count, self.max_zone_cells
            );
            true
        } else {
            false
        }
    }

    /// Get maintenance metrics
    pub fn get_metrics(&self) -> MaintenanceMetrics {
        MaintenanceMetrics {
            merge_count: self.metrics.merge_count.load(Ordering::Relaxed),
            split_count: self.metrics.split_count.load(Ordering::Relaxed),
            rebalance_disruptions: self.metrics.rebalance_disruptions.load(Ordering::Relaxed),
        }
    }
}

/// Maintenance metrics
#[derive(Debug, Clone, Copy)]
pub struct MaintenanceMetrics {
    /// Number of cell merges performed
    pub merge_count: usize,
    /// Number of cell splits performed
    pub split_count: usize,
    /// Number of rebalancing operations (disruptions)
    pub rebalance_disruptions: usize,
}

/// Rebalancing coordinator for automatic hierarchy maintenance
///
/// Monitors cells and zones, triggering rebalancing operations when needed.
/// This coordinator periodically checks cell sizes and automatically triggers
/// merge/split operations to maintain optimal hierarchy structure.
///
/// # Example
///
/// ```no_run
/// use cap_protocol::hierarchy::maintenance::{RebalancingCoordinator, HierarchyMaintainer};
/// use cap_protocol::hierarchy::RoutingTable;
/// use cap_protocol::storage::CellStore;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let maintainer = Arc::new(HierarchyMaintainer::new(3, 10, 2, 8));
/// let routing_table = Arc::new(std::sync::Mutex::new(RoutingTable::new()));
/// // CellStore::new() requires a DataSyncBackend and is async
/// // let cell_store = Arc::new(std::sync::Mutex::new(CellStore::new(backend).await.unwrap()));
///
/// // let coordinator = RebalancingCoordinator::new(
/// //     maintainer,
/// //     routing_table,
/// //     cell_store,
/// //     60, // Check every 60 seconds
/// // );
/// # }
/// ```
pub struct RebalancingCoordinator<B: crate::sync::DataSyncBackend> {
    maintainer: Arc<HierarchyMaintainer>,
    routing_table: Arc<std::sync::Mutex<crate::hierarchy::RoutingTable>>,
    cell_store: Arc<std::sync::Mutex<crate::storage::CellStore<B>>>,
    check_interval_secs: u64,
}

impl<B: crate::sync::DataSyncBackend> RebalancingCoordinator<B> {
    /// Create a new rebalancing coordinator
    ///
    /// # Arguments
    /// * `maintainer` - Hierarchy maintainer for merge/split operations
    /// * `routing_table` - Routing table to update during rebalancing
    /// * `cell_store` - Cell store containing cell states
    /// * `check_interval_secs` - How often to check for rebalancing (in seconds)
    pub fn new(
        maintainer: Arc<HierarchyMaintainer>,
        routing_table: Arc<std::sync::Mutex<crate::hierarchy::RoutingTable>>,
        cell_store: Arc<std::sync::Mutex<crate::storage::CellStore<B>>>,
        check_interval_secs: u64,
    ) -> Self {
        Self {
            maintainer,
            routing_table,
            cell_store,
            check_interval_secs,
        }
    }

    /// Get the check interval
    pub fn check_interval(&self) -> u64 {
        self.check_interval_secs
    }

    /// Check all cells and trigger rebalancing if needed
    ///
    /// This method should be called periodically by a background task.
    /// It scans all cells and performs merge/split operations as needed.
    ///
    /// # Returns
    /// Number of rebalancing operations performed
    #[allow(clippy::await_holding_lock)]
    pub async fn check_and_rebalance(&self) -> crate::Result<usize> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut operations_count = 0;

        // Get all cells from store
        let cell_store = self.cell_store.lock().unwrap();
        let all_cells = cell_store.get_valid_cells().await?;
        drop(cell_store);

        // Check each cell for rebalancing needs
        for cell in &all_cells {
            let action = self.maintainer.needs_rebalance(cell);

            match action {
                RebalanceAction::Merge => {
                    // Find merge candidate
                    let candidate_id = self.maintainer.find_merge_candidate(cell, &all_cells);

                    if let Some(target_id) = candidate_id {
                        info!(
                            "Triggering merge: {} → {}",
                            cell.get_id().unwrap_or("<unknown>"),
                            target_id
                        );

                        // Perform merge in routing table
                        let mut routing = self.routing_table.lock().unwrap();
                        let zone_id = cell.platoon_id.as_deref();

                        let merged_id = uuid::Uuid::new_v4().to_string();
                        routing.merge_cells(
                            &[cell.get_id().unwrap_or("<unknown>"), &target_id],
                            &merged_id,
                            zone_id,
                            timestamp,
                        );

                        operations_count += 1;
                    } else {
                        warn!(
                            "No merge candidate found for undersized cell {}",
                            cell.get_id().unwrap_or("<unknown>")
                        );
                    }
                }

                RebalanceAction::Split => {
                    info!("Triggering split: {}", cell.get_id().unwrap_or("<unknown>"));

                    // Perform split
                    let (cell_a, cell_b) = self.maintainer.split_cell(cell)?;

                    // Update routing table
                    let mut routing = self.routing_table.lock().unwrap();
                    let zone_id = cell.platoon_id.as_deref();

                    // Collect member IDs for each new cell
                    let nodes_a: Vec<&str> = cell_a.members.iter().map(|s| s.as_str()).collect();
                    let nodes_b: Vec<&str> = cell_b.members.iter().map(|s| s.as_str()).collect();

                    routing.split_cell(
                        cell.get_id().unwrap_or("<unknown>"),
                        cell_a.get_id().unwrap_or("<unknown>"),
                        cell_b.get_id().unwrap_or("<unknown>"),
                        &nodes_a,
                        &nodes_b,
                        zone_id,
                        timestamp,
                    );

                    operations_count += 1;
                }

                RebalanceAction::None => {
                    // Cell is balanced, nothing to do
                }
            }
        }

        if operations_count > 0 {
            info!(
                "Rebalancing complete: {} operations performed",
                operations_count
            );
        }

        Ok(operations_count)
    }

    /// Get maintenance metrics from the maintainer
    pub fn get_metrics(&self) -> MaintenanceMetrics {
        self.maintainer.get_metrics()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capability, CapabilityExt};

    fn create_test_cell(id: &str, member_count: usize, max_size: usize) -> CellState {
        let mut config = CellConfig::new(max_size as u32);
        config.id = id.to_string();
        config.min_size = 2;

        let mut cell = CellState::new(config);
        for i in 0..member_count {
            cell.add_member(format!("{}_{}", id, i));
        }
        cell
    }

    #[test]
    fn test_maintainer_creation() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
        assert_eq!(maintainer.min_cell_size, 3);
        assert_eq!(maintainer.max_cell_size, 10);
        assert_eq!(maintainer.min_zone_cells, 2);
        assert_eq!(maintainer.max_zone_cells, 8);
    }

    #[test]
    #[should_panic(expected = "min_cell_size must be > 0")]
    fn test_maintainer_invalid_min_size() {
        HierarchyMaintainer::new(0, 10, 2, 8);
    }

    #[test]
    #[should_panic(expected = "max_cell_size must be > min_cell_size")]
    fn test_maintainer_invalid_max_size() {
        HierarchyMaintainer::new(10, 5, 2, 8);
    }

    #[test]
    fn test_needs_rebalance_none() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
        let cell = create_test_cell("cell_1", 5, 10); // 5 members, within range

        assert_eq!(maintainer.needs_rebalance(&cell), RebalanceAction::None);
    }

    #[test]
    fn test_needs_rebalance_merge() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
        let cell = create_test_cell("cell_1", 2, 10); // 2 members, below min (3)

        assert_eq!(maintainer.needs_rebalance(&cell), RebalanceAction::Merge);
    }

    #[test]
    fn test_needs_rebalance_split() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
        let cell = create_test_cell("cell_1", 12, 15); // 12 members, above max (10)

        assert_eq!(maintainer.needs_rebalance(&cell), RebalanceAction::Split);
    }

    #[test]
    fn test_merge_cells() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        let cell1 = create_test_cell("cell_1", 2, 10);
        let cell2 = create_test_cell("cell_2", 3, 10);

        let merged = maintainer.merge_cells(&cell1, &cell2).unwrap();

        // Should have all members from both cells
        assert_eq!(merged.members.len(), 5);

        // Should have new ID
        assert_ne!(
            merged.get_id().unwrap_or("<unknown>"),
            cell1.get_id().unwrap_or("<unknown>")
        );
        assert_ne!(
            merged.get_id().unwrap_or("<unknown>"),
            cell2.get_id().unwrap_or("<unknown>")
        );

        // Leader should be None (needs re-election)
        assert_eq!(merged.leader_id, None);

        // Metrics updated
        let metrics = maintainer.get_metrics();
        assert_eq!(metrics.merge_count, 1);
    }

    #[test]
    fn test_merge_cells_with_capabilities() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        let mut cell1 = create_test_cell("cell_1", 2, 10);
        let mut cell2 = create_test_cell("cell_2", 3, 10);

        // Add capabilities
        cell1.capabilities.push(Capability::new(
            "cap1".to_string(),
            "Sensor".to_string(),
            crate::models::CapabilityType::Sensor,
            0.9,
        ));

        cell2.capabilities.push(Capability::new(
            "cap2".to_string(),
            "Compute".to_string(),
            crate::models::CapabilityType::Compute,
            0.85,
        ));

        let merged = maintainer.merge_cells(&cell1, &cell2).unwrap();

        // Should have both capabilities
        assert_eq!(merged.capabilities.len(), 2);
    }

    #[test]
    fn test_split_cell() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
        let cell = create_test_cell("cell_1", 12, 15);

        let (cell_a, cell_b) = maintainer.split_cell(&cell).unwrap();

        // Members should be partitioned
        assert_eq!(cell_a.members.len(), 6);
        assert_eq!(cell_b.members.len(), 6);

        // Total members preserved
        assert_eq!(cell_a.members.len() + cell_b.members.len(), 12);

        // Should have new IDs
        assert_ne!(
            cell_a.get_id().unwrap_or("<unknown>"),
            cell.get_id().unwrap_or("<unknown>")
        );
        assert_ne!(
            cell_b.get_id().unwrap_or("<unknown>"),
            cell.get_id().unwrap_or("<unknown>")
        );
        assert_ne!(
            cell_a.get_id().unwrap_or("<unknown>"),
            cell_b.get_id().unwrap_or("<unknown>")
        );

        // Leaders should be None (need re-election)
        assert_eq!(cell_a.leader_id, None);
        assert_eq!(cell_b.leader_id, None);

        // Metrics updated
        let metrics = maintainer.get_metrics();
        assert_eq!(metrics.split_count, 1);
    }

    #[test]
    fn test_split_cell_too_small() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
        let cell = create_test_cell("cell_1", 1, 10);

        let result = maintainer.split_cell(&cell);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_merge_candidate_basic() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        let cell = create_test_cell("cell_1", 2, 10); // Needs merge
        let candidate1 = create_test_cell("cell_2", 3, 10);
        let candidate2 = create_test_cell("cell_3", 5, 10);

        let candidates = vec![candidate1.clone(), candidate2.clone()];

        let best = maintainer.find_merge_candidate(&cell, &candidates);

        // Should prefer cell_2 (smaller)
        assert_eq!(best, Some("cell_2".to_string()));
    }

    #[test]
    fn test_find_merge_candidate_capacity_check() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        let cell = create_test_cell("cell_1", 2, 10); // 2 members
        let candidate = create_test_cell("cell_2", 9, 10); // 9 members (would exceed max)

        let candidates = vec![candidate];

        let best = maintainer.find_merge_candidate(&cell, &candidates);

        // Should be None (no valid candidate)
        assert_eq!(best, None);
    }

    #[test]
    fn test_find_merge_candidate_same_zone_preference() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        let mut cell = create_test_cell("cell_1", 2, 10);
        cell.platoon_id = Some("zone_north".to_string());

        let mut candidate1 = create_test_cell("cell_2", 4, 10);
        candidate1.platoon_id = Some("zone_south".to_string()); // Different zone

        let mut candidate2 = create_test_cell("cell_3", 5, 10);
        candidate2.platoon_id = Some("zone_north".to_string()); // Same zone

        let candidates = vec![candidate1, candidate2];

        let best = maintainer.find_merge_candidate(&cell, &candidates);

        // Should prefer cell_3 (same zone)
        assert_eq!(best, Some("cell_3".to_string()));
    }

    #[test]
    fn test_needs_zone_rebalance() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        let config =
            crate::models::zone::ZoneConfig::new("zone_1".to_string(), 8).with_min_cells(2);
        let mut zone = ZoneState::new(config);

        // Too few cells
        zone.cells.insert("cell_1".to_string());
        assert!(maintainer.needs_zone_rebalance(&zone));

        // Just right
        zone.cells.insert("cell_2".to_string());
        assert!(!maintainer.needs_zone_rebalance(&zone));

        // Too many cells
        for i in 3..=10 {
            zone.cells.insert(format!("cell_{}", i));
        }
        assert!(maintainer.needs_zone_rebalance(&zone));
    }

    #[test]
    fn test_metrics() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        let cell1 = create_test_cell("cell_1", 2, 10);
        let cell2 = create_test_cell("cell_2", 3, 10);
        let cell3 = create_test_cell("cell_3", 12, 15);

        // Perform operations
        let _ = maintainer.merge_cells(&cell1, &cell2);
        let _ = maintainer.split_cell(&cell3);

        let metrics = maintainer.get_metrics();
        assert_eq!(metrics.merge_count, 1);
        assert_eq!(metrics.split_count, 1);
        assert_eq!(metrics.rebalance_disruptions, 2);
    }

    // Integration Tests

    #[test]
    fn test_integration_merge_with_routing_table() {
        use crate::hierarchy::RoutingTable;

        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
        let mut routing_table = RoutingTable::new();

        // Create two undersized cells
        let mut cell1 = create_test_cell("cell_1", 2, 10);
        let mut cell2 = create_test_cell("cell_2", 2, 10);

        cell1.platoon_id = Some("zone_north".to_string());
        cell2.platoon_id = Some("zone_north".to_string());

        // Add nodes to routing table
        routing_table.assign_node("cell_1_0", "cell_1", 100);
        routing_table.assign_node("cell_1_1", "cell_1", 101);
        routing_table.assign_node("cell_2_0", "cell_2", 102);
        routing_table.assign_node("cell_2_1", "cell_2", 103);

        routing_table.assign_cell("cell_1", "zone_north", 100);
        routing_table.assign_cell("cell_2", "zone_north", 101);

        // Merge cells
        let merged = maintainer.merge_cells(&cell1, &cell2).unwrap();

        // Update routing table
        let merged_id = merged.get_id().unwrap_or("<unknown>");
        routing_table.merge_cells(&["cell_1", "cell_2"], merged_id, Some("zone_north"), 200);

        // Verify all nodes are now in merged cell
        assert_eq!(routing_table.get_node_cell("cell_1_0"), Some(merged_id));
        assert_eq!(routing_table.get_node_cell("cell_1_1"), Some(merged_id));
        assert_eq!(routing_table.get_node_cell("cell_2_0"), Some(merged_id));
        assert_eq!(routing_table.get_node_cell("cell_2_1"), Some(merged_id));

        // Verify merged cell is in zone
        assert_eq!(routing_table.get_cell_zone(merged_id), Some("zone_north"));

        // Verify old cells are removed
        assert_eq!(routing_table.get_cell_zone("cell_1"), None);
        assert_eq!(routing_table.get_cell_zone("cell_2"), None);
    }

    #[test]
    fn test_integration_split_with_routing_table() {
        use crate::hierarchy::RoutingTable;

        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
        let mut routing_table = RoutingTable::new();

        // Create oversized cell
        let mut cell = create_test_cell("cell_oversized", 12, 15);
        cell.platoon_id = Some("zone_south".to_string());

        // Add nodes to routing table
        for i in 0..12 {
            routing_table.assign_node(&format!("cell_oversized_{}", i), "cell_oversized", 100 + i);
        }
        routing_table.assign_cell("cell_oversized", "zone_south", 100);

        // Split cell
        let (cell_a, cell_b) = maintainer.split_cell(&cell).unwrap();

        // Collect node IDs for routing update
        let nodes_a: Vec<&str> = cell_a.members.iter().map(|s| s.as_str()).collect();
        let nodes_b: Vec<&str> = cell_b.members.iter().map(|s| s.as_str()).collect();

        // Update routing table
        routing_table.split_cell(
            "cell_oversized",
            cell_a.get_id().unwrap_or("<unknown>"),
            cell_b.get_id().unwrap_or("<unknown>"),
            &nodes_a,
            &nodes_b,
            Some("zone_south"),
            200,
        );

        // Verify nodes are distributed correctly
        assert_eq!(
            routing_table
                .get_cell_nodes(cell_a.get_id().unwrap_or("<unknown>"))
                .len(),
            6
        );
        assert_eq!(
            routing_table
                .get_cell_nodes(cell_b.get_id().unwrap_or("<unknown>"))
                .len(),
            6
        );

        // Verify both cells are in zone
        assert_eq!(
            routing_table.get_cell_zone(cell_a.get_id().unwrap_or("<unknown>")),
            Some("zone_south")
        );
        assert_eq!(
            routing_table.get_cell_zone(cell_b.get_id().unwrap_or("<unknown>")),
            Some("zone_south")
        );

        // Verify old cell is removed
        assert_eq!(routing_table.get_cell_zone("cell_oversized"), None);
    }

    #[test]
    fn test_integration_sequential_rebalancing() {
        use crate::hierarchy::RoutingTable;

        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);
        let mut routing_table = RoutingTable::new();

        // Scenario: Start with oversized cell, split it, then merge one of the results

        // 1. Create oversized cell with 12 members
        let mut cell = create_test_cell("cell_1", 12, 15);
        cell.platoon_id = Some("zone_alpha".to_string());

        for i in 0..12 {
            routing_table.assign_node(&format!("cell_1_{}", i), "cell_1", 100 + i);
        }
        routing_table.assign_cell("cell_1", "zone_alpha", 100);

        // 2. Split the oversized cell
        assert_eq!(maintainer.needs_rebalance(&cell), RebalanceAction::Split);
        let (cell_a, cell_b) = maintainer.split_cell(&cell).unwrap();

        let nodes_a: Vec<&str> = cell_a.members.iter().map(|s| s.as_str()).collect();
        let nodes_b: Vec<&str> = cell_b.members.iter().map(|s| s.as_str()).collect();

        routing_table.split_cell(
            "cell_1",
            cell_a.get_id().unwrap_or("<unknown>"),
            cell_b.get_id().unwrap_or("<unknown>"),
            &nodes_a,
            &nodes_b,
            Some("zone_alpha"),
            200,
        );

        // 3. Create a small cell to merge with cell_a
        let mut cell_small = create_test_cell("cell_small", 2, 10);
        cell_small.platoon_id = Some("zone_alpha".to_string());

        routing_table.assign_node("cell_small_0", "cell_small", 300);
        routing_table.assign_node("cell_small_1", "cell_small", 301);
        routing_table.assign_cell("cell_small", "zone_alpha", 300);

        // 4. Merge cell_small with cell_a
        assert_eq!(
            maintainer.needs_rebalance(&cell_small),
            RebalanceAction::Merge
        );
        let merged = maintainer.merge_cells(&cell_small, &cell_a).unwrap();

        routing_table.merge_cells(
            &["cell_small", cell_a.get_id().unwrap_or("<unknown>")],
            merged.get_id().unwrap_or("<unknown>"),
            Some("zone_alpha"),
            400,
        );

        // Verify final state
        // Should have 2 cells: merged (8 nodes) and cell_b (6 nodes)
        assert_eq!(
            routing_table
                .get_cell_nodes(merged.get_id().unwrap_or("<unknown>"))
                .len(),
            8
        );
        assert_eq!(
            routing_table
                .get_cell_nodes(cell_b.get_id().unwrap_or("<unknown>"))
                .len(),
            6
        );

        // Both should be balanced
        assert_eq!(maintainer.needs_rebalance(&merged), RebalanceAction::None);
        assert_eq!(maintainer.needs_rebalance(&cell_b), RebalanceAction::None);

        // Verify metrics
        let metrics = maintainer.get_metrics();
        assert_eq!(metrics.merge_count, 1);
        assert_eq!(metrics.split_count, 1);
        assert_eq!(metrics.rebalance_disruptions, 2);
    }

    #[test]
    fn test_integration_merge_candidate_selection_priority() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        // Create undersized cell needing merge
        let mut cell_small = create_test_cell("cell_small", 2, 10);
        cell_small.platoon_id = Some("zone_north".to_string());

        // Create candidates with different characteristics
        let mut cell_large = create_test_cell("cell_large", 8, 10);
        cell_large.platoon_id = Some("zone_south".to_string()); // Different zone

        let mut cell_medium = create_test_cell("cell_medium", 5, 10);
        cell_medium.platoon_id = Some("zone_south".to_string()); // Different zone

        let mut cell_small_same_zone = create_test_cell("cell_same_zone", 4, 10);
        cell_small_same_zone.platoon_id = Some("zone_north".to_string()); // Same zone

        let candidates = vec![
            cell_large.clone(),
            cell_medium.clone(),
            cell_small_same_zone.clone(),
        ];

        // Should prefer same-zone candidate even though it's not the smallest
        let best = maintainer.find_merge_candidate(&cell_small, &candidates);
        assert_eq!(best, Some("cell_same_zone".to_string()));

        // If same zone candidate doesn't fit, should pick smallest that fits
        let mut cell_same_zone_full = create_test_cell("cell_same_full", 9, 10);
        cell_same_zone_full.platoon_id = Some("zone_north".to_string());

        let candidates2 = vec![cell_large.clone(), cell_medium.clone(), cell_same_zone_full];

        let best2 = maintainer.find_merge_candidate(&cell_small, &candidates2);
        assert_eq!(best2, Some("cell_medium".to_string())); // Medium (5) < Large (8)
    }

    #[test]
    fn test_integration_capabilities_preserved_during_merge() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        // Create two cells with different capabilities
        let mut cell1 = create_test_cell("cell_1", 2, 10);
        let mut cell2 = create_test_cell("cell_2", 3, 10);

        let cap1 = Capability::new(
            "cap_sensor".to_string(),
            "Sensor".to_string(),
            crate::models::CapabilityType::Sensor,
            0.9,
        );

        let cap2 = Capability::new(
            "cap_payload".to_string(),
            "Payload".to_string(),
            crate::models::CapabilityType::Payload,
            0.8,
        );

        cell1.capabilities.push(cap1.clone());
        cell2.capabilities.push(cap2.clone());

        // Merge cells
        let merged = maintainer.merge_cells(&cell1, &cell2).unwrap();

        // Verify both capabilities are preserved
        assert_eq!(merged.capabilities.len(), 2);
        assert!(merged.capabilities.iter().any(|c| c.id == "cap_sensor"));
        assert!(merged.capabilities.iter().any(|c| c.id == "cap_payload"));

        // Verify all members are preserved
        assert_eq!(merged.members.len(), 5);
    }

    #[test]
    fn test_integration_capabilities_duplicated_during_split() {
        let maintainer = HierarchyMaintainer::new(3, 10, 2, 8);

        // Create cell with capabilities
        let mut cell = create_test_cell("cell_1", 12, 15);

        let cap = Capability::new(
            "cap_relay".to_string(),
            "Relay".to_string(),
            crate::models::CapabilityType::Communication,
            0.95,
        );

        cell.capabilities.push(cap.clone());

        // Split cell
        let (cell_a, cell_b) = maintainer.split_cell(&cell).unwrap();

        // Verify both cells get the capability
        assert_eq!(cell_a.capabilities.len(), 1);
        assert_eq!(cell_b.capabilities.len(), 1);
        assert_eq!(cell_a.capabilities[0].id, "cap_relay");
        assert_eq!(cell_b.capabilities[0].id, "cap_relay");

        // Verify members are split evenly
        assert_eq!(cell_a.members.len(), 6);
        assert_eq!(cell_b.members.len(), 6);
    }
}
