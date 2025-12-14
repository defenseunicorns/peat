//! HIVE Beacon Advertiser
//!
//! Builds and manages BLE advertising packets containing HIVE beacons.

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use crate::config::DiscoveryConfig;
use crate::{HierarchyLevel, NodeId, HIVE_SERVICE_UUID_16BIT};

use super::beacon::{HiveBeacon, BEACON_COMPACT_SIZE};

/// Maximum advertising data length for legacy advertising
const LEGACY_ADV_MAX: usize = 31;

/// Maximum advertising data length for extended advertising
#[allow(dead_code)]
const EXTENDED_ADV_MAX: usize = 254;

/// AD Type: Flags
const AD_TYPE_FLAGS: u8 = 0x01;

/// AD Type: Complete List of 16-bit Service UUIDs
const AD_TYPE_SERVICE_UUID_16: u8 = 0x03;

/// AD Type: Service Data - 16-bit UUID
const AD_TYPE_SERVICE_DATA_16: u8 = 0x16;

/// AD Type: Complete Local Name
const AD_TYPE_LOCAL_NAME: u8 = 0x09;

/// AD Type: Shortened Local Name
const AD_TYPE_SHORT_NAME: u8 = 0x08;

/// AD Type: TX Power Level
const AD_TYPE_TX_POWER: u8 = 0x0A;

/// Flags value: LE General Discoverable Mode + BR/EDR Not Supported
const FLAGS_VALUE: u8 = 0x06;

/// Advertiser state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvertiserState {
    /// Not advertising
    Idle,
    /// Actively advertising
    Advertising,
    /// Temporarily paused (e.g., during connection)
    Paused,
}

/// Built advertising packet
#[derive(Debug, Clone)]
pub struct AdvertisingPacket {
    /// Advertising data
    pub adv_data: Vec<u8>,
    /// Scan response data (optional)
    pub scan_rsp: Option<Vec<u8>>,
    /// Whether this uses extended advertising
    pub extended: bool,
}

impl AdvertisingPacket {
    /// Check if this packet fits in legacy advertising
    pub fn fits_legacy(&self) -> bool {
        self.adv_data.len() <= LEGACY_ADV_MAX
            && self
                .scan_rsp
                .as_ref()
                .map_or(true, |sr| sr.len() <= LEGACY_ADV_MAX)
    }

    /// Total advertising data size
    pub fn total_size(&self) -> usize {
        self.adv_data.len() + self.scan_rsp.as_ref().map_or(0, |sr| sr.len())
    }
}

/// HIVE Beacon Advertiser
///
/// Manages building and updating BLE advertisements containing HIVE beacons.
pub struct Advertiser {
    /// Configuration (will be used for PHY/power management)
    #[allow(dead_code)]
    config: DiscoveryConfig,
    /// Current beacon
    beacon: HiveBeacon,
    /// Current state
    state: AdvertiserState,
    /// When advertising started (monotonic ms timestamp)
    started_at_ms: Option<u64>,
    /// Current time (monotonic ms, set externally)
    current_time_ms: u64,
    /// TX power level to advertise
    tx_power: Option<i8>,
    /// Device name to include
    device_name: Option<String>,
    /// Use extended advertising if available
    use_extended: bool,
    /// Last built packet (cached)
    cached_packet: Option<AdvertisingPacket>,
    /// Whether cache is dirty
    cache_dirty: bool,
}

impl Advertiser {
    /// Create a new advertiser with the given configuration and node ID
    pub fn new(config: DiscoveryConfig, node_id: NodeId) -> Self {
        let beacon = HiveBeacon::new(node_id);
        Self {
            config,
            beacon,
            state: AdvertiserState::Idle,
            started_at_ms: None,
            current_time_ms: 0,
            tx_power: None,
            device_name: None,
            use_extended: false,
            cached_packet: None,
            cache_dirty: true,
        }
    }

