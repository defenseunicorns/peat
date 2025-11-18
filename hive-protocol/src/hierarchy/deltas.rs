//! Delta-based document updates for hierarchical aggregation
//!
//! This module implements field-level delta updates to replace full document
//! recreation, addressing the 20× bandwidth amplification issue identified in
//! ADR-021 E12 validation.
//!
//! # Core Principle
//!
//! Documents are created ONCE, then updated via deltas containing only changed
//! fields. This enables:
//! - CRDT delta propagation (not full document replication)
//! - 10-20× bandwidth reduction
//! - Proper document lifecycle (create-once, update-many pattern)

use hive_schema::capability::v1::Capability;
use hive_schema::common::v1::{Position, Timestamp};
use hive_schema::hierarchy::v1::BoundingBox;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

/// Field-level delta for SquadSummary documents
///
/// Represents incremental changes to a squad summary, enabling CRDT-based
/// delta synchronization instead of full document recreation.
///
/// # Example
///
/// ```rust,no_run
/// use hive_protocol::hierarchy::deltas::*;
///
/// let delta = SquadDelta {
///     squad_id: "squad-1A".to_string(),
///     timestamp_us: current_timestamp_us(),
///     sequence: 42,
///     updates: vec![
///         SquadFieldUpdate::SetMemberCount(7),
///         SquadFieldUpdate::SetOperationalCount(6),
///         SquadFieldUpdate::AddMemberId("node-8".to_string()),
///     ],
/// };
///
/// // Delta is ~100 bytes vs ~2KB for full SquadSummary
/// assert!(delta.size_bytes() < 200);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadDelta {
    /// Squad identifier
    pub squad_id: String,

    /// Timestamp when delta was generated (microseconds since epoch)
    pub timestamp_us: u64,

    /// Monotonic sequence number for ordering
    pub sequence: u64,

    /// Field-level updates
    pub updates: Vec<SquadFieldUpdate>,
}

/// Individual field update for SquadSummary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SquadFieldUpdate {
    // Scalar fields (LWW - Last Write Wins semantics)
    /// Update squad leader ID
    SetLeaderId(String),

    /// Update total member count
    SetMemberCount(u32),

    /// Update operational member count (health >= DEGRADED)
    SetOperationalCount(u32),

    /// Update average fuel across squad members (minutes)
    SetAvgFuelMinutes(f32),

    /// Update worst health status in squad
    SetWorstHealth(i32),

    /// Update squad readiness score (0.0-1.0)
    SetReadinessScore(f32),

    // Position update (LWW for centroid)
    /// Update position centroid
    UpdatePositionCentroid(Position),

    // Array operations (OR-Set semantics - Add-Wins)
    /// Add member to squad
    AddMemberId(String),

    /// Remove member from squad
    RemoveMemberId(String),

    // Capability composition (additive)
    /// Add aggregated capability
    AddCapability(Capability),

    /// Remove capability by ID
    RemoveCapability(String),

    // Spatial updates
    /// Update bounding box
    UpdateBoundingBox(BoundingBox),

    /// Update aggregation timestamp
    UpdateAggregatedAt(Timestamp),
}

