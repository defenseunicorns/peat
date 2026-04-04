//! Peat FFI - Foreign Function Interface for Kotlin/Swift
//!
//! This crate provides UniFFI bindings to expose Peat functionality
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

// Allow pre-existing warnings in FFI code - will clean up incrementally
#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(dead_code)]
#![allow(clippy::incompatible_msrv)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::single_match)]
#![allow(clippy::items_after_test_module)]

use std::collections::HashMap;
use std::sync::Arc;

// JNI support for Android
use jni::objects::{GlobalRef, JClass, JString, JValue};
use jni::sys::{jboolean, jint, jstring, JavaVM, JNI_VERSION_1_6};
use jni::JNIEnv;
use std::os::raw::c_void;
use std::sync::{LazyLock, Mutex};

// Global JavaVM reference for JNI callbacks from any thread
static JAVA_VM: LazyLock<Mutex<Option<jni::JavaVM>>> = LazyLock::new(|| Mutex::new(None));

// Global reference to PeerEventManager class
static PEER_EVENT_MANAGER_CLASS: LazyLock<Mutex<Option<GlobalRef>>> =
    LazyLock::new(|| Mutex::new(None));

// Global Peat node handle that survives APK replacement
// This allows Kotlin code to recover the node connection after plugin hot-swap
#[cfg(feature = "sync")]
static GLOBAL_NODE_HANDLE: LazyLock<Mutex<i64>> = LazyLock::new(|| Mutex::new(0));

// Global BLE transport reference for Android JNI access
// Kotlin signals BLE state (started/stopped, peer discovery) into this transport
// which makes TransportManager aware of BLE availability for PACE routing.
#[cfg(all(feature = "bluetooth", target_os = "android"))]
static ANDROID_BLE_TRANSPORT: LazyLock<
    Mutex<Option<Arc<PeatBleTransport<peat_btle::platform::android::AndroidAdapter>>>>,
> = LazyLock::new(|| Mutex::new(None));

use peat_protocol::cot::{
    CotEncoder, Position as CotPosition, TrackUpdate, Velocity as CotVelocity,
};

#[cfg(feature = "sync")]
use peat_protocol::network::PeerConfig;
#[cfg(feature = "sync")]
use peat_protocol::network::{IrohTransport, PeerInfo as PeatPeerInfo, TransportPeerEvent};
#[cfg(feature = "sync")]
use peat_protocol::storage::{AutomergeBackend, AutomergeStore, StorageBackend, SyncCapable};
#[cfg(feature = "sync")]
use peat_protocol::sync::automerge::AutomergeIrohBackend;
#[cfg(feature = "sync")]
use peat_protocol::sync::{BackendConfig, DataSyncBackend, TransportConfig};
#[cfg(all(feature = "sync", feature = "bluetooth"))]
use peat_protocol::transport::btle::PeatBleTransport;
#[cfg(feature = "sync")]
use peat_protocol::transport::{
    iroh::IrohMeshTransport, CollectionRouteTable, Transport, TransportCapabilities,
    TransportInstance, TransportManager, TransportManagerConfig, TransportPolicy, TransportType,
};
#[cfg(feature = "sync")]
use std::net::SocketAddr;
#[cfg(feature = "sync")]
use std::path::PathBuf;
#[cfg(feature = "sync")]
use std::sync::atomic::{AtomicBool, Ordering};

// Setup UniFFI scaffolding
uniffi::setup_scaffolding!();

