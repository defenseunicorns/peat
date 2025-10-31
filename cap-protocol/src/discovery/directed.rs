//! C2-Directed Assignment for bootstrap phase
//!
//! Implements Command & Control (C2) directed cell formation where C2 explicitly
//! assigns nodes to cells based on operational requirements.
//!
//! # Architecture
//!
//! Unlike autonomous geographic self-organization (E3.1), C2-directed assignment
//! provides top-down cell formation with explicit authority and validation:
//!
//! ## Assignment Flow
//!
//! 1. **C2 Issues Assignment**: C2 broadcasts `CellAssignment` messages
//! 2. **Node Receives**: Platforms observe assignments via Ditto
//! 3. **Validation**: Node validates assignment (exists, not full, authorized)
//! 4. **Execution**: Node joins squad and updates state
//! 5. **Confirmation**: Assignment status tracked in distributed state
//!
//! ## Message Format
//!
//! ```json
//! {
//!   "assignment_id": "assign_123",
//!   "squad_id": "squad_alpha",
//!   "platform_ids": ["node_1", "node_2", "node_3"],
//!   "issued_by": "c2_controller_1",
//!   "timestamp": 1698765432,
//!   "priority": "high"
//! }
//! ```
//!
//! ## Use Cases
//!
//! - **Pre-planned missions**: Assign nodes based on pre-mission planning
//! - **Capability requirements**: Form cells with specific capability mixes
//! - **Command override**: Override autonomous formation when needed
//! - **Emergency reconstitution**: Rebuild cells after casualties/failures

use crate::storage::CellStore;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, instrument, warn};

/// Assignment priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AssignmentPriority {
    /// Low priority - can be deferred
    Low,
    /// Normal priority - standard assignment
    #[default]
    Normal,
    /// High priority - process immediately
    High,
    /// Critical priority - override existing assignments
    Critical,
}

/// Assignment status tracking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignmentStatus {
    /// Assignment issued but not yet processed
    Pending,
    /// Assignment accepted and in progress
    InProgress,
    /// Assignment completed successfully
    Completed,
    /// Assignment failed validation or execution
    Failed { reason: String },
}

/// Cell assignment message from C2
///
/// This message is broadcast via Ditto and contains explicit platform-to-squad
/// assignments. Platforms observe these messages and execute them if valid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellAssignment {
    /// Unique identifier for this assignment
    pub assignment_id: String,
    /// Target squad ID
    pub squad_id: String,
    /// List of platform IDs to assign to this squad
    pub platform_ids: Vec<String>,
    /// C2 authority issuing the assignment
    pub issued_by: String,
    /// Unix timestamp when assignment was issued
    pub timestamp: u64,
    /// Assignment priority
    pub priority: AssignmentPriority,
    /// Current status of the assignment
    pub status: AssignmentStatus,
    /// Optional operational context or reason
    pub context: Option<String>,
}

impl CellAssignment {
    /// Create a new squad assignment
    pub fn new(
        assignment_id: String,
        squad_id: String,
        platform_ids: Vec<String>,
        issued_by: String,
        priority: AssignmentPriority,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            assignment_id,
            squad_id,
            platform_ids,
            issued_by,
            timestamp,
            priority,
            status: AssignmentStatus::Pending,
            context: None,
        }
    }

    /// Add operational context to the assignment
    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }

    /// Check if assignment includes a specific platform
    pub fn includes_platform(&self, platform_id: &str) -> bool {
        self.platform_ids.iter().any(|id| id == platform_id)
    }

    /// Mark assignment as in progress
    pub fn mark_in_progress(&mut self) {
        self.status = AssignmentStatus::InProgress;
    }

    /// Mark assignment as completed
    pub fn mark_completed(&mut self) {
        self.status = AssignmentStatus::Completed;
    }

    /// Mark assignment as failed
    pub fn mark_failed(&mut self, reason: String) {
        self.status = AssignmentStatus::Failed { reason };
    }

    /// Check if assignment is still active (pending or in progress)
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            AssignmentStatus::Pending | AssignmentStatus::InProgress
        )
    }
}

/// Assignment validation result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// Assignment is valid and can be executed
    Valid,
    /// Cell does not exist
    SquadNotFound,
    /// Cell is full and cannot accept more members
    SquadFull,
    /// Node is already in another squad
    PlatformAlreadyAssigned { current_squad: String },
    /// Assignment is from unauthorized source
    Unauthorized,
    /// Assignment has expired
    Expired,
    /// Other validation error
    Invalid { reason: String },
}

/// C2-Directed Assignment Manager
///
/// Processes C2-issued squad assignments and manages assignment lifecycle.
pub struct DirectedAssignmentManager {
    /// Cell storage
    store: CellStore,
    /// Active assignments being tracked
    assignments: HashMap<String, CellAssignment>,
    /// Node ID of this node
    my_platform_id: String,
    /// Assignment timeout (seconds)
    assignment_timeout: u64,
}