impl SquadDelta {
    /// Convert delta to Ditto field update operations
    ///
    /// Maps delta field updates to JSON field paths and values for Ditto's
    /// CRDT update operations.
    ///
    /// # Returns
    ///
    /// Vector of (field_path, value) tuples for Ditto update operations
    pub fn into_ditto_updates(self) -> Vec<(String, serde_json::Value)> {
        let mut updates = Vec::new();

        for update in self.updates {
            match update {
                SquadFieldUpdate::SetLeaderId(id) => {
                    updates.push(("leader_id".to_string(), json!(id)));
                }
                SquadFieldUpdate::SetMemberCount(count) => {
                    updates.push(("member_count".to_string(), json!(count)));
                }
                SquadFieldUpdate::SetOperationalCount(count) => {
                    updates.push(("operational_count".to_string(), json!(count)));
                }
                SquadFieldUpdate::SetAvgFuelMinutes(fuel) => {
                    updates.push(("avg_fuel_minutes".to_string(), json!(fuel)));
                }
                SquadFieldUpdate::SetWorstHealth(health) => {
                    updates.push(("worst_health".to_string(), json!(health)));
                }
                SquadFieldUpdate::SetReadinessScore(score) => {
                    updates.push(("readiness_score".to_string(), json!(score)));
                }
                SquadFieldUpdate::UpdatePositionCentroid(pos) => {
                    updates.push((
                        "position_centroid".to_string(),
                        serde_json::to_value(pos).unwrap_or(json!(null)),
                    ));
                }
                SquadFieldUpdate::AddMemberId(id) => {
                    // OR-Set: add to array
                    updates.push(("member_ids.$add".to_string(), json!(id)));
                }
                SquadFieldUpdate::RemoveMemberId(id) => {
                    // OR-Set: remove from array
                    updates.push(("member_ids.$remove".to_string(), json!(id)));
                }
                SquadFieldUpdate::AddCapability(cap) => {
                    updates.push((
                        "aggregated_capabilities.$add".to_string(),
                        serde_json::to_value(cap).unwrap_or(json!(null)),
                    ));
                }
                SquadFieldUpdate::RemoveCapability(cap_id) => {
                    updates.push(("aggregated_capabilities.$remove".to_string(), json!(cap_id)));
                }
                SquadFieldUpdate::UpdateBoundingBox(bbox) => {
                    updates.push((
                        "bounding_box".to_string(),
                        serde_json::to_value(bbox).unwrap_or(json!(null)),
                    ));
                }
                SquadFieldUpdate::UpdateAggregatedAt(ts) => {
                    updates.push((
                        "aggregated_at".to_string(),
                        serde_json::to_value(ts).unwrap_or(json!(null)),
                    ));
                }
            }
        }

        // Add metadata updates
        updates.push(("last_update_us".to_string(), json!(self.timestamp_us)));
        updates.push(("sequence".to_string(), json!(self.sequence)));

        updates
    }

    /// Estimate size of delta in bytes
    ///
    /// Used for bandwidth metrics and efficiency validation.
    /// Target: delta should be <5% of full SquadSummary size (~2KB).
    pub fn size_bytes(&self) -> usize {
        // Rough estimate based on field updates
        let base_overhead = 64; // squad_id, timestamp_us, sequence
        let per_update_overhead = 16; // field name + metadata

        let updates_size: usize = self
            .updates
            .iter()
            .map(|u| match u {
                SquadFieldUpdate::SetLeaderId(s) => s.len() + per_update_overhead,
                SquadFieldUpdate::SetMemberCount(_) => 4 + per_update_overhead,
                SquadFieldUpdate::SetOperationalCount(_) => 4 + per_update_overhead,
                SquadFieldUpdate::SetAvgFuelMinutes(_) => 4 + per_update_overhead,
                SquadFieldUpdate::SetWorstHealth(_) => 4 + per_update_overhead,
                SquadFieldUpdate::SetReadinessScore(_) => 4 + per_update_overhead,
                SquadFieldUpdate::UpdatePositionCentroid(_) => 24 + per_update_overhead, // 3 floats
                SquadFieldUpdate::AddMemberId(s) => s.len() + per_update_overhead,
                SquadFieldUpdate::RemoveMemberId(s) => s.len() + per_update_overhead,
                SquadFieldUpdate::AddCapability(_) => 128 + per_update_overhead, // capability ~128 bytes
                SquadFieldUpdate::RemoveCapability(s) => s.len() + per_update_overhead,
                SquadFieldUpdate::UpdateBoundingBox(_) => 64 + per_update_overhead,
                SquadFieldUpdate::UpdateAggregatedAt(_) => 16 + per_update_overhead,
            })
            .sum();

        base_overhead + updates_size
    }

    /// Check if delta is empty (no updates)
    pub fn is_empty(&self) -> bool {
        self.updates.is_empty()
    }
}