/// Get the Peat library version
#[uniffi::export]
pub fn peat_version() -> String {
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
pub enum PeatError {
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
pub fn encode_track_to_cot(track: TrackData) -> Result<String, PeatError> {
    // Validate input
    if track.track_id.is_empty() {
        return Err(PeatError::InvalidInput {
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
        source_model: "peat-ffi".to_string(),
        model_version: peat_version(),
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
        .map_err(|e| PeatError::EncodingError { msg: e.to_string() })?;

    event
        .to_xml()
        .map_err(|e| PeatError::EncodingError { msg: e.to_string() })
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
// PeatNode - P2P Sync Support (requires "sync" feature)
// =============================================================================

/// Transport configuration for BLE and other transports (ADR-039, #556)
///
/// Controls which transports are enabled and their settings.
/// Used by `NodeConfig` to configure multi-transport support.
#[cfg(feature = "sync")]
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct TransportConfigFFI {
    /// Enable Bluetooth LE transport (requires "bluetooth" feature)
    /// When enabled, BLE mesh networking is available alongside Iroh/QUIC
    pub enable_ble: bool,
    /// BLE mesh ID (optional, defaults to app_id if not specified)
    /// Used to identify the BLE mesh network for peer discovery
    pub ble_mesh_id: Option<String>,
    /// BLE power profile: "aggressive", "balanced", or "low_power"
    /// - aggressive: Maximum range/speed, higher battery usage
    /// - balanced: Default, moderate power usage
    /// - low_power: Minimal battery impact, reduced range/speed
    pub ble_power_profile: Option<String>,
    /// Transport preference order (optional)
    /// List of transport names in order of preference, e.g., ["iroh", "ble", "lora"]
    /// Used by TransportManager's PACE policy for transport selection
    pub transport_preference: Option<Vec<String>>,
    /// Per-collection transport routing (optional)
    /// JSON-encoded CollectionRouteTable for explicit collection->transport bindings.
    /// Collections not listed fall through to PACE/legacy scoring.
    pub collection_routes_json: Option<String>,
}

/// Configuration for creating a PeatNode
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
    /// Transport configuration (optional, defaults to Iroh-only)
    /// Use this to enable BLE and configure multi-transport behavior
    pub transport: Option<TransportConfigFFI>,
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

/// A Peat network node with P2P sync capabilities
///
/// Wraps AutomergeIrohBackend for authenticated document sync.
/// Requires matching app_id and shared_key for peer connections.
#[cfg(feature = "sync")]
#[derive(uniffi::Object)]
pub struct PeatNode {
    /// The sync backend with FormationKey authentication
    sync_backend: Arc<AutomergeIrohBackend>,
    /// Storage backend for document operations (shared with sync_backend)
    /// Note: This is the SAME backend instance used by sync_backend to ensure
    /// sync coordinator state is shared. Do NOT create a separate backend.
    storage_backend: Arc<AutomergeBackend>,
    /// Transport manager for multi-transport coordination (ADR-032)
    /// Enables PACE policy-based transport selection and future BLE integration
    transport_manager: TransportManager,
    /// Direct reference to Iroh transport for backward-compatible methods
    /// (peer_count, connected_peers, etc.)
    iroh_transport: Arc<IrohTransport>,
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
impl PeatNode {
    /// Get this node's unique identifier (hex-encoded)
    pub fn node_id(&self) -> String {
        hex::encode(self.iroh_transport.endpoint_id().as_bytes())
    }

    /// Get this node's endpoint address for peer connections
    pub fn endpoint_addr(&self) -> String {
        format!("{:?}", self.iroh_transport.endpoint_addr())
    }

    /// Get the number of connected peers
    pub fn peer_count(&self) -> u32 {
        self.iroh_transport.peer_count() as u32
    }

    /// Get list of connected peer IDs
    pub fn connected_peers(&self) -> Vec<String> {
        self.iroh_transport
            .connected_peers()
            .iter()
            .map(|id| hex::encode(id.as_bytes()))
            .collect()
    }

    /// Start sync operations
    ///
    /// The authenticated accept loop (with formation handshake) is already running
    /// from sync_backend.initialize() in create_node(). This method starts the
    /// sync coordination layer: event-based and polling-based sync handlers.
    pub fn start_sync(&self) -> Result<(), PeatError> {
        #[cfg(target_os = "android")]
        android_log("start_sync: called");

        // IMPORTANT: Use runtime.enter() to ensure tokio::spawn() inside start_sync()
        // can find the runtime context. block_on() alone doesn't guarantee this on
        // all platforms (especially Android where the JNI thread may not have proper
        // thread-local storage for the Tokio runtime handle).
        let _guard = self.runtime.enter();

        #[cfg(target_os = "android")]
        android_log("start_sync: runtime entered");

        // Must run inside Tokio runtime because start_sync() calls tokio::spawn()
        let result = self.runtime.block_on(async {
            #[cfg(target_os = "android")]
            android_log("start_sync: inside block_on");

            // CRITICAL: Call start_sync() on the ACTUAL storage_backend instance,
            // NOT on sync_backend.sync_engine() which returns a CLONED instance
            // that doesn't have the transport event subscriptions set up!
            //
            // Note: The authenticated accept loop (with formation handshake and
            // Connected event emission) is already running — it was started by
            // sync_backend.initialize() in create_node(). The storage_backend's
            // start_sync() will see the accept loop as already running and skip
            // starting the plain (unauthenticated) accept loop.
            self.storage_backend
                .start_sync()
                .map_err(|e| PeatError::SyncError { msg: e.to_string() })
        });

        #[cfg(target_os = "android")]
        match &result {
            Ok(_) => android_log("start_sync: SUCCESS - sync handlers spawned"),
            Err(e) => android_log(&format!("start_sync: FAILED - {}", e)),
        }

        result
    }

    /// Stop sync operations
    pub fn stop_sync(&self) -> Result<(), PeatError> {
        // Must run inside Tokio runtime for consistency with start_sync()
        self.runtime.block_on(async {
            // Call stop_sync() on the ACTUAL storage_backend instance
            self.storage_backend
                .stop_sync()
                .map_err(|e| PeatError::SyncError { msg: e.to_string() })
        })
    }

    /// Get sync statistics
    pub fn sync_stats(&self) -> Result<SyncStats, PeatError> {
        let stats = self
            .storage_backend
            .sync_stats()
            .map_err(|e| PeatError::SyncError { msg: e.to_string() })?;

        Ok(SyncStats {
            sync_active: stats.peer_count > 0, // Infer from peer count
            connected_peers: self.iroh_transport.peer_count() as u32,
            bytes_sent: stats.bytes_sent,
            bytes_received: stats.bytes_received,
        })
    }

    /// Request a full document sync with all connected peers.
    /// This pushes all local documents to each peer and pulls any documents they have.
    /// Useful for ensuring newly created documents propagate after the initial connection.
    pub fn request_sync(&self) -> Result<(), PeatError> {
        if let Some(coordinator) = self.storage_backend.sync_coordinator() {
            let peers = self.iroh_transport.connected_peers();
            let coord = Arc::clone(coordinator);
            self.runtime.block_on(async {
                for peer_id in peers {
                    if let Err(e) = coord.sync_all_documents_with_peer(peer_id).await {
                        #[cfg(target_os = "android")]
                        android_log(&format!("request_sync failed for peer: {}", e));
                    }
                }
            });
        }
        Ok(())
    }

    /// Connect to a peer node with formation handshake
    ///
    /// Establishes a QUIC connection, performs formation-key authentication,
    /// and emits a Connected event to trigger immediate sync handler spawning.
    pub fn connect_peer(&self, peer: PeerInfo) -> Result<(), PeatError> {
        let peat_peer = PeatPeerInfo {
            name: peer.name,
            node_id: peer.node_id,
            addresses: peer.addresses,
            relay_url: peer.relay_url,
        };

        let _guard = self.runtime.enter();

        self.runtime.block_on(async {
            let conn_opt = self
                .iroh_transport
                .connect_peer(&peat_peer)
                .await
                .map_err(|e| PeatError::ConnectionError { msg: e.to_string() })?;

            // If we got a new connection, perform formation handshake and emit Connected
            if let Some(conn) = conn_opt {
                let peer_id = conn.remote_id();

                if let Some(formation_key) = self.sync_backend.formation_key() {
                    use peat_protocol::network::perform_initiator_handshake;
                    match perform_initiator_handshake(&conn, &formation_key).await {
                        Ok(()) => {
                            // Emit Connected to trigger immediate sync handler spawning
                            self.iroh_transport.emit_peer_connected(peer_id);

                            // Explicitly trigger document sync with the new peer.
                            // The event-based sync handler spawner should handle this,
                            // but we also trigger sync directly to ensure documents flow.
                            if let Some(coordinator) = self.storage_backend.sync_coordinator() {
                                let coord = Arc::clone(coordinator);
                                let sync_peer = peer_id;
                                tokio::spawn(async move {
                                    // Brief delay for connection to stabilize
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                    #[cfg(target_os = "android")]
                                    android_log(&format!("Triggering sync_all_documents_with_peer for {:?}", sync_peer));
                                    match coord.sync_all_documents_with_peer(sync_peer).await {
                                        Ok(()) => {
                                            #[cfg(target_os = "android")]
                                            android_log("sync_all_documents_with_peer: SUCCESS");
                                        }
                                        Err(e) => {
                                            #[cfg(target_os = "android")]
                                            android_log(&format!("sync_all_documents_with_peer: FAILED - {}", e));
                                        }
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            conn.close(1u32.into(), b"authentication failed");
                            self.iroh_transport.disconnect(&peer_id).ok();
                            return Err(PeatError::ConnectionError {
                                msg: format!("Formation handshake failed: {}", e),
                            });
                        }
                    }
                } else {
                    // No formation key — emit Connected without handshake (backward compat)
                    self.iroh_transport.emit_peer_connected(peer_id);
                }
            }
            // If None, accept path is handling the connection

            Ok(())
        })
    }

    /// Disconnect from a peer by node ID
    ///
    /// Note: Currently disconnects matching peer from internal connection map.
    pub fn disconnect_peer(&self, node_id: &str) -> Result<(), PeatError> {
        // Find the matching endpoint ID from connected peers
        let connected = self.iroh_transport.connected_peers();
        for endpoint_id in connected {
            if hex::encode(endpoint_id.as_bytes()) == node_id {
                return self
                    .iroh_transport
                    .disconnect(&endpoint_id)
                    .map_err(|e| PeatError::ConnectionError { msg: e.to_string() });
            }
        }

        Err(PeatError::ConnectionError {
            msg: format!("Peer {} not found in connected peers", node_id),
        })
    }

    /// Store a JSON document in a collection
    pub fn put_document(
        &self,
        collection: &str,
        doc_id: &str,
        json_data: &str,
    ) -> Result<(), PeatError> {
        // Parse JSON to validate it
        let _: serde_json::Value =
            serde_json::from_str(json_data).map_err(|e| PeatError::InvalidInput {
                msg: format!("Invalid JSON: {}", e),
            })?;

        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collection);

            coll.upsert(doc_id, json_data.as_bytes().to_vec())
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })
        })
    }

    /// Retrieve a document from a collection as JSON
    pub fn get_document(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> Result<Option<String>, PeatError> {
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collection);

            match coll.get(doc_id) {
                Ok(Some(bytes)) => {
                    let json = String::from_utf8(bytes).map_err(|e| PeatError::StorageError {
                        msg: format!("Invalid UTF-8: {}", e),
                    })?;
                    Ok(Some(json))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(PeatError::StorageError { msg: e.to_string() }),
            }
        })
    }

    /// Delete a document from a collection
    pub fn delete_document(&self, collection: &str, doc_id: &str) -> Result<(), PeatError> {
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collection);

            coll.delete(doc_id)
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })
        })
    }

    /// List all document IDs in a collection
    pub fn list_documents(&self, collection: &str) -> Result<Vec<String>, PeatError> {
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collection);

            let docs = coll
                .scan()
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })?;

            Ok(docs.into_iter().map(|(id, _)| id).collect())
        })
    }

    /// Manually trigger sync for a specific document
    pub fn sync_document(&self, collection: &str, doc_id: &str) -> Result<(), PeatError> {
        let doc_key = format!("{}:{}", collection, doc_id);

        self.runtime.block_on(async {
            let backend = &self.storage_backend;

            backend
                .sync_document(&doc_key)
                .await
                .map_err(|e| PeatError::SyncError { msg: e.to_string() })
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
    ) -> Result<Arc<SubscriptionHandle>, PeatError> {
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

/// Create a new PeatNode with FormationKey authentication
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
pub fn create_node(config: NodeConfig) -> Result<Arc<PeatNode>, PeatError> {
    use std::time::Instant;
    let total_start = Instant::now();

    // Validate credentials
    if config.app_id.is_empty() {
        return Err(PeatError::InvalidInput {
            msg: "app_id cannot be empty".to_string(),
        });
    }
    if config.shared_key.is_empty() {
        return Err(PeatError::InvalidInput {
            msg: "shared_key cannot be empty".to_string(),
        });
    }

    // TIMING: Create runtime
    let phase_start = Instant::now();

    // Create a dedicated Tokio runtime for this node
    // Use 4 worker threads to avoid starving BLE D-Bus tasks when Iroh
    // background tasks (discovery, relay, pkarr) are running concurrently.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .map_err(|e| PeatError::SyncError {
            msg: format!("Failed to create runtime: {}", e),
        })?;

    let runtime_ms = phase_start.elapsed().as_millis();
    #[cfg(target_os = "android")]
    android_log(&format!("[TIMING] Runtime creation: {}ms", runtime_ms));
    #[cfg(not(target_os = "android"))]
    eprintln!("[Peat TIMING] Runtime creation: {}ms", runtime_ms);

    // Parse bind address
    let bind_addr: SocketAddr = config
        .bind_address
        .as_deref()
        .unwrap_or("0.0.0.0:0")
        .parse()
        .map_err(|e| PeatError::InvalidInput {
            msg: format!("Invalid bind address: {}", e),
        })?;

    // Create storage path
    let storage_path = PathBuf::from(&config.storage_path);
    std::fs::create_dir_all(&storage_path).map_err(|e| PeatError::StorageError {
        msg: format!("Failed to create storage directory: {}", e),
    })?;

    // TIMING: Parallel store + transport initialization
    let phase_start = Instant::now();

    // OPTIMIZATION: Run store opening and transport creation in parallel
    // These are independent operations that can overlap to reduce startup time.
    // - AutomergeStore::open() is blocking I/O (redb database)
    // - IrohTransport creation is async (QUIC endpoint binding)
    //
    // OPTIMIZATION: Use fast constructor WITHOUT mDNS discovery for faster startup.
    // mDNS discovery is deferred until after the sync backend is initialized.
    // This reduces "startup intensity" that was causing Docker API timeouts
    // in large-scale deployments (see 384-node hierarchical simulations).
    let seed = format!("{}/{}", config.app_id, config.storage_path);
    let storage_path_for_store = storage_path.clone();

    let (store, transport, store_ms, transport_ms) = runtime.block_on(async {
        let store_start = Instant::now();
        let transport_start = Instant::now();

        // Spawn store opening on blocking thread pool (it does sync I/O)
        let store_handle = tokio::task::spawn_blocking(move || {
            let result = AutomergeStore::open(&storage_path_for_store);
            (result, store_start.elapsed().as_millis())
        });

        // Create transport WITH mDNS discovery wired into the endpoint
        let transport_future = async {
            let result = IrohTransport::from_seed_with_discovery_at_addr(&seed, bind_addr).await;
            (result, transport_start.elapsed().as_millis())
        };

        // Wait for both to complete
        let (store_result, transport_result) = tokio::join!(store_handle, transport_future);

        // Unwrap the JoinHandle result first, then the actual result
        let (store_inner, store_elapsed) = store_result.map_err(|e| PeatError::StorageError {
            msg: format!("Store task panicked: {}", e),
        })?;
        let store = store_inner.map_err(|e| PeatError::StorageError {
            msg: format!("Failed to open store: {}", e),
        })?;

        let (transport_inner, transport_elapsed) = transport_result;
        let transport = transport_inner.map_err(|e| PeatError::ConnectionError {
            msg: format!("Failed to create transport with mDNS: {}", e),
        })?;

        Ok::<_, PeatError>((
            Arc::new(store),
            Arc::new(transport),
            store_elapsed,
            transport_elapsed,
        ))
    })?;

    let parallel_total_ms = phase_start.elapsed().as_millis();
    #[cfg(target_os = "android")]
    {
        android_log(&format!("[TIMING] Store open: {}ms", store_ms));
        android_log(&format!(
            "[TIMING] Transport create (with mDNS): {}ms",
            transport_ms
        ));
        android_log(&format!(
            "[TIMING] Parallel total (max of above): {}ms",
            parallel_total_ms
        ));
    }
    #[cfg(not(target_os = "android"))]
    {
        eprintln!("[Peat TIMING] Store open: {}ms", store_ms);
        eprintln!(
            "[Peat TIMING] Transport create (with mDNS): {}ms",
            transport_ms
        );
        eprintln!(
            "[Peat TIMING] Parallel total (max of above): {}ms",
            parallel_total_ms
        );
    }

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

    // TIMING: Sync backend initialization
    let phase_start = Instant::now();

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
            .map_err(|e| PeatError::SyncError {
                msg: format!("Failed to initialize sync backend: {}", e),
            })
    })?;

    let sync_init_ms = phase_start.elapsed().as_millis();
    #[cfg(target_os = "android")]
    {
        android_log(&format!("[TIMING] Sync backend init: {}ms", sync_init_ms));
        android_log("=== sync_backend.initialize() completed successfully ===");
    }
    #[cfg(not(target_os = "android"))]
    eprintln!("[Peat TIMING] Sync backend init: {}ms", sync_init_ms);

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

    // IMPORTANT (Issue #378): Use the storage_backend from sync_backend, NOT a new one!
    // Creating a separate AutomergeBackend would cause sync coordinator state to be split,
    // resulting in data not being received from peers.
    let storage_backend = sync_backend.storage_backend();

    // Create TransportManager for multi-transport coordination (ADR-032, #555)
    // Build TransportManagerConfig from FFI config (PACE policy + collection routes)
    let mut tm_config = TransportManagerConfig::default();

    if let Some(ref transport_config) = config.transport {
        // Build PACE policy from transport_preference
        if let Some(ref prefs) = transport_config.transport_preference {
            let policy = TransportPolicy::new("ffi-config").primary(prefs.clone());
            tm_config.default_policy = Some(policy);
        }

        // Parse collection routes from JSON
        if let Some(ref routes_json) = transport_config.collection_routes_json {
            match serde_json::from_str::<CollectionRouteTable>(routes_json) {
                Ok(table) => {
                    tm_config.collection_routes = table;
                }
                Err(e) => {
                    eprintln!("[Peat] Failed to parse collection_routes_json: {}", e);
                }
            }
        }
    }

    let mut transport_manager = TransportManager::new(tm_config);

    // Create IrohMeshTransport wrapper and register with TransportManager
    // This allows the transport to be selected via PACE policy alongside future transports
    let peer_config = PeerConfig {
        local: peat_protocol::network::LocalConfig {
            bind_address: bind_addr.to_string(),
            node_id: None,
        },
        formation: None,
        peers: Vec::new(),
    };
    let iroh_mesh_transport = Arc::new(IrohMeshTransport::new(Arc::clone(&transport), peer_config));
    let iroh_as_transport: Arc<dyn Transport> = iroh_mesh_transport.clone();
    transport_manager.register(iroh_as_transport.clone());

    // Register as PACE instance for collection routing
    let iroh_instance = TransportInstance::new(
        "iroh-primary",
        TransportType::Quic,
        TransportCapabilities::quic(),
    )
    .with_description("Primary Iroh/QUIC transport");
    transport_manager.register_instance(iroh_instance, iroh_as_transport);

    // Initialize BLE transport if enabled (ADR-039, #556)
    #[cfg(feature = "bluetooth")]
    if let Some(ref transport_config) = config.transport {
        if transport_config.enable_ble {
            #[cfg(target_os = "android")]
            {
                use peat_btle::platform::android::AndroidAdapter;
                use peat_btle::{BleConfig, BluetoothLETransport};

                android_log("BLE transport requested - initializing AndroidAdapter stub");

                // Derive BLE node ID from Iroh endpoint key (same as Linux path)
                let iroh_endpoint_id = transport.endpoint_id();
                let iroh_key_bytes = iroh_endpoint_id.as_bytes();
                let ble_node_id = peat_btle::NodeId::new(u32::from_be_bytes([
                    iroh_key_bytes[28],
                    iroh_key_bytes[29],
                    iroh_key_bytes[30],
                    iroh_key_bytes[31],
                ]));
                let ble_config = BleConfig::new(ble_node_id);
                let adapter = AndroidAdapter::new_stub();
                let btle = BluetoothLETransport::new(ble_config, adapter);
                let ble_transport = Arc::new(PeatBleTransport::new(btle));
                let ble_as_transport: Arc<dyn Transport> = ble_transport.clone();
                transport_manager.register(ble_as_transport.clone());

                // Register as PACE instance for collection routing
                let ble_instance = TransportInstance::new(
                    "ble-primary",
                    TransportType::BluetoothLE,
                    TransportCapabilities::bluetooth_le(),
                )
                .with_description("Primary BLE transport (Android)");
                transport_manager.register_instance(ble_instance, ble_as_transport);

                // Store in global for JNI access
                *ANDROID_BLE_TRANSPORT.lock().unwrap() = Some(ble_transport);

                android_log("BLE transport registered as PACE instance 'ble-primary'");
            }

            #[cfg(not(target_os = "android"))]
            {
                // On non-Android platforms, we can initialize BLE directly
                // Linux uses BluerAdapter, macOS uses CoreBluetoothAdapter
                #[cfg(target_os = "linux")]
                {
                    use peat_btle::platform::linux::BluerAdapter;
                    use peat_btle::{BleAdapter, BleConfig, BluetoothLETransport, PowerProfile};

                    // Parse power profile from config
                    let power_profile = match transport_config.ble_power_profile.as_deref() {
                        Some("aggressive") => PowerProfile::Aggressive,
                        Some("low_power") => PowerProfile::LowPower,
                        _ => PowerProfile::Balanced,
                    };

                    // Derive a 32-bit BLE node ID from the Iroh endpoint's public key
                    // Use last 4 bytes of the 32-byte key for a unique-enough identifier
                    let iroh_endpoint_id = transport.endpoint_id();
                    let iroh_key_bytes = iroh_endpoint_id.as_bytes();
                    let ble_node_id = peat_btle::NodeId::new(u32::from_be_bytes([
                        iroh_key_bytes[28],
                        iroh_key_bytes[29],
                        iroh_key_bytes[30],
                        iroh_key_bytes[31],
                    ]));

                    // Create BLE config with node ID, power profile, and mesh ID
                    let mut ble_config = BleConfig::new(ble_node_id);
                    ble_config.power_profile = power_profile;
                    if let Some(ref mesh_id) = transport_config.ble_mesh_id {
                        ble_config.mesh.mesh_id = mesh_id.clone();
                    }

                    // Create BLE transport with BluerAdapter
                    // IMPORTANT: All async BLE operations (create adapter, init, register
                    // GATT, start advertising/scanning) MUST happen in a single block_on().
                    // Splitting into two block_on() calls suspends the tokio runtime between
                    // them, which can cause the GATT ApplicationHandle's D-Bus registration
                    // to be dropped before advertising starts — making the GATT service
                    // intermittently invisible to remote devices.
                    match runtime_arc.block_on(async {
                        let mut adapter = BluerAdapter::new().await?;

                        // Initialize adapter with config (stores node ID, mesh ID, etc.)
                        adapter.init(&ble_config).await?;

                        // Register GATT service with BlueZ so peers can connect
                        adapter.register_gatt_service().await?;

                        // Wrap in transport layers
                        let btle = BluetoothLETransport::new(ble_config, adapter);
                        let ble_transport = Arc::new(PeatBleTransport::new(btle));

                        // Start advertising and scanning in the same async context
                        ble_transport.start().await.map_err(|e| {
                            peat_btle::BleError::PlatformError(format!(
                                "Failed to start BLE transport: {}",
                                e
                            ))
                        })?;

                        Ok::<_, peat_btle::BleError>(ble_transport)
                    }) {
                        Ok(ble_transport) => {
                            let ble_as_transport: Arc<dyn Transport> = ble_transport.clone();
                            transport_manager.register(ble_as_transport.clone());

                            // Register as PACE instance for collection routing
                            let ble_instance = TransportInstance::new(
                                "ble-primary",
                                TransportType::BluetoothLE,
                                TransportCapabilities::bluetooth_le(),
                            )
                            .with_description("Primary BLE transport");
                            transport_manager.register_instance(ble_instance, ble_as_transport);
                            eprintln!(
                                "[Peat] BLE transport registered as PACE instance 'ble-primary'"
                            );
                        }
                        Err(e) => {
                            eprintln!("[Peat] Failed to initialize BLE adapter: {} (continuing without BLE)", e);
                        }
                    }
                }

                #[cfg(not(target_os = "linux"))]
                eprintln!(
                    "[Peat] BLE transport requested but not yet implemented for this platform"
                );
            }
        }
    }

    // TIMING: Total startup time
    let total_ms = total_start.elapsed().as_millis();
    #[cfg(target_os = "android")]
    android_log(&format!(
        "[TIMING] === TOTAL create_node: {}ms ===",
        total_ms
    ));
    #[cfg(not(target_os = "android"))]
    eprintln!("[Peat TIMING] === TOTAL create_node: {}ms ===", total_ms);

    Ok(Arc::new(PeatNode {
        sync_backend,
        storage_backend,
        transport_manager,
        iroh_transport: transport,
        store,
        storage_path,
        runtime: runtime_arc,
        cleanup_running,
    }))
}

