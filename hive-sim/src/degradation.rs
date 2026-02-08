//! Time-based equipment degradation model for port-ops simulation.
//!
//! Models stochastic equipment degradation for gantry cranes. Tracks three
//! subsystems — hydraulic, spreader alignment, and electrical — each with
//! independent decay curves driven by lift cycles and utilization.
//!
//! # Degradation rules
//!
//! | Subsystem   | Trigger              | Decay per event            |
//! |-------------|----------------------|----------------------------|
//! | Hydraulic   | Each lift cycle      | 0.2–0.5 % (normal dist)   |
//! | Spreader    | Every N moves        | Alignment error accumulates|
//! | Electrical  | Sustained high util  | Proportional to load       |
//!
//! # Capability thresholds
//!
//! - `hydraulic_pct >= 70 %` → full `moves_per_hour`
//! - `hydraulic_pct  < 70 %` → downgraded `moves_per_hour`
//! - `hydraulic_pct  < 40 %` → status **FAILED**
//!
//! # Confidence mapping
//!
//! The proto `Capability.confidence` field (0.0–1.0) is derived from the
//! composite health percentage: `confidence = health_pct / 100.0`.

use crate::metrics::{log_metrics, MetricsEvent};
use rand::distributions::{Distribution, Uniform};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::Normal;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Mean hydraulic decay per lift cycle (percent).
const HYDRAULIC_DECAY_MEAN: f64 = 0.35;
/// Std-dev of hydraulic decay per lift cycle (percent).
const HYDRAULIC_DECAY_STDDEV: f64 = 0.075;

/// Every this many moves the spreader alignment drifts.
const SPREADER_DRIFT_INTERVAL: u64 = 5;
/// Base alignment drift per interval (degrees).
const SPREADER_DRIFT_BASE: f64 = 0.15;
/// Random jitter added to each drift event (degrees, uniform).
const SPREADER_DRIFT_JITTER: f64 = 0.10;
/// Maximum tolerable alignment error before capability is impacted (degrees).
const SPREADER_MAX_ALIGNMENT_ERROR: f64 = 3.0;

/// Electrical degradation factor per cycle at 100 % utilization.
const ELECTRICAL_DECAY_FULL_LOAD: f64 = 0.10;
/// Utilization threshold below which no electrical degradation occurs.
const ELECTRICAL_UTIL_THRESHOLD: f64 = 0.70;

/// Hydraulic threshold for capability downgrade (percent).
const HYDRAULIC_DOWNGRADE_THRESHOLD: f64 = 70.0;
/// Hydraulic threshold for FAILED status (percent).
const HYDRAULIC_FAILED_THRESHOLD: f64 = 40.0;

/// Nominal moves-per-hour at full health.
const NOMINAL_MOVES_PER_HOUR: f64 = 30.0;
/// Minimum moves-per-hour when degraded (before FAILED).
const MIN_DEGRADED_MOVES_PER_HOUR: f64 = 12.0;

// ---------------------------------------------------------------------------
// Health snapshot
// ---------------------------------------------------------------------------

/// Current health state for a single gantry crane.
#[derive(Debug, Clone)]
pub struct GantryCraneHealth {
    /// Hydraulic system pressure (0.0–100.0 %).
    pub hydraulic_pct: f64,
    /// Spreader alignment error in degrees (0.0 = perfect).
    pub spreader_alignment_error: f64,
    /// Electrical system health (0.0–100.0 %).
    pub electrical_pct: f64,
    /// Total lift cycles executed.
    pub total_moves: u64,
    /// Current utilization level (0.0–1.0) for electrical decay.
    pub utilization: f64,
}

impl Default for GantryCraneHealth {
    fn default() -> Self {
        Self {
            hydraulic_pct: 100.0,
            spreader_alignment_error: 0.0,
            electrical_pct: 100.0,
            total_moves: 0,
            utilization: 0.0,
        }
    }
}

impl GantryCraneHealth {
    /// Composite health percentage (0.0–100.0) weighting all subsystems.
    ///
    /// Weights: hydraulic 50 %, electrical 30 %, spreader 20 %.
    pub fn composite_health_pct(&self) -> f64 {
        let spreader_health =
            (1.0 - (self.spreader_alignment_error / SPREADER_MAX_ALIGNMENT_ERROR).min(1.0)) * 100.0;
        (self.hydraulic_pct * 0.50 + self.electrical_pct * 0.30 + spreader_health * 0.20)
            .clamp(0.0, 100.0)
    }

    /// Map composite health to the proto `Capability.confidence` (0.0–1.0).
    pub fn confidence(&self) -> f32 {
        (self.composite_health_pct() / 100.0) as f32
    }

