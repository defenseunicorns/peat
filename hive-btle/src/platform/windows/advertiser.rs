//! BLE advertisement publisher for Windows
//!
//! Wraps `BluetoothLEAdvertisementPublisher` for advertising.

use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementDataSection, BluetoothLEAdvertisementPublisher,
    BluetoothLEAdvertisementPublisherStatus, BluetoothLEManufacturerData,
};
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

use crate::config::DiscoveryConfig;
use crate::discovery::HiveBeacon;
use crate::error::{BleError, Result};
use crate::NodeId;

/// BLE advertiser using Windows Advertisement Publisher
pub struct BleAdvertiser {
    /// The WinRT publisher
    publisher: BluetoothLEAdvertisementPublisher,
    /// Whether currently advertising
    is_advertising: bool,
    /// Our node ID
    node_id: Option<NodeId>,
}

impl BleAdvertiser {
    /// Create a new BLE advertiser
    pub fn new() -> Result<Self> {
        let publisher = BluetoothLEAdvertisementPublisher::new()
            .map_err(|e| BleError::PlatformError(format!("Failed to create publisher: {}", e)))?;

        Ok(Self {
            publisher,
            is_advertising: false,
            node_id: None,
        })
    }

    /// Start advertising
    pub fn start_advertising(&mut self, node_id: NodeId, _config: &DiscoveryConfig) -> Result<()> {
        if self.is_advertising {
            return Ok(());
        }

        self.node_id = Some(node_id);

        // Get the advertisement object
        let advertisement = self
            .publisher
            .Advertisement()
            .map_err(|e| BleError::PlatformError(format!("Failed to get advertisement: {}", e)))?;

        // Clear any existing data
        if let Ok(sections) = advertisement.DataSections() {
            sections.Clear().ok();
        }
        if let Ok(manufacturer_data) = advertisement.ManufacturerData() {
            manufacturer_data.Clear().ok();
        }

        // Add HIVE service UUID (16-bit short form in advertisement)
        // The 16-bit UUID 0xF47A is encoded as data type 0x03 (Complete List of 16-bit UUIDs)
        if let Ok(data_sections) = advertisement.DataSections() {
            if let Ok(section) = Self::create_service_uuid_section() {
                data_sections.Append(&section).ok();
            }
        }

        // Add manufacturer-specific data with HIVE beacon
        if let Ok(manufacturer_data) = advertisement.ManufacturerData() {
            if let Ok(data) = Self::create_hive_beacon_data(node_id) {
                manufacturer_data.Append(&data).ok();
            }
        }

        // Start the publisher
        self.publisher
            .Start()
            .map_err(|e| BleError::PlatformError(format!("Failed to start publisher: {}", e)))?;

        self.is_advertising = true;
        log::info!("BLE advertising started for node {:08X}", node_id.as_u32());

        Ok(())
    }

    /// Stop advertising
    pub fn stop_advertising(&mut self) -> Result<()> {
        if !self.is_advertising {
            return Ok(());
        }

        self.publisher
            .Stop()
            .map_err(|e| BleError::PlatformError(format!("Failed to stop publisher: {}", e)))?;

        self.is_advertising = false;
        log::info!("BLE advertising stopped");

        Ok(())
    }

    /// Check if currently advertising
    pub fn is_advertising(&self) -> bool {
        self.is_advertising
    }

    /// Get the current publisher status
    pub fn status(&self) -> Result<BluetoothLEAdvertisementPublisherStatus> {
        self.publisher
            .Status()
            .map_err(|e| BleError::PlatformError(format!("Failed to get status: {}", e)))
    }

    /// Create a data section for the 16-bit HIVE service UUID
    fn create_service_uuid_section() -> Result<BluetoothLEAdvertisementDataSection> {
        let section = BluetoothLEAdvertisementDataSection::new()
            .map_err(|e| BleError::PlatformError(format!("Failed to create section: {}", e)))?;

        // Data type 0x03 = Complete List of 16-bit Service UUIDs
        section
            .SetDataType(0x03)
            .map_err(|e| BleError::PlatformError(format!("Failed to set data type: {}", e)))?;

        // Create buffer with 16-bit UUID (little-endian)
        let stream = InMemoryRandomAccessStream::new()
            .map_err(|e| BleError::PlatformError(format!("Failed to create stream: {}", e)))?;
        let writer = DataWriter::CreateDataWriter(&stream)
            .map_err(|e| BleError::PlatformError(format!("Failed to create writer: {}", e)))?;

        // HIVE service UUID 16-bit: 0xF47A (little-endian: 0x7A, 0xF4)
        writer
            .WriteBytes(&[0x7A, 0xF4])
            .map_err(|e| BleError::PlatformError(format!("Failed to write UUID: {}", e)))?;

        let buffer = writer
            .DetachBuffer()
            .map_err(|e| BleError::PlatformError(format!("Failed to detach buffer: {}", e)))?;

        section
            .SetData(&buffer)
            .map_err(|e| BleError::PlatformError(format!("Failed to set data: {}", e)))?;

        Ok(section)
    }

    /// Create manufacturer-specific data with HIVE beacon
    fn create_hive_beacon_data(node_id: NodeId) -> Result<BluetoothLEManufacturerData> {
        let data = BluetoothLEManufacturerData::new().map_err(|e| {
            BleError::PlatformError(format!("Failed to create manufacturer data: {}", e))
        })?;

        // Use 0xFFFF for development (would use registered company ID in production)
        data.SetCompanyId(0xFFFF)
            .map_err(|e| BleError::PlatformError(format!("Failed to set company ID: {}", e)))?;

        // Create HIVE beacon
        let beacon = HiveBeacon::new(node_id);
        let beacon_bytes = beacon.encode();

        // Create buffer with beacon data
        let stream = InMemoryRandomAccessStream::new()
            .map_err(|e| BleError::PlatformError(format!("Failed to create stream: {}", e)))?;
        let writer = DataWriter::CreateDataWriter(&stream)
            .map_err(|e| BleError::PlatformError(format!("Failed to create writer: {}", e)))?;

        writer
            .WriteBytes(&beacon_bytes)
            .map_err(|e| BleError::PlatformError(format!("Failed to write beacon: {}", e)))?;

        let buffer = writer
            .DetachBuffer()
            .map_err(|e| BleError::PlatformError(format!("Failed to detach buffer: {}", e)))?;

        data.SetData(&buffer)
            .map_err(|e| BleError::PlatformError(format!("Failed to set data: {}", e)))?;

        Ok(data)
    }
}

impl Drop for BleAdvertiser {
    fn drop(&mut self) {
        let _ = self.stop_advertising();
    }
}
