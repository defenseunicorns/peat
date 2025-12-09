//! Supporting types for data synchronization abstraction
//!
//! This module defines common types used across all sync backend implementations,
//! providing a unified interface regardless of underlying CRDT engine (Ditto, Automerge, etc).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

/// Unique identifier for a document
pub type DocumentId = String;

/// Unique identifier for a peer
pub type PeerId = String;

/// Timestamp for ordering and versioning
pub type Timestamp = SystemTime;

/// Generic value type for document fields
pub use serde_json::Value;

/// Unified document representation across backends
///
/// This provides a backend-agnostic view of documents, abstracting away
/// differences between Ditto's CBOR documents and Automerge's columnar storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Optional document ID (None for new documents)
    pub id: Option<DocumentId>,

    /// Document fields as key-value pairs
    pub fields: HashMap<String, Value>,

    /// Last update timestamp
    pub updated_at: Timestamp,
}

impl Document {
    /// Create a new document with given fields
    pub fn new(fields: HashMap<String, Value>) -> Self {
        Self {
            id: None,
            fields,
            updated_at: SystemTime::now(),
        }
    }

    /// Create a document with a specific ID
    pub fn with_id(id: impl Into<String>, fields: HashMap<String, Value>) -> Self {
        Self {
            id: Some(id.into()),
            fields,
            updated_at: SystemTime::now(),
        }
    }

    /// Get a field value by name
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.fields.get(field)
    }

    /// Set a field value
    pub fn set(&mut self, field: impl Into<String>, value: Value) {
        self.fields.insert(field.into(), value);
        self.updated_at = SystemTime::now();
    }
}

/// Geographic point for spatial queries (Issue #356)
///
/// Represents a WGS84 coordinate for spatial filtering.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    /// Latitude in degrees (-90 to 90)
    pub lat: f64,
    /// Longitude in degrees (-180 to 180)
    pub lon: f64,
}

impl GeoPoint {
    /// Create a new GeoPoint
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// Calculate haversine distance to another point in meters
    ///
    /// Uses the haversine formula for great-circle distance on a sphere.
    pub fn distance_to(&self, other: &GeoPoint) -> f64 {
        haversine_distance(self.lat, self.lon, other.lat, other.lon)
    }

    /// Check if this point is within a bounding box
    pub fn within_bounds(&self, min: &GeoPoint, max: &GeoPoint) -> bool {
        self.lat >= min.lat && self.lat <= max.lat && self.lon >= min.lon && self.lon <= max.lon
    }

    /// Check if this point is within a radius of another point
    pub fn within_radius(&self, center: &GeoPoint, radius_meters: f64) -> bool {
        self.distance_to(center) <= radius_meters
    }
}

/// Haversine distance calculation between two coordinates
///
/// Returns distance in meters using WGS84 Earth radius.
pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_METERS: f64 = 6_371_000.0; // WGS84 mean radius

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);

    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_METERS * c
}

/// Query abstraction that works across backends
///
/// Provides a simple query language that can be translated to backend-specific
/// query formats (Ditto DQL, Automerge queries, etc).
///
/// # Spatial Queries (Issue #356)
///
/// Spatial queries filter documents by geographic location:
/// - `WithinRadius`: Documents within a specified distance of a center point
/// - `WithinBounds`: Documents within a rectangular bounding box
///
/// Documents must have `lat` and `lon` fields (or configurable field names) for
/// spatial queries to match.
#[derive(Debug, Clone)]
pub enum Query {
    /// Simple equality match: field == value
    Eq { field: String, value: Value },

    /// Less than: field < value
    Lt { field: String, value: Value },

    /// Greater than: field > value
    Gt { field: String, value: Value },

    /// Multiple conditions combined with AND
    And(Vec<Query>),

    /// Multiple conditions combined with OR
    Or(Vec<Query>),

    /// All documents in collection (no filter)
    All,

    /// Custom backend-specific query string
    /// Use sparingly - limits backend portability
    Custom(String),

    // === Spatial queries (Issue #356) ===
    /// Documents within a radius of a center point
    ///
    /// Requires documents to have `lat` and `lon` fields (or fields specified
    /// by `lat_field` and `lon_field`).
    WithinRadius {
        /// Center point for the radius search
        center: GeoPoint,
        /// Radius in meters
        radius_meters: f64,
        /// Field name for latitude (default: "lat")
        lat_field: Option<String>,
        /// Field name for longitude (default: "lon")
        lon_field: Option<String>,
    },