// Add new error variants for sync operations
#[cfg(feature = "sync")]
impl From<anyhow::Error> for PeatError {
    fn from(e: anyhow::Error) -> Self {
        PeatError::SyncError { msg: e.to_string() }
    }
}

// =============================================================================
// Peat Data Types for TAK Plugin Integration
// =============================================================================
//
// These types represent Peat entities that can be synced and displayed in the
// ATAK plugin. They use well-known collection names for document storage.

/// Well-known collection names for Peat data
pub mod collections {
    /// Collection for Peat cells (teams/squads)
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

/// Peat Cell information for display
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

    pub fn as_str(&self) -> &'static str {
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
// PeatNode Extensions for Typed Data Access
// =============================================================================

#[cfg(feature = "sync")]
#[uniffi::export]
impl PeatNode {
    // -------------------------------------------------------------------------
    // Cell Operations
    // -------------------------------------------------------------------------

    /// Get all cells from the sync document
    pub fn get_cells(&self) -> Result<Vec<CellInfo>, PeatError> {
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::CELLS);

            let docs = coll
                .scan()
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })?;

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
    pub fn get_cell(&self, cell_id: &str) -> Result<Option<CellInfo>, PeatError> {
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::CELLS);

            match coll.get(cell_id) {
                Ok(Some(data)) => {
                    let json = String::from_utf8(data).map_err(|e| PeatError::StorageError {
                        msg: format!("Invalid UTF-8: {}", e),
                    })?;
                    let cell = parse_cell_json(cell_id, &json)?;
                    Ok(Some(cell))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(PeatError::StorageError { msg: e.to_string() }),
            }
        })
    }

    /// Store a cell
    pub fn put_cell(&self, cell: CellInfo) -> Result<(), PeatError> {
        let json = serialize_cell_json(&cell)?;
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::CELLS);
            coll.upsert(&cell.id, json.into_bytes())
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })
        })
    }

    // -------------------------------------------------------------------------
    // Track Operations
    // -------------------------------------------------------------------------

    /// Get all tracks from the sync document
    pub fn get_tracks(&self) -> Result<Vec<TrackInfo>, PeatError> {
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::TRACKS);

            let docs = coll
                .scan()
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })?;

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
    pub fn get_track(&self, track_id: &str) -> Result<Option<TrackInfo>, PeatError> {
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::TRACKS);

            match coll.get(track_id) {
                Ok(Some(data)) => {
                    let json = String::from_utf8(data).map_err(|e| PeatError::StorageError {
                        msg: format!("Invalid UTF-8: {}", e),
                    })?;
                    let track = parse_track_json(track_id, &json)?;
                    Ok(Some(track))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(PeatError::StorageError { msg: e.to_string() }),
            }
        })
    }

    /// Store a track
    pub fn put_track(&self, track: TrackInfo) -> Result<(), PeatError> {
        let json = serialize_track_json(&track)?;
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::TRACKS);
            coll.upsert(&track.id, json.into_bytes())
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })
        })
    }

    // -------------------------------------------------------------------------
    // Platform Operations
    // -------------------------------------------------------------------------

    /// Get all platforms from the sync document
    pub fn get_platforms(&self) -> Result<Vec<PlatformInfo>, PeatError> {
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::PLATFORMS);

            let docs = coll
                .scan()
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })?;

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
    pub fn put_platform(&self, platform: PlatformInfo) -> Result<(), PeatError> {
        let json = serialize_platform_json(&platform)?;
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::PLATFORMS);
            coll.upsert(&platform.id, json.into_bytes())
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })
        })
    }

    // -------------------------------------------------------------------------
    // Command Operations (C2)
    // -------------------------------------------------------------------------

    /// Get all pending commands
    pub fn get_commands(&self) -> Result<Vec<CommandInfo>, PeatError> {
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::COMMANDS);

            let docs = coll
                .scan()
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })?;

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
    pub fn put_command(&self, command: CommandInfo) -> Result<(), PeatError> {
        let json = serialize_command_json(&command)?;
        self.runtime.block_on(async {
            let backend = &self.storage_backend;
            let coll = backend.collection(collections::COMMANDS);
            coll.upsert(&command.id, json.into_bytes())
                .map_err(|e| PeatError::StorageError { msg: e.to_string() })
        })
    }
}

