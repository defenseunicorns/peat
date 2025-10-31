//! Geographic self-organization strategy for bootstrap phase
//!
//! Implements geohash-based spatial clustering for autonomous cell formation.
//!
//! # Architecture
//!
//! This module implements a two-layer beacon management system:
//!
//! ## Ditto Layer (Mesh Network)
//! - Each platform maintains ONE beacon document: `platform_beacons/{platform_id}`
//! - Documents have a 30-second TTL for automatic expiration
//! - Updates are LWW-Register CRDTs (no write conflicts)
//! - Platforms query by geohash for proximity-based discovery
//!
//! ## Local Memory Layer (GeographicDiscovery)
//! - Each platform maintains an in-memory cache of received beacons
//! - Janitor service periodically cleans expired beacons from cache
//! - Provides defense-in-depth against stale data
//!
//! See: docs/ADR-002-Beacon-Storage-Architecture.md

use crate::discovery::geo::GeoCoordinate;
use crate::models::capability::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Geohash precision for ~153m cells
pub const DEFAULT_GEOHASH_PRECISION: usize = 7;

/// Beacon Time-To-Live (seconds)
///
/// Beacons expire after this duration if not updated. This value is used in:
/// - Ditto document TTL (automatic mesh cleanup)
/// - Local cache expiration checks (janitor service)
///
/// Balances:
/// - Responsiveness: Detect offline nodes quickly
/// - Network efficiency: Reduce unnecessary re-broadcasts
/// - DDIL tolerance: Account for intermittent connectivity
pub const BEACON_TTL_SECONDS: u64 = 30;

/// Minimum nodes required to form a squad
pub const MIN_SQUAD_SIZE: usize = 2;

/// Node beacon for geographic discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeographicBeacon {
    pub platform_id: String,
    pub position: GeoCoordinate,
    pub geohash_cell: String,
    pub operational: bool,
    pub timestamp: u64,
    pub capabilities: Vec<String>,
}

impl GeographicBeacon {
    /// Create a new geographic beacon
    pub fn new(
        platform_id: String,
        position: GeoCoordinate,
        capabilities: Vec<Capability>,
    ) -> Self {
        let geohash_cell = encode_geohash(&position, DEFAULT_GEOHASH_PRECISION);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            platform_id,
            position,
            geohash_cell,
            operational: true,
            timestamp,
            capabilities: capabilities.iter().map(|c| c.id.clone()).collect(),
        }
    }

    /// Check if beacon is expired
    ///
    /// Returns true if the beacon timestamp is older than BEACON_TTL_SECONDS.
    /// This matches the Ditto document TTL for consistency.
    pub fn is_expired(&self, current_time: u64) -> bool {
        current_time.saturating_sub(self.timestamp) > BEACON_TTL_SECONDS
    }
}

/// Geographic cluster of nodes in the same geohash cell
#[derive(Debug, Clone)]
pub struct GeographicCluster {
    pub geohash_cell: String,
    pub platforms: Vec<GeographicBeacon>,
    pub center: GeoCoordinate,
}

impl GeographicCluster {
    /// Create a new cluster for a geohash cell
    pub fn new(geohash_cell: String) -> Result<Self, &'static str> {
        let center = decode_geohash(&geohash_cell)?;
        Ok(Self {
            geohash_cell,
            platforms: Vec::new(),
            center,
        })
    }

    /// Add a beacon to the cluster
    pub fn add_beacon(&mut self, beacon: GeographicBeacon) {
        self.platforms.push(beacon);
    }

    /// Remove expired beacons
    pub fn remove_expired(&mut self, current_time: u64) {
        self.platforms.retain(|b| !b.is_expired(current_time));
    }

    /// Check if cluster has enough nodes to form a squad
    pub fn can_form_squad(&self, min_size: usize) -> bool {
        self.platforms.len() >= min_size
    }

    /// Get platform IDs in this cluster
    pub fn platform_ids(&self) -> Vec<String> {
        self.platforms
            .iter()
            .map(|b| b.platform_id.clone())
            .collect()
    }
}

/// Encode a geographic coordinate as a geohash
pub fn encode_geohash(coord: &GeoCoordinate, precision: usize) -> String {
    let c = geohash::Coord {
        x: coord.lon,
        y: coord.lat,
    };
    geohash::encode(c, precision).expect("Valid coordinate")
}

/// Decode a geohash back to a coordinate
pub fn decode_geohash(hash: &str) -> Result<GeoCoordinate, &'static str> {
    let (coord, _, _) = geohash::decode(hash).map_err(|_| "Invalid geohash")?;
    GeoCoordinate::new(coord.y, coord.x, 0.0)
}

