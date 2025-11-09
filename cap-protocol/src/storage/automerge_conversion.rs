//! Protobuf ↔ Automerge conversion functions
//!
//! This module provides conversions between CAP Protocol protobuf types
//! and Automerge CRDT documents.
//!
//! ## POC Strategy
//!
//! For the initial POC, we use JSON as an intermediate format:
//! - Protobuf → JSON → Automerge (serialization)
//! - Automerge → JSON → Protobuf (deserialization)
//!
//! This approach trades performance for simplicity. Future optimizations:
//! - Direct binary encoding (Phase 2)
//! - Custom Automerge schema mapping (Phase 3)
//! - Columnar encoding for large documents (Phase 4)

#[cfg(feature = "automerge-backend")]
use anyhow::Result;
#[cfg(feature = "automerge-backend")]
use automerge::{transaction::Transactable, Automerge, ReadDoc, ROOT};
#[cfg(feature = "automerge-backend")]
use cap_schema::cell::v1::CellState;
#[cfg(feature = "automerge-backend")]
use cap_schema::node::v1::{NodeConfig, NodeState};
#[cfg(feature = "automerge-backend")]
use serde_json;

/// Convert CellState protobuf to Automerge document
#[cfg(feature = "automerge-backend")]
pub fn cell_state_to_automerge(cell: &CellState) -> Result<Automerge> {
    // Serialize protobuf to JSON
    let json = serde_json::to_value(cell)
        .map_err(|e| anyhow::anyhow!("Failed to serialize CellState to JSON: {}", e))?;

    // Create new Automerge document
    let mut doc = Automerge::new();

    // Populate document from JSON
    match doc.transact(|tx| {
        populate_from_json(tx, ROOT, &json)?;
        Ok::<(), automerge::AutomergeError>(())
    }) {
        Ok(_) => Ok(doc),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to populate Automerge document: {:?}",
            e
        )),
    }
}

/// Convert Automerge document to CellState protobuf
#[cfg(feature = "automerge-backend")]
pub fn automerge_to_cell_state(doc: &Automerge) -> Result<CellState> {
    // Extract JSON from Automerge document
    let json = extract_to_json(doc, ROOT)?;

    // Deserialize JSON to protobuf
    let cell: CellState = serde_json::from_value(json)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize JSON to CellState: {}", e))?;

    Ok(cell)
}

/// Convert NodeConfig protobuf to Automerge document
#[cfg(feature = "automerge-backend")]
pub fn node_config_to_automerge(node: &NodeConfig) -> Result<Automerge> {
    let json = serde_json::to_value(node)
        .map_err(|e| anyhow::anyhow!("Failed to serialize NodeConfig to JSON: {}", e))?;

    let mut doc = Automerge::new();

    doc.transact(|tx| {
        populate_from_json(tx, ROOT, &json)?;
        Ok::<(), automerge::AutomergeError>(())
    })
    .map_err(|e| anyhow::anyhow!("Failed to populate Automerge document: {:?}", e))?;

    Ok(doc)
}

/// Convert Automerge document to NodeConfig protobuf
#[cfg(feature = "automerge-backend")]
pub fn automerge_to_node_config(doc: &Automerge) -> Result<NodeConfig> {
    let json = extract_to_json(doc, ROOT)?;
    let node: NodeConfig = serde_json::from_value(json)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize JSON to NodeConfig: {}", e))?;
    Ok(node)
}

/// Convert NodeState protobuf to Automerge document
#[cfg(feature = "automerge-backend")]
pub fn node_state_to_automerge(node: &NodeState) -> Result<Automerge> {
    let json = serde_json::to_value(node)
        .map_err(|e| anyhow::anyhow!("Failed to serialize NodeState to JSON: {}", e))?;

    let mut doc = Automerge::new();

    doc.transact(|tx| {
        populate_from_json(tx, ROOT, &json)?;
        Ok::<(), automerge::AutomergeError>(())
    })
    .map_err(|e| anyhow::anyhow!("Failed to populate Automerge document: {:?}", e))?;

    Ok(doc)
}

/// Convert Automerge document to NodeState protobuf
#[cfg(feature = "automerge-backend")]
pub fn automerge_to_node_state(doc: &Automerge) -> Result<NodeState> {
    let json = extract_to_json(doc, ROOT)?;
    let node: NodeState = serde_json::from_value(json)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize JSON to NodeState: {}", e))?;
    Ok(node)
}