    /// Documents within a rectangular bounding box
    ///
    /// Requires documents to have `lat` and `lon` fields (or fields specified
    /// by `lat_field` and `lon_field`).
    WithinBounds {
        /// Southwest corner (minimum lat/lon)
        min: GeoPoint,
        /// Northeast corner (maximum lat/lon)
        max: GeoPoint,
        /// Field name for latitude (default: "lat")
        lat_field: Option<String>,
        /// Field name for longitude (default: "lon")
        lon_field: Option<String>,
    },
}

/// Stream of document changes for live queries
///
/// Returned by `DocumentStore::observe()` to receive real-time updates.
pub struct ChangeStream {
    /// Channel receiver for change events
    pub receiver: tokio::sync::mpsc::UnboundedReceiver<ChangeEvent>,
}

/// Event representing a document change
#[derive(Debug, Clone)]
pub enum ChangeEvent {
    /// Document was inserted or updated
    Updated {
        collection: String,
        document: Document,
    },

    /// Document was removed
    Removed {
        collection: String,
        doc_id: DocumentId,
    },

    /// Initial snapshot of all matching documents
    Initial { documents: Vec<Document> },
}

/// Information about a discovered peer
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Unique peer identifier
    pub peer_id: PeerId,

    /// Network address (if known)
    pub address: Option<String>,

    /// Transport type used for connection
    pub transport: TransportType,

    /// Whether peer is currently connected
    pub connected: bool,

    /// Last time this peer was seen
    pub last_seen: Timestamp,

    /// Additional peer metadata
    pub metadata: HashMap<String, String>,
}

/// Transport types for peer connections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportType {
    /// TCP/IP connection
    Tcp,

    /// Bluetooth connection
    Bluetooth,

    /// mDNS local network discovery
    #[serde(rename = "mdns")]
    Mdns,

    /// WebSocket connection
    WebSocket,

    /// Custom transport
    Custom,
}

/// Events related to peer lifecycle
#[derive(Debug, Clone)]
pub enum PeerEvent {
    /// New peer discovered
    Discovered(PeerInfo),

    /// Peer connected
    Connected(PeerInfo),

    /// Peer disconnected
    Disconnected {
        peer_id: PeerId,
        reason: Option<String>,
    },

    /// Peer lost (no longer discoverable)
    Lost(PeerId),
}

/// Configuration for a sync backend
#[derive(Debug, Clone)]
pub struct BackendConfig {
    /// Application ID (used for peer discovery and sync groups)
    pub app_id: String,

    /// Directory for persistent storage
    pub persistence_dir: PathBuf,

    /// Optional shared secret for authentication
    pub shared_key: Option<String>,

    /// Transport configuration
    pub transport: TransportConfig,

    /// Additional backend-specific configuration
    pub extra: HashMap<String, String>,
}

/// Transport-specific configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// TCP listening port (None = auto-assign)
    pub tcp_listen_port: Option<u16>,

    /// TCP address to connect to (for client mode)
    pub tcp_connect_address: Option<String>,

    /// Enable mDNS local discovery
    pub enable_mdns: bool,

    /// Enable Bluetooth discovery
    pub enable_bluetooth: bool,

    /// Enable WebSocket transport
    pub enable_websocket: bool,

    /// Custom transport configuration
    pub custom: HashMap<String, String>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            tcp_listen_port: None,
            tcp_connect_address: None,
            enable_mdns: true,
            enable_bluetooth: false,
            enable_websocket: false,
            custom: HashMap::new(),
        }
    }
}

/// Subscription handle for sync operations
///
/// Keeps sync active for a collection while alive.
/// Drop to unsubscribe.
pub struct SyncSubscription {
    collection: String,
    _handle: Box<dyn std::any::Any + Send + Sync>,
}

impl SyncSubscription {
    /// Create a new subscription
    pub fn new(collection: impl Into<String>, handle: impl std::any::Any + Send + Sync) -> Self {
        eprintln!("SyncSubscription::new() - Creating subscription wrapper");
        Self {
            collection: collection.into(),
            _handle: Box::new(handle),
        }
    }

    /// Get the collection this subscription is for
    pub fn collection(&self) -> &str {
        &self.collection
    }
}

