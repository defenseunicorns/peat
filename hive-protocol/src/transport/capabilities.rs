//! Transport capabilities and multi-transport abstractions
//!
//! This module provides the pluggable transport abstraction layer for supporting
//! multiple transport types (QUIC, Bluetooth LE, LoRa, WiFi Direct, etc.)
//!
//! ## Architecture
//!
//! - **TransportCapabilities**: Declares what a transport can do
//! - **Transport**: Extended trait with capability advertisement
//! - **MessageRequirements**: Requirements for message delivery
//! - **TransportManager**: Coordinates multiple transports
//!
//! ## Design (ADR-032)
//!
//! The design follows a pluggable architecture where:
//! 1. Each transport declares its capabilities (bandwidth, latency, range, power)
//! 2. Messages declare their requirements (reliability, latency, priority)
//! 3. TransportManager selects the best transport for each message
//!
//! ## Example
//!
//! ```ignore
//! use hive_protocol::transport::{TransportManager, MessageRequirements, MessagePriority};
//!
//! // Register transports
//! let mut manager = TransportManager::new(config);
//! manager.register(quic_transport);
//! manager.register(ble_transport);
//!
//! // Send with requirements
//! let requirements = MessageRequirements {
//!     reliable: true,
//!     priority: MessagePriority::High,
//!     ..Default::default()
//! };
//! manager.send(&peer_id, &data, requirements).await?;
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Instant;

use super::{MeshTransport, NodeId, Result, TransportError};

// =============================================================================
// Transport Type
// =============================================================================

/// Type of transport technology
///
/// Used to identify and categorize transports for selection and configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportType {
    /// QUIC over IP (Iroh) - primary mesh transport
    Quic,
    /// Classic Bluetooth (RFCOMM)
    BluetoothClassic,
    /// Bluetooth Low Energy (GATT)
    BluetoothLE,
    /// WiFi Direct (P2P)
    WifiDirect,
    /// LoRa (long range, low power)
    LoRa,
    /// Tactical radio (MANET)
    TacticalRadio,
    /// Satellite (Starlink, Iridium)
    Satellite,
    /// Custom/vendor-specific transport
    Custom(u32),
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportType::Quic => write!(f, "QUIC"),
            TransportType::BluetoothClassic => write!(f, "Bluetooth Classic"),
            TransportType::BluetoothLE => write!(f, "Bluetooth LE"),
            TransportType::WifiDirect => write!(f, "WiFi Direct"),
            TransportType::LoRa => write!(f, "LoRa"),
            TransportType::TacticalRadio => write!(f, "Tactical Radio"),
            TransportType::Satellite => write!(f, "Satellite"),
            TransportType::Custom(id) => write!(f, "Custom({})", id),
        }
    }
}

// =============================================================================
// Transport Capabilities
// =============================================================================

/// Declares the capabilities of a transport
///
/// Each transport advertises what it can do, allowing the TransportManager
/// to select the best transport for each message based on requirements.
///
/// # Example
///
/// ```
/// use hive_protocol::transport::{TransportCapabilities, TransportType};
///
/// let quic_caps = TransportCapabilities {
///     transport_type: TransportType::Quic,
///     max_bandwidth_bps: 100_000_000,  // 100 Mbps
///     typical_latency_ms: 10,
///     max_range_meters: 0,  // Unlimited (IP)
///     bidirectional: true,
///     reliable: true,
///     battery_impact: 20,
///     supports_broadcast: false,
///     requires_pairing: false,
///     max_message_size: 0,  // Unlimited (stream-based)
/// };
/// ```
#[derive(Debug, Clone)]
pub struct TransportCapabilities {
    /// Transport type identifier
    pub transport_type: TransportType,

    /// Maximum bandwidth in bytes/second (0 = unknown/unlimited)
    pub max_bandwidth_bps: u64,

    /// Typical latency in milliseconds
    pub typical_latency_ms: u32,

    /// Maximum practical range in meters (0 = unlimited/IP-based)
    pub max_range_meters: u32,

    /// Supports bidirectional communication
    pub bidirectional: bool,

    /// Supports reliable delivery (vs best-effort)
    pub reliable: bool,

