//! Hierarchical State Aggregation (E12.1)
//!
//! This module implements hierarchical state aggregation for multi-tier military formations
//! (squad → platoon → company), enabling O(log n) scaling instead of O(n²) full replication.
//!
//! ## Design Rationale
//!
//! **Problem**: In 24-node scenarios, full state replication requires 576 sync operations
//! (24 nodes × 24 states). At 96 nodes, this becomes 9,216 operations (O(n²) scaling).
//!
//! **Solution**: Hierarchical aggregation reduces this to O(n log n):
//! - 24 nodes publish NodeState to squad leaders (24 ops)
//! - 3 squad leaders publish SquadSummary to platoon leader (3 ops)
//! - Total: 27 ops vs 576 ops (95% reduction)
//!
//! ## Integration with CapabilityAggregator
//!
//! This module handles **state aggregation** (position, fuel, health) and delegates
//! **capability aggregation** to the existing `CapabilityAggregator` in
//! `cap-protocol/src/cell/capability_aggregation.rs`.
//!
//! ## Usage
//!
//! ```ignore
//! use peat_protocol::hierarchy::StateAggregator;
//!
//! // Squad-level aggregation
//! let squad_summary = StateAggregator::aggregate_squad(
//!     "squad-1",
//!     "leader-1",
//!     member_states,
//! )?;
//!
//! // Platoon-level aggregation
//! let platoon_summary = StateAggregator::aggregate_platoon(
//!     "platoon-1",
//!     "platoon-leader",
//!     squad_summaries,
//! )?;
//! ```

use crate::cell::capability_aggregation::CapabilityAggregator;
use crate::models::{NodeConfig, NodeState, NodeStateExt};
use crate::{Error, Result};
use peat_schema::common::v1::{Position, Timestamp};
use peat_schema::hierarchy::v1::{BoundingBox, CompanySummary, PlatoonSummary, SquadSummary};
use peat_schema::node::v1::HealthStatus;
use std::time::SystemTime;

/// Hierarchical state aggregator
///
/// Provides functions to aggregate individual NodeState into SquadSummary,
/// SquadSummary into PlatoonSummary, etc.
pub struct StateAggregator;

impl StateAggregator {
    /// Aggregate squad-level state from member NodeStates
    ///
    /// # Arguments
    ///
    /// * `squad_id` - Unique squad identifier
    /// * `leader_id` - Squad leader node ID
    /// * `members` - Vector of (NodeConfig, NodeState) for squad members
    ///
    /// # Returns
    ///
    /// `SquadSummary` with aggregated position, health, fuel, and capabilities
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No operational members in squad
    /// - Capability aggregation fails
    pub fn aggregate_squad(
        squad_id: &str,
        leader_id: &str,
        members: Vec<(NodeConfig, NodeState)>,
    ) -> Result<SquadSummary> {
        // Filter to only operational members
        let operational: Vec<_> = members
            .into_iter()
            .filter(|(_, state)| state.is_operational())
            .collect();

        if operational.is_empty() {
            return Err(Error::HierarchicalOp {
                message: "Squad has no operational members".to_string(),
                operation: "aggregate_squad".to_string(),
                source: None,
            });
        }

        // Extract member IDs
        let member_ids: Vec<String> = operational.iter().map(|(cfg, _)| cfg.id.clone()).collect();
        let member_count = operational.len() as u32;

        // Calculate position centroid
        let position_centroid = Self::calculate_position_centroid(&operational)?;

        // Calculate average fuel
        let avg_fuel_minutes = Self::calculate_avg_fuel(&operational);

        // Find worst health status
        let worst_health = Self::find_worst_health(&operational);

        // Count operational members (already filtered, so this is the count)
        let operational_count = member_count;

        // Aggregate capabilities using existing CapabilityAggregator
        let aggregated_capabilities_map =
            CapabilityAggregator::aggregate_capabilities(&operational)?;

        // Convert to proto capabilities
        let aggregated_capabilities = aggregated_capabilities_map
            .into_iter()
            .map(
                |(cap_type, agg_cap)| peat_schema::capability::v1::Capability {
                    id: format!("{}_{:?}", squad_id, cap_type),
                    name: format!("{:?}", cap_type),
                    capability_type: cap_type as i32,
                    confidence: agg_cap.confidence,
                    metadata_json: format!(
                        "{{\"contributors\":{},\"requires_oversight\":{}}}",
                        agg_cap.contributor_count, agg_cap.requires_oversight
                    ),
                    registered_at: Some(Self::current_timestamp()),
                },
            )
            .collect();

        // Calculate readiness score using CapabilityAggregator
        let readiness_score = CapabilityAggregator::calculate_readiness_score(
            &CapabilityAggregator::aggregate_capabilities(&operational)?,
        );

        // Calculate bounding box
        let bounding_box = Some(Self::calculate_bounding_box(&operational)?);

        // Create timestamp
        let aggregated_at = Some(Self::current_timestamp());

        Ok(SquadSummary {
            squad_id: squad_id.to_string(),
            leader_id: leader_id.to_string(),
            member_ids,
            member_count,
            position_centroid: Some(position_centroid),
            avg_fuel_minutes,
            worst_health: worst_health as i32,
            operational_count,
            aggregated_capabilities,
            readiness_score,
            bounding_box,
            aggregated_at,
        })
    }

