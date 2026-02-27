//! Policy engine implementations for HierarchicalCommand
//!
//! Implements the generic Conflictable trait for commands and provides
//! adapters for converting between the old command-specific types and
//! the new generic policy engine.

use crate::policy::{AttributeValue, Conflictable};
use peat_schema::command::v1::HierarchicalCommand;
use std::collections::HashMap;

/// Implement Conflictable for HierarchicalCommand
///
/// This allows commands to use the generic policy engine.
impl Conflictable for HierarchicalCommand {
    fn id(&self) -> String {
        self.command_id.clone()
    }

    fn conflict_keys(&self) -> Vec<String> {
        self.target
            .as_ref()
            .map(|t| t.target_ids.clone())
            .unwrap_or_default()
    }

    fn timestamp(&self) -> Option<u64> {
        self.issued_at.as_ref().map(|t| t.seconds)
    }

    fn attributes(&self) -> HashMap<String, AttributeValue> {
        let mut attrs = HashMap::new();

        // Priority
        attrs.insert(
            "priority".to_string(),
            AttributeValue::Int(self.priority as i64),
        );

        // Originator ID
        attrs.insert(
            "originator_id".to_string(),
            AttributeValue::String(self.originator_id.clone()),
        );

        // Derive authority level from originator ID (simple heuristic)
        let authority_level = derive_authority_level(&self.originator_id);
        attrs.insert(
            "authority_level".to_string(),
            AttributeValue::Int(authority_level),
        );

        // Conflict policy
        attrs.insert(
            "conflict_policy".to_string(),
            AttributeValue::Int(self.conflict_policy as i64),
        );

        // Acknowledgment policy
        attrs.insert(
            "acknowledgment_policy".to_string(),
            AttributeValue::Int(self.acknowledgment_policy as i64),
        );

        // Version for conflict resolution
        attrs.insert(
            "version".to_string(),
            AttributeValue::Uint(self.version as u64),
        );

        attrs
    }
}

/// Derive authority level from node ID
///
/// Simple heuristic based on naming convention:
/// - "zone-*" = level 3 (highest)
/// - "platoon-*" = level 2
/// - "squad-*" = level 2
/// - other = level 1
fn derive_authority_level(node_id: &str) -> i64 {
    if node_id.starts_with("zone-") {
        3
    } else if node_id.starts_with("platoon-") || node_id.starts_with("squad-") {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peat_schema::command::v1::{command_target::Scope, CommandTarget};
    use peat_schema::common::v1::Timestamp;

    #[test]
    fn test_conflictable_implementation() {
        let command = HierarchicalCommand {
            command_id: "cmd-001".to_string(),
            originator_id: "zone-leader".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Individual as i32,
                target_ids: vec!["node-1".to_string(), "node-2".to_string()],
            }),
            priority: 5,
            issued_at: Some(Timestamp {
                seconds: 1000,
                nanos: 0,
            }),
            version: 1,
            ..Default::default()
        };

        // Test id()
        assert_eq!(command.id(), "cmd-001");

        // Test conflict_keys()
        let keys = command.conflict_keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"node-1".to_string()));
        assert!(keys.contains(&"node-2".to_string()));

        // Test timestamp()
        assert_eq!(command.timestamp(), Some(1000));

        // Test attributes()
        let attrs = command.attributes();
        assert_eq!(attrs.get("priority").unwrap().as_int(), 5);
        assert_eq!(
            attrs.get("originator_id").unwrap().as_string(),
            "zone-leader"
        );
        assert_eq!(attrs.get("authority_level").unwrap().as_int(), 3); // zone = level 3
        assert_eq!(attrs.get("version").unwrap().as_uint(), 1);
    }

    #[test]
    fn test_authority_level_derivation() {
        assert_eq!(derive_authority_level("zone-alpha"), 3);
        assert_eq!(derive_authority_level("platoon-1"), 2);
        assert_eq!(derive_authority_level("squad-bravo"), 2);
        assert_eq!(derive_authority_level("node-123"), 1);
        assert_eq!(derive_authority_level("unknown"), 1);
    }

    #[test]
    fn test_command_without_target() {
        let command = HierarchicalCommand {
            command_id: "cmd-002".to_string(),
            originator_id: "node-1".to_string(),
            target: None,
            ..Default::default()
        };

        let keys = command.conflict_keys();
        assert_eq!(keys.len(), 0); // No conflict keys if no target
    }
}
