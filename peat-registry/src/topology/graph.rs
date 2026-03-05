use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::EdgeConfig;
use crate::error::{RegistryError, Result};
use crate::types::RegistryTarget;

/// A directed edge in the registry topology (child pulls from parent).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegistryEdge {
    pub parent_id: String,
    pub child_id: String,
    /// Lower preference = preferred source.
    pub preference: u32,
    pub max_fanout: Option<usize>,
    pub bandwidth_budget_bytes_per_hour: Option<u64>,
}

/// DAG of registry targets connected by sync edges.
pub struct RegistryGraph {
    /// target_id → RegistryTarget
    pub targets: HashMap<String, RegistryTarget>,
    /// child_id → list of edges (sorted by preference, ascending)
    pub edges: HashMap<String, Vec<RegistryEdge>>,
    /// target_id → wave number
    pub wave_assignments: HashMap<String, u32>,
}

impl RegistryGraph {
    pub fn new(targets: Vec<RegistryTarget>, edge_configs: &[EdgeConfig]) -> Result<Self> {
        let target_map: HashMap<String, RegistryTarget> =
            targets.into_iter().map(|t| (t.id.clone(), t)).collect();

        let mut edges: HashMap<String, Vec<RegistryEdge>> = HashMap::new();

        for ec in edge_configs {
            if !target_map.contains_key(&ec.parent_id) {
                return Err(RegistryError::Topology(format!(
                    "parent '{}' not found in targets",
                    ec.parent_id
                )));
            }
            if !target_map.contains_key(&ec.child_id) {
                return Err(RegistryError::Topology(format!(
                    "child '{}' not found in targets",
                    ec.child_id
                )));
            }

            let edge = RegistryEdge {
                parent_id: ec.parent_id.clone(),
                child_id: ec.child_id.clone(),
                preference: ec.preference,
                max_fanout: ec.max_fanout,
                bandwidth_budget_bytes_per_hour: ec.bandwidth_budget_bytes_per_hour,
            };

            edges.entry(ec.child_id.clone()).or_default().push(edge);
        }

        // Sort edges by preference (ascending — lower is better)
        for edge_list in edges.values_mut() {
            edge_list.sort_by_key(|e| e.preference);
        }

        // Compute wave assignments from topology depth
        let wave_assignments = Self::compute_waves(&target_map, &edges);

        Ok(Self {
            targets: target_map,
            edges,
            wave_assignments,
        })
    }

