//! PEAT-specific packet aggregation for hierarchical telemetry summarization
//!
//! This module provides the `PacketAggregator` which bridges the gap between:
//! - **peat-mesh routing**: DataPacket flowing through hierarchy (packets in flight)
//! - **peat-protocol aggregation**: StateAggregator for computing summaries (documents at rest)
//!
//! # Architecture
//!
//! The PacketAggregator implements `peat_mesh::routing::Aggregator` and:
//! 1. Deserializes DataPacket payloads into domain objects (NodeState, NodeConfig)
//! 2. Calls StateAggregator::aggregate_squad() from peat-protocol
//! 3. Serializes SquadSummary back into DataPacket with AggregatedTelemetry type

use crate::hierarchy::StateAggregator;
use peat_mesh::routing::{AggregationError, Aggregator, DataDirection, DataPacket, DataType};
use peat_schema::hierarchy::v1::SquadSummary;
use peat_schema::node::v1::{NodeConfig, NodeState};
use serde::{Deserialize, Serialize};

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

/// PEAT-specific packet aggregator for hierarchical telemetry summarization
///
/// Bridges peat-mesh routing layer with peat-protocol aggregation logic.
pub struct PacketAggregator;

impl PacketAggregator {
    /// Create a new packet aggregator
    pub fn new() -> Self {
        Self
    }

    /// Deserialize a SquadSummary from an aggregated telemetry packet
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

impl Aggregator for PacketAggregator {
    fn aggregate_telemetry(
        &self,
        group_id: &str,
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

        // Call StateAggregator from peat-protocol
        let squad_summary = StateAggregator::aggregate_squad(group_id, leader_id, member_states)
            .map_err(|e| AggregationError::AggregationFailed(e.to_string()))?;

        // Serialize SquadSummary back into DataPacket payload
        let aggregated_payload =
            serde_json::to_vec(&squad_summary).map_err(AggregationError::from)?;

        // Create new DataPacket with AggregatedTelemetry type
        Ok(DataPacket {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: leader_id.to_string(),
            destination_node_id: None,
            data_type: DataType::AggregatedTelemetry,
            direction: DataDirection::Upward,
            hop_count: 0,
            max_hops: 10,
            payload: aggregated_payload,
        })
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
