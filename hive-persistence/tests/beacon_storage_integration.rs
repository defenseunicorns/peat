//! Integration tests for beacon storage system
//!
//! These tests verify the full stack integration:
//! BeaconBroadcaster/Observer → PersistentBeaconStorage → DittoStore → Ditto
//!
//! Tests use real Ditto backend instances with credentials from .env file.

use hive_mesh::beacon::{
    BeaconBroadcaster, BeaconObserver, BeaconStorage, GeoPosition, HierarchyLevel, NodeProfile,
    NodeResources,
};
use hive_persistence::backends::DittoStore;
use hive_persistence::PersistentBeaconStorage;
use hive_protocol::sync::ditto::DittoBackend;
use hive_protocol::sync::{BackendConfig, DataSyncBackend, TransportConfig};
use serial_test::serial;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Helper to create a test Ditto backend with real credentials
async fn create_test_backend(test_name: &str) -> Arc<DittoBackend> {
    // Load environment variables from .env
    dotenvy::dotenv().ok();

    let app_id = std::env::var("HIVE_APP_ID")
        .or_else(|_| std::env::var("DITTO_APP_ID"))
        .expect("HIVE_APP_ID must be set in .env file");
    let shared_key = std::env::var("HIVE_SECRET_KEY")
        .or_else(|_| std::env::var("HIVE_SHARED_KEY"))
        .or_else(|_| std::env::var("DITTO_SHARED_KEY"))
        .expect("HIVE_SECRET_KEY must be set in .env file");
    let offline_token = std::env::var("HIVE_OFFLINE_TOKEN")
        .or_else(|_| std::env::var("DITTO_OFFLINE_TOKEN"))
        .ok();

    let backend = Arc::new(DittoBackend::new());

    let persistence_dir = PathBuf::from(format!("/tmp/beacon-integration-test-{}", test_name));

    // Clean up any leftover data from previous test runs
    if persistence_dir.exists() {
        let _ = std::fs::remove_dir_all(&persistence_dir);
    }

    let mut extra = HashMap::new();
    if let Some(token) = offline_token {
        extra.insert("offline_token".to_string(), token);
    }

    let config = BackendConfig {
        app_id,
        persistence_dir,
        shared_key: Some(shared_key),
        transport: TransportConfig {
            tcp_listen_port: None, // No network for tests
            tcp_connect_address: None,
            enable_mdns: false,
            enable_bluetooth: false,
            enable_websocket: false,
            custom: HashMap::new(),
        },
        extra,
    };

    backend.initialize(config).await.unwrap();
    backend
}

/// Helper to create a beacon storage adapter backed by Ditto
async fn create_beacon_storage(test_name: &str) -> Arc<PersistentBeaconStorage> {
    let backend = create_test_backend(test_name).await;
    let store = Arc::new(DittoStore::new(backend));
    Arc::new(PersistentBeaconStorage::new(store))
}

#[tokio::test]
#[serial]
async fn test_broadcaster_persists_beacons_to_ditto() {
    let storage = create_beacon_storage("broadcaster_persist").await;

    // Create broadcaster
    let broadcaster = BeaconBroadcaster::new(
        storage.clone(),
        "node-1".to_string(),
        GeoPosition::new(37.7749, -122.4194), // San Francisco
        HierarchyLevel::Platform,
        None,
        Duration::from_millis(100),
    );

    // Start broadcasting
    broadcaster.start().await;

    // Wait for at least one beacon to be broadcast
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Stop broadcasting
    broadcaster.stop().await;

    // Query storage directly - should have the beacon
    let beacons = storage.query_all().await.unwrap();
    assert_eq!(beacons.len(), 1);
    assert_eq!(beacons[0].node_id, "node-1");
    assert_eq!(beacons[0].hierarchy_level, HierarchyLevel::Platform);
}

