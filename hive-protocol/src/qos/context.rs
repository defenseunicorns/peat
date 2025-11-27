//! Mission context for dynamic QoS adjustment (ADR-019)
//!
//! This module provides context-aware priority profiles that dynamically
//! adjust QoS policies based on the current mission phase.
//!
//! # Mission Phases
//!
//! - **Ingress**: Moving to objective - prioritize enemy detection
//! - **Execution**: On objective - prioritize intel products
//! - **Egress**: Returning - prioritize health/status
//! - **Emergency**: Emergency situation - elevate all critical data
//! - **Standby**: Waiting/holding - normal priorities, enable bulk sync
//!
//! # Example
//!
//! ```
//! use hive_protocol::qos::{MissionContext, ContextProfile, QoSClass, DataType};
//!
//! // Get the execution phase profile
//! let profile = ContextProfile::execution();
//!
//! // Target images are elevated during execution
//! let adjustment = profile.get_adjustment(&DataType::TargetImage);
//! assert!(adjustment.elevates());
//! ```

use super::classification::DataType;
use super::{QoSClass, QoSPolicy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Mission phase for context-aware priority adjustment
///
/// Different mission phases have different priority requirements.
/// For example, during execution phase, target imagery becomes
/// mission-critical, while during egress, health status is elevated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MissionContext {
    /// Moving to objective
    ///
    /// Priority focus: Enemy detection, threat awareness
    /// - Contact reports: unchanged (P1)
    /// - Track history: demoted (P4 → P5)
    /// - Bulk data: deprioritized
    Ingress,

    /// On objective, executing mission
    ///
    /// Priority focus: Intel products, target verification
    /// - Target images: elevated (P2 → P1)
    /// - Audio intercepts: elevated (P2 → P1)
    /// - Capability changes: elevated (P3 → P2)
    Execution,

    /// Returning from objective
    ///
    /// Priority focus: Platform health, safe return
    /// - Health updates: elevated (P3 → P2)
    /// - Capability changes: elevated (P3 → P2)
    Egress,

    /// Emergency situation
    ///
    /// Priority focus: All critical data, situational awareness
    /// - Health updates: significantly elevated (P3 → P1)
    /// - Contact reports: unchanged (P1)
    /// - Bulk data: unchanged (P5)
    Emergency,

    /// Waiting/holding position
    ///
    /// Priority focus: Normal operations, opportunistic sync
    /// - All priorities unchanged
    /// - Enable background bulk sync
    #[default]
    Standby,
}

impl MissionContext {
    /// Get all mission contexts
    pub fn all() -> &'static [MissionContext] {
        &[
            MissionContext::Ingress,
            MissionContext::Execution,
            MissionContext::Egress,
            MissionContext::Emergency,
            MissionContext::Standby,
        ]
    }

    /// Check if this context elevates any data types
    pub fn has_elevations(&self) -> bool {
        !matches!(self, Self::Standby)
    }

    /// Check if bulk sync should be opportunistically enabled
    pub fn enables_bulk_sync(&self) -> bool {
        matches!(self, Self::Standby)
    }

    /// Get the default context profile for this mission phase
    pub fn profile(&self) -> ContextProfile {
        match self {
            Self::Ingress => ContextProfile::ingress(),
            Self::Execution => ContextProfile::execution(),
            Self::Egress => ContextProfile::egress(),
            Self::Emergency => ContextProfile::emergency(),
            Self::Standby => ContextProfile::standby(),
        }
    }
}

impl fmt::Display for MissionContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ingress => write!(f, "Ingress"),
            Self::Execution => write!(f, "Execution"),
            Self::Egress => write!(f, "Egress"),
            Self::Emergency => write!(f, "Emergency"),
            Self::Standby => write!(f, "Standby"),
        }
    }
}