    /// Aggregate platoon-level state from squad summaries
    ///
    /// # Arguments
    ///
    /// * `platoon_id` - Unique platoon identifier
    /// * `leader_id` - Platoon leader node ID
    /// * `squads` - Vector of SquadSummary from constituent squads
    ///
    /// # Returns
    ///
    /// `PlatoonSummary` with aggregated position, health, fuel, and capabilities
    pub fn aggregate_platoon(
        platoon_id: &str,
        leader_id: &str,
        squads: Vec<SquadSummary>,
    ) -> Result<PlatoonSummary> {
        if squads.is_empty() {
            return Err(Error::HierarchicalOp {
                message: "Platoon has no squads".to_string(),
                operation: "aggregate_platoon".to_string(),
                source: None,
            });
        }

        // Extract squad IDs
        let squad_ids: Vec<String> = squads.iter().map(|s| s.squad_id.clone()).collect();
        let squad_count = squads.len() as u32;

        // Sum total member count
        let total_member_count: u32 = squads.iter().map(|s| s.member_count).sum();

        // Calculate position centroid from squad centroids
        let position_centroid = Self::calculate_position_centroid_from_positions(
            &squads
                .iter()
                .filter_map(|s| s.position_centroid.as_ref())
                .cloned()
                .collect::<Vec<_>>(),
        )?;

        // Calculate average fuel across squads (weighted by member count)
        let avg_fuel_minutes = Self::calculate_weighted_avg_fuel(&squads);

        // Find worst health across squads
        let worst_health = Self::find_worst_health_from_squads(&squads);

        // Sum operational counts
        let operational_count: u32 = squads.iter().map(|s| s.operational_count).sum();

        // Aggregate capabilities across squads (union of capabilities)
        let aggregated_capabilities = Self::aggregate_capabilities_from_squads(&squads);

        // Calculate platoon readiness (weighted average of squad readiness)
        let readiness_score = Self::calculate_weighted_readiness(&squads);

        // Calculate platoon bounding box from squad bounding boxes
        let bounding_box = Some(Self::aggregate_bounding_boxes(&squads)?);

        // Create timestamp
        let aggregated_at = Some(Self::current_timestamp());

        Ok(PlatoonSummary {
            platoon_id: platoon_id.to_string(),
            leader_id: leader_id.to_string(),
            squad_ids,
            squad_count,
            total_member_count,
            position_centroid: Some(position_centroid),
            avg_fuel_minutes,
            worst_health: worst_health as i32,
            operational_count,
            aggregated_capabilities,
            readiness_score,
            bounding_box,
            aggregated_at,
        })
    }

