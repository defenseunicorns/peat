//! Logistical support events and dependency tracking for port-ops simulation.
//!
//! Models the support chain keeping capabilities alive. Tracks pending
//! logistical actions (maintenance, resupply, recertification, shift relief)
//! and factors them into capability gap analysis.
//!
//! # Event types
//!
//! | Category         | Events                                                |
//! |------------------|-------------------------------------------------------|
//! | Maintenance      | scheduled → started → complete                        |
//! | Resupply         | dispatched → delivered                                |
//! | Recertification  | assigned → complete                                   |
//! | Shift relief     | requested → arrived                                   |
//!
//! # Hold aggregator
//!
//! Tracks all pending logistical actions and produces gap analysis summaries.
//! Each pending action references the capability it sustains and the estimated
//! time to restore, enabling upstream viewers to display degradation context.

use crate::metrics::{log_metrics, MetricsEvent};
use crate::utils::time::now_micros;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default maintenance duration for equipment (30 min sim-time).
const DEFAULT_MAINTENANCE_DURATION_SECS: u64 = 1800;
/// Default resupply transit time (10 min sim-time).
const DEFAULT_RESUPPLY_TRANSIT_SECS: u64 = 600;
/// Default recertification duration (45 min sim-time).
const DEFAULT_RECERTIFICATION_DURATION_SECS: u64 = 2700;
/// Default shift relief arrival time (15 min sim-time).
const DEFAULT_SHIFT_RELIEF_ARRIVAL_SECS: u64 = 900;

// ---------------------------------------------------------------------------
// Logistical action types
// ---------------------------------------------------------------------------

/// Category of logistical support action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionCategory {
    Maintenance,
    Resupply,
    Recertification,
    ShiftRelief,
}

impl std::fmt::Display for ActionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionCategory::Maintenance => write!(f, "maintenance"),
            ActionCategory::Resupply => write!(f, "resupply"),
            ActionCategory::Recertification => write!(f, "recertification"),
            ActionCategory::ShiftRelief => write!(f, "shift_relief"),
        }
    }
}

/// Status of a logistical action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStatus {
    /// Action is scheduled/dispatched/assigned/requested but not yet started/arrived.
    Pending,
    /// Action is actively in progress (maintenance started, resupply in transit).
    InProgress,
    /// Action completed successfully.
    Complete,
}

impl std::fmt::Display for ActionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionStatus::Pending => write!(f, "PENDING"),
            ActionStatus::InProgress => write!(f, "IN_PROGRESS"),
            ActionStatus::Complete => write!(f, "COMPLETE"),
        }
    }
}

