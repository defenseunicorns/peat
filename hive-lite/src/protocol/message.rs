//! HIVE-Lite Protocol Messages
//!
//! Compact binary message format for gossip protocol.

use super::capabilities::NodeCapabilities;
use super::{MAGIC, PROTOCOL_VERSION};
use heapless::Vec;

/// Maximum packet size (fits in single UDP datagram)
pub const MAX_PACKET_SIZE: usize = 512;

/// Maximum payload size (packet - header)
pub const MAX_PAYLOAD_SIZE: usize = MAX_PACKET_SIZE - 16;

/// Message types for the gossip protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Announce presence and capabilities
    Announce = 0x01,
    /// Heartbeat / keep-alive
    Heartbeat = 0x02,
    /// Data update (CRDT state)
    Data = 0x03,
    /// Query for specific state
    Query = 0x04,
    /// Acknowledge receipt
    Ack = 0x05,
    /// Leave notification
    Leave = 0x06,
}

impl MessageType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Announce),
            0x02 => Some(Self::Heartbeat),
            0x03 => Some(Self::Data),
            0x04 => Some(Self::Query),
            0x05 => Some(Self::Ack),
            0x06 => Some(Self::Leave),
            _ => None,
        }
    }
}

/// Protocol message
///
/// Wire format:
/// ```text
/// ┌──────────┬─────────┬──────────┬──────────┬──────────┬──────────────┐
/// │  MAGIC   │ Version │   Type   │  Flags   │  NodeID  │   SeqNum     │
/// │  4 bytes │ 1 byte  │  1 byte  │  2 bytes │  4 bytes │   4 bytes    │
/// ├──────────┴─────────┴──────────┴──────────┴──────────┴──────────────┤
/// │                          Payload                                    │
/// │                       (variable, max 496 bytes)                     │
/// └─────────────────────────────────────────────────────────────────────┘
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub msg_type: MessageType,
    pub flags: u16,
    pub node_id: u32,
    pub seq_num: u32,
    pub payload: Vec<u8, MAX_PAYLOAD_SIZE>,
}

impl Message {
    /// Create a new message
    pub fn new(msg_type: MessageType, node_id: u32, seq_num: u32) -> Self {
        Self {
            msg_type,
            flags: 0,
            node_id,
            seq_num,
            payload: Vec::new(),
        }
    }

    /// Create an announce message
    pub fn announce(node_id: u32, seq_num: u32, capabilities: NodeCapabilities) -> Self {
        let mut msg = Self::new(MessageType::Announce, node_id, seq_num);
        msg.payload.extend_from_slice(&capabilities.encode()).ok();
        msg
    }

    /// Create a heartbeat message
    pub fn heartbeat(node_id: u32, seq_num: u32) -> Self {
        Self::new(MessageType::Heartbeat, node_id, seq_num)
    }

    /// Create a data message with CRDT payload
    pub fn data(node_id: u32, seq_num: u32, crdt_type: u8, crdt_data: &[u8]) -> Option<Self> {
        let mut msg = Self::new(MessageType::Data, node_id, seq_num);
        msg.payload.push(crdt_type).ok()?;
        msg.payload.extend_from_slice(crdt_data).ok()?;
        Some(msg)
    }

    /// Create an ack message
    pub fn ack(node_id: u32, ack_seq: u32) -> Self {
        let mut msg = Self::new(MessageType::Ack, node_id, 0);
        msg.payload.extend_from_slice(&ack_seq.to_le_bytes()).ok();
        msg
    }

    /// Encode message to bytes
    pub fn encode(&self, buf: &mut [u8]) -> Result<usize, MessageError> {
        let total_len = 16 + self.payload.len();
        if buf.len() < total_len {
            return Err(MessageError::BufferTooSmall);
        }

        // Header
        buf[0..4].copy_from_slice(&MAGIC);
        buf[4] = PROTOCOL_VERSION;
        buf[5] = self.msg_type as u8;
        buf[6..8].copy_from_slice(&self.flags.to_le_bytes());
        buf[8..12].copy_from_slice(&self.node_id.to_le_bytes());
        buf[12..16].copy_from_slice(&self.seq_num.to_le_bytes());

        // Payload
        buf[16..16 + self.payload.len()].copy_from_slice(&self.payload);

        Ok(total_len)
    }

    /// Decode message from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, MessageError> {
        if buf.len() < 16 {
            return Err(MessageError::TooShort);
        }

        // Check magic
        if buf[0..4] != MAGIC {
            return Err(MessageError::InvalidMagic);
        }

        // Check version
        if buf[4] != PROTOCOL_VERSION {
            return Err(MessageError::UnsupportedVersion);
        }

        let msg_type = MessageType::from_u8(buf[5]).ok_or(MessageError::InvalidMessageType)?;
        let flags = u16::from_le_bytes(buf[6..8].try_into().unwrap());
        let node_id = u32::from_le_bytes(buf[8..12].try_into().unwrap());
        let seq_num = u32::from_le_bytes(buf[12..16].try_into().unwrap());

        let mut payload = Vec::new();
        if buf.len() > 16 {
            payload
                .extend_from_slice(&buf[16..])
                .map_err(|_| MessageError::PayloadTooLarge)?;
        }

        Ok(Self {
            msg_type,
            flags,
            node_id,
            seq_num,
            payload,
        })
    }
}

/// Message encoding/decoding errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageError {
    BufferTooSmall,
    TooShort,
    InvalidMagic,
    UnsupportedVersion,
    InvalidMessageType,
    PayloadTooLarge,
}

/// CRDT type identifiers for Data messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CrdtType {
    LwwRegister = 0x01,
    GCounter = 0x02,
    PnCounter = 0x03,
    OrSet = 0x04,
}

impl CrdtType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::LwwRegister),
            0x02 => Some(Self::GCounter),
            0x03 => Some(Self::PnCounter),
            0x04 => Some(Self::OrSet),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_encode_decode() {
        let msg = Message::announce(12345, 1, NodeCapabilities::lite());
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let len = msg.encode(&mut buf).unwrap();

        let decoded = Message::decode(&buf[..len]).unwrap();
        assert_eq!(decoded.msg_type, MessageType::Announce);
        assert_eq!(decoded.node_id, 12345);
        assert_eq!(decoded.seq_num, 1);
    }

    #[test]
    fn test_heartbeat() {
        let msg = Message::heartbeat(42, 100);
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let len = msg.encode(&mut buf).unwrap();

        let decoded = Message::decode(&buf[..len]).unwrap();
        assert_eq!(decoded.msg_type, MessageType::Heartbeat);
        assert_eq!(decoded.node_id, 42);
    }

    #[test]
    fn test_data_message() {
        let crdt_data = [1, 2, 3, 4, 5];
        let msg = Message::data(99, 50, CrdtType::LwwRegister as u8, &crdt_data).unwrap();

        let mut buf = [0u8; MAX_PACKET_SIZE];
        let len = msg.encode(&mut buf).unwrap();

        let decoded = Message::decode(&buf[..len]).unwrap();
        assert_eq!(decoded.msg_type, MessageType::Data);
        assert_eq!(decoded.payload[0], CrdtType::LwwRegister as u8);
        assert_eq!(&decoded.payload[1..], &crdt_data);
    }

    #[test]
    fn test_invalid_magic() {
        let buf = [0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(Message::decode(&buf), Err(MessageError::InvalidMagic));
    }
}
