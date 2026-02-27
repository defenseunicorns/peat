//! Geographic primitives for PEAT protocol
//!
//! Provides fundamental geographic types and operations for defining
//! operational areas and spatial relationships between platforms.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Geographic coordinate (WGS84)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoCoordinate {
    /// Latitude in decimal degrees (-90 to 90)
    pub lat: f64,
    /// Longitude in decimal degrees (-180 to 180)
    pub lon: f64,
    /// Altitude in meters above sea level
    pub alt: f64,
}

impl GeoCoordinate {
    /// Create a new geographic coordinate
    pub fn new(lat: f64, lon: f64, alt: f64) -> Result<Self, &'static str> {
        if !(-90.0..=90.0).contains(&lat) {
            return Err("Latitude must be between -90 and 90 degrees");
        }
        if !(-180.0..=180.0).contains(&lon) {
            return Err("Longitude must be between -180 and 180 degrees");
        }
        Ok(Self { lat, lon, alt })
    }

    /// Calculate distance to another coordinate using Haversine formula (meters)
    pub fn distance_to(&self, other: &GeoCoordinate) -> f64 {
        const EARTH_RADIUS: f64 = 6371000.0; // meters

        let lat1 = self.lat.to_radians();
        let lat2 = other.lat.to_radians();
        let delta_lat = (other.lat - self.lat).to_radians();
        let delta_lon = (other.lon - self.lon).to_radians();

        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

        EARTH_RADIUS * c
    }

    /// Calculate 3D distance including altitude difference
    pub fn distance_3d(&self, other: &GeoCoordinate) -> f64 {
        let horizontal = self.distance_to(other);
        let vertical = (other.alt - self.alt).abs();
        (horizontal.powi(2) + vertical.powi(2)).sqrt()
    }

    /// Calculate bearing to another coordinate (degrees, 0-360)
    pub fn bearing_to(&self, other: &GeoCoordinate) -> f64 {
        let lat1 = self.lat.to_radians();
        let lat2 = other.lat.to_radians();
        let delta_lon = (other.lon - self.lon).to_radians();

        let y = delta_lon.sin() * lat2.cos();
        let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * delta_lon.cos();
        let bearing = y.atan2(x).to_degrees();

        (bearing + 360.0) % 360.0
    }
}

impl From<(f64, f64, f64)> for GeoCoordinate {
    fn from(tuple: (f64, f64, f64)) -> Self {
        Self {
            lat: tuple.0,
            lon: tuple.1,
            alt: tuple.2,
        }
    }
}

impl From<GeoCoordinate> for (f64, f64, f64) {
    fn from(coord: GeoCoordinate) -> Self {
        (coord.lat, coord.lon, coord.alt)
    }
}

impl fmt::Display for GeoCoordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.6}°{}, {:.6}°{}, {:.1}m",
            self.lat.abs(),
            if self.lat >= 0.0 { "N" } else { "S" },
            self.lon.abs(),
            if self.lon >= 0.0 { "E" } else { "W" },
            self.alt
        )
    }
}

/// Operational box defining geographic bounds for CAP operations
///
/// The operational box is a fundamental primitive provided by C2 that defines
/// the geographic area where the autonomous fleet will operate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationalBox {
    /// Unique identifier for this operational box
    pub id: String,

    /// Southwest corner (minimum lat/lon)
    pub southwest: GeoCoordinate,

    /// Northeast corner (maximum lat/lon)
    pub northeast: GeoCoordinate,

    /// Minimum altitude (meters)
    pub min_altitude: f64,

    /// Maximum altitude (meters)
    pub max_altitude: f64,

    /// Optional name/description
    pub name: Option<String>,
}

impl OperationalBox {
    /// Create a new operational box from corner coordinates
    pub fn new(
        id: String,
        southwest: GeoCoordinate,
        northeast: GeoCoordinate,
        min_altitude: f64,
        max_altitude: f64,
    ) -> Result<Self, &'static str> {
        // Validate bounds
        if southwest.lat >= northeast.lat {
            return Err("Southwest latitude must be less than northeast latitude");
        }
        if southwest.lon >= northeast.lon {
            return Err("Southwest longitude must be less than northeast longitude");
        }
        if min_altitude >= max_altitude {
            return Err("Minimum altitude must be less than maximum altitude");
        }

