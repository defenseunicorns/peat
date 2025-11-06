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

/// Query abstraction that works across backends
///
/// Provides a simple query language that can be translated to backend-specific
/// query formats (Ditto DQL, Automerge queries, etc).
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
}