    /// Battery impact score (0-100, higher = more power consumption)
    pub battery_impact: u8,

    /// Supports broadcast/multicast
    pub supports_broadcast: bool,

    /// Requires pairing/bonding before use
    pub requires_pairing: bool,

    /// Maximum message size in bytes (0 = unlimited/stream-based)
    pub max_message_size: usize,
}

impl TransportCapabilities {
    /// Create capabilities for QUIC/Iroh transport
    pub fn quic() -> Self {
        Self {
            transport_type: TransportType::Quic,
            max_bandwidth_bps: 100_000_000, // ~100 Mbps typical
            typical_latency_ms: 10,
            max_range_meters: 0, // Unlimited (IP-based)
            bidirectional: true,
            reliable: true,
            battery_impact: 20,
            supports_broadcast: false,
            requires_pairing: false,
            max_message_size: 0, // Unlimited (stream-based)
        }
    }

    /// Create capabilities for Bluetooth LE transport
    pub fn bluetooth_le() -> Self {
        Self {
            transport_type: TransportType::BluetoothLE,
            max_bandwidth_bps: 250_000, // ~2 Mbps theoretical, ~250 KB/s practical
            typical_latency_ms: 30,
            max_range_meters: 100,
            bidirectional: true,
            reliable: true,
            battery_impact: 15,       // BLE is efficient
            supports_broadcast: true, // Advertising
            requires_pairing: false,  // Can use just-works
            max_message_size: 512,    // MTU limit
        }
    }

    /// Create capabilities for LoRa transport with given spreading factor
    pub fn lora(spreading_factor: u8) -> Self {
        let (bandwidth, range, latency) = match spreading_factor {
            7 => (21_900, 6_000, 100),
            8 => (12_500, 8_000, 150),
            9 => (7_000, 10_000, 200),
            10 => (3_900, 12_000, 300),
            11 => (2_100, 14_000, 500),
            12 => (1_100, 15_000, 1000),
            _ => (5_000, 10_000, 300), // Default
        };

        Self {
            transport_type: TransportType::LoRa,
            max_bandwidth_bps: bandwidth,
            typical_latency_ms: latency,
            max_range_meters: range,
            bidirectional: true,
            reliable: false, // Best-effort by default
            battery_impact: 10,
            supports_broadcast: true,
            requires_pairing: false,
            max_message_size: 255, // LoRa packet limit
        }
    }

    /// Create capabilities for WiFi Direct transport
    pub fn wifi_direct() -> Self {
        Self {
            transport_type: TransportType::WifiDirect,
            max_bandwidth_bps: 250_000_000, // ~250 Mbps
            typical_latency_ms: 10,
            max_range_meters: 200,
            bidirectional: true,
            reliable: true,
            battery_impact: 50, // WiFi uses more power
            supports_broadcast: true,
            requires_pairing: true, // GO negotiation required
            max_message_size: 0,    // Unlimited (TCP/UDP)
        }
    }

    /// Check if this transport can meet the given requirements
    pub fn meets_requirements(&self, requirements: &MessageRequirements) -> bool {
        // Check reliability requirement
        if requirements.reliable && !self.reliable {
            return false;
        }

        // Check bandwidth requirement
        if self.max_bandwidth_bps > 0 && self.max_bandwidth_bps < requirements.min_bandwidth_bps {
            return false;
        }

        // Check message size
        if self.max_message_size > 0 && self.max_message_size < requirements.message_size {
            return false;
        }

        true
    }

    /// Estimate delivery time for a message of given size
    pub fn estimate_delivery_ms(&self, message_size: usize) -> u32 {
        let transfer_time = if self.max_bandwidth_bps > 0 {
            (message_size as u64 * 1000 / self.max_bandwidth_bps) as u32
        } else {
            0
        };
        self.typical_latency_ms + transfer_time
    }
}

impl Default for TransportCapabilities {
    fn default() -> Self {
        Self::quic()
    }
}

// =============================================================================
// Message Requirements
// =============================================================================