impl DirectedAssignmentManager {
    /// Create a new directed assignment manager
    pub fn new(store: CellStore, my_platform_id: String) -> Self {
        Self {
            store,
            assignments: HashMap::new(),
            my_platform_id,
            assignment_timeout: 300, // 5 minutes default
        }
    }

    /// Set assignment timeout
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.assignment_timeout = timeout_secs;
        self
    }

    /// Process a received squad assignment
    #[instrument(skip(self, assignment))]
    pub async fn process_assignment(&mut self, assignment: CellAssignment) -> Result<()> {
        info!(
            "Processing assignment {} for squad {}",
            assignment.assignment_id, assignment.squad_id
        );

        // Check if this assignment applies to us
        if !assignment.includes_platform(&self.my_platform_id) {
            debug!(
                "Assignment {} does not include platform {}",
                assignment.assignment_id, self.my_platform_id
            );
            return Ok(());
        }

        // Validate the assignment
        let validation = self.validate_assignment(&assignment).await?;
        if validation != ValidationResult::Valid {
            warn!(
                "Assignment {} failed validation: {:?}",
                assignment.assignment_id, validation
            );
            return Err(Error::InvalidTransition {
                from: "Pending assignment".to_string(),
                to: "Executed assignment".to_string(),
                reason: format!("Assignment validation failed: {:?}", validation),
            });
        }

        // Store assignment
        self.assignments
            .insert(assignment.assignment_id.clone(), assignment.clone());

        // Execute the assignment
        self.execute_assignment(assignment).await?;

        Ok(())
    }

    /// Validate a squad assignment
    #[instrument(skip(self, assignment))]
    async fn validate_assignment(&self, assignment: &CellAssignment) -> Result<ValidationResult> {
        debug!("Validating assignment {}", assignment.assignment_id);

        // Check if assignment has expired
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if current_time.saturating_sub(assignment.timestamp) > self.assignment_timeout {
            return Ok(ValidationResult::Expired);
        }

        // Check if squad exists
        let squad = self.store.get_cell(&assignment.squad_id).await?;
        if squad.is_none() {
            return Ok(ValidationResult::SquadNotFound);
        }

        let squad = squad.unwrap();

        // Check if squad can accept new members
        if squad.is_full() {
            return Ok(ValidationResult::SquadFull);
        }

        // Check if platform is already in a squad
        if let Some(current_squad) = self.get_current_squad(&self.my_platform_id).await? {
            if current_squad != assignment.squad_id {
                return Ok(ValidationResult::PlatformAlreadyAssigned {
                    current_squad: current_squad.clone(),
                });
            }
        }

        Ok(ValidationResult::Valid)
    }

    /// Execute a validated assignment
    #[instrument(skip(self, assignment))]
    async fn execute_assignment(&mut self, mut assignment: CellAssignment) -> Result<()> {
        info!(
            "Executing assignment {} - joining squad {}",
            assignment.assignment_id, assignment.squad_id
        );

        assignment.mark_in_progress();

        // Add platform to squad
        self.store
            .add_member(&assignment.squad_id, self.my_platform_id.clone())
            .await?;

        assignment.mark_completed();
        self.assignments
            .insert(assignment.assignment_id.clone(), assignment.clone());

        info!(
            "Assignment {} completed successfully",
            assignment.assignment_id
        );

        Ok(())
    }

    /// Get the current squad for a platform
    async fn get_current_squad(&self, platform_id: &str) -> Result<Option<String>> {
        let valid_squads = self.store.get_valid_cells().await?;

        for squad in valid_squads {
            if squad.is_member(platform_id) {
                return Ok(Some(squad.config.id.clone()));
            }
        }

        Ok(None)
    }

    /// Get assignment by ID
    pub fn get_assignment(&self, assignment_id: &str) -> Option<&CellAssignment> {
        self.assignments.get(assignment_id)
    }

    /// Get all active assignments
    pub fn active_assignments(&self) -> Vec<&CellAssignment> {
        self.assignments
            .values()
            .filter(|a| a.is_active())
            .collect()
    }

    /// Clean up expired assignments
    pub fn cleanup_expired(&mut self) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.assignments.retain(|_, assignment| {
            current_time.saturating_sub(assignment.timestamp) <= self.assignment_timeout
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CellConfig, CellState};
    use crate::storage::ditto_store::DittoStore;
    use crate::storage::CellStore;

    #[test]
    fn test_assignment_creation() {
        let assignment = CellAssignment::new(
            "assign_1".to_string(),
            "squad_alpha".to_string(),
            vec!["node_1".to_string(), "node_2".to_string()],
            "c2_controller".to_string(),
            AssignmentPriority::High,
        );

        assert_eq!(assignment.assignment_id, "assign_1");
        assert_eq!(assignment.squad_id, "squad_alpha");
        assert_eq!(assignment.platform_ids.len(), 2);
        assert_eq!(assignment.priority, AssignmentPriority::High);
        assert_eq!(assignment.status, AssignmentStatus::Pending);
        assert!(assignment.is_active());
    }

    #[test]
    fn test_assignment_includes_platform() {
        let assignment = CellAssignment::new(
            "assign_1".to_string(),
            "squad_alpha".to_string(),
            vec!["node_1".to_string(), "node_2".to_string()],
            "c2_controller".to_string(),
            AssignmentPriority::Normal,
        );

        assert!(assignment.includes_platform("node_1"));
        assert!(assignment.includes_platform("node_2"));
        assert!(!assignment.includes_platform("node_3"));
    }

    #[test]
    fn test_assignment_status_transitions() {
        let mut assignment = CellAssignment::new(
            "assign_1".to_string(),
            "squad_alpha".to_string(),
            vec!["node_1".to_string()],
            "c2_controller".to_string(),
            AssignmentPriority::Normal,
        );

        assert_eq!(assignment.status, AssignmentStatus::Pending);
        assert!(assignment.is_active());

        assignment.mark_in_progress();
        assert_eq!(assignment.status, AssignmentStatus::InProgress);
        assert!(assignment.is_active());

        assignment.mark_completed();
        assert_eq!(assignment.status, AssignmentStatus::Completed);
        assert!(!assignment.is_active());
    }

    #[test]
    fn test_assignment_with_context() {
        let assignment = CellAssignment::new(
            "assign_1".to_string(),
            "squad_alpha".to_string(),
            vec!["node_1".to_string()],
            "c2_controller".to_string(),
            AssignmentPriority::Normal,
        )
        .with_context("Emergency reconstitution after casualty".to_string());

        assert!(assignment.context.is_some());
        assert_eq!(
            assignment.context.unwrap(),
            "Emergency reconstitution after casualty"
        );
    }

    #[tokio::test]
    async fn test_directed_assignment_manager() {
        let ditto_store = match DittoStore::from_env() {
            Ok(store) => store,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let squad_store = CellStore::new(ditto_store);
        let manager = DirectedAssignmentManager::new(squad_store, "node_1".to_string());

        assert_eq!(manager.my_platform_id, "node_1");
        assert_eq!(manager.assignment_timeout, 300);
        assert_eq!(manager.active_assignments().len(), 0);
    }

    #[tokio::test]
    async fn test_assignment_validation() {
        let ditto_store = match DittoStore::from_env() {
            Ok(store) => store,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let squad_store = CellStore::new(ditto_store);

        // Create a test squad
        let config = CellConfig::new(5);
        let squad = CellState::new(config.clone());
        let _ = squad_store.store_cell(&squad).await;

        let manager = DirectedAssignmentManager::new(squad_store, "node_1".to_string());

        // Create valid assignment
        let assignment = CellAssignment::new(
            "assign_1".to_string(),
            config.id.clone(),
            vec!["node_1".to_string()],
            "c2_controller".to_string(),
            AssignmentPriority::Normal,
        );

        let validation = manager.validate_assignment(&assignment).await.unwrap();
        assert_eq!(validation, ValidationResult::Valid);
    }

    #[tokio::test]
    async fn test_assignment_nonexistent_squad() {
        let ditto_store = match DittoStore::from_env() {
            Ok(store) => store,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let squad_store = CellStore::new(ditto_store);
        let manager = DirectedAssignmentManager::new(squad_store, "node_1".to_string());

        let assignment = CellAssignment::new(
            "assign_1".to_string(),
            "nonexistent_squad".to_string(),
            vec!["node_1".to_string()],
            "c2_controller".to_string(),
            AssignmentPriority::Normal,
        );

        let validation = manager.validate_assignment(&assignment).await.unwrap();
        assert_eq!(validation, ValidationResult::SquadNotFound);
    }

    #[tokio::test]
    async fn test_assignment_cleanup() {
        let ditto_store = match DittoStore::from_env() {
            Ok(store) => store,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let squad_store = CellStore::new(ditto_store);
        let mut manager =
            DirectedAssignmentManager::new(squad_store, "node_1".to_string()).with_timeout(1);

        let mut assignment = CellAssignment::new(
            "assign_1".to_string(),
            "squad_alpha".to_string(),
            vec!["node_1".to_string()],
            "c2_controller".to_string(),
            AssignmentPriority::Normal,
        );

        // Set old timestamp
        assignment.timestamp = 0;

        manager
            .assignments
            .insert(assignment.assignment_id.clone(), assignment);

        assert_eq!(manager.assignments.len(), 1);

        manager.cleanup_expired();

        assert_eq!(manager.assignments.len(), 0);
    }
}
