//! UniFFI bindings for hive-btle on Apple platforms
//!
//! This crate provides Swift bindings for the HIVE BLE library using UniFFI.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use hive_btle::config::{BleConfig, BlePhy as HiveBlePhy, DiscoveryConfig, MeshConfig};
use hive_btle::platform::apple::CoreBluetoothAdapter;
use hive_btle::platform::BleAdapter;
use hive_btle::{NodeId as HiveNodeId, DEFAULT_MESH_ID};

// Setup UniFFI
uniffi::setup_scaffolding!();

/// Initialize logging for the library
#[uniffi::export]
pub fn init_logging() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init();
    log::info!("hive-apple-ffi initialized");
}

/// Get the default mesh ID used for demos
#[uniffi::export]
pub fn get_default_mesh_id() -> String {
    DEFAULT_MESH_ID.to_string()
}

/// Parsed device name result
#[derive(Debug, Clone, uniffi::Record)]
pub struct ParsedDeviceName {
    /// Mesh ID (None for legacy HIVE- format)
    pub mesh_id: Option<String>,
    /// Node ID
    pub node_id: u32,
}

/// Parse a HIVE device name to extract mesh ID and node ID
///
/// Supports both formats:
/// - New: `HIVE_<MESH_ID>-<NODE_ID>` (e.g., "HIVE_DEMO-12345678")
/// - Legacy: `HIVE-<NODE_ID>` (e.g., "HIVE-12345678")
#[uniffi::export]
pub fn parse_hive_device_name(name: String) -> Option<ParsedDeviceName> {
    MeshConfig::parse_device_name(&name).map(|(mesh_id, node_id)| ParsedDeviceName {
        mesh_id,
        node_id: node_id.as_u32(),
    })
}

/// Generate a HIVE device name for advertising
#[uniffi::export]
pub fn generate_hive_device_name(mesh_id: String, node_id: u32) -> String {
    let config = MeshConfig::new(mesh_id);
    config.device_name(HiveNodeId::new(node_id))
}

/// Check if a device matches a specific mesh
///
/// Returns true if the device has the same mesh ID, or if the device
/// has no mesh ID (legacy format - backwards compatible)
#[uniffi::export]
pub fn matches_mesh(our_mesh_id: String, device_mesh_id: Option<String>) -> bool {
    let config = MeshConfig::new(our_mesh_id);
    config.matches_mesh(device_mesh_id.as_deref())
}

/// BLE PHY mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BlePhy {
    Le1M,
    Le2M,
    LeCoded,
}

impl From<HiveBlePhy> for BlePhy {
    fn from(phy: HiveBlePhy) -> Self {
        match phy {
            HiveBlePhy::Le1M => BlePhy::Le1M,
            HiveBlePhy::Le2M => BlePhy::Le2M,
            HiveBlePhy::LeCodedS2 | HiveBlePhy::LeCodedS8 => BlePhy::LeCoded,
        }
    }
}

/// Bluetooth adapter state
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BluetoothState {
    Unknown,
    Resetting,
    Unsupported,
    Unauthorized,
    PoweredOff,
    PoweredOn,
}

/// Information about a discovered BLE device
#[derive(Debug, Clone, uniffi::Record)]
pub struct DiscoveredPeer {
    pub identifier: String,
    pub name: Option<String>,
    pub rssi: i8,
    pub node_id: Option<u32>,
    pub is_hive_node: bool,
}

/// Information about an active connection
#[derive(Debug, Clone, uniffi::Record)]
pub struct ConnectionInfo {
    pub peer_id: u32,
    pub identifier: String,
    pub mtu: u16,
    pub phy: BlePhy,
    pub rssi: Option<i8>,
    pub is_alive: bool,
}

/// Sync statistics
#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncStats {
    pub document_count: u32,
    pub pending_changes: u32,
    pub bytes_synced: u64,
    pub last_sync_timestamp: Option<u64>,
}

impl Default for SyncStats {
    fn default() -> Self {
        SyncStats {
            document_count: 0,
            pending_changes: 0,
            bytes_synced: 0,
            last_sync_timestamp: None,
        }
    }
}

