//! HIVE FFI - Foreign Function Interface for Kotlin/Swift
//!
//! This crate provides UniFFI bindings to expose HIVE functionality
//! to Kotlin (Android/ATAK) and Swift (iOS) applications.
//!
//! ## Features
//!
//! - **CoT Encoding**: Convert track data to Cursor-on-Target XML
//! - **Sync** (optional): P2P document sync via AutomergeIroh backend
//!
//! Uses proc-macro only UniFFI approach (no UDL file).
//!
//! ## Android JNI Support
//!
//! This crate also provides direct JNI bindings that bypass JNA's symbol lookup
//! issues on Android. The JNI functions are exported with standard naming
//! (Java_package_Class_method) and can be called directly via Android's NDK.

use std::collections::HashMap;
use std::sync::Arc;

// JNI support for Android
use jni::objects::{JClass, JString, JValue, GlobalRef};
use jni::sys::{jint, jstring, JavaVM, JNI_VERSION_1_6};
use jni::JNIEnv;
use std::os::raw::c_void;
use std::sync::{LazyLock, Mutex};

// Global JavaVM reference for JNI callbacks from any thread
static JAVA_VM: LazyLock<Mutex<Option<jni::JavaVM>>> = LazyLock::new(|| Mutex::new(None));

// Global reference to PeerEventManager class
static PEER_EVENT_MANAGER_CLASS: LazyLock<Mutex<Option<GlobalRef>>> = LazyLock::new(|| Mutex::new(None));

use hive_protocol::cot::{
    CotEncoder, Position as CotPosition, TrackUpdate, Velocity as CotVelocity,
};

#[cfg(feature = "sync")]
use hive_protocol::network::{IrohTransport, PeerInfo as HivePeerInfo, TransportPeerEvent};
#[cfg(feature = "sync")]
use hive_protocol::storage::{AutomergeBackend, AutomergeStore, StorageBackend, SyncCapable};
#[cfg(feature = "sync")]
use hive_protocol::sync::automerge::AutomergeIrohBackend;
#[cfg(feature = "sync")]
use hive_protocol::sync::{BackendConfig, DataSyncBackend, TransportConfig};
#[cfg(feature = "sync")]
use std::net::SocketAddr;
#[cfg(feature = "sync")]
use std::path::PathBuf;
#[cfg(feature = "sync")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "sync")]
use tokio::sync::RwLock;

// Setup UniFFI scaffolding
uniffi::setup_scaffolding!();

/// Get the HIVE library version
#[uniffi::export]
pub fn hive_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Geographic position for FFI
#[derive(Debug, Clone, uniffi::Record)]
pub struct Position {
    /// Latitude in degrees (WGS84)
    pub lat: f64,
    /// Longitude in degrees (WGS84)
    pub lon: f64,
    /// Height Above Ellipsoid in meters (optional)
    pub hae: Option<f64>,
}

/// Velocity vector for FFI
#[derive(Debug, Clone, uniffi::Record)]
pub struct Velocity {
    /// Bearing in degrees (0 = North, clockwise)
    pub bearing: f64,
    /// Speed in meters per second
    pub speed_mps: f64,
}

/// Track data for CoT encoding
#[derive(Debug, Clone, uniffi::Record)]
pub struct TrackData {
    /// Unique track identifier
    pub track_id: String,
    /// Source platform ID
    pub source_platform: String,
    /// Geographic position
    pub position: Position,
    /// Optional velocity
    pub velocity: Option<Velocity>,
    /// MIL-STD-2525 classification (e.g., "a-f-G-U-C")
    pub classification: String,
    /// Detection confidence (0.0 - 1.0)
    pub confidence: f64,
    /// Optional cell ID (for squad-level tracks)
    pub cell_id: Option<String>,
    /// Optional formation ID
    pub formation_id: Option<String>,
}

/// FFI Error type
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum HiveError {
    #[error("Encoding error: {msg}")]
    EncodingError { msg: String },
    #[error("Invalid input: {msg}")]
    InvalidInput { msg: String },
    #[error("Storage error: {msg}")]
    StorageError { msg: String },
    #[error("Connection error: {msg}")]
    ConnectionError { msg: String },
    #[error("Sync error: {msg}")]
    SyncError { msg: String },
}

/// Encode a track to CoT XML string
#[uniffi::export]
pub fn encode_track_to_cot(track: TrackData) -> Result<String, HiveError> {
    // Validate input
    if track.track_id.is_empty() {
        return Err(HiveError::InvalidInput {
            msg: "track_id cannot be empty".to_string(),
        });
    }

    // Convert FFI types to internal types
    let position = CotPosition {
        lat: track.position.lat,
        lon: track.position.lon,
        cep_m: None,
        hae: track.position.hae,
    };

    let velocity = track.velocity.map(|v| CotVelocity {
        bearing: v.bearing,
        speed_mps: v.speed_mps,
    });

    let track_update = TrackUpdate {
        track_id: track.track_id,
        source_platform: track.source_platform,
        source_model: "hive-ffi".to_string(),
        model_version: hive_version(),
        cell_id: track.cell_id,
        formation_id: track.formation_id,
        timestamp: chrono::Utc::now(),
        position,
        velocity,
        classification: track.classification,
        confidence: track.confidence,
        attributes: HashMap::new(),
    };

    // Encode to CoT
    let encoder = CotEncoder::new();
    let event = encoder
        .track_update_to_event(&track_update)
        .map_err(|e| HiveError::EncodingError { msg: e.to_string() })?;

    event
        .to_xml()
        .map_err(|e| HiveError::EncodingError { msg: e.to_string() })
}

/// Create a position from coordinates
#[uniffi::export]
pub fn create_position(lat: f64, lon: f64, hae: Option<f64>) -> Position {
    Position { lat, lon, hae }
}

/// Create a velocity from bearing and speed
#[uniffi::export]
pub fn create_velocity(bearing: f64, speed_mps: f64) -> Velocity {
    Velocity { bearing, speed_mps }
}

// =============================================================================
// HiveNode - P2P Sync Support (requires "sync" feature)
// =============================================================================

/// Configuration for creating a HiveNode
#[cfg(feature = "sync")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct NodeConfig {
    /// Application/Formation ID (used for peer discovery and authentication)
    /// This identifies which "formation" or "swarm" this node belongs to.
    pub app_id: String,
    /// Shared secret key (base64-encoded 32 bytes) for peer authentication
    /// Only peers with matching app_id AND shared_key can connect.
    /// Generate with: `openssl rand -base64 32`
    pub shared_key: String,
    /// Bind address for P2P connections (e.g., "0.0.0.0:0" for auto-assign)
    pub bind_address: Option<String>,
    /// Storage path for Automerge documents
    pub storage_path: String,
}

/// Information about a peer node for connection
#[cfg(feature = "sync")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct PeerInfo {
    /// Human-readable peer name
    pub name: String,
    /// Hex-encoded node ID (Iroh endpoint ID)
    pub node_id: String,
    /// List of addresses (e.g., "127.0.0.1:19001")
    pub addresses: Vec<String>,
    /// Optional relay URL
    pub relay_url: Option<String>,
}

/// Sync statistics
#[cfg(feature = "sync")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncStats {
    /// Whether sync is currently active
    pub sync_active: bool,
    /// Number of connected peers
    pub connected_peers: u32,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
}

/// Type of document change event
#[cfg(feature = "sync")]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum ChangeType {
    /// Document was created or updated
    Upsert,
    /// Document was deleted
    Delete,
}

/// Document change event for subscriptions
#[cfg(feature = "sync")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct DocumentChange {
    /// Collection name
    pub collection: String,
    /// Document ID
    pub doc_id: String,
    /// Type of change
    pub change_type: ChangeType,
}