    /// Create an advertiser for a HIVE-Lite node
    pub fn hive_lite(config: DiscoveryConfig, node_id: NodeId) -> Self {
        let beacon = HiveBeacon::hive_lite(node_id);
        Self {
            config,
            beacon,
            state: AdvertiserState::Idle,
            started_at_ms: None,
            current_time_ms: 0,
            tx_power: None,
            device_name: None,
            use_extended: false,
            cached_packet: None,
            cache_dirty: true,
        }
    }

    /// Set the current time (call periodically from platform)
    pub fn set_time_ms(&mut self, time_ms: u64) {
        self.current_time_ms = time_ms;
    }

    /// Set TX power level
    pub fn with_tx_power(mut self, tx_power: i8) -> Self {
        self.tx_power = Some(tx_power);
        self.cache_dirty = true;
        self
    }

    /// Set device name
    pub fn with_name(mut self, name: String) -> Self {
        self.device_name = Some(name);
        self.cache_dirty = true;
        self
    }

    /// Enable extended advertising
    pub fn with_extended_advertising(mut self, enabled: bool) -> Self {
        self.use_extended = enabled;
        self.cache_dirty = true;
        self
    }

    /// Get current state
    pub fn state(&self) -> AdvertiserState {
        self.state
    }

    /// Get the current beacon
    pub fn beacon(&self) -> &HiveBeacon {
        &self.beacon
    }

    /// Get mutable access to the beacon
    pub fn beacon_mut(&mut self) -> &mut HiveBeacon {
        self.cache_dirty = true;
        &mut self.beacon
    }

    /// Update hierarchy level
    pub fn set_hierarchy_level(&mut self, level: HierarchyLevel) {
        self.beacon.hierarchy_level = level;
        self.cache_dirty = true;
    }

    /// Update capabilities
    pub fn set_capabilities(&mut self, caps: u16) {
        self.beacon.capabilities = caps;
        self.cache_dirty = true;
    }

    /// Update battery percentage
    pub fn set_battery(&mut self, percent: u8) {
        self.beacon.battery_percent = percent.min(100);
        self.cache_dirty = true;
    }

    /// Update geohash
    pub fn set_geohash(&mut self, geohash: u32) {
        self.beacon.geohash = geohash & 0x00FFFFFF;
        self.cache_dirty = true;
    }

    /// Start advertising
    pub fn start(&mut self) {
        self.state = AdvertiserState::Advertising;
        self.started_at_ms = Some(self.current_time_ms);
    }

    /// Pause advertising
    pub fn pause(&mut self) {
        self.state = AdvertiserState::Paused;
    }

    /// Resume advertising
    pub fn resume(&mut self) {
        if self.state == AdvertiserState::Paused {
            self.state = AdvertiserState::Advertising;
        }
    }

    /// Stop advertising
    pub fn stop(&mut self) {
        self.state = AdvertiserState::Idle;
        self.started_at_ms = None;
    }

    /// Get duration of current advertising session in milliseconds
    pub fn advertising_duration_ms(&self) -> Option<u64> {
        self.started_at_ms
            .map(|t| self.current_time_ms.saturating_sub(t))
    }

    /// Increment sequence number and invalidate cache
    pub fn increment_sequence(&mut self) {
        self.beacon.increment_seq();
        self.cache_dirty = true;
    }

    /// Build the advertising packet
    ///
    /// Uses cached packet if available and not dirty.
    pub fn build_packet(&mut self) -> &AdvertisingPacket {
        if self.cache_dirty || self.cached_packet.is_none() {
            let packet = self.build_packet_inner();
            self.cached_packet = Some(packet);
            self.cache_dirty = false;
        }
        self.cached_packet.as_ref().unwrap()
    }

    /// Force rebuild of advertising packet
    pub fn rebuild_packet(&mut self) -> &AdvertisingPacket {
        self.cache_dirty = true;
        self.build_packet()
    }

