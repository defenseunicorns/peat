use serde::{Deserialize, Serialize};

/// Node mobility type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeMobility {
    /// Static node (command post, fixed infrastructure)
    Static,
    /// Mobile node (vehicle, soldier)
    Mobile,
    /// Semi-mobile (can relocate but not constantly moving)
    SemiMobile,
}

/// Node resource profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResources {
    /// CPU cores available for mesh operations
    pub cpu_cores: u8,

    /// Memory available in MB
    pub memory_mb: u32,

    /// Network bandwidth capacity in Mbps
    pub bandwidth_mbps: u32,

    /// Current CPU usage percentage (0-100)
    #[serde(default)]
    pub cpu_usage_percent: u8,

    /// Current memory usage percentage (0-100)
    #[serde(default)]
    pub memory_usage_percent: u8,

    /// Battery level if applicable (0-100, None if AC powered)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_percent: Option<u8>,
}

impl NodeResources {
    /// Check if node has sufficient resources to be a parent
    pub fn can_parent(&self, config: &ParentingRequirements) -> bool {
        self.cpu_cores >= config.min_cpu_cores
            && self.memory_mb >= config.min_memory_mb
            && self.bandwidth_mbps >= config.min_bandwidth_mbps
            && self.cpu_usage_percent < config.max_cpu_usage_percent
            && self.memory_usage_percent < config.max_memory_usage_percent
            && self.has_sufficient_battery(config.min_battery_percent)
    }

    fn has_sufficient_battery(&self, min_battery: Option<u8>) -> bool {
        match (self.battery_percent, min_battery) {
            (Some(battery), Some(min)) => battery >= min,
            (None, _) => true,       // AC powered, always OK
            (Some(_), None) => true, // No battery requirement
        }
    }
}

/// Requirements for a node to act as a parent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParentingRequirements {
    pub min_cpu_cores: u8,
    pub min_memory_mb: u32,
    pub min_bandwidth_mbps: u32,
    pub max_cpu_usage_percent: u8,
    pub max_memory_usage_percent: u8,
    pub min_battery_percent: Option<u8>,
}

impl Default for ParentingRequirements {
    fn default() -> Self {
        Self {
            min_cpu_cores: 2,
            min_memory_mb: 512,
            min_bandwidth_mbps: 10,
            max_cpu_usage_percent: 80,
            max_memory_usage_percent: 80,
            min_battery_percent: Some(20),
        }
    }
}

/// Node profile combining mobility and resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeProfile {
    /// Mobility type
    pub mobility: NodeMobility,

    /// Resource capacity
    pub resources: NodeResources,

    /// Whether this node can act as a parent
    pub can_parent: bool,

    /// Whether this node prefers to be a leaf (never parent)
    pub prefer_leaf: bool,

    /// Priority for parent selection (higher = more preferred)
    pub parent_priority: u8,
}

impl NodeProfile {
    /// Create a static node profile (command post, fixed infrastructure)
    pub fn static_node(resources: NodeResources) -> Self {
        Self {
            mobility: NodeMobility::Static,
            resources,
            can_parent: true,
            prefer_leaf: false,
            parent_priority: 255, // Highest priority
        }
    }

    /// Create a mobile node profile (vehicle, soldier)
    pub fn mobile_node(resources: NodeResources) -> Self {
        Self {
            mobility: NodeMobility::Mobile,
            resources,
            can_parent: false, // Mobile nodes shouldn't parent by default
            prefer_leaf: true,
            parent_priority: 0, // Lowest priority
        }
    }

    /// Create a semi-mobile node profile (relocatable but stable)
    pub fn semi_mobile_node(resources: NodeResources) -> Self {
        Self {
            mobility: NodeMobility::SemiMobile,
            resources,
            can_parent: true, // Can parent if resources allow
            prefer_leaf: false,
            parent_priority: 128, // Medium priority
        }
    }

    /// Check if this node should be considered as a parent candidate
    pub fn is_parent_candidate(&self, requirements: &ParentingRequirements) -> bool {
        if self.prefer_leaf || !self.can_parent {
            return false;
        }

        // Static nodes are always good candidates if they meet resource requirements
        if self.mobility == NodeMobility::Static {
            return self.resources.can_parent(requirements);
        }

        // Mobile nodes should not parent
        if self.mobility == NodeMobility::Mobile {
            return false;
        }

        // Semi-mobile can parent if resources are sufficient
        self.resources.can_parent(requirements)
    }
}

