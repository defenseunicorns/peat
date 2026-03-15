//! Bridge configuration types

use std::time::Duration;

/// Echelon level in military hierarchy
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum EchelonLevel {
    /// Individual platform/asset
    #[default]
    Platform,
    /// Squad (4-12 platforms)
    Squad,
    /// Cell (multiple squads)
    Cell,
    /// Platoon
    Platoon,
    /// Company
    Company,
    /// Formation (battalion+)
    Formation,
}

/// Aggregation policy for TAK publishing
///
/// Controls how Peat messages are filtered and aggregated before
/// being sent to TAK, optimizing bandwidth usage.
#[derive(Debug, Clone, Default)]
pub enum AggregationPolicy {
    /// Full fidelity - all platforms visible (O(n) bandwidth)
    /// Use for small formations or high-bandwidth links
    #[default]
    FullFidelity,

    /// Squad leader only - cell leaders + formation aggregates
    /// Reduces traffic by showing only leadership positions
    SquadLeaderOnly,

    /// Hierarchical filtering based on viewer echelon
    /// Company HQ sees platoon summaries, not individual platforms
    HierarchicalFiltering {
        /// Viewer's echelon level
        viewer_echelon: EchelonLevel,
    },

    /// Tracks only - active enemy/unknown tracks, not friendly platforms
    /// For combat-focused displays
    TracksOnly,

    /// Capability summaries only - formation capabilities, not positions
    CapabilitySummaryOnly,

    /// Time-windowed aggregation - batch updates over time window
    TimeWindowed {
        /// Window duration in seconds
        window_secs: u64,
    },

    /// Bandwidth-adaptive - switches policy based on available bandwidth
    BandwidthAdaptive {
        /// Target bandwidth in kbps
        target_kbps: u32,
    },
}

impl AggregationPolicy {
    /// Create a hierarchical filtering policy for a specific viewer
    pub fn hierarchical(viewer_echelon: EchelonLevel) -> Self {
        Self::HierarchicalFiltering { viewer_echelon }
    }

    /// Create a time-windowed policy
    pub fn time_windowed(window_secs: u64) -> Self {
        Self::TimeWindowed { window_secs }
    }

    /// Create a bandwidth-adaptive policy
    pub fn bandwidth_adaptive(target_kbps: u32) -> Self {
        Self::BandwidthAdaptive { target_kbps }
    }

    /// Should this message echelon be visible to the viewer?
    pub fn should_publish_echelon(&self, message_echelon: EchelonLevel) -> bool {
        match self {
            Self::FullFidelity => true,
            Self::SquadLeaderOnly => message_echelon >= EchelonLevel::Squad,
            Self::HierarchicalFiltering { viewer_echelon } => {
                // Viewer sees their level and one below
                match viewer_echelon {
                    EchelonLevel::Formation => message_echelon >= EchelonLevel::Company,
                    EchelonLevel::Company => message_echelon >= EchelonLevel::Platoon,
                    EchelonLevel::Platoon => message_echelon >= EchelonLevel::Cell,
                    EchelonLevel::Cell => message_echelon >= EchelonLevel::Squad,
                    EchelonLevel::Squad => true,
                    EchelonLevel::Platform => true,
                }
            }
            Self::TracksOnly => false, // Only tracks, not platforms - handled elsewhere
            Self::CapabilitySummaryOnly => message_echelon >= EchelonLevel::Formation,
            Self::TimeWindowed { .. } => true, // Aggregated by time, not filtered
            Self::BandwidthAdaptive { .. } => true, // Dynamic filtering
        }
    }
}

/// Geographic filter configuration
#[derive(Debug, Clone, Default)]
pub struct GeoFilterConfig {
    /// Bounding box: (min_lat, min_lon, max_lat, max_lon)
    pub bounding_box: Option<(f64, f64, f64, f64)>,
    /// Center point and radius in meters
    pub radius_filter: Option<(f64, f64, f64)>,
}

