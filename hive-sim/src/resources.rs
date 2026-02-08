//! Resource consumption and resupply cycle modeling for port operations
//!
//! Models the logistical support requirements for equipment:
//! - Tractor: battery_pct decreases per transport cycle (~3-5% per round trip)
//! - Crane: hydraulic fluid consumption requires periodic maintenance
//! - Worker: fatigue accumulates after continuous operation
//!
//! Equipment transitions to maintenance/resupply states when thresholds are hit,
//! and depends on maintenance crew availability to recover.

use crate::metrics::{log_metrics, MetricsEvent};
use crate::utils::time::now_micros;

/// Operational state for any equipment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquipmentState {
    /// Normal operations
    Operational,
    /// Routed to charging station / maintenance bay
    Charging,
    /// Awaiting maintenance crew (blocked if crew unavailable)
    AwaitingMaintenance,
    /// Maintenance in progress
    UnderMaintenance,
    /// Worker on break due to fatigue
    OnBreak,
    /// Equipment degraded but still functional at reduced efficiency
    Degraded,
}

impl std::fmt::Display for EquipmentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EquipmentState::Operational => write!(f, "OPERATIONAL"),
            EquipmentState::Charging => write!(f, "CHARGING"),
            EquipmentState::AwaitingMaintenance => write!(f, "AWAITING_MAINTENANCE"),
            EquipmentState::UnderMaintenance => write!(f, "UNDER_MAINTENANCE"),
            EquipmentState::OnBreak => write!(f, "ON_BREAK"),
            EquipmentState::Degraded => write!(f, "DEGRADED"),
        }
    }
}

/// Tractor state: battery drains per transport cycle, charges at station
#[derive(Debug, Clone)]
pub struct TractorState {
    pub id: String,
    pub battery_pct: f64,
    pub state: EquipmentState,
    /// Sim-seconds remaining in current charging cycle
    pub charge_remaining_secs: u64,
    /// Total transport cycles completed
    pub cycles_completed: u64,
}

/// Crane state: hydraulic fluid depletes over operations, needs maintenance
#[derive(Debug, Clone)]
pub struct CraneState {
    pub id: String,
    pub hydraulic_fluid_pct: f64,
    pub state: EquipmentState,
    /// Sim-seconds remaining in current maintenance
    pub maintenance_remaining_secs: u64,
    /// Total lift operations completed
    pub lifts_completed: u64,
}

/// Worker state: fatigue accumulates during continuous operation
#[derive(Debug, Clone)]
pub struct WorkerState {
    pub id: String,
    /// 0.0 = fresh, 100.0 = exhausted
    pub fatigue_pct: f64,
    pub state: EquipmentState,
    /// Sim-seconds of continuous operation
    pub continuous_operation_secs: u64,
    /// Sim-seconds remaining on break
    pub break_remaining_secs: u64,
    /// Current efficiency multiplier (1.0 = normal, 0.9 = fatigued)
    pub efficiency: f64,
}

/// Whether a maintenance crew is available for equipment servicing
#[derive(Debug, Clone)]
pub struct MaintenanceCrew {
    pub available: bool,
    /// Sim-seconds until crew becomes available (0 if available)
    pub busy_remaining_secs: u64,
}

// -- Thresholds and constants --

/// Battery percentage at which tractor must route to charging
pub const TRACTOR_CHARGE_THRESHOLD: f64 = 20.0;
/// Battery consumption per transport round trip (3-5% range)
pub const TRACTOR_DRAIN_MIN: f64 = 3.0;
pub const TRACTOR_DRAIN_MAX: f64 = 5.0;
/// Charging time in sim-seconds (15 min = 900 sec)
pub const TRACTOR_CHARGE_DURATION_SECS: u64 = 900;
/// Battery level after full charge
pub const TRACTOR_CHARGE_TARGET: f64 = 100.0;

