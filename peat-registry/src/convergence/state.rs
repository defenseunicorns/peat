use std::collections::HashMap;
use std::sync::RwLock;

use chrono::Utc;
use tracing::info;

use crate::types::{
    ConvergenceBlocker, ConvergenceBlockerReason, ConvergenceStatus, DigestDelta,
    SyncAggregatedStatus, TargetConvergenceState,
};

/// Tracks convergence state for all targets across all intents.
pub struct ConvergenceTracker {
    /// (intent_id, target_id) → state
    states: RwLock<HashMap<(String, String), TargetConvergenceState>>,
}

impl ConvergenceTracker {
    pub fn new() -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
        }
    }

    /// Get the current state for a target under an intent.
    pub fn get_state(&self, intent_id: &str, target_id: &str) -> Option<TargetConvergenceState> {
        let states = self.states.read().unwrap();
        states
            .get(&(intent_id.to_string(), target_id.to_string()))
            .cloned()
    }

    /// Get all states for a given intent, keyed by target_id.
    pub fn get_states_for_intent(
        &self,
        intent_id: &str,
    ) -> HashMap<String, TargetConvergenceState> {
        let states = self.states.read().unwrap();
        states
            .iter()
            .filter(|((iid, _), _)| iid == intent_id)
            .map(|((_, tid), state)| (tid.clone(), state.clone()))
            .collect()
    }

    /// Transition a target to a new status.
    pub fn update_status(
        &self,
        intent_id: &str,
        target_id: &str,
        status: ConvergenceStatus,
        remaining_delta: Option<DigestDelta>,
        checkpoint_id: Option<String>,
    ) {
        let mut states = self.states.write().unwrap();
        let key = (intent_id.to_string(), target_id.to_string());

        let state = states.entry(key).or_insert_with(|| TargetConvergenceState {
            target_id: target_id.to_string(),
            intent_id: intent_id.to_string(),
            status: ConvergenceStatus::Unknown,
            remaining_delta: None,
            active_checkpoint: None,
            blockers: vec![],
            last_updated: Utc::now(),
        });

        let old_status = state.status;
        state.status = status;
        state.remaining_delta = remaining_delta;
        state.active_checkpoint = checkpoint_id;
        state.last_updated = Utc::now();

        if old_status != status {
            info!(
                intent_id,
                target_id,
                ?old_status,
                ?status,
                "convergence state transition"
            );
        }
    }

    /// Add a blocker to a target.
    pub fn add_blocker(
        &self,
        intent_id: &str,
        target_id: &str,
        reason: ConvergenceBlockerReason,
        details: Option<String>,
    ) {
        let mut states = self.states.write().unwrap();
        let key = (intent_id.to_string(), target_id.to_string());

        if let Some(state) = states.get_mut(&key) {
            state.blockers.push(ConvergenceBlocker {
                target_id: target_id.to_string(),
                reason,
                since: Utc::now(),
                details,
            });
            state.last_updated = Utc::now();
        }
    }

    /// Clear all blockers for a target.
    pub fn clear_blockers(&self, intent_id: &str, target_id: &str) {
        let mut states = self.states.write().unwrap();
        let key = (intent_id.to_string(), target_id.to_string());
        if let Some(state) = states.get_mut(&key) {
            state.blockers.clear();
            state.last_updated = Utc::now();
        }
    }

    /// Compute aggregated status for an intent across all targets.
    pub fn aggregated_status(&self, intent_id: &str) -> SyncAggregatedStatus {
        let states = self.states.read().unwrap();
        let mut agg = SyncAggregatedStatus {
            intent_id: intent_id.to_string(),
            ..Default::default()
        };

        for ((iid, _), state) in states.iter() {
            if iid != intent_id {
                continue;
            }
            agg.total_targets += 1;
            match state.status {
                ConvergenceStatus::Converged => agg.converged += 1,
                ConvergenceStatus::InProgress | ConvergenceStatus::ContentComplete => {
                    agg.in_progress += 1
                }
                ConvergenceStatus::Pending | ConvergenceStatus::Unknown => agg.pending += 1,
                ConvergenceStatus::Failed => agg.failed += 1,
                ConvergenceStatus::Drifted => agg.drifted += 1,
            }
        }

        agg
    }
}

impl Default for ConvergenceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convergence_state_transitions() {
        let tracker = ConvergenceTracker::new();

        // Initially no state
        assert!(tracker.get_state("intent-1", "target-a").is_none());

        // Set to Pending
        tracker.update_status(
            "intent-1",
            "target-a",
            ConvergenceStatus::Pending,
            None,
            None,
        );
        let state = tracker.get_state("intent-1", "target-a").unwrap();
        assert_eq!(state.status, ConvergenceStatus::Pending);

        // Transition to InProgress
        tracker.update_status(
            "intent-1",
            "target-a",
            ConvergenceStatus::InProgress,
            None,
            Some("cp-1".into()),
        );
        let state = tracker.get_state("intent-1", "target-a").unwrap();
        assert_eq!(state.status, ConvergenceStatus::InProgress);
        assert_eq!(state.active_checkpoint.as_deref(), Some("cp-1"));

        // Transition to Converged
        tracker.update_status(
            "intent-1",
            "target-a",
            ConvergenceStatus::Converged,
            None,
            None,
        );
        let state = tracker.get_state("intent-1", "target-a").unwrap();
        assert_eq!(state.status, ConvergenceStatus::Converged);
    }

    #[test]
    fn test_convergence_blockers() {
        let tracker = ConvergenceTracker::new();
        tracker.update_status("i", "t", ConvergenceStatus::InProgress, None, None);

        tracker.add_blocker(
            "i",
            "t",
            ConvergenceBlockerReason::NetworkUnavailable,
            Some("timeout".into()),
        );
        let state = tracker.get_state("i", "t").unwrap();
        assert_eq!(state.blockers.len(), 1);
        assert_eq!(
            state.blockers[0].reason,
            ConvergenceBlockerReason::NetworkUnavailable
        );

        tracker.clear_blockers("i", "t");
        let state = tracker.get_state("i", "t").unwrap();
        assert!(state.blockers.is_empty());
    }

    #[test]
    fn test_aggregated_status() {
        let tracker = ConvergenceTracker::new();
        tracker.update_status("i", "a", ConvergenceStatus::Converged, None, None);
        tracker.update_status("i", "b", ConvergenceStatus::Converged, None, None);
        tracker.update_status("i", "c", ConvergenceStatus::InProgress, None, None);
        tracker.update_status("i", "d", ConvergenceStatus::Pending, None, None);
        tracker.update_status("i", "e", ConvergenceStatus::Failed, None, None);

        let agg = tracker.aggregated_status("i");
        assert_eq!(agg.total_targets, 5);
        assert_eq!(agg.converged, 2);
        assert_eq!(agg.in_progress, 1);
        assert_eq!(agg.pending, 1);
        assert_eq!(agg.failed, 1);
    }

    #[test]
    fn test_get_states_for_intent() {
        let tracker = ConvergenceTracker::new();
        tracker.update_status("i1", "a", ConvergenceStatus::Pending, None, None);
        tracker.update_status("i1", "b", ConvergenceStatus::Converged, None, None);
        tracker.update_status("i2", "c", ConvergenceStatus::Pending, None, None);

        let states = tracker.get_states_for_intent("i1");
        assert_eq!(states.len(), 2);
        assert!(states.contains_key("a"));
        assert!(states.contains_key("b"));
    }
}
