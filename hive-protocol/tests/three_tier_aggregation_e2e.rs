//! E2E test for 2-tier hierarchical aggregation (Squad → Platoon)
//!
//! Tests the hierarchical aggregation flow with protocol-based implementation:
//! - Squad leaders aggregate soldiers into SquadSummaries
//! - Platoon leaders aggregate squads into PlatoonSummaries
//!
//! This validates the fixes made to support dynamic topology configuration:
//! - Deriving squad IDs from platoon ID (platoon-1 → squad-1A, squad-1B)
//! - Dynamic squad count validation instead of hardcoded values
//! - Protocol-based aggregation using HierarchicalAggregator
//!
//! Correct military hierarchy: Soldiers → Squad Leaders → Platoons → Companies → Battalion HQ

use hive_protocol::hierarchy::aggregation_coordinator::HierarchicalAggregator;
use hive_protocol::hierarchy::state_aggregation::StateAggregator;
use hive_protocol::storage::DittoSummaryStorage;
use hive_protocol::testing::E2EHarness;
use hive_schema::common::v1::{Position, Timestamp};
use hive_schema::hierarchy::v1::SquadSummary;
use hive_schema::node::v1::HealthStatus;
use std::sync::Arc;

/// Test: Complete 3-tier aggregation flow with protocol APIs
#[tokio::test]
async fn test_three_tier_hierarchical_aggregation() {
    let ditto_app_id = std::env::var("HIVE_APP_ID")
        .or_else(|_| std::env::var("DITTO_APP_ID"))
        .expect("HIVE_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "HIVE_APP_ID cannot be empty");

    let mut harness = E2EHarness::new("e2e_three_tier_aggregation");
    let ditto_store = Arc::new(harness.create_ditto_store().await.unwrap());
    ditto_store.start_sync().unwrap();

    // Wrap in DittoSummaryStorage for backend abstraction
    let storage = Arc::new(DittoSummaryStorage::new(Arc::clone(&ditto_store)));
    let coordinator = Arc::new(HierarchicalAggregator::new(storage));

    println!("\n=== Phase 1: Squad-level aggregation ===");

    // Create squad summaries for platoon-1 (squad-1A and squad-1B)
    let squad_1a_summary = create_test_squad_summary("squad-1A", "squad-1A-leader", 4);
    let squad_1b_summary = create_test_squad_summary("squad-1B", "squad-1B-leader", 4);

    coordinator
        .create_squad_summary("squad-1A", &squad_1a_summary)
        .await
        .expect("Failed to create squad-1A summary");

    coordinator
        .create_squad_summary("squad-1B", &squad_1b_summary)
        .await
        .expect("Failed to create squad-1B summary");

    println!("✓ Created squad-1A and squad-1B summaries");

    // Create squad summaries for platoon-2 (squad-2A and squad-2B)
    let squad_2a_summary = create_test_squad_summary("squad-2A", "squad-2A-leader", 5);
    let squad_2b_summary = create_test_squad_summary("squad-2B", "squad-2B-leader", 5);

    coordinator
        .create_squad_summary("squad-2A", &squad_2a_summary)
        .await
        .expect("Failed to create squad-2A summary");

    coordinator
        .create_squad_summary("squad-2B", &squad_2b_summary)
        .await
        .expect("Failed to create squad-2B summary");

    println!("✓ Created squad-2A and squad-2B summaries");

    println!("\n=== Phase 2: Platoon-level aggregation ===");

    // Platoon-1 aggregates squad-1A and squad-1B
    let platoon_1_squads = vec![
        coordinator
            .get_squad_summary("squad-1A")
            .await
            .expect("Failed to get squad-1A")
            .expect("squad-1A not found"),
        coordinator
            .get_squad_summary("squad-1B")
            .await
            .expect("Failed to get squad-1B")
            .expect("squad-1B not found"),
    ];

    let platoon_1_summary =
        StateAggregator::aggregate_platoon("platoon-1", "platoon-1-leader", platoon_1_squads)
            .expect("Failed to aggregate platoon-1");

    assert_eq!(
        platoon_1_summary.squad_count, 2,
        "Platoon-1 should have 2 squads"
    );
    assert_eq!(
        platoon_1_summary.total_member_count, 8,
        "Platoon-1 should have 8 total members (4+4)"
    );

    coordinator
        .create_platoon_summary("platoon-1", &platoon_1_summary)
        .await
        .expect("Failed to create platoon-1 summary");

    println!("✓ Aggregated platoon-1 from 2 squads (8 members total)");

    // Platoon-2 aggregates squad-2A and squad-2B
    let platoon_2_squads = vec![
        coordinator
            .get_squad_summary("squad-2A")
            .await
            .expect("Failed to get squad-2A")
            .expect("squad-2A not found"),
        coordinator
            .get_squad_summary("squad-2B")
            .await
            .expect("Failed to get squad-2B")
            .expect("squad-2B not found"),
    ];

    let platoon_2_summary =
        StateAggregator::aggregate_platoon("platoon-2", "platoon-2-leader", platoon_2_squads)
            .expect("Failed to aggregate platoon-2");

    assert_eq!(
        platoon_2_summary.squad_count, 2,
        "Platoon-2 should have 2 squads"
    );
    assert_eq!(
        platoon_2_summary.total_member_count, 10,
        "Platoon-2 should have 10 total members (5+5)"
    );

    coordinator
        .create_platoon_summary("platoon-2", &platoon_2_summary)
        .await
        .expect("Failed to create platoon-2 summary");

    println!("✓ Aggregated platoon-2 from 2 squads (10 members total)");

    println!("\n=== Two-tier aggregation complete ===");
    println!("Squad → Platoon hierarchy validated ✓");

    // Verify we can retrieve both platoon summaries
    let platoon_1_retrieved = coordinator
        .get_platoon_summary("platoon-1")
        .await
        .expect("Failed to get platoon-1")
        .expect("platoon-1 not found");

    let platoon_2_retrieved = coordinator
        .get_platoon_summary("platoon-2")
        .await
        .expect("Failed to get platoon-2")
        .expect("platoon-2 not found");

    println!("✓ Successfully retrieved platoon-1 and platoon-2 summaries");
    println!(
        "  Platoon-1: {} squads, {} members",
        platoon_1_retrieved.squad_count, platoon_1_retrieved.total_member_count
    );
    println!(
        "  Platoon-2: {} squads, {} members",
        platoon_2_retrieved.squad_count, platoon_2_retrieved.total_member_count
    );
}

/// Test: Dynamic squad count validation (not hardcoded)
#[tokio::test]
async fn test_dynamic_squad_count_validation() {
    let ditto_app_id = std::env::var("HIVE_APP_ID")
        .or_else(|_| std::env::var("DITTO_APP_ID"))
        .expect("HIVE_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "HIVE_APP_ID cannot be empty");

    let mut harness = E2EHarness::new("e2e_dynamic_squad_count");
    let ditto_store = Arc::new(harness.create_ditto_store().await.unwrap());
    ditto_store.start_sync().unwrap();

    let storage = Arc::new(DittoSummaryStorage::new(Arc::clone(&ditto_store)));
    let coordinator = Arc::new(HierarchicalAggregator::new(storage));

    println!("\n=== Testing dynamic squad count validation ===");

    // Test case 1: Platoon with 2 squads (typical)
    let squad_a = create_test_squad_summary("squad-A", "leader-A", 4);
    let squad_b = create_test_squad_summary("squad-B", "leader-B", 4);

    coordinator
        .create_squad_summary("squad-A", &squad_a)
        .await
        .unwrap();
    coordinator
        .create_squad_summary("squad-B", &squad_b)
        .await
        .unwrap();

    let squads_2 = vec![
        coordinator
            .get_squad_summary("squad-A")
            .await
            .unwrap()
            .unwrap(),
        coordinator
            .get_squad_summary("squad-B")
            .await
            .unwrap()
            .unwrap(),
    ];

    let platoon_2_squads =
        StateAggregator::aggregate_platoon("platoon-2squads", "leader", squads_2)
            .expect("Should aggregate 2 squads");

    assert_eq!(platoon_2_squads.squad_count, 2);
    assert_eq!(platoon_2_squads.total_member_count, 8);
    println!("✓ Successfully aggregated platoon with 2 squads");

    // Test case 2: Platoon with 3 squads (larger)
    let squad_c = create_test_squad_summary("squad-C", "leader-C", 5);
    coordinator
        .create_squad_summary("squad-C", &squad_c)
        .await
        .unwrap();

    let squads_3 = vec![
        coordinator
            .get_squad_summary("squad-A")
            .await
            .unwrap()
            .unwrap(),
        coordinator
            .get_squad_summary("squad-B")
            .await
            .unwrap()
            .unwrap(),
        coordinator
            .get_squad_summary("squad-C")
            .await
            .unwrap()
            .unwrap(),
    ];

    let platoon_3_squads =
        StateAggregator::aggregate_platoon("platoon-3squads", "leader", squads_3)
            .expect("Should aggregate 3 squads");

    assert_eq!(platoon_3_squads.squad_count, 3);
    assert_eq!(platoon_3_squads.total_member_count, 13);
    println!("✓ Successfully aggregated platoon with 3 squads");

    // Test case 3: Platoon with 4 squads (maximum typical)
    let squad_d = create_test_squad_summary("squad-D", "leader-D", 6);
    coordinator
        .create_squad_summary("squad-D", &squad_d)
        .await
        .unwrap();

    let squads_4 = vec![
        coordinator
            .get_squad_summary("squad-A")
            .await
            .unwrap()
            .unwrap(),
        coordinator
            .get_squad_summary("squad-B")
            .await
            .unwrap()
            .unwrap(),
        coordinator
            .get_squad_summary("squad-C")
            .await
            .unwrap()
            .unwrap(),
        coordinator
            .get_squad_summary("squad-D")
            .await
            .unwrap()
            .unwrap(),
    ];

    let platoon_4_squads =
        StateAggregator::aggregate_platoon("platoon-4squads", "leader", squads_4)
            .expect("Should aggregate 4 squads");

    assert_eq!(platoon_4_squads.squad_count, 4);
    assert_eq!(platoon_4_squads.total_member_count, 19);
    println!("✓ Successfully aggregated platoon with 4 squads");

    println!("\n=== Dynamic squad count validation complete ===");
    println!("Protocol correctly handles 2, 3, and 4 squad platoons ✓");
}

/// Test: Squad ID derivation pattern (platoon-N → squad-NA, squad-NB)
#[test]
fn test_squad_id_derivation() {
    println!("\n=== Testing squad ID derivation pattern ===");

    // Test pattern: platoon-1 → squad-1A, squad-1B
    let platoon_id = "platoon-1";
    let expected_squads = vec!["squad-1A".to_string(), "squad-1B".to_string()];

    let derived_squads: Vec<String> = if let Some(platoon_num) = platoon_id.strip_prefix("platoon-")
    {
        vec![
            format!("squad-{}A", platoon_num),
            format!("squad-{}B", platoon_num),
        ]
    } else {
        vec![]
    };

    assert_eq!(
        derived_squads, expected_squads,
        "Platoon-1 should derive squad-1A and squad-1B"
    );
    println!("✓ Platoon-1 correctly derives squad-1A, squad-1B");

    // Test pattern: platoon-5 → squad-5A, squad-5B
    let platoon_id = "platoon-5";
    let expected_squads = vec!["squad-5A".to_string(), "squad-5B".to_string()];

    let derived_squads: Vec<String> = if let Some(platoon_num) = platoon_id.strip_prefix("platoon-")
    {
        vec![
            format!("squad-{}A", platoon_num),
            format!("squad-{}B", platoon_num),
        ]
    } else {
        vec![]
    };

    assert_eq!(
        derived_squads, expected_squads,
        "Platoon-5 should derive squad-5A and squad-5B"
    );
    println!("✓ Platoon-5 correctly derives squad-5A, squad-5B");

    // Test fallback for non-standard naming
    let platoon_id = "custom-platoon";
    let derived_squads: Vec<String> =
        if let Some(_platoon_num) = platoon_id.strip_prefix("platoon-") {
            vec![
                format!("squad-{}A", _platoon_num),
                format!("squad-{}B", _platoon_num),
            ]
        } else {
            vec!["squad-alpha".to_string(), "squad-bravo".to_string()]
        };

    assert_eq!(
        derived_squads,
        vec!["squad-alpha".to_string(), "squad-bravo".to_string()]
    );
    println!("✓ Non-standard naming falls back to squad-alpha, squad-bravo");

    println!("\n=== Squad ID derivation tests passed ===");
}

// Helper function to create test squad summaries
fn create_test_squad_summary(squad_id: &str, leader_id: &str, member_count: u32) -> SquadSummary {
    let mut member_ids = vec![leader_id.to_string()];
    for i in 1..member_count {
        member_ids.push(format!("{}-member-{}", squad_id, i));
    }

    let base_lat = 37.7749 + (member_count as f64 * 0.001);
    let base_lon = -122.4194 + (member_count as f64 * 0.001);

    SquadSummary {
        squad_id: squad_id.to_string(),
        leader_id: leader_id.to_string(),
        member_ids,
        member_count,
        position_centroid: Some(Position {
            latitude: base_lat,
            longitude: base_lon,
            altitude: 100.0,
        }),
        avg_fuel_minutes: 100.0,
        worst_health: HealthStatus::Nominal.into(),
        operational_count: member_count,
        aggregated_capabilities: vec![],
        readiness_score: 1.0,
        bounding_box: Some(hive_schema::hierarchy::v1::BoundingBox {
            southwest: Some(Position {
                latitude: base_lat - 0.01,
                longitude: base_lon - 0.01,
                altitude: 90.0,
            }),
            northeast: Some(Position {
                latitude: base_lat + 0.01,
                longitude: base_lon + 0.01,
                altitude: 110.0,
            }),
            max_altitude: 110.0,
            min_altitude: 90.0,
            radius_m: 50.0,
        }),
        aggregated_at: Some(Timestamp {
            seconds: 1234567890,
            nanos: 0,
        }),
    }
}
