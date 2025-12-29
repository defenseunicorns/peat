//! BLE connection and GATT client for Windows
//!
//! Wraps `BluetoothLEDevice` and GATT operations.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::Devices::Bluetooth::BluetoothLEDevice;
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristic, GattCommunicationStatus, GattDeviceService, GattWriteOption,
};
use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};

use crate::config::BlePhy;
use crate::error::{BleError, Result};
use crate::transport::BleConnection;
use crate::NodeId;

/// Helper to convert Windows errors to BleError
fn win_err(msg: &str) -> impl Fn(windows::core::Error) -> BleError + '_ {
    move |e| BleError::PlatformError(format!("{}: {}", msg, e))
}

/// BLE connection wrapper for Windows
#[derive(Clone)]
pub struct WinRtConnection {
    /// Peer node ID
    node_id: NodeId,
    /// Bluetooth address
    address: u64,
    /// The BLE device (wrapped in Arc for cloning)
    device: Arc<Option<BluetoothLEDevice>>,
    /// The HIVE GATT service
    service: Arc<Option<GattDeviceService>>,
    /// The sync data characteristic
    sync_char: Arc<Option<GattCharacteristic>>,
    /// Connection MTU (wrapped in Arc for Clone)
    mtu: Arc<AtomicU16>,
    /// Whether connected
    connected: Arc<std::sync::atomic::AtomicBool>,
    /// When connection was established
    connected_at: Arc<std::sync::Mutex<Option<Instant>>>,
}

impl WinRtConnection {
    /// Create a new connection (not yet connected)
    pub fn new(node_id: NodeId, address: u64) -> Self {
        Self {
            node_id,
            address,
            device: Arc::new(None),
            service: Arc::new(None),
            sync_char: Arc::new(None),
            mtu: Arc::new(AtomicU16::new(23)), // Default BLE MTU
            connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            connected_at: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Connect to the device (blocking)
    pub fn connect_sync(&mut self) -> Result<()> {
        // Get the BLE device from address
        let async_op = BluetoothLEDevice::FromBluetoothAddressAsync(self.address)
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to get device: {}", e)))?;

        let device_result = async_op
            .get()
            .map_err(|e| BleError::ConnectionFailed(format!("Device connection failed: {}", e)))?;

        // Get GATT services
        let services_op = device_result
            .GetGattServicesAsync()
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to get services: {}", e)))?;

        let services_result = services_op
            .get()
            .map_err(|e| BleError::ConnectionFailed(format!("Service discovery failed: {}", e)))?;

        let status = services_result
            .Status()
            .map_err(win_err("Failed to get status"))?;
        if status != GattCommunicationStatus::Success {
            return Err(BleError::ConnectionFailed(
                "GATT service discovery failed".to_string(),
            ));
        }

        // Find HIVE service
        let services = services_result
            .Services()
            .map_err(win_err("Failed to get services"))?;
        let mut hive_service: Option<GattDeviceService> = None;

        let count = services.Size().map_err(win_err("Failed to get count"))?;
        for i in 0..count {
            let service = services
                .GetAt(i)
                .map_err(win_err("Failed to get service"))?;
            let uuid = service.Uuid().map_err(win_err("Failed to get UUID"))?;
            let uuid_str = format!("{:?}", uuid).to_lowercase();

            if uuid_str.contains("f47ac10b-58cc-4372-a567-0e02b2c3d479") {
                hive_service = Some(service);
                break;
            }
        }

        let service = hive_service.ok_or_else(|| {
            BleError::ServiceNotFound("HIVE service not found on device".to_string())
        })?;

        // Get characteristics
        let chars_op = service.GetCharacteristicsAsync().map_err(|e| {
            BleError::ConnectionFailed(format!("Failed to get characteristics: {}", e))
        })?;

        let chars_result = chars_op.get().map_err(|e| {
            BleError::ConnectionFailed(format!("Characteristic discovery failed: {}", e))
        })?;

        let char_status = chars_result
            .Status()
            .map_err(win_err("Failed to get char status"))?;
        if char_status != GattCommunicationStatus::Success {
            return Err(BleError::ConnectionFailed(
                "GATT characteristic discovery failed".to_string(),
            ));
        }

        // Find sync data characteristic
        let chars = chars_result
            .Characteristics()
            .map_err(win_err("Failed to get chars"))?;
        let mut sync_char: Option<GattCharacteristic> = None;

        let char_count = chars.Size().map_err(win_err("Failed to get char count"))?;
        for i in 0..char_count {
            let char = chars.GetAt(i).map_err(win_err("Failed to get char"))?;
            let uuid = char.Uuid().map_err(win_err("Failed to get char UUID"))?;
            let uuid_str = format!("{:?}", uuid).to_lowercase();

            // Sync data characteristic ends with d003
            if uuid_str.contains("0e02b2c3d003") {
                sync_char = Some(char);
                break;
            }
        }

        // Store everything
        self.device = Arc::new(Some(device_result));
        self.service = Arc::new(Some(service));
        self.sync_char = Arc::new(sync_char);
        self.connected
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // Record connection time
        if let Ok(mut connected_at) = self.connected_at.lock() {
            *connected_at = Some(Instant::now());
        }

        log::info!(
            "Connected to node {:08X} at {:012X}",
            self.node_id.as_u32(),
            self.address
        );

        Ok(())
    }

    /// Connect to the device (async wrapper)
    pub async fn connect(&mut self) -> Result<()> {
        // Run blocking connect in a spawn_blocking task
        let mut this = self.clone();
        tokio::task::spawn_blocking(move || this.connect_sync())
            .await
            .map_err(|e| BleError::ConnectionFailed(format!("Task join failed: {}", e)))?
    }

    /// Disconnect from the device
    pub fn disconnect(&mut self) {
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.device = Arc::new(None);
        self.service = Arc::new(None);
        self.sync_char = Arc::new(None);

        log::info!("Disconnected from node {:08X}", self.node_id.as_u32());
    }

    /// Read data from the sync characteristic (blocking)
    pub fn read_sync_data_blocking(&self) -> Result<Vec<u8>> {
        let char = self
            .sync_char
            .as_ref()
            .as_ref()
            .ok_or_else(|| BleError::ConnectionLost("Not connected".to_string()))?;

        let read_op = char
            .ReadValueAsync()
            .map_err(|e| BleError::GattError(format!("Failed to read: {}", e)))?;

        let result = read_op
            .get()
            .map_err(|e| BleError::GattError(format!("Read failed: {}", e)))?;

        if result
            .Status()
            .map_err(win_err("Failed to get read status"))?
            != GattCommunicationStatus::Success
        {
            return Err(BleError::GattError(
                "Read failed with error status".to_string(),
            ));
        }

        let buffer = result
            .Value()
            .map_err(win_err("Failed to get read value"))?;
        let reader = DataReader::FromBuffer(&buffer).map_err(win_err("Failed to create reader"))?;
        let len = reader
            .UnconsumedBufferLength()
            .map_err(win_err("Failed to get buffer length"))? as usize;
        let mut data = vec![0u8; len];
        reader
            .ReadBytes(&mut data)
            .map_err(win_err("Failed to read bytes"))?;

        Ok(data)
    }

    /// Read data from the sync characteristic (async)
    pub async fn read_sync_data(&self) -> Result<Vec<u8>> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.read_sync_data_blocking())
            .await
            .map_err(|e| BleError::GattError(format!("Task join failed: {}", e)))?
    }

