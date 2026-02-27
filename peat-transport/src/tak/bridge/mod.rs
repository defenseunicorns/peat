//! PEAT-TAK Bridge
//!
//! Bidirectional bridge between Peat mesh network and TAK ecosystem.
//! Implements hierarchical filtering, aggregation policies, and QoS integration.
//!
//! ## Architecture (ADR-020, ADR-029)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        PEAT Mesh Network                        │
//! │   (Nodes, Cells, Formations with CRDT-synced state)            │
//! └────────────────────────────┬────────────────────────────────────┘
//!                              │
//!                    ┌─────────▼─────────┐
//!                    │   PeatTakBridge   │
//!                    │                   │
//!                    │ • Aggregation     │
//!                    │ • Filtering       │
//!                    │ • QoS Mapping     │
//!                    │ • CoT Encoding    │
//!                    └─────────┬─────────┘
//!                              │
//! ┌────────────────────────────▼────────────────────────────────────┐
//! │                    TAK Server / Mesh SA                         │
//! │            (ATAK, WinTAK, iTAK, TAK Server)                     │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

mod aggregation;
mod config;
mod filter;

pub use aggregation::Aggregator;
pub use config::{AggregationPolicy, BridgeConfig, EchelonLevel};
pub use filter::{BridgeFilter, FilterDecision, GeoFilter};

