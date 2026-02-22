//! Node Capabilities
//!
//! Re-exports `NodeCapabilities` from `hive_lite_protocol`.
//! Capability flags announced during handshake to enable graceful
//! degradation between Full and Lite nodes.

pub use hive_lite_protocol::NodeCapabilities;

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
