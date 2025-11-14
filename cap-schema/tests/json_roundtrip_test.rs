//! Test protobuf ↔ JSON roundtrip serialization
//!
//! Validates that protobuf messages can be serialized to JSON and back
//! for Ditto CRDT storage (which requires structured JSON, not binary blobs).

use cap_schema::common::v1::Position;
use cap_schema::hierarchy::v1::SquadSummary;

#[test]
fn test_squad_summary_json_roundtrip() {
    // Create a SquadSummary protobuf message
    let original = SquadSummary {
        squad_id: "squad-1".to_string(),
        leader_id: "node-123".to_string(),
        member_count: 5,
        member_ids: vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ],
        position_centroid: Some(Position {
            latitude: 37.7749,
            longitude: -122.4194,
            altitude: 10.0,
        }),
        avg_fuel_minutes: 45.5,
        ..Default::default()
    };

    // Serialize to JSON
    let json_value =
        serde_json::to_value(&original).expect("Failed to serialize SquadSummary to JSON");

    println!(
        "JSON representation:\n{}",
        serde_json::to_string_pretty(&json_value).unwrap()
    );

    // Verify JSON structure (note: protobuf uses snake_case in JSON)
    assert_eq!(json_value["squad_id"], "squad-1");
    assert_eq!(json_value["leader_id"], "node-123");
    assert_eq!(json_value["member_count"], 5);
    assert!(json_value["member_ids"].is_array());
    assert_eq!(json_value["member_ids"].as_array().unwrap().len(), 3);
    assert!(json_value["position_centroid"].is_object());
    assert_eq!(json_value["position_centroid"]["latitude"], 37.7749);

    // Deserialize back to protobuf
    let roundtrip: SquadSummary =
        serde_json::from_value(json_value).expect("Failed to deserialize JSON to SquadSummary");

    // Verify roundtrip integrity
    assert_eq!(roundtrip.squad_id, original.squad_id);
    assert_eq!(roundtrip.leader_id, original.leader_id);
    assert_eq!(roundtrip.member_count, original.member_count);
    assert_eq!(roundtrip.member_ids, original.member_ids);
    assert!(roundtrip.position_centroid.is_some());
    let pos = roundtrip.position_centroid.unwrap();
    assert_eq!(pos.latitude, 37.7749);
    assert_eq!(pos.longitude, -122.4194);
    assert_eq!(pos.altitude, 10.0);
}

#[test]
fn test_json_structure_for_ditto_crdt() {
    // Verify that JSON expansion gives us the structure Ditto needs for CRDT
    let summary = SquadSummary {
        squad_id: "squad-1".to_string(),
        leader_id: "node-123".to_string(),
        member_count: 5,
        member_ids: vec!["node-1".to_string(), "node-2".to_string()],
        ..Default::default()
    };

    let json = serde_json::to_value(&summary).unwrap();

    // Ditto needs flat, queryable fields for CRDT operations:
    // - OR-Set for member_ids (array with add/remove tracking)
    // - LWW-Register for scalar fields (leader_id, member_count)

    // Verify array structure (for OR-Set CRDT)
    let members = json["member_ids"]
        .as_array()
        .expect("member_ids should be array");
    assert_eq!(members.len(), 2);
    assert_eq!(members[0], "node-1");
    assert_eq!(members[1], "node-2");

    // Verify scalar fields (for LWW-Register CRDT)
    assert_eq!(json["leader_id"].as_str().unwrap(), "node-123");
    assert_eq!(json["member_count"].as_i64().unwrap(), 5);

    // This JSON structure allows Ditto to:
    // 1. Merge member_ids arrays with OR-Set semantics (track additions/removals)
    // 2. Merge scalar fields with LWW (last-write-wins based on timestamp)
    // 3. Send delta updates (only changed fields, not entire blob)
}

#[test]
fn test_nested_message_expansion() {
    // Test that nested messages are fully expanded to JSON
    let summary = SquadSummary {
        squad_id: "squad-1".to_string(),
        position_centroid: Some(Position {
            latitude: 37.7749,
            longitude: -122.4194,
            altitude: 10.0,
        }),
        ..Default::default()
    };

    let json = serde_json::to_value(&summary).unwrap();

    // Nested Position should be expanded as JSON object
    let pos = json["position_centroid"]
        .as_object()
        .expect("position_centroid should be object");
    assert!(pos.contains_key("latitude"));
    assert!(pos.contains_key("longitude"));
    assert!(pos.contains_key("altitude"));

    // Ditto can now merge position fields independently
    // (e.g., latitude change doesn't affect longitude)
}