// =============================================================================
// JSON Serialization Helpers
// =============================================================================

fn parse_cell_json(id: &str, json: &str) -> Result<CellInfo, PeatError> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| PeatError::InvalidInput {
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

fn serialize_cell_json(cell: &CellInfo) -> Result<String, PeatError> {
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
    serde_json::to_string(&v).map_err(|e| PeatError::EncodingError { msg: e.to_string() })
}

fn parse_track_json(id: &str, json: &str) -> Result<TrackInfo, PeatError> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| PeatError::InvalidInput {
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
        classification: v["classification"].as_str().unwrap_or("a-u-G").to_string(),
        confidence: v["confidence"].as_f64().unwrap_or(0.5),
        category: TrackCategory::from_str(v["category"].as_str().unwrap_or("UNKNOWN")),
        created_at: v["created_at"].as_i64().unwrap_or(0),
        last_update: v["last_update"].as_i64().unwrap_or(0),
    })
}

fn serialize_track_json(track: &TrackInfo) -> Result<String, PeatError> {
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
    serde_json::to_string(&v).map_err(|e| PeatError::EncodingError { msg: e.to_string() })
}

fn parse_platform_json(id: &str, json: &str) -> Result<PlatformInfo, PeatError> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| PeatError::InvalidInput {
        msg: format!("Invalid JSON: {}", e),
    })?;

    Ok(PlatformInfo {
        id: id.to_string(),
        platform_type: v["platform_type"].as_str().unwrap_or("unknown").to_string(),
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

fn serialize_platform_json(platform: &PlatformInfo) -> Result<String, PeatError> {
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
    serde_json::to_string(&v).map_err(|e| PeatError::EncodingError { msg: e.to_string() })
}

fn parse_command_json(id: &str, json: &str) -> Result<CommandInfo, PeatError> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| PeatError::InvalidInput {
        msg: format!("Invalid JSON: {}", e),
    })?;

    Ok(CommandInfo {
        id: id.to_string(),
        command_type: v["command_type"].as_str().unwrap_or("UNKNOWN").to_string(),
        target_id: v["target_id"].as_str().unwrap_or("").to_string(),
        parameters: v["parameters"].to_string(),
        priority: v["priority"].as_u64().unwrap_or(3) as u8,
        status: CommandStatus::from_str(v["status"].as_str().unwrap_or("PENDING")),
        originator: v["originator"].as_str().unwrap_or("").to_string(),
        created_at: v["created_at"].as_i64().unwrap_or(0),
        last_update: v["last_update"].as_i64().unwrap_or(0),
    })
}

