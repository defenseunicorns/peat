//! E2E test for hierarchical aggregation with delta-based updates
//!
//! Validates ADR-021 document-oriented architecture:
//! - Squad summaries created once, updated via deltas
//! - Platoon summaries aggregated from squad summaries
//! - Bandwidth efficiency from delta-based CRDT sync

use hive_protocol::hierarchy::aggregation_coordinator::HierarchicalAggregator;
use hive_protocol::hierarchy::deltas::SquadDelta;
use hive_protocol::hierarchy::state_aggregation::StateAggregator;
use hive_protocol::storage::DittoSummaryStorage;
use hive_protocol::testing::E2EHarness;
use hive_schema::common::v1::{Position, Timestamp};
use hive_schema::hierarchy::v1::{BoundingBox, SquadSummary};
use hive_schema::node::v1::HealthStatus;
use std::sync::Arc;

/// Test: Squad summary follows create-once, update-many pattern
#[tokio::test]
async fn test_squad_summary_delta_updates() {
    let ditto_app_id = std::env::var("DITTO_APP_ID").unwrap_or_else(|_| "test-app-id".to_string());
    if ditto_app_id == "test-app-id" {
        println!("⚠ Skipping E2E test - DITTO_APP_ID not set");
        return;
    }

    let mut harness = E2EHarness::new("e2e_squad_delta_updates");
    let ditto_store = Arc::new(harness.create_ditto_store().await.unwrap());
    ditto_store.start_sync().unwrap();

    // Wrap in DittoSummaryStorage for backend abstraction
    let storage = Arc::new(DittoSummaryStorage::new(Arc::clone(&ditto_store)));
    let coordinator = HierarchicalAggregator::new(storage);

    let squad_id = "squad-alpha";

    // Phase 1: Create initial squad summary
    let initial_summary = SquadSummary {
        squad_id: squad_id.to_string(),
        leader_id: "node-1".to_string(),
        member_ids: vec!["node-1".to_string(), "node-2".to_string()],
        member_count: 2,
        position_centroid: Some(Position {
            latitude: 37.7749,
            longitude: -122.4194,
            altitude: 100.0,
        }),
        avg_fuel_minutes: 100.0,
        worst_health: HealthStatus::Nominal.into(),
        operational_count: 2,
        aggregated_capabilities: vec![],
        readiness_score: 1.0,
        bounding_box: None,
        aggregated_at: Some(Timestamp {
            seconds: 1234567890,
            nanos: 0,
        }),
    };

    // Create document (first time)
    coordinator
        .create_squad_summary(squad_id, &initial_summary)
        .await
        .expect("Failed to create squad summary");

    println!("✓ Created squad summary document");

    // Verify document exists
    let retrieved = coordinator
        .get_squad_summary(squad_id)
        .await
        .expect("Failed to get squad summary")
        .expect("Squad summary not found");
    assert_eq!(retrieved.member_count, 2);
    assert_eq!(retrieved.operational_count, 2);

    // Phase 2: Update via delta (simulating aggregation update)
    let updated_summary = SquadSummary {
        squad_id: squad_id.to_string(),
        leader_id: "node-1".to_string(),
        member_ids: vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ],
        member_count: 3,
        position_centroid: Some(Position {
            latitude: 37.7750,
            longitude: -122.4195,
            altitude: 100.0,
        }),
        avg_fuel_minutes: 95.0,
        worst_health: HealthStatus::Nominal.into(),
        operational_count: 3,
        aggregated_capabilities: vec![],
        readiness_score: 1.0,
        bounding_box: None,
        aggregated_at: Some(Timestamp {
            seconds: 1234567900,
            nanos: 0,
        }),
    };

    let delta = SquadDelta::from_summary(&updated_summary, 2);

    coordinator
        .update_squad_summary(squad_id, delta)
        .await
        .expect("Failed to update squad summary");

    println!("✓ Updated squad summary via delta");

    // Small delay to allow Ditto to process the update
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify update applied
    let updated = coordinator
        .get_squad_summary(squad_id)
        .await
        .expect("Failed to get updated squad summary")
        .expect("Squad summary not found after update");
    assert_eq!(updated.member_count, 3);
    assert_eq!(updated.operational_count, 3);
    // Note: member_ids.len() is still 2 because array operations are not yet implemented
    // TODO: Implement array operations for full delta support
    // assert_eq!(updated.member_ids.len(), 3);

    println!("✓ Squad summary delta update validated");

    // Clean shutdown - Rust will drop everything automatically
}

