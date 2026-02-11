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

    /// Negation of a query (Issue #357)
    ///
    /// Matches documents that do NOT match the inner query.
    Not(Box<Query>),

    /// All documents in collection (no filter)
    All,

    /// Custom backend-specific query string
    /// Use sparingly - limits backend portability
    Custom(String),

    // === Deletion-aware queries (ADR-034, Issue #369) ===
    /// Include soft-deleted documents in query results
    ///
    /// By default, queries exclude documents with `_deleted=true` (soft-deleted).
    /// This modifier includes those documents in the results.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Default: excludes deleted documents
    /// let query = Query::All;
    ///
    /// // Include deleted documents
    /// let query_with_deleted = Query::IncludeDeleted(Box::new(Query::All));
    ///
    /// // With a filter
    /// let filtered_with_deleted = Query::IncludeDeleted(Box::new(Query::Eq {
    ///     field: "type".to_string(),
    ///     value: Value::String("contact_report".to_string()),
    /// }));
    /// ```
    IncludeDeleted(Box<Query>),

    /// Only return soft-deleted documents
    ///
    /// Matches only documents where `_deleted=true`.
    /// Useful for auditing or restoring deleted records.
    DeletedOnly,

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

impl Query {
    /// Check if this query includes deleted documents
    ///
    /// Returns true if the query is `IncludeDeleted` or `DeletedOnly`.
    pub fn includes_deleted(&self) -> bool {
        matches!(self, Query::IncludeDeleted(_) | Query::DeletedOnly)
    }

    /// Check if this query only matches deleted documents
    pub fn is_deleted_only(&self) -> bool {
        matches!(self, Query::DeletedOnly)
    }

    /// Wrap this query to include deleted documents
    ///
    /// If already `IncludeDeleted` or `DeletedOnly`, returns self unchanged.
    pub fn with_deleted(self) -> Self {
        if self.includes_deleted() {
            self
        } else {
            Query::IncludeDeleted(Box::new(self))
        }
    }

    /// Get the inner query if this is an IncludeDeleted wrapper
    pub fn inner_query(&self) -> &Query {
        match self {
            Query::IncludeDeleted(inner) => inner.as_ref(),
            other => other,
        }
    }

    /// Check if a document matches the soft-delete filter
    ///
    /// - For normal queries: document must NOT have `_deleted=true`
    /// - For `IncludeDeleted`: document can have any `_deleted` value
    /// - For `DeletedOnly`: document MUST have `_deleted=true`
    pub fn matches_deletion_state(&self, doc: &Document) -> bool {
        let is_deleted = doc
            .fields
            .get("_deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        match self {
            Query::DeletedOnly => is_deleted,
            Query::IncludeDeleted(_) => true, // Include all
            _ => !is_deleted,                 // Exclude deleted
        }
    }
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
/// use hive_mesh::sync::types::{Subscription, Query, GeoPoint, SubscriptionQoS};
/// use hive_mesh::qos::SyncMode;
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

    // === Dynamic subscription updates (Issue #357) ===

    /// Update the query for this subscription
    ///
    /// Allows modifying the subscription filter without recreating it.
    /// Useful for dynamic spatial queries that follow user position.
    pub fn update_query(&mut self, query: Query) {
        self.query = query;
    }

    /// Update the QoS settings for this subscription
    ///
    /// Allows adjusting sync behavior based on runtime conditions
    /// (e.g., switching to LatestOnly when bandwidth is constrained).
    pub fn update_qos(&mut self, qos: SubscriptionQoS) {
        self.qos = qos;
    }

    /// Update just the sync mode
    pub fn update_sync_mode(&mut self, sync_mode: crate::qos::SyncMode) {
        self.qos.sync_mode = sync_mode;
    }

    /// Update the spatial center point (for radius queries)
    ///
    /// If the current query is a `WithinRadius`, updates the center point.
    /// Otherwise, this is a no-op.
    pub fn update_center(&mut self, new_center: GeoPoint) {
        if let Query::WithinRadius { center, .. } = &mut self.query {
            *center = new_center;
        }
    }