fn serialize_command_json(command: &CommandInfo) -> Result<String, PeatError> {
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
    serde_json::to_string(&v).map_err(|e| PeatError::EncodingError { msg: e.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peat_version() {
        let version = peat_version();
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
// class PeatJni {
//     companion object {
//         init {
//             System.loadLibrary("peat_ffi")
//         }
//     }
//     external fun peatVersion(): String
//     external fun testJni(): String
// }
// ```

/// JNI: Get Peat library version
///
/// Kotlin signature: external fun peatVersion(): String
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_peatVersion(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let version = peat_version();
    env.new_string(&version)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Test that JNI bindings work
///
/// Kotlin signature: external fun testJni(): String
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_testJni(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let msg = "JNI bindings working! Peat FFI loaded successfully.";
    env.new_string(msg)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Create a Peat node (simplified for testing)
///
/// Kotlin signature: external fun createNodeJni(appId: String, sharedKey: String, storagePath: String): Long
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_createNodeJni(
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
        transport: None,
    };

    match create_node(config) {
        Ok(node) => {
            #[cfg(target_os = "android")]
            android_log("createNodeJni: Node created successfully");
            // Return the Arc pointer as a handle
            let handle = Arc::into_raw(node) as i64;
            // Store globally so it survives APK replacement
            if let Ok(mut global) = GLOBAL_NODE_HANDLE.lock() {
                *global = handle;
                #[cfg(target_os = "android")]
                android_log(&format!("createNodeJni: Stored global handle: {}", handle));
            }
            handle
        }
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!("createNodeJni: Error creating node: {:?}", e));
            0
        }
    }
}

/// JNI: Create a PeatNode with transport configuration (ADR-039, #558)
///
/// This extended version supports BLE transport configuration for unified
/// multi-transport operation. When enable_ble is true, the node will attempt
/// to initialize BLE transport alongside the default Iroh transport.
///
/// Note: On Android, BLE transport requires the Android BLE adapter to be
/// initialized via JNI callbacks. Full BLE support is pending Android adapter
/// integration in peat-btle.
///
/// Kotlin signature:
/// ```kotlin
/// external fun createNodeWithConfigJni(
///     appId: String,
///     sharedKey: String,
///     storagePath: String,
///     enableBle: Boolean,
///     blePowerProfile: String?  // "aggressive", "balanced", or "low_power"
/// ): Long
/// ```
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_createNodeWithConfigJni(
    mut env: JNIEnv,
    _class: JClass,
    app_id: JString,
    shared_key: JString,
    storage_path: JString,
    enable_ble: jboolean,
    ble_power_profile: JString,
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

    // Parse BLE power profile (null/empty string means use default)
    let power_profile: Option<String> = env.get_string(&ble_power_profile).ok().and_then(|s| {
        let s: String = s.into();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    });

    #[cfg(target_os = "android")]
    android_log(&format!(
        "createNodeWithConfigJni: app_id={}, storage_path={}, enable_ble={}, power_profile={:?}",
        app_id,
        storage_path,
        enable_ble != 0,
        power_profile
    ));

    // Build transport configuration
    let transport_config = if enable_ble != 0 {
        Some(TransportConfigFFI {
            enable_ble: true,
            ble_mesh_id: None, // Use app_id as mesh ID
            ble_power_profile: power_profile,
            transport_preference: None,
            collection_routes_json: None,
        })
    } else {
        None
    };

    let config = NodeConfig {
        app_id,
        shared_key,
        bind_address: None,
        storage_path,
        transport: transport_config,
    };

    match create_node(config) {
        Ok(node) => {
            #[cfg(target_os = "android")]
            android_log("createNodeWithConfigJni: Node created successfully");
            let handle = Arc::into_raw(node) as i64;
            if let Ok(mut global) = GLOBAL_NODE_HANDLE.lock() {
                *global = handle;
                #[cfg(target_os = "android")]
                android_log(&format!(
                    "createNodeWithConfigJni: Stored global handle: {}",
                    handle
                ));
            }
            handle
        }
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!(
                "createNodeWithConfigJni: Error creating node: {:?}",
                e
            ));
            0
        }
    }
}

/// JNI: Get the global node handle (survives APK replacement)
///
/// Kotlin signature: external fun getGlobalNodeHandleJni(): Long
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_getGlobalNodeHandleJni(
    _env: JNIEnv,
    _class: JClass,
) -> i64 {
    match GLOBAL_NODE_HANDLE.lock() {
        Ok(handle) => {
            let h = *handle;
            #[cfg(target_os = "android")]
            android_log(&format!("getGlobalNodeHandleJni: Returning handle: {}", h));
            h
        }
        Err(_) => 0,
    }
}

/// JNI: Get node ID from a PeatNode handle
///
/// Kotlin signature: external fun nodeIdJni(handle: Long): String
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_nodeIdJni(
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

    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };
    let node_id = node.node_id();

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    env.new_string(&node_id)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Get peer count from a PeatNode handle
///
/// Kotlin signature: external fun peerCountJni(handle: Long): Int
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_peerCountJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> i32 {
    if handle == 0 {
        return 0;
    }

    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };
    let count = node.peer_count() as i32;

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    count
}

