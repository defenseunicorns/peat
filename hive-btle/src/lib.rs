//! HIVE-BTLE: Bluetooth Low Energy mesh transport for HIVE Protocol
//!
//! This crate provides BLE-based peer-to-peer mesh networking for HIVE,
//! supporting discovery, advertisement, connectivity, and HIVE-Lite sync.
//!
//! ## Overview
//!
//! HIVE-BTLE implements the pluggable transport abstraction (ADR-032) for
//! Bluetooth Low Energy, enabling HIVE Protocol to operate over BLE in
//! resource-constrained environments like smartwatches.
//!
//! ## Key Features
//!
//! - **Cross-platform**: Linux, Android, macOS, iOS, Windows, ESP32
//! - **Power efficient**: Designed for 18+ hour battery life on watches
//! - **Long range**: Coded PHY support for 300m+ range
//! - **HIVE-Lite sync**: Optimized CRDT sync over GATT
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                  Application                     │
//! ├─────────────────────────────────────────────────┤
//! │           BluetoothLETransport                   │
//! │  (implements MeshTransport from ADR-032)        │
//! ├─────────────────────────────────────────────────┤
//! │              BleAdapter Trait                    │
//! ├──────────┬──────────┬──────────┬────────────────┤
//! │  Linux   │ Android  │  Apple   │    Windows     │
//! │ (BlueZ)  │  (JNI)   │(CoreBT)  │    (WinRT)     │
//! └──────────┴──────────┴──────────┴────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```ignore
//! use hive_btle::{BleConfig, BluetoothLETransport, NodeId};
//!
//! // Create HIVE-Lite optimized config for battery efficiency
//! let config = BleConfig::hive_lite(NodeId::new(0x12345678));
//!
//! // Create transport with platform adapter
//! #[cfg(feature = "linux")]
//! let adapter = hive_btle::platform::linux::BluerAdapter::new()?;
//!
//! let transport = BluetoothLETransport::new(config, adapter);
//!
//! // Start advertising and scanning
//! transport.start().await?;
//!
//! // Connect to a peer
//! let conn = transport.connect(&peer_id).await?;
//! ```
//!
//! ## Feature Flags
//!
//! - `std` (default): Standard library support
//! - `linux`: Linux/BlueZ support via `bluer`
//! - `android`: Android support via JNI
//! - `macos`: macOS support via CoreBluetooth
//! - `ios`: iOS support via CoreBluetooth
//! - `windows`: Windows support via WinRT
//! - `embedded`: Embedded/no_std support
//! - `coded-phy`: Enable Coded PHY for extended range
//! - `extended-adv`: Enable extended advertising
//!
//! ## Power Profiles
//!
//! | Profile | Duty Cycle | Watch Battery |
//! |---------|------------|---------------|
//! | Aggressive | 20% | ~6 hours |
//! | Balanced | 10% | ~12 hours |
//! | **LowPower** | **2%** | **~20+ hours** |
//!
//! ## Related ADRs
//!
//! - ADR-039: HIVE-BTLE Mesh Transport Crate
//! - ADR-032: Pluggable Transport Abstraction
//! - ADR-035: HIVE-Lite Embedded Nodes
//! - ADR-037: Resource-Constrained Device Optimization

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

pub mod config;
pub mod discovery;
pub mod error;
pub mod gatt;
pub mod mesh;
pub mod platform;
pub mod sync;
pub mod transport;

// Re-exports for convenience
pub use config::{BleConfig, BlePhy, DiscoveryConfig, GattConfig, MeshConfig, PowerProfile};
pub use discovery::{Advertiser, HiveBeacon, ScanFilter, Scanner};
pub use error::{BleError, Result};
pub use gatt::{HiveGattService, SyncProtocol};
pub use mesh::{MeshManager, MeshRouter, MeshTopology, TopologyConfig, TopologyEvent};
pub use platform::{BleAdapter, ConnectionEvent, DisconnectReason, DiscoveredDevice, StubAdapter};
pub use sync::{GattSyncProtocol, SyncConfig, SyncState};
pub use transport::{BleConnection, BluetoothLETransport, MeshTransport, TransportCapabilities};

/// HIVE BLE Service UUID (128-bit)
///
/// All HIVE nodes advertise this UUID for discovery.
pub const HIVE_SERVICE_UUID: uuid::Uuid = uuid::uuid!("f47ac10b-58cc-4372-a567-0e02b2c3d479");

/// HIVE BLE Service UUID (16-bit short form)
///
/// Used for legacy advertising to fit within 31-byte limit.
/// Assigned from Bluetooth SIG custom range.
pub const HIVE_SERVICE_UUID_16BIT: u16 = 0xD479;

