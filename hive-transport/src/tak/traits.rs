//! TAK transport trait definitions

use async_trait::async_trait;
use futures::Stream;
use hive_protocol::cot::CotEvent;
use std::pin::Pin;

use super::error::TakError;
use super::metrics::{QueueDepthMetrics, TakMetrics};

/// Priority level for CoT messages (1-5, where 1 is highest)
pub type Priority = u8;

/// Stream of incoming CoT events
pub type CotEventStream = Pin<Box<dyn Stream<Item = Result<CotEvent, TakError>> + Send>>;

/// Filter for incoming CoT events
#[derive(Debug, Clone, Default)]
pub struct CotFilter {
    /// Filter by CoT type prefix (e.g., "a-f-" for friendly atoms)
    pub type_prefix: Option<String>,

    /// Filter by UID pattern
    pub uid_pattern: Option<String>,

    /// Filter by callsign
    pub callsign: Option<String>,

    /// Include only events newer than this (seconds ago)
    pub max_age_secs: Option<u64>,
}

impl CotFilter {
    /// Create a filter that accepts all events
    pub fn all() -> Self {
        Self::default()
    }

    /// Filter by CoT type prefix
    pub fn with_type_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.type_prefix = Some(prefix.into());
        self
    }

    /// Filter by UID pattern (glob-style)
    pub fn with_uid_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.uid_pattern = Some(pattern.into());
        self
    }

    /// Filter by callsign
    pub fn with_callsign(mut self, callsign: impl Into<String>) -> Self {
        self.callsign = Some(callsign.into());
        self
    }

    /// Only include events from the last N seconds
    pub fn with_max_age(mut self, secs: u64) -> Self {
        self.max_age_secs = Some(secs);
        self
    }

    /// Check if an event matches this filter
    pub fn matches(&self, event: &CotEvent) -> bool {
        // Check type prefix
        if let Some(prefix) = &self.type_prefix {
            if !event.cot_type.as_str().starts_with(prefix) {
                return false;
            }
        }

        // Check UID pattern (simple prefix match for now)
        if let Some(pattern) = &self.uid_pattern {
            if !event.uid.contains(pattern) {
                return false;
            }
        }

        // TODO: Add callsign and max_age checks

        true
    }
}

/// TAK Transport Adapter
///
/// Provides bidirectional CoT message transport between HIVE and TAK ecosystem.
/// Supports TAK Server (TCP/SSL) and Mesh SA (UDP multicast) modes.
#[async_trait]
pub trait TakTransport: Send + Sync {
    /// Connect to TAK server or mesh
    ///
    /// For TAK Server mode: Establishes TCP/SSL connection
    /// For Mesh SA mode: Joins multicast group
    async fn connect(&mut self) -> Result<(), TakError>;

    /// Disconnect gracefully
    ///
    /// Closes connection and flushes pending messages (if possible)
    async fn disconnect(&mut self) -> Result<(), TakError>;

    /// Send CoT event to TAK
    ///
    /// Message is queued if disconnected (DIL resilience).
    /// Priority determines queue position and drop precedence.
    async fn send_cot(&self, event: &CotEvent, priority: Priority) -> Result<(), TakError>;

    /// Subscribe to incoming CoT events
    ///
    /// Returns a stream of CoT events matching the filter.
    /// Stream continues across reconnections.
    async fn subscribe(&self, filter: CotFilter) -> Result<CotEventStream, TakError>;

    /// Check connection health
    fn is_connected(&self) -> bool;

    /// Get connection metrics
    fn metrics(&self) -> TakMetrics;

    /// Get current queue depth
    fn queue_depth(&self) -> QueueDepthMetrics;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cot_filter_all() {
        let filter = CotFilter::all();
        assert!(filter.type_prefix.is_none());
        assert!(filter.uid_pattern.is_none());
    }

    #[test]
    fn test_cot_filter_builder() {
        let filter = CotFilter::all()
            .with_type_prefix("a-f-")
            .with_uid_pattern("HIVE-")
            .with_max_age(300);

        assert_eq!(filter.type_prefix.as_deref(), Some("a-f-"));
        assert_eq!(filter.uid_pattern.as_deref(), Some("HIVE-"));
        assert_eq!(filter.max_age_secs, Some(300));
    }
}
