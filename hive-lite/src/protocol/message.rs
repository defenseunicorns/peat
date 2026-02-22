//! HIVE-Lite Protocol Messages
//!
//! Compact binary message format for gossip protocol.

use super::capabilities::NodeCapabilities;
use hive_lite_protocol::{Header, decode_header, encode_header, HEADER_SIZE, MAX_PAYLOAD_SIZE};
use heapless::Vec;

// Re-export shared types so existing `use super::message::*` keeps working.
pub use hive_lite_protocol::{CrdtType, MAX_PACKET_SIZE, MessageError, MessageType};

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

    /// Create an OTA accept message
    pub fn ota_accept(node_id: u32, session_id: u16, resume_chunk: u16) -> Self {
        let mut msg = Self::new(MessageType::OtaAccept, node_id, 0);
        msg.payload.extend_from_slice(&session_id.to_le_bytes()).ok();
        msg.payload.extend_from_slice(&resume_chunk.to_le_bytes()).ok();
        msg
    }

    /// Create an OTA ACK message
    pub fn ota_ack(node_id: u32, session_id: u16, acked_chunk: u16) -> Self {
        let mut msg = Self::new(MessageType::OtaAck, node_id, 0);
        msg.payload.extend_from_slice(&session_id.to_le_bytes()).ok();
        msg.payload.extend_from_slice(&acked_chunk.to_le_bytes()).ok();
        msg
    }

    /// Create an OTA result message
    pub fn ota_result(node_id: u32, session_id: u16, result_code: u8) -> Self {
        let mut msg = Self::new(MessageType::OtaResult, node_id, 0);
        msg.payload.extend_from_slice(&session_id.to_le_bytes()).ok();
        msg.payload.push(result_code).ok();
        msg.payload.push(0).ok(); // reserved
        msg
    }

    /// Create an OTA abort message
    pub fn ota_abort(node_id: u32, session_id: u16, reason: u8) -> Self {
        let mut msg = Self::new(MessageType::OtaAbort, node_id, 0);
        msg.payload.extend_from_slice(&session_id.to_le_bytes()).ok();
        msg.payload.push(reason).ok();
        msg.payload.push(0).ok(); // reserved
        msg
    }

    /// Encode message to bytes
    pub fn encode(&self, buf: &mut [u8]) -> Result<usize, MessageError> {
        let total_len = HEADER_SIZE + self.payload.len();
        if buf.len() < total_len {
            return Err(MessageError::BufferTooSmall);
        }

        let header = Header {
            msg_type: self.msg_type,
            flags: self.flags,
            node_id: self.node_id,
            seq_num: self.seq_num,
        };
        encode_header(&header, buf)?;

        // Payload
        buf[HEADER_SIZE..HEADER_SIZE + self.payload.len()].copy_from_slice(&self.payload);

        Ok(total_len)
    }

    /// Decode message from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, MessageError> {
        let (header, payload_bytes) = decode_header(buf)?;

        let mut payload = Vec::new();
        if !payload_bytes.is_empty() {
            payload
                .extend_from_slice(payload_bytes)
                .map_err(|_| MessageError::PayloadTooLarge)?;
        }

        Ok(Self {
            msg_type: header.msg_type,
            flags: header.flags,
            node_id: header.node_id,
            seq_num: header.seq_num,
            payload,
        })
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
