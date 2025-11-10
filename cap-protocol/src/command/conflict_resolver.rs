//! Conflict resolution engine for hierarchical commands
//!
//! Handles conflict detection and resolution when multiple commands compete
//! for the same resources or targets according to configured policies.

use crate::error::{Error, Result};
use cap_schema::command::v1::{CommandPriority, ConflictPolicy, HierarchicalCommand};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Result of conflict detection
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictResult {
    /// No conflict detected
    NoConflict,
    /// Conflict detected with existing command(s)
    Conflict(Vec<HierarchicalCommand>),
}

/// Conflict resolution engine
///
/// Enforces ConflictPolicy when multiple commands affect the same target.
/// Maintains an index of active commands by target for efficient conflict detection.
pub struct ConflictResolver {
    /// Active commands indexed by target ID
    /// Key: target_id, Value: list of commands affecting that target
    target_commands: Arc<RwLock<HashMap<String, Vec<HierarchicalCommand>>>>,
}

impl ConflictResolver {
    /// Create a new conflict resolver
    pub fn new() -> Self {
        Self {
            target_commands: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a new command conflicts with existing commands
    ///
    /// Returns ConflictResult::Conflict if there are existing commands
    /// targeting the same resources.
    pub async fn check_conflict(&self, command: &HierarchicalCommand) -> ConflictResult {
        let target_ids = self.extract_target_ids(command);
        let commands = self.target_commands.read().await;

        let mut conflicting = Vec::new();

        for target_id in target_ids {
            if let Some(existing) = commands.get(&target_id) {
                conflicting.extend(existing.clone());
            }
        }

        if conflicting.is_empty() {
            ConflictResult::NoConflict
        } else {
            ConflictResult::Conflict(conflicting)
        }
    }

    /// Resolve conflict according to policy
    ///
    /// Takes a list of conflicting commands and returns the winning command
    /// based on the conflict policy.
    pub fn resolve(
        &self,
        commands: Vec<HierarchicalCommand>,
        policy: ConflictPolicy,
    ) -> Result<HierarchicalCommand> {
        if commands.is_empty() {
            return Err(Error::InvalidInput(
                "Cannot resolve conflict with empty command list".to_string(),
            ));
        }

        if commands.len() == 1 {
            return Ok(commands.into_iter().next().unwrap());
        }

        match policy {
            ConflictPolicy::LastWriteWins => self.resolve_last_write_wins(commands),
            ConflictPolicy::HighestPriorityWins => self.resolve_highest_priority_wins(commands),
            ConflictPolicy::HighestAuthorityWins => self.resolve_highest_authority_wins(commands),
            ConflictPolicy::MergeCompatible => self.resolve_merge_compatible(commands),
            ConflictPolicy::RejectConflict => Err(Error::ConflictDetected(
                "Conflict policy REJECT_CONFLICT: rejecting new command".to_string(),
            )),
            ConflictPolicy::Unspecified => Err(Error::InvalidInput(
                "Conflict policy must be specified".to_string(),
            )),
        }
    }

    /// Register a command as active (after conflict resolution)
    pub async fn register_command(&self, command: &HierarchicalCommand) -> Result<()> {
        let target_ids = self.extract_target_ids(command);
        let mut commands = self.target_commands.write().await;

        for target_id in target_ids {
            commands.entry(target_id).or_default().push(command.clone());
        }

        Ok(())
    }

    /// Remove a command from active tracking (when completed/expired)
    pub async fn unregister_command(&self, command_id: &str) -> Result<()> {
        let mut commands = self.target_commands.write().await;

        // Remove from all target lists
        for (_, cmd_list) in commands.iter_mut() {
            cmd_list.retain(|cmd| cmd.command_id != command_id);
        }

        // Clean up empty target entries
        commands.retain(|_, cmd_list| !cmd_list.is_empty());

        Ok(())
    }

    /// Extract target IDs from a command
    fn extract_target_ids(&self, command: &HierarchicalCommand) -> Vec<String> {
        command
            .target
            .as_ref()
            .map(|t| t.target_ids.clone())
            .unwrap_or_default()
    }

    /// Resolve using LAST_WRITE_WINS policy
    ///
    /// Most recent command (by issued_at timestamp) wins
    fn resolve_last_write_wins(
        &self,
        mut commands: Vec<HierarchicalCommand>,
    ) -> Result<HierarchicalCommand> {
        commands.sort_by(|a, b| {
            let a_time = a.issued_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            let b_time = b.issued_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            b_time.cmp(&a_time) // Descending order (most recent first)
        });

        Ok(commands.into_iter().next().unwrap())
    }

    /// Resolve using HIGHEST_PRIORITY_WINS policy
    ///
    /// Command with highest priority enum value wins
    fn resolve_highest_priority_wins(
        &self,
        mut commands: Vec<HierarchicalCommand>,
    ) -> Result<HierarchicalCommand> {
        commands.sort_by(|a, b| {
            let a_priority =
                CommandPriority::try_from(a.priority).unwrap_or(CommandPriority::Routine);
            let b_priority =
                CommandPriority::try_from(b.priority).unwrap_or(CommandPriority::Routine);
            b_priority.cmp(&a_priority) // Descending order (highest priority first)
        });

        Ok(commands.into_iter().next().unwrap())
    }

    /// Resolve using HIGHEST_AUTHORITY_WINS policy
    ///
    /// Derive authority from originator's hierarchy level.
    /// For now, we use a simple heuristic based on node ID naming convention:
    /// - "zone-*" prefix = authority level 3
    /// - "squad-*" prefix = authority level 2
    /// - other = authority level 1
    fn resolve_highest_authority_wins(
        &self,
        mut commands: Vec<HierarchicalCommand>,
    ) -> Result<HierarchicalCommand> {
        commands.sort_by(|a, b| {
            let a_authority = self.derive_authority_level(&a.originator_id);
            let b_authority = self.derive_authority_level(&b.originator_id);
            b_authority.cmp(&a_authority) // Descending order (highest authority first)
        });

        Ok(commands.into_iter().next().unwrap())
    }

    /// Resolve using MERGE_COMPATIBLE policy
    ///
    /// Check if commands are compatible (same type, non-conflicting params).
    /// For now, this is a placeholder that returns the first command.
    /// Full implementation would require command-type-specific merge logic.
    fn resolve_merge_compatible(
        &self,
        commands: Vec<HierarchicalCommand>,
    ) -> Result<HierarchicalCommand> {
        // TODO: Implement actual compatibility checking and merging
        // For now, just return the first command
        Ok(commands.into_iter().next().unwrap())
    }

    /// Derive authority level from node ID
    ///
    /// Simple heuristic based on naming convention:
    /// - "zone-*" = level 3 (highest)
    /// - "platoon-*" = level 2
    /// - "squad-*" = level 2
    /// - other = level 1
    fn derive_authority_level(&self, node_id: &str) -> u32 {
        if node_id.starts_with("zone-") {
            3
        } else if node_id.starts_with("platoon-") || node_id.starts_with("squad-") {
            2
        } else {
            1
        }
    }
}

impl Default for ConflictResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_schema::command::v1::{command_target::Scope, CommandTarget};
    use cap_schema::common::v1::Timestamp;

    fn create_test_command(
        command_id: &str,
        originator_id: &str,
        target_id: &str,
        priority: i32,
        issued_at_seconds: u64,
    ) -> HierarchicalCommand {
        HierarchicalCommand {
            command_id: command_id.to_string(),
            originator_id: originator_id.to_string(),
            target: Some(CommandTarget {
                scope: Scope::Individual as i32,
                target_ids: vec![target_id.to_string()],
            }),
            priority,
            issued_at: Some(Timestamp {
                seconds: issued_at_seconds,
                nanos: 0,
            }),
            conflict_policy: ConflictPolicy::HighestPriorityWins as i32,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_no_conflict_on_different_targets() {
        let resolver = ConflictResolver::new();

        let cmd1 = create_test_command("cmd-1", "node-1", "target-1", 3, 1000);
        resolver.register_command(&cmd1).await.unwrap();

        let cmd2 = create_test_command("cmd-2", "node-2", "target-2", 3, 1001);
        let result = resolver.check_conflict(&cmd2).await;

        assert_eq!(result, ConflictResult::NoConflict);
    }

    #[tokio::test]
    async fn test_conflict_on_same_target() {
        let resolver = ConflictResolver::new();

        let cmd1 = create_test_command("cmd-1", "node-1", "target-1", 3, 1000);
        resolver.register_command(&cmd1).await.unwrap();

        let cmd2 = create_test_command("cmd-2", "node-2", "target-1", 3, 1001);
        let result = resolver.check_conflict(&cmd2).await;

        match result {
            ConflictResult::Conflict(cmds) => {
                assert_eq!(cmds.len(), 1);
                assert_eq!(cmds[0].command_id, "cmd-1");
            }
            ConflictResult::NoConflict => panic!("Expected conflict"),
        }
    }

    #[tokio::test]
    async fn test_last_write_wins() {
        let resolver = ConflictResolver::new();

        let cmd1 = create_test_command("cmd-1", "node-1", "target-1", 3, 1000);
        let cmd2 = create_test_command("cmd-2", "node-2", "target-1", 3, 1001);
        let cmd3 = create_test_command("cmd-3", "node-3", "target-1", 3, 999);

        let winner = resolver
            .resolve(vec![cmd1, cmd2, cmd3], ConflictPolicy::LastWriteWins)
            .unwrap();

        assert_eq!(winner.command_id, "cmd-2"); // Most recent timestamp (1001)
    }

    #[tokio::test]
    async fn test_highest_priority_wins() {
        let resolver = ConflictResolver::new();

        let cmd1 = create_test_command(
            "cmd-1",
            "node-1",
            "target-1",
            CommandPriority::Routine as i32,
            1000,
        );
        let cmd2 = create_test_command(
            "cmd-2",
            "node-2",
            "target-1",
            CommandPriority::Flash as i32,
            1001,
        );
        let cmd3 = create_test_command(
            "cmd-3",
            "node-3",
            "target-1",
            CommandPriority::Immediate as i32,
            999,
        );

        let winner = resolver
            .resolve(vec![cmd1, cmd2, cmd3], ConflictPolicy::HighestPriorityWins)
            .unwrap();

        assert_eq!(winner.command_id, "cmd-2"); // FLASH priority
    }

    #[tokio::test]
    async fn test_highest_authority_wins() {
        let resolver = ConflictResolver::new();

        let cmd1 = create_test_command("cmd-1", "node-1", "target-1", 3, 1000);
        let cmd2 = create_test_command("cmd-2", "squad-alpha", "target-1", 3, 1001);
        let cmd3 = create_test_command("cmd-3", "zone-leader", "target-1", 3, 999);

        let winner = resolver
            .resolve(vec![cmd1, cmd2, cmd3], ConflictPolicy::HighestAuthorityWins)
            .unwrap();

        assert_eq!(winner.command_id, "cmd-3"); // zone-leader has highest authority
    }

    #[tokio::test]
    async fn test_reject_conflict() {
        let resolver = ConflictResolver::new();

        let cmd1 = create_test_command("cmd-1", "node-1", "target-1", 3, 1000);
        let cmd2 = create_test_command("cmd-2", "node-2", "target-1", 3, 1001);

        let result = resolver.resolve(vec![cmd1, cmd2], ConflictPolicy::RejectConflict);

        assert!(result.is_err());
        assert!(matches!(result, Err(Error::ConflictDetected(_))));
    }

    #[tokio::test]
    async fn test_unregister_command() {
        let resolver = ConflictResolver::new();

        let cmd1 = create_test_command("cmd-1", "node-1", "target-1", 3, 1000);
        resolver.register_command(&cmd1).await.unwrap();

        // Verify command is registered
        let cmd2 = create_test_command("cmd-2", "node-2", "target-1", 3, 1001);
        let result = resolver.check_conflict(&cmd2).await;
        assert!(matches!(result, ConflictResult::Conflict(_)));

        // Unregister cmd-1
        resolver.unregister_command("cmd-1").await.unwrap();

        // Verify no conflict now
        let result = resolver.check_conflict(&cmd2).await;
        assert_eq!(result, ConflictResult::NoConflict);
    }

    #[tokio::test]
    async fn test_authority_level_derivation() {
        let resolver = ConflictResolver::new();

        assert_eq!(resolver.derive_authority_level("zone-leader"), 3);
        assert_eq!(resolver.derive_authority_level("platoon-alpha"), 2);
        assert_eq!(resolver.derive_authority_level("squad-bravo"), 2);
        assert_eq!(resolver.derive_authority_level("node-1"), 1);
    }
}