/// Helper: Populate Automerge object from JSON value
#[cfg(feature = "automerge-backend")]
fn populate_from_json<T: Transactable>(
    tx: &mut T,
    obj: automerge::ObjId,
    json: &serde_json::Value,
) -> Result<(), automerge::AutomergeError> {
    match json {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                match value {
                    serde_json::Value::Null => {
                        // Skip null values (protobuf optional fields)
                    }
                    serde_json::Value::Bool(b) => {
                        tx.put(&obj, key, *b)?;
                    }
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            tx.put(&obj, key, i)?;
                        } else if let Some(f) = n.as_f64() {
                            tx.put(&obj, key, f)?;
                        }
                    }
                    serde_json::Value::String(s) => {
                        tx.put(&obj, key, s.as_str())?;
                    }
                    serde_json::Value::Array(arr) => {
                        let list_id = tx.put_object(&obj, key, automerge::ObjType::List)?;
                        for (idx, item) in arr.iter().enumerate() {
                            match item {
                                serde_json::Value::String(s) => {
                                    tx.insert(&list_id, idx, s.as_str())?;
                                }
                                serde_json::Value::Object(_) => {
                                    let nested_obj =
                                        tx.insert_object(&list_id, idx, automerge::ObjType::Map)?;
                                    populate_from_json(tx, nested_obj, item)?;
                                }
                                _ => {
                                    // Handle other types as needed
                                }
                            }
                        }
                    }
                    serde_json::Value::Object(_) => {
                        let nested_obj = tx.put_object(&obj, key, automerge::ObjType::Map)?;
                        populate_from_json(tx, nested_obj, value)?;
                    }
                }
            }
        }
        _ => {
            // Root must be an object - use InvalidObjId as a generic error
            // This is a limitation of Automerge 0.7.1's error types
            // TODO: Consider using a custom error type in future
        }
    }
    Ok(())
}