/// JNI: Request full document sync with all connected peers
///
/// Kotlin signature: external fun requestSyncJni(handle: Long): Boolean
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_requestSyncJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jboolean {
    if handle == 0 {
        return 0;
    }
    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };
    let result = node.request_sync().is_ok();
    std::mem::forget(node);
    result as jboolean
}

/// JNI: Get connected peer IDs as a JSON array
///
/// Kotlin signature: external fun connectedPeersJni(handle: Long): String
/// Returns JSON array of hex-encoded peer IDs, or "[]" on error
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_connectedPeersJni(
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

    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };
    let peers = node.connected_peers();
    let result = serde_json::to_string(&peers).unwrap_or_else(|_| "[]".to_string());

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    env.new_string(&result)
        .expect("Failed to create Java string")
        .into_raw()
}

/// JNI: Start sync on a PeatNode
///
/// Kotlin signature: external fun startSyncJni(handle: Long): Boolean
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_startSyncJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> bool {
    // CRITICAL DEBUG: Log unconditionally to verify this function is called
    eprintln!("startSyncJni: CALLED with handle={}", handle);
    #[cfg(target_os = "android")]
    android_log(&format!("startSyncJni: ENTERED with handle={}", handle));

    if handle == 0 {
        #[cfg(target_os = "android")]
        android_log("startSyncJni: handle is 0, returning false");
        return false;
    }

    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };

    #[cfg(target_os = "android")]
    android_log("startSyncJni: calling node.start_sync()");

    let result = match node.start_sync() {
        Ok(()) => {
            #[cfg(target_os = "android")]
            android_log("startSyncJni: start_sync succeeded");
            true
        }
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!("startSyncJni: start_sync failed: {}", e));
            false
        }
    };

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    result
}

/// JNI: Free a PeatNode handle
///
/// Kotlin signature: external fun freeNodeJni(handle: Long)
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_freeNodeJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    if handle != 0 {
        #[cfg(target_os = "android")]
        android_log(&format!("freeNodeJni: Freeing node handle {}", handle));

        // Reconstruct the Arc to drop it
        let node = unsafe { Arc::from_raw(handle as *const PeatNode) };

        // Signal the cleanup task to stop
        node.cleanup_running.store(false, Ordering::SeqCst);

        #[cfg(target_os = "android")]
        android_log("freeNodeJni: Signaled cleanup task to stop");

        // Give the background task a moment to exit
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Clear Android BLE transport global to prevent dangling refs
        #[cfg(all(feature = "bluetooth", target_os = "android"))]
        {
            *ANDROID_BLE_TRANSPORT.lock().unwrap() = None;
            android_log("freeNodeJni: Cleared ANDROID_BLE_TRANSPORT");
        }

        // Drop the node - this should release the database
        drop(node);

        #[cfg(target_os = "android")]
        android_log("freeNodeJni: Node dropped");
    }
}

// =============================================================================
// BLE Transport JNI Methods (Android)
// =============================================================================

/// JNI: Signal BLE transport started/stopped
///
/// Called by Kotlin when the Android BLE stack is ready or shutting down.
/// This makes `is_available()` return true/false for PACE routing.
///
/// Kotlin signature: external fun bleSetStartedJni(handle: Long, started: Boolean)
#[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_bleSetStartedJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
    started: jboolean,
) {
    if handle == 0 {
        android_log("bleSetStartedJni: Invalid handle (0)");
        return;
    }

    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };

    use peat_protocol::transport::MeshTransport;

    let guard = ANDROID_BLE_TRANSPORT.lock().unwrap();
    if let Some(ref ble_transport) = *guard {
        if started != 0 {
            match node.runtime.block_on(ble_transport.start()) {
                Ok(()) => android_log("bleSetStartedJni: BLE transport started"),
                Err(e) => android_log(&format!("bleSetStartedJni: start failed: {}", e)),
            }
        } else {
            match node.runtime.block_on(ble_transport.stop()) {
                Ok(()) => android_log("bleSetStartedJni: BLE transport stopped"),
                Err(e) => android_log(&format!("bleSetStartedJni: stop failed: {}", e)),
            }
        }
    } else {
        android_log("bleSetStartedJni: No BLE transport registered");
    }
    drop(guard);

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);
}

/// JNI: Add a reachable BLE peer
///
/// Called by Kotlin when a BLE peer is discovered/connected.
/// This makes `can_reach(peer)` return true for PACE routing.
///
/// Kotlin signature: external fun bleAddPeerJni(handle: Long, peerId: String)
#[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_bleAddPeerJni(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    peer_id: JString,
) {
    if handle == 0 {
        android_log("bleAddPeerJni: Invalid handle (0)");
        return;
    }

    let peer_id_str: String = match env.get_string(&peer_id) {
        Ok(s) => s.into(),
        Err(_) => {
            android_log("bleAddPeerJni: Failed to get peer_id string");
            return;
        }
    };

    android_log(&format!("bleAddPeerJni: Adding peer {}", peer_id_str));

    let guard = ANDROID_BLE_TRANSPORT.lock().unwrap();
    if let Some(ref ble_transport) = *guard {
        use peat_protocol::transport::NodeId;
        ble_transport.add_reachable_peer(NodeId::new(peer_id_str));
    } else {
        android_log("bleAddPeerJni: No BLE transport registered");
    }
}

