//! HIVE Document wire format for BLE mesh sync
//!
//! This module provides the unified document format used across all platforms
//! (iOS, Android, ESP32) for mesh synchronization. The format is designed for
//! efficient BLE transmission while supporting CRDT semantics.
//!
//! ## Wire Format
//!
//! ```text
//! Header (8 bytes):
//!   version:  4 bytes (LE) - document version number
//!   node_id:  4 bytes (LE) - source node identifier
//!
//! GCounter (4 + N*12 bytes):
//!   num_entries: 4 bytes (LE)
//!   entries[N]:
//!     node_id: 4 bytes (LE)
//!     count:   8 bytes (LE)
//!
//! Extended Section (optional, when peripheral data present):
//!   marker:         1 byte (0xAB)
//!   reserved:       1 byte
//!   peripheral_len: 2 bytes (LE)
//!   peripheral:     variable (34-43 bytes)
//! ```

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::sync::crdt::{EmergencyEvent, EventType, GCounter, Peripheral, PeripheralEvent};
use crate::NodeId;

/// Marker byte indicating extended section with peripheral data
pub const EXTENDED_MARKER: u8 = 0xAB;

/// Marker byte indicating emergency event section
pub const EMERGENCY_MARKER: u8 = 0xAC;

/// Minimum document size (header only, no counter entries)
pub const MIN_DOCUMENT_SIZE: usize = 8;

/// Maximum recommended mesh size for reliable single-packet sync
///
/// Beyond this, documents may exceed typical BLE MTU (244 bytes).
/// Size calculation: 8 (header) + 4 + (N × 12) (GCounter) + 42 (Peripheral)
///   20 nodes = 8 + 244 + 42 = 294 bytes
pub const MAX_MESH_SIZE: usize = 20;

/// Target document size for single-packet transmission
///
/// Based on typical negotiated BLE MTU (247 bytes - 3 ATT overhead).
pub const TARGET_DOCUMENT_SIZE: usize = 244;

/// Hard limit before fragmentation would be required
///
/// BLE 5.0+ supports up to 517 byte MTU, but 512 is practical max payload.
pub const MAX_DOCUMENT_SIZE: usize = 512;

/// A HIVE document for mesh synchronization
///
/// Contains header information, a CRDT G-Counter for tracking mesh activity,
/// optional peripheral data for events, and optional emergency event with ACK tracking.
#[derive(Debug, Clone)]
pub struct HiveDocument {
    /// Document version (incremented on each change)
    pub version: u32,

    /// Source node ID that created/last modified this document
    pub node_id: NodeId,

    /// CRDT G-Counter tracking activity across the mesh
    pub counter: GCounter,

    /// Optional peripheral data (sensor info, events)
    pub peripheral: Option<Peripheral>,

    /// Optional active emergency event with distributed ACK tracking
    pub emergency: Option<EmergencyEvent>,
}

impl Default for HiveDocument {
    fn default() -> Self {
        Self {
            version: 1,
            node_id: NodeId::default(),
            counter: GCounter::new(),
            peripheral: None,
            emergency: None,
        }
    }
}

impl HiveDocument {
    /// Create a new document for the given node
    pub fn new(node_id: NodeId) -> Self {
        Self {
            version: 1,
            node_id,
            counter: GCounter::new(),
            peripheral: None,
            emergency: None,
        }
    }

    /// Create with an initial peripheral
    pub fn with_peripheral(mut self, peripheral: Peripheral) -> Self {
        self.peripheral = Some(peripheral);
        self
    }

    /// Create with an initial emergency event
    pub fn with_emergency(mut self, emergency: EmergencyEvent) -> Self {
        self.emergency = Some(emergency);
        self
    }

