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
//! use hive_mesh::transport::{TransportManager, MessageRequirements, MessagePriority};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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
/// use hive_mesh::transport::{TransportCapabilities, TransportType};
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
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
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
/// use hive_mesh::transport::{MessageRequirements, MessagePriority};
///
/// // High-priority reliable message
/// let requirements = MessageRequirements {
///     reliable: true,
///     priority: MessagePriority::High,
///     max_latency_ms: Some(100),
///     message_size: 1024,
///     ..Default::default()
/// };
///
/// // Bypass message for low-latency delivery
/// let bypass_req = MessageRequirements {
///     bypass_sync: true,
///     reliable: false,  // UDP is unreliable
///     max_latency_ms: Some(5),
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

    /// Use UDP bypass channel instead of CRDT sync (ADR-042)
    ///
    /// When `true`, the message should be sent via the low-latency UDP
    /// bypass channel instead of the normal CRDT synchronization path.
    /// This bypasses:
    /// - Automerge encoding/decoding
    /// - CRDT conflict resolution
    /// - Iroh/QUIC transport overhead
    ///
    /// Use for ephemeral data like position updates, sensor telemetry,
    /// and time-critical commands.
    pub bypass_sync: bool,

    /// Time-to-live for bypass messages
    ///
    /// Messages older than this will be dropped by receivers.
    /// Only applies when `bypass_sync` is `true`.
    /// Default: None (use collection config or 5 seconds)
    pub ttl: Option<std::time::Duration>,
}

impl MessageRequirements {
    /// Create requirements for bypass mode with specified latency
    pub fn bypass(max_latency_ms: u32) -> Self {
        Self {
            bypass_sync: true,
            reliable: false,
            max_latency_ms: Some(max_latency_ms),
            ..Default::default()
        }
    }

    /// Create requirements for bypass mode with TTL
    pub fn bypass_with_ttl(max_latency_ms: u32, ttl: std::time::Duration) -> Self {
        Self {
            bypass_sync: true,
            reliable: false,
            max_latency_ms: Some(max_latency_ms),
            ttl: Some(ttl),
            ..Default::default()
        }
    }

    /// Set bypass_sync flag
    pub fn with_bypass(mut self, bypass: bool) -> Self {
        self.bypass_sync = bypass;
        if bypass {
            self.reliable = false; // UDP bypass is unreliable
        }
        self
    }

    /// Set TTL for bypass messages
    pub fn with_ttl(mut self, ttl: std::time::Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }
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
// PACE Transport Policy (ADR-032)
// =============================================================================

/// Unique identifier for a transport instance
///
/// Format: "{type}-{interface}" or custom string
/// Examples: "iroh-eth0", "iroh-wlan0", "lora-915mhz", "ble-hci0"
pub type TransportId = String;

/// Transport instance metadata
///
/// Represents a registered transport with its unique ID and current state.
#[derive(Debug, Clone)]
pub struct TransportInstance {
    /// Unique instance identifier
    pub id: TransportId,
    /// Transport type (for capability grouping)
    pub transport_type: TransportType,
    /// Human-readable description
    pub description: String,
    /// Physical interface name (if applicable)
    pub interface: Option<String>,
    /// Current capabilities (may change with range mode)
    pub capabilities: TransportCapabilities,
    /// Is this transport currently available?
    pub available: bool,
}

impl TransportInstance {
    /// Create a new transport instance
    pub fn new(
        id: impl Into<String>,
        transport_type: TransportType,
        capabilities: TransportCapabilities,
    ) -> Self {
        Self {
            id: id.into(),
            transport_type,
            description: String::new(),
            interface: None,
            capabilities,
            available: true,
        }
    }

    /// Set the description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set the interface name
    pub fn with_interface(mut self, iface: impl Into<String>) -> Self {
        self.interface = Some(iface.into());
        self
    }
}

/// PACE level for transport availability
///
/// Military PACE planning: Primary, Alternate, Contingency, Emergency
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum PaceLevel {
    /// Primary transports available
    #[default]
    Primary = 0,
    /// Alternate transports (primary unavailable)
    Alternate = 1,
    /// Contingency transports (degraded operation)
    Contingency = 2,
    /// Emergency transports (last resort)
    Emergency = 3,
    /// No transports available
    None = 4,
}

impl std::fmt::Display for PaceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaceLevel::Primary => write!(f, "PRIMARY"),
            PaceLevel::Alternate => write!(f, "ALTERNATE"),
            PaceLevel::Contingency => write!(f, "CONTINGENCY"),
            PaceLevel::Emergency => write!(f, "EMERGENCY"),
            PaceLevel::None => write!(f, "NONE"),
        }
    }
}