    /// Effective moves-per-hour given current degradation state.
    pub fn effective_moves_per_hour(&self) -> f64 {
        if self.hydraulic_pct < HYDRAULIC_FAILED_THRESHOLD {
            return 0.0;
        }
        if self.hydraulic_pct < HYDRAULIC_DOWNGRADE_THRESHOLD {
            // Linear interpolation between min and nominal across 40..70 range
            let ratio = (self.hydraulic_pct - HYDRAULIC_FAILED_THRESHOLD)
                / (HYDRAULIC_DOWNGRADE_THRESHOLD - HYDRAULIC_FAILED_THRESHOLD);
            return MIN_DEGRADED_MOVES_PER_HOUR
                + ratio * (NOMINAL_MOVES_PER_HOUR - MIN_DEGRADED_MOVES_PER_HOUR);
        }
        NOMINAL_MOVES_PER_HOUR
    }

    /// Operational status string matching proto HealthStatus semantics.
    pub fn health_status(&self) -> EquipmentStatus {
        if self.hydraulic_pct < HYDRAULIC_FAILED_THRESHOLD {
            EquipmentStatus::Failed
        } else if self.hydraulic_pct < HYDRAULIC_DOWNGRADE_THRESHOLD {
            EquipmentStatus::Degraded
        } else {
            EquipmentStatus::Nominal
        }
    }
}

/// Equipment status mirroring proto `HealthStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquipmentStatus {
    Nominal,
    Degraded,
    Failed,
}

impl std::fmt::Display for EquipmentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EquipmentStatus::Nominal => write!(f, "NOMINAL"),
            EquipmentStatus::Degraded => write!(f, "DEGRADED"),
            EquipmentStatus::Failed => write!(f, "FAILED"),
        }
    }
}

// ---------------------------------------------------------------------------
// Degradation event
// ---------------------------------------------------------------------------

/// Describes a single degradation step for one subsystem.
#[derive(Debug, Clone)]
pub struct DegradationStep {
    /// Subsystem that degraded (e.g. "hydraulic", "spreader", "electrical").
    pub subsystem: String,
    /// Value before this degradation step.
    pub before: f64,
    /// Value after this degradation step.
    pub after: f64,
    /// Human-readable cause.
    pub cause: String,
    /// Decay rate applied (unit depends on subsystem).
    pub decay_rate: f64,
}

// ---------------------------------------------------------------------------
// Degradation engine
// ---------------------------------------------------------------------------

/// Stateful engine that applies stochastic degradation to a gantry crane.
pub struct DegradationEngine {
    rng: StdRng,
    hydraulic_dist: Normal<f64>,
    drift_jitter_dist: Uniform<f64>,
    /// Identifier for the crane (used in metrics).
    pub crane_id: String,
    /// Node identifier (used in metrics).
    pub node_id: String,
}