/// Callback interface for document change notifications
///
/// Implement this interface in Kotlin/Swift to receive document updates.
#[cfg(feature = "sync")]
#[uniffi::export(callback_interface)]
pub trait DocumentCallback: Send + Sync {
    /// Called when a document changes
    fn on_change(&self, change: DocumentChange);

    /// Called when an error occurs in the subscription
    fn on_error(&self, message: String);
}

/// Handle for an active document subscription
///
/// Drop this handle to unsubscribe from document changes.
#[cfg(feature = "sync")]
#[derive(uniffi::Object)]
pub struct SubscriptionHandle {
    /// Flag to signal the subscription should stop
    active: Arc<AtomicBool>,
}

#[cfg(feature = "sync")]
impl SubscriptionHandle {
    fn new(active: Arc<AtomicBool>) -> Self {
        Self { active }
    }
}

#[cfg(feature = "sync")]
#[uniffi::export]
impl SubscriptionHandle {
    /// Check if the subscription is still active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Cancel the subscription
    pub fn cancel(&self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

#[cfg(feature = "sync")]
impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

/// A HIVE network node with P2P sync capabilities
///
/// Wraps AutomergeIrohBackend for authenticated document sync.
/// Requires matching app_id and shared_key for peer connections.
#[cfg(feature = "sync")]
#[derive(uniffi::Object)]
pub struct HiveNode {
    /// The sync backend with FormationKey authentication
    sync_backend: Arc<AutomergeIrohBackend>,
    /// Storage backend for document operations
    storage_backend: Arc<RwLock<AutomergeBackend>>,
    transport: Arc<IrohTransport>,
    /// Store reference for subscriptions
    store: Arc<AutomergeStore>,
    #[allow(dead_code)] // Kept for potential future use (e.g., storage cleanup)
    storage_path: PathBuf,
    /// Tokio runtime for async operations
    runtime: Arc<tokio::runtime::Runtime>,
    /// Flag to stop cleanup task on drop (used by background task)
    #[allow(dead_code)]
    cleanup_running: Arc<AtomicBool>,
}

#[cfg(feature = "sync")]
#[uniffi::export]
impl HiveNode {
    /// Get this node's unique identifier (hex-encoded)
    pub fn node_id(&self) -> String {
        hex::encode(self.transport.endpoint_id().as_bytes())
    }

    /// Get this node's endpoint address for peer connections
    pub fn endpoint_addr(&self) -> String {
        format!("{:?}", self.transport.endpoint_addr())
    }

    /// Get the number of connected peers
    pub fn peer_count(&self) -> u32 {
        self.transport.peer_count() as u32
    }

    /// Get list of connected peer IDs
    pub fn connected_peers(&self) -> Vec<String> {
        self.transport
            .connected_peers()
            .iter()
            .map(|id| hex::encode(id.as_bytes()))
            .collect()
    }

    /// Start sync operations
    pub fn start_sync(&self) -> Result<(), HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            backend
                .start_sync()
                .map_err(|e| HiveError::SyncError { msg: e.to_string() })
        })
    }

    /// Stop sync operations
    pub fn stop_sync(&self) -> Result<(), HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            backend
                .stop_sync()
                .map_err(|e| HiveError::SyncError { msg: e.to_string() })
        })
    }

    /// Get sync statistics
    pub fn sync_stats(&self) -> Result<SyncStats, HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let stats = backend
                .sync_stats()
                .map_err(|e| HiveError::SyncError { msg: e.to_string() })?;

            Ok(SyncStats {
                sync_active: stats.peer_count > 0, // Infer from peer count
                connected_peers: self.transport.peer_count() as u32,
                bytes_sent: stats.bytes_sent,
                bytes_received: stats.bytes_received,
            })
        })
    }

    /// Connect to a peer node
    pub fn connect_peer(&self, peer: PeerInfo) -> Result<(), HiveError> {
        let hive_peer = HivePeerInfo {
            name: peer.name,
            node_id: peer.node_id,
            addresses: peer.addresses,
            relay_url: peer.relay_url,
        };

        self.runtime.block_on(async {
            self.transport
                .connect_peer(&hive_peer)
                .await
                .map_err(|e| HiveError::ConnectionError { msg: e.to_string() })?;

            Ok(())
        })
    }

    /// Disconnect from a peer by node ID
    ///
    /// Note: Currently disconnects matching peer from internal connection map.
    pub fn disconnect_peer(&self, node_id: &str) -> Result<(), HiveError> {
        // Find the matching endpoint ID from connected peers
        let connected = self.transport.connected_peers();
        for endpoint_id in connected {
            if hex::encode(endpoint_id.as_bytes()) == node_id {
                return self
                    .transport
                    .disconnect(&endpoint_id)
                    .map_err(|e| HiveError::ConnectionError { msg: e.to_string() });
            }
        }

        Err(HiveError::ConnectionError {
            msg: format!("Peer {} not found in connected peers", node_id),
        })
    }

    /// Store a JSON document in a collection
    pub fn put_document(
        &self,
        collection: &str,
        doc_id: &str,
        json_data: &str,
    ) -> Result<(), HiveError> {
        // Parse JSON to validate it
        let _: serde_json::Value =
            serde_json::from_str(json_data).map_err(|e| HiveError::InvalidInput {
                msg: format!("Invalid JSON: {}", e),
            })?;

        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collection);

            coll.upsert(doc_id, json_data.as_bytes().to_vec())
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })
        })
    }

    /// Retrieve a document from a collection as JSON
    pub fn get_document(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> Result<Option<String>, HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collection);

            match coll.get(doc_id) {
                Ok(Some(bytes)) => {
                    let json = String::from_utf8(bytes).map_err(|e| HiveError::StorageError {
                        msg: format!("Invalid UTF-8: {}", e),
                    })?;
                    Ok(Some(json))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(HiveError::StorageError { msg: e.to_string() }),
            }
        })
    }

    /// Delete a document from a collection
    pub fn delete_document(&self, collection: &str, doc_id: &str) -> Result<(), HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collection);

            coll.delete(doc_id)
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })
        })
    }

    /// List all document IDs in a collection
    pub fn list_documents(&self, collection: &str) -> Result<Vec<String>, HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collection);

            let docs = coll
                .scan()
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })?;

            Ok(docs.into_iter().map(|(id, _)| id).collect())
        })
    }

    /// Manually trigger sync for a specific document
    pub fn sync_document(&self, collection: &str, doc_id: &str) -> Result<(), HiveError> {
        let doc_key = format!("{}:{}", collection, doc_id);

        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;

            backend
                .sync_document(&doc_key)
                .await
                .map_err(|e| HiveError::SyncError { msg: e.to_string() })
        })
    }

    /// Subscribe to document changes
    ///
    /// Returns a SubscriptionHandle that must be kept alive to receive callbacks.
    /// When the handle is dropped or cancel() is called, the subscription stops.
    ///
    /// The callback will receive DocumentChange events for all documents.
    /// Filter by collection in your callback implementation if needed.
    ///
    /// Note: Only one subscription per node is supported. Calling subscribe again
    /// will fail if a subscription is already active.
    pub fn subscribe(
        &self,
        callback: Box<dyn DocumentCallback>,
    ) -> Result<Arc<SubscriptionHandle>, HiveError> {
        // Get the change receiver from the store (broadcast channel)
        let change_rx = self.store.subscribe_to_changes();

        // Create active flag for the subscription
        let active = Arc::new(AtomicBool::new(true));
        let active_clone = Arc::clone(&active);

        // Spawn a task to listen for changes and call the callback
        let callback = Arc::new(callback);
        self.runtime.spawn(async move {
            let mut rx = change_rx;

            while active_clone.load(Ordering::SeqCst) {
                tokio::select! {
                    result = rx.recv() => {
                        match result {
                            Ok(doc_key) => {
                                // Parse the document key (format: "collection:doc_id")
                                let change = if let Some((collection, doc_id)) = doc_key.split_once(':') {
                                    DocumentChange {
                                        collection: collection.to_string(),
                                        doc_id: doc_id.to_string(),
                                        change_type: ChangeType::Upsert, // We only get notifications on upsert currently
                                    }
                                } else {
                                    // Key without colon - treat as collection with doc_id
                                    DocumentChange {
                                        collection: "default".to_string(),
                                        doc_id: doc_key,
                                        change_type: ChangeType::Upsert,
                                    }
                                };

                                callback.on_change(change);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                // Some messages were skipped due to slow receiver
                                callback.on_error(format!("Lagged {} messages", n));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                // Channel closed
                                callback.on_error("Document change channel closed".to_string());
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                        // Periodic check if we should stop
                        if !active_clone.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                }
            }
        });

        Ok(Arc::new(SubscriptionHandle::new(active)))
    }
}

/// Create a new HiveNode with FormationKey authentication
///
/// Requires `app_id` and `shared_key` for peer authentication.
/// Only peers with matching credentials can connect and sync.
///
/// # Arguments
///
/// * `config` - Node configuration including:
///   - `app_id`: Formation/application identifier (use same value for all nodes in your swarm)
///   - `shared_key`: Base64-encoded 32-byte secret key (generate with `openssl rand -base64 32`)
///   - `bind_address`: Optional address to bind (default: "0.0.0.0:0")
///   - `storage_path`: Directory for persistent storage
///
/// Note: This function is NOT async because we manage our own Tokio runtime
/// to ensure proper context for Iroh transport operations.
#[cfg(feature = "sync")]
#[uniffi::export]
pub fn create_node(config: NodeConfig) -> Result<Arc<HiveNode>, HiveError> {
    // Validate credentials
    if config.app_id.is_empty() {
        return Err(HiveError::InvalidInput {
            msg: "app_id cannot be empty".to_string(),
        });
    }
    if config.shared_key.is_empty() {
        return Err(HiveError::InvalidInput {
            msg: "shared_key cannot be empty".to_string(),
        });
    }

    // Create a dedicated Tokio runtime for this node
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| HiveError::SyncError {
            msg: format!("Failed to create runtime: {}", e),
        })?;

    // Parse bind address
    let bind_addr: SocketAddr = config
        .bind_address
        .as_deref()
        .unwrap_or("0.0.0.0:0")
        .parse()
        .map_err(|e| HiveError::InvalidInput {
            msg: format!("Invalid bind address: {}", e),
        })?;

    // Create storage path
    let storage_path = PathBuf::from(&config.storage_path);
    std::fs::create_dir_all(&storage_path).map_err(|e| HiveError::StorageError {
        msg: format!("Failed to create storage directory: {}", e),
    })?;

    // Create AutomergeStore
    let store =
        Arc::new(
            AutomergeStore::open(&storage_path).map_err(|e| HiveError::StorageError {
                msg: format!("Failed to open store: {}", e),
            })?,
        );

    // Create IrohTransport with mDNS discovery enabled (Issue #233)
    // Use app_id + storage_path as seed for deterministic but unique EndpointId
    let seed = format!("{}/{}", config.app_id, config.storage_path);
    let transport = runtime.block_on(async {
        IrohTransport::from_seed_with_discovery_at_addr(&seed, bind_addr)
            .await
            .map_err(|e| HiveError::ConnectionError {
                msg: format!("Failed to create transport with discovery: {}", e),
            })
    })?;
    let transport = Arc::new(transport);

    // Create storage backend with transport
    let storage_backend = Arc::new(AutomergeBackend::with_transport(
        Arc::clone(&store),
        Arc::clone(&transport),
    ));

    // Create sync backend (AutomergeIrohBackend) for authenticated P2P sync
    // Note: AutomergeIrohBackend wraps storage::AutomergeBackend for the DataSyncBackend trait
    let sync_backend = Arc::new(AutomergeIrohBackend::new(
        Arc::clone(&storage_backend),
        Arc::clone(&transport),
    ));

    // IMPORTANT (Issue #275): Subscribe to peer events BEFORE initializing sync backend.
    // The initialize() call spawns the accept loop, so we need to subscribe first
    // to catch all connection events including the initial ones.
    let mut event_rx = transport.subscribe_peer_events();

    // Initialize sync backend with credentials for FormationKey authentication
    let backend_config = BackendConfig {
        app_id: config.app_id.clone(),
        persistence_dir: storage_path.clone(),
        shared_key: Some(config.shared_key.clone()),
        transport: TransportConfig::default(),
        extra: std::collections::HashMap::new(),
    };

    runtime.block_on(async {
        sync_backend
            .initialize(backend_config)
            .await
            .map_err(|e| HiveError::SyncError {
                msg: format!("Failed to initialize sync backend: {}", e),
            })
    })?;

    #[cfg(target_os = "android")]
    android_log("=== sync_backend.initialize() completed successfully ===");

    // Start background task to listen for peer events and forward to Java (Issue #275)
    let cleanup_running = Arc::new(AtomicBool::new(true));
    let cleanup_flag = Arc::clone(&cleanup_running);
    let runtime_arc = Arc::new(runtime);

    // Clone transport for the cleanup task
    let transport_for_cleanup = Arc::clone(&transport);

    // Log that we're starting the peer event listener
    #[cfg(target_os = "android")]
    android_log("Starting peer event listener task (Issue #275)");

    runtime_arc.spawn(async move {
        #[cfg(target_os = "android")]
        android_log("Peer event listener task running");

        while cleanup_flag.load(Ordering::Relaxed) {
            tokio::select! {
                event_result = event_rx.recv() => {
                    match event_result {
                        Some(event) => {
                            #[cfg(target_os = "android")]
                            android_log(&format!("Received transport peer event: {:?}", event));

                            match event {
                                TransportPeerEvent::Connected { endpoint_id, .. } => {
                                    let peer_id = hex::encode(endpoint_id.as_bytes());
                                    #[cfg(target_os = "android")]
                                    android_log(&format!("Processing Connected event for peer: {}", peer_id));
                                    notify_peer_connected(&peer_id);
                                }
                                TransportPeerEvent::Disconnected { endpoint_id, reason } => {
                                    let peer_id = hex::encode(endpoint_id.as_bytes());
                                    #[cfg(target_os = "android")]
                                    android_log(&format!("Processing Disconnected event for peer: {} reason: {}", peer_id, reason));
                                    notify_peer_disconnected(&peer_id, &reason);
                                }
                            }
                        }
                        None => {
                            #[cfg(target_os = "android")]
                            android_log("Event channel closed, exiting peer event listener");
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    // Periodically call peer_count() to trigger cleanup_closed_connections()
                    // This detects dead connections and emits Disconnected events
                    let count = transport_for_cleanup.peer_count();
                    #[cfg(target_os = "android")]
                    android_log(&format!("Periodic cleanup tick - peer count: {}", count));
                }
            }
        }

        #[cfg(target_os = "android")]
        android_log("Peer event listener task exiting");
    });

    Ok(Arc::new(HiveNode {
        sync_backend,
        storage_backend: Arc::new(RwLock::new(AutomergeBackend::with_transport(
            Arc::clone(&store),
            Arc::clone(&transport),
        ))),
        transport,
        store,
        storage_path,
        runtime: runtime_arc,
        cleanup_running,
    }))
}

// Add new error variants for sync operations
#[cfg(feature = "sync")]
impl From<anyhow::Error> for HiveError {
    fn from(e: anyhow::Error) -> Self {
        HiveError::SyncError { msg: e.to_string() }
    }
}

// =============================================================================
// HIVE Data Types for TAK Plugin Integration
// =============================================================================
//
// These types represent HIVE entities that can be synced and displayed in the
// ATAK plugin. They use well-known collection names for document storage.

/// Well-known collection names for HIVE data
pub mod collections {
    /// Collection for HIVE cells (teams/squads)
    pub const CELLS: &str = "cells";
    /// Collection for detected tracks (entities being tracked)
    pub const TRACKS: &str = "tracks";
    /// Collection for platforms (robots, drones, sensors)
    pub const PLATFORMS: &str = "platforms";
    /// Collection for capability advertisements
    pub const CAPABILITIES: &str = "capabilities";
    /// Collection for commands (C2 messages)
    pub const COMMANDS: &str = "commands";
}

/// Cell status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CellStatus {
    /// Cell is active and operational
    Active,
    /// Cell is forming (members joining)
    Forming,
    /// Cell has degraded capability
    Degraded,
    /// Cell is offline
    Offline,
}

impl CellStatus {
    fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "ACTIVE" => Self::Active,
            "FORMING" => Self::Forming,
            "DEGRADED" => Self::Degraded,
            "OFFLINE" => Self::Offline,
            _ => Self::Offline,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Forming => "FORMING",
            Self::Degraded => "DEGRADED",
            Self::Offline => "OFFLINE",
        }
    }
}