/// PACE-style transport policy
///
/// Defines transport selection order following military PACE planning:
/// Primary → Alternate → Contingency → Emergency
///
/// # Example
///
/// ```
/// use hive_mesh::transport::TransportPolicy;
///
/// let policy = TransportPolicy::new("tactical-standard")
///     .primary(vec!["iroh-eth0", "iroh-wlan0"])
///     .alternate(vec!["iroh-starlink"])
///     .contingency(vec!["lora-primary"])
///     .emergency(vec!["ble-mesh"]);
/// ```
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TransportPolicy {
    /// Policy name for reference
    pub name: String,
    /// Primary transports - use when available
    pub primary: Vec<TransportId>,
    /// Alternate - if all primary unavailable
    pub alternate: Vec<TransportId>,
    /// Contingency - degraded but functional
    pub contingency: Vec<TransportId>,
    /// Emergency - last resort
    pub emergency: Vec<TransportId>,
}

impl TransportPolicy {
    /// Create a new transport policy
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set primary transports
    pub fn primary(mut self, transports: Vec<impl Into<TransportId>>) -> Self {
        self.primary = transports.into_iter().map(Into::into).collect();
        self
    }

    /// Set alternate transports
    pub fn alternate(mut self, transports: Vec<impl Into<TransportId>>) -> Self {
        self.alternate = transports.into_iter().map(Into::into).collect();
        self
    }

    /// Set contingency transports
    pub fn contingency(mut self, transports: Vec<impl Into<TransportId>>) -> Self {
        self.contingency = transports.into_iter().map(Into::into).collect();
        self
    }

    /// Set emergency transports
    pub fn emergency(mut self, transports: Vec<impl Into<TransportId>>) -> Self {
        self.emergency = transports.into_iter().map(Into::into).collect();
        self
    }

    /// Get transports in PACE order (primary first, then alternate, etc.)
    pub fn ordered(&self) -> impl Iterator<Item = &TransportId> {
        self.primary
            .iter()
            .chain(self.alternate.iter())
            .chain(self.contingency.iter())
            .chain(self.emergency.iter())
    }

    /// Get current PACE level based on available transports
    pub fn current_level(&self, available: &std::collections::HashSet<TransportId>) -> PaceLevel {
        if self.primary.iter().any(|t| available.contains(t)) {
            PaceLevel::Primary
        } else if self.alternate.iter().any(|t| available.contains(t)) {
            PaceLevel::Alternate
        } else if self.contingency.iter().any(|t| available.contains(t)) {
            PaceLevel::Contingency
        } else if self.emergency.iter().any(|t| available.contains(t)) {
            PaceLevel::Emergency
        } else {
            PaceLevel::None
        }
    }

    /// Get available transports at or above a minimum PACE level
    pub fn at_level(&self, level: PaceLevel) -> Vec<&TransportId> {
        match level {
            PaceLevel::Primary => self.primary.iter().collect(),
            PaceLevel::Alternate => self.primary.iter().chain(self.alternate.iter()).collect(),
            PaceLevel::Contingency => self
                .primary
                .iter()
                .chain(self.alternate.iter())
                .chain(self.contingency.iter())
                .collect(),
            PaceLevel::Emergency | PaceLevel::None => self.ordered().collect(),
        }
    }
}

/// How to use multiple available transports
#[derive(Debug, Clone, Default)]
pub enum TransportMode {
    /// Use single best transport from policy (PACE failover)
    #[default]
    Single,

    /// Send on multiple transports simultaneously for reliability
    /// Receiver deduplicates by message ID
    Redundant {
        /// Minimum transports to send on (default: 2)
        min_paths: u8,
        /// Maximum transports to send on (None = all available)
        max_paths: Option<u8>,
    },

    /// Aggregate bandwidth across transports (for large transfers)
    /// Splits message across transports, receiver reassembles
    Bonded,

    /// Distribute messages across transports (round-robin or weighted)
    LoadBalanced {
        /// Weight per transport (higher = more traffic)
        weights: Option<HashMap<TransportId, u8>>,
    },
}

