//! Integration tests for emergency event propagation and ACK tracking
//!
//! Tests the EmergencyEvent CRDT and its merge semantics across multiple nodes.

use hive_btle::document::HiveDocument;
use hive_btle::sync::crdt::{Peripheral, PeripheralType};
use hive_btle::NodeId;

/// Test basic emergency creation and encoding
#[test]
fn test_emergency_event_creation() {
    let mut doc = HiveDocument::new(NodeId::new(0x111));
    let peripheral = Peripheral::new(0x111, PeripheralType::SoldierSensor);
    doc.peripheral = Some(peripheral);

    // Set emergency with known peers
    let known_peers = vec![0x222, 0x333, 0x444];
    doc.set_emergency(0x111, 1000, &known_peers);

    assert!(doc.has_emergency());
    let emergency = doc.get_emergency().unwrap();
    assert_eq!(emergency.source_node(), 0x111);
    assert_eq!(emergency.timestamp(), 1000);
    assert!(!emergency.has_acked(0x222));
    assert!(!emergency.has_acked(0x333));
    assert!(!emergency.has_acked(0x444));
}

/// Test ACK recording
#[test]
fn test_emergency_ack_recording() {
    let mut doc = HiveDocument::new(NodeId::new(0x111));
    doc.set_emergency(0x111, 1000, &[0x222, 0x333]);

    // ACK from node 0x222
    let changed = doc.ack_emergency(0x222);
    assert!(changed);

    let emergency = doc.get_emergency().unwrap();
    assert!(emergency.has_acked(0x222));
    assert!(!emergency.has_acked(0x333));

    // Duplicate ACK should not change state
    let changed = doc.ack_emergency(0x222);
    assert!(!changed);
}

/// Test emergency propagation through mesh
#[test]
fn test_emergency_propagation() {
    // Node A initiates emergency
    let mut doc_a = HiveDocument::new(NodeId::new(0xAAA));
    let peripheral = Peripheral::new(0xAAA, PeripheralType::SoldierSensor);
    doc_a.peripheral = Some(peripheral);
    doc_a.set_emergency(0xAAA, 1000, &[0xBBB, 0xCCC]);

    // Node B receives emergency
    let mut doc_b = HiveDocument::new(NodeId::new(0xBBB));

    // Encode and decode to simulate transmission
    let data = doc_a.encode();
    let received = HiveDocument::decode(&data).unwrap();
    let changed = doc_b.merge(&received);

    assert!(changed);
    assert!(doc_b.has_emergency());
    let emergency = doc_b.get_emergency().unwrap();
    assert_eq!(emergency.source_node(), 0xAAA);
}

/// Test ACK propagation back to source
#[test]
fn test_ack_propagation() {
    // Setup: A initiates, B and C receive
    let known_peers = vec![0xBBB, 0xCCC];

    let mut doc_a = HiveDocument::new(NodeId::new(0xAAA));
    doc_a.set_emergency(0xAAA, 1000, &known_peers);

    let mut doc_b = HiveDocument::new(NodeId::new(0xBBB));
    let mut doc_c = HiveDocument::new(NodeId::new(0xCCC));

    // Propagate emergency to B and C
    let data = doc_a.encode();
    doc_b.merge(&HiveDocument::decode(&data).unwrap());
    doc_c.merge(&HiveDocument::decode(&data).unwrap());

    // B sends ACK
    doc_b.ack_emergency(0xBBB);

    // Propagate B's ACK back to A
    let data = doc_b.encode();
    let changed = doc_a.merge(&HiveDocument::decode(&data).unwrap());
    assert!(changed);

    // A should now see B's ACK
    let emergency = doc_a.get_emergency().unwrap();
    assert!(emergency.has_acked(0xBBB));
    assert!(!emergency.has_acked(0xCCC));

    // C sends ACK
    doc_c.ack_emergency(0xCCC);

    // Propagate C's ACK to A
    let data = doc_c.encode();
    doc_a.merge(&HiveDocument::decode(&data).unwrap());

    // A should now see both ACKs
    let emergency = doc_a.get_emergency().unwrap();
    assert!(emergency.has_acked(0xBBB));
    assert!(emergency.has_acked(0xCCC));
}

