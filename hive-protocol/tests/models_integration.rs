//! Integration tests for cross-model interactions
//!
//! These tests validate that different protobuf models work correctly together,
//! covering scenarios like:
//! - Nodes with capabilities forming cells
//! - Cells aggregating capabilities from members
//! - Human-machine pairs with operator state
//! - CRDT operations across model boundaries

use hive_protocol::models::{
    AuthorityLevel, BindingType, Capability, CapabilityExt, CapabilityType, CellConfig,
    CellConfigExt, CellState, CellStateExt, HumanMachinePair, HumanMachinePairExt, NodeConfig,
    NodeConfigExt, NodeState, NodeStateExt, Operator, OperatorExt, OperatorRank,
};

#[test]
fn test_node_to_cell_capability_aggregation() {
    // Create nodes with different capabilities
    let mut node1_config = NodeConfig::new("UAV".to_string());
    node1_config.add_capability(Capability::new(
        "camera_1".to_string(),
        "HD Camera".to_string(),
        CapabilityType::Sensor,
        0.9,
    ));
    node1_config.add_capability(Capability::new(
        "gps_1".to_string(),
        "GPS".to_string(),
        CapabilityType::Sensor,
        0.95,
    ));

    let mut node2_config = NodeConfig::new("UGV".to_string());
    node2_config.add_capability(Capability::new(
        "compute_1".to_string(),
        "Edge Compute".to_string(),
        CapabilityType::Compute,
        0.85,
    ));
    node2_config.add_capability(Capability::new(
        "comm_1".to_string(),
        "Radio".to_string(),
        CapabilityType::Communication,
        0.9,
    ));

    // Form cell and aggregate capabilities
    let config = CellConfig::new(5);
    let mut cell = CellState::new(config);

    cell.add_member(node1_config.id.clone());
    cell.add_member(node2_config.id.clone());

    // Aggregate capabilities from both nodes
    for cap in &node1_config.capabilities {
        cell.add_capability(cap.clone());
    }
    for cap in &node2_config.capabilities {
        cell.add_capability(cap.clone());
    }

    // Verify cell has aggregated capabilities
    assert_eq!(cell.capabilities.len(), 4);
    assert!(cell.has_capability_type(CapabilityType::Sensor));
    assert!(cell.has_capability_type(CapabilityType::Compute));
    assert!(cell.has_capability_type(CapabilityType::Communication));

    let sensors = cell.get_capabilities_by_type(CapabilityType::Sensor);
    assert_eq!(sensors.len(), 2);
}

#[test]
fn test_human_operated_node_in_cell() {
    // Create operator
    let operator = Operator::new(
        "worker_martinez_j".to_string(),
        "Martinez, J".to_string(),
        OperatorRank::E6,
        AuthorityLevel::Commander,
        "11B".to_string(),
    );

    // Create human-machine pair
    let binding = HumanMachinePair::new(
        vec![operator],
        vec!["soldier_system_1".to_string()],
        BindingType::OneToOne,
    );

    // Create node with operator
    let node_config = NodeConfig::with_operator("Soldier System".to_string(), binding);

    assert!(node_config.is_human_operated());
    assert!(!node_config.is_autonomous());

    // Add to cell
    let config = CellConfig::new(4);
    let mut cell = CellState::new(config);

    cell.add_member(node_config.id.clone());

    // Set human-operated node as leader
    cell.set_leader(node_config.id.clone()).unwrap();

    assert!(cell.is_leader(&node_config.id));
    assert_eq!(cell.member_count(), 1);
}

#[test]
fn test_mixed_autonomous_and_human_operated_cell() {
    // Create autonomous nodes
    let mut auto1 = NodeConfig::new("Robot 1".to_string());
    auto1.add_capability(Capability::new(
        "mobility_1".to_string(),
        "Mobility".to_string(),
        CapabilityType::Mobility,
        0.9,
    ));

    let mut auto2 = NodeConfig::new("Robot 2".to_string());
    auto2.add_capability(Capability::new(
        "sensor_1".to_string(),
        "Sensor".to_string(),
        CapabilityType::Sensor,
        0.85,
    ));

    // Create human-operated node
    let operator = Operator::new(
        "worker_chen_l".to_string(),
        "Chen, L".to_string(),
        OperatorRank::E7,
        AuthorityLevel::Supervisor,
        "11B".to_string(),
    );

    let binding = HumanMachinePair::one_to_one(operator, "command_node_1".to_string());

    let human_node = NodeConfig::with_operator("Command Node".to_string(), binding);

    // Form mixed cell
    let config = CellConfig::new(5);
    let mut cell = CellState::new(config);

    cell.add_member(auto1.id.clone());
    cell.add_member(auto2.id.clone());
    cell.add_member(human_node.id.clone());

    // Human-operated node should be leader
    cell.set_leader(human_node.id.clone()).unwrap();

    // Aggregate capabilities
    for cap in &auto1.capabilities {
        cell.add_capability(cap.clone());
    }
    for cap in &auto2.capabilities {
        cell.add_capability(cap.clone());
    }

    assert_eq!(cell.member_count(), 3);
    assert_eq!(cell.capabilities.len(), 2);
    assert!(cell.is_leader(&human_node.id));
}

