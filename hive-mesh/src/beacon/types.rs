use serde::{Deserialize, Serialize};

/// Geographic position with latitude, longitude, and optional altitude
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPosition {
    /// Latitude in decimal degrees (-90 to 90)
    pub lat: f64,

    /// Longitude in decimal degrees (-180 to 180)
    pub lon: f64,

    /// Altitude in meters above sea level (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<f64>,
}

impl GeoPosition {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            lat,
            lon,
            alt: None,
        }
    }

    pub fn with_altitude(mut self, alt: f64) -> Self {
        self.alt = Some(alt);
        self
    }

    /// Calculate Haversine distance to another position in meters
    pub fn distance_to(&self, other: &GeoPosition) -> f64 {
        let r = 6371000.0; // Earth radius in meters
        let lat1 = self.lat.to_radians();
        let lat2 = other.lat.to_radians();
        let delta_lat = (other.lat - self.lat).to_radians();
        let delta_lon = (other.lon - self.lon).to_radians();

        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

        r * c
    }
}

/// Hierarchy level in military command structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum HierarchyLevel {
    /// Individual platform (vehicle, soldier, etc.)
    Platform = 0,
    /// Squad level (typically 4-13 platforms)
    Squad = 1,
    /// Platoon level (typically 2-4 squads)
    Platoon = 2,
    /// Company level (typically 2-4 platoons)
    Company = 3,
}

impl HierarchyLevel {
    /// Get the parent level in the hierarchy
    pub fn parent(&self) -> Option<HierarchyLevel> {
        match self {
            HierarchyLevel::Platform => Some(HierarchyLevel::Squad),
            HierarchyLevel::Squad => Some(HierarchyLevel::Platoon),
            HierarchyLevel::Platoon => Some(HierarchyLevel::Company),
            HierarchyLevel::Company => None,
        }
    }

    /// Get the child level in the hierarchy
    pub fn child(&self) -> Option<HierarchyLevel> {
        match self {
            HierarchyLevel::Platform => None,
            HierarchyLevel::Squad => Some(HierarchyLevel::Platform),
            HierarchyLevel::Platoon => Some(HierarchyLevel::Squad),
            HierarchyLevel::Company => Some(HierarchyLevel::Platoon),
        }
    }
}

impl std::fmt::Display for HierarchyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HierarchyLevel::Platform => write!(f, "Platform"),
            HierarchyLevel::Squad => write!(f, "Squad"),
            HierarchyLevel::Platoon => write!(f, "Platoon"),
            HierarchyLevel::Company => write!(f, "Company"),
        }
    }
}

/// Geographic beacon representing a node's presence and status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeographicBeacon {
    /// Unique node identifier
    pub node_id: String,

    /// Geographic position
    pub position: GeoPosition,

    /// Geohash encoding of position (precision 7 = ~153m cells)
    pub geohash: String,

    /// Hierarchy level in command structure
    pub hierarchy_level: HierarchyLevel,

    /// Node capabilities (e.g., "ai-inference", "sensor-fusion", etc.)
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Whether node is operational
    pub operational: bool,

    /// Unix timestamp (seconds since epoch)
    pub timestamp: u64,

    /// Node mobility type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mobility: Option<super::config::NodeMobility>,

    /// Whether this node can act as a parent
    #[serde(default)]
    pub can_parent: bool,

    /// Parent selection priority (0-255, higher = more preferred)
    #[serde(default)]
    pub parent_priority: u8,

    /// Current resource metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<super::config::NodeResources>,

    /// Optional metadata
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub metadata: std::collections::HashMap<String, String>,
}

impl GeographicBeacon {
    /// Create a new beacon
    pub fn new(node_id: String, position: GeoPosition, hierarchy_level: HierarchyLevel) -> Self {
        use geohash::encode;
        use geohash::Coord;

        let geohash = encode(
            Coord {
                x: position.lon,
                y: position.lat,
            },
            7,
        )
        .unwrap_or_else(|_| String::from("invalid"));

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            node_id,
            position,
            geohash,
            hierarchy_level,
            capabilities: Vec::new(),
            operational: true,
            timestamp,
            mobility: None,
            can_parent: false,
            parent_priority: 0,
            resources: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Update the timestamp to current time
    pub fn update_timestamp(&mut self) {
        self.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }

    /// Add a capability
    pub fn add_capability(&mut self, capability: String) {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
        }
    }

    /// Check if beacon has a specific capability
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    /// Get age of beacon in seconds
    pub fn age_seconds(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .saturating_sub(self.timestamp)
    }

    /// Check if beacon is expired based on TTL
    pub fn is_expired(&self, ttl_seconds: u64) -> bool {
        self.age_seconds() > ttl_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geo_position_distance() {
        // Test distance between two known points (approximately)
        let pos1 = GeoPosition::new(37.7749, -122.4194); // San Francisco
        let pos2 = GeoPosition::new(34.0522, -118.2437); // Los Angeles

        let distance = pos1.distance_to(&pos2);

        // Distance should be approximately 559 km
        assert!(distance > 550_000.0 && distance < 570_000.0);
    }

    #[test]
    fn test_hierarchy_level_parent_child() {
        assert_eq!(
            HierarchyLevel::Platform.parent(),
            Some(HierarchyLevel::Squad)
        );
        assert_eq!(
            HierarchyLevel::Squad.parent(),
            Some(HierarchyLevel::Platoon)
        );
        assert_eq!(
            HierarchyLevel::Platoon.parent(),
            Some(HierarchyLevel::Company)
        );
        assert_eq!(HierarchyLevel::Company.parent(), None);

        assert_eq!(HierarchyLevel::Platform.child(), None);
        assert_eq!(
            HierarchyLevel::Squad.child(),
            Some(HierarchyLevel::Platform)
        );
        assert_eq!(HierarchyLevel::Platoon.child(), Some(HierarchyLevel::Squad));
        assert_eq!(
            HierarchyLevel::Company.child(),
            Some(HierarchyLevel::Platoon)
        );
    }

    #[test]
    fn test_geographic_beacon_creation() {
        let position = GeoPosition::new(37.7749, -122.4194);
        let beacon = GeographicBeacon::new(
            "test-node-1".to_string(),
            position,
            HierarchyLevel::Platform,
        );

        assert_eq!(beacon.node_id, "test-node-1");
        assert_eq!(beacon.position.lat, 37.7749);
        assert_eq!(beacon.hierarchy_level, HierarchyLevel::Platform);
        assert!(beacon.operational);
        assert!(!beacon.geohash.is_empty());
    }

    #[test]
    fn test_beacon_capabilities() {
        let position = GeoPosition::new(37.7749, -122.4194);
        let mut beacon =
            GeographicBeacon::new("test-node".to_string(), position, HierarchyLevel::Squad);

        beacon.add_capability("ai-inference".to_string());
        beacon.add_capability("sensor-fusion".to_string());

        assert!(beacon.has_capability("ai-inference"));
        assert!(beacon.has_capability("sensor-fusion"));
        assert!(!beacon.has_capability("non-existent"));
    }

    #[test]
    fn test_beacon_serialization() {
        let position = GeoPosition::new(37.7749, -122.4194);
        let beacon =
            GeographicBeacon::new("test-node".to_string(), position, HierarchyLevel::Platform);

        // Test serialization
        let json = serde_json::to_string(&beacon).unwrap();
        assert!(json.contains("test-node"));
        assert!(json.contains("37.7749"));

        // Test deserialization
        let deserialized: GeographicBeacon = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.node_id, beacon.node_id);
        assert_eq!(deserialized.position.lat, beacon.position.lat);
    }
}
