//! HIVE GATT Service Implementation
//!
//! Provides the GATT service structure and handlers for HIVE Protocol BLE communication.

use std::sync::{Arc, RwLock};

use crate::error::Result;
use crate::{HierarchyLevel, NodeId, HIVE_SERVICE_UUID};

use super::characteristics::{
    CharacteristicProperties, Command, CommandType, HiveCharacteristicUuids, NodeInfo, StatusData,
    StatusFlags, SyncDataHeader, SyncDataOp, SyncState, SyncStateData,
};

/// GATT service event handler callback type
pub type GattEventCallback = Box<dyn Fn(GattEvent) + Send + Sync>;

/// Events emitted by the GATT service
#[derive(Debug, Clone)]
pub enum GattEvent {
    /// Client connected
    ClientConnected {
        /// Client address
        address: String,
    },
    /// Client disconnected
    ClientDisconnected {
        /// Client address
        address: String,
    },
    /// Client subscribed to notifications
    NotificationSubscribed {
        /// Characteristic name
        characteristic: String,
    },
    /// Client unsubscribed from notifications
    NotificationUnsubscribed {
        /// Characteristic name
        characteristic: String,
    },
    /// Command received
    CommandReceived {
        /// Command that was received
        command: CommandType,
        /// Command payload
        payload: Vec<u8>,
    },
    /// Sync data received
    SyncDataReceived {
        /// Sync data header
        header: SyncDataHeader,
        /// Sync data payload
        payload: Vec<u8>,
    },
    /// MTU changed
    MtuChanged {
        /// New MTU value
        mtu: u16,
    },
}

/// GATT characteristic descriptor
#[derive(Debug, Clone)]
pub struct CharacteristicDescriptor {
    /// Characteristic UUID
    pub uuid: uuid::Uuid,
    /// Human-readable name
    pub name: &'static str,
    /// Properties (read, write, notify, etc.)
    pub properties: CharacteristicProperties,
    /// Whether encryption is required
    pub encrypted: bool,
}

/// Characteristic definitions for HIVE GATT service
pub struct HiveCharacteristics;

impl HiveCharacteristics {
    /// Node Info characteristic descriptor
    pub fn node_info() -> CharacteristicDescriptor {
        CharacteristicDescriptor {
            uuid: HiveCharacteristicUuids::node_info(),
            name: "Node Info",
            properties: CharacteristicProperties::new(CharacteristicProperties::READ),
            encrypted: true,
        }
    }

    /// Sync State characteristic descriptor
    pub fn sync_state() -> CharacteristicDescriptor {
        CharacteristicDescriptor {
            uuid: HiveCharacteristicUuids::sync_state(),
            name: "Sync State",
            properties: CharacteristicProperties::new(
                CharacteristicProperties::READ | CharacteristicProperties::NOTIFY,
            ),
            encrypted: true,
        }
    }

    /// Sync Data characteristic descriptor
    pub fn sync_data() -> CharacteristicDescriptor {
        CharacteristicDescriptor {
            uuid: HiveCharacteristicUuids::sync_data(),
            name: "Sync Data",
            properties: CharacteristicProperties::new(
                CharacteristicProperties::WRITE | CharacteristicProperties::INDICATE,
            ),
            encrypted: true,
        }
    }

    /// Command characteristic descriptor
    pub fn command() -> CharacteristicDescriptor {
        CharacteristicDescriptor {
            uuid: HiveCharacteristicUuids::command(),
            name: "Command",
            properties: CharacteristicProperties::new(CharacteristicProperties::WRITE),
            encrypted: true,
        }
    }

    /// Status characteristic descriptor
    pub fn status() -> CharacteristicDescriptor {
        CharacteristicDescriptor {
            uuid: HiveCharacteristicUuids::status(),
            name: "Status",
            properties: CharacteristicProperties::new(
                CharacteristicProperties::READ | CharacteristicProperties::NOTIFY,
            ),
            encrypted: true,
        }
    }

    /// Get all characteristic descriptors
    pub fn all() -> Vec<CharacteristicDescriptor> {
        vec![
            Self::node_info(),
            Self::sync_state(),
            Self::sync_data(),
            Self::command(),
            Self::status(),
        ]
    }
}

/// Internal state for the GATT service
struct ServiceState {
    /// Node information
    node_info: NodeInfo,
    /// Current sync state
    sync_state: SyncStateData,
    /// Current status
    status: StatusData,
    /// Connected clients (addresses)
    connected_clients: Vec<String>,
    /// Clients subscribed to sync state notifications
    sync_state_subscribers: Vec<String>,
    /// Clients subscribed to status notifications
    status_subscribers: Vec<String>,
    /// Negotiated MTU
    mtu: u16,
}