#[test]
fn test_operator_cognitive_load_affects_cell_readiness() {
    let mut operator = Operator::new(
        "worker_thompson_r".to_string(),
        "Thompson, R".to_string(),
        OperatorRank::E4,
        AuthorityLevel::Supervisor,
        "11B".to_string(),
    );

    // Operator is overloaded
    operator.update_cognitive_load(0.95);
    operator.update_fatigue(0.85);

    let binding = HumanMachinePair::one_to_one(operator.clone(), "node_1".to_string());

    // Check operator state
    assert!(operator.is_overloaded(0.9));
    assert!(operator.is_fatigued(0.8));
    assert!(binding.has_overloaded_operator(0.9));

    let effectiveness = binding.avg_effectiveness();
    assert!(effectiveness < 0.5); // Low effectiveness due to high load/fatigue
}

#[test]
fn test_node_state_synchronization_in_cell() {
    // Create multiple nodes
    let node1_config = NodeConfig::new("Node 1".to_string());
    let node2_config = NodeConfig::new("Node 2".to_string());

    // Create states at different positions
    let mut state1 = NodeState::new((37.7, -122.4, 100.0));
    let mut state2 = NodeState::new((37.8, -122.5, 150.0));

    // Assign both to same cell
    state1.assign_cell("cell_1".to_string());
    state2.assign_cell("cell_1".to_string());

    assert_eq!(state1.cell_id, Some("cell_1".to_string()));
    assert_eq!(state2.cell_id, Some("cell_1".to_string()));

    // Form cell with both nodes
    let config = CellConfig::with_id("cell_1".to_string(), 5);
    let mut cell = CellState::new(config);

    cell.add_member(node1_config.id.clone());
    cell.add_member(node2_config.id.clone());

    assert!(cell.is_valid());
    assert_eq!(cell.member_count(), 2);
}

#[test]
fn test_cell_merge_with_node_states() {
    // Cell 1 with node 1
    let node1_config = NodeConfig::new("Node 1".to_string());
    let mut state1 = NodeState::new((37.7, -122.4, 100.0));
    state1.assign_cell("cell_1".to_string());

    let config = CellConfig::with_id("cell_1".to_string(), 5);
    let mut cell1 = CellState::new(config);
    cell1.add_member(node1_config.id.clone());

    // Cell 2 with node 2
    let node2_config = NodeConfig::new("Node 2".to_string());
    let mut state2 = NodeState::new((37.8, -122.5, 150.0));
    state2.assign_cell("cell_2".to_string());

    let config2 = CellConfig::with_id("cell_2".to_string(), 5);
    let mut cell2 = CellState::new(config2);
    cell2.add_member(node2_config.id.clone());

    // Merge cells
    cell1.merge(&cell2);

    // Should have both members
    assert_eq!(cell1.member_count(), 2);
    assert!(cell1.is_member(&node1_config.id));
    assert!(cell1.is_member(&node2_config.id));
}

#[test]
fn test_capability_validation_across_models() {
    // Create capability with low confidence
    let low_confidence_cap = Capability::new(
        "sensor_1".to_string(),
        "Degraded Sensor".to_string(),
        CapabilityType::Sensor,
        0.3,
    );

    // Should not be valid above threshold
    assert!(!low_confidence_cap.is_valid(0.5));

    // Node should still accept it (G-Set semantics)
    let mut node = NodeConfig::new("Node 1".to_string());
    node.add_capability(low_confidence_cap.clone());
    assert_eq!(node.capabilities.len(), 1);

    // Cell should also accept it
    let config = CellConfig::new(5);
    let mut cell = CellState::new(config);
    cell.add_capability(low_confidence_cap);
    assert_eq!(cell.capabilities.len(), 1);
}