    /// Increment the document version
    pub fn increment_version(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// Increment the counter for this node
    pub fn increment_counter(&mut self) {
        self.counter.increment(&self.node_id, 1);
        self.increment_version();
    }

    /// Set an event on the peripheral
    pub fn set_event(&mut self, event_type: EventType, timestamp: u64) {
        if let Some(ref mut peripheral) = self.peripheral {
            peripheral.set_event(event_type, timestamp);
            self.increment_counter();
        }
    }

    /// Clear the event from the peripheral
    pub fn clear_event(&mut self) {
        if let Some(ref mut peripheral) = self.peripheral {
            peripheral.clear_event();
            self.increment_version();
        }
    }

    /// Set an emergency event
    ///
    /// Creates a new emergency event with the given source node, timestamp,
    /// and list of known peers to track for ACKs.
    pub fn set_emergency(&mut self, source_node: u32, timestamp: u64, known_peers: &[u32]) {
        self.emergency = Some(EmergencyEvent::new(source_node, timestamp, known_peers));
        self.increment_counter();
    }

    /// Record an ACK for the current emergency
    ///
    /// Returns true if the ACK was new (state changed)
    pub fn ack_emergency(&mut self, node_id: u32) -> bool {
        if let Some(ref mut emergency) = self.emergency {
            if emergency.ack(node_id) {
                self.increment_version();
                return true;
            }
        }
        false
    }

    /// Clear the emergency event
    pub fn clear_emergency(&mut self) {
        if self.emergency.is_some() {
            self.emergency = None;
            self.increment_version();
        }
    }

    /// Get the current emergency event (if any)
    pub fn get_emergency(&self) -> Option<&EmergencyEvent> {
        self.emergency.as_ref()
    }

    /// Check if there's an active emergency
    pub fn has_emergency(&self) -> bool {
        self.emergency.is_some()
    }

    /// Merge with another document using CRDT semantics
    ///
    /// Returns true if our state changed (useful for triggering re-broadcast)
    pub fn merge(&mut self, other: &HiveDocument) -> bool {
        let mut changed = false;

        // Merge counter
        let old_value = self.counter.value();
        self.counter.merge(&other.counter);
        if self.counter.value() != old_value {
            changed = true;
        }

        // Merge emergency event
        if let Some(ref other_emergency) = other.emergency {
            match &mut self.emergency {
                Some(ref mut our_emergency) => {
                    if our_emergency.merge(other_emergency) {
                        changed = true;
                    }
                }
                None => {
                    self.emergency = Some(other_emergency.clone());
                    changed = true;
                }
            }
        }

        if changed {
            self.increment_version();
        }
        changed
    }

    /// Get the current event type (if any)
    pub fn current_event(&self) -> Option<EventType> {
        self.peripheral
            .as_ref()
            .and_then(|p| p.last_event.as_ref())
            .map(|e| e.event_type)
    }

    /// Encode to bytes for BLE transmission
    pub fn encode(&self) -> Vec<u8> {
        let counter_data = self.counter.encode();
        let peripheral_data = self.peripheral.as_ref().map(|p| p.encode());
        let emergency_data = self.emergency.as_ref().map(|e| e.encode());

        // Calculate total size
        let mut size = 8 + counter_data.len(); // header + counter
        if let Some(ref pdata) = peripheral_data {
            size += 4 + pdata.len(); // marker + reserved + len + peripheral
        }
        if let Some(ref edata) = emergency_data {
            size += 4 + edata.len(); // marker + reserved + len + emergency
        }

        let mut buf = Vec::with_capacity(size);

        // Header
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.node_id.as_u32().to_le_bytes());

        // GCounter
        buf.extend_from_slice(&counter_data);

        // Extended section (if peripheral present)
        if let Some(pdata) = peripheral_data {
            buf.push(EXTENDED_MARKER);
            buf.push(0); // reserved
            buf.extend_from_slice(&(pdata.len() as u16).to_le_bytes());
            buf.extend_from_slice(&pdata);
        }

        // Emergency section (if emergency present)
        if let Some(edata) = emergency_data {
            buf.push(EMERGENCY_MARKER);
            buf.push(0); // reserved
            buf.extend_from_slice(&(edata.len() as u16).to_le_bytes());
            buf.extend_from_slice(&edata);
        }

        buf
    }

    /// Decode from bytes received over BLE
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < MIN_DOCUMENT_SIZE {
            return None;
        }

        // Header
        let version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let node_id = NodeId::new(u32::from_le_bytes([data[4], data[5], data[6], data[7]]));

        // GCounter (starts at offset 8)
        let counter = GCounter::decode(&data[8..])?;