/// HIVE Node Info Characteristic UUID
pub const CHAR_NODE_INFO_UUID: u16 = 0x0001;

/// HIVE Sync State Characteristic UUID
pub const CHAR_SYNC_STATE_UUID: u16 = 0x0002;

/// HIVE Sync Data Characteristic UUID
pub const CHAR_SYNC_DATA_UUID: u16 = 0x0003;

/// HIVE Command Characteristic UUID
pub const CHAR_COMMAND_UUID: u16 = 0x0004;

/// HIVE Status Characteristic UUID
pub const CHAR_STATUS_UUID: u16 = 0x0005;

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Node identifier
///
/// Represents a unique node in the HIVE mesh. For BLE, this is typically
/// derived from the Bluetooth MAC address or a configured value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct NodeId {
    /// 32-bit node identifier
    id: u32,
}

impl NodeId {
    /// Create a new node ID from a 32-bit value
    pub fn new(id: u32) -> Self {
        Self { id }
    }

    /// Get the raw 32-bit ID value
    pub fn as_u32(&self) -> u32 {
        self.id
    }

    /// Create from a string representation (hex format)
    pub fn parse(s: &str) -> Option<Self> {
        // Try parsing as hex (with or without 0x prefix)
        let s = s.trim_start_matches("0x").trim_start_matches("0X");
        u32::from_str_radix(s, 16).ok().map(Self::new)
    }
}

impl core::fmt::Display for NodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:08X}", self.id)
    }
}

impl From<u32> for NodeId {
    fn from(id: u32) -> Self {
        Self::new(id)
    }
}

impl From<NodeId> for u32 {
    fn from(node_id: NodeId) -> Self {
        node_id.id
    }
}

/// Node capability flags
///
/// Advertised in the HIVE beacon to indicate what this node can do.
pub mod capabilities {
    /// This is a HIVE-Lite node (minimal state, single parent)
    pub const LITE_NODE: u16 = 0x0001;
    /// Has accelerometer sensor
    pub const SENSOR_ACCEL: u16 = 0x0002;
    /// Has temperature sensor
    pub const SENSOR_TEMP: u16 = 0x0004;
    /// Has button input
    pub const SENSOR_BUTTON: u16 = 0x0008;
    /// Has LED output
    pub const ACTUATOR_LED: u16 = 0x0010;
    /// Has vibration motor
    pub const ACTUATOR_VIBRATE: u16 = 0x0020;
    /// Has display
    pub const HAS_DISPLAY: u16 = 0x0040;
    /// Can relay messages (not a leaf)
    pub const CAN_RELAY: u16 = 0x0080;
    /// Supports Coded PHY
    pub const CODED_PHY: u16 = 0x0100;
    /// Has GPS
    pub const HAS_GPS: u16 = 0x0200;
}

/// Hierarchy levels in the HIVE mesh
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum HierarchyLevel {
    /// Platform/soldier level (leaf nodes)
    #[default]
    Platform = 0,
    /// Squad level
    Squad = 1,
    /// Platoon level
    Platoon = 2,
    /// Company level
    Company = 3,
}

impl From<u8> for HierarchyLevel {
    fn from(value: u8) -> Self {
        match value {
            0 => HierarchyLevel::Platform,
            1 => HierarchyLevel::Squad,
            2 => HierarchyLevel::Platoon,
            3 => HierarchyLevel::Company,
            _ => HierarchyLevel::Platform,
        }
    }
}

impl From<HierarchyLevel> for u8 {
    fn from(level: HierarchyLevel) -> Self {
        level as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id() {
        let id = NodeId::new(0x12345678);
        assert_eq!(id.as_u32(), 0x12345678);
        assert_eq!(id.to_string(), "12345678");
    }

    #[test]
    fn test_node_id_parse() {
        assert_eq!(NodeId::parse("12345678").unwrap().as_u32(), 0x12345678);
        assert_eq!(NodeId::parse("0x12345678").unwrap().as_u32(), 0x12345678);
        assert!(NodeId::parse("not_hex").is_none());
    }

    #[test]
    fn test_hierarchy_level() {
        assert_eq!(HierarchyLevel::from(0), HierarchyLevel::Platform);
        assert_eq!(HierarchyLevel::from(3), HierarchyLevel::Company);
        assert_eq!(u8::from(HierarchyLevel::Squad), 1);
    }

    #[test]
    fn test_service_uuid() {
        assert_eq!(
            HIVE_SERVICE_UUID.to_string(),
            "f47ac10b-58cc-4372-a567-0e02b2c3d479"
        );
    }

    #[test]
    fn test_capabilities() {
        let caps = capabilities::LITE_NODE | capabilities::SENSOR_ACCEL | capabilities::HAS_GPS;
        assert_eq!(caps, 0x0203);
    }
}
