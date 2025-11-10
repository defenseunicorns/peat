//! # Hierarchical Command Coordination
//!
//! This module implements bidirectional command dissemination with policy-based flexibility.
//!
//! ## Overview
//!
//! Complements upward capability aggregation with downward command propagation:
//! - **Policy-based routing**: Commands carry routing policies (buffer, conflict, acknowledgment)
//! - **Hierarchical dissemination**: Commands flow down through Zone → Cell → Node hierarchy
//! - **Acknowledgment tracking**: Optional acknowledgment based on command policy
//! - **Conflict resolution**: Deterministic resolution when multiple commands target same resource
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │  Zone Commander (Higher Echelon)                │
//! │  Issues HierarchicalCommand with policies       │
//! └────────────────────┬─────────────────────────────┘
//!                      │
//!           ┌──────────┴──────────┐
//!           ▼                     ▼
//!    ┌─────────────┐       ┌─────────────┐
//!    │  Cell 1     │       │  Cell 2     │
//!    │  Receives   │       │  Receives   │
//!    │  & Routes   │       │  & Routes   │
//!    └──────┬──────┘       └──────┬──────┘
//!           │                     │
//!      ┌────┴────┐           ┌────┴────┐
//!      ▼         ▼           ▼         ▼
//!   Node-1   Node-2      Node-3   Node-4
//!   (Execute & Ack)     (Execute & Ack)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use cap_protocol::command::CommandCoordinator;
//! use cap_schema::command::v1::HierarchicalCommand;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create coordinator for a squad leader
//! let coordinator = CommandCoordinator::new(
//!     Some("squad-alpha".to_string()),
//!     "node-1".to_string(),
//!     vec!["node-1".to_string(), "node-2".to_string()], // squad members
//! );
//!
//! // Issue a command to squad members
//! let command = HierarchicalCommand {
//!     command_id: "cmd-001".to_string(),
//!     originator_id: "node-1".to_string(),
//!     // ... command details
//!     ..Default::default()
//! };
//!
//! coordinator.issue_command(command).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Related ADRs
//!
//! - ADR-014: Distributed Coordination Primitives
//! - ADR-013: AI Operations and Binary Transfer
//! - ADR-009: Bidirectional Hierarchical Flows

mod conflict_resolver;
mod coordinator;
mod policy_impl; // Conflictable implementation for HierarchicalCommand
mod routing;
mod timeout_manager;

pub use conflict_resolver::{ConflictResolver, ConflictResult};
pub use coordinator::CommandCoordinator;
pub use routing::{CommandRouter, TargetResolution};
pub use timeout_manager::{AckTimeout, TimeoutManager};

#[cfg(test)]
mod tests {
    #[test]
    fn test_command_module_accessible() {
        // Verify module compiles and types are accessible
        let _node_id = "test-node".to_string();
        // Module compiles successfully
    }
}