impl Drop for SyncSubscription {
    fn drop(&mut self) {
        eprintln!(
            "SyncSubscription::drop() - Subscription for '{}' is being dropped!",
            self.collection
        );
    }
}

impl std::fmt::Debug for SyncSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncSubscription")
            .field("collection", &self.collection)
            .finish_non_exhaustive()
    }
}

// === QoS-aware subscriptions (Issue #356) ===

/// Subscription configuration with QoS policy
///
/// Combines a collection, query filter, and QoS settings for fine-grained
/// control over what data syncs and how it syncs.
///
/// # Example
///
/// ```
/// use hive_protocol::sync::types::{Subscription, Query, GeoPoint, SubscriptionQoS};
/// use hive_protocol::qos::SyncMode;
///
/// // Subscribe to nearby beacons with LatestOnly sync
/// let subscription = Subscription {
///     collection: "beacons".to_string(),
///     query: Query::WithinRadius {
///         center: GeoPoint::new(37.7749, -122.4194),
///         radius_meters: 5000.0,
///         lat_field: None,
///         lon_field: None,
///     },
///     qos: SubscriptionQoS {
///         sync_mode: SyncMode::LatestOnly,
///         max_documents: Some(100),
///         update_rate_ms: Some(1000),
///     },
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Subscription {
    /// Collection to subscribe to
    pub collection: String,
    /// Query filter for documents
    pub query: Query,
    /// QoS settings for this subscription
    pub qos: SubscriptionQoS,
}

impl Subscription {
    /// Create a subscription for all documents in a collection
    pub fn all(collection: impl Into<String>) -> Self {
        Self {
            collection: collection.into(),
            query: Query::All,
            qos: SubscriptionQoS::default(),
        }
    }

    /// Create a subscription with a query
    pub fn with_query(collection: impl Into<String>, query: Query) -> Self {
        Self {
            collection: collection.into(),
            query,
            qos: SubscriptionQoS::default(),
        }
    }

    /// Create a subscription with query and QoS
    pub fn with_qos(collection: impl Into<String>, query: Query, qos: SubscriptionQoS) -> Self {
        Self {
            collection: collection.into(),
            query,
            qos,
        }
    }

    /// Create a spatial radius subscription
    pub fn within_radius(
        collection: impl Into<String>,
        center: GeoPoint,
        radius_meters: f64,
    ) -> Self {
        Self {
            collection: collection.into(),
            query: Query::WithinRadius {
                center,
                radius_meters,
                lat_field: None,
                lon_field: None,
            },
            qos: SubscriptionQoS::default(),
        }
    }

    /// Create a spatial bounds subscription
    pub fn within_bounds(collection: impl Into<String>, min: GeoPoint, max: GeoPoint) -> Self {
        Self {
            collection: collection.into(),
            query: Query::WithinBounds {
                min,
                max,
                lat_field: None,
                lon_field: None,
            },
            qos: SubscriptionQoS::default(),
        }
    }

    /// Set sync mode for this subscription
    pub fn with_sync_mode(mut self, sync_mode: crate::qos::SyncMode) -> Self {
        self.qos.sync_mode = sync_mode;
        self
    }
}

/// QoS settings for a subscription (Issue #356)
///
/// Controls sync behavior including sync mode, rate limiting, and document limits.
#[derive(Debug, Clone, Default)]
pub struct SubscriptionQoS {
    /// Sync mode (FullHistory, LatestOnly, WindowedHistory)
    pub sync_mode: crate::qos::SyncMode,
    /// Maximum number of documents to sync (None = unlimited)
    pub max_documents: Option<usize>,
    /// Minimum time between updates in ms (rate limiting)
    pub update_rate_ms: Option<u64>,
}

impl SubscriptionQoS {
    /// Create QoS with LatestOnly mode (no history)
    pub fn latest_only() -> Self {
        Self {
            sync_mode: crate::qos::SyncMode::LatestOnly,
            ..Default::default()
        }
    }

    /// Create QoS with FullHistory mode (all deltas)
    pub fn full_history() -> Self {
        Self {
            sync_mode: crate::qos::SyncMode::FullHistory,
            ..Default::default()
        }
    }

    /// Create QoS with WindowedHistory mode
    pub fn windowed(window_seconds: u64) -> Self {
        Self {
            sync_mode: crate::qos::SyncMode::WindowedHistory { window_seconds },
            ..Default::default()
        }
    }