/// Field-level delta for PlatoonSummary documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatoonDelta {
    /// Platoon identifier
    pub platoon_id: String,

    /// Timestamp when delta was generated (microseconds since epoch)
    pub timestamp_us: u64,

    /// Monotonic sequence number for ordering
    pub sequence: u64,

    /// Field-level updates
    pub updates: Vec<PlatoonFieldUpdate>,
}

/// Individual field update for PlatoonSummary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlatoonFieldUpdate {
    // Scalar fields (LWW semantics)
    SetLeaderId(String),
    SetSquadCount(u32),
    SetTotalMemberCount(u32),
    SetOperationalCount(u32),
    SetAvgFuelMinutes(f32),
    SetWorstHealth(i32),
    SetReadinessScore(f32),

    // Position update
    UpdatePositionCentroid(Position),

    // Array operations (OR-Set)
    AddSquadId(String),
    RemoveSquadId(String),

    // Capabilities
    AddCapability(Capability),
    RemoveCapability(String),

    // Spatial
    UpdateBoundingBox(BoundingBox),
    UpdateAggregatedAt(Timestamp),
}

impl PlatoonDelta {
    /// Convert delta to Ditto field update operations
    pub fn into_ditto_updates(self) -> Vec<(String, serde_json::Value)> {
        let mut updates = Vec::new();

        for update in self.updates {
            match update {
                PlatoonFieldUpdate::SetLeaderId(id) => {
                    updates.push(("leader_id".to_string(), json!(id)));
                }
                PlatoonFieldUpdate::SetSquadCount(count) => {
                    updates.push(("squad_count".to_string(), json!(count)));
                }
                PlatoonFieldUpdate::SetTotalMemberCount(count) => {
                    updates.push(("total_member_count".to_string(), json!(count)));
                }
                PlatoonFieldUpdate::SetOperationalCount(count) => {
                    updates.push(("operational_count".to_string(), json!(count)));
                }
                PlatoonFieldUpdate::SetAvgFuelMinutes(fuel) => {
                    updates.push(("avg_fuel_minutes".to_string(), json!(fuel)));
                }
                PlatoonFieldUpdate::SetWorstHealth(health) => {
                    updates.push(("worst_health".to_string(), json!(health)));
                }
                PlatoonFieldUpdate::SetReadinessScore(score) => {
                    updates.push(("readiness_score".to_string(), json!(score)));
                }
                PlatoonFieldUpdate::UpdatePositionCentroid(pos) => {
                    updates.push((
                        "position_centroid".to_string(),
                        serde_json::to_value(pos).unwrap_or(json!(null)),
                    ));
                }
                PlatoonFieldUpdate::AddSquadId(id) => {
                    updates.push(("squad_ids.$add".to_string(), json!(id)));
                }
                PlatoonFieldUpdate::RemoveSquadId(id) => {
                    updates.push(("squad_ids.$remove".to_string(), json!(id)));
                }
                PlatoonFieldUpdate::AddCapability(cap) => {
                    updates.push((
                        "aggregated_capabilities.$add".to_string(),
                        serde_json::to_value(cap).unwrap_or(json!(null)),
                    ));
                }
                PlatoonFieldUpdate::RemoveCapability(cap_id) => {
                    updates.push(("aggregated_capabilities.$remove".to_string(), json!(cap_id)));
                }
                PlatoonFieldUpdate::UpdateBoundingBox(bbox) => {
                    updates.push((
                        "bounding_box".to_string(),
                        serde_json::to_value(bbox).unwrap_or(json!(null)),
                    ));
                }
                PlatoonFieldUpdate::UpdateAggregatedAt(ts) => {
                    updates.push((
                        "aggregated_at".to_string(),
                        serde_json::to_value(ts).unwrap_or(json!(null)),
                    ));
                }
            }
        }

        updates.push(("last_update_us".to_string(), json!(self.timestamp_us)));
        updates.push(("sequence".to_string(), json!(self.sequence)));

        updates
    }