impl TransportMode {
    /// Create redundant mode with minimum paths
    pub fn redundant(min_paths: u8) -> Self {
        Self::Redundant {
            min_paths,
            max_paths: None,
        }
    }

    /// Create redundant mode with min and max paths
    pub fn redundant_bounded(min_paths: u8, max_paths: u8) -> Self {
        Self::Redundant {
            min_paths,
            max_paths: Some(max_paths),
        }
    }

    /// Create load balanced mode with weights
    pub fn load_balanced_weighted(weights: HashMap<TransportId, u8>) -> Self {
        Self::LoadBalanced {
            weights: Some(weights),
        }
    }
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

    #[test]
    fn test_message_requirements_bypass() {
        let req = MessageRequirements::bypass(5);
        assert!(req.bypass_sync);
        assert!(!req.reliable);
        assert_eq!(req.max_latency_ms, Some(5));
    }

    #[test]
    fn test_message_requirements_bypass_with_ttl() {
        use std::time::Duration;

        let req = MessageRequirements::bypass_with_ttl(5, Duration::from_millis(200));
        assert!(req.bypass_sync);
        assert!(!req.reliable);
        assert_eq!(req.max_latency_ms, Some(5));
        assert_eq!(req.ttl, Some(Duration::from_millis(200)));
    }

    #[test]
    fn test_message_requirements_builder() {
        use std::time::Duration;

        let req = MessageRequirements::default()
            .with_bypass(true)
            .with_ttl(Duration::from_secs(1));

        assert!(req.bypass_sync);
        assert!(!req.reliable); // auto-set to false when bypass=true
        assert_eq!(req.ttl, Some(Duration::from_secs(1)));
    }

    #[test]
    fn test_message_requirements_default() {
        let req = MessageRequirements::default();
        assert!(!req.bypass_sync);
        assert!(req.ttl.is_none());
    }

    // =========================================================================
    // PACE Policy Tests (ADR-032)
    // =========================================================================

    #[test]
    fn test_transport_instance_creation() {
        let instance = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        )
        .with_description("Primary ethernet")
        .with_interface("eth0");