    /// Aggregate company-level state from platoon summaries
    ///
    /// # Arguments
    ///
    /// * `company_id` - Unique company identifier
    /// * `leader_id` - Company commander node ID
    /// * `platoons` - Vector of PlatoonSummary from constituent platoons
    ///
    /// # Returns
    ///
    /// `CompanySummary` with aggregated position, health, fuel, and capabilities
    pub fn aggregate_company(
        company_id: &str,
        leader_id: &str,
        platoons: Vec<PlatoonSummary>,
    ) -> Result<CompanySummary> {
        if platoons.is_empty() {
            return Err(Error::HierarchicalOp {
                message: "Company has no platoons".to_string(),
                operation: "aggregate_company".to_string(),
                source: None,
            });
        }

        // Extract platoon IDs
        let platoon_ids: Vec<String> = platoons.iter().map(|p| p.platoon_id.clone()).collect();
        let platoon_count = platoons.len() as u32;

        // Sum total member count
        let total_member_count: u32 = platoons.iter().map(|p| p.total_member_count).sum();

        // Calculate position centroid from platoon centroids
        let position_centroid = Self::calculate_position_centroid_from_positions(
            &platoons
                .iter()
                .filter_map(|p| p.position_centroid.as_ref())
                .cloned()
                .collect::<Vec<_>>(),
        )?;

        // Calculate average fuel across platoons (weighted by member count)
        let avg_fuel_minutes = Self::calculate_weighted_avg_fuel_from_platoons(&platoons);

        // Find worst health across platoons
        let worst_health = Self::find_worst_health_from_platoons(&platoons);

        // Sum operational counts
        let operational_count: u32 = platoons.iter().map(|p| p.operational_count).sum();

        // Aggregate capabilities across platoons (union of capabilities)
        let aggregated_capabilities = Self::aggregate_capabilities_from_platoons(&platoons);

        // Calculate company readiness (weighted average of platoon readiness)
        let readiness_score = Self::calculate_weighted_readiness_from_platoons(&platoons);

        // Calculate company bounding box from platoon bounding boxes
        let bounding_box = Some(Self::aggregate_bounding_boxes_from_platoons(&platoons)?);

        // Create timestamp
        let aggregated_at = Some(Self::current_timestamp());

        Ok(CompanySummary {
            company_id: company_id.to_string(),
            leader_id: leader_id.to_string(),
            platoon_ids,
            platoon_count,
            total_member_count,
            position_centroid: Some(position_centroid),
            avg_fuel_minutes,
            worst_health: worst_health as i32,
            operational_count,
            aggregated_capabilities,
            readiness_score,
            bounding_box,
            aggregated_at,
        })
    }

    // Helper functions

    /// Calculate position centroid from member states
    fn calculate_position_centroid(members: &[(NodeConfig, NodeState)]) -> Result<Position> {
        let positions: Vec<&Position> = members
            .iter()
            .filter_map(|(_, state)| state.position.as_ref())
            .collect();

        if positions.is_empty() {
            return Err(Error::HierarchicalOp {
                message: "No valid positions to aggregate".to_string(),
                operation: "calculate_position_centroid".to_string(),
                source: None,
            });
        }

        let lat_sum: f64 = positions.iter().map(|p| p.latitude).sum();
        let lon_sum: f64 = positions.iter().map(|p| p.longitude).sum();
        let alt_sum: f64 = positions.iter().map(|p| p.altitude).sum();
        let count = positions.len() as f64;

        Ok(Position {
            latitude: lat_sum / count,
            longitude: lon_sum / count,
            altitude: alt_sum / count,
        })
    }

    /// Calculate position centroid from existing positions (for platoon aggregation)
    fn calculate_position_centroid_from_positions(positions: &[Position]) -> Result<Position> {
        if positions.is_empty() {
            return Err(Error::HierarchicalOp {
                message: "No valid positions to aggregate".to_string(),
                operation: "calculate_position_centroid_from_positions".to_string(),
                source: None,
            });
        }

        let lat_sum: f64 = positions.iter().map(|p| p.latitude).sum();
        let lon_sum: f64 = positions.iter().map(|p| p.longitude).sum();
        let alt_sum: f64 = positions.iter().map(|p| p.altitude).sum();
        let count = positions.len() as f64;

        Ok(Position {
            latitude: lat_sum / count,
            longitude: lon_sum / count,
            altitude: alt_sum / count,
        })
    }

    /// Calculate average fuel from member states
    fn calculate_avg_fuel(members: &[(NodeConfig, NodeState)]) -> f32 {
        if members.is_empty() {
            return 0.0;
        }

        let sum: u32 = members.iter().map(|(_, state)| state.fuel_minutes).sum();
        sum as f32 / members.len() as f32
    }