/// HIVE Cell information for display
#[derive(Debug, Clone, uniffi::Record)]
pub struct CellInfo {
    /// Unique cell identifier
    pub id: String,
    /// Human-readable cell name (e.g., "Alpha Team")
    pub name: String,
    /// Cell status
    pub status: CellStatus,
    /// Number of platforms in this cell
    pub platform_count: u32,
    /// Center latitude (WGS84)
    pub center_lat: f64,
    /// Center longitude (WGS84)
    pub center_lon: f64,
    /// List of capabilities (e.g., ["OBJECT_TRACKING", "COMMUNICATION"])
    pub capabilities: Vec<String>,
    /// Parent formation ID (if any)
    pub formation_id: Option<String>,
    /// Cell leader platform ID (if any)
    pub leader_id: Option<String>,
    /// Last update timestamp (Unix millis)
    pub last_update: i64,
}

/// Track category enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TrackCategory {
    Person,
    Vehicle,
    Aircraft,
    Vessel,
    Installation,
    Unknown,
}

impl TrackCategory {
    fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "PERSON" => Self::Person,
            "VEHICLE" => Self::Vehicle,
            "AIRCRAFT" => Self::Aircraft,
            "VESSEL" => Self::Vessel,
            "INSTALLATION" => Self::Installation,
            _ => Self::Unknown,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "PERSON",
            Self::Vehicle => "VEHICLE",
            Self::Aircraft => "AIRCRAFT",
            Self::Vessel => "VESSEL",
            Self::Installation => "INSTALLATION",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// Track information for display
#[derive(Debug, Clone, uniffi::Record)]
pub struct TrackInfo {
    /// Unique track identifier
    pub id: String,
    /// Source platform that detected this track
    pub source_platform: String,
    /// Cell ID that owns this track (if any)
    pub cell_id: Option<String>,
    /// Formation ID (if any)
    pub formation_id: Option<String>,
    /// Track latitude (WGS84)
    pub lat: f64,
    /// Track longitude (WGS84)
    pub lon: f64,
    /// Height above ellipsoid (meters, optional)
    pub hae: Option<f64>,
    /// Circular error probable (meters, optional)
    pub cep: Option<f64>,
    /// Heading in degrees (0 = North, optional)
    pub heading: Option<f64>,
    /// Speed in m/s (optional)
    pub speed: Option<f64>,
    /// MIL-STD-2525 classification or category
    pub classification: String,
    /// Detection confidence (0.0 - 1.0)
    pub confidence: f64,
    /// Track category
    pub category: TrackCategory,
    /// Created timestamp (Unix millis)
    pub created_at: i64,
    /// Last update timestamp (Unix millis)
    pub last_update: i64,
}

/// Platform status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PlatformStatus {
    /// Platform is ready
    Ready,
    /// Platform is active
    Active,
    /// Platform has degraded capability
    Degraded,
    /// Platform is offline
    Offline,
    /// Platform is loading/initializing
    Loading,
}