/// Bridge configuration
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Bridge identifier
    pub bridge_id: String,

    /// Aggregation policy
    pub aggregation_policy: AggregationPolicy,

    /// Geographic filter (area of operations)
    pub geo_filter: Option<GeoFilterConfig>,

    /// Maximum stale time for messages (seconds)
    pub max_stale_secs: u64,

    /// TAK contact group for this bridge
    pub tak_group: Option<String>,

    /// Filter CoT types (prefix match)
    /// Empty means accept all
    pub allowed_cot_types: Vec<String>,

    /// Blocked CoT types (prefix match)
    pub blocked_cot_types: Vec<String>,

    /// Flush interval for aggregated messages
    pub flush_interval: Duration,

    /// Include Peat extension in outgoing CoT
    pub include_peat_extension: bool,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            bridge_id: "peat-bridge".to_string(),
            aggregation_policy: AggregationPolicy::default(),
            geo_filter: None,
            max_stale_secs: 300,
            tak_group: None,
            allowed_cot_types: Vec::new(),
            blocked_cot_types: Vec::new(),
            flush_interval: Duration::from_secs(5),
            include_peat_extension: true,
        }
    }
}

impl BridgeConfig {
    /// Create a new bridge config with the given ID
    pub fn new(bridge_id: impl Into<String>) -> Self {
        Self {
            bridge_id: bridge_id.into(),
            ..Default::default()
        }
    }

    /// Set aggregation policy
    pub fn with_aggregation(mut self, policy: AggregationPolicy) -> Self {
        self.aggregation_policy = policy;
        self
    }

    /// Set geographic bounding box filter
    pub fn with_bounding_box(
        mut self,
        min_lat: f64,
        min_lon: f64,
        max_lat: f64,
        max_lon: f64,
    ) -> Self {
        self.geo_filter = Some(GeoFilterConfig {
            bounding_box: Some((min_lat, min_lon, max_lat, max_lon)),
            radius_filter: None,
        });
        self
    }

    /// Set geographic radius filter
    pub fn with_radius_filter(mut self, center_lat: f64, center_lon: f64, radius_m: f64) -> Self {
        self.geo_filter = Some(GeoFilterConfig {
            bounding_box: None,
            radius_filter: Some((center_lat, center_lon, radius_m)),
        });
        self
    }

    /// Set TAK contact group
    pub fn with_tak_group(mut self, group: impl Into<String>) -> Self {
        self.tak_group = Some(group.into());
        self
    }

    /// Add allowed CoT type prefix
    pub fn allow_cot_type(mut self, type_prefix: impl Into<String>) -> Self {
        self.allowed_cot_types.push(type_prefix.into());
        self
    }

    /// Add blocked CoT type prefix
    pub fn block_cot_type(mut self, type_prefix: impl Into<String>) -> Self {
        self.blocked_cot_types.push(type_prefix.into());
        self
    }

    /// Set flush interval
    pub fn with_flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = interval;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregation_policy_echelon_filtering() {
        // Full fidelity allows everything
        let policy = AggregationPolicy::FullFidelity;
        assert!(policy.should_publish_echelon(EchelonLevel::Platform));
        assert!(policy.should_publish_echelon(EchelonLevel::Formation));

        // Squad leader only filters platforms
        let policy = AggregationPolicy::SquadLeaderOnly;
        assert!(!policy.should_publish_echelon(EchelonLevel::Platform));
        assert!(policy.should_publish_echelon(EchelonLevel::Squad));
        assert!(policy.should_publish_echelon(EchelonLevel::Formation));

        // Hierarchical: Company HQ sees platoon and up
        let policy = AggregationPolicy::hierarchical(EchelonLevel::Company);
        assert!(!policy.should_publish_echelon(EchelonLevel::Platform));
        assert!(!policy.should_publish_echelon(EchelonLevel::Squad));
        assert!(!policy.should_publish_echelon(EchelonLevel::Cell));
        assert!(policy.should_publish_echelon(EchelonLevel::Platoon));
        assert!(policy.should_publish_echelon(EchelonLevel::Company));
    }

    #[test]
    fn test_config_builder() {
        let config = BridgeConfig::new("test-bridge")
            .with_aggregation(AggregationPolicy::SquadLeaderOnly)
            .with_tak_group("ALPHA")
            .allow_cot_type("a-f-")
            .block_cot_type("b-m-");

        assert_eq!(config.bridge_id, "test-bridge");
        assert_eq!(config.tak_group, Some("ALPHA".to_string()));
        assert_eq!(config.allowed_cot_types, vec!["a-f-"]);
        assert_eq!(config.blocked_cot_types, vec!["b-m-"]);
    }

    #[test]
    fn test_echelon_ordering() {
        assert!(EchelonLevel::Platform < EchelonLevel::Squad);
        assert!(EchelonLevel::Squad < EchelonLevel::Cell);
        assert!(EchelonLevel::Cell < EchelonLevel::Formation);
    }
}