/// Geographic discovery manager for organizing nodes into squads
///
/// # Architecture
///
/// GeographicDiscovery maintains an in-memory cache of received beacons from the
/// Ditto mesh network. This cache requires periodic cleanup via a janitor service.
///
/// ## Usage Pattern
///
/// ```rust,ignore
/// // Create discovery manager
/// let discovery = Arc::new(Mutex::new(
///     GeographicDiscovery::new("platform_1".to_string())
/// ));
///
/// // Spawn janitor service (runs periodically)
/// let janitor_discovery = discovery.clone();
/// tokio::spawn(async move {
///     let mut interval = tokio::time::interval(Duration::from_secs(10));
///     loop {
///         interval.tick().await;
///         janitor_discovery.lock().unwrap().cleanup_expired();
///     }
/// });
///
/// // Process beacons from Ditto
/// ditto.observe_beacons(|beacon| {
///     discovery.lock().unwrap().process_beacon(beacon);
/// });
/// ```
pub struct GeographicDiscovery {
    clusters: HashMap<String, GeographicCluster>,
    my_platform_id: String,
}

impl GeographicDiscovery {
    /// Create a new geographic discovery manager
    pub fn new(platform_id: String) -> Self {
        Self {
            clusters: HashMap::new(),
            my_platform_id: platform_id,
        }
    }

    /// Process a received beacon
    pub fn process_beacon(&mut self, beacon: GeographicBeacon) {
        let geohash = beacon.geohash_cell.clone();

        self.clusters
            .entry(geohash.clone())
            .or_insert_with(|| GeographicCluster::new(geohash).unwrap())
            .add_beacon(beacon);
    }

    /// Clean up expired beacons
    pub fn cleanup_expired(&mut self) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for cluster in self.clusters.values_mut() {
            cluster.remove_expired(current_time);
        }