impl PlatformStatus {
    fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "READY" => Self::Ready,
            "ACTIVE" => Self::Active,
            "DEGRADED" => Self::Degraded,
            "OFFLINE" => Self::Offline,
            "LOADING" => Self::Loading,
            _ => Self::Offline,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Active => "ACTIVE",
            Self::Degraded => "DEGRADED",
            Self::Offline => "OFFLINE",
            Self::Loading => "LOADING",
        }
    }
}

/// Platform information for display
#[derive(Debug, Clone, uniffi::Record)]
pub struct PlatformInfo {
    /// Unique platform identifier
    pub id: String,
    /// Platform type (e.g., "UGV", "UAV", "Soldier System")
    pub platform_type: String,
    /// Platform name/callsign
    pub name: String,
    /// Platform status
    pub status: PlatformStatus,
    /// Platform latitude (WGS84)
    pub lat: f64,
    /// Platform longitude (WGS84)
    pub lon: f64,
    /// Height above ellipsoid (meters, optional)
    pub hae: Option<f64>,
    /// Readiness level (0.0 - 1.0)
    pub readiness: f64,
    /// List of capabilities
    pub capabilities: Vec<String>,
    /// Cell membership (if any)
    pub cell_id: Option<String>,
    /// Last heartbeat timestamp (Unix millis)
    pub last_heartbeat: i64,
}

/// Command status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CommandStatus {
    /// Command is pending execution
    Pending,
    /// Command is being executed
    Executing,
    /// Command completed successfully
    Completed,
    /// Command failed
    Failed,
    /// Command was cancelled
    Cancelled,
}

impl CommandStatus {
    fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "PENDING" => Self::Pending,
            "EXECUTING" => Self::Executing,
            "COMPLETED" => Self::Completed,
            "FAILED" => Self::Failed,
            "CANCELLED" => Self::Cancelled,
            _ => Self::Pending,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Executing => "EXECUTING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

/// Command information for C2
#[derive(Debug, Clone, uniffi::Record)]
pub struct CommandInfo {
    /// Unique command identifier
    pub id: String,
    /// Command type (e.g., "TRACK_TARGET", "MOVE", "ABORT")
    pub command_type: String,
    /// Target cell or platform ID
    pub target_id: String,
    /// Command parameters as JSON string
    pub parameters: String,
    /// Command priority (1-5, 1 = highest)
    pub priority: u8,
    /// Command status
    pub status: CommandStatus,
    /// Originator ID
    pub originator: String,
    /// Created timestamp (Unix millis)
    pub created_at: i64,
    /// Last update timestamp (Unix millis)
    pub last_update: i64,
}

// =============================================================================
// HiveNode Extensions for Typed Data Access
// =============================================================================

#[cfg(feature = "sync")]
#[uniffi::export]
impl HiveNode {
    // -------------------------------------------------------------------------
    // Cell Operations
    // -------------------------------------------------------------------------

    /// Get all cells from the sync document
    pub fn get_cells(&self) -> Result<Vec<CellInfo>, HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::CELLS);