/// Hydraulic fluid percentage triggering maintenance
pub const CRANE_MAINTENANCE_THRESHOLD: f64 = 25.0;
/// Hydraulic fluid consumed per lift operation
pub const CRANE_FLUID_PER_LIFT: f64 = 1.5;
/// Maintenance window duration in sim-seconds (20 min = 1200 sec)
pub const CRANE_MAINTENANCE_DURATION_SECS: u64 = 1200;
/// Fluid level after maintenance
pub const CRANE_FLUID_TARGET: f64 = 100.0;

/// Worker continuous operation threshold in sim-seconds (4 hours = 14400 sec)
pub const WORKER_FATIGUE_THRESHOLD_SECS: u64 = 14400;
/// Efficiency drop when fatigued
pub const WORKER_FATIGUED_EFFICIENCY: f64 = 0.90;
/// Break duration in sim-seconds (15 min = 900 sec)
pub const WORKER_BREAK_DURATION_SECS: u64 = 900;

impl TractorState {
    pub fn new(id: String) -> Self {
        Self {
            id,
            battery_pct: 100.0,
            state: EquipmentState::Operational,
            charge_remaining_secs: 0,
            cycles_completed: 0,
        }
    }

    /// Simulate one transport cycle. Returns true if tractor completed the cycle.
    pub fn complete_transport_cycle(&mut self, node_id: &str, drain_pct: f64) -> bool {
        if self.state != EquipmentState::Operational {
            return false;
        }

        let previous = self.battery_pct;
        let drain = drain_pct.clamp(TRACTOR_DRAIN_MIN, TRACTOR_DRAIN_MAX);
        self.battery_pct = (self.battery_pct - drain).max(0.0);
        self.cycles_completed += 1;

        log_metrics(&MetricsEvent::ResourceConsumed {
            node_id: node_id.to_string(),
            equipment_id: self.id.clone(),
            equipment_type: "tractor".to_string(),
            resource_type: "battery_pct".to_string(),
            previous_level: previous,
            current_level: self.battery_pct,
            consumed: drain,
            trigger: Some("transport_cycle".to_string()),
            timestamp_us: now_micros(),
        });

        // Check if we need to route to charging
        if self.battery_pct <= TRACTOR_CHARGE_THRESHOLD {
            self.transition_to(EquipmentState::Charging, "battery_low", node_id);
            self.charge_remaining_secs = TRACTOR_CHARGE_DURATION_SECS;

            log_metrics(&MetricsEvent::ResupplyRequested {
                node_id: node_id.to_string(),
                equipment_id: self.id.clone(),
                equipment_type: "tractor".to_string(),
                resource_type: "battery_pct".to_string(),
                current_level: self.battery_pct,
                threshold: TRACTOR_CHARGE_THRESHOLD,
                requires_maintenance_crew: Some(false),
                timestamp_us: now_micros(),
            });
        }

        true
    }

    /// Advance charging by elapsed sim-seconds. Returns true when charging completes.
    pub fn tick_charging(&mut self, elapsed_secs: u64, node_id: &str) -> bool {
        if self.state != EquipmentState::Charging {
            return false;
        }

        self.charge_remaining_secs = self.charge_remaining_secs.saturating_sub(elapsed_secs);

        if self.charge_remaining_secs == 0 {
            let previous = self.battery_pct;
            self.battery_pct = TRACTOR_CHARGE_TARGET;
            self.transition_to(EquipmentState::Operational, "charging_complete", node_id);

            log_metrics(&MetricsEvent::ResupplyCompleted {
                node_id: node_id.to_string(),
                equipment_id: self.id.clone(),
                equipment_type: "tractor".to_string(),
                resource_type: "battery_pct".to_string(),
                previous_level: previous,
                restored_level: TRACTOR_CHARGE_TARGET,
                duration_sim_secs: TRACTOR_CHARGE_DURATION_SECS,
                timestamp_us: now_micros(),
            });

            return true;
        }

        false
    }

