//! HIVE GATT Service Module
//!
//! Provides the GATT service implementation for HIVE Protocol BLE communication.
//!
//! ## Service Structure
//!
//! ```text
//! HIVE GATT Service (UUID: f47ac10b-58cc-4372-a567-0e02b2c3d479)
//! ├── Node Info (read)           - Basic node information
//! ├── Sync State (read/notify)   - Current sync status
//! ├── Sync Data (write/indicate) - Sync data transfer
//! ├── Command (write)            - Control commands
//! └── Status (read/notify)       - Node status updates
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::gatt::{HiveGattService, HiveCharacteristics};
//! use hive_btle::{NodeId, HierarchyLevel};
//!
//! // Create the GATT service
//! let service = HiveGattService::new(
//!     NodeId::new(0x12345678),
//!     HierarchyLevel::Platform,
//!     0, // capabilities
//! );
//!
//! // Get characteristic descriptors for registration
//! let chars = service.characteristics();
//!
//! // Handle reads
//! let node_info = service.read_node_info().await;
//!
//! // Handle writes
//! service.write_command(&command_data).await?;
//! ```
//!
//! ## Sync Protocol
//!
//! The sync protocol uses fragmentation for large messages:
//!
//! ```ignore
//! use hive_btle::gatt::SyncProtocol;
//!
//! let mut protocol = SyncProtocol::new();
//! protocol.set_mtu(251); // Use negotiated MTU
//!
//! // Start sync with local sync vector
//! protocol.start_sync(sync_vector);
//!
//! // Send outgoing messages
//! while let Some(msg) = protocol.next_outgoing() {
//!     // Write msg.encode() to Sync Data characteristic
//! }
//!
//! // Process incoming messages
//! if let Some((msg_type, payload)) = protocol.process_incoming(&data) {
//!     // Handle received message
//! }
//! ```

mod characteristics;
mod protocol;
#[cfg(feature = "std")]
mod service;

pub use characteristics::{
    CharacteristicProperties, Command, CommandType, HiveCharacteristicUuids, NodeInfo, StatusData,
    StatusFlags, SyncDataHeader, SyncDataOp, SyncState, SyncStateData,
};
pub use protocol::{
    fragment_payload, max_payload_size, FragmentReassembler, SyncMessage, SyncMessageType,
    SyncProtocol, SyncProtocolState, DEFAULT_MAX_PAYLOAD,
};
#[cfg(feature = "std")]
pub use service::{
    CharacteristicDescriptor, GattEvent, GattEventCallback, HiveCharacteristics, HiveGattService,
};