/// Priority adjustment for a data type within a context
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QoSClassAdjustment {
    /// Elevate priority by N levels (toward Critical)
    ///
    /// E.g., Elevate(1) changes P3 → P2, P2 → P1
    Elevate(u8),

    /// Demote priority by N levels (toward Bulk)
    ///
    /// E.g., Demote(1) changes P2 → P3, P3 → P4
    Demote(u8),

    /// Override to specific QoS class regardless of base
    Override(QoSClass),

    /// No change from base priority
    NoChange,
}

impl QoSClassAdjustment {
    /// Apply this adjustment to a base QoS class
    pub fn apply(&self, base: QoSClass) -> QoSClass {
        match self {
            Self::Elevate(n) => {
                let base_val = base.as_u8();
                let new_val = base_val.saturating_sub(*n).max(1);
                match new_val {
                    1 => QoSClass::Critical,
                    2 => QoSClass::High,
                    3 => QoSClass::Normal,
                    4 => QoSClass::Low,
                    _ => QoSClass::Bulk,
                }
            }
            Self::Demote(n) => {
                let base_val = base.as_u8();
                let new_val = base_val.saturating_add(*n).min(5);
                match new_val {
                    1 => QoSClass::Critical,
                    2 => QoSClass::High,
                    3 => QoSClass::Normal,
                    4 => QoSClass::Low,
                    _ => QoSClass::Bulk,
                }
            }
            Self::Override(class) => *class,
            Self::NoChange => base,
        }
    }

    /// Check if this adjustment elevates priority
    pub fn elevates(&self) -> bool {
        matches!(
            self,
            Self::Elevate(_) | Self::Override(QoSClass::Critical | QoSClass::High)
        )
    }

    /// Check if this adjustment demotes priority
    pub fn demotes(&self) -> bool {
        matches!(
            self,
            Self::Demote(_) | Self::Override(QoSClass::Low | QoSClass::Bulk)
        )
    }
}

impl Default for QoSClassAdjustment {
    fn default() -> Self {
        Self::NoChange
    }
}

/// Priority adjustments for a mission context
///
/// A context profile defines how priorities should be adjusted
/// for each data type during a specific mission phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextProfile {
    /// The mission context this profile applies to
    pub context: MissionContext,

    /// Per-data-type priority adjustments
    pub adjustments: HashMap<DataType, QoSClassAdjustment>,

    /// Optional description of the profile
    pub description: Option<String>,
}

impl ContextProfile {
    /// Create a new empty context profile
    pub fn new(context: MissionContext) -> Self {
        Self {
            context,
            adjustments: HashMap::new(),
            description: None,
        }
    }

    /// Create a new context profile with a description
    pub fn with_description(context: MissionContext, description: impl Into<String>) -> Self {
        Self {
            context,
            adjustments: HashMap::new(),
            description: Some(description.into()),
        }
    }

    /// Default profile for Ingress phase
    ///
    /// During ingress (moving to objective):
    /// - Contact reports: P1 → P1 (unchanged, already critical)
    /// - Track history: P4 → P5 (deprioritized)
    /// - Images/video: P2 → P2 (unchanged)
    pub fn ingress() -> Self {
        let mut profile = Self::with_description(
            MissionContext::Ingress,
            "Moving to objective - prioritize enemy detection",
        );

        // Demote track history during ingress
        profile
            .adjustments
            .insert(DataType::HistoricalTrack, QoSClassAdjustment::Demote(1));

        // Demote position updates slightly
        profile
            .adjustments
            .insert(DataType::PositionUpdate, QoSClassAdjustment::Demote(1));

        profile
    }

