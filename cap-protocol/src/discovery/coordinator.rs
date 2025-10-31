//! Discovery Coordinator for Phase 1
//!
//! Orchestrates the bootstrap phase for nodes to discover and form initial squads.
//!
//! # Architecture
//!
//! The DiscoveryCoordinator manages:
//! - Phase state transitions (Discovery → Squad)
//! - Discovery timeout management (default 60s)
//! - Tracking unassigned platforms
//! - Discovery metrics collection
//! - Re-bootstrap on failure
//!
//! ## Discovery Strategies
//!
//! Three strategies are supported:
//! 1. **Geographic Self-Organization** (E3.1) - Platforms form cells based on proximity
//! 2. **C2-Directed Assignment** (E3.2) - C2 explicitly assigns nodes to squads
//! 3. **Capability-Based Queries** (E3.3) - Platforms query and form cells by capabilities
//!
//! ## State Machine
//!
//! ```text
//! Discovery (initial)
//!   │
//!   ├─ timeout expired & assigned → Squad
//!   ├─ timeout expired & unassigned → Failed (can retry)
//!   └─ forced transition → Squad
//! ```

use crate::storage::CellStore;
use crate::traits::Phase;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument, warn};

/// Default bootstrap timeout (60 seconds)
pub const DEFAULT_BOOTSTRAP_TIMEOUT_SECS: u64 = 60;

/// Discovery strategy selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootstrapStrategy {
    /// Geographic proximity-based cell formation
    Geographic,
    /// C2-directed squad assignment
    Directed,
    /// Capability-based query and formation
    CapabilityBased,
}

impl std::fmt::Display for BootstrapStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootstrapStrategy::Geographic => write!(f, "geographic"),
            BootstrapStrategy::Directed => write!(f, "directed"),
            BootstrapStrategy::CapabilityBased => write!(f, "capability_based"),
        }
    }
}

/// Discovery phase status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootstrapStatus {
    /// Discovery phase not started
    NotStarted,
    /// Discovery phase in progress
    InProgress,
    /// Discovery completed successfully
    Completed,
    /// Discovery failed (timeout with no assignment)
    Failed,
    /// Discovery timed out but partially completed
    PartiallyCompleted,
}

/// Discovery metrics for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryMetrics {
    /// Total nodes participating
    pub total_platforms: usize,
    /// Platforms successfully assigned to squads
    pub assigned_platforms: usize,
    /// Platforms still unassigned
    pub unassigned_platforms: usize,
    /// Number of cells formed
    pub squads_formed: usize,
    /// Time elapsed since bootstrap start (seconds)
    pub elapsed_seconds: f64,
    /// Discovery strategy used
    pub strategy: BootstrapStrategy,
    /// Final status
    pub status: BootstrapStatus,
    /// Total messages sent during bootstrap (if tracked)
    pub messages_sent: Option<usize>,
}

impl DiscoveryMetrics {
    /// Calculate assignment rate (0.0 - 1.0)
    pub fn assignment_rate(&self) -> f32 {
        if self.total_platforms == 0 {
            return 0.0;
        }
        self.assigned_platforms as f32 / self.total_platforms as f32
    }

    /// Calculate average squad size
    pub fn avg_squad_size(&self) -> f32 {
        if self.squads_formed == 0 {
            return 0.0;
        }
        self.assigned_platforms as f32 / self.squads_formed as f32
    }

    /// Check if bootstrap was successful (>90% assigned)
    pub fn is_successful(&self) -> bool {
        self.assignment_rate() > 0.9 && self.status == BootstrapStatus::Completed
    }
}

/// Discovery Coordinator
///
/// Manages the bootstrap phase lifecycle for a platform or simulation.
pub struct DiscoveryCoordinator {
    /// Cell storage
    store: CellStore,
    /// Current phase
    current_phase: Phase,
    /// Discovery strategy
    strategy: BootstrapStrategy,
    /// Discovery timeout duration
    timeout: Duration,
    /// Discovery start time
    start_time: Option<Instant>,
    /// Discovery status
    status: BootstrapStatus,
    /// Tracked platform IDs
    tracked_platforms: HashSet<String>,
    /// Node to squad assignments
    assignments: HashMap<String, String>,
    /// Message count (optional tracking)
    message_count: usize,
}

