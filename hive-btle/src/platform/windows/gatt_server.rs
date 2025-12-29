//! GATT server for Windows
//!
//! Hosts the HIVE BLE service using `GattServiceProvider`.
//! Requires Windows 10 version 1803 or later.

use std::sync::{Arc, Mutex};

use windows::core::GUID;
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristicProperties, GattLocalCharacteristic, GattLocalCharacteristicParameters,
    GattLocalService, GattProtectionLevel, GattServiceProvider,
    GattServiceProviderAdvertisementStatus, GattServiceProviderAdvertisingParameters,
};
use windows::Foundation::TypedEventHandler;

use crate::error::{BleError, Result};
use crate::NodeId;

/// HIVE Service UUID
const HIVE_SERVICE_UUID: GUID = GUID::from_values(
    0xf47ac10b,
    0x58cc,
    0x4372,
    [0xa5, 0x67, 0x0e, 0x02, 0xb2, 0xc3, 0xd4, 0x79],
);

/// Node Info Characteristic UUID
const CHAR_NODE_INFO_UUID: GUID = GUID::from_values(
    0xf47ac10b,
    0x58cc,
    0x4372,
    [0xa5, 0x67, 0x0e, 0x02, 0xb2, 0xc3, 0xd0, 0x01],
);

/// Sync State Characteristic UUID
const CHAR_SYNC_STATE_UUID: GUID = GUID::from_values(
    0xf47ac10b,
    0x58cc,
    0x4372,
    [0xa5, 0x67, 0x0e, 0x02, 0xb2, 0xc3, 0xd0, 0x02],
);

/// Sync Data Characteristic UUID
const CHAR_SYNC_DATA_UUID: GUID = GUID::from_values(
    0xf47ac10b,
    0x58cc,
    0x4372,
    [0xa5, 0x67, 0x0e, 0x02, 0xb2, 0xc3, 0xd0, 0x03],
);

/// Internal state for the GATT server
struct GattServerState {
    /// Our node ID
    node_id: NodeId,
    /// Current document data to serve
    document_data: Vec<u8>,
    /// Callback for when data is written to us
    write_callback: Option<Box<dyn Fn(Vec<u8>) + Send + Sync>>,
}

/// GATT server hosting the HIVE service
pub struct GattServer {
    /// Service provider (owns the service)
    provider: Option<GattServiceProvider>,
    /// The local service
    service: Option<GattLocalService>,
    /// Node info characteristic
    node_info_char: Option<GattLocalCharacteristic>,
    /// Sync state characteristic
    sync_state_char: Option<GattLocalCharacteristic>,
    /// Sync data characteristic
    sync_data_char: Option<GattLocalCharacteristic>,
    /// Internal state
    state: Arc<Mutex<GattServerState>>,
    /// Whether the server is advertising
    is_advertising: bool,
}

impl GattServer {
    /// Create a new GATT server
    pub fn new(node_id: NodeId) -> Result<Self> {
        Ok(Self {
            provider: None,
            service: None,
            node_info_char: None,
            sync_state_char: None,
            sync_data_char: None,
            state: Arc::new(Mutex::new(GattServerState {
                node_id,
                document_data: Vec::new(),
                write_callback: None,
            })),
            is_advertising: false,
        })
    }

    /// Initialize the GATT service (blocking)
    ///
    /// This creates the service provider and characteristics.
    /// Requires Windows 10 1803+.
    pub fn init_sync(&mut self) -> Result<()> {
        // Create the service provider
        let create_op = GattServiceProvider::CreateAsync(HIVE_SERVICE_UUID).map_err(|e| {
            BleError::GattError(format!("Failed to create service provider: {}", e))
        })?;

        let provider_result = create_op
            .get()
            .map_err(|e| BleError::GattError(format!("Service provider creation failed: {}", e)))?;

        let provider = provider_result
            .ServiceProvider()
            .map_err(|e| BleError::GattError(format!("Failed to get provider: {}", e)))?;

        let service = provider
            .Service()
            .map_err(|e| BleError::GattError(format!("Failed to get service: {}", e)))?;

        // Create Node Info characteristic (read-only)
        let node_info_char = self.create_characteristic_sync(
            &service,
            CHAR_NODE_INFO_UUID,
            GattCharacteristicProperties::Read,
        )?;

        // Create Sync State characteristic (read + notify)
        let sync_state_char = self.create_characteristic_sync(
            &service,
            CHAR_SYNC_STATE_UUID,
            GattCharacteristicProperties::Read | GattCharacteristicProperties::Notify,
        )?;

        // Create Sync Data characteristic (read + write + notify)
        let sync_data_char = self.create_characteristic_sync(
            &service,
            CHAR_SYNC_DATA_UUID,
            GattCharacteristicProperties::Read
                | GattCharacteristicProperties::Write
                | GattCharacteristicProperties::Notify,
        )?;

        // Set up write handler for sync data
        self.setup_write_handler(&sync_data_char)?;

        self.provider = Some(provider);
        self.service = Some(service);
        self.node_info_char = Some(node_info_char);
        self.sync_state_char = Some(sync_state_char);
        self.sync_data_char = Some(sync_data_char);

        log::info!("GATT server initialized");
        Ok(())
    }