        // Calculate where counter ends
        let num_entries = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let mut offset = 8 + 4 + num_entries * 12;

        let mut peripheral = None;
        let mut emergency = None;

        // Parse extended sections (can have peripheral and/or emergency)
        while offset < data.len() {
            let marker = data[offset];

            if marker == EXTENDED_MARKER {
                // Parse peripheral section
                if data.len() < offset + 4 {
                    break;
                }
                let _reserved = data[offset + 1];
                let section_len = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;

                let section_start = offset + 4;
                if data.len() < section_start + section_len {
                    break;
                }

                peripheral = Peripheral::decode(&data[section_start..section_start + section_len]);
                offset = section_start + section_len;
            } else if marker == EMERGENCY_MARKER {
                // Parse emergency section
                if data.len() < offset + 4 {
                    break;
                }
                let _reserved = data[offset + 1];
                let section_len = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;

                let section_start = offset + 4;
                if data.len() < section_start + section_len {
                    break;
                }

                emergency =
                    EmergencyEvent::decode(&data[section_start..section_start + section_len]);
                offset = section_start + section_len;
            } else {
                // Unknown marker, stop parsing
                break;
            }
        }

        Some(Self {
            version,
            node_id,
            counter,
            peripheral,
            emergency,
        })
    }

    /// Get the total counter value
    pub fn total_count(&self) -> u64 {
        self.counter.value()
    }

    /// Get the encoded size of this document
    ///
    /// Use this to check if the document fits within BLE MTU constraints.
    pub fn encoded_size(&self) -> usize {
        let counter_size = 4 + self.counter.node_count_total() * 12;
        let peripheral_size = self.peripheral.as_ref().map_or(0, |p| 4 + p.encode().len());
        let emergency_size = self.emergency.as_ref().map_or(0, |e| 4 + e.encode().len());
        8 + counter_size + peripheral_size + emergency_size
    }

    /// Check if the document exceeds the target size for single-packet transmission
    ///
    /// Returns `true` if the document is larger than [`TARGET_DOCUMENT_SIZE`].
    pub fn exceeds_target_size(&self) -> bool {
        self.encoded_size() > TARGET_DOCUMENT_SIZE
    }

    /// Check if the document exceeds the maximum size
    ///
    /// Returns `true` if the document is larger than [`MAX_DOCUMENT_SIZE`].
    pub fn exceeds_max_size(&self) -> bool {
        self.encoded_size() > MAX_DOCUMENT_SIZE
    }
}

/// Result from merging a received document
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// Node ID that sent this document
    pub source_node: NodeId,

    /// Event contained in the document (if any)
    pub event: Option<PeripheralEvent>,

    /// Whether the counter changed (indicates new data)
    pub counter_changed: bool,

    /// Whether the emergency state changed (new emergency or ACK updates)
    pub emergency_changed: bool,

    /// Updated total count after merge
    pub total_count: u64,
}

impl MergeResult {
    /// Check if this result contains an emergency event
    pub fn is_emergency(&self) -> bool {
        self.event
            .as_ref()
            .is_some_and(|e| e.event_type == EventType::Emergency)
    }