    /// Set max documents
    pub fn with_max_documents(mut self, max: usize) -> Self {
        self.max_documents = Some(max);
        self
    }

    /// Set update rate limit
    pub fn with_rate_limit(mut self, rate_ms: u64) -> Self {
        self.update_rate_ms = Some(rate_ms);
        self
    }
}

/// Priority level for sync operations
///
/// Used by backends that support priority-based synchronization
/// (e.g., prioritize critical updates over metadata changes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Priority {
    /// Critical updates (e.g., capability loss, safety-critical)
    Critical = 0,

    /// High priority (e.g., cell membership changes)
    High = 1,

    /// Medium priority (e.g., leader election)
    #[default]
    Medium = 2,

    /// Low priority (e.g., capability additions, metadata)
    Low = 3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_creation() {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), Value::String("test".to_string()));

        let doc = Document::new(fields.clone());
        assert!(doc.id.is_none());
        assert_eq!(doc.get("name"), Some(&Value::String("test".to_string())));

        let doc_with_id = Document::with_id("doc1", fields);
        assert_eq!(doc_with_id.id, Some("doc1".to_string()));
    }

    #[test]
    fn test_document_field_access() {
        let mut doc = Document::new(HashMap::new());
        doc.set("key", Value::String("value".to_string()));

        assert_eq!(doc.get("key"), Some(&Value::String("value".to_string())));
        assert_eq!(doc.get("missing"), None);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Medium);
        assert!(Priority::Medium < Priority::Low);
    }

    // === Spatial query tests (Issue #356) ===

    #[test]
    fn test_geopoint_creation() {
        let point = GeoPoint::new(37.7749, -122.4194); // San Francisco
        assert_eq!(point.lat, 37.7749);
        assert_eq!(point.lon, -122.4194);
    }

    #[test]
    fn test_haversine_distance_same_point() {
        let sf = GeoPoint::new(37.7749, -122.4194);
        let distance = sf.distance_to(&sf);
        assert!(
            distance < 1.0,
            "Distance to self should be ~0, got {}",
            distance
        );
    }

    #[test]
    fn test_haversine_distance_known_values() {
        // San Francisco to Los Angeles: approximately 559 km
        let sf = GeoPoint::new(37.7749, -122.4194);
        let la = GeoPoint::new(34.0522, -118.2437);
        let distance = sf.distance_to(&la);

        // Allow 1% tolerance
        let expected = 559_000.0;
        let tolerance = expected * 0.01;
        assert!(
            (distance - expected).abs() < tolerance,
            "SF to LA should be ~559km, got {}m",
            distance
        );
    }

    #[test]
    fn test_haversine_distance_across_equator() {
        // Quito, Ecuador (near equator) to Buenos Aires, Argentina
        let quito = GeoPoint::new(-0.1807, -78.4678);
        let buenos_aires = GeoPoint::new(-34.6037, -58.3816);
        let distance = quito.distance_to(&buenos_aires);

        // Approximately 4,360 km
        assert!(
            distance > 4_300_000.0 && distance < 4_500_000.0,
            "Quito to Buenos Aires should be ~4,360km, got {}m",
            distance
        );
    }

    #[test]
    fn test_geopoint_within_bounds() {
        let point = GeoPoint::new(37.7749, -122.4194); // San Francisco
        let min = GeoPoint::new(37.0, -123.0);
        let max = GeoPoint::new(38.0, -122.0);

        assert!(point.within_bounds(&min, &max));

        // Outside bounds
        let outside = GeoPoint::new(40.0, -122.0);
        assert!(!outside.within_bounds(&min, &max));
    }

    #[test]
    fn test_geopoint_within_radius() {
        let center = GeoPoint::new(37.7749, -122.4194); // San Francisco

        // Point 1km away (approximately)
        let nearby = GeoPoint::new(37.7839, -122.4194); // ~1km north
        assert!(nearby.within_radius(&center, 2000.0)); // Within 2km
        assert!(!nearby.within_radius(&center, 500.0)); // Not within 500m

        // Point far away
        let la = GeoPoint::new(34.0522, -118.2437);
        assert!(!la.within_radius(&center, 100_000.0)); // Not within 100km
        assert!(la.within_radius(&center, 600_000.0)); // Within 600km
    }

    #[test]
    fn test_spatial_query_within_radius() {
        let query = Query::WithinRadius {
            center: GeoPoint::new(37.7749, -122.4194),
            radius_meters: 5000.0,
            lat_field: None,
            lon_field: None,
        };

        match query {
            Query::WithinRadius {
                center,
                radius_meters,
                ..
            } => {
                assert_eq!(center.lat, 37.7749);
                assert_eq!(radius_meters, 5000.0);
            }
            _ => panic!("Expected WithinRadius query"),
        }
    }

    #[test]
    fn test_spatial_query_within_bounds() {
        let query = Query::WithinBounds {
            min: GeoPoint::new(37.0, -123.0),
            max: GeoPoint::new(38.0, -122.0),
            lat_field: Some("latitude".to_string()),
            lon_field: Some("longitude".to_string()),
        };

        match query {
            Query::WithinBounds {
                min,
                max,
                lat_field,
                lon_field,
            } => {
                assert_eq!(min.lat, 37.0);
                assert_eq!(max.lon, -122.0);
                assert_eq!(lat_field, Some("latitude".to_string()));
                assert_eq!(lon_field, Some("longitude".to_string()));
            }
            _ => panic!("Expected WithinBounds query"),
        }
    }

    #[test]
    fn test_geopoint_serialization() {
        let point = GeoPoint::new(37.7749, -122.4194);
        let json = serde_json::to_string(&point).unwrap();
        let deserialized: GeoPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(point, deserialized);
    }

    // === Subscription tests (Issue #356) ===

    #[test]
    fn test_subscription_all() {
        let sub = Subscription::all("beacons");
        assert_eq!(sub.collection, "beacons");
        assert!(matches!(sub.query, Query::All));
    }

    #[test]
    fn test_subscription_with_query() {
        let query = Query::Eq {
            field: "type".to_string(),
            value: Value::String("soldier".to_string()),
        };
        let sub = Subscription::with_query("platforms", query);
        assert_eq!(sub.collection, "platforms");
    }

    #[test]
    fn test_subscription_within_radius() {
        let center = GeoPoint::new(37.7749, -122.4194);
        let sub = Subscription::within_radius("beacons", center, 5000.0);

        assert_eq!(sub.collection, "beacons");
        match sub.query {
            Query::WithinRadius {
                center: c,
                radius_meters,
                ..
            } => {
                assert_eq!(c.lat, 37.7749);
                assert_eq!(radius_meters, 5000.0);
            }
            _ => panic!("Expected WithinRadius query"),
        }
    }

    #[test]
    fn test_subscription_within_bounds() {
        let min = GeoPoint::new(37.0, -123.0);
        let max = GeoPoint::new(38.0, -122.0);
        let sub = Subscription::within_bounds("beacons", min, max);

        assert_eq!(sub.collection, "beacons");
        match sub.query {
            Query::WithinBounds {
                min: m, max: mx, ..
            } => {
                assert_eq!(m.lat, 37.0);
                assert_eq!(mx.lon, -122.0);
            }
            _ => panic!("Expected WithinBounds query"),
        }
    }

    #[test]
    fn test_subscription_with_sync_mode() {
        let sub = Subscription::all("beacons").with_sync_mode(crate::qos::SyncMode::LatestOnly);
        assert!(sub.qos.sync_mode.is_latest_only());
    }

    #[test]
    fn test_subscription_qos_defaults() {
        let qos = SubscriptionQoS::default();
        assert!(qos.sync_mode.is_full_history());
        assert!(qos.max_documents.is_none());
        assert!(qos.update_rate_ms.is_none());
    }

    #[test]
    fn test_subscription_qos_latest_only() {
        let qos = SubscriptionQoS::latest_only();
        assert!(qos.sync_mode.is_latest_only());
    }

    #[test]
    fn test_subscription_qos_windowed() {
        let qos = SubscriptionQoS::windowed(300);
        assert!(qos.sync_mode.is_windowed());
        assert_eq!(qos.sync_mode.window_seconds(), Some(300));
    }

    #[test]
    fn test_subscription_qos_with_limits() {
        let qos = SubscriptionQoS::latest_only()
            .with_max_documents(100)
            .with_rate_limit(1000);
        assert_eq!(qos.max_documents, Some(100));
        assert_eq!(qos.update_rate_ms, Some(1000));
    }
}