impl DegradationEngine {
    /// Create a new engine with a deterministic seed for reproducibility.
    pub fn new(crane_id: String, node_id: String, seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            hydraulic_dist: Normal::new(HYDRAULIC_DECAY_MEAN, HYDRAULIC_DECAY_STDDEV)
                .expect("valid normal distribution parameters"),
            drift_jitter_dist: Uniform::new(0.0, SPREADER_DRIFT_JITTER),
            crane_id,
            node_id,
        }
    }

    /// Execute one lift cycle and return any degradation events that occurred.
    ///
    /// This is the main tick function. Call it once per simulated lift move.
    pub fn tick(&mut self, health: &mut GantryCraneHealth) -> Vec<DegradationStep> {
        let mut steps = Vec::new();

        health.total_moves += 1;

        // --- Hydraulic degradation (every cycle) ---
        let decay = self.hydraulic_dist.sample(&mut self.rng).abs();
        let before = health.hydraulic_pct;
        health.hydraulic_pct = (health.hydraulic_pct - decay).max(0.0);
        steps.push(DegradationStep {
            subsystem: "hydraulic".into(),
            before,
            after: health.hydraulic_pct,
            cause: format!("lift_cycle_{}", health.total_moves),
            decay_rate: decay,
        });

        // --- Spreader alignment drift (every N moves) ---
        if health.total_moves % SPREADER_DRIFT_INTERVAL == 0 {
            let jitter = self.drift_jitter_dist.sample(&mut self.rng);
            let drift = SPREADER_DRIFT_BASE + jitter;
            let before = health.spreader_alignment_error;
            health.spreader_alignment_error =
                (health.spreader_alignment_error + drift).min(SPREADER_MAX_ALIGNMENT_ERROR);
            steps.push(DegradationStep {
                subsystem: "spreader".into(),
                before,
                after: health.spreader_alignment_error,
                cause: format!("alignment_drift_after_{}_moves", health.total_moves),
                decay_rate: drift,
            });
        }

        // --- Electrical degradation (when utilization is high) ---
        if health.utilization > ELECTRICAL_UTIL_THRESHOLD {
            let excess = health.utilization - ELECTRICAL_UTIL_THRESHOLD;
            let decay = ELECTRICAL_DECAY_FULL_LOAD * (excess / (1.0 - ELECTRICAL_UTIL_THRESHOLD));
            let before = health.electrical_pct;
            health.electrical_pct = (health.electrical_pct - decay).max(0.0);
            steps.push(DegradationStep {
                subsystem: "electrical".into(),
                before,
                after: health.electrical_pct,
                cause: format!("sustained_utilization_{:.0}pct", health.utilization * 100.0),
                decay_rate: decay,
            });
        }

        // --- Emit metrics for each step ---
        for step in &steps {
            log_metrics(&MetricsEvent::CapabilityDegraded {
                node_id: self.node_id.clone(),
                crane_id: self.crane_id.clone(),
                subsystem: step.subsystem.clone(),
                before: step.before,
                after: step.after,
                cause: step.cause.clone(),
                decay_rate: step.decay_rate,
                confidence: health.confidence(),
                effective_moves_per_hour: health.effective_moves_per_hour(),
                status: health.health_status().to_string(),
                total_moves: health.total_moves,
                timestamp_us: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_micros(),
            });
        }

        steps
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> DegradationEngine {
        DegradationEngine::new("crane-1".into(), "node-1".into(), 42)
    }

    #[test]
    fn default_health_is_full() {
        let h = GantryCraneHealth::default();
        assert_eq!(h.hydraulic_pct, 100.0);
        assert_eq!(h.spreader_alignment_error, 0.0);
        assert_eq!(h.electrical_pct, 100.0);
        assert_eq!(h.total_moves, 0);
        assert!((h.confidence() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn composite_health_at_full() {
        let h = GantryCraneHealth::default();
        assert!((h.composite_health_pct() - 100.0).abs() < 0.01);
    }

    #[test]
    fn hydraulic_decays_each_cycle() {
        let mut engine = make_engine();
        let mut health = GantryCraneHealth::default();

        engine.tick(&mut health);
        assert!(health.hydraulic_pct < 100.0);
        assert!(health.hydraulic_pct > 99.0); // should be ~99.5-99.8
    }

    #[test]
    fn hydraulic_decay_within_expected_range_over_many_cycles() {
        let mut engine = make_engine();
        let mut health = GantryCraneHealth::default();

        for _ in 0..100 {
            engine.tick(&mut health);
        }

        // After 100 cycles at ~0.35% mean, expect ~65 pct remaining
        // Allow generous range for stochastic variance
        assert!(health.hydraulic_pct > 50.0, "got {}", health.hydraulic_pct);
        assert!(health.hydraulic_pct < 80.0, "got {}", health.hydraulic_pct);
    }

    #[test]
    fn spreader_drifts_at_interval() {
        let mut engine = make_engine();
        let mut health = GantryCraneHealth::default();

        // First 4 moves: no drift
        for _ in 0..4 {
            let steps = engine.tick(&mut health);
            assert!(
                !steps.iter().any(|s| s.subsystem == "spreader"),
                "no drift before interval"
            );
        }

        // 5th move: drift happens
        let steps = engine.tick(&mut health);
        assert!(
            steps.iter().any(|s| s.subsystem == "spreader"),
            "drift should occur at move 5"
        );
        assert!(health.spreader_alignment_error > 0.0);
    }

    #[test]
    fn spreader_alignment_error_capped() {
        let mut engine = make_engine();
        let mut health = GantryCraneHealth::default();

        for _ in 0..500 {
            engine.tick(&mut health);
        }

        assert!(health.spreader_alignment_error <= SPREADER_MAX_ALIGNMENT_ERROR);
    }

    #[test]
    fn electrical_degrades_only_at_high_utilization() {
        let mut engine = make_engine();
        let mut health = GantryCraneHealth {
            utilization: 0.50, // below threshold
            ..Default::default()
        };

        engine.tick(&mut health);
        assert_eq!(health.electrical_pct, 100.0);

        health.utilization = 0.90; // above threshold
        engine.tick(&mut health);
        assert!(health.electrical_pct < 100.0);
    }

    #[test]
    fn electrical_decay_proportional_to_excess_utilization() {
        let mut engine_lo = DegradationEngine::new("c1".into(), "n1".into(), 42);
        let mut engine_hi = DegradationEngine::new("c2".into(), "n2".into(), 42);

        let mut health_lo = GantryCraneHealth {
            utilization: 0.80,
            ..Default::default()
        };

        let mut health_hi = GantryCraneHealth {
            utilization: 1.0,
            ..Default::default()
        };

        engine_lo.tick(&mut health_lo);
        engine_hi.tick(&mut health_hi);

        // Higher utilization should cause more decay
        assert!(health_hi.electrical_pct < health_lo.electrical_pct);
    }

    #[test]
    fn moves_per_hour_nominal_above_70() {
        let health = GantryCraneHealth {
            hydraulic_pct: 75.0,
            ..Default::default()
        };
        assert_eq!(health.effective_moves_per_hour(), NOMINAL_MOVES_PER_HOUR);
    }

    #[test]
    fn moves_per_hour_degraded_between_40_and_70() {
        let health = GantryCraneHealth {
            hydraulic_pct: 55.0, // midpoint of 40-70 range
            ..Default::default()
        };
        let mph = health.effective_moves_per_hour();
        assert!(mph > MIN_DEGRADED_MOVES_PER_HOUR);
        assert!(mph < NOMINAL_MOVES_PER_HOUR);
    }

    #[test]
    fn moves_per_hour_zero_below_40() {
        let health = GantryCraneHealth {
            hydraulic_pct: 39.0,
            ..Default::default()
        };
        assert_eq!(health.effective_moves_per_hour(), 0.0);
    }

    #[test]
    fn status_transitions() {
        let mut health = GantryCraneHealth {
            hydraulic_pct: 80.0,
            ..Default::default()
        };
        assert_eq!(health.health_status(), EquipmentStatus::Nominal);

        health.hydraulic_pct = 60.0;
        assert_eq!(health.health_status(), EquipmentStatus::Degraded);

        health.hydraulic_pct = 30.0;
        assert_eq!(health.health_status(), EquipmentStatus::Failed);
    }

    #[test]
    fn confidence_maps_linearly_from_composite() {
        let health = GantryCraneHealth {
            hydraulic_pct: 50.0,
            electrical_pct: 50.0,
            spreader_alignment_error: 1.5, // 50% of max
            ..Default::default()
        };

        // composite = 50*0.5 + 50*0.3 + 50*0.2 = 50.0
        let conf = health.confidence();
        assert!((conf - 0.50).abs() < 0.01, "got {}", conf);
    }

    #[test]
    fn tick_returns_steps_with_correct_fields() {
        let mut engine = make_engine();
        let mut health = GantryCraneHealth {
            utilization: 0.95,
            ..Default::default()
        };

        let steps = engine.tick(&mut health);

        // Should have at least hydraulic + electrical (move 1, not spreader interval)
        assert!(steps.len() >= 2, "got {} steps", steps.len());

        let hydraulic_step = steps.iter().find(|s| s.subsystem == "hydraulic").unwrap();
        assert!(hydraulic_step.before > hydraulic_step.after);
        assert!(hydraulic_step.decay_rate > 0.0);
        assert!(hydraulic_step.cause.starts_with("lift_cycle_"));
    }

    #[test]
    fn full_lifecycle_to_failure() {
        let mut engine = make_engine();
        let mut health = GantryCraneHealth {
            utilization: 0.85,
            ..Default::default()
        };

        let mut reached_degraded = false;
        let mut reached_failed = false;

        for _ in 0..500 {
            engine.tick(&mut health);

            match health.health_status() {
                EquipmentStatus::Degraded => reached_degraded = true,
                EquipmentStatus::Failed => {
                    reached_failed = true;
                    break;
                }
                _ => {}
            }
        }

        assert!(reached_degraded, "should pass through DEGRADED");
        assert!(reached_failed, "should reach FAILED within 500 cycles");
        assert_eq!(health.effective_moves_per_hour(), 0.0);
    }

    #[test]
    fn deterministic_with_same_seed() {
        let mut engine1 = DegradationEngine::new("c".into(), "n".into(), 99);
        let mut engine2 = DegradationEngine::new("c".into(), "n".into(), 99);

        let mut h1 = GantryCraneHealth::default();
        let mut h2 = GantryCraneHealth::default();

        for _ in 0..50 {
            engine1.tick(&mut h1);
            engine2.tick(&mut h2);
        }

        assert_eq!(h1.hydraulic_pct, h2.hydraulic_pct);
        assert_eq!(h1.spreader_alignment_error, h2.spreader_alignment_error);
        assert_eq!(h1.electrical_pct, h2.electrical_pct);
    }
}
