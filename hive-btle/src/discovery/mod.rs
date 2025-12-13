//! HIVE Discovery Module
//!
//! This module implements BLE discovery for HIVE mesh networks, including:
//! - Beacon format encoding/decoding
//! - Advertising for broadcasting presence
//! - Scanning for discovering peers
//!
//! ## Discovery Flow
//!
//! 1. **Advertising**: Nodes broadcast their presence using HIVE beacons
//!    containing node ID, hierarchy level, capabilities, and battery status.
//!
//! 2. **Scanning**: Nodes scan for HIVE beacons, filtering by hierarchy level
//!    and signal strength to find potential parents.
//!
//! 3. **Parent Selection**: The scanner tracks discovered devices and selects
//!    the best parent candidate based on hierarchy level and RSSI.
//!
//! ## Example
//!
//! ```ignore
//! use hive_btle::discovery::{Advertiser, Scanner, ScanFilter};
//! use hive_btle::{NodeId, HierarchyLevel};
//! use hive_btle::config::DiscoveryConfig;
//!
//! // Create advertiser
//! let config = DiscoveryConfig::default();
//! let mut advertiser = Advertiser::new(config.clone(), NodeId::new(0x12345678))
//!     .with_name("HIVE-Node".to_string());
//!
//! advertiser.set_hierarchy_level(HierarchyLevel::Squad);
//! advertiser.start();
//!
//! // Create scanner
//! let mut scanner = Scanner::new(config);
//! scanner.set_filter(ScanFilter::potential_parents(HierarchyLevel::Platform));
//! scanner.start();
//! ```

mod advertiser;
mod beacon;
mod scanner;

pub use advertiser::{Advertiser, AdvertiserState, AdvertisingPacket};
pub use beacon::{
    HiveBeacon, ParsedAdvertisement, BEACON_COMPACT_SIZE, BEACON_SIZE, BEACON_VERSION,
};
pub use scanner::{ScanFilter, Scanner, ScannerState, TrackedDevice};
