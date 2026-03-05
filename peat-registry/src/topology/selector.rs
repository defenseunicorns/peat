use std::collections::HashMap;

use crate::topology::graph::RegistryGraph;
use crate::types::{ConvergenceStatus, TargetConvergenceState};

/// Select the best source for a target based on topology edges and convergence state.
///
/// Walks parent edges in preference order, preferring the nearest converged parent.
/// Falls back to highest-preference parent if none are converged.
pub fn select_source(
    graph: &RegistryGraph,
    target_id: &str,
    convergence_states: &HashMap<String, TargetConvergenceState>,
) -> Option<String> {
    let parents = graph.parents(target_id);
    if parents.is_empty() {
        return None;
    }

    // First pass: prefer converged parents (already sorted by preference)
    for edge in parents {
        if let Some(state) = convergence_states.get(&edge.parent_id) {
            if state.status == ConvergenceStatus::Converged {
                return Some(edge.parent_id.clone());
            }
        }
    }

    // Second pass: prefer content-complete parents
    for edge in parents {
        if let Some(state) = convergence_states.get(&edge.parent_id) {
            if state.status == ConvergenceStatus::ContentComplete {
                return Some(edge.parent_id.clone());
            }
        }
    }

    // Fallback: highest preference parent regardless of state
    Some(parents[0].parent_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EdgeConfig;
    use crate::types::{
        ConvergenceStatus, RegistryAuth, RegistryTarget, RegistryTier, TargetConvergenceState,
    };
    use chrono::Utc;

    fn make_target(id: &str) -> RegistryTarget {
        RegistryTarget {
            id: id.to_string(),
            endpoint: format!("https://{}.example.com", id),
            tier: RegistryTier::Regional,
            auth: RegistryAuth::Anonymous,
            metadata: Default::default(),
        }
    }

    fn make_state(id: &str, status: ConvergenceStatus) -> TargetConvergenceState {
        TargetConvergenceState {
            target_id: id.to_string(),
            intent_id: "test".to_string(),
            status,
            remaining_delta: None,
            active_checkpoint: None,
            blockers: vec![],
            last_updated: Utc::now(),
        }
    }

    #[test]
    fn test_select_converged_parent() {
        let targets = vec![make_target("root"), make_target("mid"), make_target("leaf")];
        let edges = vec![
            EdgeConfig {
                parent_id: "root".into(),
                child_id: "mid".into(),
                preference: 1,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
            EdgeConfig {
                parent_id: "root".into(),
                child_id: "leaf".into(),
                preference: 2,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
            EdgeConfig {
                parent_id: "mid".into(),
                child_id: "leaf".into(),
                preference: 1,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
        ];

        let graph = RegistryGraph::new(targets, &edges).unwrap();

        let mut states = HashMap::new();
        states.insert(
            "root".to_string(),
            make_state("root", ConvergenceStatus::Converged),
        );
        states.insert(
            "mid".to_string(),
            make_state("mid", ConvergenceStatus::Converged),
        );

        // leaf has two parents: mid (pref 1, converged) and root (pref 2, converged)
        // Should pick mid (lower preference number = more preferred)
        let source = select_source(&graph, "leaf", &states);
        assert_eq!(source.as_deref(), Some("mid"));
    }

    #[test]
    fn test_select_fallback_to_non_converged() {
        let targets = vec![make_target("parent"), make_target("child")];
        let edges = vec![EdgeConfig {
            parent_id: "parent".into(),
            child_id: "child".into(),
            preference: 1,
            max_fanout: None,
            bandwidth_budget_bytes_per_hour: None,
        }];

        let graph = RegistryGraph::new(targets, &edges).unwrap();
        let states = HashMap::new(); // No convergence info

        let source = select_source(&graph, "child", &states);
        assert_eq!(source.as_deref(), Some("parent"));
    }

    #[test]
    fn test_select_no_parents() {
        let targets = vec![make_target("root")];
        let graph = RegistryGraph::new(targets, &[]).unwrap();
        let states = HashMap::new();

        let source = select_source(&graph, "root", &states);
        assert!(source.is_none());
    }

    #[test]
    fn test_select_prefers_content_complete_over_unknown() {
        let targets = vec![make_target("a"), make_target("b"), make_target("child")];
        let edges = vec![
            EdgeConfig {
                parent_id: "a".into(),
                child_id: "child".into(),
                preference: 1,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
            EdgeConfig {
                parent_id: "b".into(),
                child_id: "child".into(),
                preference: 2,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
        ];

        let graph = RegistryGraph::new(targets, &edges).unwrap();
        let mut states = HashMap::new();
        states.insert("a".to_string(), make_state("a", ConvergenceStatus::Pending));
        states.insert(
            "b".to_string(),
            make_state("b", ConvergenceStatus::ContentComplete),
        );

        // b is content-complete, a is only pending — prefer b
        let source = select_source(&graph, "child", &states);
        assert_eq!(source.as_deref(), Some("b"));
    }
}