impl DiscoveryCoordinator {
    /// Create a new bootstrap coordinator
    pub fn new(store: CellStore, strategy: BootstrapStrategy) -> Self {
        Self {
            store,
            current_phase: Phase::Discovery,
            strategy,
            timeout: Duration::from_secs(DEFAULT_BOOTSTRAP_TIMEOUT_SECS),
            start_time: None,
            status: BootstrapStatus::NotStarted,
            tracked_platforms: HashSet::new(),
            assignments: HashMap::new(),
            message_count: 0,
        }
    }

    /// Set custom bootstrap timeout
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout = Duration::from_secs(timeout_secs);
        self
    }

    /// Get current phase
    pub fn phase(&self) -> Phase {
        self.current_phase
    }

    /// Get bootstrap status
    pub fn status(&self) -> BootstrapStatus {
        self.status
    }

    /// Start the bootstrap phase
    #[instrument(skip(self))]
    pub fn start_bootstrap(&mut self, platform_ids: Vec<String>) -> Result<()> {
        if self.status != BootstrapStatus::NotStarted {
            return Err(Error::InvalidTransition {
                from: format!("{:?}", self.status),
                to: "InProgress".to_string(),
                reason: "Discovery already started".to_string(),
            });
        }

        info!(
            "Starting bootstrap with {} nodes using {} strategy",
            platform_ids.len(),
            self.strategy
        );

        self.tracked_platforms = platform_ids.into_iter().collect();
        self.start_time = Some(Instant::now());
        self.status = BootstrapStatus::InProgress;

        Ok(())
    }

    /// Register a platform assignment to a squad
    #[instrument(skip(self))]
    pub fn register_assignment(&mut self, platform_id: String, squad_id: String) -> Result<()> {
        if self.status != BootstrapStatus::InProgress {
            return Err(Error::InvalidTransition {
                from: format!("{:?}", self.status),
                to: "Assignment".to_string(),
                reason: "Discovery not in progress".to_string(),
            });
        }

        if !self.tracked_platforms.contains(&platform_id) {
            warn!("Attempted to assign unknown platform: {}", platform_id);
            return Ok(());
        }

        debug!(
            "Registering assignment: {} → squad {}",
            platform_id, squad_id
        );

        self.assignments.insert(platform_id, squad_id);
        Ok(())
    }

    /// Increment message count (for metrics)
    pub fn increment_messages(&mut self, count: usize) {
        self.message_count += count;
    }

    /// Check if bootstrap has timed out
    pub fn has_timed_out(&self) -> bool {
        if let Some(start_time) = self.start_time {
            start_time.elapsed() >= self.timeout
        } else {
            false
        }
    }

    /// Get unassigned platform IDs
    pub fn unassigned_platforms(&self) -> Vec<String> {
        self.tracked_platforms
            .iter()
            .filter(|id| !self.assignments.contains_key(*id))
            .cloned()
            .collect()
    }

    /// Get assigned platform IDs
    pub fn assigned_platforms(&self) -> Vec<String> {
        self.assignments.keys().cloned().collect()
    }

    /// Get number of unique cells formed
    pub async fn squads_formed(&self) -> Result<usize> {
        let cells = self.store.get_valid_cells().await?;
        Ok(cells.len())
    }

    /// Check if bootstrap is complete
    ///
    /// Discovery is complete when:
    /// - Timeout has been reached, OR
    /// - All nodes have been assigned (100% completion)
    #[instrument(skip(self))]
    pub async fn check_completion(&mut self) -> Result<bool> {
        if self.status != BootstrapStatus::InProgress {
            return Ok(false);
        }

        let all_assigned = self.unassigned_platforms().is_empty();
        let timed_out = self.has_timed_out();

        if all_assigned {
            info!("Discovery completed: all nodes assigned");
            self.status = BootstrapStatus::Completed;
            return Ok(true);
        }

        if timed_out {
            let assignment_rate =
                self.assignments.len() as f32 / self.tracked_platforms.len() as f32;

            if assignment_rate > 0.9 {
                info!(
                    "Discovery timed out but mostly successful ({:.1}% assigned)",
                    assignment_rate * 100.0
                );
                self.status = BootstrapStatus::Completed;
            } else if assignment_rate > 0.5 {
                warn!(
                    "Discovery timed out with partial completion ({:.1}% assigned)",
                    assignment_rate * 100.0
                );
                self.status = BootstrapStatus::PartiallyCompleted;
            } else {
                warn!(
                    "Discovery failed: timeout with only {:.1}% assigned",
                    assignment_rate * 100.0
                );
                self.status = BootstrapStatus::Failed;
            }
            return Ok(true);
        }

        Ok(false)
    }

    /// Transition to Cell phase
    ///
    /// Should be called after bootstrap completes successfully or times out.
    #[instrument(skip(self))]
    pub async fn transition_to_squad_phase(&mut self) -> Result<()> {
        if self.current_phase != Phase::Discovery {
            return Err(Error::InvalidTransition {
                from: format!("{:?}", self.current_phase),
                to: "Squad".to_string(),
                reason: "Not in Discovery phase".to_string(),
            });
        }

        if self.status == BootstrapStatus::InProgress {
            return Err(Error::InvalidTransition {
                from: format!("{:?}", self.status),
                to: "Squad".to_string(),
                reason: "Discovery still in progress".to_string(),
            });
        }

        info!("Transitioning from Discovery to Cell phase");
        self.current_phase = Phase::Cell;

        Ok(())
    }

    /// Reset bootstrap state for retry
    #[instrument(skip(self))]
    pub fn reset_for_retry(&mut self) -> Result<()> {
        if self.status == BootstrapStatus::InProgress {
            return Err(Error::InvalidTransition {
                from: "InProgress".to_string(),
                to: "Reset".to_string(),
                reason: "Cannot reset while bootstrap is in progress".to_string(),
            });
        }

        info!("Resetting bootstrap coordinator for retry");

        self.status = BootstrapStatus::NotStarted;
        self.start_time = None;
        self.assignments.clear();
        self.message_count = 0;
        // Keep tracked_platforms for retry

        Ok(())
    }

    /// Get current bootstrap metrics
    #[instrument(skip(self))]
    pub async fn get_metrics(&self) -> Result<DiscoveryMetrics> {
        let elapsed = if let Some(start_time) = self.start_time {
            start_time.elapsed().as_secs_f64()
        } else {
            0.0
        };

        let squads_formed = self.squads_formed().await?;

        Ok(DiscoveryMetrics {
            total_platforms: self.tracked_platforms.len(),
            assigned_platforms: self.assignments.len(),
            unassigned_platforms: self.unassigned_platforms().len(),
            squads_formed,
            elapsed_seconds: elapsed,
            strategy: self.strategy,
            status: self.status,
            messages_sent: Some(self.message_count),
        })
    }

    /// Force complete bootstrap (for testing or emergency)
    #[instrument(skip(self))]
    pub fn force_complete(&mut self) -> Result<()> {
        if self.status != BootstrapStatus::InProgress {
            return Err(Error::InvalidTransition {
                from: format!("{:?}", self.status),
                to: "Completed".to_string(),
                reason: "Discovery not in progress".to_string(),
            });
        }

        warn!("Force completing bootstrap");
        self.status = BootstrapStatus::Completed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ditto_store::DittoStore;

    async fn create_test_coordinator() -> Result<DiscoveryCoordinator> {
        let ditto_store = DittoStore::from_env()?;
        let squad_store = CellStore::new(ditto_store);
        Ok(DiscoveryCoordinator::new(
            squad_store,
            BootstrapStrategy::Geographic,
        ))
    }

    #[tokio::test]
    async fn test_coordinator_creation() {
        let coordinator = match create_test_coordinator().await {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        assert_eq!(coordinator.phase(), Phase::Discovery);
        assert_eq!(coordinator.status(), BootstrapStatus::NotStarted);
    }

    #[tokio::test]
    async fn test_start_bootstrap() {
        let mut coordinator = match create_test_coordinator().await {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_ids = vec![
            "node_1".to_string(),
            "node_2".to_string(),
            "node_3".to_string(),
        ];

        coordinator.start_bootstrap(platform_ids).unwrap();

        assert_eq!(coordinator.status(), BootstrapStatus::InProgress);
        assert!(coordinator.start_time.is_some());
        assert_eq!(coordinator.tracked_platforms.len(), 3);
    }

    #[tokio::test]
    async fn test_register_assignments() {
        let mut coordinator = match create_test_coordinator().await {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_ids = vec!["node_1".to_string(), "node_2".to_string()];

        coordinator.start_bootstrap(platform_ids).unwrap();

        coordinator
            .register_assignment("node_1".to_string(), "squad_alpha".to_string())
            .unwrap();

        assert_eq!(coordinator.assignments.len(), 1);
        assert_eq!(coordinator.unassigned_platforms().len(), 1);
        assert_eq!(coordinator.assigned_platforms().len(), 1);
    }

    #[tokio::test]
    async fn test_completion_all_assigned() {
        let mut coordinator = match create_test_coordinator().await {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_ids = vec!["node_1".to_string(), "node_2".to_string()];

        coordinator.start_bootstrap(platform_ids).unwrap();

        coordinator
            .register_assignment("node_1".to_string(), "squad_alpha".to_string())
            .unwrap();
        coordinator
            .register_assignment("node_2".to_string(), "squad_alpha".to_string())
            .unwrap();

        let complete = coordinator.check_completion().await.unwrap();

        assert!(complete);
        assert_eq!(coordinator.status(), BootstrapStatus::Completed);
    }

    #[tokio::test]
    async fn test_timeout_detection() {
        let mut coordinator = match create_test_coordinator().await {
            Ok(c) => c.with_timeout(0), // Immediate timeout
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_ids = vec!["node_1".to_string()];
        coordinator.start_bootstrap(platform_ids).unwrap();

        // Wait a tiny bit for timeout
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert!(coordinator.has_timed_out());
    }

    #[tokio::test]
    async fn test_timeout_with_partial_completion() {
        let mut coordinator = match create_test_coordinator().await {
            Ok(c) => c.with_timeout(0),
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_ids = vec![
            "node_1".to_string(),
            "node_2".to_string(),
            "node_3".to_string(),
        ];

        coordinator.start_bootstrap(platform_ids).unwrap();

        // Assign 2 out of 3 (66%)
        coordinator
            .register_assignment("node_1".to_string(), "squad_alpha".to_string())
            .unwrap();
        coordinator
            .register_assignment("node_2".to_string(), "squad_alpha".to_string())
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let complete = coordinator.check_completion().await.unwrap();

        assert!(complete);
        assert_eq!(coordinator.status(), BootstrapStatus::PartiallyCompleted);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let mut coordinator = match create_test_coordinator().await {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_ids = vec!["node_1".to_string(), "node_2".to_string()];

        coordinator.start_bootstrap(platform_ids).unwrap();
        coordinator
            .register_assignment("node_1".to_string(), "squad_alpha".to_string())
            .unwrap();
        coordinator.increment_messages(10);

        let metrics = coordinator.get_metrics().await.unwrap();

        assert_eq!(metrics.total_platforms, 2);
        assert_eq!(metrics.assigned_platforms, 1);
        assert_eq!(metrics.unassigned_platforms, 1);
        assert_eq!(metrics.assignment_rate(), 0.5);
        assert_eq!(metrics.messages_sent, Some(10));
    }

    #[tokio::test]
    async fn test_phase_transition() {
        let mut coordinator = match create_test_coordinator().await {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_ids = vec!["node_1".to_string()];
        coordinator.start_bootstrap(platform_ids).unwrap();
        coordinator
            .register_assignment("node_1".to_string(), "squad_alpha".to_string())
            .unwrap();

        coordinator.check_completion().await.unwrap();
        coordinator.transition_to_squad_phase().await.unwrap();

        assert_eq!(coordinator.phase(), Phase::Cell);
    }

    #[tokio::test]
    async fn test_reset_for_retry() {
        let mut coordinator = match create_test_coordinator().await {
            Ok(c) => c.with_timeout(0),
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_ids = vec!["node_1".to_string()];
        coordinator.start_bootstrap(platform_ids.clone()).unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        coordinator.check_completion().await.unwrap();

        coordinator.reset_for_retry().unwrap();

        assert_eq!(coordinator.status(), BootstrapStatus::NotStarted);
        assert!(coordinator.assignments.is_empty());
        assert_eq!(coordinator.message_count, 0);
        // Platforms should still be tracked for retry
        assert_eq!(coordinator.tracked_platforms.len(), 1);
    }

    #[tokio::test]
    async fn test_force_complete() {
        let mut coordinator = match create_test_coordinator().await {
            Ok(c) => c,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_ids = vec!["node_1".to_string()];
        coordinator.start_bootstrap(platform_ids).unwrap();

        coordinator.force_complete().unwrap();

        assert_eq!(coordinator.status(), BootstrapStatus::Completed);
    }
}