/// JNI: Remove a reachable BLE peer
///
/// Called by Kotlin when a BLE peer is disconnected/lost.
/// This makes `can_reach(peer)` return false for PACE routing.
///
/// Kotlin signature: external fun bleRemovePeerJni(handle: Long, peerId: String)
#[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_bleRemovePeerJni(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    peer_id: JString,
) {
    if handle == 0 {
        android_log("bleRemovePeerJni: Invalid handle (0)");
        return;
    }

    let peer_id_str: String = match env.get_string(&peer_id) {
        Ok(s) => s.into(),
        Err(_) => {
            android_log("bleRemovePeerJni: Failed to get peer_id string");
            return;
        }
    };

    android_log(&format!("bleRemovePeerJni: Removing peer {}", peer_id_str));

    let guard = ANDROID_BLE_TRANSPORT.lock().unwrap();
    if let Some(ref ble_transport) = *guard {
        use peat_protocol::transport::NodeId;
        ble_transport.remove_reachable_peer(&NodeId::new(peer_id_str));
    } else {
        android_log("bleRemovePeerJni: No BLE transport registered");
    }
}

/// JNI: Query whether BLE transport is available (started)
///
/// Called by Kotlin to check if BLE transport is active for UI display.
/// Returns true if BLE transport has been started via bleSetStartedJni.
///
/// Kotlin signature: external fun bleIsAvailableJni(handle: Long): Boolean
#[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_bleIsAvailableJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jboolean {
    if handle == 0 {
        android_log("bleIsAvailableJni: Invalid handle (0)");
        return 0;
    }

    use peat_protocol::transport::Transport;

    let guard = ANDROID_BLE_TRANSPORT.lock().unwrap();
    let result = match guard.as_ref() {
        Some(t) => {
            if t.is_available() {
                1
            } else {
                0
            }
        }
        None => 0,
    };

    android_log(&format!("bleIsAvailableJni: {}", result != 0));
    result
}

/// JNI: Get the number of reachable BLE peers
///
/// Called by Kotlin to get BLE peer count for unified UI display.
/// Returns the number of peers added via bleAddPeerJni.
///
/// Kotlin signature: external fun blePeerCountJni(handle: Long): Int
#[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_blePeerCountJni(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jint {
    if handle == 0 {
        android_log("blePeerCountJni: Invalid handle (0)");
        return 0;
    }

    let guard = ANDROID_BLE_TRANSPORT.lock().unwrap();
    let count = match guard.as_ref() {
        Some(t) => t.reachable_peer_count() as jint,
        None => 0,
    };

    android_log(&format!("blePeerCountJni: {}", count));
    count
}

/// JNI: Get all cells as JSON array string
///
/// Kotlin signature: external fun getCellsJni(handle: Long): String
/// Returns JSON array of cell objects, or "[]" on error
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_getCellsJni(
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

    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };
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
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_getTracksJni(
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

    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };
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
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_getPlatformsJni(
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

    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };
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

/// JNI: Publish a platform (self-position/PLI) to the Peat network
///
/// Kotlin signature: external fun publishPlatformJni(handle: Long, platformJson: String): Boolean
/// Stores the platform in the "platforms" collection for sync to peers.
///
/// Expected JSON format:
/// ```json
/// {
///   "id": "atak-device-uid",
///   "name": "CALLSIGN",
///   "platform_type": "SOLDIER",
///   "lat": 33.7490,
///   "lon": -84.3880,
///   "hae": 320.0,
///   "heading": 45.0,
///   "speed": 1.5,
///   "status": "ACTIVE",
///   "capabilities": ["PLI"],
///   "cell_id": null,
///   "readiness": 1.0
/// }
/// ```
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_publishPlatformJni(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    platform_json: JString,
) -> jboolean {
    if handle == 0 {
        #[cfg(target_os = "android")]
        android_log("publishPlatformJni: Invalid handle (0)");
        return 0; // JNI_FALSE
    }

    // Get platform JSON string from Java
    let json_str: String = match env.get_string(&platform_json) {
        Ok(s) => s.into(),
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!(
                "publishPlatformJni: Failed to get JSON string: {:?}",
                e
            ));
            return 0; // JNI_FALSE
        }
    };

    #[cfg(target_os = "android")]
    android_log(&format!("publishPlatformJni: Received JSON: {}", json_str));

    // Parse JSON to validate and extract platform info
    let platform: PlatformInfo = match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(v) => {
            // Extract platform ID - required field
            let id = match v["id"].as_str() {
                Some(id) if !id.is_empty() => id.to_string(),
                _ => {
                    #[cfg(target_os = "android")]
                    android_log("publishPlatformJni: Missing or empty 'id' field");
                    return 0; // JNI_FALSE
                }
            };

            PlatformInfo {
                id,
                platform_type: v["platform_type"].as_str().unwrap_or("SOLDIER").to_string(),
                name: v["name"].as_str().unwrap_or("Unknown").to_string(),
                status: PlatformStatus::from_str(v["status"].as_str().unwrap_or("ACTIVE")),
                lat: v["lat"].as_f64().unwrap_or(0.0),
                lon: v["lon"].as_f64().unwrap_or(0.0),
                hae: v["hae"].as_f64(),
                readiness: v["readiness"].as_f64().unwrap_or(1.0),
                capabilities: v["capabilities"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_else(|| vec!["PLI".to_string()]),
                cell_id: v["cell_id"].as_str().map(|s| s.to_string()),
                last_heartbeat: chrono::Utc::now().timestamp_millis(),
            }
        }
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!("publishPlatformJni: Invalid JSON: {:?}", e));
            return 0; // JNI_FALSE
        }
    };

    #[cfg(target_os = "android")]
    android_log(&format!(
        "publishPlatformJni: Publishing platform id={}, name={}, lat={}, lon={}",
        platform.id, platform.name, platform.lat, platform.lon
    ));

    // Get node from handle and store platform
    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };
    let result = match node.put_platform(platform) {
        Ok(_) => {
            #[cfg(target_os = "android")]
            android_log("publishPlatformJni: Platform published successfully");
            1 // JNI_TRUE
        }
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!("publishPlatformJni: Failed to publish: {:?}", e));
            0 // JNI_FALSE
        }
    };

    // Don't drop the Arc - we're just borrowing
    std::mem::forget(node);

    result
}

/// Connect to a known peer by node ID and address (bypasses mDNS).
///
/// Kotlin signature: external fun connectPeerJni(handle: Long, nodeId: String, address: String): Boolean
/// Used by the dual-transport test to connect Android to rpi-ci2 over QUIC
/// when mDNS is unreliable.
#[cfg(feature = "sync")]
#[no_mangle]
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_connectPeerJni(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    node_id: JString,
    address: JString,
) -> jboolean {
    if handle == 0 {
        #[cfg(target_os = "android")]
        android_log("connectPeerJni: Invalid handle (0)");
        return 0;
    }

    let node_id_str: String = match env.get_string(&node_id) {
        Ok(s) => s.into(),
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!("connectPeerJni: Failed to get nodeId: {:?}", e));
            return 0;
        }
    };

    let addr_str: String = match env.get_string(&address) {
        Ok(s) => s.into(),
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!("connectPeerJni: Failed to get address: {:?}", e));
            return 0;
        }
    };

    #[cfg(target_os = "android")]
    android_log(&format!(
        "connectPeerJni: Connecting to node={}, addr={}",
        node_id_str, addr_str
    ));

    let peer_info = PeerInfo {
        name: "quic-peer".to_string(),
        node_id: node_id_str,
        addresses: vec![addr_str],
        relay_url: None,
    };

    let node = unsafe { Arc::from_raw(handle as *const PeatNode) };
    let result = match node.connect_peer(peer_info) {
        Ok(()) => {
            #[cfg(target_os = "android")]
            android_log("connectPeerJni: Connected successfully");
            1
        }
        Err(e) => {
            #[cfg(target_os = "android")]
            android_log(&format!("connectPeerJni: Failed to connect: {:?}", e));
            0
        }
    };

    std::mem::forget(node);
    result
}

// =============================================================================
// JNI Native Method Registration
// =============================================================================
//
// Android's linker namespace isolation prevents normal JNI symbol lookup.
// We provide a nativeInit function that Kotlin must call after System.load()
// to explicitly register the native methods.