    /// Update the spatial radius (for radius queries)
    ///
    /// If the current query is a `WithinRadius`, updates the radius.
    /// Otherwise, this is a no-op.
    pub fn update_radius(&mut self, new_radius: f64) {
        if let Query::WithinRadius { radius_meters, .. } = &mut self.query {
            *radius_meters = new_radius;
        }
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

// === Sync Mode Metrics (Issue #357) ===

/// Metrics for sync mode performance tracking
///
/// Provides statistics on sync operations by mode, enabling analysis
/// of the ~300× reconnection improvement from LatestOnly mode.
///
/// # Example
///
/// ```
/// use hive_mesh::sync::types::SyncModeMetrics;
/// use hive_mesh::qos::SyncMode;
///
/// let mut metrics = SyncModeMetrics::new();
/// metrics.record_sync("beacons", SyncMode::LatestOnly, 1024, std::time::Duration::from_millis(5));
/// assert_eq!(metrics.total_syncs, 1);
/// assert_eq!(metrics.latest_only_syncs, 1);
/// ```
#[derive(Debug, Clone, Default)]
pub struct SyncModeMetrics {
    /// Total number of sync operations
    pub total_syncs: u64,
    /// Number of syncs using FullHistory mode
    pub full_history_syncs: u64,
    /// Number of syncs using LatestOnly mode
    pub latest_only_syncs: u64,
    /// Number of syncs using WindowedHistory mode
    pub windowed_syncs: u64,
    /// Total bytes synced with FullHistory mode
    pub full_history_bytes: u64,
    /// Total bytes synced with LatestOnly mode
    pub latest_only_bytes: u64,
    /// Total bytes synced with WindowedHistory mode
    pub windowed_bytes: u64,
    /// Total sync duration (in milliseconds) for FullHistory
    pub full_history_duration_ms: u64,
    /// Total sync duration (in milliseconds) for LatestOnly
    pub latest_only_duration_ms: u64,
    /// Total sync duration (in milliseconds) for WindowedHistory
    pub windowed_duration_ms: u64,
}

impl SyncModeMetrics {
    /// Create new empty metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a sync operation
    pub fn record_sync(
        &mut self,
        _collection: &str,
        mode: crate::qos::SyncMode,
        bytes: u64,
        duration: std::time::Duration,
    ) {
        self.total_syncs += 1;
        let duration_ms = duration.as_millis() as u64;

        match mode {
            crate::qos::SyncMode::FullHistory => {
                self.full_history_syncs += 1;
                self.full_history_bytes += bytes;
                self.full_history_duration_ms += duration_ms;
            }
            crate::qos::SyncMode::LatestOnly => {
                self.latest_only_syncs += 1;
                self.latest_only_bytes += bytes;
                self.latest_only_duration_ms += duration_ms;
            }
            crate::qos::SyncMode::WindowedHistory { .. } => {
                self.windowed_syncs += 1;
                self.windowed_bytes += bytes;
                self.windowed_duration_ms += duration_ms;
            }
        }
    }

    /// Average bytes per sync for FullHistory mode
    pub fn avg_full_history_bytes(&self) -> f64 {
        if self.full_history_syncs == 0 {
            0.0
        } else {
            self.full_history_bytes as f64 / self.full_history_syncs as f64
        }
    }

    /// Average bytes per sync for LatestOnly mode
    pub fn avg_latest_only_bytes(&self) -> f64 {
        if self.latest_only_syncs == 0 {
            0.0
        } else {
            self.latest_only_bytes as f64 / self.latest_only_syncs as f64
        }
    }

    /// Bandwidth savings ratio (LatestOnly vs FullHistory)
    ///
    /// Returns the ratio of FullHistory bytes to LatestOnly bytes.
    /// A ratio of 300.0 means LatestOnly uses 300× less bandwidth.
    pub fn bandwidth_savings_ratio(&self) -> Option<f64> {
        let fh_avg = self.avg_full_history_bytes();
        let lo_avg = self.avg_latest_only_bytes();

        if lo_avg == 0.0 || fh_avg == 0.0 {
            None
        } else {
            Some(fh_avg / lo_avg)
        }
    }

    /// Reset all metrics
    pub fn reset(&mut self) {
        *self = Self::default();
    }
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

    // === Compound query tests (Issue #357) ===

    #[test]
    fn test_query_not() {
        // Create a NOT query
        let inner = Query::Eq {
            field: "type".to_string(),
            value: Value::String("hidden".to_string()),
        };
        let not_query = Query::Not(Box::new(inner));

        match not_query {
            Query::Not(inner) => match inner.as_ref() {
                Query::Eq { field, value } => {
                    assert_eq!(field, "type");
                    assert_eq!(value, &Value::String("hidden".to_string()));
                }
                _ => panic!("Expected Eq query inside Not"),
            },
            _ => panic!("Expected Not query"),
        }
    }

    #[test]
    fn test_compound_query_not_and() {
        // NOT (type == "hidden" AND status == "deleted")
        let and_query = Query::And(vec![
            Query::Eq {
                field: "type".to_string(),
                value: Value::String("hidden".to_string()),
            },
            Query::Eq {
                field: "status".to_string(),
                value: Value::String("deleted".to_string()),
            },
        ]);
        let not_and = Query::Not(Box::new(and_query));

        match not_and {
            Query::Not(inner) => match inner.as_ref() {
                Query::And(queries) => {
                    assert_eq!(queries.len(), 2);
                }
                _ => panic!("Expected And query inside Not"),
            },
            _ => panic!("Expected Not query"),
        }
    }

    // === Dynamic subscription update tests (Issue #357) ===

    #[test]
    fn test_subscription_update_query() {
        let mut sub = Subscription::all("beacons");

        // Update to a spatial query
        sub.update_query(Query::WithinRadius {
            center: GeoPoint::new(37.7749, -122.4194),
            radius_meters: 5000.0,
            lat_field: None,
            lon_field: None,
        });

        match &sub.query {
            Query::WithinRadius { radius_meters, .. } => {
                assert_eq!(*radius_meters, 5000.0);
            }
            _ => panic!("Expected WithinRadius query"),
        }
    }

    #[test]
    fn test_subscription_update_qos() {
        let mut sub = Subscription::all("beacons");
        assert!(sub.qos.sync_mode.is_full_history());

        // Update QoS
        sub.update_qos(SubscriptionQoS::latest_only().with_max_documents(50));
        assert!(sub.qos.sync_mode.is_latest_only());
        assert_eq!(sub.qos.max_documents, Some(50));
    }

    #[test]
    fn test_subscription_update_sync_mode() {
        let mut sub = Subscription::all("beacons");
        sub.update_sync_mode(crate::qos::SyncMode::LatestOnly);
        assert!(sub.qos.sync_mode.is_latest_only());
    }

    #[test]
    fn test_subscription_update_center() {
        let mut sub =
            Subscription::within_radius("beacons", GeoPoint::new(37.7749, -122.4194), 5000.0);

        // Move center to new location
        sub.update_center(GeoPoint::new(34.0522, -118.2437)); // LA

        match &sub.query {
            Query::WithinRadius { center, .. } => {
                assert_eq!(center.lat, 34.0522);
                assert_eq!(center.lon, -118.2437);
            }
            _ => panic!("Expected WithinRadius query"),
        }
    }

    #[test]
    fn test_subscription_update_radius() {
        let mut sub =
            Subscription::within_radius("beacons", GeoPoint::new(37.7749, -122.4194), 5000.0);

        // Expand radius
        sub.update_radius(10000.0);

        match &sub.query {
            Query::WithinRadius { radius_meters, .. } => {
                assert_eq!(*radius_meters, 10000.0);
            }
            _ => panic!("Expected WithinRadius query"),
        }
    }

    #[test]
    fn test_subscription_update_center_noop_on_non_radius() {
        let mut sub = Subscription::all("beacons");

        // Should be a no-op since this isn't a radius query
        sub.update_center(GeoPoint::new(34.0522, -118.2437));

        assert!(matches!(sub.query, Query::All));
    }

    // === SyncModeMetrics tests (Issue #357) ===

    #[test]
    fn test_sync_mode_metrics_new() {
        let metrics = SyncModeMetrics::new();
        assert_eq!(metrics.total_syncs, 0);
        assert_eq!(metrics.full_history_syncs, 0);
        assert_eq!(metrics.latest_only_syncs, 0);
    }

    #[test]
    fn test_sync_mode_metrics_record_full_history() {
        let mut metrics = SyncModeMetrics::new();
        metrics.record_sync(
            "beacons",
            crate::qos::SyncMode::FullHistory,
            10000,
            std::time::Duration::from_millis(50),
        );

        assert_eq!(metrics.total_syncs, 1);
        assert_eq!(metrics.full_history_syncs, 1);
        assert_eq!(metrics.full_history_bytes, 10000);
        assert_eq!(metrics.full_history_duration_ms, 50);
    }

    #[test]
    fn test_sync_mode_metrics_record_latest_only() {
        let mut metrics = SyncModeMetrics::new();
        metrics.record_sync(
            "beacons",
            crate::qos::SyncMode::LatestOnly,
            500,
            std::time::Duration::from_millis(5),
        );

        assert_eq!(metrics.total_syncs, 1);
        assert_eq!(metrics.latest_only_syncs, 1);
        assert_eq!(metrics.latest_only_bytes, 500);
        assert_eq!(metrics.latest_only_duration_ms, 5);
    }

    #[test]
    fn test_sync_mode_metrics_bandwidth_savings() {
        let mut metrics = SyncModeMetrics::new();

        // Simulate full history sync: 30,000 bytes
        metrics.record_sync(
            "beacons",
            crate::qos::SyncMode::FullHistory,
            30000,
            std::time::Duration::from_millis(100),
        );

        // Simulate latest only sync: 100 bytes
        metrics.record_sync(
            "beacons",
            crate::qos::SyncMode::LatestOnly,
            100,
            std::time::Duration::from_millis(2),
        );

        assert_eq!(metrics.avg_full_history_bytes(), 30000.0);
        assert_eq!(metrics.avg_latest_only_bytes(), 100.0);

        // Bandwidth savings ratio should be 300x
        let ratio = metrics.bandwidth_savings_ratio().unwrap();
        assert_eq!(ratio, 300.0);
    }

    #[test]
    fn test_sync_mode_metrics_reset() {
        let mut metrics = SyncModeMetrics::new();
        metrics.record_sync(
            "beacons",
            crate::qos::SyncMode::LatestOnly,
            500,
            std::time::Duration::from_millis(5),
        );

        assert_eq!(metrics.total_syncs, 1);

        metrics.reset();

        assert_eq!(metrics.total_syncs, 0);
        assert_eq!(metrics.latest_only_syncs, 0);
    }

    #[test]
    fn test_sync_mode_metrics_windowed() {
        let mut metrics = SyncModeMetrics::new();
        metrics.record_sync(
            "track_history",
            crate::qos::SyncMode::WindowedHistory {
                window_seconds: 300,
            },
            5000,
            std::time::Duration::from_millis(20),
        );

        assert_eq!(metrics.total_syncs, 1);
        assert_eq!(metrics.windowed_syncs, 1);
        assert_eq!(metrics.windowed_bytes, 5000);
    }

    // === Deletion-aware query tests (ADR-034, Issue #369) ===

    #[test]
    fn test_query_include_deleted() {
        let inner = Query::All;
        let query = Query::IncludeDeleted(Box::new(inner));

        assert!(query.includes_deleted());
        assert!(!query.is_deleted_only());

        match query.inner_query() {
            Query::All => {}
            _ => panic!("Expected All query inside IncludeDeleted"),
        }
    }

    #[test]
    fn test_query_deleted_only() {
        let query = Query::DeletedOnly;

        assert!(query.includes_deleted());
        assert!(query.is_deleted_only());
    }

    #[test]
    fn test_query_with_deleted() {
        // Normal query should be wrapped
        let query = Query::All;
        let wrapped = query.with_deleted();
        assert!(matches!(wrapped, Query::IncludeDeleted(_)));

        // Already wrapped should stay the same
        let already_wrapped = Query::IncludeDeleted(Box::new(Query::All));
        let still_wrapped = already_wrapped.with_deleted();
        assert!(matches!(still_wrapped, Query::IncludeDeleted(_)));

        // DeletedOnly should stay the same
        let deleted_only = Query::DeletedOnly;
        let still_deleted_only = deleted_only.with_deleted();
        assert!(matches!(still_deleted_only, Query::DeletedOnly));
    }

    #[test]
    fn test_query_matches_deletion_state_normal() {
        let query = Query::All;

        // Non-deleted document should match
        let mut non_deleted = Document::new(HashMap::new());
        non_deleted.set("name", Value::String("test".to_string()));
        assert!(query.matches_deletion_state(&non_deleted));

        // Deleted document should NOT match
        let mut deleted = Document::new(HashMap::new());
        deleted.set("name", Value::String("test".to_string()));
        deleted.set("_deleted", Value::Bool(true));
        assert!(!query.matches_deletion_state(&deleted));

        // _deleted=false should match
        let mut not_deleted = Document::new(HashMap::new());
        not_deleted.set("_deleted", Value::Bool(false));
        assert!(query.matches_deletion_state(&not_deleted));
    }

    #[test]
    fn test_query_matches_deletion_state_include_deleted() {
        let query = Query::IncludeDeleted(Box::new(Query::All));

        // Non-deleted document should match
        let non_deleted = Document::new(HashMap::new());
        assert!(query.matches_deletion_state(&non_deleted));

        // Deleted document should also match
        let mut deleted = Document::new(HashMap::new());
        deleted.set("_deleted", Value::Bool(true));
        assert!(query.matches_deletion_state(&deleted));
    }

    #[test]
    fn test_query_matches_deletion_state_deleted_only() {
        let query = Query::DeletedOnly;

        // Non-deleted document should NOT match
        let non_deleted = Document::new(HashMap::new());
        assert!(!query.matches_deletion_state(&non_deleted));

        // Deleted document should match
        let mut deleted = Document::new(HashMap::new());
        deleted.set("_deleted", Value::Bool(true));
        assert!(query.matches_deletion_state(&deleted));

        // _deleted=false should NOT match
        let mut not_deleted = Document::new(HashMap::new());
        not_deleted.set("_deleted", Value::Bool(false));
        assert!(!query.matches_deletion_state(&not_deleted));
    }

    #[test]
    fn test_query_include_deleted_with_filter() {
        // IncludeDeleted wrapping a more complex query
        let inner = Query::Eq {
            field: "type".to_string(),
            value: Value::String("contact_report".to_string()),
        };
        let query = Query::IncludeDeleted(Box::new(inner));

        assert!(query.includes_deleted());

        match query.inner_query() {
            Query::Eq { field, value } => {
                assert_eq!(field, "type");
                assert_eq!(value, &Value::String("contact_report".to_string()));
            }
            _ => panic!("Expected Eq query inside IncludeDeleted"),
        }
    }

    #[test]
    fn test_query_normal_excludes_deleted() {
        // All query variants (except IncludeDeleted/DeletedOnly) should exclude deleted docs
        let queries = vec![
            Query::All,
            Query::Eq {
                field: "x".to_string(),
                value: Value::Null,
            },
            Query::And(vec![Query::All]),
            Query::Or(vec![Query::All]),
            Query::Not(Box::new(Query::All)),
        ];

        let mut deleted_doc = Document::new(HashMap::new());
        deleted_doc.set("_deleted", Value::Bool(true));

        for query in queries {
            assert!(
                !query.matches_deletion_state(&deleted_doc),
                "Query {:?} should exclude deleted docs",
                query
            );
        }
    }
}
