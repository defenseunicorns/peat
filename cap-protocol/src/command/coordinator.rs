//! Command coordination and dissemination logic
//!
//! Manages command lifecycle: issuance, routing, acknowledgment, and status tracking.

use crate::command::routing::{CommandRouter, TargetResolution};
use crate::Result;
use cap_schema::command::v1::{
    AckStatus, CommandAcknowledgment, CommandStatus, HierarchicalCommand,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Coordinates hierarchical command dissemination
pub struct CommandCoordinator {
    /// Node identifier
    node_id: String,

    /// Router for target resolution
    router: CommandRouter,

    /// Active commands indexed by command_id
    active_commands: Arc<RwLock<HashMap<String, HierarchicalCommand>>>,

    /// Command acknowledgments indexed by (command_id, node_id)
    acknowledgments: Arc<RwLock<HashMap<(String, String), CommandAcknowledgment>>>,

    /// Command execution status
    command_status: Arc<RwLock<HashMap<String, CommandStatus>>>,
}

impl CommandCoordinator {
    /// Create new command coordinator
    pub fn new(squad_id: Option<String>, node_id: String, squad_members: Vec<String>) -> Self {
        let router = CommandRouter::new(node_id.clone(), squad_id, squad_members, None);

        Self {
            node_id,
            router,
            active_commands: Arc::new(RwLock::new(HashMap::new())),
            acknowledgments: Arc::new(RwLock::new(HashMap::new())),
            command_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Issue a command (originating from this node)
    pub async fn issue_command(&self, command: HierarchicalCommand) -> Result<()> {
        tracing::info!(
            "[{}] Issuing command: {} (priority: {})",
            self.node_id,
            command.command_id,
            command.priority
        );

        // Store in active commands
        self.active_commands
            .write()
            .await
            .insert(command.command_id.clone(), command.clone());

        // Create initial status
        let status = CommandStatus {
            command_id: command.command_id.clone(),
            state: 1, // PENDING
            acknowledgments: Vec::new(),
            last_updated: Some(cap_schema::common::v1::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        };

        self.command_status
            .write()
            .await
            .insert(command.command_id.clone(), status);

        // Route command to targets
        self.route_command(&command).await?;

        Ok(())
    }

    /// Receive a command (from higher echelon)
    pub async fn receive_command(&self, command: HierarchicalCommand) -> Result<()> {
        tracing::info!(
            "[{}] Received command: {} from {}",
            self.node_id,
            command.command_id,
            command.originator_id
        );

        // Resolve target
        let resolution = self.router.resolve_target(&command);

        match resolution {
            TargetResolution::Self_ => {
                // Command targets this node - execute it
                self.execute_command(&command).await?;

                // Send acknowledgment if required
                if self.requires_acknowledgment(&command) {
                    self.send_acknowledgment(&command, AckStatus::AckReceived as i32)
                        .await?;
                }
            }

            TargetResolution::Subordinates(_) | TargetResolution::AllSquadMembers(_) => {
                // Command targets subordinates - route it
                self.route_command(&command).await?;
            }

            TargetResolution::NotApplicable => {
                tracing::debug!(
                    "[{}] Command {} not applicable to this node",
                    self.node_id,
                    command.command_id
                );
            }
        }

        Ok(())
    }

    /// Route command to subordinate nodes
    async fn route_command(&self, command: &HierarchicalCommand) -> Result<()> {
        let resolution = self.router.resolve_target(command);

        if !self.router.should_route(&resolution) {
            return Ok(());
        }

        let targets = self.router.get_routing_targets(&resolution);

        tracing::info!(
            "[{}] Routing command {} to {} nodes",
            self.node_id,
            command.command_id,
            targets.len()
        );

        // In a real implementation, this would publish to Ditto
        // For now, we'll just log the routing action
        for target_id in &targets {
            tracing::debug!(
                "[{}] → Routing command {} to {}",
                self.node_id,
                command.command_id,
                target_id
            );
        }

        Ok(())
    }

    /// Execute a command locally
    async fn execute_command(&self, command: &HierarchicalCommand) -> Result<()> {
        tracing::info!(
            "[{}] Executing command: {}",
            self.node_id,
            command.command_id
        );

        // Update status to EXECUTING
        let mut status_map = self.command_status.write().await;
        if let Some(status) = status_map.get_mut(&command.command_id) {
            status.state = 2; // EXECUTING
        } else {
            status_map.insert(
                command.command_id.clone(),
                CommandStatus {
                    command_id: command.command_id.clone(),
                    state: 2, // EXECUTING
                    acknowledgments: Vec::new(),
                    last_updated: Some(cap_schema::common::v1::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        nanos: 0,
                    }),
                },
            );
        }

        // TODO: Actual command execution logic based on command_type
        // For now, just mark as completed
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Update status to COMPLETED
        if let Some(status) = status_map.get_mut(&command.command_id) {
            status.state = 3; // COMPLETED
        }

        tracing::info!(
            "[{}] ✓ Completed command: {}",
            self.node_id,
            command.command_id
        );

        Ok(())
    }

    /// Check if command requires acknowledgment
    fn requires_acknowledgment(&self, command: &HierarchicalCommand) -> bool {
        // Check acknowledgment_policy
        // 0 = UNSPECIFIED, 1 = NONE, 2 = RECEIVED_ONLY, 3 = COMPLETED_ONLY, 4 = BOTH
        command.acknowledgment_policy > 1
    }

    /// Send acknowledgment for a command
    async fn send_acknowledgment(&self, command: &HierarchicalCommand, status: i32) -> Result<()> {
        let ack = CommandAcknowledgment {
            command_id: command.command_id.clone(),
            node_id: self.node_id.clone(),
            status,
            reason: None,
            timestamp: Some(cap_schema::common::v1::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        };

        tracing::debug!(
            "[{}] Sending ACK for command {} with status {}",
            self.node_id,
            command.command_id,
            status
        );

        // Store acknowledgment
        self.acknowledgments
            .write()
            .await
            .insert((command.command_id.clone(), self.node_id.clone()), ack);

        // In a real implementation, this would publish to Ditto
        // TODO: Publish acknowledgment to hierarchical_commands_acks collection

        Ok(())
    }

    /// Get command status
    pub async fn get_command_status(&self, command_id: &str) -> Option<CommandStatus> {
        self.command_status.read().await.get(command_id).cloned()
    }

    /// Get all acknowledgments for a command
    pub async fn get_command_acknowledgments(
        &self,
        command_id: &str,
    ) -> Vec<CommandAcknowledgment> {
        self.acknowledgments
            .read()
            .await
            .iter()
            .filter(|((cmd_id, _), _)| cmd_id == command_id)
            .map(|(_, ack)| ack.clone())
            .collect()
    }

    /// Check if command has been acknowledged by all targets
    pub async fn is_command_acknowledged(&self, command_id: &str) -> bool {
        let command = match self.active_commands.read().await.get(command_id) {
            Some(cmd) => cmd.clone(),
            None => return false,
        };

        let resolution = self.router.resolve_target(&command);
        let targets = self.router.get_routing_targets(&resolution);

        if targets.is_empty() {
            return true;
        }

        let acks = self.get_command_acknowledgments(command_id).await;
        let acked_nodes: std::collections::HashSet<String> =
            acks.iter().map(|a| a.node_id.clone()).collect();

        targets.iter().all(|t| acked_nodes.contains(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_schema::command::v1::{command_target::Scope, CommandTarget};

    #[tokio::test]
    async fn test_issue_command() {
        let coordinator = CommandCoordinator::new(
            Some("squad-alpha".to_string()),
            "node-1".to_string(),
            vec!["node-1".to_string(), "node-2".to_string()],
        );

        let command = HierarchicalCommand {
            command_id: "cmd-001".to_string(),
            originator_id: "node-1".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Individual as i32,
                target_ids: vec!["node-2".to_string()],
            }),
            priority: 5,
            acknowledgment_policy: 2, // RECEIVED_ONLY
            ..Default::default()
        };

        coordinator.issue_command(command.clone()).await.unwrap();

        let status = coordinator.get_command_status("cmd-001").await;
        assert!(status.is_some());
        assert_eq!(status.unwrap().state, 1); // PENDING
    }

    #[tokio::test]
    async fn test_receive_and_execute_command() {
        let coordinator = CommandCoordinator::new(
            Some("squad-alpha".to_string()),
            "node-1".to_string(),
            vec!["node-1".to_string(), "node-2".to_string()],
        );

        let command = HierarchicalCommand {
            command_id: "cmd-002".to_string(),
            originator_id: "node-leader".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Individual as i32,
                target_ids: vec!["node-1".to_string()],
            }),
            priority: 5,
            acknowledgment_policy: 4, // BOTH
            ..Default::default()
        };

        coordinator.receive_command(command).await.unwrap();

        // Wait for execution
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let status = coordinator.get_command_status("cmd-002").await;
        assert!(status.is_some());
        assert_eq!(status.unwrap().state, 3); // COMPLETED
    }

    #[tokio::test]
    async fn test_acknowledgment_tracking() {
        let coordinator = CommandCoordinator::new(
            Some("squad-alpha".to_string()),
            "node-1".to_string(),
            vec!["node-1".to_string(), "node-2".to_string()],
        );

        let command = HierarchicalCommand {
            command_id: "cmd-003".to_string(),
            originator_id: "node-1".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Individual as i32,
                target_ids: vec!["node-1".to_string()],
            }),
            priority: 5,
            acknowledgment_policy: 2, // RECEIVED_ONLY
            ..Default::default()
        };

        coordinator.receive_command(command).await.unwrap();

        let acks = coordinator.get_command_acknowledgments("cmd-003").await;
        assert!(!acks.is_empty());
        assert_eq!(acks[0].node_id, "node-1");
    }
}
