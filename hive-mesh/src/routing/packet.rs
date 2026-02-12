//! Data packet types for hierarchical mesh routing
//!
//! Defines the structure and metadata for data flowing through the mesh.

use serde::{Deserialize, Serialize};

/// Direction of data flow in the hierarchy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataDirection {
    /// Data flowing upward toward higher hierarchy levels (telemetry aggregation)
    Upward,

    /// Data flowing downward toward lower hierarchy levels (command dissemination)
    Downward,

    /// Data exchanged between peers at the same hierarchy level (coordination)
    Lateral,
}

/// Type of data being routed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    /// Telemetry data from sensors/platforms (flows upward)
    Telemetry,

    /// Status updates (typically flows upward)
    Status,

    /// Commands for execution (flows downward)
    Command,

    /// Configuration updates (flows downward)
    Configuration,

    /// Coordination messages between lateral peers (same level)
    Coordination,

    /// Aggregated telemetry (flows upward, higher-level summary)
    AggregatedTelemetry,
}

impl DataType {
    /// Get the typical direction for this data type
    pub fn typical_direction(&self) -> DataDirection {
        match self {
            DataType::Telemetry => DataDirection::Upward,
            DataType::Status => DataDirection::Upward,
            DataType::Command => DataDirection::Downward,
            DataType::Configuration => DataDirection::Downward,
            DataType::Coordination => DataDirection::Lateral,
            DataType::AggregatedTelemetry => DataDirection::Upward,
        }
    }

    /// Check if this data type requires aggregation
    pub fn requires_aggregation(&self) -> bool {
        matches!(self, DataType::Telemetry | DataType::Status)
    }
}

/// Data packet metadata for routing decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPacket {
    /// Unique packet identifier
    pub packet_id: String,

    /// Node ID of the original sender
    pub source_node_id: String,

    /// Node ID of the intended final recipient (None = broadcast)
    pub destination_node_id: Option<String>,

    /// Type of data in this packet
    pub data_type: DataType,

    /// Direction of data flow
    pub direction: DataDirection,

    /// Hop count (incremented at each forward)
    pub hop_count: u16,

    /// Maximum hops before packet is dropped (prevents loops)
    pub max_hops: u16,

    /// Payload data (opaque to router)
    pub payload: Vec<u8>,
}

