//! Document synchronization for HIVE BLE mesh
//!
//! This module provides centralized document state management for HIVE-Lite nodes.
//! It manages the local CRDT state (GCounter) and handles merging with received documents.
//!
//! ## Design Notes
//!
//! This implementation uses a simple GCounter for resource-constrained devices (ESP32,
//! smartwatches). For full HIVE nodes using AutomergeIroh, this component can be replaced
//! or extended - the observer pattern and BLE transport layer are independent of the
//! document format.
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::document_sync::DocumentSync;
//! use hive_btle::NodeId;
//!
//! let sync = DocumentSync::new(NodeId::new(0x12345678), "SOLDIER-1");
//!
//! // Trigger an emergency
//! let doc_bytes = sync.send_emergency();
//! // ... broadcast doc_bytes over BLE
//!
//! // Handle received document
//! if let Some(result) = sync.merge_document(&received_data) {
//!     if result.is_emergency() {
//!         println!("EMERGENCY from {:08X}", result.source_node.as_u32());
//!     }
//! }
//! ```

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};
#[cfg(feature = "std")]
use std::sync::RwLock;

#[cfg(not(feature = "std"))]
use spin::RwLock;

use core::sync::atomic::{AtomicU32, Ordering};

use crate::document::{HiveDocument, MergeResult};
use crate::sync::crdt::{EmergencyEvent, EventType, GCounter, Peripheral, PeripheralType};
use crate::NodeId;

/// Document synchronization manager for HIVE-Lite nodes
///
/// Manages the local CRDT state and handles document serialization/merging.
/// Thread-safe for use from multiple BLE callbacks.
///
/// ## Integration with Full HIVE
///
/// This implementation uses a simple GCounter suitable for embedded devices.
/// For integration with the larger HIVE project using AutomergeIroh:
/// - The `build_document()` output can be wrapped in an Automerge-compatible format
/// - The observer events (Emergency, Ack, DocumentSynced) work with any backend
/// - The BLE transport layer is document-format agnostic
pub struct DocumentSync {
    /// Our node ID
    node_id: NodeId,

    /// CRDT G-Counter for mesh activity tracking
    counter: RwLock<GCounter>,

    /// Peripheral data (callsign, type, location)
    peripheral: RwLock<Peripheral>,

    /// Active emergency event with ACK tracking (CRDT)
    emergency: RwLock<Option<EmergencyEvent>>,

    /// Document version (monotonically increasing)
    version: AtomicU32,
}

impl DocumentSync {
    /// Create a new document sync manager
    pub fn new(node_id: NodeId, callsign: &str) -> Self {
        let peripheral = Peripheral::new(node_id.as_u32(), PeripheralType::SoldierSensor)
            .with_callsign(callsign);

        Self {
            node_id,
            counter: RwLock::new(GCounter::new()),
            peripheral: RwLock::new(peripheral),
            emergency: RwLock::new(None),
            version: AtomicU32::new(1),
        }
    }

    /// Create with a specific peripheral type
    pub fn with_peripheral_type(node_id: NodeId, callsign: &str, ptype: PeripheralType) -> Self {
        let peripheral = Peripheral::new(node_id.as_u32(), ptype).with_callsign(callsign);

        Self {
            node_id,
            counter: RwLock::new(GCounter::new()),
            peripheral: RwLock::new(peripheral),
            emergency: RwLock::new(None),
            version: AtomicU32::new(1),
        }
    }

    /// Get our node ID
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get the current document version
    pub fn version(&self) -> u32 {
        self.version.load(Ordering::Relaxed)
    }

    /// Get the total counter value
    pub fn total_count(&self) -> u64 {
        self.counter.read().unwrap().value()
    }

    /// Get our counter contribution
    pub fn local_count(&self) -> u64 {
        self.counter.read().unwrap().node_count(&self.node_id)
    }

    /// Get current event type (if any)
    pub fn current_event(&self) -> Option<EventType> {
        self.peripheral
            .read()
            .unwrap()
            .last_event
            .as_ref()
            .map(|e| e.event_type)
    }

    /// Check if we're in emergency state
    pub fn is_emergency_active(&self) -> bool {
        self.current_event() == Some(EventType::Emergency)
    }