/// Register native methods for PeatJni class
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
pub extern "system" fn Java_com_defenseunicorns_atak_peat_PeatJni_nativeInit(
    mut env: JNIEnv,
    class: JClass,
) {
    use jni::NativeMethod;

    let methods: Vec<NativeMethod> = vec![
        NativeMethod {
            name: "peatVersion".into(),
            sig: "()Ljava/lang/String;".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_peatVersion as *mut c_void,
        },
        NativeMethod {
            name: "testJni".into(),
            sig: "()Ljava/lang/String;".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_testJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "createNodeJni".into(),
            sig: "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)J".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_createNodeJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "getGlobalNodeHandleJni".into(),
            sig: "()J".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_getGlobalNodeHandleJni
                as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "nodeIdJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_nodeIdJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "peerCountJni".into(),
            sig: "(J)I".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_peerCountJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "connectedPeersJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_connectedPeersJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "requestSyncJni".into(),
            sig: "(J)Z".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_requestSyncJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "startSyncJni".into(),
            sig: "(J)Z".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_startSyncJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "freeNodeJni".into(),
            sig: "(J)V".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_freeNodeJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "getCellsJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_getCellsJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "getTracksJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_getTracksJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "getPlatformsJni".into(),
            sig: "(J)Ljava/lang/String;".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_getPlatformsJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "publishPlatformJni".into(),
            sig: "(JLjava/lang/String;)Z".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_publishPlatformJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "connectPeerJni".into(),
            sig: "(JLjava/lang/String;Ljava/lang/String;)Z".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_connectPeerJni as *mut c_void,
        },
        #[cfg(feature = "sync")]
        NativeMethod {
            name: "createNodeWithConfigJni".into(),
            sig: "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;ZLjava/lang/String;)J"
                .into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_createNodeWithConfigJni
                as *mut c_void,
        },
        #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
        NativeMethod {
            name: "bleSetStartedJni".into(),
            sig: "(JZ)V".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_bleSetStartedJni as *mut c_void,
        },
        #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
        NativeMethod {
            name: "bleAddPeerJni".into(),
            sig: "(JLjava/lang/String;)V".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_bleAddPeerJni as *mut c_void,
        },
        #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
        NativeMethod {
            name: "bleRemovePeerJni".into(),
            sig: "(JLjava/lang/String;)V".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_bleRemovePeerJni as *mut c_void,
        },
        #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
        NativeMethod {
            name: "bleIsAvailableJni".into(),
            sig: "(J)Z".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_bleIsAvailableJni as *mut c_void,
        },
        #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
        NativeMethod {
            name: "blePeerCountJni".into(),
            sig: "(J)I".into(),
            fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_blePeerCountJni as *mut c_void,
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
#[allow(clippy::not_unsafe_ptr_arg_deref)] // JNI ABI requires raw pointer params
pub extern "C" fn JNI_OnLoad(vm: *mut JavaVM, _reserved: *mut c_void) -> jint {
    // Log that we're being called
    #[cfg(target_os = "android")]
    android_log("JNI_OnLoad called for peat_ffi");

    // NOTE: Tracing initialization disabled - was causing issues and blocking debugging.
    // The android_log() function works directly and is used for critical logging.
    // Tracing from protocol crate won't show in logcat, but we can add android_log
    // callbacks if needed for debugging specific paths.

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
    let peer_event_manager_class = "com/defenseunicorns/atak/peat/PeerEventManager";
    match env.find_class(peer_event_manager_class) {
        Ok(class) => match env.new_global_ref(class) {
            Ok(global_ref) => {
                *PEER_EVENT_MANAGER_CLASS.lock().unwrap() = Some(global_ref);
                #[cfg(target_os = "android")]
                android_log("JNI_OnLoad: PeerEventManager class found and cached");
            }
            Err(_) => {
                #[cfg(target_os = "android")]
                android_log("JNI_OnLoad: Failed to create global ref for PeerEventManager");
            }
        },
        Err(_) => {
            #[cfg(target_os = "android")]
            android_log(
                "JNI_OnLoad: PeerEventManager class not found (OK if loading before class init)",
            );
        }
    }

    #[cfg(target_os = "android")]
    android_log("JNI_OnLoad: Got JNIEnv, looking for PeatJni class...");

    // Try to find the PeatJni class and register natives
    let class_name = "com/defenseunicorns/atak/peat/PeatJni";
    match env.find_class(class_name) {
        Ok(class) => {
            #[cfg(target_os = "android")]
            android_log("JNI_OnLoad: Found PeatJni class, registering natives...");

            // Register native methods
            use jni::NativeMethod;
            let methods: Vec<NativeMethod> = vec![
                NativeMethod {
                    name: "nativeInit".into(),
                    sig: "()V".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_nativeInit as *mut c_void,
                },
                NativeMethod {
                    name: "peatVersion".into(),
                    sig: "()Ljava/lang/String;".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_peatVersion as *mut c_void,
                },
                NativeMethod {
                    name: "testJni".into(),
                    sig: "()Ljava/lang/String;".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_testJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "createNodeJni".into(),
                    sig: "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)J".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_createNodeJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "getGlobalNodeHandleJni".into(),
                    sig: "()J".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_getGlobalNodeHandleJni
                        as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "nodeIdJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_nodeIdJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "peerCountJni".into(),
                    sig: "(J)I".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_peerCountJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "connectedPeersJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_connectedPeersJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "requestSyncJni".into(),
                    sig: "(J)Z".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_requestSyncJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "startSyncJni".into(),
                    sig: "(J)Z".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_startSyncJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "freeNodeJni".into(),
                    sig: "(J)V".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_freeNodeJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "getCellsJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_getCellsJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "getTracksJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_getTracksJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "getPlatformsJni".into(),
                    sig: "(J)Ljava/lang/String;".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_getPlatformsJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "publishPlatformJni".into(),
                    sig: "(JLjava/lang/String;)Z".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_publishPlatformJni
                        as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "connectPeerJni".into(),
                    sig: "(JLjava/lang/String;Ljava/lang/String;)Z".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_connectPeerJni as *mut c_void,
                },
                #[cfg(feature = "sync")]
                NativeMethod {
                    name: "createNodeWithConfigJni".into(),
                    sig: "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;ZLjava/lang/String;)J"
                        .into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_createNodeWithConfigJni
                        as *mut c_void,
                },
                #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
                NativeMethod {
                    name: "bleSetStartedJni".into(),
                    sig: "(JZ)V".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_bleSetStartedJni as *mut c_void,
                },
                #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
                NativeMethod {
                    name: "bleAddPeerJni".into(),
                    sig: "(JLjava/lang/String;)V".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_bleAddPeerJni as *mut c_void,
                },
                #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
                NativeMethod {
                    name: "bleRemovePeerJni".into(),
                    sig: "(JLjava/lang/String;)V".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_bleRemovePeerJni as *mut c_void,
                },
                #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
                NativeMethod {
                    name: "bleIsAvailableJni".into(),
                    sig: "(J)Z".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_bleIsAvailableJni as *mut c_void,
                },
                #[cfg(all(feature = "sync", feature = "bluetooth", target_os = "android"))]
                NativeMethod {
                    name: "blePeerCountJni".into(),
                    sig: "(J)I".into(),
                    fn_ptr: Java_com_defenseunicorns_atak_peat_PeatJni_blePeerCountJni as *mut c_void,
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
                "JNI_OnLoad: PeatJni class not found (this is OK if loading before class init)",
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

    let tag = CString::new("PeatFFI").unwrap();
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
            let peer_event_manager_class = "com/defenseunicorns/atak/peat/PeerEventManager";
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
            android_log(&format!(
                "notify_peer_event: Failed to attach thread: {:?}",
                e
            ));
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
        android_log(&format!(
            "notify_peer_event: {} called for {}",
            method_name, peer_id
        ));
    }
}