/// Priority level for message delivery
///
/// Higher priority messages will be routed via faster/more reliable transports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub enum MessagePriority {
    /// Background sync, can use any available transport
    Background = 0,
    /// Normal operational messages
    #[default]
    Normal = 1,
    /// Time-sensitive, prefer low-latency transports
    High = 2,
    /// Emergency/critical, use fastest available path
    Critical = 3,
}

impl std::fmt::Display for MessagePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessagePriority::Background => write!(f, "background"),
            MessagePriority::Normal => write!(f, "normal"),
            MessagePriority::High => write!(f, "high"),
            MessagePriority::Critical => write!(f, "critical"),
        }
    }
}

/// Requirements for message delivery
///
/// Used by TransportManager to select the best transport for a message.
///
/// # Example
///
/// ```
/// use hive_protocol::transport::{MessageRequirements, MessagePriority};
///
/// // High-priority reliable message
/// let requirements = MessageRequirements {
///     reliable: true,
///     priority: MessagePriority::High,
///     max_latency_ms: Some(100),
///     message_size: 1024,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct MessageRequirements {
    /// Minimum required bandwidth (bytes/second)
    pub min_bandwidth_bps: u64,

    /// Maximum acceptable latency (ms)
    pub max_latency_ms: Option<u32>,

    /// Message size in bytes (for capacity checking)
    pub message_size: usize,

    /// Requires reliable delivery
    pub reliable: bool,

    /// Priority level (higher = more important)
    pub priority: MessagePriority,

    /// Prefer low power consumption
    pub power_sensitive: bool,
}

// =============================================================================
// Range Mode (Dynamic Range/Bandwidth Tradeoff)
// =============================================================================

/// Available range modes for configurable transports
///
/// Many radio technologies allow trading bandwidth for range. This enum
/// represents standard operating modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RangeMode {
    /// Default/balanced mode
    #[default]
    Standard,
    /// Extended range at cost of bandwidth
    Extended,
    /// Maximum range (lowest bandwidth)
    Maximum,
    /// Custom configuration (transport-specific value)
    Custom(u8),
}

impl std::fmt::Display for RangeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RangeMode::Standard => write!(f, "standard"),
            RangeMode::Extended => write!(f, "extended"),
            RangeMode::Maximum => write!(f, "maximum"),
            RangeMode::Custom(val) => write!(f, "custom({})", val),
        }
    }
}

/// Range mode configuration for a transport
#[derive(Debug, Clone)]
pub struct RangeModeConfig {
    /// Available modes for this transport
    pub available_modes: Vec<RangeMode>,
    /// Current active mode
    pub current_mode: RangeMode,
    /// Capabilities per mode
    pub mode_capabilities: HashMap<RangeMode, TransportCapabilities>,
}

impl RangeModeConfig {
    /// Create a new range mode configuration
    pub fn new(modes: Vec<(RangeMode, TransportCapabilities)>) -> Self {
        let available_modes: Vec<_> = modes.iter().map(|(m, _)| *m).collect();
        let current_mode = available_modes
            .first()
            .copied()
            .unwrap_or(RangeMode::Standard);
        let mode_capabilities = modes.into_iter().collect();

        Self {
            available_modes,
            current_mode,
            mode_capabilities,
        }
    }

    /// Get capabilities for the current mode
    pub fn current_capabilities(&self) -> Option<&TransportCapabilities> {
        self.mode_capabilities.get(&self.current_mode)
    }

    /// Find the best mode for a target distance
    pub fn recommend_for_distance(&self, distance_meters: u32) -> Option<RangeMode> {
        // Find mode with sufficient range and best bandwidth
        self.mode_capabilities
            .iter()
            .filter(|(_, caps)| {
                caps.max_range_meters >= distance_meters || caps.max_range_meters == 0
            })
            .max_by_key(|(_, caps)| caps.max_bandwidth_bps)
            .map(|(mode, _)| *mode)
    }
}

// =============================================================================
// Distance Estimation
// =============================================================================