    /// Calculate weighted average fuel from squad summaries
    fn calculate_weighted_avg_fuel(squads: &[SquadSummary]) -> f32 {
        if squads.is_empty() {
            return 0.0;
        }

        let total_fuel: f32 = squads
            .iter()
            .map(|s| s.avg_fuel_minutes * s.member_count as f32)
            .sum();
        let total_members: u32 = squads.iter().map(|s| s.member_count).sum();

        if total_members == 0 {
            return 0.0;
        }

        total_fuel / total_members as f32
    }

    /// Find worst health status from member states
    ///
    /// HealthStatus enum values increase with severity:
    /// NOMINAL(1) < DEGRADED(2) < CRITICAL(3) < FAILED(4)
    /// So we use max_by_key to find the worst (highest severity).
    fn find_worst_health(members: &[(NodeConfig, NodeState)]) -> HealthStatus {
        members
            .iter()
            .map(|(_, state)| HealthStatus::try_from(state.health).unwrap_or(HealthStatus::Failed))
            .max_by_key(|h| *h as i32)
            .unwrap_or(HealthStatus::Nominal)
    }

    /// Find worst health status from squad summaries
    fn find_worst_health_from_squads(squads: &[SquadSummary]) -> HealthStatus {
        squads
            .iter()
            .map(|s| HealthStatus::try_from(s.worst_health).unwrap_or(HealthStatus::Failed))
            .max_by_key(|h| *h as i32)
            .unwrap_or(HealthStatus::Nominal)
    }

    /// Calculate bounding box from member positions
    fn calculate_bounding_box(members: &[(NodeConfig, NodeState)]) -> Result<BoundingBox> {
        let positions: Vec<&Position> = members
            .iter()
            .filter_map(|(_, state)| state.position.as_ref())
            .collect();

        if positions.is_empty() {
            return Err(Error::HierarchicalOp {
                message: "No valid positions for bounding box".to_string(),
                operation: "calculate_bounding_box".to_string(),
                source: None,
            });
        }

        let min_lat = positions
            .iter()
            .map(|p| p.latitude)
            .fold(f64::INFINITY, f64::min);
        let max_lat = positions
            .iter()
            .map(|p| p.latitude)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_lon = positions
            .iter()
            .map(|p| p.longitude)
            .fold(f64::INFINITY, f64::min);
        let max_lon = positions
            .iter()
            .map(|p| p.longitude)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_alt = positions
            .iter()
            .map(|p| p.altitude)
            .fold(f64::INFINITY, f64::min) as f32;
        let max_alt = positions
            .iter()
            .map(|p| p.altitude)
            .fold(f64::NEG_INFINITY, f64::max) as f32;

        // Calculate centroid for radius calculation
        let centroid_lat = (min_lat + max_lat) / 2.0;
        let centroid_lon = (min_lon + max_lon) / 2.0;

        // Calculate approximate radius (Haversine distance to furthest point)
        let radius_m = positions
            .iter()
            .map(|p| Self::haversine_distance(centroid_lat, centroid_lon, p.latitude, p.longitude))
            .fold(0.0, f32::max);

        Ok(BoundingBox {
            southwest: Some(Position {
                latitude: min_lat,
                longitude: min_lon,
                altitude: min_alt as f64,
            }),
            northeast: Some(Position {
                latitude: max_lat,
                longitude: max_lon,
                altitude: max_alt as f64,
            }),
            max_altitude: max_alt,
            min_altitude: min_alt,
            radius_m,
        })
    }

    /// Aggregate bounding boxes from squad summaries
    fn aggregate_bounding_boxes(squads: &[SquadSummary]) -> Result<BoundingBox> {
        let boxes: Vec<&BoundingBox> = squads
            .iter()
            .filter_map(|s| s.bounding_box.as_ref())
            .collect();

        if boxes.is_empty() {
            return Err(Error::HierarchicalOp {
                message: "No valid bounding boxes to aggregate".to_string(),
                operation: "aggregate_bounding_boxes".to_string(),
                source: None,
            });
        }

        let min_lat = boxes
            .iter()
            .filter_map(|b| b.southwest.as_ref())
            .map(|p| p.latitude)
            .fold(f64::INFINITY, f64::min);
        let max_lat = boxes
            .iter()
            .filter_map(|b| b.northeast.as_ref())
            .map(|p| p.latitude)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_lon = boxes
            .iter()
            .filter_map(|b| b.southwest.as_ref())
            .map(|p| p.longitude)
            .fold(f64::INFINITY, f64::min);
        let max_lon = boxes
            .iter()
            .filter_map(|b| b.northeast.as_ref())
            .map(|p| p.longitude)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_alt = boxes
            .iter()
            .map(|b| b.min_altitude)
            .fold(f32::INFINITY, f32::min);
        let max_alt = boxes
            .iter()
            .map(|b| b.max_altitude)
            .fold(f32::NEG_INFINITY, f32::max);

        // Calculate radius from max of constituent squad radii
        let radius_m = boxes.iter().map(|b| b.radius_m).fold(0.0, f32::max);

        Ok(BoundingBox {
            southwest: Some(Position {
                latitude: min_lat,
                longitude: min_lon,
                altitude: min_alt as f64,
            }),
            northeast: Some(Position {
                latitude: max_lat,
                longitude: max_lon,
                altitude: max_alt as f64,
            }),
            max_altitude: max_alt,
            min_altitude: min_alt,
            radius_m,
        })
    }

