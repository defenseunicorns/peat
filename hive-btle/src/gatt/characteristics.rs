//! HIVE GATT Characteristic Definitions
//!
//! Defines the characteristics exposed by the HIVE GATT service.

use uuid::Uuid;

use crate::{
    HierarchyLevel, NodeId, CHAR_COMMAND_UUID, CHAR_NODE_INFO_UUID, CHAR_STATUS_UUID,
    CHAR_SYNC_DATA_UUID, CHAR_SYNC_STATE_UUID, HIVE_SERVICE_UUID,
};

/// Characteristic properties bitfield
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacteristicProperties(u8);

impl CharacteristicProperties {
    /// Characteristic supports reading
    pub const READ: u8 = 0x02;
    /// Characteristic supports writing without response
    pub const WRITE_WITHOUT_RESPONSE: u8 = 0x04;
    /// Characteristic supports writing with response
    pub const WRITE: u8 = 0x08;
    /// Characteristic supports notifications
    pub const NOTIFY: u8 = 0x10;
    /// Characteristic supports indications
    pub const INDICATE: u8 = 0x20;

    /// Create new properties
    pub const fn new(flags: u8) -> Self {
        Self(flags)
    }

    /// Check if read is supported
    pub fn can_read(&self) -> bool {
        self.0 & Self::READ != 0
    }

    /// Check if write is supported
    pub fn can_write(&self) -> bool {
        self.0 & Self::WRITE != 0
    }

    /// Check if notifications are supported
    pub fn can_notify(&self) -> bool {
        self.0 & Self::NOTIFY != 0
    }

    /// Check if indications are supported
    pub fn can_indicate(&self) -> bool {
        self.0 & Self::INDICATE != 0
    }

    /// Get raw flags
    pub fn flags(&self) -> u8 {
        self.0
    }
}

/// HIVE characteristic UUIDs derived from base service UUID
pub struct HiveCharacteristicUuids;

impl HiveCharacteristicUuids {
    /// Get Node Info characteristic UUID
    pub fn node_info() -> Uuid {
        Self::derive_uuid(CHAR_NODE_INFO_UUID)
    }

    /// Get Sync State characteristic UUID
    pub fn sync_state() -> Uuid {
        Self::derive_uuid(CHAR_SYNC_STATE_UUID)
    }

    /// Get Sync Data characteristic UUID
    pub fn sync_data() -> Uuid {
        Self::derive_uuid(CHAR_SYNC_DATA_UUID)
    }

    /// Get Command characteristic UUID
    pub fn command() -> Uuid {
        Self::derive_uuid(CHAR_COMMAND_UUID)
    }

    /// Get Status characteristic UUID
    pub fn status() -> Uuid {
        Self::derive_uuid(CHAR_STATUS_UUID)
    }

    /// Derive a characteristic UUID from the base service UUID
    ///
    /// Uses the standard BLE approach of modifying the 3rd and 4th bytes
    /// of the base UUID with the 16-bit characteristic ID.
    fn derive_uuid(char_id: u16) -> Uuid {
        let mut bytes = HIVE_SERVICE_UUID.as_bytes().to_owned();
        bytes[2] = (char_id >> 8) as u8;
        bytes[3] = char_id as u8;
        Uuid::from_bytes(bytes)
    }
}

/// Node Info characteristic data
///
/// Read-only characteristic containing basic node information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeInfo {
    /// Node identifier
    pub node_id: NodeId,
    /// Protocol version
    pub protocol_version: u8,
    /// Hierarchy level
    pub hierarchy_level: HierarchyLevel,
    /// Capability flags
    pub capabilities: u16,
    /// Battery percentage (0-100, 255 = unknown)
    pub battery_percent: u8,
}

impl NodeInfo {
    /// Encoded size in bytes
    pub const ENCODED_SIZE: usize = 9;

    /// Create new node info
    pub fn new(node_id: NodeId, hierarchy_level: HierarchyLevel, capabilities: u16) -> Self {
        Self {
            node_id,
            protocol_version: 1,
            hierarchy_level,
            capabilities,
            battery_percent: 255,
        }
    }