    /// Check if this result contains an ACK event
    pub fn is_ack(&self) -> bool {
        self.event
            .as_ref()
            .is_some_and(|e| e.event_type == EventType::Ack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::crdt::PeripheralType;

    #[test]
    fn test_document_encode_decode_minimal() {
        let node_id = NodeId::new(0x12345678);
        let doc = HiveDocument::new(node_id);

        let encoded = doc.encode();
        assert_eq!(encoded.len(), 12); // 8 header + 4 counter (0 entries)

        let decoded = HiveDocument::decode(&encoded).unwrap();
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.node_id.as_u32(), 0x12345678);
        assert_eq!(decoded.counter.value(), 0);
        assert!(decoded.peripheral.is_none());
    }

    #[test]
    fn test_document_encode_decode_with_counter() {
        let node_id = NodeId::new(0x12345678);
        let mut doc = HiveDocument::new(node_id);
        doc.increment_counter();
        doc.increment_counter();

        let encoded = doc.encode();
        // 8 header + 4 num_entries + 1 entry (12 bytes) = 24
        assert_eq!(encoded.len(), 24);

        let decoded = HiveDocument::decode(&encoded).unwrap();
        assert_eq!(decoded.counter.value(), 2);
    }

    #[test]
    fn test_document_encode_decode_with_peripheral() {
        let node_id = NodeId::new(0x12345678);
        let peripheral =
            Peripheral::new(0xAABBCCDD, PeripheralType::SoldierSensor).with_callsign("ALPHA-1");

        let doc = HiveDocument::new(node_id).with_peripheral(peripheral);

        let encoded = doc.encode();
        let decoded = HiveDocument::decode(&encoded).unwrap();

        assert!(decoded.peripheral.is_some());
        let p = decoded.peripheral.unwrap();
        assert_eq!(p.id, 0xAABBCCDD);
        assert_eq!(p.callsign_str(), "ALPHA-1");
    }

    #[test]
    fn test_document_encode_decode_with_event() {
        let node_id = NodeId::new(0x12345678);
        let mut peripheral = Peripheral::new(0xAABBCCDD, PeripheralType::SoldierSensor);
        peripheral.set_event(EventType::Emergency, 1234567890);

        let doc = HiveDocument::new(node_id).with_peripheral(peripheral);

        let encoded = doc.encode();
        let decoded = HiveDocument::decode(&encoded).unwrap();

        assert!(decoded.peripheral.is_some());
        let p = decoded.peripheral.unwrap();
        assert!(p.last_event.is_some());
        let event = p.last_event.unwrap();
        assert_eq!(event.event_type, EventType::Emergency);
        assert_eq!(event.timestamp, 1234567890);
    }

    #[test]
    fn test_document_merge() {
        let node1 = NodeId::new(0x11111111);
        let node2 = NodeId::new(0x22222222);

        let mut doc1 = HiveDocument::new(node1);
        doc1.increment_counter();

        let mut doc2 = HiveDocument::new(node2);
        doc2.counter.increment(&node2, 3);

        // Merge doc2 into doc1
        let changed = doc1.merge(&doc2);
        assert!(changed);
        assert_eq!(doc1.counter.value(), 4); // 1 + 3
    }

    #[test]
    fn test_merge_result_helpers() {
        let emergency_event = PeripheralEvent::new(EventType::Emergency, 123);
        let result = MergeResult {
            source_node: NodeId::new(0x12345678),
            event: Some(emergency_event),
            counter_changed: true,
            emergency_changed: false,
            total_count: 10,
        };

        assert!(result.is_emergency());
        assert!(!result.is_ack());

        let ack_event = PeripheralEvent::new(EventType::Ack, 456);
        let result = MergeResult {
            source_node: NodeId::new(0x12345678),
            event: Some(ack_event),
            counter_changed: false,
            emergency_changed: false,
            total_count: 10,
        };

        assert!(!result.is_emergency());
        assert!(result.is_ack());
    }

    #[test]
    fn test_document_size_calculation() {
        use crate::sync::crdt::PeripheralType;

        let node_id = NodeId::new(0x12345678);

        // Minimal document: 8 header + 4 counter (0 entries) = 12 bytes
        let doc = HiveDocument::new(node_id);
        assert_eq!(doc.encoded_size(), 12);
        assert!(!doc.exceeds_target_size());

        // With one counter entry: 8 + (4 + 12) = 24 bytes
        let mut doc = HiveDocument::new(node_id);
        doc.increment_counter();
        assert_eq!(doc.encoded_size(), 24);

        // With peripheral: adds ~42 bytes (4 marker/len + 38 data)
        let peripheral = Peripheral::new(0xAABBCCDD, PeripheralType::SoldierSensor);
        let doc = HiveDocument::new(node_id).with_peripheral(peripheral);
        let encoded = doc.encode();
        assert_eq!(doc.encoded_size(), encoded.len());

        // Verify size stays under target for reasonable mesh
        let mut doc = HiveDocument::new(node_id);
        for i in 0..10 {
            doc.counter.increment(&NodeId::new(i), 1);
        }
        assert!(doc.encoded_size() < TARGET_DOCUMENT_SIZE);
        assert!(!doc.exceeds_max_size());
    }
}
