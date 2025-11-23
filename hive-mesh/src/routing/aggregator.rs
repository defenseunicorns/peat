//! Packet aggregation for hierarchical telemetry summarization
//!
//! This module provides the PacketAggregator which bridges the gap between:
//! - **hive-mesh routing**: DataPacket flowing through hierarchy (packets in flight)
//! - **hive-protocol aggregation**: StateAggregator for computing summaries (documents at rest)
//!
//! # Architecture
//!
//! The PacketAggregator is a lightweight adapter that:
//! 1. Deserializes DataPacket payloads into domain objects (NodeState, NodeConfig)
//! 2. Calls StateAggregator::aggregate_squad() from hive-protocol
//! 3. Serializes SquadSummary back into DataPacket with AggregatedTelemetry type
//!
//! This reuses all existing aggregation logic (position centroid, health, fuel,
//! capabilities) without reimplementing it in hive-mesh.
//!
//! # Example
//!
//! ```ignore
//! use hive_mesh::routing::{PacketAggregator, DataPacket};
//!
//! let aggregator = PacketAggregator::new();
//!
//! // Incoming telemetry packets from squad members
//! let telemetry_packets = vec![packet1, packet2, packet3];
//!
//! // Aggregate into single squad summary packet
//! let aggregated_packet = aggregator.aggregate_telemetry(
//!     "squad-1",
//!     "leader-node-1",
//!     telemetry_packets,
//! )?;
//!
//! // Forward aggregated packet up to platoon level
//! router.route(&aggregated_packet, &state, "leader-node-1");
//! ```

use super::packet::{DataPacket, DataType};
use hive_protocol::hierarchy::StateAggregator;
use hive_schema::hierarchy::v1::SquadSummary;
use hive_schema::node::v1::{NodeConfig, NodeState};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during packet aggregation
#[derive(Debug, Error)]
pub enum AggregationError {
    /// Payload deserialization failed
    #[error("Failed to deserialize payload: {0}")]
    DeserializationError(#[from] serde_json::Error),

    /// Hierarchical aggregation operation failed
    #[error("Aggregation operation failed: {0}")]
    AggregationFailed(String),

    /// Invalid packet type for aggregation
    #[error("Expected {expected} packet type, got {actual:?}")]
    InvalidPacketType { expected: String, actual: DataType },

    /// Empty input when non-empty required
    #[error("Cannot aggregate empty packet list")]
    EmptyInput,
}

/// Envelope for NodeState + NodeConfig in DataPacket payload
///
/// Since DataPacket payload is opaque Vec<u8>, we need to serialize
/// both the node configuration and state together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryPayload {
    /// Node configuration (capabilities, comm range, etc.)
    pub config: NodeConfig,

    /// Current node state (position, fuel, health, etc.)
    pub state: NodeState,
}

impl TelemetryPayload {
    /// Create a new telemetry payload
    pub fn new(config: NodeConfig, state: NodeState) -> Self {
        Self { config, state }
    }

    /// Serialize to JSON bytes for DataPacket payload
    pub fn to_bytes(&self) -> Result<Vec<u8>, AggregationError> {
        serde_json::to_vec(self).map_err(AggregationError::from)
    }

    /// Deserialize from DataPacket payload bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AggregationError> {
        serde_json::from_slice(bytes).map_err(AggregationError::from)
    }
}

/// Packet aggregator for hierarchical telemetry summarization
///
/// Bridges hive-mesh routing layer with hive-protocol aggregation logic.
pub struct PacketAggregator;

impl PacketAggregator {
    /// Create a new packet aggregator
    pub fn new() -> Self {
        Self
    }