    /// Write data to the sync characteristic (blocking)
    pub fn write_sync_data_blocking(&self, data: &[u8]) -> Result<()> {
        let char = self
            .sync_char
            .as_ref()
            .as_ref()
            .ok_or_else(|| BleError::ConnectionLost("Not connected".to_string()))?;

        // Create buffer
        let stream = InMemoryRandomAccessStream::new()
            .map_err(|e| BleError::GattError(format!("Failed to create stream: {}", e)))?;
        let writer = DataWriter::CreateDataWriter(&stream)
            .map_err(|e| BleError::GattError(format!("Failed to create writer: {}", e)))?;

        writer
            .WriteBytes(data)
            .map_err(|e| BleError::GattError(format!("Failed to write bytes: {}", e)))?;

        let buffer = writer
            .DetachBuffer()
            .map_err(|e| BleError::GattError(format!("Failed to detach buffer: {}", e)))?;

        let write_op = char
            .WriteValueWithOptionAsync(&buffer, GattWriteOption::WriteWithResponse)
            .map_err(|e| BleError::GattError(format!("Failed to write: {}", e)))?;

        let result = write_op
            .get()
            .map_err(|e| BleError::GattError(format!("Write failed: {}", e)))?;

        if result != GattCommunicationStatus::Success {
            return Err(BleError::GattError(
                "Write failed with error status".to_string(),
            ));
        }

        Ok(())
    }

    /// Write data to the sync characteristic (async)
    pub async fn write_sync_data(&self, data: &[u8]) -> Result<()> {
        let this = self.clone();
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || this.write_sync_data_blocking(&data))
            .await
            .map_err(|e| BleError::GattError(format!("Task join failed: {}", e)))?
    }
}

impl BleConnection for WinRtConnection {
    fn peer_id(&self) -> &NodeId {
        &self.node_id
    }

    fn is_alive(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn mtu(&self) -> u16 {
        self.mtu.load(Ordering::Relaxed)
    }

    fn phy(&self) -> BlePhy {
        // Windows doesn't expose PHY selection to applications
        BlePhy::Le1M
    }

    fn rssi(&self) -> Option<i8> {
        // Would need to query the device for current RSSI
        None
    }

    fn connected_duration(&self) -> Duration {
        if let Ok(connected_at) = self.connected_at.lock() {
            if let Some(start) = *connected_at {
                return start.elapsed();
            }
        }
        Duration::ZERO
    }
}

impl std::fmt::Debug for WinRtConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinRtConnection")
            .field("node_id", &self.node_id)
            .field("address", &format!("{:012X}", self.address))
            .field("connected", &self.connected.load(Ordering::Relaxed))
            .field("mtu", &self.mtu.load(Ordering::Relaxed))
            .finish()
    }
}
