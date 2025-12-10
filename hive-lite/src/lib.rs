//! HIVE-Lite: Resource-Constrained Mesh Protocol
//!
//! A lightweight implementation of the HIVE protocol for embedded devices.
//!
//! # Features
//!
//! - **First-class mesh participation** - Same protocol as HIVE-Full
//! - **Primitive CRDTs** - LWW-Register, G-Counter, PN-Counter
//! - **Ephemeral operation** - No persistent storage required
//! - **Capability negotiation** - Graceful degradation with Full nodes
//!
//! # Example
//!
//! ```ignore
//! use hive_lite::prelude::*;
//!
//! // Create a new node
//! let node_id = 0x12345678;
//! let mut state = GossipState::new(node_id, NodeCapabilities::lite());
//!
//! // Create a sensor reading
//! let mut temp = LwwRegister::<i32>::new(2350, timestamp(), node_id); // 23.50°C
//!
//! // Queue update for gossip
//! let mut buf = [0u8; 64];
//! let len = temp.encode(&mut buf).unwrap();
//! state.queue_crdt_update(CrdtType::LwwRegister, &buf[..len]);
//! ```

#![no_std]
#![allow(async_fn_in_trait)]

pub mod crdt;
pub mod protocol;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::crdt::{CrdtError, GCounter, LiteCrdt, LwwRegister, PnCounter};
    pub use crate::crdt::lww_register::{LwwEncodable, SensorValue};
    pub use crate::protocol::{
        GossipState, Message, MessageTarget, MessageType, NodeCapabilities, Peer, PeerTable,
        DEFAULT_PORT, MAX_PACKET_SIZE, MULTICAST_ADDR, PROTOCOL_VERSION,
    };
    pub use crate::protocol::message::CrdtType;
}

// Re-export key types at crate root
pub use crdt::{GCounter, LwwRegister, PnCounter};
pub use protocol::{GossipState, NodeCapabilities};
