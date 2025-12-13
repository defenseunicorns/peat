//! BLE Scanner for discovering HIVE nodes
//!
//! Provides filtering, deduplication, and tracking of discovered HIVE beacons.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::config::DiscoveryConfig;
use crate::{HierarchyLevel, NodeId};

use super::beacon::{HiveBeacon, ParsedAdvertisement};

/// Default timeout for considering a device "stale"
const DEFAULT_DEVICE_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimum interval between processing duplicate beacons from same node
const DEDUP_INTERVAL: Duration = Duration::from_millis(500);

/// Tracked device state
#[derive(Debug, Clone)]
pub struct TrackedDevice {
    /// Last received beacon
    pub beacon: HiveBeacon,
    /// Device address
    pub address: String,
    /// Last RSSI reading
    pub rssi: i8,
    /// RSSI history for averaging (last N readings)
    pub rssi_history: Vec<i8>,
    /// When first discovered
    pub first_seen: Instant,
    /// When last beacon received
    pub last_seen: Instant,
    /// Estimated distance in meters
    pub estimated_distance: Option<f32>,
    /// Whether this device is currently connectable
    pub connectable: bool,
}

impl TrackedDevice {
    /// Create a new tracked device
    fn new(beacon: HiveBeacon, address: String, rssi: i8, connectable: bool) -> Self {
        let now = Instant::now();
        Self {
            beacon,
            address,
            rssi,
            rssi_history: vec![rssi],
            first_seen: now,
            last_seen: now,
            estimated_distance: None,
            connectable,
        }
    }

    /// Update with new beacon data
    fn update(&mut self, beacon: HiveBeacon, rssi: i8, connectable: bool) {
        self.beacon = beacon;
        self.rssi = rssi;
        self.last_seen = Instant::now();
        self.connectable = connectable;

        // Keep last 10 RSSI readings for averaging
        self.rssi_history.push(rssi);
        if self.rssi_history.len() > 10 {
            self.rssi_history.remove(0);
        }
    }

    /// Get average RSSI
    pub fn average_rssi(&self) -> i8 {
        if self.rssi_history.is_empty() {
            return self.rssi;
        }
        let sum: i32 = self.rssi_history.iter().map(|&r| r as i32).sum();
        (sum / self.rssi_history.len() as i32) as i8
    }

    /// Check if this device is stale (not seen recently)
    pub fn is_stale(&self, timeout: Duration) -> bool {
        self.last_seen.elapsed() > timeout
    }

    /// Get time since first discovery
    pub fn time_tracked(&self) -> Duration {
        self.first_seen.elapsed()
    }
}

/// Filter criteria for scanning
#[derive(Debug, Clone, Default)]
pub struct ScanFilter {
    /// Only include HIVE nodes
    pub hive_only: bool,
    /// Only include nodes at or above this hierarchy level
    pub min_hierarchy_level: Option<HierarchyLevel>,
    /// Only include nodes with these capabilities (bitmask)
    pub required_capabilities: Option<u16>,
    /// Exclude nodes with these capabilities
    pub excluded_capabilities: Option<u16>,
    /// Minimum RSSI threshold (exclude weaker signals)
    pub min_rssi: Option<i8>,
    /// Maximum estimated distance in meters
    pub max_distance: Option<f32>,
    /// Only include connectable devices
    pub connectable_only: bool,
}

impl ScanFilter {
    /// Create a filter for HIVE nodes only
    pub fn hive_nodes() -> Self {
        Self {
            hive_only: true,
            ..Default::default()
        }
    }

    /// Create a filter for potential parents (nodes above our level)
    pub fn potential_parents(our_level: HierarchyLevel) -> Self {
        Self {
            hive_only: true,
            min_hierarchy_level: Some(our_level),
            connectable_only: true,
            ..Default::default()
        }
    }

    /// Check if a parsed advertisement passes this filter
    pub fn matches(&self, adv: &ParsedAdvertisement) -> bool {
        // HIVE-only filter
        if self.hive_only && !adv.is_hive_device() {
            return false;
        }

        // RSSI filter
        if let Some(min_rssi) = self.min_rssi {
            if adv.rssi < min_rssi {
                return false;
            }
        }

        // Distance filter
        if let Some(max_distance) = self.max_distance {
            if let Some(distance) = adv.estimated_distance_meters() {
                if distance > max_distance {
                    return false;
                }
            }
        }

        // Connectable filter
        if self.connectable_only && !adv.connectable {
            return false;
        }

        // Beacon-specific filters
        if let Some(ref beacon) = adv.beacon {
            // Hierarchy level filter
            if let Some(min_level) = self.min_hierarchy_level {
                if beacon.hierarchy_level < min_level {
                    return false;
                }
            }

            // Required capabilities
            if let Some(required) = self.required_capabilities {
                if beacon.capabilities & required != required {
                    return false;
                }
            }

            // Excluded capabilities
            if let Some(excluded) = self.excluded_capabilities {
                if beacon.capabilities & excluded != 0 {
                    return false;
                }
            }
        }

        true
    }
}