    fn transition_to(&mut self, new_state: EquipmentState, reason: &str, node_id: &str) {
        let previous = self.state;
        self.state = new_state;

        log_metrics(&MetricsEvent::EquipmentStateChanged {
            node_id: node_id.to_string(),
            equipment_id: self.id.clone(),
            equipment_type: "tractor".to_string(),
            previous_state: previous.to_string(),
            new_state: self.state.to_string(),
            reason: reason.to_string(),
            timestamp_us: now_micros(),
        });
    }
}

impl CraneState {
    pub fn new(id: String) -> Self {
        Self {
            id,
            hydraulic_fluid_pct: 100.0,
            state: EquipmentState::Operational,
            maintenance_remaining_secs: 0,
            lifts_completed: 0,
        }
    }

    /// Simulate one lift operation. Returns true if crane completed the lift.
    pub fn complete_lift(&mut self, node_id: &str) -> bool {
        if self.state != EquipmentState::Operational && self.state != EquipmentState::Degraded {
            return false;
        }

        let previous = self.hydraulic_fluid_pct;
        self.hydraulic_fluid_pct = (self.hydraulic_fluid_pct - CRANE_FLUID_PER_LIFT).max(0.0);
        self.lifts_completed += 1;

        log_metrics(&MetricsEvent::ResourceConsumed {
            node_id: node_id.to_string(),
            equipment_id: self.id.clone(),
            equipment_type: "crane".to_string(),
            resource_type: "hydraulic_fluid_pct".to_string(),
            previous_level: previous,
            current_level: self.hydraulic_fluid_pct,
            consumed: CRANE_FLUID_PER_LIFT,
            trigger: Some("lift_operation".to_string()),
            timestamp_us: now_micros(),
        });

        // Check if maintenance needed
        if self.hydraulic_fluid_pct <= CRANE_MAINTENANCE_THRESHOLD {
            self.transition_to(
                EquipmentState::AwaitingMaintenance,
                "hydraulic_fluid_low",
                node_id,
            );

            log_metrics(&MetricsEvent::ResupplyRequested {
                node_id: node_id.to_string(),
                equipment_id: self.id.clone(),
                equipment_type: "crane".to_string(),
                resource_type: "hydraulic_fluid_pct".to_string(),
                current_level: self.hydraulic_fluid_pct,
                threshold: CRANE_MAINTENANCE_THRESHOLD,
                requires_maintenance_crew: Some(true),
                timestamp_us: now_micros(),
            });
        }

        true
    }

    /// Begin maintenance if crew is available. Returns true if maintenance started.
    pub fn begin_maintenance(&mut self, crew: &mut MaintenanceCrew, node_id: &str) -> bool {
        if self.state != EquipmentState::AwaitingMaintenance {
            return false;
        }

        if !crew.available {
            // Logistical dependency: without maintenance crew, equipment stays degraded
            return false;
        }

        crew.available = false;
        crew.busy_remaining_secs = CRANE_MAINTENANCE_DURATION_SECS;
        self.maintenance_remaining_secs = CRANE_MAINTENANCE_DURATION_SECS;
        self.transition_to(
            EquipmentState::UnderMaintenance,
            "maintenance_crew_arrived",
            node_id,
        );

        true
    }

    /// Advance maintenance by elapsed sim-seconds. Returns true when complete.
    pub fn tick_maintenance(
        &mut self,
        elapsed_secs: u64,
        crew: &mut MaintenanceCrew,
        node_id: &str,
    ) -> bool {
        if self.state != EquipmentState::UnderMaintenance {
            return false;
        }

        self.maintenance_remaining_secs = self.maintenance_remaining_secs.saturating_sub(elapsed_secs);
        crew.busy_remaining_secs = crew.busy_remaining_secs.saturating_sub(elapsed_secs);

        if self.maintenance_remaining_secs == 0 {
            let previous = self.hydraulic_fluid_pct;
            self.hydraulic_fluid_pct = CRANE_FLUID_TARGET;
            crew.available = true;
            crew.busy_remaining_secs = 0;
            self.transition_to(
                EquipmentState::Operational,
                "maintenance_complete",
                node_id,
            );

            log_metrics(&MetricsEvent::ResupplyCompleted {
                node_id: node_id.to_string(),
                equipment_id: self.id.clone(),
                equipment_type: "crane".to_string(),
                resource_type: "hydraulic_fluid_pct".to_string(),
                previous_level: previous,
                restored_level: CRANE_FLUID_TARGET,
                duration_sim_secs: CRANE_MAINTENANCE_DURATION_SECS,
                timestamp_us: now_micros(),
            });

            return true;
        }

        false
    }

