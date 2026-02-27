//! Core types for persistence layer

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Unique document identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for DocumentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for DocumentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Query builder for filtering documents
#[derive(Debug, Clone)]
pub struct Query {
    pub(crate) filters: Vec<Filter>,
    pub(crate) sort: Option<Sort>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

impl Query {
    /// Create a new empty query (matches all documents)
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            sort: None,
            limit: None,
            offset: None,
        }
    }

    /// Query all documents (no filtering)
    pub fn all() -> Self {
        Self::new()
    }

    /// Add a filter condition
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Set sort order
    pub fn sort(mut self, field: impl Into<String>, order: SortOrder) -> Self {
        self.sort = Some(Sort {
            field: field.into(),
            order,
        });
        self
    }

    /// Limit number of results
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set offset for pagination
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

impl Default for Query {
    fn default() -> Self {
        Self::new()
    }
}

/// Filter condition for queries
#[derive(Debug, Clone)]
pub enum Filter {
    /// Field equals value
    Eq(String, serde_json::Value),
    /// Field not equals value
    Ne(String, serde_json::Value),
    /// Field greater than value
    Gt(String, serde_json::Value),
    /// Field greater than or equal to value
    Gte(String, serde_json::Value),
    /// Field less than value
    Lt(String, serde_json::Value),
    /// Field less than or equal to value
    Lte(String, serde_json::Value),
    /// Field contains value (for strings)
    Contains(String, String),
    /// Field starts with value (for strings)
    StartsWith(String, String),
    /// Field is in list of values
    In(String, Vec<serde_json::Value>),
    /// Logical AND of filters
    And(Vec<Filter>),
    /// Logical OR of filters
    Or(Vec<Filter>),
}

/// Sort configuration
#[derive(Debug, Clone)]
pub struct Sort {
    pub field: String,
    pub order: SortOrder,
}

/// Sort order direction
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// Document with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Document ID (if persisted)
    pub id: Option<DocumentId>,
    /// Document fields as JSON
    pub fields: serde_json::Value,
    /// Metadata (timestamps, version, etc.)
    pub metadata: DocumentMetadata,
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocumentMetadata {
    /// When document was created
    pub created_at: Option<i64>,
    /// When document was last updated
    pub updated_at: Option<i64>,
    /// Document version (for optimistic locking)
    pub version: Option<u64>,
}

// =============================================================================
// Bypass Integration Types (ADR-042)
// =============================================================================

/// Priority level for messages
///
/// Used for QoS and bandwidth allocation in bypass channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessagePriority {
    /// Background priority (bulk transfers)
    Background = 0,
    /// Normal priority (default)
    #[default]
    Normal = 1,
    /// High priority (important updates)
    High = 2,
    /// Critical priority (emergency commands)
    Critical = 3,
}

/// Options for write operations
///
/// Controls how a document write is handled, including whether to
/// bypass the CRDT sync engine for low-latency delivery.
///
/// # Example
///
/// ```rust
/// use peat_persistence::WriteOptions;
///
/// // High-frequency position update - bypass CRDT
/// let opts = WriteOptions {
///     bypass_sync: true,
///     ..Default::default()
/// };
///
/// // Important command - use CRDT for reliability
/// let opts = WriteOptions {
///     bypass_sync: false,
///     priority: peat_persistence::MessagePriority::High,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct WriteOptions {
    /// Skip CRDT sync and send via UDP bypass channel
    ///
    /// When `true`, the document is sent directly via UDP without
    /// going through the CRDT synchronization engine. This provides:
    /// - Lower latency (~5ms vs ~200ms)
    /// - Lower overhead (12-byte header vs CRDT metadata)
    /// - No persistence or conflict resolution
    ///
    /// Use for ephemeral data like position updates, telemetry, etc.
    pub bypass_sync: bool,

    /// Time-to-live for bypass messages
    ///
    /// Messages older than this are dropped by receivers.
    /// Only applies when `bypass_sync` is `true`.
    /// Default: 5 seconds
    pub ttl: Option<Duration>,

    /// Message priority for QoS
    ///
    /// Affects bandwidth allocation and processing order.
    pub priority: MessagePriority,

    /// Target address for unicast bypass
    ///
    /// Required when the collection is configured for unicast transport.
    /// Ignored for multicast/broadcast collections.
    pub target_addr: Option<std::net::SocketAddr>,
}

impl WriteOptions {
    /// Create options for bypass mode
    pub fn bypass() -> Self {
        Self {
            bypass_sync: true,
            ..Default::default()
        }
    }

    /// Create options for bypass mode with TTL
    pub fn bypass_with_ttl(ttl: Duration) -> Self {
        Self {
            bypass_sync: true,
            ttl: Some(ttl),
            ..Default::default()
        }
    }

    /// Create options for normal CRDT sync
    pub fn sync() -> Self {
        Self::default()
    }

    /// Set the bypass flag
    pub fn with_bypass(mut self, bypass: bool) -> Self {
        self.bypass_sync = bypass;
        self
    }

    /// Set TTL for bypass messages
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Set message priority
    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set target address for unicast
    pub fn with_target(mut self, addr: std::net::SocketAddr) -> Self {
        self.target_addr = Some(addr);
        self
    }
}