/// How peer distance was determined
#[derive(Debug, Clone)]
pub enum DistanceSource {
    /// GPS coordinates from both peers
    Gps {
        /// Confidence in meters
        confidence_meters: u32,
    },
    /// Signal strength (RSSI) estimation
    Rssi {
        /// Estimated distance
        estimated_meters: u32,
        /// Variance in estimate
        variance: u32,
    },
    /// Time-of-flight measurement
    Tof {
        /// Measurement precision in nanoseconds
        precision_ns: u32,
    },
    /// Manual/configured distance
    Configured,
    /// Unknown distance
    Unknown,
}

/// Peer distance information
#[derive(Debug, Clone)]
pub struct PeerDistance {
    /// Peer node ID
    pub peer_id: NodeId,
    /// Estimated distance in meters
    pub distance_meters: u32,
    /// How distance was determined
    pub source: DistanceSource,
    /// When this estimate was made
    pub last_updated: Instant,
}

// =============================================================================
// Transport Trait (Extended)
// =============================================================================

/// Extended transport trait with capability advertisement
///
/// This trait extends `MeshTransport` with capability declaration and
/// selection support. All pluggable transports should implement this.
///
/// # Example Implementation
///
/// ```ignore
/// impl Transport for MyTransport {
///     fn capabilities(&self) -> &TransportCapabilities {
///         &self.caps
///     }
///
///     fn is_available(&self) -> bool {
///         self.hardware.is_ready()
///     }
///
///     fn can_reach(&self, peer_id: &NodeId) -> bool {
///         self.known_peers.contains(peer_id)
///     }
/// }
/// ```
#[async_trait]
pub trait Transport: MeshTransport {
    /// Get transport capabilities
    fn capabilities(&self) -> &TransportCapabilities;

    /// Check if transport is currently available/enabled
    fn is_available(&self) -> bool;

    /// Get current signal quality (0-100, for wireless transports)
    ///
    /// Returns `None` for wired/IP transports.
    fn signal_quality(&self) -> Option<u8> {
        None
    }

    /// Estimate if peer is reachable via this transport
    fn can_reach(&self, peer_id: &NodeId) -> bool;

    /// Get estimated delivery time for message of given size
    fn estimate_delivery_ms(&self, message_size: usize) -> u32 {
        self.capabilities().estimate_delivery_ms(message_size)
    }

    /// Calculate selection score for this transport
    ///
    /// Higher scores are better. Used by TransportManager for selection.
    fn calculate_score(&self, requirements: &MessageRequirements, preference_bonus: i32) -> i32 {
        let caps = self.capabilities();
        let mut score = 100i32;

        // Latency bonus for high-priority messages
        if requirements.priority >= MessagePriority::High {
            score += 50 - (caps.typical_latency_ms.min(50) as i32);
        }

        // Power penalty if power-sensitive
        if requirements.power_sensitive {
            score -= caps.battery_impact as i32;
        }

        // Add preference bonus
        score += preference_bonus;

        // Signal quality bonus for wireless
        if let Some(quality) = self.signal_quality() {
            score += (quality / 10) as i32;
        }

        score
    }
}

/// Extended transport trait with range mode configuration
///
/// Transports that support dynamic range/bandwidth tradeoffs should
/// implement this trait.
#[async_trait]
pub trait ConfigurableTransport: Transport {
    /// Get available range modes
    fn range_modes(&self) -> Option<&RangeModeConfig> {
        None
    }

    /// Set range mode (returns new capabilities)
    async fn set_range_mode(&self, _mode: RangeMode) -> Result<TransportCapabilities> {
        Err(TransportError::Other(
            "Range mode not supported".to_string().into(),
        ))
    }