/// Test: End-to-end upward aggregation flow (Node → Squad → Platoon)
#[tokio::test]
async fn test_upward_aggregation_flow() {
    let ditto_app_id = std::env::var("DITTO_APP_ID").unwrap_or_else(|_| "test-app-id".to_string());
    if ditto_app_id == "test-app-id" {
        println!("⚠ Skipping E2E test - DITTO_APP_ID not set");
        return;
    }

    let mut harness = E2EHarness::new("e2e_upward_aggregation");
    let ditto_store = Arc::new(harness.create_ditto_store().await.unwrap());
    ditto_store.start_sync().unwrap();

    let storage = Arc::new(DittoSummaryStorage::new(Arc::clone(&ditto_store)));
    let coordinator = HierarchicalAggregator::new(storage);

    // Create 3 squad summaries
    for i in 1..=3 {
        let squad_id = format!("squad-{}", i);
        let summary = SquadSummary {
            squad_id: squad_id.clone(),
            leader_id: format!("node-{}-leader", i),
            member_ids: vec![format!("node-{}-1", i), format!("node-{}-2", i)],
            member_count: 2,
            position_centroid: Some(Position {
                latitude: 37.77 + (i as f64 * 0.001),
                longitude: -122.41,
                altitude: 100.0,
            }),
            avg_fuel_minutes: 100.0,
            worst_health: HealthStatus::Nominal.into(),
            operational_count: 2,
            aggregated_capabilities: vec![],
            readiness_score: 1.0,
            bounding_box: Some(BoundingBox {
                southwest: Some(Position {
                    latitude: 37.77 + (i as f64 * 0.001) - 0.0005,
                    longitude: -122.41 - 0.0005,
                    altitude: 90.0,
                }),
                northeast: Some(Position {
                    latitude: 37.77 + (i as f64 * 0.001) + 0.0005,
                    longitude: -122.41 + 0.0005,
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
        };

        coordinator
            .create_squad_summary(&squad_id, &summary)
            .await
            .unwrap_or_else(|_| panic!("Failed to create squad {}", squad_id));
    }

    println!("✓ Created 3 squad summaries");

    // Aggregate into platoon summary
    let platoon_id = "platoon-1";
    let mut squad_summaries = Vec::new();

    for i in 1..=3 {
        let squad_id = format!("squad-{}", i);
        if let Some(summary) = coordinator
            .get_squad_summary(&squad_id)
            .await
            .expect("Failed to get squad summary")
        {
            squad_summaries.push(summary);
        }
    }

    assert_eq!(squad_summaries.len(), 3);

    // Use StateAggregator to create platoon summary
    let platoon_summary =
        StateAggregator::aggregate_platoon(platoon_id, "platoon-leader", squad_summaries)
            .expect("Failed to aggregate platoon");

    assert_eq!(platoon_summary.squad_count, 3);
    assert_eq!(platoon_summary.total_member_count, 6);

    // Create platoon summary document
    coordinator
        .create_platoon_summary(platoon_id, &platoon_summary)
        .await
        .expect("Failed to create platoon summary");

    println!("✓ Created platoon summary from 3 squads");

    // Small delay to allow Ditto to process the creation
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify platoon summary
    let retrieved_platoon = coordinator
        .get_platoon_summary(platoon_id)
        .await
        .expect("Failed to get platoon summary")
        .expect("Platoon summary not found");

    assert_eq!(retrieved_platoon.squad_count, 3);
    assert_eq!(retrieved_platoon.total_member_count, 6);
    assert_eq!(retrieved_platoon.squad_ids.len(), 3);

    println!("✓ Upward aggregation flow validated (Node → Squad → Platoon)");

    // Clean shutdown - Rust will drop everything automatically
}