/// Beacon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconConfig {
    /// Geohash precision (5-9, default 7)
    /// 5 = ~4.9km cells, 6 = ~1.2km, 7 = ~153m, 8 = ~38m, 9 = ~4.8m
    pub geohash_precision: usize,

    /// Whether to track all beacons regardless of distance
    pub track_all: bool,

    /// Maximum distance in meters to track (None = unlimited)
    pub max_distance_meters: Option<f64>,

    /// Broadcast interval
    pub broadcast_interval_secs: u64,

    /// Beacon TTL (time to live)
    pub ttl_secs: u64,

    /// Cleanup interval for janitor
    pub cleanup_interval_secs: u64,
}

impl Default for BeaconConfig {
    fn default() -> Self {
        Self {
            geohash_precision: 7,
            track_all: false,
            max_distance_meters: Some(5000.0), // 5km default
            broadcast_interval_secs: 5,
            ttl_secs: 30,
            cleanup_interval_secs: 5,
        }
    }
}

impl BeaconConfig {
    /// Tactical field operations - tight proximity, frequent updates
    pub fn tactical() -> Self {
        Self {
            geohash_precision: 8, // ~38m cells
            track_all: false,
            max_distance_meters: Some(2000.0), // 2km
            broadcast_interval_secs: 3,
            ttl_secs: 15,
            cleanup_interval_secs: 3,
        }
    }

    /// Distributed cloud - no proximity filtering
    pub fn distributed() -> Self {
        Self {
            geohash_precision: 5, // ~4.9km cells (doesn't matter much)
            track_all: true,
            max_distance_meters: None,
            broadcast_interval_secs: 10,
            ttl_secs: 60,
            cleanup_interval_secs: 10,
        }
    }

    /// Hybrid - balance between proximity and connectivity
    pub fn hybrid() -> Self {
        Self {
            geohash_precision: 6, // ~1.2km cells
            track_all: false,
            max_distance_meters: Some(10000.0), // 10km
            broadcast_interval_secs: 5,
            ttl_secs: 30,
            cleanup_interval_secs: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_mobility_types() {
        let static_resources = NodeResources {
            cpu_cores: 4,
            memory_mb: 2048,
            bandwidth_mbps: 100,
            cpu_usage_percent: 30,
            memory_usage_percent: 40,
            battery_percent: None, // AC powered
        };

        let mobile_resources = NodeResources {
            cpu_cores: 2,
            memory_mb: 512,
            bandwidth_mbps: 20,
            cpu_usage_percent: 50,
            memory_usage_percent: 60,
            battery_percent: Some(75),
        };

        let static_profile = NodeProfile::static_node(static_resources);
        let mobile_profile = NodeProfile::mobile_node(mobile_resources);

        let requirements = ParentingRequirements::default();

        // Static node should be a parent candidate
        assert!(static_profile.is_parent_candidate(&requirements));
        assert_eq!(static_profile.parent_priority, 255);

        // Mobile node should NOT be a parent candidate
        assert!(!mobile_profile.is_parent_candidate(&requirements));
        assert_eq!(mobile_profile.parent_priority, 0);
    }

    #[test]
    fn test_resource_based_parenting() {
        let low_resources = NodeResources {
            cpu_cores: 1,
            memory_mb: 256,
            bandwidth_mbps: 5,
            cpu_usage_percent: 90,
            memory_usage_percent: 85,
            battery_percent: Some(15),
        };

        let high_resources = NodeResources {
            cpu_cores: 8,
            memory_mb: 4096,
            bandwidth_mbps: 1000,
            cpu_usage_percent: 20,
            memory_usage_percent: 30,
            battery_percent: None,
        };

        let requirements = ParentingRequirements::default();

        assert!(!low_resources.can_parent(&requirements));
        assert!(high_resources.can_parent(&requirements));
    }

    #[test]
    fn test_semi_mobile_profile() {
        let resources = NodeResources {
            cpu_cores: 4,
            memory_mb: 1024,
            bandwidth_mbps: 50,
            cpu_usage_percent: 40,
            memory_usage_percent: 50,
            battery_percent: Some(80),
        };

        let profile = NodeProfile::semi_mobile_node(resources);
        let requirements = ParentingRequirements::default();

        // Semi-mobile with good resources should be parent candidate
        assert!(profile.is_parent_candidate(&requirements));
        assert_eq!(profile.parent_priority, 128);
        assert_eq!(profile.mobility, NodeMobility::SemiMobile);
    }

    #[test]
    fn test_beacon_config_presets() {
        let tactical = BeaconConfig::tactical();
        assert_eq!(tactical.geohash_precision, 8);
        assert_eq!(tactical.max_distance_meters, Some(2000.0));

        let distributed = BeaconConfig::distributed();
        assert!(distributed.track_all);
        assert_eq!(distributed.max_distance_meters, None);

        let hybrid = BeaconConfig::hybrid();
        assert_eq!(hybrid.geohash_precision, 6);
        assert_eq!(hybrid.max_distance_meters, Some(10000.0));
    }
}