    /// Aggregate telemetry packets into a squad summary packet
    ///
    /// # Arguments
    ///
    /// * `squad_id` - Unique squad identifier
    /// * `leader_id` - Squad leader node ID (source of aggregated packet)
    /// * `telemetry_packets` - Vector of telemetry DataPackets from squad members
    ///
    /// # Returns
    ///
    /// A new DataPacket with:
    /// - `data_type`: DataType::AggregatedTelemetry
    /// - `payload`: Serialized SquadSummary
    /// - `source_node_id`: leader_id
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Input packets are empty
    /// - Any packet has wrong data type (not Telemetry)
    /// - Payload deserialization fails
    /// - StateAggregator::aggregate_squad() fails
    pub fn aggregate_telemetry(
        &self,
        squad_id: &str,
        leader_id: &str,
        telemetry_packets: Vec<DataPacket>,
    ) -> Result<DataPacket, AggregationError> {
        if telemetry_packets.is_empty() {
            return Err(AggregationError::EmptyInput);
        }

        // Validate all packets are Telemetry type
        for packet in &telemetry_packets {
            if packet.data_type != DataType::Telemetry {
                return Err(AggregationError::InvalidPacketType {
                    expected: "Telemetry".to_string(),
                    actual: packet.data_type,
                });
            }
        }

        // Deserialize all payloads into (NodeConfig, NodeState) pairs
        let member_states: Result<Vec<(NodeConfig, NodeState)>, AggregationError> =
            telemetry_packets
                .iter()
                .map(|packet| {
                    let payload = TelemetryPayload::from_bytes(&packet.payload)?;
                    Ok((payload.config, payload.state))
                })
                .collect();

        let member_states = member_states?;

        // Call StateAggregator from hive-protocol
        let squad_summary = StateAggregator::aggregate_squad(squad_id, leader_id, member_states)
            .map_err(|e| AggregationError::AggregationFailed(e.to_string()))?;

        // Serialize SquadSummary back into DataPacket payload
        let aggregated_payload =
            serde_json::to_vec(&squad_summary).map_err(AggregationError::from)?;

        // Create new DataPacket with AggregatedTelemetry type
        Ok(DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: leader_id.to_string(),
            destination_node_id: None, // Flows upward (determined by topology)
            data_type: DataType::AggregatedTelemetry,
            direction: super::packet::DataDirection::Upward,
            hop_count: 0,
            max_hops: 10,
            payload: aggregated_payload,
        })
    }

    /// Deserialize a SquadSummary from an aggregated telemetry packet
    ///
    /// Helper function for consuming aggregated packets at higher levels.
    ///
    /// # Arguments
    ///
    /// * `packet` - DataPacket with DataType::AggregatedTelemetry
    ///
    /// # Returns
    ///
    /// Deserialized SquadSummary
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Packet is not AggregatedTelemetry type
    /// - Payload deserialization fails
    pub fn extract_squad_summary(
        &self,
        packet: &DataPacket,
    ) -> Result<SquadSummary, AggregationError> {
        if packet.data_type != DataType::AggregatedTelemetry {
            return Err(AggregationError::InvalidPacketType {
                expected: "AggregatedTelemetry".to_string(),
                actual: packet.data_type,
            });
        }

        serde_json::from_slice(&packet.payload).map_err(AggregationError::from)
    }
}

impl Default for PacketAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregator_creation() {
        let aggregator = PacketAggregator::new();
        assert!(std::mem::size_of_val(&aggregator) == 0); // ZST
    }

    #[test]
    fn test_aggregate_empty_packets() {
        let aggregator = PacketAggregator::new();

        let result = aggregator.aggregate_telemetry("squad-1", "node-1", vec![]);

        assert!(matches!(result, Err(AggregationError::EmptyInput)));
    }

    #[test]
    fn test_aggregate_wrong_packet_type() {
        let aggregator = PacketAggregator::new();

        // Create a command packet instead of telemetry
        let command_packet = DataPacket::command("hq", "node-1", vec![1, 2, 3]);

        let result = aggregator.aggregate_telemetry("squad-1", "node-1", vec![command_packet]);

        assert!(matches!(
            result,
            Err(AggregationError::InvalidPacketType { .. })
        ));
    }

    #[test]
    fn test_extract_summary_wrong_type() {
        let aggregator = PacketAggregator::new();

        // Create a telemetry packet (not aggregated)
        let telemetry_packet = DataPacket::telemetry("node-1", vec![1, 2, 3]);

        let result = aggregator.extract_squad_summary(&telemetry_packet);

        assert!(matches!(
            result,
            Err(AggregationError::InvalidPacketType { .. })
        ));
    }
}