/// Options for subscription operations
///
/// Controls what data sources are included in a subscription stream.
///
/// # Example
///
/// ```rust
/// use peat_persistence::SubscribeOptions;
///
/// // Subscribe to bypass messages only (high-frequency telemetry)
/// let opts = SubscribeOptions {
///     include_bypass: true,
///     include_sync: false,
///     ..Default::default()
/// };
///
/// // Subscribe to both sources (unified view)
/// let opts = SubscribeOptions {
///     include_bypass: true,
///     include_sync: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct SubscribeOptions {
    /// Include messages from UDP bypass channel
    ///
    /// When `true`, the subscription stream will include messages
    /// received via the bypass channel (low-latency UDP).
    pub include_bypass: bool,

    /// Include messages from CRDT sync
    ///
    /// When `true`, the subscription stream will include changes
    /// from the CRDT synchronization engine (reliable, persistent).
    pub include_sync: bool,

    /// Filter by minimum priority
    ///
    /// Only include messages with priority >= this level.
    /// Default: None (include all priorities)
    pub min_priority: Option<MessagePriority>,
}

impl Default for SubscribeOptions {
    fn default() -> Self {
        Self {
            include_bypass: false,
            include_sync: true,
            min_priority: None,
        }
    }
}

impl SubscribeOptions {
    /// Subscribe to bypass channel only
    pub fn bypass_only() -> Self {
        Self {
            include_bypass: true,
            include_sync: false,
            min_priority: None,
        }
    }

    /// Subscribe to CRDT sync only (default behavior)
    pub fn sync_only() -> Self {
        Self::default()
    }

    /// Subscribe to both bypass and sync
    pub fn both() -> Self {
        Self {
            include_bypass: true,
            include_sync: true,
            min_priority: None,
        }
    }

    /// Set include_bypass flag
    pub fn with_bypass(mut self, include: bool) -> Self {
        self.include_bypass = include;
        self
    }

    /// Set include_sync flag
    pub fn with_sync(mut self, include: bool) -> Self {
        self.include_sync = include;
        self
    }

    /// Filter by minimum priority
    pub fn with_min_priority(mut self, priority: MessagePriority) -> Self {
        self.min_priority = Some(priority);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_id_creation() {
        let id = DocumentId::new("test-id");
        assert_eq!(id.as_str(), "test-id");
    }

    #[test]
    fn test_document_id_from_string() {
        let id: DocumentId = "another-id".into();
        assert_eq!(id.as_str(), "another-id");
    }

    #[test]
    fn test_document_id_display() {
        let id = DocumentId::new("display-test");
        assert_eq!(format!("{}", id), "display-test");
    }

    #[test]
    fn test_query_builder() {
        let query = Query::new()
            .limit(10)
            .offset(5)
            .sort("created_at", SortOrder::Descending);

        assert_eq!(query.limit, Some(10));
        assert_eq!(query.offset, Some(5));
        assert!(query.sort.is_some());
    }

    #[test]
    fn test_query_all() {
        let query = Query::all();
        assert!(query.filters.is_empty());
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_filter_eq() {
        let filter = Filter::Eq("status".to_string(), serde_json::json!("active"));
        match filter {
            Filter::Eq(field, _) => assert_eq!(field, "status"),
            _ => panic!("Wrong filter type"),
        }
    }

    #[test]
    fn test_document_metadata_default() {
        let metadata = DocumentMetadata::default();
        assert!(metadata.created_at.is_none());
        assert!(metadata.updated_at.is_none());
        assert!(metadata.version.is_none());
    }

    #[test]
    fn test_write_options_default() {
        let opts = WriteOptions::default();
        assert!(!opts.bypass_sync);
        assert!(opts.ttl.is_none());
        assert_eq!(opts.priority, MessagePriority::Normal);
        assert!(opts.target_addr.is_none());
    }

    #[test]
    fn test_write_options_bypass() {
        let opts = WriteOptions::bypass();
        assert!(opts.bypass_sync);
    }

    #[test]
    fn test_write_options_bypass_with_ttl() {
        let opts = WriteOptions::bypass_with_ttl(Duration::from_millis(200));
        assert!(opts.bypass_sync);
        assert_eq!(opts.ttl, Some(Duration::from_millis(200)));
    }

    #[test]
    fn test_write_options_builder() {
        let addr: std::net::SocketAddr = "127.0.0.1:5150".parse().unwrap();
        let opts = WriteOptions::default()
            .with_bypass(true)
            .with_ttl(Duration::from_secs(1))
            .with_priority(MessagePriority::High)
            .with_target(addr);

        assert!(opts.bypass_sync);
        assert_eq!(opts.ttl, Some(Duration::from_secs(1)));
        assert_eq!(opts.priority, MessagePriority::High);
        assert_eq!(opts.target_addr, Some(addr));
    }

    #[test]
    fn test_subscribe_options_default() {
        let opts = SubscribeOptions::default();
        assert!(!opts.include_bypass);
        assert!(opts.include_sync);
        assert!(opts.min_priority.is_none());
    }

    #[test]
    fn test_subscribe_options_bypass_only() {
        let opts = SubscribeOptions::bypass_only();
        assert!(opts.include_bypass);
        assert!(!opts.include_sync);
    }

    #[test]
    fn test_subscribe_options_both() {
        let opts = SubscribeOptions::both();
        assert!(opts.include_bypass);
        assert!(opts.include_sync);
    }

    #[test]
    fn test_subscribe_options_builder() {
        let opts = SubscribeOptions::default()
            .with_bypass(true)
            .with_sync(true)
            .with_min_priority(MessagePriority::High);

        assert!(opts.include_bypass);
        assert!(opts.include_sync);
        assert_eq!(opts.min_priority, Some(MessagePriority::High));
    }

    #[test]
    fn test_message_priority_values() {
        assert_eq!(MessagePriority::Background as u8, 0);
        assert_eq!(MessagePriority::Normal as u8, 1);
        assert_eq!(MessagePriority::High as u8, 2);
        assert_eq!(MessagePriority::Critical as u8, 3);
    }
}
