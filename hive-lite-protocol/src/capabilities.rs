//! Node capability flags announced during handshake.

/// Capability flags for HIVE nodes.
///
/// These flags are announced during handshake so peers know what
/// features each node supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NodeCapabilities(u16);

impl NodeCapabilities {
    /// Can persist data across restarts.
    pub const PERSISTENT_STORAGE: u16 = 0x0001;
    /// Can forward messages for multi-hop routing.
    pub const RELAY_CAPABLE: u16 = 0x0002;
    /// Supports full Automerge documents.
    pub const DOCUMENT_CRDT: u16 = 0x0004;
    /// Supports primitive CRDTs (LWW, counters, sets).
    pub const PRIMITIVE_CRDT: u16 = 0x0008;
    /// Can store and serve blobs.
    pub const BLOB_STORAGE: u16 = 0x0010;
    /// Can answer historical queries.
    pub const HISTORY_QUERY: u16 = 0x0020;
    /// Can aggregate data for upstream.
    pub const AGGREGATION: u16 = 0x0040;
    /// Has sensor inputs.
    pub const SENSOR_INPUT: u16 = 0x0080;
    /// Has display output.
    pub const DISPLAY_OUTPUT: u16 = 0x0100;
    /// Has actuation capability (motors, etc.).
    pub const ACTUATION: u16 = 0x0200;

    /// Create empty capabilities.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Create capabilities with all flags set.
    pub const fn all() -> Self {
        Self(0xFFFF)
    }

    /// Create typical HIVE-Lite capabilities.
    pub const fn lite() -> Self {
        Self(Self::PRIMITIVE_CRDT | Self::SENSOR_INPUT)
    }

    /// Create typical HIVE-Full capabilities.
    pub const fn full() -> Self {
        Self(
            Self::PERSISTENT_STORAGE
                | Self::RELAY_CAPABLE
                | Self::DOCUMENT_CRDT
                | Self::PRIMITIVE_CRDT
                | Self::BLOB_STORAGE
                | Self::HISTORY_QUERY
                | Self::AGGREGATION,
        )
    }

    /// Create new capabilities from raw bits.
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// Get raw bits.
    pub const fn bits(&self) -> u16 {
        self.0
    }

    /// Check if a capability is set.
    pub const fn has(&self, cap: u16) -> bool {
        (self.0 & cap) != 0
    }

    /// Set a capability.
    pub fn set(&mut self, cap: u16) {
        self.0 |= cap;
    }

    /// Clear a capability.
    pub fn clear(&mut self, cap: u16) {
        self.0 &= !cap;
    }

    /// Get intersection of capabilities (what both nodes support).
    pub const fn intersection(&self, other: &Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Check if this node can sync CRDTs with another.
    pub const fn can_sync_with(&self, other: &Self) -> bool {
        self.has(Self::PRIMITIVE_CRDT) && other.has(Self::PRIMITIVE_CRDT)
    }

    /// Encode to 2 bytes (little-endian).
    pub fn encode(&self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    /// Decode from 2 bytes (little-endian).
    pub fn decode(bytes: [u8; 2]) -> Self {
        Self(u16::from_le_bytes(bytes))
    }
}

impl core::fmt::Display for NodeCapabilities {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[")?;
        let mut first = true;

        macro_rules! flag {
            ($cap:expr, $name:expr) => {
                if self.has($cap) {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, $name)?;
                    #[allow(unused_assignments)]
                    {
                        first = false;
                    }
                }
            };
        }

        flag!(Self::PERSISTENT_STORAGE, "storage");
        flag!(Self::RELAY_CAPABLE, "relay");
        flag!(Self::DOCUMENT_CRDT, "doc-crdt");
        flag!(Self::PRIMITIVE_CRDT, "prim-crdt");
        flag!(Self::BLOB_STORAGE, "blob");
        flag!(Self::HISTORY_QUERY, "history");
        flag!(Self::AGGREGATION, "agg");
        flag!(Self::SENSOR_INPUT, "sensor");
        flag!(Self::DISPLAY_OUTPUT, "display");
        flag!(Self::ACTUATION, "actuate");

        write!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use alloc::format;

    #[test]
    fn test_lite_capabilities() {
        let caps = NodeCapabilities::lite();
        assert!(caps.has(NodeCapabilities::PRIMITIVE_CRDT));
        assert!(caps.has(NodeCapabilities::SENSOR_INPUT));
        assert!(!caps.has(NodeCapabilities::PERSISTENT_STORAGE));
        assert!(!caps.has(NodeCapabilities::DOCUMENT_CRDT));
    }

    #[test]
    fn test_full_capabilities() {
        let caps = NodeCapabilities::full();
        assert!(caps.has(NodeCapabilities::PERSISTENT_STORAGE));
        assert!(caps.has(NodeCapabilities::DOCUMENT_CRDT));
        assert!(caps.has(NodeCapabilities::PRIMITIVE_CRDT));
    }

    #[test]
    fn test_can_sync() {
        let lite = NodeCapabilities::lite();
        let full = NodeCapabilities::full();
        assert!(lite.can_sync_with(&full));
        assert!(full.can_sync_with(&lite));
    }

    #[test]
    fn test_encode_decode() {
        let caps = NodeCapabilities::lite();
        let encoded = caps.encode();
        let decoded = NodeCapabilities::decode(encoded);
        assert_eq!(caps, decoded);
    }

    #[test]
    fn test_display_lite() {
        let caps = NodeCapabilities::lite();
        let s = format!("{}", caps);
        assert_eq!(s, "[prim-crdt, sensor]");
    }

    #[test]
    fn test_display_empty() {
        let caps = NodeCapabilities::empty();
        let s = format!("{}", caps);
        assert_eq!(s, "[]");
    }

    #[test]
    fn test_bit_values_match_spec() {
        assert_eq!(NodeCapabilities::PERSISTENT_STORAGE, 0x0001);
        assert_eq!(NodeCapabilities::RELAY_CAPABLE, 0x0002);
        assert_eq!(NodeCapabilities::DOCUMENT_CRDT, 0x0004);
        assert_eq!(NodeCapabilities::PRIMITIVE_CRDT, 0x0008);
        assert_eq!(NodeCapabilities::BLOB_STORAGE, 0x0010);
        assert_eq!(NodeCapabilities::HISTORY_QUERY, 0x0020);
        assert_eq!(NodeCapabilities::AGGREGATION, 0x0040);
        assert_eq!(NodeCapabilities::SENSOR_INPUT, 0x0080);
        assert_eq!(NodeCapabilities::DISPLAY_OUTPUT, 0x0100);
        assert_eq!(NodeCapabilities::ACTUATION, 0x0200);
    }
}
