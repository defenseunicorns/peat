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

use std::collections::HashMap;
use std::sync::Arc;

use hive_protocol::cot::{
    CotEncoder, Position as CotPosition, TrackUpdate, Velocity as CotVelocity,
};

#[cfg(feature = "sync")]
use hive_protocol::network::{IrohTransport, PeerInfo as HivePeerInfo};
#[cfg(feature = "sync")]
use hive_protocol::storage::{AutomergeBackend, AutomergeStore, StorageBackend, SyncCapable};
#[cfg(feature = "sync")]
use std::net::SocketAddr;
#[cfg(feature = "sync")]
use std::path::PathBuf;
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
    /// Bind address for P2P connections (e.g., "127.0.0.1:0" for auto-assign)
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

/// A HIVE network node with P2P sync capabilities
///
/// Wraps AutomergeBackend + IrohTransport for document sync.
#[cfg(feature = "sync")]
#[derive(uniffi::Object)]
pub struct HiveNode {
    backend: Arc<RwLock<AutomergeBackend>>,
    transport: Arc<IrohTransport>,
    #[allow(dead_code)] // Kept for potential future use (e.g., storage cleanup)
    storage_path: PathBuf,
    /// Tokio runtime for async operations
    runtime: Arc<tokio::runtime::Runtime>,
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
            let backend = self.backend.read().await;
            backend
                .start_sync()
                .map_err(|e| HiveError::SyncError { msg: e.to_string() })
        })
    }

    /// Stop sync operations
    pub fn stop_sync(&self) -> Result<(), HiveError> {
        self.runtime.block_on(async {
            let backend = self.backend.read().await;
            backend
                .stop_sync()
                .map_err(|e| HiveError::SyncError { msg: e.to_string() })
        })
    }

    /// Get sync statistics
    pub fn sync_stats(&self) -> Result<SyncStats, HiveError> {
        self.runtime.block_on(async {
            let backend = self.backend.read().await;
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
            let backend = self.backend.read().await;
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
            let backend = self.backend.read().await;
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
            let backend = self.backend.read().await;
            let coll = backend.collection(collection);

            coll.delete(doc_id)
                .map_err(|e| HiveError::StorageError { msg: e.to_string() })
        })
    }

    /// List all document IDs in a collection
    pub fn list_documents(&self, collection: &str) -> Result<Vec<String>, HiveError> {
        self.runtime.block_on(async {
            let backend = self.backend.read().await;
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
            let backend = self.backend.read().await;

            backend
                .sync_document(&doc_key)
                .await
                .map_err(|e| HiveError::SyncError { msg: e.to_string() })
        })
    }
}

/// Create a new HiveNode
///
/// Note: This function is NOT async because we manage our own Tokio runtime
/// to ensure proper context for Iroh transport operations.
#[cfg(feature = "sync")]
#[uniffi::export]
pub fn create_node(config: NodeConfig) -> Result<Arc<HiveNode>, HiveError> {
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
        .unwrap_or("127.0.0.1:0")
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

    // Create IrohTransport within our runtime context
    let transport = runtime.block_on(async {
        IrohTransport::bind(bind_addr)
            .await
            .map_err(|e| HiveError::ConnectionError {
                msg: format!("Failed to bind transport: {}", e),
            })
    })?;
    let transport = Arc::new(transport);

    // Create backend with transport
    let backend = AutomergeBackend::with_transport(store, Arc::clone(&transport));

    Ok(Arc::new(HiveNode {
        backend: Arc::new(RwLock::new(backend)),
        transport,
        storage_path,
        runtime: Arc::new(runtime),
    }))
}

// Add new error variants for sync operations
#[cfg(feature = "sync")]
impl From<anyhow::Error> for HiveError {
    fn from(e: anyhow::Error) -> Self {
        HiveError::SyncError { msg: e.to_string() }
    }
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