    /// Aggregate capabilities from squad summaries (union of capabilities)
    fn aggregate_capabilities_from_squads(
        squads: &[SquadSummary],
    ) -> Vec<peat_schema::capability::v1::Capability> {
        use std::collections::HashMap;

        let mut capability_map: HashMap<i32, peat_schema::capability::v1::Capability> =
            HashMap::new();

        for squad in squads {
            for cap in &squad.aggregated_capabilities {
                capability_map
                    .entry(cap.capability_type)
                    .and_modify(|existing| {
                        // Take the higher confidence score
                        if cap.confidence > existing.confidence {
                            existing.confidence = cap.confidence;
                        }
                    })
                    .or_insert_with(|| cap.clone());
            }
        }

        capability_map.into_values().collect()
    }

    /// Calculate weighted average readiness score from squad summaries
    fn calculate_weighted_readiness(squads: &[SquadSummary]) -> f32 {
        if squads.is_empty() {
            return 0.0;
        }

        let total_readiness: f32 = squads
            .iter()
            .map(|s| s.readiness_score * s.member_count as f32)
            .sum();
        let total_members: u32 = squads.iter().map(|s| s.member_count).sum();

        if total_members == 0 {
            return 0.0;
        }

        total_readiness / total_members as f32
    }

    // ========================================================================
    // Company-level helper functions (aggregate from PlatoonSummary)
    // ========================================================================

    /// Calculate weighted average fuel from platoon summaries
    fn calculate_weighted_avg_fuel_from_platoons(platoons: &[PlatoonSummary]) -> f32 {
        if platoons.is_empty() {
            return 0.0;
        }

        let total_fuel: f32 = platoons
            .iter()
            .map(|p| p.avg_fuel_minutes * p.total_member_count as f32)
            .sum();
        let total_members: u32 = platoons.iter().map(|p| p.total_member_count).sum();

        if total_members == 0 {
            return 0.0;
        }

        total_fuel / total_members as f32
    }

    /// Find worst health status from platoon summaries
    fn find_worst_health_from_platoons(platoons: &[PlatoonSummary]) -> HealthStatus {
        platoons
            .iter()
            .map(|p| HealthStatus::try_from(p.worst_health).unwrap_or(HealthStatus::Failed))
            .max_by_key(|h| *h as i32)
            .unwrap_or(HealthStatus::Nominal)
    }

    /// Aggregate capabilities from platoon summaries (union of capabilities)
    fn aggregate_capabilities_from_platoons(
        platoons: &[PlatoonSummary],
    ) -> Vec<peat_schema::capability::v1::Capability> {
        use std::collections::HashMap;

        let mut capability_map: HashMap<i32, peat_schema::capability::v1::Capability> =
            HashMap::new();

        for platoon in platoons {
            for cap in &platoon.aggregated_capabilities {
                capability_map
                    .entry(cap.capability_type)
                    .and_modify(|existing| {
                        // Take the higher confidence score
                        if cap.confidence > existing.confidence {
                            existing.confidence = cap.confidence;
                        }
                    })
                    .or_insert_with(|| cap.clone());
            }
        }

        capability_map.into_values().collect()
    }

    /// Calculate weighted average readiness score from platoon summaries
    fn calculate_weighted_readiness_from_platoons(platoons: &[PlatoonSummary]) -> f32 {
        if platoons.is_empty() {
            return 0.0;
        }

        let total_readiness: f32 = platoons
            .iter()
            .map(|p| p.readiness_score * p.total_member_count as f32)
            .sum();
        let total_members: u32 = platoons.iter().map(|p| p.total_member_count).sum();

        if total_members == 0 {
            return 0.0;
        }

        total_readiness / total_members as f32
    }