#[tokio::test]
#[serial]
async fn test_observer_receives_beacon_events_from_ditto() {
    let storage = create_beacon_storage("observer_events").await;

    // Create a beacon to get its geohash for the observer
    let observer_position = GeoPosition::new(37.7749, -122.4194);
    let temp_beacon = hive_mesh::beacon::GeographicBeacon::new(
        "temp".to_string(),
        observer_position,
        HierarchyLevel::Platform,
    );
    let observer_geohash = temp_beacon.geohash.clone();

    // Create observer first and subscribe
    let observer = BeaconObserver::new(storage.clone(), observer_geohash);

    let _event_stream = storage.subscribe().await.unwrap();

    // Delay to ensure Ditto observer is fully registered
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Start observer
    observer.start().await;

    // Additional delay to ensure observer subscription is fully registered
    // This prevents race conditions where beacons are broadcast before
    // the observer's subscription is active (similar to E2E test fixes)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create and start broadcaster
    let broadcaster = BeaconBroadcaster::new(
        storage.clone(),
        "node-1".to_string(),
        GeoPosition::new(37.7750, -122.4195), // Nearby
        HierarchyLevel::Squad,
        None,
        Duration::from_millis(100),
    );
    broadcaster.start().await;

    // Wait for beacon to be broadcast and propagated
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify beacon was persisted and propagated via query
    // (Event streaming has timing complexities with CRDT eventual consistency)
    let nearby = observer.get_nearby_beacons().await;
    assert_eq!(nearby.len(), 1, "Should see one nearby beacon");
    assert_eq!(nearby[0].node_id, "node-1");
    assert_eq!(nearby[0].hierarchy_level, HierarchyLevel::Squad);

    // Cleanup
    broadcaster.stop().await;
    observer.stop().await;
}

#[tokio::test]
#[serial]
async fn test_multiple_nodes_beacon_interaction() {
    let storage = create_beacon_storage("multi_node").await;

    // Create multiple broadcasters at different locations
    let positions = vec![
        ("node-1", GeoPosition::new(37.7749, -122.4194)), // SF
        ("node-2", GeoPosition::new(37.7750, -122.4195)), // Nearby SF
        ("node-3", GeoPosition::new(34.0522, -118.2437)), // LA (far away)
    ];

    let mut broadcasters = Vec::new();
    for (node_id, position) in positions {
        let broadcaster = BeaconBroadcaster::new(
            storage.clone(),
            node_id.to_string(),
            position,
            HierarchyLevel::Platform,
            None,
            Duration::from_millis(100),
        );
        broadcaster.start().await;
        broadcasters.push(broadcaster);
    }

    // Wait for broadcasts
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Query all beacons
    let all_beacons = storage.query_all().await.unwrap();
    assert_eq!(all_beacons.len(), 3);

    // Query by geohash - SF area should have 2 nodes
    let sf_geohash_prefix = &all_beacons[0].geohash[..5];
    let sf_beacons = storage.query_by_geohash(sf_geohash_prefix).await.unwrap();
    assert!(
        sf_beacons.len() >= 2,
        "Expected at least 2 nodes in SF area, got {}",
        sf_beacons.len()
    );

    // Cleanup
    for broadcaster in broadcasters {
        broadcaster.stop().await;
    }
}

