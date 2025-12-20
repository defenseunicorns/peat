//! CBCentralManager wrapper
//!
//! This module provides a Rust wrapper around CoreBluetooth's CBCentralManager,
//! which is used for scanning and connecting to BLE peripherals (GATT client role).

use std::collections::HashMap;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2_core_bluetooth::{CBCentralManager, CBPeripheral};
use tokio::sync::{mpsc, RwLock};

use crate::config::DiscoveryConfig;
use crate::error::{BleError, Result};
use crate::NodeId;

use super::delegates::{CentralEvent, CentralState, RustCentralManagerDelegate};

/// Wrapper around CBCentralManager for BLE scanning and connecting
///
/// CBCentralManager is the central role in CoreBluetooth, used to:
/// - Scan for BLE peripherals
/// - Connect to peripherals
/// - Discover services and characteristics
/// - Read/write characteristic values
///
/// # Safety
/// This type is marked `Send + Sync` because CoreBluetooth callbacks are
/// dispatched on the main queue and the manager is only accessed from async
/// tasks that ensure proper synchronization. The underlying CBCentralManager
/// and CBPeripheralManager are not inherently thread-safe, but our usage pattern
/// (single async context + main queue dispatch) ensures safety.
pub struct CentralManager {
    /// The actual CBCentralManager instance
    manager: Retained<CBCentralManager>,
    /// The delegate that receives callbacks
    delegate: Retained<RustCentralManagerDelegate>,
    /// Current state of the central manager
    state: Arc<RwLock<CentralState>>,
    /// Channel receiver for delegate events
    event_rx: Arc<RwLock<mpsc::Receiver<CentralEvent>>>,
    /// Known peripherals by identifier
    peripherals: Arc<RwLock<HashMap<String, PeripheralInfo>>>,
    /// Whether scanning is active
    scanning: Arc<RwLock<bool>>,
}

/// Information about a discovered peripheral
#[derive(Debug, Clone)]
pub struct PeripheralInfo {
    /// Peripheral identifier (UUID)
    pub identifier: String,
    /// Advertised name
    pub name: Option<String>,
    /// Last seen RSSI
    pub rssi: i8,
    /// Is this a HIVE node
    pub is_hive_node: bool,
    /// Node ID if HIVE node
    pub node_id: Option<NodeId>,
    /// Whether currently connected
    pub connected: bool,
}

// SAFETY: CentralManager uses interior mutability via Arc<RwLock<_>> for all
// mutable state. The CBCentralManager is only accessed from the async context
// and its callbacks are dispatched on the main queue. We ensure that all
// access to the Objective-C objects goes through the proper synchronization.
unsafe impl Send for CentralManager {}
unsafe impl Sync for CentralManager {}

impl CentralManager {
    /// Create a new CentralManager
    ///
    /// This initializes the CBCentralManager with default options.
    /// The manager won't be ready until `state` becomes `PoweredOn`.
    pub fn new() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(100);

        // Create delegate
        let delegate = RustCentralManagerDelegate::new(event_tx);

        // Create CBCentralManager and set delegate
        // Using main queue for callbacks
        let manager = unsafe { CBCentralManager::new() };
        unsafe {
            manager.setDelegate(Some(delegate.as_protocol()));
        }

        log::info!("CBCentralManager initialized");