/// Error types for the HIVE adapter
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HiveError {
    #[error("Adapter not initialized")]
    NotInitialized,
    #[error("Adapter already initialized")]
    AlreadyInitialized,
    #[error("Adapter not running")]
    NotRunning,
    #[error("Bluetooth is unavailable")]
    BluetoothUnavailable,
    #[error("Bluetooth is not authorized")]
    BluetoothUnauthorized,
    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },
    #[error("Send failed: {reason}")]
    SendFailed { reason: String },
    #[error("Operation timed out")]
    Timeout,
    #[error("Invalid state for operation")]
    InvalidState,
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl From<hive_btle::error::BleError> for HiveError {
    fn from(err: hive_btle::error::BleError) -> Self {
        match err {
            hive_btle::error::BleError::NotSupported(_) => HiveError::BluetoothUnavailable,
            hive_btle::error::BleError::ConnectionFailed(msg) => {
                HiveError::ConnectionFailed { reason: msg }
            }
            hive_btle::error::BleError::Timeout => HiveError::Timeout,
            hive_btle::error::BleError::PlatformError(msg) => HiveError::Internal { message: msg },
            _ => HiveError::Internal {
                message: err.to_string(),
            },
        }
    }
}

/// Callback interface for discovery events
#[uniffi::export(callback_interface)]
pub trait DiscoveryCallback: Send + Sync {
    fn on_peer_discovered(&self, peer: DiscoveredPeer);
    fn on_peer_lost(&self, identifier: String);
}

/// Callback interface for connection events
#[uniffi::export(callback_interface)]
pub trait ConnectionCallback: Send + Sync {
    fn on_connected(&self, peer_id: u32, info: ConnectionInfo);
    fn on_disconnected(&self, peer_id: u32, reason: String);
    fn on_connection_failed(&self, identifier: String, error: String);
}

/// Callback interface for data reception
#[uniffi::export(callback_interface)]
pub trait DataCallback: Send + Sync {
    fn on_data_received(&self, peer_id: u32, data: Vec<u8>);
}

/// Internal state for the adapter
struct AdapterState {
    is_running: bool,
    is_discovering: bool,
    is_advertising: bool,
    bluetooth_state: BluetoothState,
    discovered_peers: HashMap<String, DiscoveredPeer>,
    connections: HashMap<u32, ConnectionInfo>,
    sync_stats: SyncStats,
}

impl Default for AdapterState {
    fn default() -> Self {
        AdapterState {
            is_running: false,
            is_discovering: false,
            is_advertising: false,
            bluetooth_state: BluetoothState::Unknown,
            discovered_peers: HashMap::new(),
            connections: HashMap::new(),
            sync_stats: SyncStats::default(),
        }
    }
}

/// Main HIVE BLE adapter interface
#[derive(uniffi::Object)]
pub struct HiveAdapter {
    node_id: u32,
    mesh_id: String,
    state: RwLock<AdapterState>,
    adapter: RwLock<Option<CoreBluetoothAdapter>>,
    runtime: tokio::runtime::Runtime,
    discovery_callback: Mutex<Option<Box<dyn DiscoveryCallback>>>,
    connection_callback: Mutex<Option<Box<dyn ConnectionCallback>>>,
    data_callback: Mutex<Option<Box<dyn DataCallback>>>,
}

#[uniffi::export]
impl HiveAdapter {
    /// Create a new adapter with the given node ID and default mesh ID ("DEMO")
    #[uniffi::constructor]
    pub fn new(node_id: u32) -> Result<Arc<Self>, HiveError> {
        Self::with_mesh_id(node_id, DEFAULT_MESH_ID.to_string())
    }

