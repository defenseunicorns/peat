//! Generic packet aggregation for hierarchical telemetry summarization
//!
//! This module provides a generic `Aggregator` trait that bridges the gap between:
//! - **hive-mesh routing**: DataPacket flowing through hierarchy (packets in flight)
//! - **Application-specific aggregation**: Computing summaries from raw data
//!
//! # Architecture
//!
//! The `Aggregator` trait defines a generic interface for aggregating
//! telemetry packets into summary packets. Concrete implementations
//! (e.g., HIVE's `PacketAggregator`) provide domain-specific aggregation
//! logic by implementing this trait.

use super::packet::{DataPacket, DataType};
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

/// Generic trait for aggregating telemetry packets into summaries
///
/// Implementations provide domain-specific aggregation logic that transforms
/// a collection of telemetry `DataPacket`s into a single aggregated packet.
pub trait Aggregator: Send + Sync {
    /// Aggregate telemetry packets into a summary packet
    ///
    /// # Arguments
    ///
    /// * `group_id` - Identifier for the group being aggregated (e.g., squad ID)
    /// * `leader_id` - Node ID of the group leader (source of aggregated packet)
    /// * `telemetry_packets` - Vector of telemetry DataPackets from group members
    ///
    /// # Returns
    ///
    /// A new DataPacket with aggregated telemetry data
    fn aggregate_telemetry(
        &self,
        group_id: &str,
        leader_id: &str,
        telemetry_packets: Vec<DataPacket>,
    ) -> Result<DataPacket, AggregationError>;

    /// Extract a summary from an aggregated telemetry packet
    ///
    /// # Arguments
    ///
    /// * `packet` - DataPacket with DataType::AggregatedTelemetry
    ///
    /// # Returns
    ///
    /// The raw summary bytes from the aggregated packet
    fn extract_summary_bytes(&self, packet: &DataPacket) -> Result<Vec<u8>, AggregationError> {
        if packet.data_type != DataType::AggregatedTelemetry {
            return Err(AggregationError::InvalidPacketType {
                expected: "AggregatedTelemetry".to_string(),
                actual: packet.data_type,
            });
        }
        Ok(packet.payload.clone())
    }
}

/// A no-op aggregator for use when aggregation is not needed
///
/// Always returns an error from `aggregate_telemetry`. Useful as a default
/// type parameter when creating a `MeshRouter` without a real aggregator.
pub struct NoOpAggregator;

impl Aggregator for NoOpAggregator {
    fn aggregate_telemetry(
        &self,
        _group_id: &str,
        _leader_id: &str,
        _telemetry_packets: Vec<DataPacket>,
    ) -> Result<DataPacket, AggregationError> {
        Err(AggregationError::AggregationFailed(
            "No aggregator configured".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregation_error_display() {
        let err = AggregationError::EmptyInput;
        assert_eq!(err.to_string(), "Cannot aggregate empty packet list");

        let err = AggregationError::AggregationFailed("test".to_string());
        assert_eq!(err.to_string(), "Aggregation operation failed: test");
    }

    #[test]
    fn test_aggregation_error_invalid_type() {
        let err = AggregationError::InvalidPacketType {
            expected: "Telemetry".to_string(),
            actual: DataType::Command,
        };
        assert!(err.to_string().contains("Expected Telemetry"));
    }

    #[test]
    fn test_aggregation_error_from_serde() {
        let serde_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err: AggregationError = serde_err.into();
        assert!(err.to_string().contains("deserialize"));
    }

    #[test]
    fn test_noop_aggregator_returns_error() {
        let agg = NoOpAggregator;
        let result = agg.aggregate_telemetry("group", "leader", vec![]);
        assert!(result.is_err());
        if let Err(AggregationError::AggregationFailed(msg)) = result {
            assert!(msg.contains("No aggregator configured"));
        }
    }

    #[test]
    fn test_extract_summary_bytes_valid() {
        let agg = NoOpAggregator;
        let mut packet = DataPacket::telemetry("src", vec![1, 2, 3]);
        packet.data_type = DataType::AggregatedTelemetry;
        let result = agg.extract_summary_bytes(&packet);
        assert_eq!(result.unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_extract_summary_bytes_wrong_type() {
        let agg = NoOpAggregator;
        let packet = DataPacket::command("src", "dst", vec![]);
        let result = agg.extract_summary_bytes(&packet);
        assert!(result.is_err());
        if let Err(AggregationError::InvalidPacketType { expected, actual }) = result {
            assert_eq!(expected, "AggregatedTelemetry");
            assert_eq!(actual, DataType::Command);
        }
    }
}