    /// Encode to bytes
    pub fn encode(&self) -> [u8; Self::ENCODED_SIZE] {
        let mut buf = [0u8; Self::ENCODED_SIZE];
        let node_id = self.node_id.as_u32();

        buf[0] = (node_id >> 24) as u8;
        buf[1] = (node_id >> 16) as u8;
        buf[2] = (node_id >> 8) as u8;
        buf[3] = node_id as u8;
        buf[4] = self.protocol_version;
        buf[5] = self.hierarchy_level.into();
        buf[6] = (self.capabilities >> 8) as u8;
        buf[7] = self.capabilities as u8;
        buf[8] = self.battery_percent;

        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < Self::ENCODED_SIZE {
            return None;
        }

        let node_id = NodeId::new(
            ((data[0] as u32) << 24)
                | ((data[1] as u32) << 16)
                | ((data[2] as u32) << 8)
                | (data[3] as u32),
        );
        let protocol_version = data[4];
        let hierarchy_level = HierarchyLevel::from(data[5]);
        let capabilities = ((data[6] as u16) << 8) | (data[7] as u16);
        let battery_percent = data[8];

        Some(Self {
            node_id,
            protocol_version,
            hierarchy_level,
            capabilities,
            battery_percent,
        })
    }
}

/// Sync state values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SyncState {
    /// Not syncing
    #[default]
    Idle = 0,
    /// Sync in progress
    Syncing = 1,
    /// Sync complete
    Complete = 2,
    /// Sync error
    Error = 3,
}

impl From<u8> for SyncState {
    fn from(value: u8) -> Self {
        match value {
            0 => SyncState::Idle,
            1 => SyncState::Syncing,
            2 => SyncState::Complete,
            3 => SyncState::Error,
            _ => SyncState::Idle,
        }
    }
}

/// Sync State characteristic data
///
/// Read/Notify characteristic for sync status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStateData {
    /// Current sync state
    pub state: SyncState,
    /// Sync progress (0-100)
    pub progress: u8,
    /// Number of pending documents
    pub pending_docs: u16,
    /// Last sync timestamp (Unix seconds, truncated to 32 bits)
    pub last_sync: u32,
}

impl SyncStateData {
    /// Encoded size in bytes
    pub const ENCODED_SIZE: usize = 8;

    /// Create new sync state data
    pub fn new(state: SyncState) -> Self {
        Self {
            state,
            progress: 0,
            pending_docs: 0,
            last_sync: 0,
        }
    }

    /// Encode to bytes
    pub fn encode(&self) -> [u8; Self::ENCODED_SIZE] {
        let mut buf = [0u8; Self::ENCODED_SIZE];
        buf[0] = self.state as u8;
        buf[1] = self.progress;
        buf[2] = (self.pending_docs >> 8) as u8;
        buf[3] = self.pending_docs as u8;
        buf[4] = (self.last_sync >> 24) as u8;
        buf[5] = (self.last_sync >> 16) as u8;
        buf[6] = (self.last_sync >> 8) as u8;
        buf[7] = self.last_sync as u8;
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < Self::ENCODED_SIZE {
            return None;
        }

        Some(Self {
            state: SyncState::from(data[0]),
            progress: data[1],
            pending_docs: ((data[2] as u16) << 8) | (data[3] as u16),
            last_sync: ((data[4] as u32) << 24)
                | ((data[5] as u32) << 16)
                | ((data[6] as u32) << 8)
                | (data[7] as u32),
        })
    }
}

/// Sync Data operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SyncDataOp {
    /// Document sync message
    Document = 0x01,
    /// Sync vector update
    Vector = 0x02,
    /// Acknowledgement
    Ack = 0x03,
    /// End of sync
    End = 0xFF,
}

impl From<u8> for SyncDataOp {
    fn from(value: u8) -> Self {
        match value {
            0x01 => SyncDataOp::Document,
            0x02 => SyncDataOp::Vector,
            0x03 => SyncDataOp::Ack,
            0xFF => SyncDataOp::End,
            _ => SyncDataOp::Document,
        }
    }
}

/// Sync Data characteristic header
///
/// Write/Indicate characteristic for sync data transfer.
#[derive(Debug, Clone)]
pub struct SyncDataHeader {
    /// Operation type
    pub op: SyncDataOp,
    /// Sequence number
    pub seq: u16,
    /// Total fragments (for multi-packet transfers)
    pub total_fragments: u8,
    /// Current fragment index
    pub fragment_index: u8,
}

impl SyncDataHeader {
    /// Header size in bytes
    pub const SIZE: usize = 5;