    /// Create a new adapter with a specific mesh ID
    #[uniffi::constructor]
    pub fn with_mesh_id(node_id: u32, mesh_id: String) -> Result<Arc<Self>, HiveError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| HiveError::Internal {
                message: format!("Failed to create runtime: {}", e),
            })?;

        log::info!(
            "Creating HiveAdapter with node ID: {:08X}, mesh ID: {}",
            node_id,
            mesh_id
        );

        Ok(Arc::new(HiveAdapter {
            node_id,
            mesh_id,
            state: RwLock::new(AdapterState::default()),
            adapter: RwLock::new(None),
            runtime,
            discovery_callback: Mutex::new(None),
            connection_callback: Mutex::new(None),
            data_callback: Mutex::new(None),
        }))
    }

    /// Get the local node ID
    pub fn get_node_id(&self) -> u32 {
        self.node_id
    }

    /// Get the mesh ID this adapter is configured for
    pub fn get_mesh_id(&self) -> String {
        self.mesh_id.clone()
    }

    /// Get the BLE device name for this node
    ///
    /// Format: `HIVE_<MESH_ID>-<NODE_ID>` (e.g., "HIVE_DEMO-12345678")
    pub fn get_device_name(&self) -> String {
        let config = MeshConfig::new(&self.mesh_id);
        config.device_name(HiveNodeId::new(self.node_id))
    }

    /// Check if a discovered device matches our mesh
    pub fn device_matches_mesh(&self, device_mesh_id: Option<String>) -> bool {
        let config = MeshConfig::new(&self.mesh_id);
        config.matches_mesh(device_mesh_id.as_deref())
    }

    /// Get current Bluetooth state
    pub fn get_bluetooth_state(&self) -> BluetoothState {
        self.state.read().unwrap().bluetooth_state
    }

    /// Initialize and start the adapter
    pub fn start(&self) -> Result<(), HiveError> {
        let mut state = self.state.write().unwrap();
        if state.is_running {
            return Err(HiveError::AlreadyInitialized);
        }

        log::info!("Starting HiveAdapter...");

        // Create the CoreBluetooth adapter
        let adapter = CoreBluetoothAdapter::new().map_err(|e| HiveError::Internal {
            message: format!("Failed to create adapter: {}", e),
        })?;

        *self.adapter.write().unwrap() = Some(adapter);
        state.is_running = true;
        state.bluetooth_state = BluetoothState::PoweredOn;

        log::info!("HiveAdapter started successfully");
        Ok(())
    }

    /// Stop the adapter
    pub fn stop(&self) -> Result<(), HiveError> {
        let mut state = self.state.write().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }

        log::info!("Stopping HiveAdapter...");

        // Clear adapter
        *self.adapter.write().unwrap() = None;
        state.is_running = false;
        state.is_discovering = false;
        state.is_advertising = false;
        state.discovered_peers.clear();
        state.connections.clear();

        log::info!("HiveAdapter stopped");
        Ok(())
    }

    /// Check if the adapter is running
    pub fn is_running(&self) -> bool {
        self.state.read().unwrap().is_running
    }

    /// Start scanning for HIVE peers
    pub fn start_discovery(&self) -> Result<(), HiveError> {
        let mut state = self.state.write().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }

        log::info!("Starting discovery...");

        let adapter_guard = self.adapter.read().unwrap();
        if let Some(adapter) = adapter_guard.as_ref() {
            let config = DiscoveryConfig::default();
            self.runtime
                .block_on(async { adapter.start_scan(&config).await })
                .map_err(|e| HiveError::Internal {
                    message: e.to_string(),
                })?;
        }

        state.is_discovering = true;
        Ok(())
    }

    /// Stop scanning
    pub fn stop_discovery(&self) -> Result<(), HiveError> {
        let mut state = self.state.write().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }

        log::info!("Stopping discovery...");

        let adapter_guard = self.adapter.read().unwrap();
        if let Some(adapter) = adapter_guard.as_ref() {
            self.runtime
                .block_on(async { adapter.stop_scan().await })
                .map_err(|e| HiveError::Internal {
                    message: e.to_string(),
                })?;
        }

        state.is_discovering = false;
        Ok(())
    }

    /// Check if scanning is active
    pub fn is_discovering(&self) -> bool {
        self.state.read().unwrap().is_discovering
    }

    /// Start advertising as a HIVE node
    pub fn start_advertising(&self) -> Result<(), HiveError> {
        let mut state = self.state.write().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }

        log::info!("Starting advertising...");

        let adapter_guard = self.adapter.read().unwrap();
        if let Some(adapter) = adapter_guard.as_ref() {
            let config = DiscoveryConfig::default();
            self.runtime
                .block_on(async { adapter.start_advertising(&config).await })
                .map_err(|e| HiveError::Internal {
                    message: e.to_string(),
                })?;
        }

        state.is_advertising = true;
        Ok(())
    }

    /// Stop advertising
    pub fn stop_advertising(&self) -> Result<(), HiveError> {
        let mut state = self.state.write().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }

        log::info!("Stopping advertising...");

        let adapter_guard = self.adapter.read().unwrap();
        if let Some(adapter) = adapter_guard.as_ref() {
            self.runtime
                .block_on(async { adapter.stop_advertising().await })
                .map_err(|e| HiveError::Internal {
                    message: e.to_string(),
                })?;
        }

        state.is_advertising = false;
        Ok(())
    }

    /// Check if advertising is active
    pub fn is_advertising(&self) -> bool {
        self.state.read().unwrap().is_advertising
    }

    /// Connect to a discovered peer by identifier
    pub fn connect(&self, identifier: String) -> Result<(), HiveError> {
        let state = self.state.read().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }

        // Get node ID from discovered peers
        let peer = state.discovered_peers.get(&identifier).cloned();
        drop(state);

        let node_id = peer
            .and_then(|p| p.node_id)
            .ok_or_else(|| HiveError::ConnectionFailed {
                reason: "Peer not found or no node ID".to_string(),
            })?;

        log::info!("Connecting to peer: {} (node {:08X})", identifier, node_id);

        let adapter_guard = self.adapter.read().unwrap();
        if let Some(adapter) = adapter_guard.as_ref() {
            let hive_node_id = HiveNodeId::new(node_id);
            self.runtime
                .block_on(async { adapter.connect(&hive_node_id).await })
                .map_err(|e| HiveError::ConnectionFailed {
                    reason: e.to_string(),
                })?;
        }

        // Add to connections
        let mut state = self.state.write().unwrap();
        let conn_info = ConnectionInfo {
            peer_id: node_id,
            identifier: identifier.clone(),
            mtu: 247,
            phy: BlePhy::Le1M,
            rssi: None,
            is_alive: true,
        };
        state.connections.insert(node_id, conn_info.clone());

        // Notify callback
        if let Some(cb) = self.connection_callback.lock().unwrap().as_ref() {
            cb.on_connected(node_id, conn_info);
        }

        Ok(())
    }

    /// Disconnect from a peer
    pub fn disconnect(&self, peer_id: u32) -> Result<(), HiveError> {
        let mut state = self.state.write().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }

        log::info!("Disconnecting from peer: {:08X}", peer_id);

        let adapter_guard = self.adapter.read().unwrap();
        if let Some(adapter) = adapter_guard.as_ref() {
            let node_id = HiveNodeId::new(peer_id);
            self.runtime
                .block_on(async { adapter.disconnect(&node_id).await })
                .map_err(|e| HiveError::Internal {
                    message: e.to_string(),
                })?;
        }

        state.connections.remove(&peer_id);

        // Notify callback
        if let Some(cb) = self.connection_callback.lock().unwrap().as_ref() {
            cb.on_disconnected(peer_id, "User requested".to_string());
        }

        Ok(())
    }

    /// Disconnect all peers
    pub fn disconnect_all(&self) {
        let state = self.state.read().unwrap();
        let peer_ids: Vec<u32> = state.connections.keys().cloned().collect();
        drop(state);

        for peer_id in peer_ids {
            let _ = self.disconnect(peer_id);
        }
    }

    /// Get list of discovered peers
    pub fn get_discovered_peers(&self) -> Vec<DiscoveredPeer> {
        self.state
            .read()
            .unwrap()
            .discovered_peers
            .values()
            .cloned()
            .collect()
    }

    /// Get list of active connections
    pub fn get_connections(&self) -> Vec<ConnectionInfo> {
        self.state
            .read()
            .unwrap()
            .connections
            .values()
            .cloned()
            .collect()
    }

    /// Get connection info for a specific peer
    pub fn get_connection(&self, peer_id: u32) -> Option<ConnectionInfo> {
        self.state.read().unwrap().connections.get(&peer_id).cloned()
    }

    /// Send data to a connected peer
    pub fn send_data(&self, peer_id: u32, data: Vec<u8>) -> Result<(), HiveError> {
        let state = self.state.read().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }
        if !state.connections.contains_key(&peer_id) {
            return Err(HiveError::ConnectionFailed {
                reason: "Peer not connected".to_string(),
            });
        }

        log::debug!("Sending {} bytes to peer {:08X}", data.len(), peer_id);

        // TODO: Implement actual data sending via adapter
        // For now, just update stats
        drop(state);
        let mut state = self.state.write().unwrap();
        state.sync_stats.bytes_synced += data.len() as u64;

        Ok(())
    }

    /// Broadcast data to all connected peers
    pub fn broadcast_data(&self, data: Vec<u8>) -> Result<(), HiveError> {
        let state = self.state.read().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }

        let peer_ids: Vec<u32> = state.connections.keys().cloned().collect();
        drop(state);

        for peer_id in peer_ids {
            self.send_data(peer_id, data.clone())?;
        }

        Ok(())
    }

    /// Get sync statistics
    pub fn get_sync_stats(&self) -> SyncStats {
        self.state.read().unwrap().sync_stats.clone()
    }

    /// Trigger manual sync with all connected peers
    pub fn trigger_sync(&self) -> Result<(), HiveError> {
        let state = self.state.read().unwrap();
        if !state.is_running {
            return Err(HiveError::NotRunning);
        }
        if state.connections.is_empty() {
            return Err(HiveError::InvalidState);
        }

        log::info!("Triggering sync with {} peers", state.connections.len());

        // TODO: Implement actual sync via hive-btle
        drop(state);
        let mut state = self.state.write().unwrap();
        state.sync_stats.last_sync_timestamp = Some(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );

        Ok(())
    }

    /// Set callback for discovery events
    pub fn set_discovery_callback(&self, callback: Box<dyn DiscoveryCallback>) {
        *self.discovery_callback.lock().unwrap() = Some(callback);
    }

    /// Set callback for connection events
    pub fn set_connection_callback(&self, callback: Box<dyn ConnectionCallback>) {
        *self.connection_callback.lock().unwrap() = Some(callback);
    }

    /// Set callback for received data
    pub fn set_data_callback(&self, callback: Box<dyn DataCallback>) {
        *self.data_callback.lock().unwrap() = Some(callback);
    }

    /// Process pending events (call periodically from main thread)
    pub fn process_events(&self) {
        let adapter_guard = self.adapter.read().unwrap();
        if let Some(_adapter) = adapter_guard.as_ref() {
            // TODO: Process adapter events when the API is available
            // For now, just simulate some discovery for testing
        }
    }

    /// Add a mock discovered peer (for testing)
    pub fn add_mock_peer(&self, name: String, rssi: i8) {
        let node_id = rand::random::<u32>() | 0x10000000;
        let identifier = uuid::Uuid::new_v4().to_string();

        let peer = DiscoveredPeer {
            identifier: identifier.clone(),
            name: Some(name),
            rssi,
            node_id: Some(node_id),
            is_hive_node: true,
        };

        let mut state = self.state.write().unwrap();
        state.discovered_peers.insert(identifier, peer.clone());
        drop(state);

        if let Some(cb) = self.discovery_callback.lock().unwrap().as_ref() {
            cb.on_peer_discovered(peer);
        }
    }
}