use async_trait::async_trait;
use peat_protocol::cot::{
    CapabilityAdvertisement, CotEncoder, CotEvent, FormationCapabilitySummary, HandoffMessage,
    TrackUpdate,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use super::error::TakError;
use super::traits::{Priority, TakTransport};

/// PEAT message that can be bridged to TAK
#[derive(Debug, Clone)]
pub enum PeatMessage {
    /// Track/platform position update
    Track(TrackUpdate),
    /// Capability advertisement
    Capability(CapabilityAdvertisement),
    /// Track handoff between cells
    Handoff(HandoffMessage),
    /// Formation capability summary
    FormationSummary(FormationCapabilitySummary),
}

impl PeatMessage {
    /// Get the source platform ID for this message
    pub fn source_node(&self) -> &str {
        match self {
            PeatMessage::Track(t) => &t.source_platform,
            PeatMessage::Capability(c) => &c.platform_id,
            PeatMessage::Handoff(h) => &h.source_cell,
            PeatMessage::FormationSummary(f) => &f.formation_id,
        }
    }

    /// Get the echelon level for filtering decisions
    pub fn echelon(&self) -> EchelonLevel {
        match self {
            PeatMessage::Track(t) => {
                // Determine echelon from cell membership
                if t.cell_id.is_some() {
                    EchelonLevel::Squad
                } else {
                    EchelonLevel::Platform
                }
            }
            PeatMessage::Capability(_) => EchelonLevel::Platform,
            PeatMessage::Handoff(_) => EchelonLevel::Cell,
            PeatMessage::FormationSummary(_) => EchelonLevel::Formation,
        }
    }

    /// Get position if available (for geo filtering)
    pub fn position(&self) -> Option<(f64, f64)> {
        match self {
            PeatMessage::Track(t) => Some((t.position.lat, t.position.lon)),
            PeatMessage::Capability(c) => Some((c.position.lat, c.position.lon)),
            _ => None,
        }
    }
}

/// Bridge metrics
#[derive(Debug, Default)]
pub struct BridgeMetrics {
    /// Messages received from PEAT
    pub messages_received: AtomicU64,
    /// Messages published to TAK
    pub messages_published: AtomicU64,
    /// Messages filtered out
    pub messages_filtered: AtomicU64,
    /// Messages aggregated
    pub messages_aggregated: AtomicU64,
    /// Encoding errors
    pub encoding_errors: AtomicU64,
    /// Transport errors
    pub transport_errors: AtomicU64,
}

impl BridgeMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_published(&self) {
        self.messages_published.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_filtered(&self) {
        self.messages_filtered.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_aggregated(&self) {
        self.messages_aggregated.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_encoding_error(&self) {
        self.encoding_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_transport_error(&self) {
        self.transport_errors.fetch_add(1, Ordering::Relaxed);
    }
}

/// Result of attempting to publish a message
#[derive(Debug, Clone)]
pub enum PublishResult {
    /// Message was published successfully
    Published,
    /// Message was filtered out (with reason)
    Filtered(String),
    /// Message was aggregated (will be sent later)
    Aggregated,
    /// Message failed to encode
    EncodingError(String),
    /// Transport error
    TransportError(String),
}

/// PEAT-TAK Bridge
///
/// Connects Peat mesh network to TAK ecosystem with:
/// - Hierarchical filtering based on echelon
/// - Aggregation policies for bandwidth optimization
/// - QoS-aware priority mapping
/// - Bidirectional message flow
pub struct PeatTakBridge<T: TakTransport> {
    /// Configuration (for future use with extended filtering)
    #[allow(dead_code)]
    config: BridgeConfig,
    /// TAK transport (server or mesh)
    transport: Arc<RwLock<T>>,
    /// CoT encoder
    encoder: CotEncoder,
    /// Aggregator for bandwidth optimization
    aggregator: Aggregator,
    /// Filter for message selection
    filter: BridgeFilter,
    /// Metrics
    metrics: Arc<BridgeMetrics>,
}

impl<T: TakTransport> PeatTakBridge<T> {
    /// Create a new bridge with the given transport
    pub fn new(transport: T, config: BridgeConfig) -> Self {
        let filter = BridgeFilter::from_config(&config);
        let aggregator = Aggregator::new(config.aggregation_policy.clone());

        Self {
            config,
            transport: Arc::new(RwLock::new(transport)),
            encoder: CotEncoder::new(),
            aggregator,
            filter,
            metrics: Arc::new(BridgeMetrics::new()),
        }
    }

    /// Get bridge metrics
    pub fn metrics(&self) -> &BridgeMetrics {
        &self.metrics
    }

    /// Get mutable access to encoder for custom type mappings
    pub fn encoder_mut(&mut self) -> &mut CotEncoder {
        &mut self.encoder
    }

    /// Publish a PEAT message to TAK
    ///
    /// Applies filtering and aggregation based on configuration.
    /// Returns the result of the publish attempt.
    pub async fn publish_to_tak(&self, message: PeatMessage) -> PublishResult {
        self.metrics.record_received();

        // Apply filter
        match self.filter.should_publish(&message) {
            FilterDecision::Publish => {}
            FilterDecision::Drop(reason) => {
                self.metrics.record_filtered();
                debug!("Filtered message: {}", reason);
                return PublishResult::Filtered(reason);
            }
            FilterDecision::Aggregate => {
                self.metrics.record_aggregated();
                self.aggregator.add(message);
                return PublishResult::Aggregated;
            }
        }

        // Encode to CoT
        let (cot_event, priority) = match self.encode_message(&message) {
            Ok(result) => result,
            Err(e) => {
                self.metrics.record_encoding_error();
                return PublishResult::EncodingError(e.to_string());
            }
        };

        // Send via transport
        let transport = self.transport.read().await;
        if let Err(e) = transport.send_cot(&cot_event, priority).await {
            self.metrics.record_transport_error();
            return PublishResult::TransportError(e.to_string());
        }

        self.metrics.record_published();
        PublishResult::Published
    }

    /// Flush aggregated messages to TAK
    ///
    /// Call periodically based on aggregation policy.
    pub async fn flush_aggregated(&self) -> Vec<PublishResult> {
        let messages = self.aggregator.flush();
        let mut results = Vec::with_capacity(messages.len());

        for msg in messages {
            let (cot_event, priority) = match self.encode_message(&msg) {
                Ok(result) => result,
                Err(e) => {
                    self.metrics.record_encoding_error();
                    results.push(PublishResult::EncodingError(e.to_string()));
                    continue;
                }
            };

            let transport = self.transport.read().await;
            if let Err(e) = transport.send_cot(&cot_event, priority).await {
                self.metrics.record_transport_error();
                results.push(PublishResult::TransportError(e.to_string()));
            } else {
                self.metrics.record_published();
                results.push(PublishResult::Published);
            }
        }

        results
    }

    /// Encode a PEAT message to CoT with priority
    fn encode_message(&self, message: &PeatMessage) -> Result<(CotEvent, Priority), TakError> {
        let (event, priority) = match message {
            PeatMessage::Track(track) => {
                let event = self
                    .encoder
                    .track_update_to_event(track)
                    .map_err(|e| TakError::EncodingError(e.to_string()))?;
                // Track priority: P2 (High) for combat tracks, P3 for routine
                let priority = if track.classification.starts_with("a-h") {
                    2 // Hostile tracks are high priority
                } else {
                    3 // Friendly/unknown tracks are normal priority
                };
                (event, priority)
            }
            PeatMessage::Capability(cap) => {
                let event = self
                    .encoder
                    .capability_to_event(cap)
                    .map_err(|e| TakError::EncodingError(e.to_string()))?;
                (event, 4) // Capability updates are lower priority
            }
            PeatMessage::Handoff(handoff) => {
                let event = self
                    .encoder
                    .handoff_to_event(handoff)
                    .map_err(|e| TakError::EncodingError(e.to_string()))?;
                (event, 2) // Handoffs are important for coordination
            }
            PeatMessage::FormationSummary(summary) => {
                let event = self
                    .encoder
                    .formation_summary_to_event(summary)
                    .map_err(|e| TakError::EncodingError(e.to_string()))?;
                (event, 3) // Formation summaries are normal priority
            }
        };

        Ok((event, priority))
    }

    /// Check if transport is connected
    pub async fn is_connected(&self) -> bool {
        self.transport.read().await.is_connected()
    }
}

/// Trait for receiving messages from TAK
#[async_trait]
pub trait TakMessageHandler: Send + Sync {
    /// Handle incoming CoT event from TAK
    async fn handle_cot_event(&self, event: &CotEvent) -> Result<(), TakError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peat_message_echelon() {
        use peat_protocol::cot::Position;

        let track = TrackUpdate {
            track_id: "track-1".to_string(),
            source_platform: "platform-1".to_string(),
            source_model: "test-model".to_string(),
            model_version: "1.0".to_string(),
            cell_id: Some("cell-1".to_string()),
            formation_id: None,
            timestamp: chrono::Utc::now(),
            position: Position::new(34.0, -118.0),
            velocity: None,
            classification: "a-f-G-U-C".to_string(),
            confidence: 0.9,
            attributes: Default::default(),
        };

        let msg = PeatMessage::Track(track);
        assert_eq!(msg.echelon(), EchelonLevel::Squad);
    }

    #[test]
    fn test_bridge_metrics() {
        let metrics = BridgeMetrics::new();
        metrics.record_received();
        metrics.record_published();
        metrics.record_filtered();

        assert_eq!(metrics.messages_received.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.messages_published.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.messages_filtered.load(Ordering::Relaxed), 1);
    }
}