        Ok(Self {
            manager,
            delegate,
            state: Arc::new(RwLock::new(CentralState::Unknown)),
            event_rx: Arc::new(RwLock::new(event_rx)),
            peripherals: Arc::new(RwLock::new(HashMap::new())),
            scanning: Arc::new(RwLock::new(false)),
        })
    }

    /// Get the current central manager state
    pub async fn state(&self) -> CentralState {
        *self.state.read().await
    }

    /// Wait for the central manager to be ready (powered on)
    ///
    /// Returns an error if Bluetooth is unavailable or unauthorized.
    pub async fn wait_ready(&self) -> Result<()> {
        // Process events until state changes to a terminal state
        loop {
            self.process_events().await?;

            let state = self.state().await;
            match state {
                CentralState::PoweredOn => return Ok(()),
                CentralState::Unsupported => {
                    return Err(BleError::NotSupported(
                        "Bluetooth not supported".to_string(),
                    ))
                }
                CentralState::Unauthorized => {
                    return Err(BleError::PlatformError(
                        "Bluetooth not authorized".to_string(),
                    ))
                }
                CentralState::PoweredOff => {
                    return Err(BleError::PlatformError(
                        "Bluetooth is powered off".to_string(),
                    ))
                }
                CentralState::Unknown | CentralState::Resetting => {
                    // Wait a bit and try again
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Start scanning for BLE peripherals
    ///
    /// # Arguments
    /// * `config` - Discovery configuration
    /// * `service_uuids` - Optional list of service UUIDs to filter by
    pub async fn start_scan(
        &self,
        _config: &DiscoveryConfig,
        _service_uuids: Option<Vec<String>>,
    ) -> Result<()> {
        // Scan with no service filter and allow duplicates for RSSI updates
        // TODO: Create proper options dictionary based on config
        unsafe {
            self.manager
                .scanForPeripheralsWithServices_options(None, None);
        }

        log::info!("Started BLE scanning");
        *self.scanning.write().await = true;
        Ok(())
    }

    /// Stop scanning for peripherals
    pub async fn stop_scan(&self) -> Result<()> {
        unsafe {
            self.manager.stopScan();
        }

        log::info!("Stopped BLE scanning");
        *self.scanning.write().await = false;
        Ok(())
    }

    /// Check if currently scanning
    pub async fn is_scanning(&self) -> bool {
        *self.scanning.read().await
    }

    /// Connect to a peripheral by identifier
    ///
    /// # Arguments
    /// * `identifier` - The peripheral's UUID identifier
    pub async fn connect(&self, identifier: &str) -> Result<()> {
        // Get the CBPeripheral from delegate's storage
        let peripheral = self.delegate.get_peripheral(identifier).ok_or_else(|| {
            BleError::ConnectionFailed(format!("Unknown peripheral: {}", identifier))
        })?;

        // Connect without specific options
        unsafe {
            self.manager.connectPeripheral_options(&peripheral, None);
        }

        log::info!("Connecting to peripheral: {}", identifier);
        Ok(())
    }

    /// Disconnect from a peripheral
    pub async fn disconnect(&self, identifier: &str) -> Result<()> {
        // Get the CBPeripheral from delegate's storage
        if let Some(peripheral) = self.delegate.get_peripheral(identifier) {
            unsafe {
                self.manager.cancelPeripheralConnection(&peripheral);
            }
            log::info!("Disconnecting from peripheral: {}", identifier);
        }

        Ok(())
    }

    /// Get the CBPeripheral for an identifier
    pub fn get_cb_peripheral(&self, identifier: &str) -> Option<Retained<CBPeripheral>> {
        self.delegate.get_peripheral(identifier)
    }

    /// Get information about a discovered peripheral
    pub async fn get_peripheral(&self, identifier: &str) -> Option<PeripheralInfo> {
        let peripherals = self.peripherals.read().await;
        peripherals.get(identifier).cloned()
    }

    /// Get all discovered peripherals
    pub async fn get_discovered_peripherals(&self) -> Vec<PeripheralInfo> {
        let peripherals = self.peripherals.read().await;
        peripherals.values().cloned().collect()
    }

    /// Get all HIVE node peripherals
    pub async fn get_hive_peripherals(&self) -> Vec<PeripheralInfo> {
        let peripherals = self.peripherals.read().await;
        peripherals
            .values()
            .filter(|p| p.is_hive_node)
            .cloned()
            .collect()
    }

    /// Process pending delegate events
    ///
    /// Call this periodically to update internal state from delegate callbacks.
    pub async fn process_events(&self) -> Result<()> {
        let mut event_rx = self.event_rx.write().await;

        while let Ok(event) = event_rx.try_recv() {
            match event {
                CentralEvent::StateChanged(state) => {
                    log::debug!("Central state changed: {:?}", state);
                    *self.state.write().await = state;
                }
                CentralEvent::DiscoveredPeripheral {
                    identifier,
                    name,
                    rssi,
                    is_hive_node,
                    node_id,
                    ..
                } => {
                    let mut peripherals = self.peripherals.write().await;
                    peripherals.insert(
                        identifier.clone(),
                        PeripheralInfo {
                            identifier,
                            name,
                            rssi,
                            is_hive_node,
                            node_id,
                            connected: false,
                        },
                    );
                }
                CentralEvent::Connected { identifier } => {
                    log::info!("Connected to peripheral: {}", identifier);
                    let mut peripherals = self.peripherals.write().await;
                    if let Some(peripheral) = peripherals.get_mut(&identifier) {
                        peripheral.connected = true;
                    }
                }
                CentralEvent::Disconnected { identifier, .. } => {
                    log::info!("Disconnected from peripheral: {}", identifier);
                    let mut peripherals = self.peripherals.write().await;
                    if let Some(peripheral) = peripherals.get_mut(&identifier) {
                        peripheral.connected = false;
                    }
                }
                CentralEvent::ConnectionFailed { identifier, error } => {
                    log::warn!("Connection to {} failed: {}", identifier, error);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peripheral_info() {
        let info = PeripheralInfo {
            identifier: "12345678-1234-1234-1234-123456789ABC".to_string(),
            name: Some("HIVE-DEADBEEF".to_string()),
            rssi: -65,
            is_hive_node: true,
            node_id: Some(NodeId::new(0xDEADBEEF)),
            connected: false,
        };

        assert!(info.is_hive_node);
        assert!(!info.connected);
        assert_eq!(info.rssi, -65);
    }
}
