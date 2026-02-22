//! Protocol-level constants.

/// Magic bytes identifying a HIVE-Lite packet: ASCII "HIVE".
pub const MAGIC: [u8; 4] = [0x48, 0x49, 0x56, 0x45];

/// Protocol version for compatibility checking.
pub const PROTOCOL_VERSION: u8 = 1;

/// Default UDP port for HIVE-Lite communication.
///
/// This is the canonical deployed value used by both hive-lite firmware
/// and hive-mesh transport.
pub const DEFAULT_PORT: u16 = 5555;

/// Default multicast address for discovery: 239.255.72.76 (H.L).
pub const MULTICAST_ADDR: [u8; 4] = [239, 255, 72, 76];

/// Fixed header size in bytes.
pub const HEADER_SIZE: usize = 16;

/// Maximum packet size (fits in a single UDP datagram).
pub const MAX_PACKET_SIZE: usize = 512;

/// Maximum payload size (packet minus header).
pub const MAX_PAYLOAD_SIZE: usize = MAX_PACKET_SIZE - HEADER_SIZE;