    fn transition_to(&mut self, new_state: EquipmentState, reason: &str, node_id: &str) {
        let previous = self.state;
        self.state = new_state;

        log_metrics(&MetricsEvent::EquipmentStateChanged {
            node_id: node_id.to_string(),
            equipment_id: self.id.clone(),
            equipment_type: "crane".to_string(),
            previous_state: previous.to_string(),
            new_state: self.state.to_string(),
            reason: reason.to_string(),
            timestamp_us: now_micros(),
        });
    }
}

impl WorkerState {
    pub fn new(id: String) -> Self {
        Self {
            id,
            fatigue_pct: 0.0,
            state: EquipmentState::Operational,
            continuous_operation_secs: 0,
            break_remaining_secs: 0,
            efficiency: 1.0,
        }
    }

    /// Record continuous work for elapsed sim-seconds.
    /// Returns current efficiency multiplier.
    pub fn work(&mut self, elapsed_secs: u64, node_id: &str) -> f64 {
        if self.state != EquipmentState::Operational && self.state != EquipmentState::Degraded {
            return 0.0;
        }

        self.continuous_operation_secs += elapsed_secs;
        let previous_fatigue = self.fatigue_pct;

        // Fatigue increases proportionally to time worked vs threshold
        self.fatigue_pct =
            (self.continuous_operation_secs as f64 / WORKER_FATIGUE_THRESHOLD_SECS as f64 * 100.0)
                .min(100.0);

        if self.fatigue_pct != previous_fatigue {
            log_metrics(&MetricsEvent::ResourceConsumed {
                node_id: node_id.to_string(),
                equipment_id: self.id.clone(),
                equipment_type: "worker".to_string(),
                resource_type: "fatigue_pct".to_string(),
                previous_level: previous_fatigue,
                current_level: self.fatigue_pct,
                consumed: self.fatigue_pct - previous_fatigue,
                trigger: Some("continuous_operation".to_string()),
                timestamp_us: now_micros(),
            });
        }

        // After 4 hours continuous, efficiency drops 10%
        if self.continuous_operation_secs >= WORKER_FATIGUE_THRESHOLD_SECS
            && self.state == EquipmentState::Operational
        {
            self.efficiency = WORKER_FATIGUED_EFFICIENCY;
            self.transition_to(EquipmentState::Degraded, "fatigue", node_id);

            log_metrics(&MetricsEvent::EfficiencyDegraded {
                node_id: node_id.to_string(),
                equipment_id: self.id.clone(),
                equipment_type: "worker".to_string(),
                efficiency_pct: self.efficiency * 100.0,
                cause: "fatigue".to_string(),
                timestamp_us: now_micros(),
            });

            log_metrics(&MetricsEvent::ResupplyRequested {
                node_id: node_id.to_string(),
                equipment_id: self.id.clone(),
                equipment_type: "worker".to_string(),
                resource_type: "fatigue_pct".to_string(),
                current_level: self.fatigue_pct,
                threshold: 100.0,
                requires_maintenance_crew: Some(false),
                timestamp_us: now_micros(),
            });
        }

        self.efficiency
    }