    /// Get recommended mode for target distance
    fn recommend_mode_for_distance(&self, distance_meters: u32) -> Option<RangeMode> {
        self.range_modes()?.recommend_for_distance(distance_meters)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_type_display() {
        assert_eq!(TransportType::Quic.to_string(), "QUIC");
        assert_eq!(TransportType::BluetoothLE.to_string(), "Bluetooth LE");
        assert_eq!(TransportType::LoRa.to_string(), "LoRa");
        assert_eq!(TransportType::Custom(42).to_string(), "Custom(42)");
    }

    #[test]
    fn test_quic_capabilities() {
        let caps = TransportCapabilities::quic();
        assert_eq!(caps.transport_type, TransportType::Quic);
        assert!(caps.reliable);
        assert!(caps.bidirectional);
        assert_eq!(caps.max_range_meters, 0); // Unlimited
    }

    #[test]
    fn test_ble_capabilities() {
        let caps = TransportCapabilities::bluetooth_le();
        assert_eq!(caps.transport_type, TransportType::BluetoothLE);
        assert_eq!(caps.max_range_meters, 100);
        assert_eq!(caps.max_message_size, 512);
        assert!(caps.supports_broadcast);
    }

    #[test]
    fn test_lora_capabilities() {
        let caps_sf7 = TransportCapabilities::lora(7);
        let caps_sf12 = TransportCapabilities::lora(12);

        // SF7 has more bandwidth but less range
        assert!(caps_sf7.max_bandwidth_bps > caps_sf12.max_bandwidth_bps);
        assert!(caps_sf7.max_range_meters < caps_sf12.max_range_meters);
    }

    #[test]
    fn test_meets_requirements_reliable() {
        let caps = TransportCapabilities::lora(7);
        assert!(!caps.reliable);

        let requirements = MessageRequirements {
            reliable: true,
            ..Default::default()
        };

        assert!(!caps.meets_requirements(&requirements));
    }

    #[test]
    fn test_meets_requirements_bandwidth() {
        let caps = TransportCapabilities::lora(12); // ~1.1 kbps

        let low_bandwidth = MessageRequirements {
            min_bandwidth_bps: 500,
            ..Default::default()
        };

        let high_bandwidth = MessageRequirements {
            min_bandwidth_bps: 1_000_000,
            ..Default::default()
        };

        assert!(caps.meets_requirements(&low_bandwidth));
        assert!(!caps.meets_requirements(&high_bandwidth));
    }

    #[test]
    fn test_meets_requirements_message_size() {
        let caps = TransportCapabilities::lora(7); // 255 byte limit

        let small_message = MessageRequirements {
            message_size: 100,
            ..Default::default()
        };

        let large_message = MessageRequirements {
            message_size: 1000,
            ..Default::default()
        };

        assert!(caps.meets_requirements(&small_message));
        assert!(!caps.meets_requirements(&large_message));
    }

    #[test]
    fn test_estimate_delivery_ms() {
        let caps = TransportCapabilities::quic();
        // 1MB message at 100 Mbps = ~80ms transfer + 10ms latency
        let estimate = caps.estimate_delivery_ms(1_000_000);
        assert!(estimate >= 10);
        assert!(estimate < 200);
    }

    #[test]
    fn test_message_priority_ordering() {
        assert!(MessagePriority::Critical > MessagePriority::High);
        assert!(MessagePriority::High > MessagePriority::Normal);
        assert!(MessagePriority::Normal > MessagePriority::Background);
    }

    #[test]
    fn test_range_mode_config() {
        let modes = vec![
            (RangeMode::Standard, TransportCapabilities::bluetooth_le()),
            (
                RangeMode::Extended,
                TransportCapabilities {
                    max_bandwidth_bps: 125_000,
                    max_range_meters: 200,
                    ..TransportCapabilities::bluetooth_le()
                },
            ),
            (
                RangeMode::Maximum,
                TransportCapabilities {
                    max_bandwidth_bps: 62_500,
                    max_range_meters: 400,
                    ..TransportCapabilities::bluetooth_le()
                },
            ),
        ];

        let config = RangeModeConfig::new(modes);

        // Should recommend Standard for short range
        assert_eq!(config.recommend_for_distance(50), Some(RangeMode::Standard));

        // Should recommend Extended for medium range
        assert_eq!(
            config.recommend_for_distance(150),
            Some(RangeMode::Extended)
        );

        // Should recommend Maximum for long range
        assert_eq!(config.recommend_for_distance(300), Some(RangeMode::Maximum));
    }

    #[test]
    fn test_distance_source() {
        let gps = DistanceSource::Gps {
            confidence_meters: 10,
        };
        let rssi = DistanceSource::Rssi {
            estimated_meters: 50,
            variance: 20,
        };

        // Just ensure these compile and can be debugged
        let _ = format!("{:?}", gps);
        let _ = format!("{:?}", rssi);
    }
}