    /// Check if we've sent an ACK
    pub fn is_ack_active(&self) -> bool {
        self.current_event() == Some(EventType::Ack)
    }

    /// Get the callsign
    pub fn callsign(&self) -> String {
        self.peripheral.read().unwrap().callsign_str().to_string()
    }

    // ==================== State Mutations ====================

    /// Send an emergency - returns the document bytes to broadcast
    pub fn send_emergency(&self, timestamp: u64) -> Vec<u8> {
        // Set emergency event
        {
            let mut peripheral = self.peripheral.write().unwrap();
            peripheral.set_event(EventType::Emergency, timestamp);
        }

        // Increment counter
        self.increment_counter_internal();

        // Build and return document
        self.build_document()
    }

    /// Send an ACK - returns the document bytes to broadcast
    pub fn send_ack(&self, timestamp: u64) -> Vec<u8> {
        // Set ACK event
        {
            let mut peripheral = self.peripheral.write().unwrap();
            peripheral.set_event(EventType::Ack, timestamp);
        }

        // Increment counter
        self.increment_counter_internal();

        // Build and return document
        self.build_document()
    }

    /// Clear the current event
    pub fn clear_event(&self) {
        let mut peripheral = self.peripheral.write().unwrap();
        peripheral.clear_event();
        self.bump_version();
    }

    /// Increment the counter (for periodic sync)
    pub fn increment_counter(&self) {
        self.increment_counter_internal();
    }

    /// Update health status (battery percentage)
    pub fn update_health(&self, battery_percent: u8) {
        let mut peripheral = self.peripheral.write().unwrap();
        peripheral.health.battery_percent = battery_percent;
        self.bump_version();
    }

    // ==================== Emergency Management ====================

    /// Start a new emergency event
    ///
    /// Creates an emergency event that tracks ACKs from all known peers.
    /// Returns the document bytes to broadcast.
    pub fn start_emergency(&self, timestamp: u64, known_peers: &[u32]) -> Vec<u8> {
        // Create emergency event with our node as source
        {
            let mut emergency = self.emergency.write().unwrap();
            *emergency = Some(EmergencyEvent::new(
                self.node_id.as_u32(),
                timestamp,
                known_peers,
            ));
        }

        // Also set peripheral event for backward compatibility
        {
            let mut peripheral = self.peripheral.write().unwrap();
            peripheral.set_event(EventType::Emergency, timestamp);
        }

        self.increment_counter_internal();
        self.build_document()
    }

    /// Record our ACK for the current emergency
    ///
    /// Returns the document bytes to broadcast, or None if no emergency is active.
    pub fn ack_emergency(&self, timestamp: u64) -> Option<Vec<u8>> {
        let changed = {
            let mut emergency = self.emergency.write().unwrap();
            if let Some(ref mut e) = *emergency {
                e.ack(self.node_id.as_u32())
            } else {
                return None;
            }
        };

        if changed {
            // Also set peripheral event for backward compatibility
            {
                let mut peripheral = self.peripheral.write().unwrap();
                peripheral.set_event(EventType::Ack, timestamp);
            }

            self.increment_counter_internal();
        }

        Some(self.build_document())
    }

    /// Clear the current emergency event
    pub fn clear_emergency(&self) {
        let mut emergency = self.emergency.write().unwrap();
        if emergency.is_some() {
            *emergency = None;
            drop(emergency);

            // Also clear peripheral event
            let mut peripheral = self.peripheral.write().unwrap();
            peripheral.clear_event();

            self.bump_version();
        }
    }

    /// Check if there's an active emergency
    pub fn has_active_emergency(&self) -> bool {
        self.emergency.read().unwrap().is_some()
    }

    /// Get emergency status info
    ///
    /// Returns (source_node, timestamp, acked_count, pending_count) if emergency is active.
    pub fn get_emergency_status(&self) -> Option<(u32, u64, usize, usize)> {
        let emergency = self.emergency.read().unwrap();
        emergency.as_ref().map(|e| {
            (
                e.source_node(),
                e.timestamp(),
                e.ack_count(),
                e.pending_nodes().len(),
            )
        })
    }