impl DataPacket {
    /// Create a new telemetry packet
    pub fn telemetry(source_node_id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: source_node_id.into(),
            destination_node_id: None, // Telemetry flows to HQ (determined by topology)
            data_type: DataType::Telemetry,
            direction: DataDirection::Upward,
            hop_count: 0,
            max_hops: 10, // Reasonable default for typical hierarchy depth
            payload,
        }
    }

    /// Create a new command packet
    pub fn command(
        source_node_id: impl Into<String>,
        destination_node_id: impl Into<String>,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: source_node_id.into(),
            destination_node_id: Some(destination_node_id.into()),
            data_type: DataType::Command,
            direction: DataDirection::Downward,
            hop_count: 0,
            max_hops: 10,
            payload,
        }
    }

    /// Create a new status update packet
    pub fn status(source_node_id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: source_node_id.into(),
            destination_node_id: None,
            data_type: DataType::Status,
            direction: DataDirection::Upward,
            hop_count: 0,
            max_hops: 10,
            payload,
        }
    }

    /// Create a new coordination packet (lateral)
    pub fn coordination(
        source_node_id: impl Into<String>,
        destination_node_id: impl Into<String>,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: source_node_id.into(),
            destination_node_id: Some(destination_node_id.into()),
            data_type: DataType::Coordination,
            direction: DataDirection::Lateral,
            hop_count: 0,
            max_hops: 3, // Lateral messages shouldn't travel far
            payload,
        }
    }

    /// Create an aggregated telemetry packet
    pub fn aggregated_telemetry(source_node_id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            packet_id: uuid::Uuid::new_v4().to_string(),
            source_node_id: source_node_id.into(),
            destination_node_id: None,
            data_type: DataType::AggregatedTelemetry,
            direction: DataDirection::Upward,
            hop_count: 0,
            max_hops: 10,
            payload,
        }
    }

    /// Increment hop count when forwarding packet
    ///
    /// Returns true if packet can be forwarded (not yet at max hops),
    /// false if packet should be dropped.
    pub fn increment_hop(&mut self) -> bool {
        self.hop_count += 1;
        self.hop_count < self.max_hops
    }

    /// Check if packet has reached maximum hop count
    pub fn at_max_hops(&self) -> bool {
        self.hop_count >= self.max_hops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_packet_creation() {
        let packet = DataPacket::telemetry("node-1", vec![1, 2, 3]);

        assert_eq!(packet.source_node_id, "node-1");
        assert_eq!(packet.data_type, DataType::Telemetry);
        assert_eq!(packet.direction, DataDirection::Upward);
        assert_eq!(packet.hop_count, 0);
        assert_eq!(packet.max_hops, 10);
        assert_eq!(packet.payload, vec![1, 2, 3]);
        assert!(packet.destination_node_id.is_none());
    }

    #[test]
    fn test_command_packet_creation() {
        let packet = DataPacket::command("hq", "node-1", vec![4, 5, 6]);

        assert_eq!(packet.source_node_id, "hq");
        assert_eq!(packet.destination_node_id, Some("node-1".to_string()));
        assert_eq!(packet.data_type, DataType::Command);
        assert_eq!(packet.direction, DataDirection::Downward);
    }

    #[test]
    fn test_hop_increment() {
        let mut packet = DataPacket::telemetry("node-1", vec![]);

        // Should be able to increment up to max_hops - 1
        for i in 0..9 {
            assert!(packet.increment_hop(), "Failed at hop {}", i);
            assert_eq!(packet.hop_count, i + 1);
        }

        // At hop 10, should return false (at max)
        assert!(!packet.increment_hop());
        assert_eq!(packet.hop_count, 10);
    }

    #[test]
    fn test_at_max_hops() {
        let mut packet = DataPacket::telemetry("node-1", vec![]);
        assert!(!packet.at_max_hops());

        packet.hop_count = 10;
        assert!(packet.at_max_hops());
    }

    #[test]
    fn test_data_type_typical_direction() {
        assert_eq!(
            DataType::Telemetry.typical_direction(),
            DataDirection::Upward
        );
        assert_eq!(DataType::Status.typical_direction(), DataDirection::Upward);
        assert_eq!(
            DataType::Command.typical_direction(),
            DataDirection::Downward
        );
        assert_eq!(
            DataType::Configuration.typical_direction(),
            DataDirection::Downward
        );
        assert_eq!(
            DataType::Coordination.typical_direction(),
            DataDirection::Lateral
        );
        assert_eq!(
            DataType::AggregatedTelemetry.typical_direction(),
            DataDirection::Upward
        );
    }

    #[test]
    fn test_data_type_requires_aggregation() {
        assert!(DataType::Telemetry.requires_aggregation());
        assert!(DataType::Status.requires_aggregation());
        assert!(!DataType::Command.requires_aggregation());
        assert!(!DataType::Configuration.requires_aggregation());
        assert!(!DataType::Coordination.requires_aggregation());
        assert!(!DataType::AggregatedTelemetry.requires_aggregation()); // Already aggregated
    }

    #[test]
    fn test_status_packet_creation() {
        let packet = DataPacket::status("node-1", vec![10, 20]);

        assert_eq!(packet.source_node_id, "node-1");
        assert_eq!(packet.data_type, DataType::Status);
        assert_eq!(packet.direction, DataDirection::Upward);
        assert!(packet.destination_node_id.is_none());
        assert_eq!(packet.hop_count, 0);
        assert_eq!(packet.max_hops, 10);
        assert_eq!(packet.payload, vec![10, 20]);
    }

    #[test]
    fn test_coordination_packet_creation() {
        let packet = DataPacket::coordination("node-1", "node-2", vec![5]);

        assert_eq!(packet.source_node_id, "node-1");
        assert_eq!(packet.destination_node_id, Some("node-2".to_string()));
        assert_eq!(packet.data_type, DataType::Coordination);
        assert_eq!(packet.direction, DataDirection::Lateral);
        assert_eq!(packet.max_hops, 3); // Lateral messages have lower max hops
    }

    #[test]
    fn test_aggregated_telemetry_packet_creation() {
        let packet = DataPacket::aggregated_telemetry("leader-1", vec![99]);

        assert_eq!(packet.source_node_id, "leader-1");
        assert_eq!(packet.data_type, DataType::AggregatedTelemetry);
        assert_eq!(packet.direction, DataDirection::Upward);
        assert!(packet.destination_node_id.is_none());
        assert_eq!(packet.max_hops, 10);
    }

    #[test]
    fn test_increment_hop_returns_false_at_max() {
        let mut packet = DataPacket::coordination("a", "b", vec![]);
        // max_hops is 3 for coordination
        assert!(packet.increment_hop()); // hop_count = 1
        assert!(packet.increment_hop()); // hop_count = 2
        assert!(!packet.increment_hop()); // hop_count = 3, equals max_hops

        assert!(packet.at_max_hops());
    }

    #[test]
    fn test_at_max_hops_initially_false() {
        let packet = DataPacket::telemetry("node-1", vec![]);
        assert!(!packet.at_max_hops());
    }

    #[test]
    fn test_packet_unique_ids() {
        let p1 = DataPacket::telemetry("node-1", vec![1]);
        let p2 = DataPacket::telemetry("node-1", vec![1]);

        // Each packet should get a unique ID
        assert_ne!(p1.packet_id, p2.packet_id);
    }

    #[test]
    fn test_packet_serialization() {
        let packet = DataPacket::command("hq", "node-1", vec![1, 2, 3]);
        let json = serde_json::to_string(&packet).unwrap();
        let deserialized: DataPacket = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.source_node_id, "hq");
        assert_eq!(deserialized.destination_node_id, Some("node-1".to_string()));
        assert_eq!(deserialized.data_type, DataType::Command);
        assert_eq!(deserialized.direction, DataDirection::Downward);
        assert_eq!(deserialized.payload, vec![1, 2, 3]);
    }

    #[test]
    fn test_data_direction_serialization() {
        let json = serde_json::to_string(&DataDirection::Lateral).unwrap();
        let deserialized: DataDirection = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, DataDirection::Lateral);
    }
}