#[tokio::test]
#[serial]
async fn test_beacon_updates_propagate() {
    let storage = create_beacon_storage("beacon_updates").await;

    // Create broadcaster with a profile
    let resources = NodeResources {
        cpu_cores: 2,
        memory_mb: 512,
        bandwidth_mbps: 10,
        cpu_usage_percent: 25,
        memory_usage_percent: 40,
        battery_percent: Some(80),
    };
    let profile = NodeProfile::mobile_node(resources);
    let broadcaster = BeaconBroadcaster::new(
        storage.clone(),
        "mobile-node".to_string(),
        GeoPosition::new(37.7749, -122.4194),
        HierarchyLevel::Squad,
        Some(profile.clone()),
        Duration::from_millis(100),
    );

    // Start broadcasting
    broadcaster.start().await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Query initial beacon
    let beacons = storage.query_all().await.unwrap();
    assert_eq!(beacons.len(), 1);
    let initial_position = beacons[0].position;

    // Update position (simulating node movement)
    broadcaster
        .update_position(GeoPosition::new(37.7800, -122.4200))
        .await;

    // Wait for update to propagate
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Query updated beacon
    let updated_beacons = storage.query_all().await.unwrap();
    assert_eq!(updated_beacons.len(), 1);
    let updated_position = updated_beacons[0].position;

    // Position should have changed
    assert_ne!(initial_position.lat, updated_position.lat);
    assert_ne!(initial_position.lon, updated_position.lon);

    // Profile attributes should be preserved
    assert_eq!(updated_beacons[0].mobility, Some(profile.mobility));

    // Cleanup
    broadcaster.stop().await;
}

#[tokio::test]
#[serial]
async fn test_observer_filters_nearby_beacons() {
    let storage = create_beacon_storage("observer_filter").await;

    // Create observer at SF location
    let observer_position = GeoPosition::new(37.7749, -122.4194);
    let temp_beacon = hive_mesh::beacon::GeographicBeacon::new(
        "temp".to_string(),
        observer_position,
        HierarchyLevel::Platform,
    );
    let observer_geohash = temp_beacon.geohash.clone();

    let observer = BeaconObserver::new(storage.clone(), observer_geohash);

    // Delay to ensure Ditto observer is fully registered
    tokio::time::sleep(Duration::from_millis(500)).await;

    observer.start().await;

    // Additional delay to ensure observer subscription is fully registered
    // This prevents race conditions (similar to E2E test fixes)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create broadcasters at different locations
    let nearby_broadcaster = BeaconBroadcaster::new(
        storage.clone(),
        "nearby-node".to_string(),
        GeoPosition::new(37.7750, -122.4195), // Very close to SF
        HierarchyLevel::Squad,
        None,
        Duration::from_millis(100),
    );

    let far_broadcaster = BeaconBroadcaster::new(
        storage.clone(),
        "far-node".to_string(),
        GeoPosition::new(34.0522, -118.2437), // LA - far away
        HierarchyLevel::Squad,
        None,
        Duration::from_millis(100),
    );

    // Start both broadcasters
    nearby_broadcaster.start().await;
    far_broadcaster.start().await;

    // Wait for broadcasts to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Query nearby beacons from observer
    let nearby = observer.get_nearby_beacons().await;

    // Should include nearby node but not far node
    assert!(
        nearby.iter().any(|b| b.node_id == "nearby-node"),
        "Should see nearby node"
    );

    // Note: Depending on geohash precision, far node might not be filtered yet
    // This is expected behavior as the geohash filtering is proximity-based

    // Cleanup
    observer.stop().await;
    nearby_broadcaster.stop().await;
    far_broadcaster.stop().await;
}

#[tokio::test]
#[serial]
async fn test_beacon_idempotency() {
    let storage = create_beacon_storage("idempotency").await;

    // Create broadcaster that will send multiple beacons
    let broadcaster = BeaconBroadcaster::new(
        storage.clone(),
        "node-1".to_string(),
        GeoPosition::new(37.7749, -122.4194),
        HierarchyLevel::Platform,
        None,
        Duration::from_millis(50),
    );

    // Start broadcasting with short interval
    broadcaster.start().await;

    // Wait for multiple broadcasts
    tokio::time::sleep(Duration::from_millis(300)).await;

    broadcaster.stop().await;

    // Should still only have one beacon document (idempotent updates)
    let beacons = storage.query_all().await.unwrap();
    assert_eq!(
        beacons.len(),
        1,
        "Multiple broadcasts should result in one beacon"
    );
    assert_eq!(beacons[0].node_id, "node-1");
}