    /// Start a break. Worker becomes unavailable for break duration.
    pub fn start_break(&mut self, node_id: &str) -> bool {
        if self.state != EquipmentState::Degraded && self.state != EquipmentState::Operational {
            return false;
        }

        self.break_remaining_secs = WORKER_BREAK_DURATION_SECS;
        self.transition_to(EquipmentState::OnBreak, "fatigue_break", node_id);
        true
    }

    /// Advance break by elapsed sim-seconds. Returns true when break completes.
    pub fn tick_break(&mut self, elapsed_secs: u64, node_id: &str) -> bool {
        if self.state != EquipmentState::OnBreak {
            return false;
        }

        self.break_remaining_secs = self.break_remaining_secs.saturating_sub(elapsed_secs);

        if self.break_remaining_secs == 0 {
            let previous = self.fatigue_pct;
            self.fatigue_pct = 0.0;
            self.continuous_operation_secs = 0;
            self.efficiency = 1.0;
            self.transition_to(EquipmentState::Operational, "break_complete", node_id);

            log_metrics(&MetricsEvent::ResupplyCompleted {
                node_id: node_id.to_string(),
                equipment_id: self.id.clone(),
                equipment_type: "worker".to_string(),
                resource_type: "fatigue_pct".to_string(),
                previous_level: previous,
                restored_level: 0.0,
                duration_sim_secs: WORKER_BREAK_DURATION_SECS,
                timestamp_us: now_micros(),
            });

            return true;
        }

        false
    }

    fn transition_to(&mut self, new_state: EquipmentState, reason: &str, node_id: &str) {
        let previous = self.state;
        self.state = new_state;

        log_metrics(&MetricsEvent::EquipmentStateChanged {
            node_id: node_id.to_string(),
            equipment_id: self.id.clone(),
            equipment_type: "worker".to_string(),
            previous_state: previous.to_string(),
            new_state: self.state.to_string(),
            reason: reason.to_string(),
            timestamp_us: now_micros(),
        });
    }
}

impl MaintenanceCrew {
    pub fn new() -> Self {
        Self {
            available: true,
            busy_remaining_secs: 0,
        }
    }

    /// Advance busy timer. Returns true if crew just became available.
    pub fn tick(&mut self, elapsed_secs: u64) -> bool {
        if self.available {
            return false;
        }

        self.busy_remaining_secs = self.busy_remaining_secs.saturating_sub(elapsed_secs);
        if self.busy_remaining_secs == 0 {
            self.available = true;
            return true;
        }

        false
    }
}

/// All port operations resource state bundled together
#[derive(Debug, Clone)]
pub struct PortOpsResources {
    pub tractor: TractorState,
    pub crane: CraneState,
    pub workers: Vec<WorkerState>,
    pub maintenance_crew: MaintenanceCrew,
}

impl PortOpsResources {
    pub fn new(node_id: &str, num_workers: usize) -> Self {
        let workers = (0..num_workers)
            .map(|i| WorkerState::new(format!("{}-worker-{}", node_id, i)))
            .collect();

        Self {
            tractor: TractorState::new(format!("{}-tractor", node_id)),
            crane: CraneState::new(format!("{}-crane", node_id)),
            workers,
            maintenance_crew: MaintenanceCrew::new(),
        }
    }

