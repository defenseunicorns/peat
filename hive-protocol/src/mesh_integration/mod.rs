//! Mesh integration adapters
//!
//! This module contains HIVE-specific adapters that bridge `hive-mesh`
//! generic interfaces with HIVE Protocol domain types.

mod aggregator;

pub use aggregator::{PacketAggregator, TelemetryPayload};
