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
}
