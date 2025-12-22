//! Peer management types for HIVE BLE mesh
//!
//! This module provides the core peer representation and configuration
//! for centralized peer management across all platforms (iOS, Android, ESP32).

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use crate::NodeId;

/// Unified peer representation across all platforms
///
/// Represents a discovered or connected HIVE mesh peer with all
/// relevant metadata for mesh operations.
#[derive(Debug, Clone)]
pub struct HivePeer {
    /// HIVE node identifier (32-bit)
    pub node_id: NodeId,

    /// Platform-specific BLE identifier
    /// - iOS: CBPeripheral UUID string
    /// - Android: MAC address string
    /// - ESP32: MAC address or NimBLE handle
    pub identifier: String,

    /// Mesh ID this peer belongs to (e.g., "DEMO")
    pub mesh_id: Option<String>,

    /// Advertised device name (e.g., "HIVE_DEMO-12345678")
    pub name: Option<String>,

    /// Last known signal strength (RSSI in dBm)
    pub rssi: i8,

    /// Whether we have an active BLE connection to this peer
    pub is_connected: bool,

    /// Timestamp when this peer was last seen (milliseconds since epoch/boot)
    pub last_seen_ms: u64,
}

impl HivePeer {
    /// Create a new peer from discovery data
    pub fn new(
        node_id: NodeId,
        identifier: String,
        mesh_id: Option<String>,
        name: Option<String>,
        rssi: i8,
    ) -> Self {
        Self {
            node_id,
            identifier,
            mesh_id,
            name,
            rssi,
            is_connected: false,
            last_seen_ms: 0,
        }
    }

    /// Update the peer's last seen timestamp
    pub fn touch(&mut self, now_ms: u64) {
        self.last_seen_ms = now_ms;
    }

    /// Check if this peer is stale (not seen within timeout)
    pub fn is_stale(&self, now_ms: u64, timeout_ms: u64) -> bool {
        if self.last_seen_ms == 0 {
            return false; // Never seen, don't consider stale
        }
        now_ms.saturating_sub(self.last_seen_ms) > timeout_ms
    }

    /// Get display name for this peer
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(self.identifier.as_str())
    }

    /// Get signal strength category
    pub fn signal_strength(&self) -> SignalStrength {
        match self.rssi {
            r if r >= -50 => SignalStrength::Excellent,
            r if r >= -70 => SignalStrength::Good,
            r if r >= -85 => SignalStrength::Fair,
            _ => SignalStrength::Weak,
        }
    }
}

impl Default for HivePeer {
    fn default() -> Self {
        Self {
            node_id: NodeId::default(),
            identifier: String::new(),
            mesh_id: None,
            name: None,
            rssi: -100,
            is_connected: false,
            last_seen_ms: 0,
        }
    }
}

/// Signal strength categories for display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalStrength {
    /// RSSI >= -50 dBm
    Excellent,
    /// RSSI >= -70 dBm
    Good,
    /// RSSI >= -85 dBm
    Fair,
    /// RSSI < -85 dBm
    Weak,
}

/// Configuration for the PeerManager
///
/// Provides configurable timeouts and behaviors for peer management.
/// All time values are in milliseconds.
#[derive(Debug, Clone)]
pub struct PeerManagerConfig {
    /// Time after which a peer is considered stale and removed (default: 45000ms)
    pub peer_timeout_ms: u64,

    /// How often to run cleanup of stale peers (default: 10000ms)
    pub cleanup_interval_ms: u64,

    /// How often to sync documents with peers (default: 5000ms)
    pub sync_interval_ms: u64,

    /// Minimum time between syncs to the same peer (default: 30000ms)
    /// Prevents "thrashing" when peers keep reconnecting
    pub sync_cooldown_ms: u64,

    /// Whether to automatically connect to discovered peers (default: true)
    pub auto_connect: bool,

    /// Local mesh ID for filtering peers (e.g., "DEMO")
    pub mesh_id: String,

    /// Maximum number of tracked peers (for no_std/embedded, default: 8)
    pub max_peers: usize,
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self {
            peer_timeout_ms: 45_000,     // 45 seconds
            cleanup_interval_ms: 10_000, // 10 seconds
            sync_interval_ms: 5_000,     // 5 seconds
            sync_cooldown_ms: 30_000,    // 30 seconds
            auto_connect: true,
            mesh_id: String::from("DEMO"),
            max_peers: 8,
        }
    }
}

impl PeerManagerConfig {
    /// Create a new config with the specified mesh ID
    pub fn with_mesh_id(mesh_id: impl Into<String>) -> Self {
        Self {
            mesh_id: mesh_id.into(),
            ..Default::default()
        }
    }

    /// Set peer timeout
    pub fn peer_timeout(mut self, timeout_ms: u64) -> Self {
        self.peer_timeout_ms = timeout_ms;
        self
    }

    /// Set sync interval
    pub fn sync_interval(mut self, interval_ms: u64) -> Self {
        self.sync_interval_ms = interval_ms;
        self
    }

    /// Set auto-connect behavior
    pub fn auto_connect(mut self, enabled: bool) -> Self {
        self.auto_connect = enabled;
        self
    }

    /// Set max peers (for embedded systems)
    pub fn max_peers(mut self, max: usize) -> Self {
        self.max_peers = max;
        self
    }

    /// Check if a device mesh ID matches our mesh
    ///
    /// Returns true if:
    /// - Device mesh ID matches our mesh ID exactly, OR
    /// - Device mesh ID is None (legacy device, matches any mesh)
    pub fn matches_mesh(&self, device_mesh_id: Option<&str>) -> bool {
        match device_mesh_id {
            Some(id) => id == self.mesh_id,
            None => true, // Legacy devices match any mesh
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_stale_detection() {
        let mut peer = HivePeer::new(
            NodeId::new(0x12345678),
            "test-id".into(),
            Some("DEMO".into()),
            Some("HIVE_DEMO-12345678".into()),
            -70,
        );

        // Fresh peer is not stale
        peer.touch(1000);
        assert!(!peer.is_stale(2000, 45_000));

        // Peer becomes stale after timeout
        assert!(peer.is_stale(50_000, 45_000));
    }

    #[test]
    fn test_signal_strength() {
        let peer_excellent = HivePeer {
            rssi: -45,
            ..Default::default()
        };
        assert_eq!(peer_excellent.signal_strength(), SignalStrength::Excellent);

        let peer_good = HivePeer {
            rssi: -65,
            ..Default::default()
        };
        assert_eq!(peer_good.signal_strength(), SignalStrength::Good);

        let peer_fair = HivePeer {
            rssi: -80,
            ..Default::default()
        };
        assert_eq!(peer_fair.signal_strength(), SignalStrength::Fair);

        let peer_weak = HivePeer {
            rssi: -95,
            ..Default::default()
        };
        assert_eq!(peer_weak.signal_strength(), SignalStrength::Weak);
    }

    #[test]
    fn test_mesh_matching() {
        let config = PeerManagerConfig::with_mesh_id("ALPHA");

        // Exact match
        assert!(config.matches_mesh(Some("ALPHA")));

        // No match
        assert!(!config.matches_mesh(Some("BETA")));

        // Legacy device (no mesh ID) matches any
        assert!(config.matches_mesh(None));
    }
}
