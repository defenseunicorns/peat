//! HIVE-Lite packet header codec.
//!
//! The header is a fixed 16-byte prefix on every packet:
//!
//! ```text
//! ┌──────────┬─────────┬──────────┬──────────┬──────────┬──────────────┐
//! │  MAGIC   │ Version │   Type   │  Flags   │  NodeID  │   SeqNum     │
//! │  4 bytes │ 1 byte  │  1 byte  │  2 bytes │  4 bytes │   4 bytes    │
//! └──────────┴─────────┴──────────┴──────────┴──────────┴──────────────┘
//! ```

use crate::constants::{HEADER_SIZE, MAGIC, PROTOCOL_VERSION};
use crate::error::MessageError;
use crate::message_type::MessageType;

/// Decoded header fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub msg_type: MessageType,
    pub flags: u16,
    pub node_id: u32,
    pub seq_num: u32,
}

/// Encode a header into the first 16 bytes of `buf`.
///
/// Returns `Err(MessageError::BufferTooSmall)` if `buf.len() < HEADER_SIZE`.
pub fn encode_header(header: &Header, buf: &mut [u8]) -> Result<(), MessageError> {
    if buf.len() < HEADER_SIZE {
        return Err(MessageError::BufferTooSmall);
    }
    buf[0..4].copy_from_slice(&MAGIC);
    buf[4] = PROTOCOL_VERSION;
    buf[5] = header.msg_type as u8;
    buf[6..8].copy_from_slice(&header.flags.to_le_bytes());
    buf[8..12].copy_from_slice(&header.node_id.to_le_bytes());
    buf[12..16].copy_from_slice(&header.seq_num.to_le_bytes());
    Ok(())
}

/// Decode a header from `buf`, returning the header and a slice of the
/// remaining payload bytes.
///
/// Validates magic bytes, protocol version, and message type.
pub fn decode_header(buf: &[u8]) -> Result<(Header, &[u8]), MessageError> {
    if buf.len() < HEADER_SIZE {
        return Err(MessageError::TooShort);
    }
    if buf[0..4] != MAGIC {
        return Err(MessageError::InvalidMagic);
    }
    if buf[4] != PROTOCOL_VERSION {
        return Err(MessageError::UnsupportedVersion);
    }
    let msg_type = MessageType::from_u8(buf[5]).ok_or(MessageError::InvalidMessageType)?;
    let flags = u16::from_le_bytes([buf[6], buf[7]]);
    let node_id = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let seq_num = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);

    let header = Header {
        msg_type,
        flags,
        node_id,
        seq_num,
    };
    Ok((header, &buf[HEADER_SIZE..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let hdr = Header {
            msg_type: MessageType::Announce,
            flags: 0x1234,
            node_id: 0xDEADBEEF,
            seq_num: 42,
        };
        let mut buf = [0u8; 32];
        encode_header(&hdr, &mut buf).unwrap();
        // Append some payload bytes
        buf[16] = 0xAA;
        buf[17] = 0xBB;

        let (decoded, payload) = decode_header(&buf[..18]).unwrap();
        assert_eq!(decoded, hdr);
        assert_eq!(payload, &[0xAA, 0xBB]);
    }

    #[test]
    fn too_short() {
        let buf = [0u8; 10];
        assert_eq!(decode_header(&buf), Err(MessageError::TooShort));
    }

    #[test]
    fn bad_magic() {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&[0, 0, 0, 0]);
        buf[4] = PROTOCOL_VERSION;
        buf[5] = MessageType::Heartbeat as u8;
        assert_eq!(decode_header(&buf), Err(MessageError::InvalidMagic));
    }

    #[test]
    fn bad_version() {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&MAGIC);
        buf[4] = 99;
        buf[5] = MessageType::Heartbeat as u8;
        assert_eq!(decode_header(&buf), Err(MessageError::UnsupportedVersion));
    }

    #[test]
    fn bad_message_type() {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&MAGIC);
        buf[4] = PROTOCOL_VERSION;
        buf[5] = 0xFF;
        assert_eq!(decode_header(&buf), Err(MessageError::InvalidMessageType));
    }

    #[test]
    fn header_only_no_payload() {
        let hdr = Header {
            msg_type: MessageType::Leave,
            flags: 0,
            node_id: 1,
            seq_num: 0,
        };
        let mut buf = [0u8; 16];
        encode_header(&hdr, &mut buf).unwrap();
        let (decoded, payload) = decode_header(&buf).unwrap();
        assert_eq!(decoded, hdr);
        assert!(payload.is_empty());
    }

    #[test]
    fn encode_buffer_too_small() {
        let hdr = Header {
            msg_type: MessageType::Data,
            flags: 0,
            node_id: 0,
            seq_num: 0,
        };
        let mut buf = [0u8; 10];
        assert_eq!(
            encode_header(&hdr, &mut buf),
            Err(MessageError::BufferTooSmall)
        );
    }
}