// ============================================================================
// HiveMesh Bindings - Centralized Peer & Document Management
// ============================================================================

use hive_btle::hive_mesh::{HiveMesh as RustHiveMesh, HiveMeshConfig as RustHiveMeshConfig};
use hive_btle::observer::{
    DisconnectReason as RustDisconnectReason, HiveEvent as RustHiveEvent,
};
use hive_btle::peer::HivePeer as RustHivePeer;
use hive_btle::sync::crdt::PeripheralType as RustPeripheralType;

/// UniFFI-compatible peer representation
#[derive(Debug, Clone, uniffi::Record)]
pub struct MeshPeer {
    /// Unique node ID (32-bit)
    pub node_id: u32,
    /// Platform-specific identifier (CBPeripheral UUID, MAC address, etc.)
    pub identifier: String,
    /// Mesh ID this peer belongs to
    pub mesh_id: Option<String>,
    /// Display name
    pub name: Option<String>,
    /// Signal strength (RSSI in dBm)
    pub rssi: i8,
    /// Whether currently connected
    pub is_connected: bool,
    /// Last seen timestamp (milliseconds since epoch)
    pub last_seen_ms: u64,
}

impl From<RustHivePeer> for MeshPeer {
    fn from(peer: RustHivePeer) -> Self {
        MeshPeer {
            node_id: peer.node_id.as_u32(),
            identifier: peer.identifier.clone(),
            mesh_id: peer.mesh_id.clone(),
            name: peer.name.clone(),
            rssi: peer.rssi,
            is_connected: peer.is_connected,
            last_seen_ms: peer.last_seen_ms,
        }
    }
}