    /// Aggregate bounding boxes from platoon summaries
    fn aggregate_bounding_boxes_from_platoons(platoons: &[PlatoonSummary]) -> Result<BoundingBox> {
        let boxes: Vec<&BoundingBox> = platoons
            .iter()
            .filter_map(|p| p.bounding_box.as_ref())
            .collect();

        if boxes.is_empty() {
            return Err(Error::HierarchicalOp {
                message: "No valid bounding boxes to aggregate".to_string(),
                operation: "aggregate_bounding_boxes_from_platoons".to_string(),
                source: None,
            });
        }

        let min_lat = boxes
            .iter()
            .filter_map(|b| b.southwest.as_ref())
            .map(|p| p.latitude)
            .fold(f64::INFINITY, f64::min);
        let max_lat = boxes
            .iter()
            .filter_map(|b| b.northeast.as_ref())
            .map(|p| p.latitude)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_lon = boxes
            .iter()
            .filter_map(|b| b.southwest.as_ref())
            .map(|p| p.longitude)
            .fold(f64::INFINITY, f64::min);
        let max_lon = boxes
            .iter()
            .filter_map(|b| b.northeast.as_ref())
            .map(|p| p.longitude)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_alt = boxes
            .iter()
            .map(|b| b.min_altitude)
            .fold(f32::INFINITY, f32::min);
        let max_alt = boxes
            .iter()
            .map(|b| b.max_altitude)
            .fold(f32::NEG_INFINITY, f32::max);

        // Calculate radius from max of constituent platoon radii
        let radius_m = boxes.iter().map(|b| b.radius_m).fold(0.0, f32::max);

        Ok(BoundingBox {
            southwest: Some(Position {
                latitude: min_lat,
                longitude: min_lon,
                altitude: min_alt as f64,
            }),
            northeast: Some(Position {
                latitude: max_lat,
                longitude: max_lon,
                altitude: max_alt as f64,
            }),
            max_altitude: max_alt,
            min_altitude: min_alt,
            radius_m,
        })
    }

    /// Haversine distance calculation (approximate)
    ///
    /// Returns distance in meters between two lat/lon points
    fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f32 {
        const EARTH_RADIUS_M: f64 = 6_371_000.0;

        let d_lat = (lat2 - lat1).to_radians();
        let d_lon = (lon2 - lon1).to_radians();

        let a = (d_lat / 2.0).sin().powi(2)
            + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);

        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

