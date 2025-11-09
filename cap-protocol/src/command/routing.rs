//! Command routing logic for hierarchical dissemination
//!
//! Implements target resolution and routing decisions based on command policies.

use cap_schema::command::v1::{command_target::Scope, HierarchicalCommand};
use std::collections::HashSet;

/// Resolves command targets to specific node IDs
pub struct CommandRouter {
    /// Current node ID
    node_id: String,

    /// Squad ID (if member of a squad)
    squad_id: Option<String>,

    /// Squad members (if leader)
    squad_members: Vec<String>,

    /// Platoon ID (if member of a platoon)
    platoon_id: Option<String>,
}

/// Result of target resolution
#[derive(Debug, Clone, PartialEq)]
pub enum TargetResolution {
    /// Command targets this node directly
    Self_,

    /// Command targets subordinate nodes (IDs listed)
    Subordinates(Vec<String>),

    /// Command targets all squad members
    AllSquadMembers(Vec<String>),

    /// Command does not target this node or subordinates
    NotApplicable,
}

impl CommandRouter {
    /// Create new router for a node
    pub fn new(
        node_id: String,
        squad_id: Option<String>,
        squad_members: Vec<String>,
        platoon_id: Option<String>,
    ) -> Self {
        Self {
            node_id,
            squad_id,
            squad_members,
            platoon_id,
        }
    }

    /// Resolve command target to specific nodes
    pub fn resolve_target(&self, command: &HierarchicalCommand) -> TargetResolution {
        let target = match &command.target {
            Some(t) => t,
            None => return TargetResolution::NotApplicable,
        };

        let scope = Scope::try_from(target.scope).unwrap_or(Scope::Unspecified);

        match scope {
            Scope::Individual => {
                // Target specific individuals
                let target_ids: HashSet<String> = target.target_ids.iter().cloned().collect();

                if target_ids.contains(&self.node_id) {
                    TargetResolution::Self_
                } else {
                    // Check if any subordinates are targeted
                    let subordinate_targets: Vec<String> = self
                        .squad_members
                        .iter()
                        .filter(|m| target_ids.contains(*m))
                        .cloned()
                        .collect();

                    if !subordinate_targets.is_empty() {
                        TargetResolution::Subordinates(subordinate_targets)
                    } else {
                        TargetResolution::NotApplicable
                    }
                }
            }

            Scope::Squad => {
                // Target entire squad(s)
                if let Some(ref my_squad) = self.squad_id {
                    if target.target_ids.contains(my_squad) {
                        // This squad is targeted
                        if !self.squad_members.is_empty() {
                            // This node is squad leader - target all members
                            TargetResolution::AllSquadMembers(self.squad_members.clone())
                        } else {
                            // This node is a squad member - target self
                            TargetResolution::Self_
                        }
                    } else {
                        TargetResolution::NotApplicable
                    }
                } else {
                    TargetResolution::NotApplicable
                }
            }

            Scope::Platoon => {
                // Target entire platoon(s)
                if let Some(ref my_platoon) = self.platoon_id {
                    if target.target_ids.contains(my_platoon) {
                        // This platoon is targeted
                        if !self.squad_members.is_empty() {
                            // This node is squad leader - target all members
                            TargetResolution::AllSquadMembers(self.squad_members.clone())
                        } else {
                            // This node is a platoon member - target self
                            TargetResolution::Self_
                        }
                    } else {
                        TargetResolution::NotApplicable
                    }
                } else {
                    TargetResolution::NotApplicable
                }
            }

            Scope::Broadcast => {
                // Broadcast to all nodes
                if !self.squad_members.is_empty() {
                    // Squad leader - target all members
                    TargetResolution::AllSquadMembers(self.squad_members.clone())
                } else {
                    // Regular node - target self
                    TargetResolution::Self_
                }
            }

            Scope::Unspecified => TargetResolution::NotApplicable,
        }
    }

    /// Check if this node should route the command downward
    pub fn should_route(&self, resolution: &TargetResolution) -> bool {
        matches!(
            resolution,
            TargetResolution::Subordinates(_) | TargetResolution::AllSquadMembers(_)
        )
    }

    /// Get list of nodes to route command to
    pub fn get_routing_targets(&self, resolution: &TargetResolution) -> Vec<String> {
        match resolution {
            TargetResolution::Subordinates(nodes) => nodes.clone(),
            TargetResolution::AllSquadMembers(nodes) => nodes.clone(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_schema::command::v1::CommandTarget;

    #[test]
    fn test_resolve_individual_self() {
        let router = CommandRouter::new(
            "node-1".to_string(),
            Some("squad-alpha".to_string()),
            vec!["node-1".to_string(), "node-2".to_string()],
            None,
        );

        let command = HierarchicalCommand {
            command_id: "cmd-1".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Individual as i32,
                target_ids: vec!["node-1".to_string()],
            }),
            ..Default::default()
        };

        let resolution = router.resolve_target(&command);
        assert_eq!(resolution, TargetResolution::Self_);
    }

    #[test]
    fn test_resolve_individual_subordinate() {
        let router = CommandRouter::new(
            "node-1".to_string(),
            Some("squad-alpha".to_string()),
            vec!["node-1".to_string(), "node-2".to_string()],
            None,
        );

        let command = HierarchicalCommand {
            command_id: "cmd-1".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Individual as i32,
                target_ids: vec!["node-2".to_string()],
            }),
            ..Default::default()
        };

        let resolution = router.resolve_target(&command);
        assert_eq!(
            resolution,
            TargetResolution::Subordinates(vec!["node-2".to_string()])
        );
    }

    #[test]
    fn test_resolve_squad() {
        let router = CommandRouter::new(
            "node-1".to_string(),
            Some("squad-alpha".to_string()),
            vec![
                "node-1".to_string(),
                "node-2".to_string(),
                "node-3".to_string(),
            ],
            None,
        );

        let command = HierarchicalCommand {
            command_id: "cmd-1".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Squad as i32,
                target_ids: vec!["squad-alpha".to_string()],
            }),
            ..Default::default()
        };

        let resolution = router.resolve_target(&command);
        if let TargetResolution::AllSquadMembers(members) = resolution {
            assert_eq!(members.len(), 3);
        } else {
            panic!("Expected AllSquadMembers resolution");
        }
    }

    #[test]
    fn test_should_route() {
        let router = CommandRouter::new(
            "node-1".to_string(),
            Some("squad-alpha".to_string()),
            vec!["node-1".to_string(), "node-2".to_string()],
            None,
        );

        assert!(router.should_route(&TargetResolution::Subordinates(vec!["node-2".to_string()])));
        assert!(!router.should_route(&TargetResolution::Self_));
        assert!(!router.should_route(&TargetResolution::NotApplicable));
    }
}