impl ServiceState {
    fn new(node_id: NodeId, hierarchy_level: HierarchyLevel, capabilities: u16) -> Self {
        Self {
            node_info: NodeInfo::new(node_id, hierarchy_level, capabilities),
            sync_state: SyncStateData::new(SyncState::Idle),
            status: StatusData::new(),
            connected_clients: Vec::new(),
            sync_state_subscribers: Vec::new(),
            status_subscribers: Vec::new(),
            mtu: 23, // Default BLE MTU
        }
    }
}

/// HIVE GATT Service
///
/// Manages the GATT service lifecycle and provides handlers for characteristic operations.
pub struct HiveGattService {
    /// Service UUID
    pub uuid: uuid::Uuid,
    /// Internal state
    state: Arc<RwLock<ServiceState>>,
    /// Event callback
    #[allow(dead_code)]
    event_callback: Option<GattEventCallback>,
}

impl HiveGattService {
    /// Create a new HIVE GATT service
    pub fn new(node_id: NodeId, hierarchy_level: HierarchyLevel, capabilities: u16) -> Self {
        Self {
            uuid: HIVE_SERVICE_UUID,
            state: Arc::new(RwLock::new(ServiceState::new(
                node_id,
                hierarchy_level,
                capabilities,
            ))),
            event_callback: None,
        }
    }

    /// Set event callback
    pub fn set_event_callback(&mut self, callback: GattEventCallback) {
        self.event_callback = Some(callback);
    }

    /// Get service UUID
    pub fn service_uuid(&self) -> uuid::Uuid {
        self.uuid
    }

    /// Get all characteristic descriptors
    pub fn characteristics(&self) -> Vec<CharacteristicDescriptor> {
        HiveCharacteristics::all()
    }

    // === Read Handlers ===

    /// Handle Node Info read request
    pub fn read_node_info(&self) -> Vec<u8> {
        let state = self.state.read().unwrap();
        state.node_info.encode().to_vec()
    }

    /// Handle Sync State read request
    pub fn read_sync_state(&self) -> Vec<u8> {
        let state = self.state.read().unwrap();
        state.sync_state.encode().to_vec()
    }

    /// Handle Status read request
    pub fn read_status(&self) -> Vec<u8> {
        let state = self.state.read().unwrap();
        state.status.encode().to_vec()
    }

    // === Write Handlers ===

    /// Handle Sync Data write request
    pub fn write_sync_data(&self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        let header = SyncDataHeader::decode(data).ok_or_else(|| {
            crate::error::BleError::GattError("Invalid sync data header".to_string())
        })?;

        let payload = if data.len() > SyncDataHeader::SIZE {
            data[SyncDataHeader::SIZE..].to_vec()
        } else {
            Vec::new()
        };

        // Process based on operation type
        match header.op {
            SyncDataOp::Document => {
                // Update sync state to syncing
                let mut state = self.state.write().unwrap();
                state.sync_state.state = SyncState::Syncing;
                state.status.flags =
                    StatusFlags::new(state.status.flags.flags() | StatusFlags::SYNCING);

                // Return acknowledgement
                let ack = SyncDataHeader::new(SyncDataOp::Ack, header.seq);
                Ok(Some(ack.encode().to_vec()))
            }
            SyncDataOp::Vector => {
                // Sync vector update
                let ack = SyncDataHeader::new(SyncDataOp::Ack, header.seq);
                Ok(Some(ack.encode().to_vec()))
            }
            SyncDataOp::End => {
                // Sync complete
                let mut state = self.state.write().unwrap();
                state.sync_state.state = SyncState::Complete;
                state.sync_state.progress = 100;
                state.status.flags =
                    StatusFlags::new(state.status.flags.flags() & !StatusFlags::SYNCING);

                // Emit event if callback set
                if let Some(ref callback) = self.event_callback {
                    callback(GattEvent::SyncDataReceived { header, payload });
                }

                Ok(None)
            }
            SyncDataOp::Ack => {
                // Acknowledgement received (shouldn't happen on write)
                Ok(None)
            }
        }
    }