    /// Internal packet building
    fn build_packet_inner(&self) -> AdvertisingPacket {
        let mut adv_data = Vec::with_capacity(31);
        let mut scan_rsp = Vec::with_capacity(31);

        // Flags (3 bytes)
        adv_data.push(2); // Length
        adv_data.push(AD_TYPE_FLAGS);
        adv_data.push(FLAGS_VALUE);

        // Service UUID (4 bytes for 16-bit UUID)
        adv_data.push(3); // Length
        adv_data.push(AD_TYPE_SERVICE_UUID_16);
        adv_data.push((HIVE_SERVICE_UUID_16BIT & 0xFF) as u8);
        adv_data.push((HIVE_SERVICE_UUID_16BIT >> 8) as u8);

        // Service Data with beacon (3 + 10 = 13 bytes for compact beacon)
        let beacon_data = self.beacon.encode_compact();
        adv_data.push((2 + BEACON_COMPACT_SIZE) as u8); // Length
        adv_data.push(AD_TYPE_SERVICE_DATA_16);
        adv_data.push((HIVE_SERVICE_UUID_16BIT & 0xFF) as u8);
        adv_data.push((HIVE_SERVICE_UUID_16BIT >> 8) as u8);
        adv_data.extend_from_slice(&beacon_data);

        // TX Power (3 bytes) - add if space permits
        if let Some(tx_power) = self.tx_power {
            if adv_data.len() + 3 <= LEGACY_ADV_MAX {
                adv_data.push(2); // Length
                adv_data.push(AD_TYPE_TX_POWER);
                adv_data.push(tx_power as u8);
            } else {
                // Put in scan response
                scan_rsp.push(2);
                scan_rsp.push(AD_TYPE_TX_POWER);
                scan_rsp.push(tx_power as u8);
            }
        }

        // Device name - prefer scan response
        if let Some(ref name) = self.device_name {
            let name_bytes = name.as_bytes();
            let max_name_len = LEGACY_ADV_MAX - 2; // Room for length and type

            if name_bytes.len() <= max_name_len {
                // Full name fits
                scan_rsp.push(name_bytes.len() as u8 + 1);
                scan_rsp.push(AD_TYPE_LOCAL_NAME);
                scan_rsp.extend_from_slice(name_bytes);
            } else {
                // Shorten name
                let short_name = &name_bytes[..max_name_len.min(name_bytes.len())];
                scan_rsp.push(short_name.len() as u8 + 1);
                scan_rsp.push(AD_TYPE_SHORT_NAME);
                scan_rsp.extend_from_slice(short_name);
            }
        }

        let extended =
            self.use_extended || adv_data.len() > LEGACY_ADV_MAX || scan_rsp.len() > LEGACY_ADV_MAX;

        AdvertisingPacket {
            adv_data,
            scan_rsp: if scan_rsp.is_empty() {
                None
            } else {
                Some(scan_rsp)
            },
            extended,
        }
    }

    /// Get raw advertising data bytes
    pub fn advertising_data(&mut self) -> Vec<u8> {
        self.build_packet().adv_data.clone()
    }