    /// Run one simulation tick of the port operations resource cycle.
    /// `sim_elapsed_secs` is how many sim-seconds have passed since last tick.
    /// `transport_this_tick` indicates whether a transport cycle completed.
    /// `lifts_this_tick` is how many crane lifts happened.
    pub fn tick(
        &mut self,
        sim_elapsed_secs: u64,
        transport_this_tick: bool,
        lifts_this_tick: u32,
        battery_drain: f64,
        node_id: &str,
    ) {
        // -- Tractor --
        if transport_this_tick {
            self.tractor.complete_transport_cycle(node_id, battery_drain);
        }
        self.tractor.tick_charging(sim_elapsed_secs, node_id);

        // -- Crane --
        for _ in 0..lifts_this_tick {
            if !self.crane.complete_lift(node_id) {
                break; // Crane not operational
            }
        }
        // Try to begin maintenance if awaiting
        if self.crane.state == EquipmentState::AwaitingMaintenance {
            self.crane
                .begin_maintenance(&mut self.maintenance_crew, node_id);
        }
        self.crane
            .tick_maintenance(sim_elapsed_secs, &mut self.maintenance_crew, node_id);

        // -- Workers --
        for worker in &mut self.workers {
            match worker.state {
                EquipmentState::Operational | EquipmentState::Degraded => {
                    worker.work(sim_elapsed_secs, node_id);
                    // Auto-start break when degraded
                    if worker.state == EquipmentState::Degraded {
                        worker.start_break(node_id);
                    }
                }
                EquipmentState::OnBreak => {
                    worker.tick_break(sim_elapsed_secs, node_id);
                }
                _ => {}
            }
        }

        // -- Maintenance crew timer --
        self.maintenance_crew.tick(sim_elapsed_secs);
    }