/// Signal strength category
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MeshSignalStrength {
    Excellent,
    Good,
    Fair,
    Weak,
}

/// Disconnect reason
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MeshDisconnectReason {
    LocalRequest,
    RemoteRequest,
    Timeout,
    LinkLoss,
    ConnectionFailed,
    Unknown,
}

impl From<RustDisconnectReason> for MeshDisconnectReason {
    fn from(reason: RustDisconnectReason) -> Self {
        match reason {
            RustDisconnectReason::LocalRequest => MeshDisconnectReason::LocalRequest,
            RustDisconnectReason::RemoteRequest => MeshDisconnectReason::RemoteRequest,
            RustDisconnectReason::Timeout => MeshDisconnectReason::Timeout,
            RustDisconnectReason::LinkLoss => MeshDisconnectReason::LinkLoss,
            RustDisconnectReason::ConnectionFailed => MeshDisconnectReason::ConnectionFailed,
            RustDisconnectReason::Unknown => MeshDisconnectReason::Unknown,
        }
    }
}

impl From<MeshDisconnectReason> for RustDisconnectReason {
    fn from(reason: MeshDisconnectReason) -> Self {
        match reason {
            MeshDisconnectReason::LocalRequest => RustDisconnectReason::LocalRequest,
            MeshDisconnectReason::RemoteRequest => RustDisconnectReason::RemoteRequest,
            MeshDisconnectReason::Timeout => RustDisconnectReason::Timeout,
            MeshDisconnectReason::LinkLoss => RustDisconnectReason::LinkLoss,
            MeshDisconnectReason::ConnectionFailed => RustDisconnectReason::ConnectionFailed,
            MeshDisconnectReason::Unknown => RustDisconnectReason::Unknown,
        }
    }
}