    /// Default profile for Execution phase
    ///
    /// During execution (on objective):
    /// - Images/video: P2 → P1 (elevated - target verification critical)
    /// - Audio intercepts: P2 → P1 (elevated)
    /// - Capability changes: P3 → P2 (elevated)
    pub fn execution() -> Self {
        let mut profile = Self::with_description(
            MissionContext::Execution,
            "On objective - prioritize intel products",
        );

        // Elevate target imagery to critical
        profile
            .adjustments
            .insert(DataType::TargetImage, QoSClassAdjustment::Elevate(1));

        // Elevate audio intercepts to critical
        profile
            .adjustments
            .insert(DataType::AudioIntercept, QoSClassAdjustment::Elevate(1));

        // Elevate capability changes
        profile
            .adjustments
            .insert(DataType::CapabilityChange, QoSClassAdjustment::Elevate(1));

        // Elevate formation changes
        profile
            .adjustments
            .insert(DataType::FormationChange, QoSClassAdjustment::Elevate(1));

        profile
    }

    /// Default profile for Egress phase
    ///
    /// During egress (returning from objective):
    /// - Health updates: P3 → P2 (elevated)
    /// - Capability changes: P3 → P2 (elevated)
    /// - Track history: P4 → P4 (unchanged)
    pub fn egress() -> Self {
        let mut profile = Self::with_description(
            MissionContext::Egress,
            "Returning from objective - prioritize health/status",
        );

        // Elevate health status
        profile
            .adjustments
            .insert(DataType::HealthStatus, QoSClassAdjustment::Elevate(1));

        // Elevate capability changes
        profile
            .adjustments
            .insert(DataType::CapabilityChange, QoSClassAdjustment::Elevate(1));

        // Elevate formation updates
        profile
            .adjustments
            .insert(DataType::FormationUpdate, QoSClassAdjustment::Elevate(1));

        profile
    }

    /// Default profile for Emergency
    ///
    /// During emergency:
    /// - Contact reports: P1 → P1 (unchanged)
    /// - Health updates: P3 → P1 (significantly elevated)
    /// - Capability changes: P3 → P1 (significantly elevated)
    /// - Model updates: P5 → P5 (unchanged, still bulk)
    pub fn emergency() -> Self {
        let mut profile = Self::with_description(
            MissionContext::Emergency,
            "Emergency - elevate all status data to critical",
        );

        // Elevate health status to critical
        profile
            .adjustments
            .insert(DataType::HealthStatus, QoSClassAdjustment::Elevate(2));

        // Elevate capability changes to critical
        profile
            .adjustments
            .insert(DataType::CapabilityChange, QoSClassAdjustment::Elevate(2));

        // Elevate emergency alerts (already P1, but ensure)
        profile.adjustments.insert(
            DataType::EmergencyAlert,
            QoSClassAdjustment::Override(QoSClass::Critical),
        );

        // Elevate position updates - need to know where everyone is
        profile
            .adjustments
            .insert(DataType::PositionUpdate, QoSClassAdjustment::Elevate(2));

        // Demote bulk data during emergency
        profile.adjustments.insert(
            DataType::TrainingData,
            QoSClassAdjustment::Override(QoSClass::Bulk),
        );

        profile
    }

    /// Default profile for Standby
    ///
    /// During standby:
    /// - All priorities unchanged
    /// - Enable opportunistic bulk sync
    pub fn standby() -> Self {
        Self::with_description(
            MissionContext::Standby,
            "Standby - normal priorities, opportunistic sync",
        )
    }

    /// Get the adjustment for a specific data type
    pub fn get_adjustment(&self, data_type: &DataType) -> QoSClassAdjustment {
        self.adjustments
            .get(data_type)
            .copied()
            .unwrap_or(QoSClassAdjustment::NoChange)
    }

    /// Set an adjustment for a data type
    pub fn set_adjustment(&mut self, data_type: DataType, adjustment: QoSClassAdjustment) {
        self.adjustments.insert(data_type, adjustment);
    }

