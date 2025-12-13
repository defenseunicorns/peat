//! BlueZ connection wrapper

use bluer::Device;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::config::BlePhy;
use crate::error::{BleError, Result};
use crate::transport::BleConnection;
use crate::NodeId;

/// Internal connection state
struct ConnectionState {
    /// Whether the connection is alive
    alive: bool,
    /// Negotiated MTU
    mtu: u16,
    /// Current PHY
    phy: BlePhy,
    /// Last RSSI reading
    rssi: Option<i8>,
}

/// BlueZ connection wrapper
///
/// Wraps a `bluer::Device` with connection state tracking.
#[derive(Clone)]
pub struct BluerConnection {
    /// Remote peer ID
    peer_id: NodeId,
    /// BlueZ device handle
    device: Device,
    /// Connection state
    state: Arc<RwLock<ConnectionState>>,
    /// When the connection was established
    connected_at: Instant,
}

impl BluerConnection {
    /// Create a new connection wrapper
    pub(crate) async fn new(peer_id: NodeId, device: Device) -> Result<Self> {
        // Get initial MTU
        // BlueZ doesn't expose MTU directly, use default
        let mtu = 23; // Will be updated after MTU exchange

        let state = ConnectionState {
            alive: true,
            mtu,
            phy: BlePhy::Le1M, // Default PHY
            rssi: None,
        };

        let conn = Self {
            peer_id,
            device,
            state: Arc::new(RwLock::new(state)),
            connected_at: Instant::now(),
        };

        // Try to get initial RSSI
        conn.update_rssi().await;

        Ok(conn)
    }

    /// Get the underlying BlueZ device
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Update RSSI from device
    pub async fn update_rssi(&self) {
        if let Ok(Some(rssi)) = self.device.rssi().await {
            let mut state = self.state.write().await;
            state.rssi = Some(rssi as i8);
        }
    }

    /// Update MTU
    pub async fn set_mtu(&self, mtu: u16) {
        let mut state = self.state.write().await;
        state.mtu = mtu;
    }

    /// Update PHY
    pub async fn set_phy(&self, phy: BlePhy) {
        let mut state = self.state.write().await;
        state.phy = phy;
    }

    /// Mark connection as dead
    pub async fn mark_dead(&self) {
        let mut state = self.state.write().await;
        state.alive = false;
    }

    /// Disconnect from the device
    pub async fn disconnect(&self) -> Result<()> {
        self.device
            .disconnect()
            .await
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to disconnect: {}", e)))?;
        self.mark_dead().await;
        Ok(())
    }

    /// Discover GATT services
    pub async fn discover_services(&self) -> Result<()> {
        // Trigger service discovery
        // In bluer, services are discovered automatically on connect
        // but we can force a refresh
        let _ = self.device.services().await;
        Ok(())
    }

    /// Get GATT services
    pub async fn services(&self) -> Result<Vec<bluer::gatt::remote::Service>> {
        self.device
            .services()
            .await
            .map_err(|e| BleError::GattError(format!("Failed to get services: {}", e)))
    }

    /// Find a service by UUID
    pub async fn find_service(
        &self,
        uuid: uuid::Uuid,
    ) -> Result<Option<bluer::gatt::remote::Service>> {
        let services = self.services().await?;
        for service in services {
            if service.uuid().await.ok() == Some(uuid) {
                return Ok(Some(service));
            }
        }
        Ok(None)
    }

    /// Read a characteristic value
    pub async fn read_characteristic(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
    ) -> Result<Vec<u8>> {
        let service = self
            .find_service(service_uuid)
            .await?
            .ok_or_else(|| BleError::ServiceNotFound(service_uuid.to_string()))?;

        let characteristics = service
            .characteristics()
            .await
            .map_err(|e| BleError::GattError(format!("Failed to get characteristics: {}", e)))?;

        for char in characteristics {
            if char.uuid().await.ok() == Some(char_uuid) {
                return char.read().await.map_err(|e| {
                    BleError::GattError(format!("Failed to read characteristic: {}", e))
                });
            }
        }

        Err(BleError::CharacteristicNotFound(char_uuid.to_string()))
    }

    /// Write a characteristic value
    pub async fn write_characteristic(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
        value: &[u8],
    ) -> Result<()> {
        let service = self
            .find_service(service_uuid)
            .await?
            .ok_or_else(|| BleError::ServiceNotFound(service_uuid.to_string()))?;

        let characteristics = service
            .characteristics()
            .await
            .map_err(|e| BleError::GattError(format!("Failed to get characteristics: {}", e)))?;

        for char in characteristics {
            if char.uuid().await.ok() == Some(char_uuid) {
                return char.write(value).await.map_err(|e| {
                    BleError::GattError(format!("Failed to write characteristic: {}", e))
                });
            }
        }

        Err(BleError::CharacteristicNotFound(char_uuid.to_string()))
    }

    /// Subscribe to characteristic notifications
    pub async fn subscribe_characteristic(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
    ) -> Result<impl tokio_stream::Stream<Item = Vec<u8>>> {
        let service = self
            .find_service(service_uuid)
            .await?
            .ok_or_else(|| BleError::ServiceNotFound(service_uuid.to_string()))?;

        let characteristics = service
            .characteristics()
            .await
            .map_err(|e| BleError::GattError(format!("Failed to get characteristics: {}", e)))?;

        for char in characteristics {
            if char.uuid().await.ok() == Some(char_uuid) {
                return char.notify().await.map_err(|e| {
                    BleError::GattError(format!("Failed to subscribe to notifications: {}", e))
                });
            }
        }

        Err(BleError::CharacteristicNotFound(char_uuid.to_string()))
    }
}

impl BleConnection for BluerConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        // Try to read state without blocking
        if let Ok(state) = self.state.try_read() {
            state.alive
        } else {
            // If we can't get the lock, assume alive
            true
        }
    }

    fn mtu(&self) -> u16 {
        if let Ok(state) = self.state.try_read() {
            state.mtu
        } else {
            23 // Default BLE MTU
        }
    }

    fn phy(&self) -> BlePhy {
        if let Ok(state) = self.state.try_read() {
            state.phy
        } else {
            BlePhy::Le1M
        }
    }

    fn rssi(&self) -> Option<i8> {
        if let Ok(state) = self.state.try_read() {
            state.rssi
        } else {
            None
        }
    }

    fn connected_duration(&self) -> Duration {
        self.connected_at.elapsed()
    }
}

#[cfg(test)]
mod tests {
    // Integration tests require actual Bluetooth hardware
    // and a connected device
}