/// Peripheral type
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MeshPeripheralType {
    Unknown,
    SoldierSensor,
    FixedSensor,
    Relay,
}

impl From<MeshPeripheralType> for RustPeripheralType {
    fn from(t: MeshPeripheralType) -> Self {
        match t {
            MeshPeripheralType::Unknown => RustPeripheralType::Unknown,
            MeshPeripheralType::SoldierSensor => RustPeripheralType::SoldierSensor,
            MeshPeripheralType::FixedSensor => RustPeripheralType::FixedSensor,
            MeshPeripheralType::Relay => RustPeripheralType::Relay,
        }
    }
}

/// Event received from the mesh
#[derive(Debug, Clone, uniffi::Enum)]
pub enum MeshEvent {
    /// New peer discovered
    PeerDiscovered { peer: MeshPeer },
    /// Peer connected
    PeerConnected { node_id: u32 },
    /// Peer disconnected
    PeerDisconnected {
        node_id: u32,
        reason: MeshDisconnectReason,
    },
    /// Peer lost (cleanup timeout)
    PeerLost { node_id: u32 },
    /// Emergency received from peer
    EmergencyReceived { from_node: u32 },
    /// ACK received from peer
    AckReceived { from_node: u32 },
    /// Document synced with peer
    DocumentSynced { from_node: u32, total_count: u64 },
    /// Mesh state changed (peer count, etc.)
    MeshStateChanged { peer_count: u32, connected_count: u32 },
}

impl From<RustHiveEvent> for MeshEvent {
    fn from(event: RustHiveEvent) -> Self {
        match event {
            RustHiveEvent::PeerDiscovered { peer } => MeshEvent::PeerDiscovered {
                peer: peer.into(),
            },
            RustHiveEvent::PeerConnected { node_id } => MeshEvent::PeerConnected {
                node_id: node_id.as_u32(),
            },
            RustHiveEvent::PeerDisconnected { node_id, reason } => MeshEvent::PeerDisconnected {
                node_id: node_id.as_u32(),
                reason: reason.into(),
            },
            RustHiveEvent::PeerLost { node_id } => MeshEvent::PeerLost {
                node_id: node_id.as_u32(),
            },
            RustHiveEvent::EmergencyReceived { from_node } => MeshEvent::EmergencyReceived {
                from_node: from_node.as_u32(),
            },
            RustHiveEvent::AckReceived { from_node } => MeshEvent::AckReceived {
                from_node: from_node.as_u32(),
            },
            RustHiveEvent::DocumentSynced {
                from_node,
                total_count,
            } => MeshEvent::DocumentSynced {
                from_node: from_node.as_u32(),
                total_count,
            },
            RustHiveEvent::MeshStateChanged {
                peer_count,
                connected_count,
            } => MeshEvent::MeshStateChanged {
                peer_count: peer_count as u32,
                connected_count: connected_count as u32,
            },
            // Handle any other events by converting to MeshStateChanged with 0s
            _ => MeshEvent::MeshStateChanged {
                peer_count: 0,
                connected_count: 0,
            },
        }
    }
}