            let docs = coll
                .scan()
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })?;

            let mut cells = Vec::new();
            for (id, data) in docs {
                if let Ok(json) = String::from_utf8(data) {
                    if let Ok(cell) = parse_cell_json(&id, &json) {
                        cells.push(cell);
                    }
                }
            }
            Ok(cells)
        })
    }

    /// Get a specific cell by ID
    pub fn get_cell(&self, cell_id: &str) -> Result<Option<CellInfo>, HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::CELLS);

            match coll.get(cell_id) {
                Ok(Some(data)) => {
                    let json = String::from_utf8(data).map_err(|e| HiveError::StorageError {
                        msg: format!("Invalid UTF-8: {}", e),
                    })?;
                    let cell = parse_cell_json(cell_id, &json)?;
                    Ok(Some(cell))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(HiveError::StorageError { msg: e.to_string() }),
            }
        })
    }

    /// Store a cell
    pub fn put_cell(&self, cell: CellInfo) -> Result<(), HiveError> {
        let json = serialize_cell_json(&cell)?;
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::CELLS);
            coll.upsert(&cell.id, json.into_bytes())
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })
        })
    }

    // -------------------------------------------------------------------------
    // Track Operations
    // -------------------------------------------------------------------------

    /// Get all tracks from the sync document
    pub fn get_tracks(&self) -> Result<Vec<TrackInfo>, HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::TRACKS);

            let docs = coll
                .scan()
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })?;

            let mut tracks = Vec::new();
            for (id, data) in docs {
                if let Ok(json) = String::from_utf8(data) {
                    if let Ok(track) = parse_track_json(&id, &json) {
                        tracks.push(track);
                    }
                }
            }
            Ok(tracks)
        })
    }

    /// Get a specific track by ID
    pub fn get_track(&self, track_id: &str) -> Result<Option<TrackInfo>, HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::TRACKS);

            match coll.get(track_id) {
                Ok(Some(data)) => {
                    let json = String::from_utf8(data).map_err(|e| HiveError::StorageError {
                        msg: format!("Invalid UTF-8: {}", e),
                    })?;
                    let track = parse_track_json(track_id, &json)?;
                    Ok(Some(track))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(HiveError::StorageError { msg: e.to_string() }),
            }
        })
    }

    /// Store a track
    pub fn put_track(&self, track: TrackInfo) -> Result<(), HiveError> {
        let json = serialize_track_json(&track)?;
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::TRACKS);
            coll.upsert(&track.id, json.into_bytes())
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })
        })
    }

    // -------------------------------------------------------------------------
    // Platform Operations
    // -------------------------------------------------------------------------

    /// Get all platforms from the sync document
    pub fn get_platforms(&self) -> Result<Vec<PlatformInfo>, HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::PLATFORMS);

            let docs = coll
                .scan()
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })?;

            let mut platforms = Vec::new();
            for (id, data) in docs {
                if let Ok(json) = String::from_utf8(data) {
                    if let Ok(platform) = parse_platform_json(&id, &json) {
                        platforms.push(platform);
                    }
                }
            }
            Ok(platforms)
        })
    }

    /// Store a platform
    pub fn put_platform(&self, platform: PlatformInfo) -> Result<(), HiveError> {
        let json = serialize_platform_json(&platform)?;
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::PLATFORMS);
            coll.upsert(&platform.id, json.into_bytes())
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })
        })
    }

    // -------------------------------------------------------------------------
    // Command Operations (C2)
    // -------------------------------------------------------------------------

    /// Get all pending commands
    pub fn get_commands(&self) -> Result<Vec<CommandInfo>, HiveError> {
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::COMMANDS);

            let docs = coll
                .scan()
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })?;

            let mut commands = Vec::new();
            for (id, data) in docs {
                if let Ok(json) = String::from_utf8(data) {
                    if let Ok(cmd) = parse_command_json(&id, &json) {
                        commands.push(cmd);
                    }
                }
            }
            Ok(commands)
        })
    }

    /// Store a command (for C2 issuance)
    pub fn put_command(&self, command: CommandInfo) -> Result<(), HiveError> {
        let json = serialize_command_json(&command)?;
        self.runtime.block_on(async {
            let backend = self.storage_backend.read().await;
            let coll = backend.collection(collections::COMMANDS);
            coll.upsert(&command.id, json.into_bytes())
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })
        })
    }
}

// =============================================================================
// JSON Serialization Helpers
// =============================================================================

fn parse_cell_json(id: &str, json: &str) -> Result<CellInfo, HiveError> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| HiveError::InvalidInput {
            msg: format!("Invalid JSON: {}", e),
        })?;

    Ok(CellInfo {
        id: id.to_string(),
        name: v["name"].as_str().unwrap_or(id).to_string(),
        status: CellStatus::from_str(v["status"].as_str().unwrap_or("OFFLINE")),
        platform_count: v["platform_count"].as_u64().unwrap_or(0) as u32,
        center_lat: v["center_lat"].as_f64().unwrap_or(0.0),
        center_lon: v["center_lon"].as_f64().unwrap_or(0.0),
        capabilities: v["capabilities"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        formation_id: v["formation_id"].as_str().map(|s| s.to_string()),
        leader_id: v["leader_id"].as_str().map(|s| s.to_string()),
        last_update: v["last_update"].as_i64().unwrap_or(0),
    })
}

fn serialize_cell_json(cell: &CellInfo) -> Result<String, HiveError> {
    let v = serde_json::json!({
        "name": cell.name,
        "status": cell.status.as_str(),
        "platform_count": cell.platform_count,
        "center_lat": cell.center_lat,
        "center_lon": cell.center_lon,
        "capabilities": cell.capabilities,
        "formation_id": cell.formation_id,
        "leader_id": cell.leader_id,
        "last_update": cell.last_update,
    });
    serde_json::to_string(&v).map_err(|e| HiveError::EncodingError { msg: e.to_string() })
}

fn parse_track_json(id: &str, json: &str) -> Result<TrackInfo, HiveError> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| HiveError::InvalidInput {
            msg: format!("Invalid JSON: {}", e),
        })?;

    Ok(TrackInfo {
        id: id.to_string(),
        source_platform: v["source_platform"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        cell_id: v["cell_id"].as_str().map(|s| s.to_string()),
        formation_id: v["formation_id"].as_str().map(|s| s.to_string()),
        lat: v["lat"].as_f64().unwrap_or(0.0),
        lon: v["lon"].as_f64().unwrap_or(0.0),
        hae: v["hae"].as_f64(),
        cep: v["cep"].as_f64(),
        heading: v["heading"].as_f64(),
        speed: v["speed"].as_f64(),
        classification: v["classification"]
            .as_str()
            .unwrap_or("a-u-G")
            .to_string(),
        confidence: v["confidence"].as_f64().unwrap_or(0.5),
        category: TrackCategory::from_str(v["category"].as_str().unwrap_or("UNKNOWN")),
        created_at: v["created_at"].as_i64().unwrap_or(0),
        last_update: v["last_update"].as_i64().unwrap_or(0),
    })
}

fn serialize_track_json(track: &TrackInfo) -> Result<String, HiveError> {
    let v = serde_json::json!({
        "source_platform": track.source_platform,
        "cell_id": track.cell_id,
        "formation_id": track.formation_id,
        "lat": track.lat,
        "lon": track.lon,
        "hae": track.hae,
        "cep": track.cep,
        "heading": track.heading,
        "speed": track.speed,
        "classification": track.classification,
        "confidence": track.confidence,
        "category": track.category.as_str(),
        "created_at": track.created_at,
        "last_update": track.last_update,
    });
    serde_json::to_string(&v).map_err(|e| HiveError::EncodingError { msg: e.to_string() })
}