    /// Estimate size of delta in bytes
    pub fn size_bytes(&self) -> usize {
        let base_overhead = 64;
        let per_update_overhead = 16;

        let updates_size: usize = self
            .updates
            .iter()
            .map(|u| match u {
                PlatoonFieldUpdate::SetLeaderId(s) => s.len() + per_update_overhead,
                PlatoonFieldUpdate::SetSquadCount(_) => 4 + per_update_overhead,
                PlatoonFieldUpdate::SetTotalMemberCount(_) => 4 + per_update_overhead,
                PlatoonFieldUpdate::SetOperationalCount(_) => 4 + per_update_overhead,
                PlatoonFieldUpdate::SetAvgFuelMinutes(_) => 4 + per_update_overhead,
                PlatoonFieldUpdate::SetWorstHealth(_) => 4 + per_update_overhead,
                PlatoonFieldUpdate::SetReadinessScore(_) => 4 + per_update_overhead,
                PlatoonFieldUpdate::UpdatePositionCentroid(_) => 24 + per_update_overhead,
                PlatoonFieldUpdate::AddSquadId(s) => s.len() + per_update_overhead,
                PlatoonFieldUpdate::RemoveSquadId(s) => s.len() + per_update_overhead,
                PlatoonFieldUpdate::AddCapability(_) => 128 + per_update_overhead,
                PlatoonFieldUpdate::RemoveCapability(s) => s.len() + per_update_overhead,
                PlatoonFieldUpdate::UpdateBoundingBox(_) => 64 + per_update_overhead,
                PlatoonFieldUpdate::UpdateAggregatedAt(_) => 16 + per_update_overhead,
            })
            .sum();

        base_overhead + updates_size
    }

    /// Check if delta is empty
    pub fn is_empty(&self) -> bool {
        self.updates.is_empty()
    }
}

/// Field-level delta for CompanySummary documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyDelta {
    /// Company identifier
    pub company_id: String,

    /// Timestamp when delta was generated (microseconds since epoch)
    pub timestamp_us: u64,

    /// Monotonic sequence number for ordering
    pub sequence: u64,

    /// Field-level updates
    pub updates: Vec<CompanyFieldUpdate>,
}

/// Individual field update for CompanySummary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompanyFieldUpdate {
    // Scalar fields (LWW semantics)
    SetLeaderId(String),
    SetPlatoonCount(u32),
    SetTotalMemberCount(u32),
    SetOperationalCount(u32),
    SetAvgFuelMinutes(f32),
    SetWorstHealth(i32),
    SetReadinessScore(f32),

    // Position update
    UpdatePositionCentroid(Position),

    // Array operations (OR-Set)
    AddPlatoonId(String),
    RemovePlatoonId(String),

    // Capabilities
    AddCapability(Capability),
    RemoveCapability(String),

    // Spatial
    UpdateBoundingBox(BoundingBox),
    UpdateAggregatedAt(Timestamp),
}