/// Result of receiving BLE data
#[derive(Debug, Clone, uniffi::Record)]
pub struct DataReceivedResult {
    /// Node ID of sender
    pub source_node: u32,
    /// Whether document contained an emergency event
    pub is_emergency: bool,
    /// Whether document contained an ACK event
    pub is_ack: bool,
    /// Whether counter changed (new data)
    pub counter_changed: bool,
    /// Total counter value after merge
    pub total_count: u64,
}

/// Callback interface for mesh events
#[uniffi::export(callback_interface)]
pub trait MeshEventCallback: Send + Sync {
    fn on_event(&self, event: MeshEvent);
}

/// Observer that forwards events to Swift callback
struct SwiftMeshObserver {
    callback: Box<dyn MeshEventCallback>,
}

impl hive_btle::observer::HiveObserver for SwiftMeshObserver {
    fn on_event(&self, event: RustHiveEvent) {
        self.callback.on_event(event.into());
    }
}

/// Main HiveMesh wrapper for UniFFI
#[derive(uniffi::Object)]
pub struct HiveMeshWrapper {
    mesh: RustHiveMesh,
}

#[uniffi::export]
impl HiveMeshWrapper {
    /// Create a new HiveMesh with the given configuration
    #[uniffi::constructor]
    pub fn new(
        node_id: u32,
        callsign: String,
        mesh_id: String,
        peripheral_type: MeshPeripheralType,
    ) -> Arc<Self> {
        let config = RustHiveMeshConfig::new(HiveNodeId::new(node_id), &callsign, &mesh_id)
            .with_peripheral_type(peripheral_type.into());

        Arc::new(HiveMeshWrapper {
            mesh: RustHiveMesh::new(config),
        })
    }

    /// Create with default configuration
    #[uniffi::constructor]
    pub fn with_defaults(node_id: u32, callsign: String) -> Arc<Self> {
        let config = RustHiveMeshConfig::new(
            HiveNodeId::new(node_id),
            &callsign,
            DEFAULT_MESH_ID,
        );

        Arc::new(HiveMeshWrapper {
            mesh: RustHiveMesh::new(config),
        })
    }

    /// Get local node ID
    pub fn node_id(&self) -> u32 {
        self.mesh.node_id().as_u32()
    }

    /// Get mesh ID
    pub fn mesh_id(&self) -> String {
        self.mesh.mesh_id().to_string()
    }

    /// Get device name for BLE advertising
    pub fn device_name(&self) -> String {
        self.mesh.device_name()
    }

    /// Add an event observer
    pub fn add_observer(&self, callback: Box<dyn MeshEventCallback>) {
        let observer = SwiftMeshObserver { callback };
        self.mesh.add_observer(Arc::new(observer));
    }

    // ==================== User Actions ====================

    /// Send an emergency event
    /// Returns the document bytes to broadcast via BLE
    pub fn send_emergency(&self, timestamp: u64) -> Vec<u8> {
        self.mesh.send_emergency(timestamp)
    }

    /// Send an ACK event
    /// Returns the document bytes to broadcast via BLE
    pub fn send_ack(&self, timestamp: u64) -> Vec<u8> {
        self.mesh.send_ack(timestamp)
    }

    /// Clear the current event
    pub fn clear_event(&self) {
        self.mesh.clear_event();
    }

    /// Build current document for sync
    pub fn build_document(&self) -> Vec<u8> {
        self.mesh.build_document()
    }

    // ==================== BLE Callbacks ====================