    /// Handle Command write request
    pub fn write_command(&self, data: &[u8]) -> Result<()> {
        let command = Command::decode(data)
            .ok_or_else(|| crate::error::BleError::GattError("Invalid command data".to_string()))?;

        match command.cmd_type {
            CommandType::StartSync => {
                let mut state = self.state.write().unwrap();
                state.sync_state.state = SyncState::Syncing;
                state.sync_state.progress = 0;
            }
            CommandType::StopSync => {
                let mut state = self.state.write().unwrap();
                state.sync_state.state = SyncState::Idle;
            }
            CommandType::RefreshInfo => {
                // Trigger info refresh (no-op for now)
            }
            CommandType::SetHierarchy => {
                if !command.payload.is_empty() {
                    let mut state = self.state.write().unwrap();
                    state.node_info.hierarchy_level = HierarchyLevel::from(command.payload[0]);
                }
            }
            CommandType::Ping => {
                // Keepalive - no action needed
            }
            CommandType::Reset => {
                let mut state = self.state.write().unwrap();
                state.sync_state = SyncStateData::new(SyncState::Idle);
            }
        }

        // Emit event if callback set
        if let Some(ref callback) = self.event_callback {
            callback(GattEvent::CommandReceived {
                command: command.cmd_type,
                payload: command.payload,
            });
        }

        Ok(())
    }

    // === State Updates ===

    /// Update battery percentage
    pub fn update_battery(&self, percent: u8) {
        let mut state = self.state.write().unwrap();
        state.node_info.battery_percent = percent.min(100);

        // Update low battery flag
        if percent < 20 {
            state.status.flags =
                StatusFlags::new(state.status.flags.flags() | StatusFlags::LOW_BATTERY);
        } else {
            state.status.flags =
                StatusFlags::new(state.status.flags.flags() & !StatusFlags::LOW_BATTERY);
        }
    }

    /// Update hierarchy level
    pub fn update_hierarchy_level(&self, level: HierarchyLevel) {
        let mut state = self.state.write().unwrap();
        state.node_info.hierarchy_level = level;
    }

    /// Update sync progress
    pub fn update_sync_progress(&self, progress: u8, pending_docs: u16) {
        let mut state = self.state.write().unwrap();
        state.sync_state.progress = progress.min(100);
        state.sync_state.pending_docs = pending_docs;

        if progress >= 100 {
            state.sync_state.state = SyncState::Complete;
        }
    }

    /// Update parent connection status
    pub fn update_parent_status(&self, connected: bool, rssi: Option<i8>) {
        let mut state = self.state.write().unwrap();

        if connected {
            state.status.flags =
                StatusFlags::new(state.status.flags.flags() | StatusFlags::CONNECTED);
            state.status.parent_rssi = rssi.unwrap_or(0);
        } else {
            state.status.flags =
                StatusFlags::new(state.status.flags.flags() & !StatusFlags::CONNECTED);
            state.status.parent_rssi = 127; // No parent
        }
    }

    /// Update child count
    pub fn update_child_count(&self, count: u8) {
        let mut state = self.state.write().unwrap();
        state.status.child_count = count;
    }

    /// Update uptime
    pub fn update_uptime(&self, minutes: u16) {
        let mut state = self.state.write().unwrap();
        state.status.uptime_minutes = minutes;
    }

    // === Connection Management ===

    /// Handle client connection
    pub fn on_client_connected(&self, address: String) {
        let mut state = self.state.write().unwrap();
        if !state.connected_clients.contains(&address) {
            state.connected_clients.push(address.clone());
        }

        if let Some(ref callback) = self.event_callback {
            callback(GattEvent::ClientConnected { address });
        }
    }

    /// Handle client disconnection
    pub fn on_client_disconnected(&self, address: &str) {
        let mut state = self.state.write().unwrap();
        state.connected_clients.retain(|a| a != address);
        state.sync_state_subscribers.retain(|a| a != address);
        state.status_subscribers.retain(|a| a != address);

        if let Some(ref callback) = self.event_callback {
            callback(GattEvent::ClientDisconnected {
                address: address.to_string(),
            });
        }
    }

    /// Handle notification subscription
    pub fn on_subscribe(&self, address: String, characteristic: &str) {
        let mut state = self.state.write().unwrap();

        match characteristic {
            "sync_state" => {
                if !state.sync_state_subscribers.contains(&address) {
                    state.sync_state_subscribers.push(address);
                }
            }
            "status" => {
                if !state.status_subscribers.contains(&address) {
                    state.status_subscribers.push(address);
                }
            }
            _ => {}
        }

        if let Some(ref callback) = self.event_callback {
            callback(GattEvent::NotificationSubscribed {
                characteristic: characteristic.to_string(),
            });
        }
    }

    /// Handle MTU change
    pub fn on_mtu_changed(&self, mtu: u16) {
        let mut state = self.state.write().unwrap();
        state.mtu = mtu;

        if let Some(ref callback) = self.event_callback {
            callback(GattEvent::MtuChanged { mtu });
        }
    }

    /// Get current MTU
    pub fn mtu(&self) -> u16 {
        let state = self.state.read().unwrap();
        state.mtu
    }