impl CompanyDelta {
    /// Convert delta to Ditto field update operations
    pub fn into_ditto_updates(self) -> Vec<(String, serde_json::Value)> {
        let mut updates = Vec::new();

        for update in self.updates {
            match update {
                CompanyFieldUpdate::SetLeaderId(id) => {
                    updates.push(("leader_id".to_string(), json!(id)));
                }
                CompanyFieldUpdate::SetPlatoonCount(count) => {
                    updates.push(("platoon_count".to_string(), json!(count)));
                }
                CompanyFieldUpdate::SetTotalMemberCount(count) => {
                    updates.push(("total_member_count".to_string(), json!(count)));
                }
                CompanyFieldUpdate::SetOperationalCount(count) => {
                    updates.push(("operational_count".to_string(), json!(count)));
                }
                CompanyFieldUpdate::SetAvgFuelMinutes(fuel) => {
                    updates.push(("avg_fuel_minutes".to_string(), json!(fuel)));
                }
                CompanyFieldUpdate::SetWorstHealth(health) => {
                    updates.push(("worst_health".to_string(), json!(health)));
                }
                CompanyFieldUpdate::SetReadinessScore(score) => {
                    updates.push(("readiness_score".to_string(), json!(score)));
                }
                CompanyFieldUpdate::UpdatePositionCentroid(pos) => {
                    updates.push((
                        "position_centroid".to_string(),
                        serde_json::to_value(pos).unwrap_or(json!(null)),
                    ));
                }
                CompanyFieldUpdate::AddPlatoonId(id) => {
                    updates.push(("platoon_ids.$add".to_string(), json!(id)));
                }
                CompanyFieldUpdate::RemovePlatoonId(id) => {
                    updates.push(("platoon_ids.$remove".to_string(), json!(id)));
                }
                CompanyFieldUpdate::AddCapability(cap) => {
                    updates.push((
                        "aggregated_capabilities.$add".to_string(),
                        serde_json::to_value(cap).unwrap_or(json!(null)),
                    ));
                }
                CompanyFieldUpdate::RemoveCapability(cap_id) => {
                    updates.push(("aggregated_capabilities.$remove".to_string(), json!(cap_id)));
                }
                CompanyFieldUpdate::UpdateBoundingBox(bbox) => {
                    updates.push((
                        "bounding_box".to_string(),
                        serde_json::to_value(bbox).unwrap_or(json!(null)),
                    ));
                }
                CompanyFieldUpdate::UpdateAggregatedAt(ts) => {
                    updates.push((
                        "aggregated_at".to_string(),
                        serde_json::to_value(ts).unwrap_or(json!(null)),
                    ));
                }
            }
        }

        updates.push(("last_update_us".to_string(), json!(self.timestamp_us)));
        updates.push(("sequence".to_string(), json!(self.sequence)));

        updates
    }

    /// Estimate size of delta in bytes
    pub fn size_bytes(&self) -> usize {
        let base_overhead = 64;
        let per_update_overhead = 16;

        let updates_size: usize = self
            .updates
            .iter()
            .map(|u| match u {
                CompanyFieldUpdate::SetLeaderId(s) => s.len() + per_update_overhead,
                CompanyFieldUpdate::SetPlatoonCount(_) => 4 + per_update_overhead,
                CompanyFieldUpdate::SetTotalMemberCount(_) => 4 + per_update_overhead,
                CompanyFieldUpdate::SetOperationalCount(_) => 4 + per_update_overhead,
                CompanyFieldUpdate::SetAvgFuelMinutes(_) => 4 + per_update_overhead,
                CompanyFieldUpdate::SetWorstHealth(_) => 4 + per_update_overhead,
                CompanyFieldUpdate::SetReadinessScore(_) => 4 + per_update_overhead,
                CompanyFieldUpdate::UpdatePositionCentroid(_) => 24 + per_update_overhead,
                CompanyFieldUpdate::AddPlatoonId(s) => s.len() + per_update_overhead,
                CompanyFieldUpdate::RemovePlatoonId(s) => s.len() + per_update_overhead,
                CompanyFieldUpdate::AddCapability(_) => 128 + per_update_overhead,
                CompanyFieldUpdate::RemoveCapability(s) => s.len() + per_update_overhead,
                CompanyFieldUpdate::UpdateBoundingBox(_) => 64 + per_update_overhead,
                CompanyFieldUpdate::UpdateAggregatedAt(_) => 16 + per_update_overhead,
            })
            .sum();

        base_overhead + updates_size
    }

    /// Check if delta is empty
    pub fn is_empty(&self) -> bool {
        self.updates.is_empty()
    }
}