/// Test ACK collection from multiple paths
#[test]
fn test_ack_merge_from_multiple_paths() {
    // A initiates, B and C ACK independently, D collects both
    let known_peers = vec![0xBBB, 0xCCC, 0xDDD];

    let mut doc_a = HiveDocument::new(NodeId::new(0xAAA));
    doc_a.set_emergency(0xAAA, 1000, &known_peers);

    let mut doc_b = HiveDocument::new(NodeId::new(0xBBB));
    let mut doc_c = HiveDocument::new(NodeId::new(0xCCC));
    let mut doc_d = HiveDocument::new(NodeId::new(0xDDD));

    // All receive emergency from A
    let data = doc_a.encode();
    doc_b.merge(&HiveDocument::decode(&data).unwrap());
    doc_c.merge(&HiveDocument::decode(&data).unwrap());
    doc_d.merge(&HiveDocument::decode(&data).unwrap());

    // B and C ACK independently
    doc_b.ack_emergency(0xBBB);
    doc_c.ack_emergency(0xCCC);

    // D receives from both B and C
    let data_b = doc_b.encode();
    let data_c = doc_c.encode();

    doc_d.merge(&HiveDocument::decode(&data_b).unwrap());
    doc_d.merge(&HiveDocument::decode(&data_c).unwrap());

    // D should have both ACKs
    let emergency = doc_d.get_emergency().unwrap();
    assert!(emergency.has_acked(0xBBB));
    assert!(emergency.has_acked(0xCCC));
    assert!(!emergency.has_acked(0xDDD)); // D hasn't ACKed yet

    // D ACKs
    doc_d.ack_emergency(0xDDD);

    // Now propagate D's full state back to A
    let data_d = doc_d.encode();
    doc_a.merge(&HiveDocument::decode(&data_d).unwrap());

    // A should have all ACKs
    let emergency = doc_a.get_emergency().unwrap();
    assert!(emergency.has_acked(0xBBB));
    assert!(emergency.has_acked(0xCCC));
    assert!(emergency.has_acked(0xDDD));
}

/// Test emergency clear
#[test]
fn test_emergency_clear() {
    let mut doc = HiveDocument::new(NodeId::new(0x111));
    doc.set_emergency(0x111, 1000, &[0x222]);

    assert!(doc.has_emergency());

    doc.clear_emergency();

    assert!(!doc.has_emergency());
    assert!(doc.get_emergency().is_none());
}

/// Test emergency timestamp comparison (newer replaces older)
#[test]
fn test_emergency_supersede() {
    let known_peers = vec![0xBBB, 0xCCC];

    // A creates emergency at time 1000
    let mut doc_a = HiveDocument::new(NodeId::new(0xAAA));
    doc_a.set_emergency(0xAAA, 1000, &known_peers);

    // B receives it
    let mut doc_b = HiveDocument::new(NodeId::new(0xBBB));
    doc_b.merge(&HiveDocument::decode(&doc_a.encode()).unwrap());

    // B creates a NEWER emergency at time 2000
    doc_b.set_emergency(0xBBB, 2000, &known_peers);

    // A receives B's newer emergency
    doc_a.merge(&HiveDocument::decode(&doc_b.encode()).unwrap());

    // A should have B's newer emergency
    let emergency = doc_a.get_emergency().unwrap();
    assert_eq!(emergency.source_node(), 0xBBB);
    assert_eq!(emergency.timestamp(), 2000);
}

/// Test document size with emergency data
#[test]
fn test_emergency_document_size() {
    use hive_btle::document::TARGET_DOCUMENT_SIZE;

    let mut doc = HiveDocument::new(NodeId::new(0x111));
    let peripheral = Peripheral::new(0x111, PeripheralType::SoldierSensor);
    doc.peripheral = Some(peripheral);

    // Small emergency (few peers)
    doc.set_emergency(0x111, 1000, &[0x222, 0x333, 0x444]);

    let size = doc.encoded_size();
    assert!(
        size < TARGET_DOCUMENT_SIZE,
        "Small emergency should fit: {} bytes",
        size
    );

    // Verify encoding/decoding preserves emergency
    let data = doc.encode();
    let decoded = HiveDocument::decode(&data).unwrap();
    assert!(decoded.has_emergency());
    assert_eq!(decoded.get_emergency().unwrap().source_node(), 0x111);
}

/// Test idempotent ACK merges
#[test]
fn test_ack_idempotent_merge() {
    let mut doc_a = HiveDocument::new(NodeId::new(0xAAA));
    doc_a.set_emergency(0xAAA, 1000, &[0xBBB]);

    let mut doc_b = HiveDocument::new(NodeId::new(0xBBB));
    doc_b.merge(&HiveDocument::decode(&doc_a.encode()).unwrap());
    doc_b.ack_emergency(0xBBB);

    // Merge same ACK multiple times
    for _ in 0..5 {
        doc_a.merge(&HiveDocument::decode(&doc_b.encode()).unwrap());
    }

    // Should still only have one ACK
    let emergency = doc_a.get_emergency().unwrap();
    assert!(emergency.has_acked(0xBBB));

    // Count of acked peers should be 1
    let acked_count = [0xBBB]
        .iter()
        .filter(|&&id| emergency.has_acked(id))
        .count();
    assert_eq!(acked_count, 1);
}