/// A single pending logistical action tracked by the hold aggregator.
#[derive(Debug, Clone)]
pub struct LogisticalAction {
    /// Unique action identifier.
    pub action_id: String,
    /// What kind of logistical action.
    pub category: ActionCategory,
    /// Current status.
    pub status: ActionStatus,
    /// Equipment or worker this action targets.
    pub target_id: String,
    /// Capability this action sustains.
    pub capability_id: String,
    /// Sim-time (us) when action was created.
    pub created_at_us: u128,
    /// Estimated duration in sim-seconds.
    pub estimated_duration_secs: u64,
    /// Sim-time (us) when capability is estimated to be restored.
    pub estimated_restore_time_us: u128,
    /// Sim-seconds elapsed in current phase.
    pub elapsed_secs: u64,
    /// Additional context (resource type, cert type, reason, etc.).
    pub detail: String,
    /// Crew ID if assigned.
    pub crew_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Logistical event dispatcher
// ---------------------------------------------------------------------------

/// Dispatches logistical events and registers them with the hold aggregator.
pub struct LogisticsDispatcher {
    pub node_id: String,
    next_action_id: u64,
}

impl LogisticsDispatcher {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            next_action_id: 0,
        }
    }

    fn next_id(&mut self) -> String {
        let id = format!("{}-logistic-{}", self.node_id, self.next_action_id);
        self.next_action_id += 1;
        id
    }

    /// Schedule maintenance for equipment. Returns the action to register.
    pub fn schedule_maintenance(
        &mut self,
        equipment_id: &str,
        capability_id: &str,
        reason: &str,
        duration_secs: Option<u64>,
    ) -> LogisticalAction {
        let duration = duration_secs.unwrap_or(DEFAULT_MAINTENANCE_DURATION_SECS);
        let now = now_micros();
        let restore_time = now + (duration as u128) * 1_000_000;

        log_metrics(&MetricsEvent::MaintenanceScheduled {
            node_id: self.node_id.clone(),
            equipment_id: equipment_id.to_string(),
            capability_id: capability_id.to_string(),
            reason: reason.to_string(),
            estimated_duration_secs: duration,
            estimated_restore_time_us: restore_time,
            timestamp_us: now,
        });

        LogisticalAction {
            action_id: self.next_id(),
            category: ActionCategory::Maintenance,
            status: ActionStatus::Pending,
            target_id: equipment_id.to_string(),
            capability_id: capability_id.to_string(),
            created_at_us: now,
            estimated_duration_secs: duration,
            estimated_restore_time_us: restore_time,
            elapsed_secs: 0,
            detail: reason.to_string(),
            crew_id: None,
        }
    }

    /// Start maintenance (crew assigned). Returns updated action.
    pub fn start_maintenance(
        &mut self,
        action: &mut LogisticalAction,
        crew_id: &str,
    ) {
        action.status = ActionStatus::InProgress;
        action.crew_id = Some(crew_id.to_string());
        action.elapsed_secs = 0;
        let now = now_micros();
        action.estimated_restore_time_us = now + (action.estimated_duration_secs as u128) * 1_000_000;

        log_metrics(&MetricsEvent::MaintenanceStarted {
            node_id: self.node_id.clone(),
            equipment_id: action.target_id.clone(),
            capability_id: action.capability_id.clone(),
            crew_id: crew_id.to_string(),
            estimated_duration_secs: action.estimated_duration_secs,
            estimated_restore_time_us: action.estimated_restore_time_us,
            timestamp_us: now,
        });
    }

    /// Complete maintenance. Returns restored confidence.
    pub fn complete_maintenance(
        &mut self,
        action: &mut LogisticalAction,
        restored_confidence: f32,
    ) {
        action.status = ActionStatus::Complete;
        let now = now_micros();

        log_metrics(&MetricsEvent::MaintenanceComplete {
            node_id: self.node_id.clone(),
            equipment_id: action.target_id.clone(),
            capability_id: action.capability_id.clone(),
            crew_id: action.crew_id.clone().unwrap_or_default(),
            actual_duration_secs: action.elapsed_secs,
            restored_confidence,
            timestamp_us: now,
        });
    }

    /// Dispatch a resupply. Returns the action to register.
    pub fn dispatch_resupply(
        &mut self,
        equipment_id: &str,
        capability_id: &str,
        resource_type: &str,
        quantity_pct: f64,
        transit_secs: Option<u64>,
    ) -> LogisticalAction {
        let transit = transit_secs.unwrap_or(DEFAULT_RESUPPLY_TRANSIT_SECS);
        let now = now_micros();
        let restore_time = now + (transit as u128) * 1_000_000;

        log_metrics(&MetricsEvent::ResupplyDispatched {
            node_id: self.node_id.clone(),
            equipment_id: equipment_id.to_string(),
            capability_id: capability_id.to_string(),
            resource_type: resource_type.to_string(),
            quantity_pct,
            estimated_arrival_secs: transit,
            estimated_restore_time_us: restore_time,
            timestamp_us: now,
        });

        LogisticalAction {
            action_id: self.next_id(),
            category: ActionCategory::Resupply,
            status: ActionStatus::InProgress,
            target_id: equipment_id.to_string(),
            capability_id: capability_id.to_string(),
            created_at_us: now,
            estimated_duration_secs: transit,
            estimated_restore_time_us: restore_time,
            elapsed_secs: 0,
            detail: format!("{}:{:.0}%", resource_type, quantity_pct),
            crew_id: None,
        }
    }

    /// Deliver a resupply.
    pub fn deliver_resupply(
        &mut self,
        action: &mut LogisticalAction,
        previous_level: f64,
        restored_level: f64,
    ) {
        action.status = ActionStatus::Complete;
        let resource_type = action.detail.split(':').next().unwrap_or("unknown");
        let quantity_pct = restored_level - previous_level;
        let now = now_micros();

        log_metrics(&MetricsEvent::ResupplyDelivered {
            node_id: self.node_id.clone(),
            equipment_id: action.target_id.clone(),
            capability_id: action.capability_id.clone(),
            resource_type: resource_type.to_string(),
            quantity_pct,
            previous_level,
            restored_level,
            transit_duration_secs: action.elapsed_secs,
            timestamp_us: now,
        });
    }

    /// Assign recertification. Returns the action to register.
    pub fn assign_recertification(
        &mut self,
        worker_id: &str,
        capability_id: &str,
        cert_type: &str,
        duration_secs: Option<u64>,
    ) -> LogisticalAction {
        let duration = duration_secs.unwrap_or(DEFAULT_RECERTIFICATION_DURATION_SECS);
        let now = now_micros();
        let restore_time = now + (duration as u128) * 1_000_000;

        log_metrics(&MetricsEvent::RecertificationAssigned {
            node_id: self.node_id.clone(),
            worker_id: worker_id.to_string(),
            capability_id: capability_id.to_string(),
            cert_type: cert_type.to_string(),
            estimated_duration_secs: duration,
            estimated_restore_time_us: restore_time,
            timestamp_us: now,
        });

        LogisticalAction {
            action_id: self.next_id(),
            category: ActionCategory::Recertification,
            status: ActionStatus::InProgress,
            target_id: worker_id.to_string(),
            capability_id: capability_id.to_string(),
            created_at_us: now,
            estimated_duration_secs: duration,
            estimated_restore_time_us: restore_time,
            elapsed_secs: 0,
            detail: cert_type.to_string(),
            crew_id: None,
        }
    }

    /// Complete recertification.
    pub fn complete_recertification(
        &mut self,
        action: &mut LogisticalAction,
        restored_confidence: f32,
    ) {
        action.status = ActionStatus::Complete;
        let now = now_micros();

        log_metrics(&MetricsEvent::RecertificationComplete {
            node_id: self.node_id.clone(),
            worker_id: action.target_id.clone(),
            capability_id: action.capability_id.clone(),
            cert_type: action.detail.clone(),
            actual_duration_secs: action.elapsed_secs,
            restored_confidence,
            timestamp_us: now,
        });
    }

    /// Request shift relief. Returns the action to register.
    pub fn request_shift_relief(
        &mut self,
        worker_id: &str,
        capability_id: &str,
        reason: &str,
        arrival_secs: Option<u64>,
    ) -> LogisticalAction {
        let arrival = arrival_secs.unwrap_or(DEFAULT_SHIFT_RELIEF_ARRIVAL_SECS);
        let now = now_micros();
        let restore_time = now + (arrival as u128) * 1_000_000;

        log_metrics(&MetricsEvent::ShiftReliefRequested {
            node_id: self.node_id.clone(),
            worker_id: worker_id.to_string(),
            capability_id: capability_id.to_string(),
            reason: reason.to_string(),
            estimated_arrival_secs: arrival,
            estimated_restore_time_us: restore_time,
            timestamp_us: now,
        });

        LogisticalAction {
            action_id: self.next_id(),
            category: ActionCategory::ShiftRelief,
            status: ActionStatus::Pending,
            target_id: worker_id.to_string(),
            capability_id: capability_id.to_string(),
            created_at_us: now,
            estimated_duration_secs: arrival,
            estimated_restore_time_us: restore_time,
            elapsed_secs: 0,
            detail: reason.to_string(),
            crew_id: None,
        }
    }

    /// Shift relief has arrived.
    pub fn arrive_shift_relief(
        &mut self,
        action: &mut LogisticalAction,
        incoming_worker_id: &str,
        restored_confidence: f32,
    ) {
        action.status = ActionStatus::Complete;
        let now = now_micros();

        log_metrics(&MetricsEvent::ShiftReliefArrived {
            node_id: self.node_id.clone(),
            outgoing_worker_id: action.target_id.clone(),
            incoming_worker_id: incoming_worker_id.to_string(),
            capability_id: action.capability_id.clone(),
            wait_duration_secs: action.elapsed_secs,
            restored_confidence,
            timestamp_us: now,
        });
    }
}

