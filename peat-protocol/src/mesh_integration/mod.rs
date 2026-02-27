//! Mesh integration adapters
//!
//! This module contains PEAT-specific adapters that bridge `peat-mesh`
//! generic interfaces with Peat Protocol domain types.

mod aggregator;

pub use aggregator::{PacketAggregator, TelemetryPayload};
