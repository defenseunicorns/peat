//! HIVE-Lite Gossip Protocol
//!
//! Lightweight gossip protocol for mesh communication between HIVE nodes.
//! Compatible with HIVE-Full nodes via capability negotiation.

pub mod message;
pub mod peer;
pub mod state;
pub mod capabilities;

pub use message::{Message, MessageType, MAX_PACKET_SIZE};
pub use peer::{Peer, PeerTable};
pub use state::{GossipState, MessageTarget};
pub use capabilities::NodeCapabilities;

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: u8 = 1;

/// Magic bytes to identify HIVE-Lite packets
pub const MAGIC: [u8; 4] = [0x48, 0x49, 0x56, 0x45]; // "HIVE"

/// Default multicast address for discovery
pub const MULTICAST_ADDR: [u8; 4] = [239, 255, 72, 76]; // 239.255.H.L

/// Default port for HIVE-Lite communication
pub const DEFAULT_PORT: u16 = 4872; // "HIVE" on phone keypad: 4-4-8-3