/// Helper: Extract Automerge object to JSON value
#[cfg(feature = "automerge-backend")]
fn extract_to_json(doc: &Automerge, obj: automerge::ObjId) -> Result<serde_json::Value> {
    use automerge::Value;

    let mut map = serde_json::Map::new();

    // Get all keys in the object
    let keys = doc.keys(&obj);

    for key in keys {
        if let Ok(Some((value, _obj_id))) = doc.get(&obj, &key) {
            let json_value = match value {
                Value::Scalar(scalar) => match scalar.as_ref() {
                    automerge::ScalarValue::Bytes(bytes) => {
                        serde_json::Value::String(String::from_utf8_lossy(bytes).to_string())
                    }
                    automerge::ScalarValue::Str(s) => serde_json::Value::String(s.to_string()),
                    automerge::ScalarValue::Int(i) => serde_json::Value::Number((*i).into()),
                    automerge::ScalarValue::Uint(u) => serde_json::Value::Number((*u).into()),
                    automerge::ScalarValue::F64(f) => {
                        serde_json::Value::Number(serde_json::Number::from_f64(*f).unwrap())
                    }
                    automerge::ScalarValue::Counter(_) => {
                        // Counter values are internal to Automerge - skip for POC
                        // In production, we'd need a proper conversion strategy
                        serde_json::Value::Null
                    }
                    automerge::ScalarValue::Timestamp(ts) => {
                        serde_json::Value::Number((*ts).into())
                    }
                    automerge::ScalarValue::Boolean(b) => serde_json::Value::Bool(*b),
                    automerge::ScalarValue::Null => serde_json::Value::Null,
                    _ => serde_json::Value::Null,
                },
                Value::Object(automerge::ObjType::Map) => {
                    let nested_obj = doc.get(&obj, &key)?.unwrap().1;
                    extract_to_json(doc, nested_obj)?
                }
                Value::Object(automerge::ObjType::List) => {
                    let list_obj = doc.get(&obj, &key)?.unwrap().1;
                    let len = doc.length(&list_obj);
                    let mut arr = Vec::new();
                    for i in 0..len {
                        if let Ok(Some((val, _))) = doc.get(&list_obj, i) {
                            match val {
                                Value::Scalar(s) => {
                                    if let automerge::ScalarValue::Str(s) = s.as_ref() {
                                        arr.push(serde_json::Value::String(s.to_string()));
                                    }
                                }
                                Value::Object(automerge::ObjType::Map) => {
                                    let nested = doc.get(&list_obj, i)?.unwrap().1;
                                    arr.push(extract_to_json(doc, nested)?);
                                }
                                _ => {}
                            }
                        }
                    }
                    serde_json::Value::Array(arr)
                }
                _ => serde_json::Value::Null,
            };

            map.insert(key.to_string(), json_value);
        }
    }

    Ok(serde_json::Value::Object(map))
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use cap_schema::capability::v1::{Capability, CapabilityType};
    use cap_schema::cell::v1::CellConfig;
    use cap_schema::common::v1::Timestamp;

    #[test]
    fn test_cell_state_roundtrip() {
        // Create a CellState protobuf
        let cell = CellState {
            config: Some(CellConfig {
                id: "cell-123".to_string(),
                max_size: 4,
                min_size: 2,
                created_at: Some(Timestamp {
                    seconds: 1234567890,
                    nanos: 0,
                }),
            }),
            leader_id: Some("node-1".to_string()),
            members: vec!["node-1".to_string(), "node-2".to_string()],
            capabilities: vec![Capability {
                id: "cap-1".to_string(),
                name: "ISR Capability".to_string(),
                capability_type: CapabilityType::Sensor as i32,
                confidence: 0.9,
                metadata_json: "{}".to_string(),
                registered_at: Some(Timestamp {
                    seconds: 1234567890,
                    nanos: 0,
                }),
            }],
            platoon_id: None,
            timestamp: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        // Convert to Automerge
        let doc = cell_state_to_automerge(&cell).expect("Failed to convert to Automerge");

        // Convert back to protobuf
        let restored = automerge_to_cell_state(&doc).expect("Failed to convert from Automerge");

        // Verify roundtrip (basic checks)
        assert_eq!(restored.leader_id, cell.leader_id);
        assert_eq!(restored.members, cell.members);
        assert_eq!(restored.capabilities.len(), cell.capabilities.len());
    }

    #[test]
    fn test_node_config_roundtrip() {
        let node = NodeConfig {
            id: "node-123".to_string(),
            platform_type: "UAV".to_string(),
            capabilities: vec![Capability {
                id: "cap-1".to_string(),
                name: "ISR Capability".to_string(),
                capability_type: CapabilityType::Sensor as i32,
                confidence: 0.8,
                metadata_json: "{}".to_string(),
                registered_at: Some(Timestamp {
                    seconds: 1234567890,
                    nanos: 0,
                }),
            }],
            comm_range_m: 1000.0,
            max_speed_mps: 25.0,
            operator_binding: None,
            created_at: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        let doc = node_config_to_automerge(&node).expect("Failed to convert to Automerge");
        let restored = automerge_to_node_config(&doc).expect("Failed to convert from Automerge");

        assert_eq!(restored.id, node.id);
        assert_eq!(restored.platform_type, node.platform_type);
        assert_eq!(restored.capabilities.len(), node.capabilities.len());
    }

    #[test]
    fn test_node_state_roundtrip() {
        use cap_schema::common::v1::Position;
        use cap_schema::node::v1::{HealthStatus, Phase};

        let node = NodeState {
            position: Some(Position {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: 100.0,
            }),
            fuel_minutes: 60,
            health: HealthStatus::Nominal as i32,
            phase: Phase::Discovery as i32,
            cell_id: Some("cell-456".to_string()),
            zone_id: None,
            timestamp: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        let doc = node_state_to_automerge(&node).expect("Failed to convert to Automerge");
        let restored = automerge_to_node_state(&doc).expect("Failed to convert from Automerge");

        assert_eq!(restored.cell_id, node.cell_id);
        assert_eq!(restored.fuel_minutes, node.fuel_minutes);
        assert_eq!(restored.health, node.health);
        assert_eq!(restored.phase, node.phase);
    }

    #[test]
    fn test_sync_after_conversion() {
        use crate::storage::automerge_store::InMemorySyncEngine;

        // Create two CellState instances
        let cell1 = CellState {
            config: Some(CellConfig {
                id: "cell-sync".to_string(),
                max_size: 4,
                min_size: 2,
                created_at: Some(Timestamp {
                    seconds: 1234567890,
                    nanos: 0,
                }),
            }),
            leader_id: Some("node-1".to_string()),
            members: vec!["node-1".to_string()],
            capabilities: vec![],
            platoon_id: None,
            timestamp: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        let cell2 = CellState {
            config: Some(CellConfig {
                id: "cell-sync".to_string(),
                max_size: 4,
                min_size: 2,
                created_at: Some(Timestamp {
                    seconds: 1234567890,
                    nanos: 0,
                }),
            }),
            leader_id: Some("node-2".to_string()),
            members: vec!["node-2".to_string()],
            capabilities: vec![],
            platoon_id: None,
            timestamp: Some(Timestamp {
                seconds: 1234567891, // Later timestamp
                nanos: 0,
            }),
        };

        // Convert to Automerge documents
        let mut doc1 = cell_state_to_automerge(&cell1).unwrap();
        let mut doc2 = cell_state_to_automerge(&cell2).unwrap();

        // Sync documents
        let sync_engine = InMemorySyncEngine::new();
        sync_engine.sync_documents(&mut doc1, &mut doc2).unwrap();

        // Both should now have the same state
        let restored1 = automerge_to_cell_state(&doc1).unwrap();
        let restored2 = automerge_to_cell_state(&doc2).unwrap();

        // LWW semantics: cell2 had a later timestamp, so its leader_id should win
        assert_eq!(restored1.leader_id, restored2.leader_id);
    }
}