/// Get current timestamp in microseconds since Unix epoch
pub fn current_timestamp_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_squad_delta_serialization() {
        let delta = SquadDelta {
            squad_id: "squad-1A".to_string(),
            timestamp_us: 1234567890,
            sequence: 42,
            updates: vec![
                SquadFieldUpdate::SetMemberCount(7),
                SquadFieldUpdate::SetOperationalCount(6),
                SquadFieldUpdate::AddMemberId("node-8".to_string()),
            ],
        };

        // Should serialize to JSON
        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("squad-1A"));
        assert!(json.contains("SetMemberCount"));

        // Should deserialize back
        let deserialized: SquadDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.squad_id, "squad-1A");
        assert_eq!(deserialized.updates.len(), 3);
    }

    #[test]
    fn test_squad_delta_into_ditto_updates() {
        let delta = SquadDelta {
            squad_id: "squad-1A".to_string(),
            timestamp_us: 1234567890,
            sequence: 42,
            updates: vec![
                SquadFieldUpdate::SetLeaderId("leader-1".to_string()),
                SquadFieldUpdate::SetMemberCount(8),
                SquadFieldUpdate::AddMemberId("node-9".to_string()),
            ],
        };

        let ditto_updates = delta.into_ditto_updates();

        // Should have field updates + metadata
        assert!(ditto_updates.len() >= 3);

        // Check specific updates
        let leader_update = ditto_updates
            .iter()
            .find(|(path, _)| path == "leader_id")
            .unwrap();
        assert_eq!(leader_update.1, json!("leader-1"));

        let member_count_update = ditto_updates
            .iter()
            .find(|(path, _)| path == "member_count")
            .unwrap();
        assert_eq!(member_count_update.1, json!(8));

        let add_member_update = ditto_updates
            .iter()
            .find(|(path, _)| path == "member_ids.$add")
            .unwrap();
        assert_eq!(add_member_update.1, json!("node-9"));
    }

    #[test]
    fn test_delta_size_estimation() {
        let small_delta = SquadDelta {
            squad_id: "squad-1A".to_string(),
            timestamp_us: 1234567890,
            sequence: 1,
            updates: vec![SquadFieldUpdate::SetMemberCount(7)],
        };

        let large_delta = SquadDelta {
            squad_id: "squad-1A".to_string(),
            timestamp_us: 1234567890,
            sequence: 1,
            updates: vec![
                SquadFieldUpdate::SetMemberCount(7),
                SquadFieldUpdate::SetOperationalCount(6),
                SquadFieldUpdate::AddMemberId("node-123456789".to_string()),
                SquadFieldUpdate::UpdatePositionCentroid(Position {
                    latitude: 37.7749,
                    longitude: -122.4194,
                    altitude: 100.0,
                }),
            ],
        };

        // Small delta should be ~100 bytes
        assert!(small_delta.size_bytes() < 150);

        // Large delta should still be much smaller than full SquadSummary (~2KB)
        assert!(large_delta.size_bytes() < 500);
        assert!(large_delta.size_bytes() > small_delta.size_bytes());
    }

    #[test]
    fn test_empty_delta() {
        let delta = SquadDelta {
            squad_id: "squad-1A".to_string(),
            timestamp_us: 1234567890,
            sequence: 1,
            updates: vec![],
        };

        assert!(delta.is_empty());
        assert_eq!(delta.updates.len(), 0);
    }

    #[test]
    fn test_platoon_delta_basic() {
        let delta = PlatoonDelta {
            platoon_id: "platoon-1".to_string(),
            timestamp_us: 1234567890,
            sequence: 10,
            updates: vec![
                PlatoonFieldUpdate::SetSquadCount(3),
                PlatoonFieldUpdate::AddSquadId("squad-1A".to_string()),
            ],
        };

        let ditto_updates = delta.into_ditto_updates();
        assert!(ditto_updates.len() >= 2);

        let squad_count = ditto_updates
            .iter()
            .find(|(path, _)| path == "squad_count")
            .unwrap();
        assert_eq!(squad_count.1, json!(3));
    }

    #[test]
    fn test_company_delta_basic() {
        let delta = CompanyDelta {
            company_id: "company-alpha".to_string(),
            timestamp_us: 1234567890,
            sequence: 5,
            updates: vec![
                CompanyFieldUpdate::SetPlatoonCount(4),
                CompanyFieldUpdate::SetTotalMemberCount(96),
            ],
        };

        assert!(!delta.is_empty());
        assert_eq!(delta.updates.len(), 2);
    }

    #[test]
    fn test_current_timestamp_us() {
        let ts1 = current_timestamp_us();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let ts2 = current_timestamp_us();

        // Timestamp should increase
        assert!(ts2 > ts1);

        // Should be reasonable microseconds since epoch (after 2020)
        assert!(ts1 > 1_600_000_000_000_000);
    }
}
