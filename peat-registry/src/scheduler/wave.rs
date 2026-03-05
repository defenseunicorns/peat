use std::collections::HashMap;

use crate::types::{ConvergenceStatus, TargetConvergenceState};

/// Controls wave progression — wave N+1 only starts when a threshold fraction of wave N converges.
pub struct WaveController {
    gate_threshold: f64,
}

impl WaveController {
    pub fn new(gate_threshold: f64) -> Self {
        Self {
            gate_threshold: gate_threshold.clamp(0.0, 1.0),
        }
    }

    /// Check if a wave is active (eligible for sync).
    ///
    /// Wave 0 is always active. Wave N is active when the gate_threshold fraction
    /// of wave N-1 targets are converged.
    pub fn is_wave_active(
        &self,
        wave: u32,
        wave_assignments: &HashMap<String, u32>,
        convergence_states: &HashMap<String, TargetConvergenceState>,
    ) -> bool {
        if wave == 0 {
            return true;
        }

        let prev_wave = wave - 1;
        let prev_targets: Vec<&str> = wave_assignments
            .iter()
            .filter(|(_, w)| **w == prev_wave)
            .map(|(id, _)| id.as_str())
            .collect();

        if prev_targets.is_empty() {
            return true;
        }

        let converged_count = prev_targets
            .iter()
            .filter(|id| {
                convergence_states
                    .get(**id)
                    .map(|s| s.status == ConvergenceStatus::Converged)
                    .unwrap_or(false)
            })
            .count();

        let fraction = converged_count as f64 / prev_targets.len() as f64;
        fraction >= self.gate_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

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
    fn test_wave_0_always_active() {
        let wc = WaveController::new(0.8);
        let waves = HashMap::new();
        let states = HashMap::new();
        assert!(wc.is_wave_active(0, &waves, &states));
    }

    #[test]
    fn test_wave_1_active_when_wave_0_converged() {
        let wc = WaveController::new(0.8);
        let mut waves = HashMap::new();
        waves.insert("root".to_string(), 0);
        waves.insert("child".to_string(), 1);

        let mut states = HashMap::new();
        states.insert(
            "root".to_string(),
            make_state("root", ConvergenceStatus::Converged),
        );

        assert!(wc.is_wave_active(1, &waves, &states));
    }

    #[test]
    fn test_wave_1_not_active_when_wave_0_pending() {
        let wc = WaveController::new(0.8);
        let mut waves = HashMap::new();
        waves.insert("root".to_string(), 0);
        waves.insert("child".to_string(), 1);

        let mut states = HashMap::new();
        states.insert(
            "root".to_string(),
            make_state("root", ConvergenceStatus::Pending),
        );

        assert!(!wc.is_wave_active(1, &waves, &states));
    }

    #[test]
    fn test_wave_threshold_partial() {
        let wc = WaveController::new(0.5); // 50% threshold
        let mut waves = HashMap::new();
        waves.insert("a".to_string(), 0);
        waves.insert("b".to_string(), 0);
        waves.insert("c".to_string(), 0);
        waves.insert("d".to_string(), 0);
        waves.insert("child".to_string(), 1);

        let mut states = HashMap::new();
        states.insert(
            "a".to_string(),
            make_state("a", ConvergenceStatus::Converged),
        );
        states.insert(
            "b".to_string(),
            make_state("b", ConvergenceStatus::Converged),
        );
        states.insert(
            "c".to_string(),
            make_state("c", ConvergenceStatus::InProgress),
        );
        states.insert("d".to_string(), make_state("d", ConvergenceStatus::Pending));

        // 2/4 = 50%, meets threshold
        assert!(wc.is_wave_active(1, &waves, &states));
    }

    #[test]
    fn test_wave_threshold_not_met() {
        let wc = WaveController::new(0.8); // 80% threshold
        let mut waves = HashMap::new();
        waves.insert("a".to_string(), 0);
        waves.insert("b".to_string(), 0);
        waves.insert("c".to_string(), 0);
        waves.insert("child".to_string(), 1);

        let mut states = HashMap::new();
        states.insert(
            "a".to_string(),
            make_state("a", ConvergenceStatus::Converged),
        );
        states.insert(
            "b".to_string(),
            make_state("b", ConvergenceStatus::Converged),
        );
        states.insert(
            "c".to_string(),
            make_state("c", ConvergenceStatus::InProgress),
        );

        // 2/3 = 66%, below 80% threshold
        assert!(!wc.is_wave_active(1, &waves, &states));
    }
}