    /// Apply this profile to a base QoS policy
    pub fn apply_to_policy(&self, base: &QoSPolicy, data_type: &DataType) -> QoSPolicy {
        let adjustment = self.get_adjustment(data_type);
        let adjusted_class = adjustment.apply(base.base_class);

        // Create adjusted policy
        let mut adjusted = base.clone();
        adjusted.base_class = adjusted_class;

        // Adjust latency based on priority change
        // Note: QoSClass Ord puts Critical > High > Normal > Low > Bulk
        // So "priority increased" means adjusted_class > base.base_class
        if adjusted_class > base.base_class {
            // Priority increased - tighten latency
            if let Some(latency) = adjusted.max_latency_ms {
                adjusted.max_latency_ms = Some(latency / 2);
            }
        } else if adjusted_class < base.base_class {
            // Priority decreased - relax latency
            if let Some(latency) = adjusted.max_latency_ms {
                adjusted.max_latency_ms = Some(latency * 2);
            }
        }

        adjusted
    }

    /// Get all data types with non-default adjustments
    pub fn adjusted_types(&self) -> impl Iterator<Item = (&DataType, &QoSClassAdjustment)> {
        self.adjustments
            .iter()
            .filter(|(_, adj)| !matches!(adj, QoSClassAdjustment::NoChange))
    }

