//! HIVE Beacon format for BLE advertisements
//!
//! This module defines the wire format for HIVE beacons that are broadcast
//! via BLE advertising packets. The beacon format is designed to fit within
//! the 31-byte legacy advertising limit while conveying essential node info.
//!
//! ## Wire Format (16 bytes)
//!
//! ```text
//! Byte  0: Version (4 bits) | Capabilities high (4 bits)
//! Byte  1: Capabilities low (8 bits)
//! Bytes 2-5: Node ID (32 bits, big-endian)
//! Byte  6: Hierarchy level (8 bits)
//! Bytes 7-9: Geohash (24 bits, 6-char precision)
//! Byte 10: Battery percent (0-100)
//! Bytes 11-12: Sequence number (16 bits, big-endian)
//! Bytes 13-15: Reserved (for future use)
//! ```
//!
//! ## Advertising Packet Layout
//!
//! The complete advertising packet includes:
//! - Flags (3 bytes): `02 01 06`
//! - Complete 128-bit UUID (18 bytes): `11 07 <UUID>`
//! - Manufacturer Data (remaining): `<len> FF <company_id> <beacon_data>`
//!
//! Total: 3 + 18 + (3 + 16) = 40 bytes (requires extended advertising)
//! Or with shortened beacon: 3 + 18 + (3 + 10) = 34 bytes (still needs extended)
//!
//! For legacy (31 bytes), we use service data instead:
//! - Flags (3 bytes)
//! - Service Data (18 bytes): `11 16 <UUID_16bit> <beacon_data>`
//!
//! Total: 3 + 18 = 21 bytes (fits!)

#[cfg(not(feature = "std"))]
use alloc::string::String;

use crate::{capabilities, HierarchyLevel, NodeId};

/// HIVE beacon protocol version
pub const BEACON_VERSION: u8 = 1;

/// Beacon size in bytes
pub const BEACON_SIZE: usize = 16;

/// Compact beacon size (for legacy advertising)
pub const BEACON_COMPACT_SIZE: usize = 10;

/// HIVE Beacon data structure
///
/// Contains all information broadcast in a HIVE BLE advertisement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiveBeacon {
    /// Protocol version (0-15)
    pub version: u8,
    /// Node capabilities flags
    pub capabilities: u16,
    /// Node identifier
    pub node_id: NodeId,
    /// Hierarchy level in the mesh
    pub hierarchy_level: HierarchyLevel,
    /// Geohash for location (24-bit, ~600m precision)
    pub geohash: u32,
    /// Battery percentage (0-100, 255 = unknown)
    pub battery_percent: u8,
    /// Sequence number for deduplication
    pub seq_num: u16,
}

impl HiveBeacon {
    /// Create a new beacon with the given node ID
    pub fn new(node_id: NodeId) -> Self {
        Self {
            version: BEACON_VERSION,
            capabilities: 0,
            node_id,
            hierarchy_level: HierarchyLevel::Platform,
            geohash: 0,
            battery_percent: 255, // Unknown
            seq_num: 0,
        }
    }

    /// Create a beacon for a HIVE-Lite node
    pub fn hive_lite(node_id: NodeId) -> Self {
        Self {
            version: BEACON_VERSION,
            capabilities: capabilities::LITE_NODE,
            node_id,
            hierarchy_level: HierarchyLevel::Platform,
            geohash: 0,
            battery_percent: 255,
            seq_num: 0,
        }
    }

    /// Set capabilities
    pub fn with_capabilities(mut self, capabilities: u16) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Set hierarchy level
    pub fn with_hierarchy_level(mut self, level: HierarchyLevel) -> Self {
        self.hierarchy_level = level;
        self
    }

    /// Set geohash
    pub fn with_geohash(mut self, geohash: u32) -> Self {
        self.geohash = geohash & 0x00FFFFFF; // 24 bits only
        self
    }

    /// Set battery percentage
    pub fn with_battery(mut self, percent: u8) -> Self {
        self.battery_percent = percent.min(100);
        self
    }

    /// Increment sequence number
    pub fn increment_seq(&mut self) {
        self.seq_num = self.seq_num.wrapping_add(1);
    }

    /// Encode beacon to bytes (full 16-byte format)
    pub fn encode(&self) -> [u8; BEACON_SIZE] {
        let mut buf = [0u8; BEACON_SIZE];

        // Byte 0: Version (4 bits) | Capabilities high (4 bits)
        buf[0] = ((self.version & 0x0F) << 4) | ((self.capabilities >> 8) as u8 & 0x0F);

        // Byte 1: Capabilities low (8 bits)
        buf[1] = (self.capabilities & 0xFF) as u8;

        // Bytes 2-5: Node ID (big-endian)
        let node_id = self.node_id.as_u32();
        buf[2] = (node_id >> 24) as u8;
        buf[3] = (node_id >> 16) as u8;
        buf[4] = (node_id >> 8) as u8;
        buf[5] = node_id as u8;

        // Byte 6: Hierarchy level
        buf[6] = self.hierarchy_level.into();

        // Bytes 7-9: Geohash (24 bits, big-endian)
        buf[7] = (self.geohash >> 16) as u8;
        buf[8] = (self.geohash >> 8) as u8;
        buf[9] = self.geohash as u8;

        // Byte 10: Battery percent
        buf[10] = self.battery_percent;

        // Bytes 11-12: Sequence number (big-endian)
        buf[11] = (self.seq_num >> 8) as u8;
        buf[12] = self.seq_num as u8;

        // Bytes 13-15: Reserved
        buf[13] = 0;
        buf[14] = 0;
        buf[15] = 0;

        buf
    }