    /// Check if a specific peer has ACKed the current emergency
    pub fn has_peer_acked(&self, peer_id: u32) -> bool {
        let emergency = self.emergency.read().unwrap();
        emergency
            .as_ref()
            .map(|e| e.has_acked(peer_id))
            .unwrap_or(false)
    }

    /// Check if all peers have ACKed the current emergency
    pub fn all_peers_acked(&self) -> bool {
        let emergency = self.emergency.read().unwrap();
        emergency.as_ref().map(|e| e.all_acked()).unwrap_or(true)
    }

    // ==================== Document I/O ====================

    /// Build the document for transmission
    ///
    /// Returns the encoded bytes ready for BLE GATT write.
    pub fn build_document(&self) -> Vec<u8> {
        let counter = self.counter.read().unwrap().clone();
        let peripheral = self.peripheral.read().unwrap().clone();
        let emergency = self.emergency.read().unwrap().clone();

        let doc = HiveDocument {
            version: self.version.load(Ordering::Relaxed),
            node_id: self.node_id,
            counter,
            peripheral: Some(peripheral),
            emergency,
        };

        doc.encode()
    }

    /// Merge a received document
    ///
    /// Returns `Some(MergeResult)` if the document was valid, `None` otherwise.
    /// The result contains information about what changed and any events.
    pub fn merge_document(&self, data: &[u8]) -> Option<MergeResult> {
        let received = HiveDocument::decode(data)?;

        // Don't process our own documents
        if received.node_id == self.node_id {
            return None;
        }

        // Merge the counter
        let counter_changed = {
            let mut counter = self.counter.write().unwrap();
            let old_value = counter.value();
            counter.merge(&received.counter);
            counter.value() != old_value
        };

        // Merge emergency event (CRDT merge)
        let emergency_changed = if let Some(ref received_emergency) = received.emergency {
            let mut emergency = self.emergency.write().unwrap();
            match &mut *emergency {
                Some(ref mut our_emergency) => our_emergency.merge(received_emergency),
                None => {
                    *emergency = Some(received_emergency.clone());
                    true
                }
            }
        } else {
            false
        };

        if counter_changed || emergency_changed {
            self.bump_version();
        }

        // Extract event from received document
        let event = received
            .peripheral
            .as_ref()
            .and_then(|p| p.last_event.clone());

        Some(MergeResult {
            source_node: received.node_id,
            event,
            counter_changed,
            emergency_changed,
            total_count: self.total_count(),
        })
    }

    /// Create a document from raw bytes (for inspection without merging)
    pub fn decode_document(data: &[u8]) -> Option<HiveDocument> {
        HiveDocument::decode(data)
    }

    // ==================== Internal Helpers ====================

    fn increment_counter_internal(&self) {
        let mut counter = self.counter.write().unwrap();
        counter.increment(&self.node_id, 1);
        drop(counter);
        self.bump_version();
    }

    fn bump_version(&self) {
        self.version.fetch_add(1, Ordering::Relaxed);
    }
}

/// Result from checking if a document contains an emergency
#[derive(Debug, Clone)]
pub struct DocumentCheck {
    /// Node ID from the document
    pub node_id: NodeId,
    /// Whether this document contains an emergency
    pub is_emergency: bool,
    /// Whether this document contains an ACK
    pub is_ack: bool,
}