        assert_eq!(instance.id, "iroh-eth0");
        assert_eq!(instance.transport_type, TransportType::Quic);
        assert_eq!(instance.description, "Primary ethernet");
        assert_eq!(instance.interface, Some("eth0".to_string()));
        assert!(instance.available);
    }

    #[test]
    fn test_pace_level_ordering() {
        assert!(PaceLevel::Primary < PaceLevel::Alternate);
        assert!(PaceLevel::Alternate < PaceLevel::Contingency);
        assert!(PaceLevel::Contingency < PaceLevel::Emergency);
        assert!(PaceLevel::Emergency < PaceLevel::None);
    }

    #[test]
    fn test_pace_level_display() {
        assert_eq!(PaceLevel::Primary.to_string(), "PRIMARY");
        assert_eq!(PaceLevel::Alternate.to_string(), "ALTERNATE");
        assert_eq!(PaceLevel::Contingency.to_string(), "CONTINGENCY");
        assert_eq!(PaceLevel::Emergency.to_string(), "EMERGENCY");
        assert_eq!(PaceLevel::None.to_string(), "NONE");
    }

    #[test]
    fn test_transport_policy_builder() {
        let policy = TransportPolicy::new("tactical-standard")
            .primary(vec!["iroh-eth0", "iroh-wlan0"])
            .alternate(vec!["iroh-starlink"])
            .contingency(vec!["lora-primary"])
            .emergency(vec!["ble-mesh"]);

        assert_eq!(policy.name, "tactical-standard");
        assert_eq!(policy.primary.len(), 2);
        assert_eq!(policy.alternate.len(), 1);
        assert_eq!(policy.contingency.len(), 1);
        assert_eq!(policy.emergency.len(), 1);
    }

    #[test]
    fn test_transport_policy_ordered() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["p1", "p2"])
            .alternate(vec!["a1"])
            .contingency(vec!["c1"])
            .emergency(vec!["e1"]);

        let ordered: Vec<_> = policy.ordered().collect();
        assert_eq!(ordered, vec!["p1", "p2", "a1", "c1", "e1"]);
    }

    #[test]
    fn test_transport_policy_current_level() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["p1", "p2"])
            .alternate(vec!["a1"])
            .contingency(vec!["c1"])
            .emergency(vec!["e1"]);

        // Primary available
        let mut available = std::collections::HashSet::new();
        available.insert("p1".to_string());
        assert_eq!(policy.current_level(&available), PaceLevel::Primary);

        // Only alternate available
        available.clear();
        available.insert("a1".to_string());
        assert_eq!(policy.current_level(&available), PaceLevel::Alternate);

        // Only contingency available
        available.clear();
        available.insert("c1".to_string());
        assert_eq!(policy.current_level(&available), PaceLevel::Contingency);

        // Only emergency available
        available.clear();
        available.insert("e1".to_string());
        assert_eq!(policy.current_level(&available), PaceLevel::Emergency);

        // Nothing available
        available.clear();
        assert_eq!(policy.current_level(&available), PaceLevel::None);
    }

    #[test]
    fn test_transport_policy_at_level() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["p1"])
            .alternate(vec!["a1"])
            .contingency(vec!["c1"])
            .emergency(vec!["e1"]);

        // At Primary level - only primary
        assert_eq!(policy.at_level(PaceLevel::Primary).len(), 1);

        // At Alternate level - primary + alternate
        assert_eq!(policy.at_level(PaceLevel::Alternate).len(), 2);

        // At Contingency level - primary + alternate + contingency
        assert_eq!(policy.at_level(PaceLevel::Contingency).len(), 3);

        // At Emergency level - all
        assert_eq!(policy.at_level(PaceLevel::Emergency).len(), 4);
    }

    #[test]
    fn test_transport_mode_single() {
        let mode = TransportMode::Single;
        assert!(matches!(mode, TransportMode::Single));
    }

    #[test]
    fn test_transport_mode_redundant() {
        let mode = TransportMode::redundant(2);
        assert!(matches!(
            mode,
            TransportMode::Redundant {
                min_paths: 2,
                max_paths: None
            }
        ));

        let bounded = TransportMode::redundant_bounded(2, 4);
        assert!(matches!(
            bounded,
            TransportMode::Redundant {
                min_paths: 2,
                max_paths: Some(4)
            }
        ));
    }

    #[test]
    fn test_wifi_direct_capabilities() {
        let caps = TransportCapabilities::wifi_direct();
        assert_eq!(caps.transport_type, TransportType::WifiDirect);
        assert_eq!(caps.max_range_meters, 200);
        assert!(caps.reliable);
        assert!(caps.supports_broadcast);
        assert!(caps.requires_pairing);
        assert_eq!(caps.battery_impact, 50);
    }

    #[test]
    fn test_lora_all_spreading_factors() {
        // Test all known spreading factors
        for sf in [7, 8, 9, 10, 11, 12] {
            let caps = TransportCapabilities::lora(sf);
            assert_eq!(caps.transport_type, TransportType::LoRa);
            assert!(!caps.reliable);
            assert!(caps.supports_broadcast);
            assert_eq!(caps.max_message_size, 255);
        }

        // Test default/unknown SF
        let caps_default = TransportCapabilities::lora(6);
        assert_eq!(caps_default.max_bandwidth_bps, 5_000);
        assert_eq!(caps_default.max_range_meters, 10_000);
    }

    #[test]
    fn test_transport_capabilities_default() {
        let caps = TransportCapabilities::default();
        assert_eq!(caps.transport_type, TransportType::Quic);
    }

    #[test]
    fn test_transport_type_display_all() {
        assert_eq!(
            TransportType::BluetoothClassic.to_string(),
            "Bluetooth Classic"
        );
        assert_eq!(TransportType::WifiDirect.to_string(), "WiFi Direct");
        assert_eq!(TransportType::TacticalRadio.to_string(), "Tactical Radio");
        assert_eq!(TransportType::Satellite.to_string(), "Satellite");
    }

    #[test]
    fn test_range_mode_display() {
        assert_eq!(RangeMode::Standard.to_string(), "standard");
        assert_eq!(RangeMode::Extended.to_string(), "extended");
        assert_eq!(RangeMode::Maximum.to_string(), "maximum");
        assert_eq!(RangeMode::Custom(42).to_string(), "custom(42)");
    }

    #[test]
    fn test_range_mode_default() {
        assert_eq!(RangeMode::default(), RangeMode::Standard);
    }

    #[test]
    fn test_message_priority_display() {
        assert_eq!(MessagePriority::Background.to_string(), "background");
        assert_eq!(MessagePriority::Normal.to_string(), "normal");
        assert_eq!(MessagePriority::High.to_string(), "high");
        assert_eq!(MessagePriority::Critical.to_string(), "critical");
    }

    #[test]
    fn test_message_priority_default() {
        assert_eq!(MessagePriority::default(), MessagePriority::Normal);
    }

    #[test]
    fn test_pace_level_default() {
        assert_eq!(PaceLevel::default(), PaceLevel::Primary);
    }

    #[test]
    fn test_transport_mode_default() {
        let mode = TransportMode::default();
        assert!(matches!(mode, TransportMode::Single));
    }

    #[test]
    fn test_transport_mode_bonded() {
        let mode = TransportMode::Bonded;
        let _ = format!("{:?}", mode);
    }

    #[test]
    fn test_distance_source_all_variants() {
        let tof = DistanceSource::Tof { precision_ns: 100 };
        let configured = DistanceSource::Configured;
        let unknown = DistanceSource::Unknown;
        let _ = format!("{:?}", tof);
        let _ = format!("{:?}", configured);
        let _ = format!("{:?}", unknown);
    }

    #[test]
    fn test_peer_distance_construction() {
        let pd = PeerDistance {
            peer_id: NodeId::new("test-peer".to_string()),
            distance_meters: 500,
            source: DistanceSource::Gps {
                confidence_meters: 5,
            },
            last_updated: Instant::now(),
        };
        assert_eq!(pd.distance_meters, 500);
        let _ = format!("{:?}", pd);
    }

    #[test]
    fn test_range_mode_config_current_capabilities() {
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
        ];
        let config = RangeModeConfig::new(modes);
        let caps = config.current_capabilities();
        assert!(caps.is_some());
        assert_eq!(caps.unwrap().transport_type, TransportType::BluetoothLE);
    }

    #[test]
    fn test_range_mode_config_no_match_for_distance() {
        let modes = vec![(
            RangeMode::Standard,
            TransportCapabilities {
                max_range_meters: 50,
                ..TransportCapabilities::bluetooth_le()
            },
        )];
        let config = RangeModeConfig::new(modes);
        // 1000m exceeds the only mode's 50m range
        let result = config.recommend_for_distance(1000);
        assert!(result.is_none());
    }

    #[test]
    fn test_with_bypass_false() {
        let req = MessageRequirements::default().with_bypass(false);
        assert!(!req.bypass_sync);
        // reliable shouldn't be forced to false when bypass is false
        assert!(!req.reliable); // default is false
    }

    #[test]
    fn test_message_priority_serde() {
        let priority = MessagePriority::Critical;
        let json = serde_json::to_string(&priority).unwrap();
        let deserialized: MessagePriority = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, MessagePriority::Critical);
    }

    #[test]
    fn test_transport_policy_at_level_none() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["p1"])
            .alternate(vec!["a1"])
            .contingency(vec!["c1"])
            .emergency(vec!["e1"]);

        // PaceLevel::None should return all
        assert_eq!(policy.at_level(PaceLevel::None).len(), 4);
    }

    #[test]
    fn test_estimate_delivery_zero_bandwidth() {
        let caps = TransportCapabilities {
            max_bandwidth_bps: 0,
            typical_latency_ms: 50,
            ..TransportCapabilities::quic()
        };
        // Should just return latency when bandwidth is 0/unlimited
        assert_eq!(caps.estimate_delivery_ms(1_000_000), 50);
    }

    #[test]
    fn test_meets_requirements_all_pass() {
        let caps = TransportCapabilities::quic();
        let req = MessageRequirements {
            reliable: true,
            min_bandwidth_bps: 1_000,
            message_size: 100,
            ..Default::default()
        };
        // QUIC caps: max_bandwidth_bps=100M (>1000), reliable=true, max_message_size=0 (unlimited)
        assert!(caps.meets_requirements(&req));
    }

    #[test]
    fn test_transport_mode_load_balanced() {
        let mut weights = std::collections::HashMap::new();
        weights.insert("t1".to_string(), 3);
        weights.insert("t2".to_string(), 1);

        let mode = TransportMode::load_balanced_weighted(weights.clone());
        if let TransportMode::LoadBalanced { weights: Some(w) } = mode {
            assert_eq!(w.get("t1"), Some(&3));
            assert_eq!(w.get("t2"), Some(&1));
        } else {
            panic!("Expected LoadBalanced with weights");
        }
    }
}