// ---------------------------------------------------------------------------
// Hold aggregator
// ---------------------------------------------------------------------------

/// Tracks pending logistical actions and produces gap analysis.
///
/// The hold aggregator maintains a registry of all in-flight logistical
/// actions across equipment and personnel. It factors pending actions
/// into capability gap analysis, producing summaries like:
/// "crane-2 DEGRADED, maintenance ETA 20 min sim time".
pub struct HoldAggregator {
    node_id: String,
    actions: HashMap<String, LogisticalAction>,
}

impl HoldAggregator {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            actions: HashMap::new(),
        }
    }

    /// Register a new logistical action for tracking.
    pub fn register(&mut self, action: LogisticalAction) {
        self.actions.insert(action.action_id.clone(), action);
    }

    /// Update an existing action (e.g., after status change).
    pub fn update(&mut self, action: &LogisticalAction) {
        if let Some(existing) = self.actions.get_mut(&action.action_id) {
            *existing = action.clone();
        }
    }

    /// Advance all pending/in-progress actions by sim-seconds.
    /// Returns action IDs that just crossed their estimated duration this tick.
    pub fn tick(&mut self, elapsed_secs: u64) -> Vec<String> {
        let mut ready = Vec::new();

        for action in self.actions.values_mut() {
            if action.status == ActionStatus::Complete {
                continue;
            }
            let was_under = action.elapsed_secs < action.estimated_duration_secs;
            action.elapsed_secs += elapsed_secs;
            if was_under && action.elapsed_secs >= action.estimated_duration_secs {
                ready.push(action.action_id.clone());
            }
        }

        ready
    }

    /// Remove completed actions from tracking.
    pub fn prune_completed(&mut self) {
        self.actions.retain(|_, a| a.status != ActionStatus::Complete);
    }

    /// Get a mutable reference to an action by ID.
    pub fn get_mut(&mut self, action_id: &str) -> Option<&mut LogisticalAction> {
        self.actions.get_mut(action_id)
    }

    /// Get an immutable reference to an action by ID.
    pub fn get(&self, action_id: &str) -> Option<&LogisticalAction> {
        self.actions.get(action_id)
    }

    /// Count of pending (non-complete) actions.
    pub fn pending_count(&self) -> usize {
        self.actions
            .values()
            .filter(|a| a.status != ActionStatus::Complete)
            .count()
    }

    /// Capabilities affected by pending actions.
    pub fn affected_capabilities(&self) -> Vec<String> {
        let mut caps: Vec<String> = self
            .actions
            .values()
            .filter(|a| a.status != ActionStatus::Complete)
            .map(|a| a.capability_id.clone())
            .collect();
        caps.sort();
        caps.dedup();
        caps
    }

    /// Produce a gap analysis summary string for all pending actions.
    ///
    /// Format: "crane-2 DEGRADED, maintenance ETA 20 min; tractor-1 OFFLINE, resupply ETA 5 min"
    pub fn gap_summary(&self, current_time_us: u128) -> String {
        let mut entries: Vec<String> = self
            .actions
            .values()
            .filter(|a| a.status != ActionStatus::Complete)
            .map(|a| {
                let eta_secs = if a.estimated_restore_time_us > current_time_us {
                    ((a.estimated_restore_time_us - current_time_us) / 1_000_000) as u64
                } else {
                    0
                };
                let eta_min = eta_secs / 60;
                let status = match a.status {
                    ActionStatus::Pending => "PENDING",
                    ActionStatus::InProgress => "IN_PROGRESS",
                    ActionStatus::Complete => "COMPLETE",
                };
                format!(
                    "{} {}, {} ETA {} min",
                    a.target_id, status, a.category, eta_min
                )
            })
            .collect();
        entries.sort();
        entries.join("; ")
    }

    /// Emit a hold aggregator update event with current gap analysis.
    pub fn emit_update(&self) {
        let now = now_micros();
        let pending = self.pending_count();
        if pending == 0 {
            return;
        }

        log_metrics(&MetricsEvent::HoldAggregatorUpdate {
            node_id: self.node_id.clone(),
            pending_actions: pending,
            capabilities_affected: self.affected_capabilities(),
            gap_summary: self.gap_summary(now),
            timestamp_us: now,
        });
    }

    /// Get all pending actions as a slice-like iterator.
    pub fn pending_actions(&self) -> Vec<&LogisticalAction> {
        self.actions
            .values()
            .filter(|a| a.status != ActionStatus::Complete)
            .collect()
    }

    /// Produce document fields for CRDT sync.
    pub fn to_document_fields(&self) -> HashMap<String, serde_json::Value> {
        use serde_json::json;
        let mut fields = HashMap::new();

        let pending: Vec<serde_json::Value> = self
            .actions
            .values()
            .filter(|a| a.status != ActionStatus::Complete)
            .map(|a| {
                json!({
                    "action_id": a.action_id,
                    "category": a.category.to_string(),
                    "status": a.status.to_string(),
                    "target_id": a.target_id,
                    "capability_id": a.capability_id,
                    "estimated_duration_secs": a.estimated_duration_secs,
                    "elapsed_secs": a.elapsed_secs,
                    "detail": a.detail,
                })
            })
            .collect();

        fields.insert("pending_logistical_actions".to_string(), json!(pending));
        fields.insert(
            "logistical_gap_count".to_string(),
            json!(self.pending_count()),
        );
        fields.insert(
            "affected_capabilities".to_string(),
            json!(self.affected_capabilities()),
        );
        let now = now_micros();
        fields.insert(
            "gap_summary".to_string(),
            json!(self.gap_summary(now)),
        );

        fields
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dispatcher() -> LogisticsDispatcher {
        LogisticsDispatcher::new("node-1".into())
    }

    fn make_aggregator() -> HoldAggregator {
        HoldAggregator::new("node-1".into())
    }

    // -- Maintenance lifecycle --

    #[test]
    fn schedule_maintenance_creates_pending_action() {
        let mut disp = make_dispatcher();
        let action = disp.schedule_maintenance("crane-2", "crane-ops", "hydraulic_low", None);

        assert_eq!(action.category, ActionCategory::Maintenance);
        assert_eq!(action.status, ActionStatus::Pending);
        assert_eq!(action.target_id, "crane-2");
        assert_eq!(action.capability_id, "crane-ops");
        assert_eq!(action.estimated_duration_secs, DEFAULT_MAINTENANCE_DURATION_SECS);
    }

    #[test]
    fn start_maintenance_transitions_to_in_progress() {
        let mut disp = make_dispatcher();
        let mut action = disp.schedule_maintenance("crane-2", "crane-ops", "hydraulic_low", None);

        disp.start_maintenance(&mut action, "crew-alpha");

        assert_eq!(action.status, ActionStatus::InProgress);
        assert_eq!(action.crew_id, Some("crew-alpha".to_string()));
    }

    #[test]
    fn complete_maintenance_transitions_to_complete() {
        let mut disp = make_dispatcher();
        let mut action = disp.schedule_maintenance("crane-2", "crane-ops", "hydraulic_low", None);
        disp.start_maintenance(&mut action, "crew-alpha");
        action.elapsed_secs = 1800;

        disp.complete_maintenance(&mut action, 1.0);

        assert_eq!(action.status, ActionStatus::Complete);
    }

    #[test]
    fn maintenance_custom_duration() {
        let mut disp = make_dispatcher();
        let action = disp.schedule_maintenance("crane-2", "crane-ops", "spreader_drift", Some(600));

        assert_eq!(action.estimated_duration_secs, 600);
    }

    // -- Resupply lifecycle --

    #[test]
    fn dispatch_resupply_creates_in_progress_action() {
        let mut disp = make_dispatcher();
        let action = disp.dispatch_resupply("tractor-1", "transport", "battery", 80.0, None);

        assert_eq!(action.category, ActionCategory::Resupply);
        assert_eq!(action.status, ActionStatus::InProgress);
        assert_eq!(action.estimated_duration_secs, DEFAULT_RESUPPLY_TRANSIT_SECS);
    }

    #[test]
    fn deliver_resupply_completes_action() {
        let mut disp = make_dispatcher();
        let mut action = disp.dispatch_resupply("tractor-1", "transport", "battery", 80.0, None);
        action.elapsed_secs = 600;

        disp.deliver_resupply(&mut action, 15.0, 100.0);

        assert_eq!(action.status, ActionStatus::Complete);
    }

    #[test]
    fn resupply_custom_transit_time() {
        let mut disp = make_dispatcher();
        let action =
            disp.dispatch_resupply("tractor-1", "transport", "battery", 80.0, Some(300));

        assert_eq!(action.estimated_duration_secs, 300);
    }

    // -- Recertification lifecycle --

    #[test]
    fn assign_recertification_creates_in_progress_action() {
        let mut disp = make_dispatcher();
        let action =
            disp.assign_recertification("worker-1", "hazmat-handling", "hazmat_class_3", None);

        assert_eq!(action.category, ActionCategory::Recertification);
        assert_eq!(action.status, ActionStatus::InProgress);
        assert_eq!(
            action.estimated_duration_secs,
            DEFAULT_RECERTIFICATION_DURATION_SECS
        );
    }

    #[test]
    fn complete_recertification_transitions_to_complete() {
        let mut disp = make_dispatcher();
        let mut action =
            disp.assign_recertification("worker-1", "hazmat-handling", "hazmat_class_3", None);
        action.elapsed_secs = 2700;

        disp.complete_recertification(&mut action, 1.0);

        assert_eq!(action.status, ActionStatus::Complete);
    }

    // -- Shift relief lifecycle --

    #[test]
    fn request_shift_relief_creates_pending_action() {
        let mut disp = make_dispatcher();
        let action = disp.request_shift_relief("worker-1", "crane-ops", "fatigue", None);

        assert_eq!(action.category, ActionCategory::ShiftRelief);
        assert_eq!(action.status, ActionStatus::Pending);
        assert_eq!(
            action.estimated_duration_secs,
            DEFAULT_SHIFT_RELIEF_ARRIVAL_SECS
        );
    }

    #[test]
    fn arrive_shift_relief_completes_action() {
        let mut disp = make_dispatcher();
        let mut action = disp.request_shift_relief("worker-1", "crane-ops", "fatigue", None);
        action.elapsed_secs = 900;

        disp.arrive_shift_relief(&mut action, "worker-2", 1.0);

        assert_eq!(action.status, ActionStatus::Complete);
    }

    // -- Hold aggregator --

    #[test]
    fn aggregator_registers_and_counts_actions() {
        let mut disp = make_dispatcher();
        let mut agg = make_aggregator();

        let a1 = disp.schedule_maintenance("crane-1", "crane-ops", "hydraulic_low", None);
        let a2 = disp.dispatch_resupply("tractor-1", "transport", "battery", 80.0, None);

        agg.register(a1);
        agg.register(a2);

        assert_eq!(agg.pending_count(), 2);
    }

    #[test]
    fn aggregator_tick_advances_elapsed_time() {
        let mut disp = make_dispatcher();
        let mut agg = make_aggregator();

        let action = disp.schedule_maintenance("crane-1", "crane-ops", "hydraulic_low", Some(60));
        let id = action.action_id.clone();
        agg.register(action);

        let ready = agg.tick(30);
        assert!(ready.is_empty());
        assert_eq!(agg.get(&id).unwrap().elapsed_secs, 30);

        let ready = agg.tick(30);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], id);
    }

    #[test]
    fn aggregator_affected_capabilities_deduplicates() {
        let mut disp = make_dispatcher();
        let mut agg = make_aggregator();

        let a1 = disp.schedule_maintenance("crane-1", "crane-ops", "hydraulic_low", None);
        let a2 = disp.request_shift_relief("worker-1", "crane-ops", "fatigue", None);
        let a3 = disp.dispatch_resupply("tractor-1", "transport", "battery", 80.0, None);

        agg.register(a1);
        agg.register(a2);
        agg.register(a3);

        let caps = agg.affected_capabilities();
        assert_eq!(caps.len(), 2); // "crane-ops" deduplicated, "transport"
        assert!(caps.contains(&"crane-ops".to_string()));
        assert!(caps.contains(&"transport".to_string()));
    }

    #[test]
    fn aggregator_prune_removes_completed() {
        let mut disp = make_dispatcher();
        let mut agg = make_aggregator();

        let mut action = disp.schedule_maintenance("crane-1", "crane-ops", "hydraulic_low", None);
        disp.start_maintenance(&mut action, "crew-alpha");
        disp.complete_maintenance(&mut action, 1.0);
        agg.register(action);

        assert_eq!(agg.pending_count(), 0);
        agg.prune_completed();
        assert_eq!(agg.actions.len(), 0);
    }

    #[test]
    fn aggregator_gap_summary_format() {
        let mut disp = make_dispatcher();
        let mut agg = make_aggregator();

        let action = disp.schedule_maintenance("crane-2", "crane-ops", "hydraulic_low", Some(1200));
        agg.register(action);

        let summary = agg.gap_summary(now_micros());
        assert!(summary.contains("crane-2"));
        assert!(summary.contains("maintenance"));
        assert!(summary.contains("ETA"));
    }

    #[test]
    fn aggregator_to_document_fields() {
        let mut disp = make_dispatcher();
        let mut agg = make_aggregator();

        let a1 = disp.schedule_maintenance("crane-1", "crane-ops", "hydraulic_low", None);
        agg.register(a1);

        let fields = agg.to_document_fields();
        assert!(fields.contains_key("pending_logistical_actions"));
        assert!(fields.contains_key("logistical_gap_count"));
        assert!(fields.contains_key("affected_capabilities"));
        assert!(fields.contains_key("gap_summary"));

        let count = fields["logistical_gap_count"].as_u64().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn full_maintenance_lifecycle_with_aggregator() {
        let mut disp = make_dispatcher();
        let mut agg = make_aggregator();

        // 1. Schedule maintenance
        let action = disp.schedule_maintenance("crane-2", "crane-ops", "hydraulic_low", Some(120));
        let action_id = action.action_id.clone();
        agg.register(action);
        assert_eq!(agg.pending_count(), 1);

        // 2. Start maintenance
        let action = agg.get_mut(&action_id).unwrap();
        disp.start_maintenance(action, "crew-alpha");
        assert_eq!(action.status, ActionStatus::InProgress);

        // 3. Tick to half duration
        let ready = agg.tick(60);
        assert!(ready.is_empty());
        assert_eq!(agg.pending_count(), 1);

        // 4. Tick to completion
        let ready = agg.tick(60);
        assert_eq!(ready.len(), 1);

        // 5. Complete
        {
            let action = agg.get_mut(&action_id).unwrap();
            disp.complete_maintenance(action, 1.0);
        }

        assert_eq!(agg.pending_count(), 0);
        agg.prune_completed();
        assert_eq!(agg.actions.len(), 0);
    }

    #[test]
    fn multiple_concurrent_actions() {
        let mut disp = make_dispatcher();
        let mut agg = make_aggregator();

        let a1 = disp.schedule_maintenance("crane-1", "crane-ops", "hydraulic_low", Some(300));
        let a2 = disp.dispatch_resupply("tractor-1", "transport", "battery", 80.0, Some(100));
        let a3 = disp.request_shift_relief("worker-1", "workforce", "fatigue", Some(200));

        agg.register(a1);
        agg.register(a2);
        agg.register(a3);

        assert_eq!(agg.pending_count(), 3);

        // Tick 100 secs: resupply should be ready
        let ready = agg.tick(100);
        assert_eq!(ready.len(), 1);

        // Tick another 100 secs: shift relief should be ready
        let ready = agg.tick(100);
        assert_eq!(ready.len(), 1);

        // Tick another 100 secs: maintenance should be ready
        let ready = agg.tick(100);
        assert_eq!(ready.len(), 1);
    }

    #[test]
    fn action_id_uniqueness() {
        let mut disp = make_dispatcher();

        let a1 = disp.schedule_maintenance("crane-1", "crane-ops", "hydraulic_low", None);
        let a2 = disp.schedule_maintenance("crane-2", "crane-ops", "hydraulic_low", None);
        let a3 = disp.dispatch_resupply("tractor-1", "transport", "battery", 80.0, None);

        assert_ne!(a1.action_id, a2.action_id);
        assert_ne!(a2.action_id, a3.action_id);
    }

    #[test]
    fn tick_skips_completed_actions() {
        let mut disp = make_dispatcher();
        let mut agg = make_aggregator();

        let mut action = disp.schedule_maintenance("crane-1", "crane-ops", "hydraulic_low", Some(60));
        disp.start_maintenance(&mut action, "crew-alpha");
        disp.complete_maintenance(&mut action, 1.0);
        agg.register(action);

        let ready = agg.tick(120);
        assert!(ready.is_empty()); // completed actions should not appear
    }
}