impl DocumentCheck {
    /// Quick check of a document without full parsing
    pub fn from_document(data: &[u8]) -> Option<Self> {
        let doc = HiveDocument::decode(data)?;

        let (is_emergency, is_ack) = doc
            .peripheral
            .as_ref()
            .and_then(|p| p.last_event.as_ref())
            .map(|e| {
                (
                    e.event_type == EventType::Emergency,
                    e.event_type == EventType::Ack,
                )
            })
            .unwrap_or((false, false));

        Some(Self {
            node_id: doc.node_id,
            is_emergency,
            is_ack,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_sync_new() {
        let sync = DocumentSync::new(NodeId::new(0x12345678), "ALPHA-1");

        assert_eq!(sync.node_id().as_u32(), 0x12345678);
        assert_eq!(sync.version(), 1);
        assert_eq!(sync.total_count(), 0);
        assert_eq!(sync.callsign(), "ALPHA-1");
        assert!(sync.current_event().is_none());
    }

    #[test]
    fn test_send_emergency() {
        let sync = DocumentSync::new(NodeId::new(0x12345678), "ALPHA-1");

        let doc_bytes = sync.send_emergency(1234567890);

        assert!(!doc_bytes.is_empty());
        assert_eq!(sync.total_count(), 1);
        assert!(sync.is_emergency_active());
        assert!(!sync.is_ack_active());

        // Verify we can decode what we sent
        let doc = HiveDocument::decode(&doc_bytes).unwrap();
        assert_eq!(doc.node_id.as_u32(), 0x12345678);
        assert!(doc.peripheral.is_some());
        let event = doc.peripheral.unwrap().last_event.unwrap();
        assert_eq!(event.event_type, EventType::Emergency);
    }

    #[test]
    fn test_send_ack() {
        let sync = DocumentSync::new(NodeId::new(0x12345678), "ALPHA-1");

        let doc_bytes = sync.send_ack(1234567890);

        assert!(!doc_bytes.is_empty());
        assert_eq!(sync.total_count(), 1);
        assert!(sync.is_ack_active());
        assert!(!sync.is_emergency_active());
    }

    #[test]
    fn test_clear_event() {
        let sync = DocumentSync::new(NodeId::new(0x12345678), "ALPHA-1");

        sync.send_emergency(1000);
        assert!(sync.is_emergency_active());

        sync.clear_event();
        assert!(sync.current_event().is_none());
    }

    #[test]
    fn test_merge_document() {
        let sync1 = DocumentSync::new(NodeId::new(0x11111111), "ALPHA-1");
        let sync2 = DocumentSync::new(NodeId::new(0x22222222), "BRAVO-1");

        // sync2 sends emergency
        let doc_bytes = sync2.send_emergency(1000);

        // sync1 receives and merges
        let result = sync1.merge_document(&doc_bytes);
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.source_node.as_u32(), 0x22222222);
        assert!(result.is_emergency());
        assert!(result.counter_changed);
        assert_eq!(result.total_count, 1);

        // sync1's local count is still 0, but total includes sync2's contribution
        assert_eq!(sync1.local_count(), 0);
        assert_eq!(sync1.total_count(), 1);
    }

    #[test]
    fn test_merge_own_document_ignored() {
        let sync = DocumentSync::new(NodeId::new(0x12345678), "ALPHA-1");

        let doc_bytes = sync.send_emergency(1000);

        // Merging our own document should be ignored
        let result = sync.merge_document(&doc_bytes);
        assert!(result.is_none());
    }

    #[test]
    fn test_version_increments() {
        let sync = DocumentSync::new(NodeId::new(0x12345678), "ALPHA-1");

        assert_eq!(sync.version(), 1);

        sync.increment_counter();
        assert_eq!(sync.version(), 2);

        sync.send_emergency(1000);
        assert_eq!(sync.version(), 3);

        sync.clear_event();
        assert_eq!(sync.version(), 4);
    }

    #[test]
    fn test_document_check() {
        let sync = DocumentSync::new(NodeId::new(0x12345678), "ALPHA-1");

        let emergency_doc = sync.send_emergency(1000);
        let check = DocumentCheck::from_document(&emergency_doc).unwrap();
        assert_eq!(check.node_id.as_u32(), 0x12345678);
        assert!(check.is_emergency);
        assert!(!check.is_ack);

        sync.clear_event();
        let ack_doc = sync.send_ack(2000);
        let check = DocumentCheck::from_document(&ack_doc).unwrap();
        assert!(!check.is_emergency);
        assert!(check.is_ack);
    }

    #[test]
    fn test_counter_merge_idempotent() {
        let sync1 = DocumentSync::new(NodeId::new(0x11111111), "ALPHA-1");
        let sync2 = DocumentSync::new(NodeId::new(0x22222222), "BRAVO-1");

        // sync2 sends something
        let doc_bytes = sync2.send_emergency(1000);

        // sync1 merges twice - second should not change counter
        let result1 = sync1.merge_document(&doc_bytes).unwrap();
        assert!(result1.counter_changed);
        assert_eq!(sync1.total_count(), 1);

        let result2 = sync1.merge_document(&doc_bytes).unwrap();
        assert!(!result2.counter_changed); // No change on re-merge
        assert_eq!(sync1.total_count(), 1);
    }
}