    /// Initialize the GATT service (async wrapper)
    pub async fn init(&mut self) -> Result<()> {
        // Run blocking init in a spawn_blocking task
        // Note: We need to use a different approach since self is mutable
        // For now, just call the sync version directly
        self.init_sync()
    }

    /// Create a characteristic with the given properties (blocking)
    fn create_characteristic_sync(
        &self,
        service: &GattLocalService,
        uuid: GUID,
        properties: GattCharacteristicProperties,
    ) -> Result<GattLocalCharacteristic> {
        let params = GattLocalCharacteristicParameters::new()
            .map_err(|e| BleError::GattError(format!("Failed to create params: {}", e)))?;

        params
            .SetCharacteristicProperties(properties)
            .map_err(|e| BleError::GattError(format!("Failed to set properties: {}", e)))?;

        params
            .SetReadProtectionLevel(GattProtectionLevel::Plain)
            .map_err(|e| BleError::GattError(format!("Failed to set read protection: {}", e)))?;

        params
            .SetWriteProtectionLevel(GattProtectionLevel::Plain)
            .map_err(|e| BleError::GattError(format!("Failed to set write protection: {}", e)))?;

        let create_op = service
            .CreateCharacteristicAsync(uuid, &params)
            .map_err(|e| BleError::GattError(format!("Failed to create characteristic: {}", e)))?;

        let char_result = create_op
            .get()
            .map_err(|e| BleError::GattError(format!("Characteristic creation failed: {}", e)))?;

        let char = char_result
            .Characteristic()
            .map_err(|e| BleError::GattError(format!("Failed to get characteristic: {}", e)))?;

        Ok(char)
    }

    /// Set up the write request handler for sync data characteristic
    fn setup_write_handler(&self, char: &GattLocalCharacteristic) -> Result<()> {
        let _state = self.state.clone();

        let handler = TypedEventHandler::new(
            move |_char,
                  args: &Option<
                windows::Devices::Bluetooth::GenericAttributeProfile::GattWriteRequestedEventArgs,
            >| {
                if let Some(args) = args {
                    // Get the write request
                    if let Ok(deferral) = args.GetDeferral() {
                        // Note: In a real implementation, we'd process the write here
                        log::debug!("Received write request on sync data characteristic");
                        deferral.Complete().ok();
                    }
                }
                Ok(())
            },
        );

        char.WriteRequested(&handler)
            .map_err(|e| BleError::GattError(format!("Failed to set write handler: {}", e)))?;

        Ok(())
    }

    /// Start advertising the GATT service
    pub fn start_advertising(&mut self) -> Result<()> {
        if self.is_advertising {
            return Ok(());
        }

        let provider = self
            .provider
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Server not initialized".to_string()))?;

        let params = GattServiceProviderAdvertisingParameters::new().map_err(|e| {
            BleError::GattError(format!("Failed to create advertising params: {}", e))
        })?;

        // Make the service discoverable and connectable
        params
            .SetIsDiscoverable(true)
            .map_err(|e| BleError::GattError(format!("Failed to set discoverable: {}", e)))?;

        params
            .SetIsConnectable(true)
            .map_err(|e| BleError::GattError(format!("Failed to set connectable: {}", e)))?;

        provider
            .StartAdvertisingWithParameters(&params)
            .map_err(|e| BleError::GattError(format!("Failed to start advertising: {}", e)))?;

        self.is_advertising = true;
        log::info!("GATT server advertising started");

        Ok(())
    }

    /// Stop advertising the GATT service
    pub fn stop_advertising(&mut self) -> Result<()> {
        if !self.is_advertising {
            return Ok(());
        }

        if let Some(provider) = &self.provider {
            provider
                .StopAdvertising()
                .map_err(|e| BleError::GattError(format!("Failed to stop advertising: {}", e)))?;
        }

        self.is_advertising = false;
        log::info!("GATT server advertising stopped");

        Ok(())
    }

    /// Update the document data to serve
    pub fn set_document_data(&self, data: Vec<u8>) {
        if let Ok(mut state) = self.state.lock() {
            state.document_data = data;
        }
    }

    /// Set callback for when data is written to us
    pub fn set_write_callback(&self, callback: Box<dyn Fn(Vec<u8>) + Send + Sync>) {
        if let Ok(mut state) = self.state.lock() {
            state.write_callback = Some(callback);
        }
    }

    /// Get the advertising status
    pub fn advertising_status(&self) -> Result<GattServiceProviderAdvertisementStatus> {
        let provider = self
            .provider
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Server not initialized".to_string()))?;

        provider
            .AdvertisementStatus()
            .map_err(|e| BleError::GattError(format!("Failed to get status: {}", e)))
    }

    /// Check if the server is advertising
    pub fn is_advertising(&self) -> bool {
        self.is_advertising
    }
}

impl Drop for GattServer {
    fn drop(&mut self) {
        let _ = self.stop_advertising();
    }
}
