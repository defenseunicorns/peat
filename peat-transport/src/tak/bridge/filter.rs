//! Message filtering for Peat-TAK bridge

use super::config::{AggregationPolicy, BridgeConfig, GeoFilterConfig};
use super::PeatMessage;

/// Decision from filter evaluation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterDecision {
    /// Message should be published immediately
    Publish,
    /// Message should be dropped (with reason)
    Drop(String),
    /// Message should be aggregated for later
    Aggregate,
}

/// Geographic filter
#[derive(Debug, Clone)]
pub struct GeoFilter {
    /// Bounding box: (min_lat, min_lon, max_lat, max_lon)
    bounding_box: Option<(f64, f64, f64, f64)>,
    /// Center point and radius in meters
    radius_filter: Option<(f64, f64, f64)>,
}

impl GeoFilter {
    /// Create a new geo filter from config
    pub fn from_config(config: &GeoFilterConfig) -> Self {
        Self {
            bounding_box: config.bounding_box,
            radius_filter: config.radius_filter,
        }
    }

    /// Check if a position passes the geo filter
    pub fn passes(&self, lat: f64, lon: f64) -> bool {
        // Check bounding box
        if let Some((min_lat, min_lon, max_lat, max_lon)) = self.bounding_box {
            if lat < min_lat || lat > max_lat || lon < min_lon || lon > max_lon {
                return false;
            }
        }

        // Check radius
        if let Some((center_lat, center_lon, radius_m)) = self.radius_filter {
            let distance = haversine_distance(lat, lon, center_lat, center_lon);
            if distance > radius_m {
                return false;
            }
        }

        true
    }
}

/// Calculate haversine distance between two points in meters
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_M * c
}

/// Bridge filter for message selection
#[derive(Debug, Clone)]
pub struct BridgeFilter {
    /// Aggregation policy
    aggregation_policy: AggregationPolicy,
    /// Geographic filter
    geo_filter: Option<GeoFilter>,
    /// Allowed CoT type prefixes (for future type-based filtering)
    #[allow(dead_code)]
    allowed_types: Vec<String>,
    /// Blocked CoT type prefixes (for future type-based filtering)
    #[allow(dead_code)]
    blocked_types: Vec<String>,
}

impl BridgeFilter {
    /// Create filter from bridge config
    pub fn from_config(config: &BridgeConfig) -> Self {
        Self {
            aggregation_policy: config.aggregation_policy.clone(),
            geo_filter: config.geo_filter.as_ref().map(GeoFilter::from_config),
            allowed_types: config.allowed_cot_types.clone(),
            blocked_types: config.blocked_cot_types.clone(),
        }
    }

    /// Evaluate whether a message should be published
    pub fn should_publish(&self, message: &PeatMessage) -> FilterDecision {
        // Check echelon filtering
        if !self
            .aggregation_policy
            .should_publish_echelon(message.echelon())
        {
            return FilterDecision::Drop(format!(
                "Echelon {:?} filtered by policy",
                message.echelon()
            ));
        }

        // Check geographic filter
        if let Some(geo_filter) = &self.geo_filter {
            if let Some((lat, lon)) = message.position() {
                if !geo_filter.passes(lat, lon) {
                    return FilterDecision::Drop("Outside geographic filter".to_string());
                }
            }
        }

        // Check time-windowed aggregation
        if let AggregationPolicy::TimeWindowed { .. } = &self.aggregation_policy {
            return FilterDecision::Aggregate;
        }

        // TracksOnly policy - only publish track messages that are hostile/unknown
        if let AggregationPolicy::TracksOnly = &self.aggregation_policy {
            match message {
                PeatMessage::Track(track) => {
                    // Only pass hostile (a-h-*) or unknown (a-u-*) tracks
                    let cot_type = track.classification.as_str();
                    if !cot_type.starts_with("a-h") && !cot_type.starts_with("a-u") {
                        return FilterDecision::Drop(
                            "TracksOnly: filtering friendly platform".to_string(),
                        );
                    }
                }
                PeatMessage::Capability(_) | PeatMessage::FormationSummary(_) => {
                    return FilterDecision::Drop(
                        "TracksOnly: filtering non-track message".to_string(),
                    );
                }
                PeatMessage::Handoff(_) => {
                    // Handoffs are always important
                }
            }
        }

        FilterDecision::Publish
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_distance() {
        // Los Angeles to San Francisco (approx 560 km)
        let la_lat = 34.0522;
        let la_lon = -118.2437;
        let sf_lat = 37.7749;
        let sf_lon = -122.4194;

        let distance = haversine_distance(la_lat, la_lon, sf_lat, sf_lon);
        assert!(distance > 500_000.0); // > 500 km
        assert!(distance < 600_000.0); // < 600 km
    }

    #[test]
    fn test_geo_filter_bounding_box() {
        let filter = GeoFilter {
            bounding_box: Some((33.0, -119.0, 35.0, -117.0)),
            radius_filter: None,
        };

        // Inside box
        assert!(filter.passes(34.0, -118.0));
        // Outside box (north)
        assert!(!filter.passes(36.0, -118.0));
        // Outside box (east)
        assert!(!filter.passes(34.0, -116.0));
    }

    #[test]
    fn test_geo_filter_radius() {
        // 10km radius around LA
        let filter = GeoFilter {
            bounding_box: None,
            radius_filter: Some((34.0522, -118.2437, 10_000.0)),
        };

        // LA downtown (inside)
        assert!(filter.passes(34.0522, -118.2437));
        // Santa Monica (about 15km - outside)
        assert!(!filter.passes(34.0195, -118.4912));
    }

    #[test]
    fn test_bridge_filter_echelon() {
        let config = BridgeConfig::new("test").with_aggregation(AggregationPolicy::SquadLeaderOnly);
        let filter = BridgeFilter::from_config(&config);

        use peat_protocol::cot::{Position, TrackUpdate};

        // Platform-level message should be filtered
        let track = TrackUpdate {
            track_id: "t1".to_string(),
            source_platform: "platform-1".to_string(),
            source_model: "test-model".to_string(),
            model_version: "1.0".to_string(),
            cell_id: None, // No cell = platform level
            formation_id: None,
            timestamp: chrono::Utc::now(),
            position: Position::new(34.0, -118.0),
            velocity: None,
            classification: "a-f-G-U-C".to_string(),
            confidence: 0.9,
            attributes: Default::default(),
        };

        let msg = PeatMessage::Track(track);
        assert!(matches!(
            filter.should_publish(&msg),
            FilterDecision::Drop(_)
        ));
    }
}