    /// Create new header
    pub fn new(op: SyncDataOp, seq: u16) -> Self {
        Self {
            op,
            seq,
            total_fragments: 1,
            fragment_index: 0,
        }
    }

    /// Encode header to bytes
    pub fn encode(&self) -> [u8; Self::SIZE] {
        [
            self.op as u8,
            (self.seq >> 8) as u8,
            self.seq as u8,
            self.total_fragments,
            self.fragment_index,
        ]
    }

    /// Decode header from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        Some(Self {
            op: SyncDataOp::from(data[0]),
            seq: ((data[1] as u16) << 8) | (data[2] as u16),
            total_fragments: data[3],
            fragment_index: data[4],
        })
    }
}

/// Command types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandType {
    /// Request sync start
    StartSync = 0x01,
    /// Request sync stop
    StopSync = 0x02,
    /// Request node info refresh
    RefreshInfo = 0x03,
    /// Set hierarchy level (for testing)
    SetHierarchy = 0x10,
    /// Ping (keepalive)
    Ping = 0xFE,
    /// Reset connection
    Reset = 0xFF,
}

impl From<u8> for CommandType {
    fn from(value: u8) -> Self {
        match value {
            0x01 => CommandType::StartSync,
            0x02 => CommandType::StopSync,
            0x03 => CommandType::RefreshInfo,
            0x10 => CommandType::SetHierarchy,
            0xFE => CommandType::Ping,
            0xFF => CommandType::Reset,
            _ => CommandType::Ping,
        }
    }
}

/// Command characteristic data
#[derive(Debug, Clone)]
pub struct Command {
    /// Command type
    pub cmd_type: CommandType,
    /// Command payload (variable length)
    pub payload: Vec<u8>,
}

impl Command {
    /// Create a new command
    pub fn new(cmd_type: CommandType) -> Self {
        Self {
            cmd_type,
            payload: Vec::new(),
        }
    }

    /// Create a command with payload
    pub fn with_payload(cmd_type: CommandType, payload: Vec<u8>) -> Self {
        Self { cmd_type, payload }
    }

    /// Encode command to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + self.payload.len());
        buf.push(self.cmd_type as u8);
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode command from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        Some(Self {
            cmd_type: CommandType::from(data[0]),
            payload: data[1..].to_vec(),
        })
    }
}

/// Status flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusFlags(u8);

impl StatusFlags {
    /// Node is connected to parent
    pub const CONNECTED: u8 = 0x01;
    /// Node is syncing
    pub const SYNCING: u8 = 0x02;
    /// Node has pending data
    pub const PENDING_DATA: u8 = 0x04;
    /// Node is low on battery
    pub const LOW_BATTERY: u8 = 0x08;
    /// Node has error condition
    pub const ERROR: u8 = 0x80;

    /// Create new status flags
    pub const fn new(flags: u8) -> Self {
        Self(flags)
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.0 & Self::CONNECTED != 0
    }

    /// Check if syncing
    pub fn is_syncing(&self) -> bool {
        self.0 & Self::SYNCING != 0
    }

    /// Check if has pending data
    pub fn has_pending_data(&self) -> bool {
        self.0 & Self::PENDING_DATA != 0
    }

    /// Check if low battery
    pub fn is_low_battery(&self) -> bool {
        self.0 & Self::LOW_BATTERY != 0
    }

    /// Check if error
    pub fn has_error(&self) -> bool {
        self.0 & Self::ERROR != 0
    }

    /// Get raw flags
    pub fn flags(&self) -> u8 {
        self.0
    }
}

/// Status characteristic data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusData {
    /// Status flags
    pub flags: StatusFlags,
    /// Number of connected children
    pub child_count: u8,
    /// RSSI to parent (-128 to 127, 127 = no parent)
    pub parent_rssi: i8,
    /// Uptime in minutes (max ~45 days)
    pub uptime_minutes: u16,
}

impl StatusData {
    /// Encoded size in bytes
    pub const ENCODED_SIZE: usize = 5;

    /// Create new status data
    pub fn new() -> Self {
        Self {
            flags: StatusFlags::default(),
            child_count: 0,
            parent_rssi: 127, // No parent
            uptime_minutes: 0,
        }
    }