    /// Count the number of adjustments
    pub fn adjustment_count(&self) -> usize {
        self.adjustments
            .values()
            .filter(|adj| !matches!(adj, QoSClassAdjustment::NoChange))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mission_context_display() {
        assert_eq!(MissionContext::Ingress.to_string(), "Ingress");
        assert_eq!(MissionContext::Execution.to_string(), "Execution");
        assert_eq!(MissionContext::Egress.to_string(), "Egress");
        assert_eq!(MissionContext::Emergency.to_string(), "Emergency");
        assert_eq!(MissionContext::Standby.to_string(), "Standby");
    }

    #[test]
    fn test_mission_context_default() {
        assert_eq!(MissionContext::default(), MissionContext::Standby);
    }

    #[test]
    fn test_adjustment_elevate() {
        let adj = QoSClassAdjustment::Elevate(1);

        // P3 → P2
        assert_eq!(adj.apply(QoSClass::Normal), QoSClass::High);
        // P2 → P1
        assert_eq!(adj.apply(QoSClass::High), QoSClass::Critical);
        // P1 → P1 (can't go higher)
        assert_eq!(adj.apply(QoSClass::Critical), QoSClass::Critical);
        // P5 → P4
        assert_eq!(adj.apply(QoSClass::Bulk), QoSClass::Low);
    }

    #[test]
    fn test_adjustment_demote() {
        let adj = QoSClassAdjustment::Demote(1);

        // P3 → P4
        assert_eq!(adj.apply(QoSClass::Normal), QoSClass::Low);
        // P4 → P5
        assert_eq!(adj.apply(QoSClass::Low), QoSClass::Bulk);
        // P5 → P5 (can't go lower)
        assert_eq!(adj.apply(QoSClass::Bulk), QoSClass::Bulk);
        // P1 → P2
        assert_eq!(adj.apply(QoSClass::Critical), QoSClass::High);
    }

    #[test]
    fn test_adjustment_override() {
        let adj = QoSClassAdjustment::Override(QoSClass::Critical);

        // All become Critical
        assert_eq!(adj.apply(QoSClass::Normal), QoSClass::Critical);
        assert_eq!(adj.apply(QoSClass::Bulk), QoSClass::Critical);
        assert_eq!(adj.apply(QoSClass::Critical), QoSClass::Critical);
    }

    #[test]
    fn test_adjustment_no_change() {
        let adj = QoSClassAdjustment::NoChange;

        assert_eq!(adj.apply(QoSClass::Normal), QoSClass::Normal);
        assert_eq!(adj.apply(QoSClass::Bulk), QoSClass::Bulk);
    }

    #[test]
    fn test_adjustment_elevates_demotes() {
        assert!(QoSClassAdjustment::Elevate(1).elevates());
        assert!(!QoSClassAdjustment::Elevate(1).demotes());

        assert!(QoSClassAdjustment::Demote(1).demotes());
        assert!(!QoSClassAdjustment::Demote(1).elevates());

        assert!(!QoSClassAdjustment::NoChange.elevates());
        assert!(!QoSClassAdjustment::NoChange.demotes());
    }

    #[test]
    fn test_ingress_profile() {
        let profile = ContextProfile::ingress();

        assert_eq!(profile.context, MissionContext::Ingress);

        // Historical track should be demoted
        let adj = profile.get_adjustment(&DataType::HistoricalTrack);
        assert!(adj.demotes());

        // Contact report should be unchanged
        let adj = profile.get_adjustment(&DataType::ContactReport);
        assert_eq!(adj, QoSClassAdjustment::NoChange);
    }

    #[test]
    fn test_execution_profile() {
        let profile = ContextProfile::execution();

        assert_eq!(profile.context, MissionContext::Execution);

        // Target image should be elevated
        let adj = profile.get_adjustment(&DataType::TargetImage);
        assert!(adj.elevates());
        assert_eq!(adj.apply(QoSClass::High), QoSClass::Critical);

        // Audio intercept should be elevated
        let adj = profile.get_adjustment(&DataType::AudioIntercept);
        assert!(adj.elevates());
    }

    #[test]
    fn test_egress_profile() {
        let profile = ContextProfile::egress();

        assert_eq!(profile.context, MissionContext::Egress);

        // Health status should be elevated
        let adj = profile.get_adjustment(&DataType::HealthStatus);
        assert!(adj.elevates());
        assert_eq!(adj.apply(QoSClass::Normal), QoSClass::High);
    }

    #[test]
    fn test_emergency_profile() {
        let profile = ContextProfile::emergency();

        assert_eq!(profile.context, MissionContext::Emergency);

        // Health status should be significantly elevated
        let adj = profile.get_adjustment(&DataType::HealthStatus);
        assert!(adj.elevates());
        assert_eq!(adj.apply(QoSClass::Normal), QoSClass::Critical);

        // Position updates elevated
        let adj = profile.get_adjustment(&DataType::PositionUpdate);
        assert!(adj.elevates());
    }

    #[test]
    fn test_standby_profile() {
        let profile = ContextProfile::standby();

        assert_eq!(profile.context, MissionContext::Standby);

        // No adjustments in standby
        assert_eq!(profile.adjustment_count(), 0);
    }

    #[test]
    fn test_apply_to_policy() {
        let profile = ContextProfile::execution();
        let base_policy = QoSPolicy::high();

        let adjusted = profile.apply_to_policy(&base_policy, &DataType::TargetImage);

        // Should be elevated to critical
        assert_eq!(adjusted.base_class, QoSClass::Critical);
        // Latency should be tighter
        assert!(adjusted.max_latency_ms < base_policy.max_latency_ms);
    }

    #[test]
    fn test_mission_context_serialization() {
        let context = MissionContext::Execution;
        let json = serde_json::to_string(&context).unwrap();
        assert_eq!(json, "\"Execution\"");

        let deserialized: MissionContext = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, MissionContext::Execution);
    }

    #[test]
    fn test_profile_adjustment_count() {
        let profile = ContextProfile::emergency();
        assert!(profile.adjustment_count() > 0);

        let standby = ContextProfile::standby();
        assert_eq!(standby.adjustment_count(), 0);
    }

    #[test]
    fn test_enables_bulk_sync() {
        assert!(MissionContext::Standby.enables_bulk_sync());
        assert!(!MissionContext::Emergency.enables_bulk_sync());
        assert!(!MissionContext::Execution.enables_bulk_sync());
    }

    #[test]
    fn test_has_elevations() {
        assert!(!MissionContext::Standby.has_elevations());
        assert!(MissionContext::Emergency.has_elevations());
        assert!(MissionContext::Execution.has_elevations());
    }
}