    /// Get raw scan response bytes
    pub fn scan_response_data(&mut self) -> Option<Vec<u8>> {
        self.build_packet().scan_rsp.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities;

    #[test]
    fn test_advertiser_new() {
        let config = DiscoveryConfig::default();
        let node_id = NodeId::new(0x12345678);
        let advertiser = Advertiser::new(config, node_id);

        assert_eq!(advertiser.state(), AdvertiserState::Idle);
        assert_eq!(advertiser.beacon().node_id, node_id);
    }

    #[test]
    fn test_advertiser_hive_lite() {
        let config = DiscoveryConfig::default();
        let node_id = NodeId::new(0xCAFEBABE);
        let advertiser = Advertiser::hive_lite(config, node_id);

        assert!(advertiser.beacon().is_lite_node());
    }

    #[test]
    fn test_advertiser_state_transitions() {
        let config = DiscoveryConfig::default();
        let mut advertiser = Advertiser::new(config, NodeId::new(0x12345678));

        assert_eq!(advertiser.state(), AdvertiserState::Idle);

        advertiser.set_time_ms(1000);
        advertiser.start();
        assert_eq!(advertiser.state(), AdvertiserState::Advertising);
        advertiser.set_time_ms(2000);
        assert_eq!(advertiser.advertising_duration_ms(), Some(1000));

        advertiser.pause();
        assert_eq!(advertiser.state(), AdvertiserState::Paused);

        advertiser.resume();
        assert_eq!(advertiser.state(), AdvertiserState::Advertising);

        advertiser.stop();
        assert_eq!(advertiser.state(), AdvertiserState::Idle);
        assert!(advertiser.advertising_duration_ms().is_none());
    }

    #[test]
    fn test_build_packet_fits_legacy() {
        let config = DiscoveryConfig::default();
        let mut advertiser = Advertiser::new(config, NodeId::new(0x12345678));

        let packet = advertiser.build_packet();
        assert!(packet.fits_legacy());
        assert!(!packet.extended);

        // Should be: Flags(3) + UUID(4) + ServiceData(14) = 21 bytes
        assert!(packet.adv_data.len() <= LEGACY_ADV_MAX);
    }

    #[test]
    fn test_build_packet_with_name() {
        let config = DiscoveryConfig::default();
        let mut advertiser =
            Advertiser::new(config, NodeId::new(0x12345678)).with_name("HIVE-12345678".to_string());

        let packet = advertiser.build_packet();
        assert!(packet.scan_rsp.is_some());

        let scan_rsp = packet.scan_rsp.as_ref().unwrap();
        // Should contain the name
        assert!(scan_rsp.contains(&AD_TYPE_LOCAL_NAME));
    }

    #[test]
    fn test_build_packet_with_tx_power() {
        let config = DiscoveryConfig::default();
        let mut advertiser = Advertiser::new(config, NodeId::new(0x12345678)).with_tx_power(0);

        let packet = advertiser.build_packet();

        // TX power should be in adv_data (we have space)
        assert!(packet.adv_data.contains(&AD_TYPE_TX_POWER));
    }

    #[test]
    fn test_packet_caching() {
        let config = DiscoveryConfig::default();
        let mut advertiser = Advertiser::new(config, NodeId::new(0x12345678));

        // First build
        let packet1 = advertiser.build_packet();
        let data1 = packet1.adv_data.clone();

        // Second build should return same data (cached)
        let packet2 = advertiser.build_packet();
        assert_eq!(data1, packet2.adv_data);

        // Modify beacon - should invalidate cache
        advertiser.set_battery(50);
        let packet3 = advertiser.build_packet();
        // Data changes because battery is in beacon
        assert_ne!(data1, packet3.adv_data);
    }

    #[test]
    fn test_sequence_increment() {
        let config = DiscoveryConfig::default();
        let mut advertiser = Advertiser::new(config, NodeId::new(0x12345678));

        let seq1 = advertiser.beacon().seq_num;
        advertiser.increment_sequence();
        let seq2 = advertiser.beacon().seq_num;

        assert_eq!(seq2, seq1 + 1);
    }

    #[test]
    fn test_update_beacon_fields() {
        let config = DiscoveryConfig::default();
        let mut advertiser = Advertiser::new(config, NodeId::new(0x12345678));

        advertiser.set_hierarchy_level(HierarchyLevel::Squad);
        assert_eq!(advertiser.beacon().hierarchy_level, HierarchyLevel::Squad);

        advertiser.set_capabilities(capabilities::CAN_RELAY);
        assert!(advertiser.beacon().can_relay());

        advertiser.set_battery(75);
        assert_eq!(advertiser.beacon().battery_percent, 75);

        advertiser.set_geohash(0x123456);
        assert_eq!(advertiser.beacon().geohash, 0x123456);
    }
}
