//! Node Capabilities
//!
//! Capability flags announced during handshake to enable graceful
//! degradation between Full and Lite nodes.

/// Capability flags for HIVE nodes
///
/// These flags are announced during handshake so peers know what
/// features each node supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NodeCapabilities(u16);

impl NodeCapabilities {
    /// Can persist data across restarts
    pub const PERSISTENT_STORAGE: u16 = 0b0000_0000_0000_0001;
    /// Can forward messages for multi-hop routing
    pub const RELAY_CAPABLE: u16 = 0b0000_0000_0000_0010;
    /// Supports full Automerge documents
    pub const DOCUMENT_CRDT: u16 = 0b0000_0000_0000_0100;
    /// Supports primitive CRDTs (LWW, counters, sets)
    pub const PRIMITIVE_CRDT: u16 = 0b0000_0000_0000_1000;
    /// Can store and serve blobs
    pub const BLOB_STORAGE: u16 = 0b0000_0000_0001_0000;
    /// Can answer historical queries
    pub const HISTORY_QUERY: u16 = 0b0000_0000_0010_0000;
    /// Can aggregate data for upstream
    pub const AGGREGATION: u16 = 0b0000_0000_0100_0000;
    /// Has sensor inputs
    pub const SENSOR_INPUT: u16 = 0b0000_0000_1000_0000;
    /// Has display output
    pub const DISPLAY_OUTPUT: u16 = 0b0000_0001_0000_0000;
    /// Has actuation capability (motors, etc.)
    pub const ACTUATION: u16 = 0b0000_0010_0000_0000;

    /// Create empty capabilities
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Create capabilities with all flags set
    pub const fn all() -> Self {
        Self(0xFFFF)
    }

    /// Create typical HIVE-Lite capabilities
    pub const fn lite() -> Self {
        Self(Self::PRIMITIVE_CRDT | Self::SENSOR_INPUT)
    }

    /// Create typical HIVE-Full capabilities
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

    /// Create new capabilities from raw bits
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// Get raw bits
    pub const fn bits(&self) -> u16 {
        self.0
    }

    /// Check if a capability is set
    pub const fn has(&self, cap: u16) -> bool {
        (self.0 & cap) != 0
    }

    /// Set a capability
    pub fn set(&mut self, cap: u16) {
        self.0 |= cap;
    }

    /// Clear a capability
    pub fn clear(&mut self, cap: u16) {
        self.0 &= !cap;
    }

    /// Get intersection of capabilities (what both nodes support)
    pub const fn intersection(&self, other: &Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Check if this node can sync CRDTs with another
    pub const fn can_sync_with(&self, other: &Self) -> bool {
        // Both must support at least primitive CRDTs
        self.has(Self::PRIMITIVE_CRDT) && other.has(Self::PRIMITIVE_CRDT)
    }

    /// Encode to 2 bytes
    pub fn encode(&self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    /// Decode from 2 bytes
    pub fn decode(bytes: [u8; 2]) -> Self {
        Self(u16::from_le_bytes(bytes))
    }
}

impl core::fmt::Display for NodeCapabilities {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut caps = heapless::Vec::<&str, 10>::new();

        if self.has(Self::PERSISTENT_STORAGE) {
            caps.push("storage").ok();
        }
        if self.has(Self::RELAY_CAPABLE) {
            caps.push("relay").ok();
        }
        if self.has(Self::DOCUMENT_CRDT) {
            caps.push("doc-crdt").ok();
        }
        if self.has(Self::PRIMITIVE_CRDT) {
            caps.push("prim-crdt").ok();
        }
        if self.has(Self::BLOB_STORAGE) {
            caps.push("blob").ok();
        }
        if self.has(Self::HISTORY_QUERY) {
            caps.push("history").ok();
        }
        if self.has(Self::AGGREGATION) {
            caps.push("agg").ok();
        }
        if self.has(Self::SENSOR_INPUT) {
            caps.push("sensor").ok();
        }
        if self.has(Self::DISPLAY_OUTPUT) {
            caps.push("display").ok();
        }
        if self.has(Self::ACTUATION) {
            caps.push("actuate").ok();
        }

        write!(f, "[")?;
        for (i, cap) in caps.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", cap)?;
        }
        write!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