    /// Encode to bytes
    pub fn encode(&self) -> [u8; Self::ENCODED_SIZE] {
        [
            self.flags.flags(),
            self.child_count,
            self.parent_rssi as u8,
            (self.uptime_minutes >> 8) as u8,
            self.uptime_minutes as u8,
        ]
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < Self::ENCODED_SIZE {
            return None;
        }

        Some(Self {
            flags: StatusFlags::new(data[0]),
            child_count: data[1],
            parent_rssi: data[2] as i8,
            uptime_minutes: ((data[3] as u16) << 8) | (data[4] as u16),
        })
    }
}

impl Default for StatusData {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities;

    #[test]
    fn test_characteristic_properties() {
        let props = CharacteristicProperties::new(
            CharacteristicProperties::READ | CharacteristicProperties::NOTIFY,
        );
        assert!(props.can_read());
        assert!(props.can_notify());
        assert!(!props.can_write());
        assert!(!props.can_indicate());
    }

    #[test]
    fn test_characteristic_uuids() {
        let node_info = HiveCharacteristicUuids::node_info();
        let sync_state = HiveCharacteristicUuids::sync_state();

        // UUIDs should be different
        assert_ne!(node_info, sync_state);

        // Should be derived from base UUID
        assert_ne!(node_info, HIVE_SERVICE_UUID);
    }

    #[test]
    fn test_node_info_encode_decode() {
        let info = NodeInfo::new(
            NodeId::new(0x12345678),
            HierarchyLevel::Squad,
            capabilities::CAN_RELAY | capabilities::HAS_GPS,
        );

        let encoded = info.encode();
        assert_eq!(encoded.len(), NodeInfo::ENCODED_SIZE);

        let decoded = NodeInfo::decode(&encoded).unwrap();
        assert_eq!(decoded.node_id, info.node_id);
        assert_eq!(decoded.hierarchy_level, info.hierarchy_level);
        assert_eq!(decoded.capabilities, info.capabilities);
    }

    #[test]
    fn test_sync_state_encode_decode() {
        let state = SyncStateData {
            state: SyncState::Syncing,
            progress: 50,
            pending_docs: 10,
            last_sync: 1234567890,
        };

        let encoded = state.encode();
        assert_eq!(encoded.len(), SyncStateData::ENCODED_SIZE);

        let decoded = SyncStateData::decode(&encoded).unwrap();
        assert_eq!(decoded.state, state.state);
        assert_eq!(decoded.progress, state.progress);
        assert_eq!(decoded.pending_docs, state.pending_docs);
        assert_eq!(decoded.last_sync, state.last_sync);
    }

    #[test]
    fn test_sync_data_header() {
        let header = SyncDataHeader::new(SyncDataOp::Document, 42);

        let encoded = header.encode();
        assert_eq!(encoded.len(), SyncDataHeader::SIZE);

        let decoded = SyncDataHeader::decode(&encoded).unwrap();
        assert_eq!(decoded.op, SyncDataOp::Document);
        assert_eq!(decoded.seq, 42);
    }

    #[test]
    fn test_command_encode_decode() {
        let cmd = Command::with_payload(CommandType::SetHierarchy, vec![2]); // Set to Platoon

        let encoded = cmd.encode();
        assert_eq!(encoded[0], CommandType::SetHierarchy as u8);
        assert_eq!(encoded[1], 2);

        let decoded = Command::decode(&encoded).unwrap();
        assert_eq!(decoded.cmd_type, CommandType::SetHierarchy);
        assert_eq!(decoded.payload, vec![2]);
    }

    #[test]
    fn test_status_flags() {
        let flags = StatusFlags::new(StatusFlags::CONNECTED | StatusFlags::SYNCING);
        assert!(flags.is_connected());
        assert!(flags.is_syncing());
        assert!(!flags.has_pending_data());
        assert!(!flags.has_error());
    }

    #[test]
    fn test_status_data_encode_decode() {
        let status = StatusData {
            flags: StatusFlags::new(StatusFlags::CONNECTED),
            child_count: 3,
            parent_rssi: -60,
            uptime_minutes: 1440, // 24 hours
        };

        let encoded = status.encode();
        assert_eq!(encoded.len(), StatusData::ENCODED_SIZE);

        let decoded = StatusData::decode(&encoded).unwrap();
        assert!(decoded.flags.is_connected());
        assert_eq!(decoded.child_count, 3);
        assert_eq!(decoded.parent_rssi, -60);
        assert_eq!(decoded.uptime_minutes, 1440);
    }
}