        (EARTH_RADIUS_M * c) as f32
    }

    /// Get current timestamp
    fn current_timestamp() -> Timestamp {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();

        Timestamp {
            seconds: now.as_secs(),
            nanos: now.subsec_nanos(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capability, CapabilityExt, CapabilityType, NodeConfigExt, NodeStateExt};
    use peat_schema::node::v1::HealthStatus as SchemaHealthStatus;

    fn create_test_member(
        id: &str,
        lat: f64,
        lon: f64,
        fuel: u32,
        health: SchemaHealthStatus,
    ) -> (NodeConfig, NodeState) {
        let mut config = NodeConfig::new("TestPlatform".to_string());
        config.id = id.to_string();
        config.add_capability(Capability::new(
            format!("{}_sensor", id),
            "Test sensor".to_string(),
            CapabilityType::Sensor,
            0.8,
        ));

        let mut state = NodeState::new((lat, lon, 100.0));
        state.fuel_minutes = fuel;
        state.health = health as i32;

        (config, state)
    }

    #[test]
    fn test_aggregate_squad_basic() {
        let members = vec![
            create_test_member(
                "node-1",
                37.7749,
                -122.4194,
                100,
                SchemaHealthStatus::Nominal,
            ),
            create_test_member(
                "node-2",
                37.7750,
                -122.4195,
                90,
                SchemaHealthStatus::Nominal,
            ),
            create_test_member(
                "node-3",
                37.7751,
                -122.4196,
                80,
                SchemaHealthStatus::Degraded,
            ),
        ];

        let result = StateAggregator::aggregate_squad("squad-1", "node-1", members);
        assert!(result.is_ok());

        let summary = result.unwrap();
        assert_eq!(summary.squad_id, "squad-1");
        assert_eq!(summary.leader_id, "node-1");
        assert_eq!(summary.member_count, 3);
        assert_eq!(summary.operational_count, 3);
        assert!(summary.avg_fuel_minutes > 85.0 && summary.avg_fuel_minutes < 95.0);
        assert_eq!(
            HealthStatus::try_from(summary.worst_health).unwrap(),
            HealthStatus::Degraded
        );
    }

    #[test]
    fn test_aggregate_squad_position_centroid() {
        let members = vec![
            create_test_member("node-1", 0.0, 0.0, 100, SchemaHealthStatus::Nominal),
            create_test_member("node-2", 10.0, 10.0, 100, SchemaHealthStatus::Nominal),
        ];

        let summary = StateAggregator::aggregate_squad("squad-1", "node-1", members).unwrap();
        let centroid = summary.position_centroid.unwrap();

        assert!((centroid.latitude - 5.0).abs() < 0.001);
        assert!((centroid.longitude - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_aggregate_platoon_basic() {
        // Create 3 squad summaries
        let squads = vec![
            SquadSummary {
                squad_id: "squad-1".to_string(),
                leader_id: "leader-1".to_string(),
                member_ids: vec!["n1".to_string(), "n2".to_string()],
                member_count: 2,
                position_centroid: Some(Position {
                    latitude: 37.7749,
                    longitude: -122.4194,
                    altitude: 100.0,
                }),
                avg_fuel_minutes: 100.0,
                worst_health: HealthStatus::Nominal as i32,
                operational_count: 2,
                aggregated_capabilities: vec![],
                readiness_score: 0.8,
                bounding_box: Some(BoundingBox {
                    southwest: Some(Position {
                        latitude: 37.7748,
                        longitude: -122.4195,
                        altitude: 100.0,
                    }),
                    northeast: Some(Position {
                        latitude: 37.7750,
                        longitude: -122.4193,
                        altitude: 100.0,
                    }),
                    max_altitude: 100.0,
                    min_altitude: 100.0,
                    radius_m: 50.0,
                }),
                aggregated_at: None,
            },
            SquadSummary {
                squad_id: "squad-2".to_string(),
                leader_id: "leader-2".to_string(),
                member_ids: vec!["n3".to_string(), "n4".to_string()],
                member_count: 2,
                position_centroid: Some(Position {
                    latitude: 37.7750,
                    longitude: -122.4195,
                    altitude: 100.0,
                }),
                avg_fuel_minutes: 90.0,
                worst_health: HealthStatus::Degraded as i32,
                operational_count: 2,
                aggregated_capabilities: vec![],
                readiness_score: 0.7,
                bounding_box: Some(BoundingBox {
                    southwest: Some(Position {
                        latitude: 37.7749,
                        longitude: -122.4196,
                        altitude: 100.0,
                    }),
                    northeast: Some(Position {
                        latitude: 37.7751,
                        longitude: -122.4194,
                        altitude: 100.0,
                    }),
                    max_altitude: 100.0,
                    min_altitude: 100.0,
                    radius_m: 50.0,
                }),
                aggregated_at: None,
            },
        ];

        let result = StateAggregator::aggregate_platoon("platoon-1", "platoon-leader", squads);
        assert!(result.is_ok());

        let summary = result.unwrap();
        assert_eq!(summary.platoon_id, "platoon-1");
        assert_eq!(summary.leader_id, "platoon-leader");
        assert_eq!(summary.squad_count, 2);
        assert_eq!(summary.total_member_count, 4);
        assert_eq!(summary.operational_count, 4);
        assert!((summary.avg_fuel_minutes - 95.0).abs() < 0.1);
        assert_eq!(
            HealthStatus::try_from(summary.worst_health).unwrap(),
            HealthStatus::Degraded
        );
    }

    #[test]
    fn test_haversine_distance() {
        // San Francisco to Oakland (approx 12-13 km)
        let distance = StateAggregator::haversine_distance(
            37.7749,   // SF lat
            -122.4194, // SF lon
            37.8044,   // Oakland lat
            -122.2712, // Oakland lon
        );

        // Should be approximately 12000-13000 meters
        assert!(distance > 11000.0 && distance < 14000.0);
    }
}