    /// Called when a BLE device is discovered
    /// Returns the peer if it's a valid HIVE peer, None otherwise
    pub fn on_ble_discovered(
        &self,
        identifier: String,
        name: Option<String>,
        rssi: i8,
        mesh_id: Option<String>,
        now_ms: u64,
    ) -> Option<MeshPeer> {
        self.mesh
            .on_ble_discovered(&identifier, name.as_deref(), rssi, mesh_id.as_deref(), now_ms)
            .map(|p| p.into())
    }

    /// Called when a BLE connection is established
    pub fn on_ble_connected(&self, identifier: String, now_ms: u64) -> Option<u32> {
        self.mesh
            .on_ble_connected(&identifier, now_ms)
            .map(|id| id.as_u32())
    }

    /// Called when a BLE connection is lost
    pub fn on_ble_disconnected(
        &self,
        identifier: String,
        reason: MeshDisconnectReason,
    ) -> Option<u32> {
        self.mesh
            .on_ble_disconnected(&identifier, reason.into())
            .map(|id| id.as_u32())
    }

    /// Called when a remote device connects to us (incoming connection)
    pub fn on_incoming_connection(
        &self,
        identifier: String,
        node_id: u32,
        now_ms: u64,
    ) -> bool {
        self.mesh
            .on_incoming_connection(&identifier, HiveNodeId::new(node_id), now_ms)
    }

    /// Called when BLE data is received from a peer
    /// Returns merge result if successful
    pub fn on_ble_data_received(
        &self,
        identifier: String,
        data: Vec<u8>,
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        self.mesh
            .on_ble_data_received(&identifier, &data, now_ms)
            .map(|result| DataReceivedResult {
                source_node: result.source_node.as_u32(),
                is_emergency: result.is_emergency,
                is_ack: result.is_ack,
                counter_changed: result.counter_changed,
                total_count: result.total_count,
            })
    }

    // ==================== Periodic Maintenance ====================

    /// Call periodically to perform maintenance tasks
    /// Returns document bytes if a sync broadcast is needed
    pub fn tick(&self, now_ms: u64) -> Option<Vec<u8>> {
        self.mesh.tick(now_ms)
    }

    // ==================== State Queries ====================

    /// Get all known peers
    pub fn get_peers(&self) -> Vec<MeshPeer> {
        self.mesh.get_peers().into_iter().map(|p| p.into()).collect()
    }

    /// Get connected peers only
    pub fn get_connected_peers(&self) -> Vec<MeshPeer> {
        self.mesh
            .get_connected_peers()
            .into_iter()
            .map(|p| p.into())
            .collect()
    }

    /// Get peer count
    pub fn peer_count(&self) -> u32 {
        self.mesh.peer_count() as u32
    }

    /// Get connected peer count
    pub fn connected_count(&self) -> u32 {
        self.mesh.connected_count() as u32
    }

    /// Get total counter value
    pub fn total_count(&self) -> u64 {
        self.mesh.total_count()
    }

    /// Check if emergency is currently active
    pub fn is_emergency_active(&self) -> bool {
        self.mesh.is_emergency_active()
    }

    /// Check if ACK is currently active
    pub fn is_ack_active(&self) -> bool {
        self.mesh.is_ack_active()
    }

    /// Check if a device matches our mesh
    pub fn matches_mesh(&self, device_mesh_id: Option<String>) -> bool {
        self.mesh.matches_mesh(device_mesh_id.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_adapter() {
        init_logging();
        let adapter = HiveAdapter::new(0xDEADBEEF).unwrap();
        assert_eq!(adapter.get_node_id(), 0xDEADBEEF);
        assert!(!adapter.is_running());
    }

    #[test]
    fn test_sync_stats_default() {
        let stats = SyncStats::default();
        assert_eq!(stats.document_count, 0);
        assert_eq!(stats.bytes_synced, 0);
        assert!(stats.last_sync_timestamp.is_none());
    }

    #[test]
    fn test_hive_mesh_wrapper() {
        let mesh = HiveMeshWrapper::with_defaults(0x12345678, "TEST".to_string());
        assert_eq!(mesh.node_id(), 0x12345678);
        assert_eq!(mesh.mesh_id(), "DEMO");
        assert_eq!(mesh.peer_count(), 0);
    }
}