    /// Build document fields representing current resource state
    pub fn to_document_fields(&self) -> std::collections::HashMap<String, serde_json::Value> {
        use serde_json::json;
        let mut fields = std::collections::HashMap::new();

        fields.insert(
            "tractor_battery_pct".to_string(),
            json!(self.tractor.battery_pct),
        );
        fields.insert(
            "tractor_state".to_string(),
            json!(self.tractor.state.to_string()),
        );
        fields.insert(
            "tractor_cycles".to_string(),
            json!(self.tractor.cycles_completed),
        );

        fields.insert(
            "crane_hydraulic_fluid_pct".to_string(),
            json!(self.crane.hydraulic_fluid_pct),
        );
        fields.insert(
            "crane_state".to_string(),
            json!(self.crane.state.to_string()),
        );
        fields.insert(
            "crane_lifts".to_string(),
            json!(self.crane.lifts_completed),
        );

        let worker_states: Vec<serde_json::Value> = self
            .workers
            .iter()
            .map(|w| {
                json!({
                    "id": w.id,
                    "fatigue_pct": w.fatigue_pct,
                    "efficiency": w.efficiency,
                    "state": w.state.to_string(),
                    "continuous_operation_secs": w.continuous_operation_secs,
                })
            })
            .collect();
        fields.insert("workers".to_string(), json!(worker_states));

        fields.insert(
            "maintenance_crew_available".to_string(),
            json!(self.maintenance_crew.available),
        );

        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tractor_battery_drain() {
        let mut tractor = TractorState::new("t1".to_string());
        assert_eq!(tractor.battery_pct, 100.0);
        assert_eq!(tractor.state, EquipmentState::Operational);

        // Complete a transport cycle with 4% drain
        assert!(tractor.complete_transport_cycle("node1", 4.0));
        assert!((tractor.battery_pct - 96.0).abs() < f64::EPSILON);
        assert_eq!(tractor.cycles_completed, 1);
        assert_eq!(tractor.state, EquipmentState::Operational);
    }

    #[test]
    fn test_tractor_battery_threshold_triggers_charging() {
        let mut tractor = TractorState::new("t1".to_string());
        tractor.battery_pct = 22.0;

        // This cycle should trigger charging (22 - 4 = 18 < 20)
        tractor.complete_transport_cycle("node1", 4.0);
        assert_eq!(tractor.state, EquipmentState::Charging);
        assert_eq!(tractor.charge_remaining_secs, TRACTOR_CHARGE_DURATION_SECS);
    }

    #[test]
    fn test_tractor_charging_cycle() {
        let mut tractor = TractorState::new("t1".to_string());
        tractor.battery_pct = 15.0;
        tractor.state = EquipmentState::Charging;
        tractor.charge_remaining_secs = TRACTOR_CHARGE_DURATION_SECS;

        // Tick partway through
        assert!(!tractor.tick_charging(500, "node1"));
        assert_eq!(tractor.state, EquipmentState::Charging);
        assert_eq!(
            tractor.charge_remaining_secs,
            TRACTOR_CHARGE_DURATION_SECS - 500
        );

        // Tick to completion
        assert!(tractor.tick_charging(TRACTOR_CHARGE_DURATION_SECS, "node1"));
        assert_eq!(tractor.state, EquipmentState::Operational);
        assert!((tractor.battery_pct - TRACTOR_CHARGE_TARGET).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tractor_cannot_transport_while_charging() {
        let mut tractor = TractorState::new("t1".to_string());
        tractor.state = EquipmentState::Charging;

        assert!(!tractor.complete_transport_cycle("node1", 4.0));
        assert_eq!(tractor.cycles_completed, 0);
    }

    #[test]
    fn test_tractor_drain_clamped() {
        let mut tractor = TractorState::new("t1".to_string());

        // Drain is clamped to [3.0, 5.0]
        tractor.complete_transport_cycle("node1", 1.0); // clamped to 3.0
        assert!((tractor.battery_pct - 97.0).abs() < f64::EPSILON);

        tractor.complete_transport_cycle("node1", 10.0); // clamped to 5.0
        assert!((tractor.battery_pct - 92.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_crane_hydraulic_drain() {
        let mut crane = CraneState::new("c1".to_string());
        assert_eq!(crane.hydraulic_fluid_pct, 100.0);

        assert!(crane.complete_lift("node1"));
        assert!(
            (crane.hydraulic_fluid_pct - (100.0 - CRANE_FLUID_PER_LIFT)).abs() < f64::EPSILON
        );
        assert_eq!(crane.lifts_completed, 1);
    }

    #[test]
    fn test_crane_maintenance_threshold() {
        let mut crane = CraneState::new("c1".to_string());
        crane.hydraulic_fluid_pct = 26.0;

        crane.complete_lift("node1"); // 26 - 1.5 = 24.5 < 25
        assert_eq!(crane.state, EquipmentState::AwaitingMaintenance);
    }

    #[test]
    fn test_crane_maintenance_requires_crew() {
        let mut crane = CraneState::new("c1".to_string());
        crane.state = EquipmentState::AwaitingMaintenance;

        let mut crew = MaintenanceCrew::new();
        crew.available = false;

        // Can't start without crew
        assert!(!crane.begin_maintenance(&mut crew, "node1"));
        assert_eq!(crane.state, EquipmentState::AwaitingMaintenance);

        // Crew becomes available
        crew.available = true;
        assert!(crane.begin_maintenance(&mut crew, "node1"));
        assert_eq!(crane.state, EquipmentState::UnderMaintenance);
        assert!(!crew.available);
    }

    #[test]
    fn test_crane_maintenance_completes() {
        let mut crane = CraneState::new("c1".to_string());
        crane.hydraulic_fluid_pct = 20.0;
        crane.state = EquipmentState::UnderMaintenance;
        crane.maintenance_remaining_secs = CRANE_MAINTENANCE_DURATION_SECS;

        let mut crew = MaintenanceCrew::new();
        crew.available = false;
        crew.busy_remaining_secs = CRANE_MAINTENANCE_DURATION_SECS;

        assert!(crane.tick_maintenance(CRANE_MAINTENANCE_DURATION_SECS, &mut crew, "node1"));
        assert_eq!(crane.state, EquipmentState::Operational);
        assert!((crane.hydraulic_fluid_pct - CRANE_FLUID_TARGET).abs() < f64::EPSILON);
        assert!(crew.available);
    }

    #[test]
    fn test_worker_fatigue_accumulation() {
        let mut worker = WorkerState::new("w1".to_string());
        assert_eq!(worker.fatigue_pct, 0.0);
        assert_eq!(worker.efficiency, 1.0);

        // Work for 2 hours (7200 sec) - should be at 50% fatigue
        let eff = worker.work(7200, "node1");
        assert_eq!(eff, 1.0); // Still normal efficiency
        assert!((worker.fatigue_pct - 50.0).abs() < f64::EPSILON);
        assert_eq!(worker.state, EquipmentState::Operational);
    }

    #[test]
    fn test_worker_fatigue_degrades_efficiency() {
        let mut worker = WorkerState::new("w1".to_string());

        // Work for exactly 4 hours
        worker.work(WORKER_FATIGUE_THRESHOLD_SECS, "node1");
        assert_eq!(worker.state, EquipmentState::Degraded);
        assert!((worker.efficiency - WORKER_FATIGUED_EFFICIENCY).abs() < f64::EPSILON);
    }

    #[test]
    fn test_worker_break_restores() {
        let mut worker = WorkerState::new("w1".to_string());
        worker.fatigue_pct = 100.0;
        worker.continuous_operation_secs = WORKER_FATIGUE_THRESHOLD_SECS;
        worker.state = EquipmentState::Degraded;
        worker.efficiency = WORKER_FATIGUED_EFFICIENCY;

        assert!(worker.start_break("node1"));
        assert_eq!(worker.state, EquipmentState::OnBreak);

        // Complete the break
        assert!(worker.tick_break(WORKER_BREAK_DURATION_SECS, "node1"));
        assert_eq!(worker.state, EquipmentState::Operational);
        assert!((worker.fatigue_pct - 0.0).abs() < f64::EPSILON);
        assert!((worker.efficiency - 1.0).abs() < f64::EPSILON);
        assert_eq!(worker.continuous_operation_secs, 0);
    }

    #[test]
    fn test_maintenance_crew_timer() {
        let mut crew = MaintenanceCrew::new();
        assert!(crew.available);

        crew.available = false;
        crew.busy_remaining_secs = 100;

        assert!(!crew.tick(50));
        assert!(!crew.available);
        assert_eq!(crew.busy_remaining_secs, 50);

        assert!(crew.tick(50));
        assert!(crew.available);
    }

    #[test]
    fn test_port_ops_resources_tick() {
        let mut resources = PortOpsResources::new("port1", 2);

        // Tick with a transport cycle and some lifts
        resources.tick(300, true, 2, 4.0, "port1");

        // Tractor should have drained
        assert!(resources.tractor.battery_pct < 100.0);
        assert_eq!(resources.tractor.cycles_completed, 1);

        // Crane should have completed lifts
        assert!(resources.crane.hydraulic_fluid_pct < 100.0);
        assert_eq!(resources.crane.lifts_completed, 2);

        // Workers should have accumulated some fatigue
        for worker in &resources.workers {
            assert!(worker.continuous_operation_secs > 0);
        }
    }

    #[test]
    fn test_port_ops_to_document_fields() {
        let resources = PortOpsResources::new("port1", 2);
        let fields = resources.to_document_fields();

        assert!(fields.contains_key("tractor_battery_pct"));
        assert!(fields.contains_key("tractor_state"));
        assert!(fields.contains_key("crane_hydraulic_fluid_pct"));
        assert!(fields.contains_key("crane_state"));
        assert!(fields.contains_key("workers"));
        assert!(fields.contains_key("maintenance_crew_available"));
    }

    #[test]
    fn test_logistical_dependency_chain() {
        // Without maintenance crew, crane stays in AwaitingMaintenance
        let mut crane = CraneState::new("c1".to_string());
        crane.state = EquipmentState::AwaitingMaintenance;

        let mut crew = MaintenanceCrew::new();
        crew.available = false;
        crew.busy_remaining_secs = 500;

        // Can't start maintenance
        assert!(!crane.begin_maintenance(&mut crew, "node1"));
        assert_eq!(crane.state, EquipmentState::AwaitingMaintenance);

        // Crew finishes other job
        crew.tick(500);
        assert!(crew.available);

        // Now maintenance can begin
        assert!(crane.begin_maintenance(&mut crew, "node1"));
        assert_eq!(crane.state, EquipmentState::UnderMaintenance);
    }
}