    /// Get connected client count
    pub fn connected_client_count(&self) -> usize {
        let state = self.state.read().unwrap();
        state.connected_clients.len()
    }

    /// Get list of addresses subscribed to sync state
    pub fn sync_state_subscribers(&self) -> Vec<String> {
        let state = self.state.read().unwrap();
        state.sync_state_subscribers.clone()
    }

    /// Get list of addresses subscribed to status
    pub fn status_subscribers(&self) -> Vec<String> {
        let state = self.state.read().unwrap();
        state.status_subscribers.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities;

    #[test]
    fn test_gatt_service_creation() {
        let service = HiveGattService::new(
            NodeId::new(0x12345678),
            HierarchyLevel::Squad,
            capabilities::CAN_RELAY,
        );

        assert_eq!(service.service_uuid(), HIVE_SERVICE_UUID);
        assert_eq!(service.characteristics().len(), 5);
    }

    #[test]
    fn test_read_node_info() {
        let service = HiveGattService::new(
            NodeId::new(0x12345678),
            HierarchyLevel::Squad,
            capabilities::CAN_RELAY,
        );

        let data = service.read_node_info();
        assert_eq!(data.len(), NodeInfo::ENCODED_SIZE);

        let info = NodeInfo::decode(&data).unwrap();
        assert_eq!(info.node_id, NodeId::new(0x12345678));
        assert_eq!(info.hierarchy_level, HierarchyLevel::Squad);
    }

    #[test]
    fn test_write_command() {
        let service = HiveGattService::new(NodeId::new(0x12345678), HierarchyLevel::Platform, 0);

        // Set hierarchy command
        let cmd = Command::with_payload(CommandType::SetHierarchy, vec![2]); // Platoon
        service.write_command(&cmd.encode()).unwrap();

        let data = service.read_node_info();
        let info = NodeInfo::decode(&data).unwrap();
        assert_eq!(info.hierarchy_level, HierarchyLevel::Platoon);
    }

    #[test]
    fn test_sync_data_flow() {
        let service = HiveGattService::new(NodeId::new(0x12345678), HierarchyLevel::Platform, 0);

        // Start sync
        let cmd = Command::new(CommandType::StartSync);
        service.write_command(&cmd.encode()).unwrap();

        // Check sync state
        let state_data = service.read_sync_state();
        let state = SyncStateData::decode(&state_data).unwrap();
        assert_eq!(state.state, SyncState::Syncing);

        // Send document
        let mut header = SyncDataHeader::new(SyncDataOp::Document, 1);
        let mut data = header.encode().to_vec();
        data.extend_from_slice(b"test document data");

        let response = service.write_sync_data(&data).unwrap();
        assert!(response.is_some()); // Should get ACK

        // End sync
        header = SyncDataHeader::new(SyncDataOp::End, 2);
        service.write_sync_data(&header.encode()).unwrap();

        // Check sync complete
        let state_data = service.read_sync_state();
        let state = SyncStateData::decode(&state_data).unwrap();
        assert_eq!(state.state, SyncState::Complete);
    }

    #[test]
    fn test_battery_update() {
        let service = HiveGattService::new(NodeId::new(0x12345678), HierarchyLevel::Platform, 0);

        service.update_battery(15);

        let data = service.read_node_info();
        let info = NodeInfo::decode(&data).unwrap();
        assert_eq!(info.battery_percent, 15);

        let status_data = service.read_status();
        let status = StatusData::decode(&status_data).unwrap();
        assert!(status.flags.is_low_battery());
    }

    #[test]
    fn test_client_connection() {
        let service = HiveGattService::new(NodeId::new(0x12345678), HierarchyLevel::Platform, 0);

        service.on_client_connected("AA:BB:CC:DD:EE:FF".to_string());
        assert_eq!(service.connected_client_count(), 1);

        service.on_client_disconnected("AA:BB:CC:DD:EE:FF");
        assert_eq!(service.connected_client_count(), 0);
    }

    #[test]
    fn test_mtu_negotiation() {
        let service = HiveGattService::new(NodeId::new(0x12345678), HierarchyLevel::Platform, 0);

        assert_eq!(service.mtu(), 23); // Default

        service.on_mtu_changed(251);
        assert_eq!(service.mtu(), 251);
    }

    #[test]
    fn test_hive_characteristics() {
        let chars = HiveCharacteristics::all();
        assert_eq!(chars.len(), 5);

        let node_info = HiveCharacteristics::node_info();
        assert!(node_info.properties.can_read());
        assert!(!node_info.properties.can_write());

        let sync_data = HiveCharacteristics::sync_data();
        assert!(sync_data.properties.can_write());
        assert!(sync_data.properties.can_indicate());
    }
}