fn parse_platform_json(id: &str, json: &str) -> Result<PlatformInfo, HiveError> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| HiveError::InvalidInput {
            msg: format!("Invalid JSON: {}", e),
        })?;

    Ok(PlatformInfo {
        id: id.to_string(),
        platform_type: v["platform_type"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        name: v["name"].as_str().unwrap_or(id).to_string(),
        status: PlatformStatus::from_str(v["status"].as_str().unwrap_or("OFFLINE")),
        lat: v["lat"].as_f64().unwrap_or(0.0),
        lon: v["lon"].as_f64().unwrap_or(0.0),
        hae: v["hae"].as_f64(),
        readiness: v["readiness"].as_f64().unwrap_or(0.0),
        capabilities: v["capabilities"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        cell_id: v["cell_id"].as_str().map(|s| s.to_string()),
        last_heartbeat: v["last_heartbeat"].as_i64().unwrap_or(0),
    })
}

fn serialize_platform_json(platform: &PlatformInfo) -> Result<String, HiveError> {
    let v = serde_json::json!({
        "platform_type": platform.platform_type,
        "name": platform.name,
        "status": platform.status.as_str(),
        "lat": platform.lat,
        "lon": platform.lon,
        "hae": platform.hae,
        "readiness": platform.readiness,
        "capabilities": platform.capabilities,
        "cell_id": platform.cell_id,
        "last_heartbeat": platform.last_heartbeat,
    });
    serde_json::to_string(&v).map_err(|e| HiveError::EncodingError { msg: e.to_string() })
}

fn parse_command_json(id: &str, json: &str) -> Result<CommandInfo, HiveError> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| HiveError::InvalidInput {
            msg: format!("Invalid JSON: {}", e),
        })?;

    Ok(CommandInfo {
        id: id.to_string(),
        command_type: v["command_type"]
            .as_str()
            .unwrap_or("UNKNOWN")
            .to_string(),
        target_id: v["target_id"].as_str().unwrap_or("").to_string(),
        parameters: v["parameters"].to_string(),
        priority: v["priority"].as_u64().unwrap_or(3) as u8,
        status: CommandStatus::from_str(v["status"].as_str().unwrap_or("PENDING")),
        originator: v["originator"].as_str().unwrap_or("").to_string(),
        created_at: v["created_at"].as_i64().unwrap_or(0),
        last_update: v["last_update"].as_i64().unwrap_or(0),
    })
}

fn serialize_command_json(command: &CommandInfo) -> Result<String, HiveError> {
    // Parse parameters as JSON or use empty object
    let params: serde_json::Value =
        serde_json::from_str(&command.parameters).unwrap_or(serde_json::json!({}));

    let v = serde_json::json!({
        "command_type": command.command_type,
        "target_id": command.target_id,
        "parameters": params,
        "priority": command.priority,
        "status": command.status.as_str(),
        "originator": command.originator,
        "created_at": command.created_at,
        "last_update": command.last_update,
    });
    serde_json::to_string(&v).map_err(|e| HiveError::EncodingError { msg: e.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hive_version() {
        let version = hive_version();
        assert!(!version.is_empty());
        assert!(version.contains('.'));
    }

    #[test]
    fn test_encode_track() {
        let track = TrackData {
            track_id: "track-001".to_string(),
            source_platform: "platform-1".to_string(),
            position: Position {
                lat: 34.0522,
                lon: -118.2437,
                hae: Some(100.0),
            },
            velocity: Some(Velocity {
                bearing: 90.0,
                speed_mps: 10.0,
            }),
            classification: "a-f-G-U-C".to_string(),
            confidence: 0.95,
            cell_id: Some("cell-1".to_string()),
            formation_id: None,
        };

        let result = encode_track_to_cot(track);
        assert!(result.is_ok());

        let xml = result.unwrap();
        assert!(xml.contains("<event"));
        assert!(xml.contains("track-001"));
    }

    #[test]
    fn test_encode_minimal_track() {
        let track = TrackData {
            track_id: "t1".to_string(),
            source_platform: "p1".to_string(),
            position: Position {
                lat: 0.0,
                lon: 0.0,
                hae: None,
            },
            velocity: None,
            classification: "a-u-G".to_string(),
            confidence: 0.5,
            cell_id: None,
            formation_id: None,
        };

        let result = encode_track_to_cot(track);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_track_id() {
        let track = TrackData {
            track_id: "".to_string(), // Empty - should fail
            source_platform: "p1".to_string(),
            position: Position {
                lat: 0.0,
                lon: 0.0,
                hae: None,
            },
            velocity: None,
            classification: "a-u-G".to_string(),
            confidence: 0.5,
            cell_id: None,
            formation_id: None,
        };

        let result = encode_track_to_cot(track);
        assert!(result.is_err());
    }

    #[test]
    fn test_helper_functions() {
        let pos = create_position(34.0, -118.0, Some(50.0));
        assert_eq!(pos.lat, 34.0);
        assert_eq!(pos.lon, -118.0);
        assert_eq!(pos.hae, Some(50.0));

        let vel = create_velocity(45.0, 15.0);
        assert_eq!(vel.bearing, 45.0);
        assert_eq!(vel.speed_mps, 15.0);
    }
}

// =============================================================================
// JNI Bindings - Direct Android native method support
// =============================================================================
//
// These functions provide a direct JNI interface that bypasses JNA's symbol
// lookup issues on Android. When System.loadLibrary() is called, these
// functions are registered via JNI's standard naming convention.
//
// Usage in Kotlin:
// ```kotlin
// class HiveJni {
//     companion object {
//         init {
//             System.loadLibrary("hive_ffi")
//         }
//     }
//     external fun hiveVersion(): String
//     external fun testJni(): String
// }
// ```

/// JNI: Get HIVE library version
///
/// Kotlin signature: external fun hiveVersion(): String
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_hiveVersion(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let version = hive_version();
    env.new_string(&version)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Test that JNI bindings work
///
/// Kotlin signature: external fun testJni(): String
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_testJni(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let msg = "JNI bindings working! HIVE FFI loaded successfully.";
    env.new_string(msg)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Create a HIVE node (simplified for testing)
///
/// Kotlin signature: external fun createNodeJni(appId: String, sharedKey: String, storagePath: String): Long
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_createNodeJni(
    mut env: JNIEnv,
    _class: JClass,
    app_id: JString,
    shared_key: JString,
    storage_path: JString,
) -> i64 {
    let app_id: String = match env.get_string(&app_id) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };
    let shared_key: String = match env.get_string(&shared_key) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };
    let storage_path: String = match env.get_string(&storage_path) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    #[cfg(target_os = "android")]
    android_log(&format!(
        "createNodeJni: app_id={}, storage_path={}",
        app_id, storage_path
    ));

    let config = NodeConfig {
        app_id,
        shared_key,
        bind_address: None,
        storage_path,
    };

    match create_node(config) {
        Ok(node) => {
            #[cfg(target_os = "android")]
            android_log("createNodeJni: Node created successfully");
            // Return the Arc pointer as a handle
            Arc::into_raw(node) as i64
        }
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!("createNodeJni: Error creating node: {:?}", e));
            0
        }
    }
}

/// JNI: Get node ID from a HiveNode handle
///
/// Kotlin signature: external fun nodeIdJni(handle: Long): String
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_nodeIdJni(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    if handle == 0 {
        return env
            .new_string("")
            .expect("Failed to create Java string")
            .into_raw();
    }

    let node = unsafe { Arc::from_raw(handle as *const HiveNode) };
    let node_id = node.node_id();

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    env.new_string(&node_id)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Get peer count from a HiveNode handle
///
/// Kotlin signature: external fun peerCountJni(handle: Long): Int
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_peerCountJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> i32 {
    if handle == 0 {
        return 0;
    }

    let node = unsafe { Arc::from_raw(handle as *const HiveNode) };
    let count = node.peer_count() as i32;

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    count
}

/// JNI: Get connected peer IDs as a JSON array
///
/// Kotlin signature: external fun connectedPeersJni(handle: Long): String
/// Returns JSON array of hex-encoded peer IDs, or "[]" on error
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_connectedPeersJni(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    if handle == 0 {
        return env
            .new_string("[]")
            .expect("Failed to create Java string")
            .into_raw();
    }

    let node = unsafe { Arc::from_raw(handle as *const HiveNode) };
    let peers = node.connected_peers();
    let result = serde_json::to_string(&peers).unwrap_or_else(|_| "[]".to_string());

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    env.new_string(&result)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Start sync on a HiveNode
///
/// Kotlin signature: external fun startSyncJni(handle: Long): Boolean
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_startSyncJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> bool {
    if handle == 0 {
        return false;
    }

    let node = unsafe { Arc::from_raw(handle as *const HiveNode) };
    let result = node.start_sync().is_ok();

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    result
}

/// JNI: Free a HiveNode handle
///
/// Kotlin signature: external fun freeNodeJni(handle: Long)
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_freeNodeJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    if handle != 0 {
        #[cfg(target_os = "android")]
        android_log(&format!("freeNodeJni: Freeing node handle {}", handle));

        // Reconstruct the Arc to drop it
        let node = unsafe { Arc::from_raw(handle as *const HiveNode) };

        // Signal the cleanup task to stop
        node.cleanup_running.store(false, Ordering::SeqCst);

        #[cfg(target_os = "android")]
        android_log("freeNodeJni: Signaled cleanup task to stop");

        // Give the background task a moment to exit
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Drop the node - this should release the database
        drop(node);

        #[cfg(target_os = "android")]
        android_log("freeNodeJni: Node dropped");
    }
}

/// JNI: Get all cells as JSON array string
///
/// Kotlin signature: external fun getCellsJni(handle: Long): String
/// Returns JSON array of cell objects, or "[]" on error
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_getCellsJni(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    if handle == 0 {
        return env
            .new_string("[]")
            .expect("Failed to create Java string")
            .into_raw();
    }

    let node = unsafe { Arc::from_raw(handle as *const HiveNode) };
    let result = match node.get_cells() {
        Ok(cells) => {
            let json_array: Vec<serde_json::Value> = cells
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "name": c.name,
                        "status": c.status.as_str(),
                        "platform_count": c.platform_count,
                        "center_lat": c.center_lat,
                        "center_lon": c.center_lon,
                        "capabilities": c.capabilities,
                        "formation_id": c.formation_id,
                        "leader_id": c.leader_id,
                        "last_update": c.last_update,
                    })
                })
                .collect();
            serde_json::to_string(&json_array).unwrap_or_else(|_| "[]".to_string())
        }
        Err(_) => "[]".to_string(),
    };

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    env.new_string(&result)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Get all tracks as JSON array string
///
/// Kotlin signature: external fun getTracksJni(handle: Long): String
/// Returns JSON array of track objects, or "[]" on error
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_getTracksJni(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    if handle == 0 {
        return env
            .new_string("[]")
            .expect("Failed to create Java string")
            .into_raw();
    }

    let node = unsafe { Arc::from_raw(handle as *const HiveNode) };
    let result = match node.get_tracks() {
        Ok(tracks) => {
            let json_array: Vec<serde_json::Value> = tracks
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "source_platform": t.source_platform,
                        "cell_id": t.cell_id,
                        "formation_id": t.formation_id,
                        "lat": t.lat,
                        "lon": t.lon,
                        "hae": t.hae,
                        "cep": t.cep,
                        "heading": t.heading,
                        "speed": t.speed,
                        "classification": t.classification,
                        "confidence": t.confidence,
                        "category": t.category.as_str(),
                        "created_at": t.created_at,
                        "last_update": t.last_update,
                    })
                })
                .collect();
            serde_json::to_string(&json_array).unwrap_or_else(|_| "[]".to_string())
        }
        Err(_) => "[]".to_string(),
    };

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    env.new_string(&result)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Get all platforms as JSON array string
///
/// Kotlin signature: external fun getPlatformsJni(handle: Long): String
/// Returns JSON array of platform objects, or "[]" on error
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_getPlatformsJni(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    if handle == 0 {
        return env
            .new_string("[]")
            .expect("Failed to create Java string")
            .into_raw();
    }

    let node = unsafe { Arc::from_raw(handle as *const HiveNode) };
    let result = match node.get_platforms() {
        Ok(platforms) => {
            let json_array: Vec<serde_json::Value> = platforms
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "platform_type": p.platform_type,
                        "name": p.name,
                        "status": p.status.as_str(),
                        "lat": p.lat,
                        "lon": p.lon,
                        "hae": p.hae,
                        "readiness": p.readiness,
                        "capabilities": p.capabilities,
                        "cell_id": p.cell_id,
                        "last_heartbeat": p.last_heartbeat,
                    })
                })
                .collect();
            serde_json::to_string(&json_array).unwrap_or_else(|_| "[]".to_string())
        }
        Err(_) => "[]".to_string(),
    };

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    env.new_string(&result)
        .expect("Failed to create Java string")
        .into_raw()
}