        // Remove empty clusters
        self.clusters
            .retain(|_, cluster| !cluster.platforms.is_empty());
    }

    /// Find clusters that can form squads
    pub fn find_formable_squads(&self, min_size: usize) -> Vec<&GeographicCluster> {
        self.clusters
            .values()
            .filter(|c| c.can_form_squad(min_size))
            .collect()
    }

    /// Get the cluster containing this platform
    pub fn my_cluster(&self) -> Option<&GeographicCluster> {
        self.clusters.values().find(|c| {
            c.platforms
                .iter()
                .any(|b| b.platform_id == self.my_platform_id)
        })
    }

    /// Check if this platform should initiate cell formation
    /// Returns true if this platform is the "leader" (lowest ID) in its cluster
    pub fn should_initiate_squad_formation(&self) -> bool {
        if let Some(cluster) = self.my_cluster() {
            if cluster.can_form_squad(MIN_SQUAD_SIZE) {
                // Check if we're the lowest platform ID (deterministic leader selection)
                if let Some(min_id) = cluster.platforms.iter().map(|b| &b.platform_id).min() {
                    return min_id == &self.my_platform_id;
                }
            }
        }
        false
    }

    /// Get proposed squad members from my cluster
    pub fn get_squad_members(&self, max_size: usize) -> Option<Vec<String>> {
        if let Some(cluster) = self.my_cluster() {
            if cluster.can_form_squad(MIN_SQUAD_SIZE) {
                let mut members = cluster.platform_ids();
                members.sort(); // Deterministic ordering
                members.truncate(max_size);
                return Some(members);
            }
        }
        None
    }

    /// Get total number of discovered platforms
    pub fn total_platforms(&self) -> usize {
        self.clusters.values().map(|c| c.platforms.len()).sum()
    }

    /// Get number of active clusters
    pub fn cluster_count(&self) -> usize {
        self.clusters.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geohash_encoding() {
        let coord = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();
        let hash = encode_geohash(&coord, 7);
        assert_eq!(hash.len(), 7);
        assert!(hash.starts_with("9q8yy")); // SF area
    }

    #[test]
    fn test_geohash_decoding() {
        let hash = "9q8yyk8";
        let coord = decode_geohash(hash).unwrap();
        // Should be approximately SF coordinates
        assert!((coord.lat - 37.77).abs() < 0.01);
        assert!((coord.lon - (-122.41)).abs() < 0.01);
    }

    #[test]
    fn test_beacon_creation() {
        use crate::models::capability::CapabilityType;

        let pos = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();
        let caps = vec![
            Capability {
                id: "intel1".to_string(),
                name: "Intelligence".to_string(),
                capability_type: CapabilityType::Sensor,
                confidence: 0.9,
                metadata: serde_json::json!({}),
            },
            Capability {
                id: "comms1".to_string(),
                name: "Communications".to_string(),
                capability_type: CapabilityType::Communication,
                confidence: 0.95,
                metadata: serde_json::json!({}),
            },
        ];

        let beacon = GeographicBeacon::new("platform_1".to_string(), pos, caps);

        assert_eq!(beacon.platform_id, "platform_1");
        assert_eq!(beacon.position, pos);
        assert!(beacon.geohash_cell.starts_with("9q8yy"));
        assert!(beacon.operational);
        assert_eq!(beacon.capabilities.len(), 2);
    }

    #[test]
    fn test_beacon_expiration() {
        let pos = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();
        let beacon = GeographicBeacon::new("platform_1".to_string(), pos, vec![]);

        // Not expired immediately
        assert!(!beacon.is_expired(beacon.timestamp));

        // Expired after 31 seconds
        assert!(beacon.is_expired(beacon.timestamp + 31));
    }

    #[test]
    fn test_cluster_creation() {
        let cluster = GeographicCluster::new("9q8yyk8".to_string()).unwrap();
        assert_eq!(cluster.geohash_cell, "9q8yyk8");
        assert_eq!(cluster.platforms.len(), 0);
        assert!(!cluster.can_form_squad(2));
    }

    #[test]
    fn test_cluster_beacon_management() {
        let mut cluster = GeographicCluster::new("9q8yyk8".to_string()).unwrap();
        let pos = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();

        let beacon1 = GeographicBeacon::new("platform_1".to_string(), pos, vec![]);
        let beacon2 = GeographicBeacon::new("platform_2".to_string(), pos, vec![]);

        cluster.add_beacon(beacon1);
        cluster.add_beacon(beacon2);

        assert_eq!(cluster.platforms.len(), 2);
        assert!(cluster.can_form_squad(2));

        let ids = cluster.platform_ids();
        assert!(ids.contains(&"platform_1".to_string()));
        assert!(ids.contains(&"platform_2".to_string()));
    }

    #[test]
    fn test_discovery_basic_operations() {
        let mut discovery = GeographicDiscovery::new("platform_1".to_string());
        assert_eq!(discovery.total_platforms(), 0);
        assert_eq!(discovery.cluster_count(), 0);

        let pos = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();
        let beacon = GeographicBeacon::new("platform_2".to_string(), pos, vec![]);

        discovery.process_beacon(beacon);

        assert_eq!(discovery.total_platforms(), 1);
        assert_eq!(discovery.cluster_count(), 1);
    }

    #[test]
    fn test_discovery_squad_formation() {
        let mut discovery = GeographicDiscovery::new("platform_1".to_string());
        let pos = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();

        // Add own beacon
        let beacon1 = GeographicBeacon::new("platform_1".to_string(), pos, vec![]);
        discovery.process_beacon(beacon1);

        // Add another platform in same location
        let beacon2 = GeographicBeacon::new("platform_2".to_string(), pos, vec![]);
        discovery.process_beacon(beacon2);

        assert_eq!(discovery.total_platforms(), 2);

        // Should be able to form cells now
        let formable = discovery.find_formable_squads(2);
        assert_eq!(formable.len(), 1);

        // platform_1 should be leader (lowest ID)
        assert!(discovery.should_initiate_squad_formation());

        // Get squad members
        let members = discovery.get_squad_members(5).unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains(&"platform_1".to_string()));
        assert!(members.contains(&"platform_2".to_string()));
    }

    #[test]
    fn test_discovery_multiple_clusters() {
        let mut discovery = GeographicDiscovery::new("platform_1".to_string());

        // Cluster 1: SF
        let pos1 = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();
        let beacon1 = GeographicBeacon::new("platform_1".to_string(), pos1, vec![]);
        let beacon2 = GeographicBeacon::new("platform_2".to_string(), pos1, vec![]);

        // Cluster 2: LA (different geohash)
        let pos2 = GeoCoordinate::new(34.0522, -118.2437, 100.0).unwrap();
        let beacon3 = GeographicBeacon::new("platform_3".to_string(), pos2, vec![]);
        let beacon4 = GeographicBeacon::new("platform_4".to_string(), pos2, vec![]);

        discovery.process_beacon(beacon1);
        discovery.process_beacon(beacon2);
        discovery.process_beacon(beacon3);
        discovery.process_beacon(beacon4);

        assert_eq!(discovery.total_platforms(), 4);
        assert_eq!(discovery.cluster_count(), 2);

        let formable = discovery.find_formable_squads(2);
        assert_eq!(formable.len(), 2);
    }

    #[test]
    fn test_discovery_cleanup_expired() {
        let mut discovery = GeographicDiscovery::new("platform_1".to_string());
        let pos = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();

        let mut beacon = GeographicBeacon::new("platform_2".to_string(), pos, vec![]);
        beacon.timestamp = 0; // Set to very old timestamp

        discovery.process_beacon(beacon);
        assert_eq!(discovery.total_platforms(), 1);

        discovery.cleanup_expired();
        assert_eq!(discovery.total_platforms(), 0);
        assert_eq!(discovery.cluster_count(), 0);
    }

    #[test]
    fn test_deterministic_leader_selection() {
        // Test that lowest ID is always selected as leader
        let mut discovery1 = GeographicDiscovery::new("platform_a".to_string());
        let mut discovery2 = GeographicDiscovery::new("platform_b".to_string());
        let mut discovery3 = GeographicDiscovery::new("platform_c".to_string());

        let pos = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();

        for id in ["platform_a", "platform_b", "platform_c"] {
            let beacon = GeographicBeacon::new(id.to_string(), pos, vec![]);
            discovery1.process_beacon(beacon.clone());
            discovery2.process_beacon(beacon.clone());
            discovery3.process_beacon(beacon);
        }

        // Only platform_a should be leader
        assert!(discovery1.should_initiate_squad_formation());
        assert!(!discovery2.should_initiate_squad_formation());
        assert!(!discovery3.should_initiate_squad_formation());
    }
}