/// Scanner state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScannerState {
    /// Not scanning
    Idle,
    /// Actively scanning
    Scanning,
    /// Paused (e.g., during connection)
    Paused,
}

/// BLE Scanner for discovering HIVE nodes
///
/// Handles beacon reception, filtering, deduplication, and device tracking.
pub struct Scanner {
    /// Scanner configuration (will be used for PHY/power management)
    #[allow(dead_code)]
    config: DiscoveryConfig,
    /// Current state
    state: ScannerState,
    /// Tracked devices by node ID
    devices: HashMap<NodeId, TrackedDevice>,
    /// Address to node ID mapping (for devices without parsed beacon)
    address_map: HashMap<String, NodeId>,
    /// Filter criteria
    filter: ScanFilter,
    /// Device timeout
    device_timeout: Duration,
    /// Last dedup timestamps per node
    last_processed: HashMap<NodeId, Instant>,
}

impl Scanner {
    /// Create a new scanner with default settings
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            state: ScannerState::Idle,
            devices: HashMap::new(),
            address_map: HashMap::new(),
            filter: ScanFilter::default(),
            device_timeout: DEFAULT_DEVICE_TIMEOUT,
            last_processed: HashMap::new(),
        }
    }

    /// Set the scan filter
    pub fn set_filter(&mut self, filter: ScanFilter) {
        self.filter = filter;
    }

    /// Set device timeout
    pub fn set_device_timeout(&mut self, timeout: Duration) {
        self.device_timeout = timeout;
    }

    /// Get current state
    pub fn state(&self) -> ScannerState {
        self.state
    }

    /// Start scanning
    pub fn start(&mut self) {
        self.state = ScannerState::Scanning;
    }

    /// Pause scanning
    pub fn pause(&mut self) {
        self.state = ScannerState::Paused;
    }

    /// Stop scanning
    pub fn stop(&mut self) {
        self.state = ScannerState::Idle;
    }

    /// Process a received advertisement
    ///
    /// Returns true if this is a new or updated device that passes the filter.
    pub fn process_advertisement(&mut self, adv: ParsedAdvertisement) -> bool {
        // Apply filter
        if !self.filter.matches(&adv) {
            return false;
        }

        // Extract beacon and node ID
        let (beacon, node_id) = match adv.beacon {
            Some(ref b) => (b.clone(), b.node_id.clone()),
            None => return false, // No beacon = not a HIVE device
        };

        // Check deduplication
        if let Some(last) = self.last_processed.get(&node_id) {
            if last.elapsed() < DEDUP_INTERVAL {
                return false;
            }
        }
        self.last_processed.insert(node_id.clone(), Instant::now());

        // Update or create tracked device
        let is_new = !self.devices.contains_key(&node_id);

        if let Some(device) = self.devices.get_mut(&node_id) {
            // Update existing device
            device.update(beacon, adv.rssi, adv.connectable);
        } else {
            // New device
            let device = TrackedDevice::new(beacon, adv.address.clone(), adv.rssi, adv.connectable);
            self.devices.insert(node_id.clone(), device);
            self.address_map.insert(adv.address, node_id);
        }

        is_new
    }

    /// Get a tracked device by node ID
    pub fn get_device(&self, node_id: &NodeId) -> Option<&TrackedDevice> {
        self.devices.get(node_id)
    }

    /// Get node ID for an address
    pub fn get_node_id_for_address(&self, address: &str) -> Option<&NodeId> {
        self.address_map.get(address)
    }

    /// Get all tracked devices
    pub fn devices(&self) -> impl Iterator<Item = &TrackedDevice> {
        self.devices.values()
    }

    /// Get devices sorted by RSSI (strongest first)
    pub fn devices_by_rssi(&self) -> Vec<&TrackedDevice> {
        let mut devices: Vec<_> = self.devices.values().collect();
        devices.sort_by(|a, b| b.rssi.cmp(&a.rssi));
        devices
    }

    /// Get devices sorted by hierarchy level (highest first)
    pub fn devices_by_hierarchy(&self) -> Vec<&TrackedDevice> {
        let mut devices: Vec<_> = self.devices.values().collect();
        devices.sort_by(|a, b| b.beacon.hierarchy_level.cmp(&a.beacon.hierarchy_level));
        devices
    }

    /// Get count of tracked devices
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Remove stale devices
    ///
    /// Returns the number of devices removed.
    pub fn remove_stale(&mut self) -> usize {
        let timeout = self.device_timeout;
        let stale: Vec<NodeId> = self
            .devices
            .iter()
            .filter(|(_, d)| d.is_stale(timeout))
            .map(|(id, _)| id.clone())
            .collect();

        let count = stale.len();
        for node_id in stale {
            if let Some(device) = self.devices.remove(&node_id) {
                self.address_map.remove(&device.address);
                self.last_processed.remove(&node_id);
            }
        }

        count
    }

    /// Clear all tracked devices
    pub fn clear(&mut self) {
        self.devices.clear();
        self.address_map.clear();
        self.last_processed.clear();
    }

    /// Find the best parent candidate
    ///
    /// Selects based on hierarchy level (prefer higher) and RSSI (prefer stronger).
    pub fn find_best_parent(&self, our_level: HierarchyLevel) -> Option<&TrackedDevice> {
        self.devices
            .values()
            .filter(|d| {
                d.beacon.hierarchy_level > our_level && d.connectable && !d.beacon.is_lite_node()
            })
            .max_by(|a, b| {
                // First compare hierarchy level
                match a.beacon.hierarchy_level.cmp(&b.beacon.hierarchy_level) {
                    std::cmp::Ordering::Equal => {
                        // Then compare RSSI
                        a.average_rssi().cmp(&b.average_rssi())
                    }
                    other => other,
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_adv(node_id: u32, rssi: i8, level: HierarchyLevel) -> ParsedAdvertisement {
        let beacon = HiveBeacon::new(NodeId::new(node_id))
            .with_hierarchy_level(level)
            .with_battery(80);

        ParsedAdvertisement {
            address: format!("00:11:22:33:44:{:02X}", node_id as u8),
            rssi,
            beacon: Some(beacon),
            local_name: Some(format!("HIVE-{:08X}", node_id)),
            tx_power: Some(0),
            connectable: true,
        }
    }

    #[test]
    fn test_scanner_process_advertisement() {
        let config = DiscoveryConfig::default();
        let mut scanner = Scanner::new(config);

        let adv = make_adv(0x12345678, -60, HierarchyLevel::Platform);
        assert!(scanner.process_advertisement(adv));
        assert_eq!(scanner.device_count(), 1);

        // Duplicate within dedup interval should be ignored
        let adv2 = make_adv(0x12345678, -65, HierarchyLevel::Platform);
        assert!(!scanner.process_advertisement(adv2));
        assert_eq!(scanner.device_count(), 1);
    }

    #[test]
    fn test_scan_filter_hive_only() {
        let filter = ScanFilter::hive_nodes();

        let hive_adv = make_adv(0x12345678, -60, HierarchyLevel::Platform);
        assert!(filter.matches(&hive_adv));

        let non_hive = ParsedAdvertisement {
            address: "AA:BB:CC:DD:EE:FF".to_string(),
            rssi: -50,
            beacon: None,
            local_name: Some("Other Device".to_string()),
            tx_power: None,
            connectable: true,
        };
        assert!(!filter.matches(&non_hive));
    }

    #[test]
    fn test_scan_filter_rssi() {
        let filter = ScanFilter {
            hive_only: true,
            min_rssi: Some(-70),
            ..Default::default()
        };

        let strong = make_adv(0x11111111, -60, HierarchyLevel::Platform);
        assert!(filter.matches(&strong));

        let weak = make_adv(0x22222222, -80, HierarchyLevel::Platform);
        assert!(!filter.matches(&weak));
    }

    #[test]
    fn test_find_best_parent() {
        let config = DiscoveryConfig::default();
        let mut scanner = Scanner::new(config);

        // Add a squad leader
        let squad = make_adv(0x11111111, -60, HierarchyLevel::Squad);
        scanner.process_advertisement(squad);

        // Add a platoon leader (higher hierarchy)
        std::thread::sleep(std::time::Duration::from_millis(501)); // Avoid dedup
        let platoon = make_adv(0x22222222, -70, HierarchyLevel::Platoon);
        scanner.process_advertisement(platoon);

        // Find parent for platform node
        let parent = scanner.find_best_parent(HierarchyLevel::Platform);
        assert!(parent.is_some());
        // Should prefer platoon (higher hierarchy) despite weaker signal
        assert_eq!(
            parent.unwrap().beacon.hierarchy_level,
            HierarchyLevel::Platoon
        );
    }

    #[test]
    fn test_devices_by_rssi() {
        let config = DiscoveryConfig::default();
        let mut scanner = Scanner::new(config);

        scanner.process_advertisement(make_adv(0x11111111, -80, HierarchyLevel::Platform));
        std::thread::sleep(std::time::Duration::from_millis(501));
        scanner.process_advertisement(make_adv(0x22222222, -50, HierarchyLevel::Platform));
        std::thread::sleep(std::time::Duration::from_millis(501));
        scanner.process_advertisement(make_adv(0x33333333, -70, HierarchyLevel::Platform));

        let sorted = scanner.devices_by_rssi();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].rssi, -50); // Strongest first
        assert_eq!(sorted[1].rssi, -70);
        assert_eq!(sorted[2].rssi, -80);
    }
}