        Ok(Self {
            id,
            southwest,
            northeast,
            min_altitude,
            max_altitude,
            name: None,
        })
    }

    /// Create from center point and dimensions
    pub fn from_center(
        id: String,
        center: GeoCoordinate,
        width_meters: f64,
        height_meters: f64,
        altitude_range: (f64, f64),
    ) -> Result<Self, &'static str> {
        // Approximate degrees per meter at this latitude
        let meters_per_degree_lat = 111320.0;
        let meters_per_degree_lon = 111320.0 * center.lat.to_radians().cos();

        let half_width_deg = (width_meters / 2.0) / meters_per_degree_lon;
        let half_height_deg = (height_meters / 2.0) / meters_per_degree_lat;

        let southwest = GeoCoordinate::new(
            center.lat - half_height_deg,
            center.lon - half_width_deg,
            altitude_range.0,
        )?;

        let northeast = GeoCoordinate::new(
            center.lat + half_height_deg,
            center.lon + half_width_deg,
            altitude_range.1,
        )?;

        Self::new(id, southwest, northeast, altitude_range.0, altitude_range.1)
    }

    /// Check if a coordinate is within the operational box
    pub fn contains(&self, coord: &GeoCoordinate) -> bool {
        coord.lat >= self.southwest.lat
            && coord.lat <= self.northeast.lat
            && coord.lon >= self.southwest.lon
            && coord.lon <= self.northeast.lon
            && coord.alt >= self.min_altitude
            && coord.alt <= self.max_altitude
    }

    /// Get the center point of the box
    pub fn center(&self) -> GeoCoordinate {
        GeoCoordinate {
            lat: (self.southwest.lat + self.northeast.lat) / 2.0,
            lon: (self.southwest.lon + self.northeast.lon) / 2.0,
            alt: (self.min_altitude + self.max_altitude) / 2.0,
        }
    }

    /// Get the width of the box (meters)
    pub fn width(&self) -> f64 {
        let sw_ne = GeoCoordinate::new(self.southwest.lat, self.northeast.lon, 0.0).unwrap();
        self.southwest.distance_to(&sw_ne)
    }

    /// Get the height of the box (meters)
    pub fn height(&self) -> f64 {
        let sw_ne = GeoCoordinate::new(self.northeast.lat, self.southwest.lon, 0.0).unwrap();
        self.southwest.distance_to(&sw_ne)
    }

    /// Get the area of the box (square meters)
    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }

    /// Get the volume of the box (cubic meters)
    pub fn volume(&self) -> f64 {
        self.area() * (self.max_altitude - self.min_altitude)
    }

    /// Divide the box into a grid of sub-boxes
    pub fn subdivide(&self, rows: usize, cols: usize) -> Vec<OperationalBox> {
        let lat_step = (self.northeast.lat - self.southwest.lat) / rows as f64;
        let lon_step = (self.northeast.lon - self.southwest.lon) / cols as f64;

        let mut boxes = Vec::new();

        for row in 0..rows {
            for col in 0..cols {
                let sw_lat = self.southwest.lat + (row as f64 * lat_step);
                let sw_lon = self.southwest.lon + (col as f64 * lon_step);
                let ne_lat = sw_lat + lat_step;
                let ne_lon = sw_lon + lon_step;

                let sw = GeoCoordinate::new(sw_lat, sw_lon, self.min_altitude).unwrap();
                let ne = GeoCoordinate::new(ne_lat, ne_lon, self.max_altitude).unwrap();

                let sub_box = OperationalBox::new(
                    format!("{}_{}_{}", self.id, row, col),
                    sw,
                    ne,
                    self.min_altitude,
                    self.max_altitude,
                )
                .unwrap();

                boxes.push(sub_box);
            }
        }

        boxes
    }
}