// =============================================================================
// JNI Native Method Registration
// =============================================================================
//
// Android's linker namespace isolation prevents normal JNI symbol lookup.
// We provide a nativeInit function that Kotlin must call after System.load()
// to explicitly register the native methods.

/// Register native methods for HiveJni class
///
/// This must be called from Kotlin after System.load() to register native methods.
/// Android's classloader isolation prevents JNI_OnLoad from finding the class.
///
/// Kotlin usage:
/// ```kotlin
/// companion object {
///     init {
///         System.load(libPath)
///         nativeInit()
///     }
///     @JvmStatic external fun nativeInit()
/// }
/// ```
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_atak_hive_HiveJni_nativeInit(
    mut env: JNIEnv,
    class: JClass,
) {
    use jni::NativeMethod;

    let methods: Vec<NativeMethod> = vec![
        NativeMethod {
            name: "hiveVersion".into(),
            sig: "()Ljava/lang/String;".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_hiveVersion as *mut c_void,
        },
        NativeMethod {
            name: "testJni".into(),
            sig: "()Ljava/lang/String;".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_testJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "createNodeJni".into(),
            sig: "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)J".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_createNodeJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "nodeIdJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_nodeIdJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "peerCountJni".into(),
            sig: "(J)I".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_peerCountJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "connectedPeersJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_connectedPeersJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "startSyncJni".into(),
            sig: "(J)Z".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_startSyncJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "freeNodeJni".into(),
            sig: "(J)V".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_freeNodeJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "getCellsJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_getCellsJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "getTracksJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_getTracksJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "getPlatformsJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_getPlatformsJni as *mut c_void,
        },
    ];

    // Register native methods - the class is passed in from Kotlin so it's valid
    if let Err(_e) = env.register_native_methods(&class, &methods) {
        // Log error but don't crash - caller will see methods not registered
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
}

/// JNI_OnLoad - Called when library is loaded via System.loadLibrary()
///
/// This is our chance to register native methods while we have access to
/// the JNI environment from inside the library's linker namespace.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn JNI_OnLoad(vm: *mut JavaVM, _reserved: *mut c_void) -> jint {
    // Log that we're being called
    #[cfg(target_os = "android")]
    {
        android_log("JNI_OnLoad called for hive_ffi");
    }

    // Store JavaVM globally for callbacks from any thread
    let java_vm = unsafe {
        match jni::JavaVM::from_raw(vm) {
            Ok(jvm) => jvm,
            Err(_) => {
                #[cfg(target_os = "android")]
                android_log("JNI_OnLoad: Failed to create JavaVM from raw pointer");
                return jni::sys::JNI_ERR;
            }
        }
    };
    *JAVA_VM.lock().unwrap() = Some(java_vm);

    // Get JNIEnv from JavaVM
    let mut env = unsafe {
        let mut env_ptr: *mut jni::sys::JNIEnv = std::ptr::null_mut();
        let get_env_result = (**vm).GetEnv.unwrap()(
            vm,
            &mut env_ptr as *mut _ as *mut *mut c_void,
            JNI_VERSION_1_6 as i32,
        );
        if get_env_result != jni::sys::JNI_OK as i32 {
            #[cfg(target_os = "android")]
            android_log("JNI_OnLoad: GetEnv failed");
            return jni::sys::JNI_ERR;
        }
        match JNIEnv::from_raw(env_ptr) {
            Ok(env) => env,
            Err(_) => {
                #[cfg(target_os = "android")]
                android_log("JNI_OnLoad: JNIEnv::from_raw failed");
                return jni::sys::JNI_ERR;
            }
        }
    };

    // Try to find PeerEventManager class and store global reference for callbacks
    let peer_event_manager_class = "com/revolveteam/atak/hive/PeerEventManager";
    match env.find_class(peer_event_manager_class) {
        Ok(class) => {
            match env.new_global_ref(class) {
                Ok(global_ref) => {
                    *PEER_EVENT_MANAGER_CLASS.lock().unwrap() = Some(global_ref);
                    #[cfg(target_os = "android")]
                    android_log("JNI_OnLoad: PeerEventManager class found and cached");
                }
                Err(_) => {
                    #[cfg(target_os = "android")]
                    android_log("JNI_OnLoad: Failed to create global ref for PeerEventManager");
                }
            }
        }
        Err(_) => {
            #[cfg(target_os = "android")]
            android_log("JNI_OnLoad: PeerEventManager class not found (OK if loading before class init)");
        }
    }

    #[cfg(target_os = "android")]
    android_log("JNI_OnLoad: Got JNIEnv, looking for HiveJni class...");

    // Try to find the HiveJni class and register natives
    let class_name = "com/revolveteam/atak/hive/HiveJni";
    match env.find_class(class_name) {
        Ok(class) => {
            #[cfg(target_os = "android")]
            android_log("JNI_OnLoad: Found HiveJni class, registering natives...");

            // Register native methods
            use jni::NativeMethod;
            let methods: Vec<NativeMethod> = vec![
                NativeMethod {
                    name: "nativeInit".into(),
                    sig: "()V".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_nativeInit as *mut c_void,
                },
                NativeMethod {
                    name: "hiveVersion".into(),
                    sig: "()Ljava/lang/String;".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_hiveVersion as *mut c_void,
                },
                NativeMethod {
                    name: "testJni".into(),
                    sig: "()Ljava/lang/String;".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_testJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "createNodeJni".into(),
                    sig: "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)J".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_createNodeJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "nodeIdJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_nodeIdJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "peerCountJni".into(),
                    sig: "(J)I".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_peerCountJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "connectedPeersJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_connectedPeersJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "startSyncJni".into(),
                    sig: "(J)Z".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_startSyncJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "freeNodeJni".into(),
                    sig: "(J)V".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_freeNodeJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "getCellsJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_getCellsJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "getTracksJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_getTracksJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "getPlatformsJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_revolveteam_atak_hive_HiveJni_getPlatformsJni as *mut c_void,
                },
            ];

            match env.register_native_methods(&class, &methods) {
                Ok(_) => {
                    #[cfg(target_os = "android")]
                    android_log("JNI_OnLoad: Native methods registered successfully!");
                }
                Err(_) => {
                    #[cfg(target_os = "android")]
                    android_log("JNI_OnLoad: Failed to register native methods");
                    let _ = env.exception_describe();
                    let _ = env.exception_clear();
                }
            }
        }
        Err(_) => {
            #[cfg(target_os = "android")]
            android_log(
                "JNI_OnLoad: HiveJni class not found (this is OK if loading before class init)",
            );
            // Class not loaded yet - this is OK, nativeInit will be called later
        }
    }

    JNI_VERSION_1_6
}