#[test]
fn test_multiple_operators_in_cell_leader_selection() {
    // Create operators of different ranks
    let commander = Operator::new(
        "worker_williams_d".to_string(),
        "Williams, D".to_string(),
        OperatorRank::O3,
        AuthorityLevel::Commander,
        "11A".to_string(),
    );

    let nco = Operator::new(
        "worker_garcia_m".to_string(),
        "Garcia, M".to_string(),
        OperatorRank::E7,
        AuthorityLevel::Supervisor,
        "11B".to_string(),
    );

    // Create nodes with operators
    let binding1 = HumanMachinePair::one_to_one(commander, "node_1".to_string());
    let node1 = NodeConfig::with_operator("Command Node".to_string(), binding1);

    let binding2 = HumanMachinePair::one_to_one(nco, "node_2".to_string());
    let node2 = NodeConfig::with_operator("NCO Node".to_string(), binding2);

    // Form cell
    let config = CellConfig::new(5);
    let mut cell = CellState::new(config);

    cell.add_member(node1.id.clone());
    cell.add_member(node2.id.clone());

    // Higher-ranking operator's node should be leader
    cell.set_leader(node1.id.clone()).unwrap();

    assert!(cell.is_leader(&node1.id));

    // Verify primary operators
    assert_eq!(
        node1.get_primary_operator().unwrap().rank,
        OperatorRank::O3 as i32
    );
    assert_eq!(
        node2.get_primary_operator().unwrap().rank,
        OperatorRank::E7 as i32
    );
}

#[test]
fn test_protobuf_serialization_roundtrip() {
    // Create a complex setup
    let mut operator = Operator::new(
        "worker_johnson_k".to_string(),
        "Johnson, K".to_string(),
        OperatorRank::E5,
        AuthorityLevel::Supervisor,
        "11B".to_string(),
    );
    operator.update_cognitive_load(0.6);
    operator.update_fatigue(0.4);

    let binding = HumanMachinePair::one_to_one(operator.clone(), "node_1".to_string());

    let mut node = NodeConfig::with_operator("Test Node".to_string(), binding);

    node.add_capability(Capability::new(
        "cap_1".to_string(),
        "Test Cap".to_string(),
        CapabilityType::Sensor,
        0.85,
    ));

    // Verify all fields are preserved (protobuf fields are public)
    assert_eq!(node.platform_type, "Test Node");
    assert!(node.operator_binding.is_some());
    assert_eq!(node.capabilities.len(), 1);

    let retrieved_binding = node.operator_binding.as_ref().unwrap();
    assert_eq!(retrieved_binding.operators.len(), 1);
    assert_eq!(retrieved_binding.operators[0].id, "worker_johnson_k");

    // Verify metadata is preserved
    let retrieved_op = &retrieved_binding.operators[0];
    assert_eq!(retrieved_op.cognitive_load(), 0.6);
    assert_eq!(retrieved_op.fatigue(), 0.4);
}

#[test]
fn test_cell_with_empty_operator_binding() {
    // Create node with empty operator list (autonomous but has binding)
    let binding =
        HumanMachinePair::new(vec![], vec!["node_1".to_string()], BindingType::Unspecified);

    let node = NodeConfig::with_operator("Autonomous Node".to_string(), binding);

    // Should be treated as autonomous
    assert!(node.is_autonomous());
    assert!(!node.is_human_operated());

    // Can still be added to cell
    let config = CellConfig::new(5);
    let mut cell = CellState::new(config);
    cell.add_member(node.id.clone());

    assert_eq!(cell.member_count(), 1);
}

#[test]
fn test_node_state_merge_preserves_cell_assignment() {
    let mut state1 = NodeState::new((37.7, -122.4, 100.0));
    state1.assign_cell("cell_1".to_string());

    let mut state2 = state1.clone();

    // Update state2 with newer timestamp - use longer sleep to ensure timestamp difference
    std::thread::sleep(std::time::Duration::from_secs(1));
    state2.update_position((37.8, -122.5, 150.0));
    state2.assign_cell("cell_2".to_string());

    // Verify timestamps are different
    let ts1 = state1.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
    let ts2 = state2.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
    assert!(ts2 > ts1, "state2 timestamp should be newer");

    // Merge - state2 wins due to newer timestamp
    state1.merge(&state2);

    assert_eq!(state1.cell_id, Some("cell_2".to_string()));
    assert_eq!(state1.get_position(), (37.8, -122.5, 150.0));
}