impl fmt::Display for OperationalBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OperationalBox[{}]: {} to {}, alt {:.0}-{:.0}m ({:.1}km²)",
            self.id,
            self.southwest,
            self.northeast,
            self.min_altitude,
            self.max_altitude,
            self.area() / 1_000_000.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geocoordinate_creation() {
        let coord = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();
        assert_eq!(coord.lat, 37.7749);
        assert_eq!(coord.lon, -122.4194);
        assert_eq!(coord.alt, 100.0);

        // Invalid latitude
        assert!(GeoCoordinate::new(91.0, 0.0, 0.0).is_err());
        assert!(GeoCoordinate::new(-91.0, 0.0, 0.0).is_err());

        // Invalid longitude
        assert!(GeoCoordinate::new(0.0, 181.0, 0.0).is_err());
        assert!(GeoCoordinate::new(0.0, -181.0, 0.0).is_err());
    }

    #[test]
    fn test_distance_calculation() {
        // San Francisco to Los Angeles (approximately 559 km)
        let sf = GeoCoordinate::new(37.7749, -122.4194, 0.0).unwrap();
        let la = GeoCoordinate::new(34.0522, -118.2437, 0.0).unwrap();

        let distance = sf.distance_to(&la);
        assert!((distance - 559_000.0).abs() < 5000.0); // Within 5km tolerance
    }

    #[test]
    fn test_bearing_calculation() {
        let coord1 = GeoCoordinate::new(0.0, 0.0, 0.0).unwrap();
        let coord2 = GeoCoordinate::new(1.0, 0.0, 0.0).unwrap(); // North
        let coord3 = GeoCoordinate::new(0.0, 1.0, 0.0).unwrap(); // East

        let bearing_north = coord1.bearing_to(&coord2);
        let bearing_east = coord1.bearing_to(&coord3);

        assert!((bearing_north - 0.0).abs() < 1.0); // North is ~0 degrees
        assert!((bearing_east - 90.0).abs() < 1.0); // East is ~90 degrees
    }

    #[test]
    fn test_operational_box_creation() {
        let sw = GeoCoordinate::new(37.0, -122.0, 0.0).unwrap();
        let ne = GeoCoordinate::new(38.0, -121.0, 0.0).unwrap();

        let op_box = OperationalBox::new("test_box".to_string(), sw, ne, 0.0, 1000.0).unwrap();

        assert_eq!(op_box.id, "test_box");
        assert_eq!(op_box.southwest.lat, 37.0);
        assert_eq!(op_box.northeast.lat, 38.0);
    }

    #[test]
    fn test_operational_box_contains() {
        let sw = GeoCoordinate::new(37.0, -122.0, 0.0).unwrap();
        let ne = GeoCoordinate::new(38.0, -121.0, 0.0).unwrap();
        let op_box = OperationalBox::new("test".to_string(), sw, ne, 0.0, 1000.0).unwrap();

        let inside = GeoCoordinate::new(37.5, -121.5, 500.0).unwrap();
        let outside = GeoCoordinate::new(36.5, -121.5, 500.0).unwrap();

        assert!(op_box.contains(&inside));
        assert!(!op_box.contains(&outside));
    }

    #[test]
    fn test_operational_box_center() {
        let sw = GeoCoordinate::new(37.0, -122.0, 0.0).unwrap();
        let ne = GeoCoordinate::new(38.0, -121.0, 0.0).unwrap();
        let op_box = OperationalBox::new("test".to_string(), sw, ne, 0.0, 1000.0).unwrap();

        let center = op_box.center();
        assert_eq!(center.lat, 37.5);
        assert_eq!(center.lon, -121.5);
        assert_eq!(center.alt, 500.0);
    }

    #[test]
    fn test_operational_box_from_center() {
        let center = GeoCoordinate::new(37.5, -121.5, 500.0).unwrap();
        let op_box = OperationalBox::from_center(
            "test".to_string(),
            center,
            10000.0, // 10km wide
            20000.0, // 20km tall
            (0.0, 1000.0),
        )
        .unwrap();

        let box_center = op_box.center();
        assert!((box_center.lat - center.lat).abs() < 0.01);
        assert!((box_center.lon - center.lon).abs() < 0.01);
    }

    #[test]
    fn test_operational_box_subdivide() {
        let sw = GeoCoordinate::new(37.0, -122.0, 0.0).unwrap();
        let ne = GeoCoordinate::new(38.0, -121.0, 0.0).unwrap();
        let op_box = OperationalBox::new("test".to_string(), sw, ne, 0.0, 1000.0).unwrap();

        let sub_boxes = op_box.subdivide(2, 2);
        assert_eq!(sub_boxes.len(), 4);

        // Verify all sub-boxes are within original box
        for sub_box in &sub_boxes {
            assert!(op_box.contains(&sub_box.center()));
        }
    }
}