    /// Encode beacon to compact format (10 bytes for legacy advertising)
    ///
    /// Compact format omits geohash and reserved bytes:
    /// - Byte 0: Version | Capabilities high
    /// - Byte 1: Capabilities low
    /// - Bytes 2-5: Node ID
    /// - Byte 6: Hierarchy level
    /// - Byte 7: Battery percent
    /// - Bytes 8-9: Sequence number
    pub fn encode_compact(&self) -> [u8; BEACON_COMPACT_SIZE] {
        let mut buf = [0u8; BEACON_COMPACT_SIZE];

        buf[0] = ((self.version & 0x0F) << 4) | ((self.capabilities >> 8) as u8 & 0x0F);
        buf[1] = (self.capabilities & 0xFF) as u8;

        let node_id = self.node_id.as_u32();
        buf[2] = (node_id >> 24) as u8;
        buf[3] = (node_id >> 16) as u8;
        buf[4] = (node_id >> 8) as u8;
        buf[5] = node_id as u8;

        buf[6] = self.hierarchy_level.into();
        buf[7] = self.battery_percent;

        buf[8] = (self.seq_num >> 8) as u8;
        buf[9] = self.seq_num as u8;

        buf
    }

    /// Decode beacon from bytes (full 16-byte format)
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < BEACON_SIZE {
            return None;
        }

        let version = (data[0] >> 4) & 0x0F;
        let capabilities = ((data[0] as u16 & 0x0F) << 8) | (data[1] as u16);

        let node_id = NodeId::new(
            ((data[2] as u32) << 24)
                | ((data[3] as u32) << 16)
                | ((data[4] as u32) << 8)
                | (data[5] as u32),
        );

        let hierarchy_level = HierarchyLevel::from(data[6]);

        let geohash = ((data[7] as u32) << 16) | ((data[8] as u32) << 8) | (data[9] as u32);

        let battery_percent = data[10];

        let seq_num = ((data[11] as u16) << 8) | (data[12] as u16);

        Some(Self {
            version,
            capabilities,
            node_id,
            hierarchy_level,
            geohash,
            battery_percent,
            seq_num,
        })
    }

    /// Decode beacon from compact format (10 bytes)
    pub fn decode_compact(data: &[u8]) -> Option<Self> {
        if data.len() < BEACON_COMPACT_SIZE {
            return None;
        }

        let version = (data[0] >> 4) & 0x0F;
        let capabilities = ((data[0] as u16 & 0x0F) << 8) | (data[1] as u16);

        let node_id = NodeId::new(
            ((data[2] as u32) << 24)
                | ((data[3] as u32) << 16)
                | ((data[4] as u32) << 8)
                | (data[5] as u32),
        );

        let hierarchy_level = HierarchyLevel::from(data[6]);
        let battery_percent = data[7];
        let seq_num = ((data[8] as u16) << 8) | (data[9] as u16);

        Some(Self {
            version,
            capabilities,
            node_id,
            hierarchy_level,
            geohash: 0, // Not included in compact format
            battery_percent,
            seq_num,
        })
    }

    /// Check if this is a HIVE-Lite node
    pub fn is_lite_node(&self) -> bool {
        self.capabilities & capabilities::LITE_NODE != 0
    }

    /// Check if this node can relay messages
    pub fn can_relay(&self) -> bool {
        self.capabilities & capabilities::CAN_RELAY != 0
    }

    /// Check if this node supports Coded PHY
    pub fn supports_coded_phy(&self) -> bool {
        self.capabilities & capabilities::CODED_PHY != 0
    }
}

impl Default for HiveBeacon {
    fn default() -> Self {
        Self::new(NodeId::default())
    }
}

/// Parsed advertising data from a discovered device
#[derive(Debug, Clone)]
pub struct ParsedAdvertisement {
    /// Device address (MAC or platform-specific)
    pub address: String,
    /// RSSI in dBm
    pub rssi: i8,
    /// Parsed HIVE beacon (if this is a HIVE device)
    pub beacon: Option<HiveBeacon>,
    /// Device local name
    pub local_name: Option<String>,
    /// TX power level (if advertised)
    pub tx_power: Option<i8>,
    /// Whether the device is connectable
    pub connectable: bool,
}

impl ParsedAdvertisement {
    /// Check if this is a HIVE device
    pub fn is_hive_device(&self) -> bool {
        self.beacon.is_some()
    }

    /// Get the node ID if this is a HIVE device
    pub fn node_id(&self) -> Option<&NodeId> {
        self.beacon.as_ref().map(|b| &b.node_id)
    }