/// Log to Android logcat
#[cfg(target_os = "android")]
fn android_log(msg: &str) {
    use std::ffi::CString;
    use std::os::raw::c_char;

    let tag = CString::new("HiveFFI").unwrap();
    let msg = CString::new(msg).unwrap();

    unsafe {
        // Android log priority INFO = 4
        extern "C" {
            fn __android_log_write(prio: i32, tag: *const c_char, text: *const c_char) -> i32;
        }
        __android_log_write(4, tag.as_ptr(), msg.as_ptr());
    }
}

/// Notify Java PeerEventManager of a peer connected event
#[cfg(feature = "sync")]
fn notify_peer_connected(peer_id: &str) {
    notify_peer_event("notifyPeerConnected", peer_id, None);
}

/// Notify Java PeerEventManager of a peer disconnected event
#[cfg(feature = "sync")]
fn notify_peer_disconnected(peer_id: &str, reason: &str) {
    notify_peer_event("notifyPeerDisconnected", peer_id, Some(reason));
}

/// Helper to call PeerEventManager static methods
#[cfg(feature = "sync")]
fn notify_peer_event(method_name: &str, peer_id: &str, reason: Option<&str>) {
    let java_vm_guard = JAVA_VM.lock().unwrap();
    let java_vm = match java_vm_guard.as_ref() {
        Some(vm) => vm,
        None => {
            #[cfg(target_os = "android")]
            android_log("notify_peer_event: No JavaVM available");
            return;
        }
    };

    // Check if we already have the class cached
    let mut class_guard = PEER_EVENT_MANAGER_CLASS.lock().unwrap();

    // If not cached, try to find it now (lazy loading)
    if class_guard.is_none() {
        #[cfg(target_os = "android")]
        android_log("notify_peer_event: PeerEventManager class not cached, trying to find it...");

        // Attach current thread to get env for class lookup
        if let Ok(mut env) = java_vm.attach_current_thread() {
            let peer_event_manager_class = "com/revolveteam/atak/hive/PeerEventManager";
            if let Ok(class) = env.find_class(peer_event_manager_class) {
                if let Ok(global_ref) = env.new_global_ref(class) {
                    *class_guard = Some(global_ref);
                    #[cfg(target_os = "android")]
                    android_log("notify_peer_event: PeerEventManager class found and cached!");
                }
            } else {
                #[cfg(target_os = "android")]
                android_log("notify_peer_event: PeerEventManager class not found");
            }
        }
    }

    let class_ref = match class_guard.as_ref() {
        Some(c) => c,
        None => {
            #[cfg(target_os = "android")]
            android_log("notify_peer_event: PeerEventManager class not available");
            return;
        }
    };

    // Attach current thread to JVM
    let mut env = match java_vm.attach_current_thread() {
        Ok(env) => env,
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!("notify_peer_event: Failed to attach thread: {:?}", e));
            return;
        }
    };

    // Create Java string for peer_id
    let peer_id_jstring = match env.new_string(peer_id) {
        Ok(s) => s,
        Err(_) => {
            #[cfg(target_os = "android")]
            android_log("notify_peer_event: Failed to create peer_id string");
            return;
        }
    };

    // Call the appropriate method
    let result = if let Some(reason) = reason {
        // notifyPeerDisconnected(String peerId, String reason)
        let reason_jstring = match env.new_string(reason) {
            Ok(s) => s,
            Err(_) => {
                #[cfg(target_os = "android")]
                android_log("notify_peer_event: Failed to create reason string");
                return;
            }
        };
        env.call_static_method(
            class_ref,
            method_name,
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[
                JValue::Object(&peer_id_jstring),
                JValue::Object(&reason_jstring),
            ],
        )
    } else {
        // notifyPeerConnected(String peerId)
        env.call_static_method(
            class_ref,
            method_name,
            "(Ljava/lang/String;)V",
            &[JValue::Object(&peer_id_jstring)],
        )
    };

    if let Err(e) = result {
        #[cfg(target_os = "android")]
        android_log(&format!("notify_peer_event: Method call failed: {:?}", e));
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    } else {
        #[cfg(target_os = "android")]
        android_log(&format!("notify_peer_event: {} called for {}", method_name, peer_id));
    }
}