    /// Get parent edges for a target (sorted by preference).
    pub fn parents(&self, child_id: &str) -> &[RegistryEdge] {
        self.edges
            .get(child_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get child edges for a target.
    pub fn children(&self, parent_id: &str) -> Vec<&RegistryEdge> {
        self.edges
            .values()
            .flatten()
            .filter(|e| e.parent_id == parent_id)
            .collect()
    }

    /// Get targets in a specific wave.
    pub fn targets_in_wave(&self, wave: u32) -> Vec<&str> {
        self.wave_assignments
            .iter()
            .filter(|(_, w)| **w == wave)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Maximum wave number in the graph.
    pub fn max_wave(&self) -> u32 {
        self.wave_assignments.values().copied().max().unwrap_or(0)
    }

    /// Compute wave assignments based on topology depth.
    /// Root nodes (no parents) are wave 0, their children are wave 1, etc.
    fn compute_waves(
        targets: &HashMap<String, RegistryTarget>,
        edges: &HashMap<String, Vec<RegistryEdge>>,
    ) -> HashMap<String, u32> {
        let mut waves = HashMap::new();

        // Find roots (nodes with no parents)
        for id in targets.keys() {
            if !edges.contains_key(id) || edges[id].is_empty() {
                waves.insert(id.clone(), 0);
            }
        }

        // BFS to assign waves
        let mut changed = true;
        while changed {
            changed = false;
            for (child_id, child_edges) in edges {
                if waves.contains_key(child_id) {
                    continue;
                }
                // Check if all parents have waves assigned
                let parent_waves: Vec<u32> = child_edges
                    .iter()
                    .filter_map(|e| waves.get(&e.parent_id).copied())
                    .collect();

                if !parent_waves.is_empty() && parent_waves.len() == child_edges.len() {
                    let max_parent_wave = parent_waves.into_iter().max().unwrap_or(0);
                    waves.insert(child_id.clone(), max_parent_wave + 1);
                    changed = true;
                }
            }
        }

        // Assign remaining (disconnected) nodes to wave 0
        for id in targets.keys() {
            waves.entry(id.clone()).or_insert(0);
        }

        waves
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RegistryAuth, RegistryTier};

    fn make_target(id: &str, tier: RegistryTier) -> RegistryTarget {
        RegistryTarget {
            id: id.to_string(),
            endpoint: format!("https://{}.example.com", id),
            tier,
            auth: RegistryAuth::Anonymous,
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_graph_construction() {
        let targets = vec![
            make_target("enterprise", RegistryTier::Enterprise),
            make_target("regional", RegistryTier::Regional),
            make_target("tactical", RegistryTier::Tactical),
        ];
        let edges = vec![
            EdgeConfig {
                parent_id: "enterprise".into(),
                child_id: "regional".into(),
                preference: 1,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
            EdgeConfig {
                parent_id: "regional".into(),
                child_id: "tactical".into(),
                preference: 1,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
        ];

        let graph = RegistryGraph::new(targets, &edges).unwrap();
        assert_eq!(graph.targets.len(), 3);
        assert_eq!(graph.parents("regional").len(), 1);
        assert_eq!(graph.parents("tactical").len(), 1);
        assert_eq!(graph.parents("enterprise").len(), 0);
    }

    #[test]
    fn test_wave_assignments() {
        let targets = vec![
            make_target("root", RegistryTier::Enterprise),
            make_target("mid", RegistryTier::Regional),
            make_target("leaf", RegistryTier::Tactical),
        ];
        let edges = vec![
            EdgeConfig {
                parent_id: "root".into(),
                child_id: "mid".into(),
                preference: 1,
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
        assert_eq!(graph.wave_assignments["root"], 0);
        assert_eq!(graph.wave_assignments["mid"], 1);
        assert_eq!(graph.wave_assignments["leaf"], 2);
        assert_eq!(graph.max_wave(), 2);
    }

    #[test]
    fn test_edge_preference_sorting() {
        let targets = vec![
            make_target("parent-a", RegistryTier::Enterprise),
            make_target("parent-b", RegistryTier::Regional),
            make_target("child", RegistryTier::Tactical),
        ];
        let edges = vec![
            EdgeConfig {
                parent_id: "parent-b".into(),
                child_id: "child".into(),
                preference: 5,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
            EdgeConfig {
                parent_id: "parent-a".into(),
                child_id: "child".into(),
                preference: 1,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
        ];

        let graph = RegistryGraph::new(targets, &edges).unwrap();
        let parents = graph.parents("child");
        assert_eq!(parents.len(), 2);
        assert_eq!(parents[0].parent_id, "parent-a"); // preference 1 first
        assert_eq!(parents[1].parent_id, "parent-b"); // preference 5 second
    }

    #[test]
    fn test_invalid_edge_parent() {
        let targets = vec![make_target("a", RegistryTier::Enterprise)];
        let edges = vec![EdgeConfig {
            parent_id: "nonexistent".into(),
            child_id: "a".into(),
            preference: 1,
            max_fanout: None,
            bandwidth_budget_bytes_per_hour: None,
        }];

        let result = RegistryGraph::new(targets, &edges);
        assert!(result.is_err());
    }

    #[test]
    fn test_targets_in_wave() {
        let targets = vec![
            make_target("root", RegistryTier::Enterprise),
            make_target("a", RegistryTier::Regional),
            make_target("b", RegistryTier::Regional),
        ];
        let edges = vec![
            EdgeConfig {
                parent_id: "root".into(),
                child_id: "a".into(),
                preference: 1,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
            EdgeConfig {
                parent_id: "root".into(),
                child_id: "b".into(),
                preference: 1,
                max_fanout: None,
                bandwidth_budget_bytes_per_hour: None,
            },
        ];

        let graph = RegistryGraph::new(targets, &edges).unwrap();
        let wave0 = graph.targets_in_wave(0);
        assert_eq!(wave0.len(), 1);
        assert!(wave0.contains(&"root"));

        let mut wave1 = graph.targets_in_wave(1);
        wave1.sort();
        assert_eq!(wave1.len(), 2);
        assert!(wave1.contains(&"a"));
        assert!(wave1.contains(&"b"));
    }
}