    /// Estimate distance based on RSSI and TX power
    ///
    /// Uses the log-distance path loss model:
    /// distance = 10 ^ ((tx_power - rssi) / (10 * n))
    /// where n is the path loss exponent (typically 2-4)
    ///
    /// Note: Requires std feature for floating point math.
    #[cfg(feature = "std")]
    pub fn estimated_distance_meters(&self) -> Option<f32> {
        let tx_power = self.tx_power.unwrap_or(0) as f32;
        let rssi = self.rssi as f32;
        let n = 2.5; // Path loss exponent (indoor environment)

        if rssi >= tx_power {
            return Some(1.0); // Very close
        }

        let distance = 10.0_f32.powf((tx_power - rssi) / (10.0 * n));
        Some(distance)
    }

    /// Stub for no_std - always returns None
    #[cfg(not(feature = "std"))]
    pub fn estimated_distance_meters(&self) -> Option<f32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beacon_encode_decode() {
        let beacon = HiveBeacon::new(NodeId::new(0x12345678))
            .with_capabilities(capabilities::LITE_NODE | capabilities::SENSOR_ACCEL)
            .with_hierarchy_level(HierarchyLevel::Squad)
            .with_geohash(0x98FF88)
            .with_battery(75);

        let encoded = beacon.encode();
        let decoded = HiveBeacon::decode(&encoded).unwrap();

        assert_eq!(decoded.version, beacon.version);
        assert_eq!(decoded.capabilities, beacon.capabilities);
        assert_eq!(decoded.node_id, beacon.node_id);
        assert_eq!(decoded.hierarchy_level, beacon.hierarchy_level);
        assert_eq!(decoded.geohash, beacon.geohash & 0x00FFFFFF);
        assert_eq!(decoded.battery_percent, beacon.battery_percent);
    }

    #[test]
    fn test_beacon_compact_encode_decode() {
        let beacon = HiveBeacon::new(NodeId::new(0xDEADBEEF))
            .with_capabilities(capabilities::CAN_RELAY)
            .with_battery(50);

        let encoded = beacon.encode_compact();
        assert_eq!(encoded.len(), BEACON_COMPACT_SIZE);

        let decoded = HiveBeacon::decode_compact(&encoded).unwrap();

        assert_eq!(decoded.node_id, beacon.node_id);
        assert_eq!(decoded.capabilities, beacon.capabilities);
        assert_eq!(decoded.battery_percent, beacon.battery_percent);
        assert_eq!(decoded.geohash, 0); // Not in compact format
    }

    #[test]
    fn test_beacon_size() {
        let beacon = HiveBeacon::new(NodeId::new(0x12345678));
        let encoded = beacon.encode();
        assert_eq!(encoded.len(), BEACON_SIZE);
        assert_eq!(encoded.len(), 16);
    }

    #[test]
    fn test_beacon_version() {
        let beacon = HiveBeacon::new(NodeId::new(0x12345678));
        let encoded = beacon.encode();
        let version = (encoded[0] >> 4) & 0x0F;
        assert_eq!(version, BEACON_VERSION);
    }

    #[test]
    fn test_beacon_capabilities() {
        let caps = capabilities::LITE_NODE | capabilities::CODED_PHY | capabilities::HAS_GPS;
        let beacon = HiveBeacon::new(NodeId::new(0x12345678)).with_capabilities(caps);

        assert!(beacon.is_lite_node());
        assert!(beacon.supports_coded_phy());
        assert!(!beacon.can_relay());

        let encoded = beacon.encode();
        let decoded = HiveBeacon::decode(&encoded).unwrap();
        assert_eq!(decoded.capabilities, caps);
    }

    #[test]
    fn test_sequence_number_wrap() {
        let mut beacon = HiveBeacon::new(NodeId::new(0x12345678));
        beacon.seq_num = 0xFFFF;
        beacon.increment_seq();
        assert_eq!(beacon.seq_num, 0);
    }

    #[test]
    fn test_decode_invalid_length() {
        let short_data = [0u8; 5];
        assert!(HiveBeacon::decode(&short_data).is_none());
        assert!(HiveBeacon::decode_compact(&short_data).is_none());
    }

    #[test]
    fn test_estimated_distance() {
        let adv = ParsedAdvertisement {
            address: "00:11:22:33:44:55".to_string(),
            rssi: -60,
            beacon: None,
            local_name: None,
            tx_power: Some(-20), // Typical BLE TX power
            connectable: true,
        };

        let distance = adv.estimated_distance_meters().unwrap();
        // Path loss model gives rough estimate - test that it returns a reasonable value
        // With TX=-20dBm, RSSI=-60dBm, n=2.5: d = 10^(40/25) ≈ 25m
        assert!(distance > 1.0 && distance < 100.0);
    }

    #[test]
    fn test_hive_lite_beacon() {
        let beacon = HiveBeacon::hive_lite(NodeId::new(0xCAFEBABE));
        assert!(beacon.is_lite_node());
        assert!(!beacon.can_relay());
    }
}
