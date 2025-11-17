//! Integration tests for DittoStore implementation
//!
//! These tests verify the DittoStore adapter works correctly with a real
//! Ditto backend instance using credentials from .env file.

use hive_persistence::backends::DittoStore;
use hive_persistence::{DataStore, Query};
use hive_protocol::sync::ditto::DittoBackend;
use hive_protocol::sync::{BackendConfig, DataSyncBackend, TransportConfig};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Helper to create a test Ditto backend with real credentials
async fn create_test_backend(test_name: &str) -> Arc<DittoBackend> {
    // Load environment variables from .env
    dotenvy::dotenv().ok();

    let app_id = std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set in .env file");
    let shared_key =
        std::env::var("DITTO_SHARED_KEY").expect("DITTO_SHARED_KEY must be set in .env file");

    let backend = Arc::new(DittoBackend::new());

    let persistence_dir = PathBuf::from(format!("/tmp/cap-persistence-test-{}", test_name));

    // Clean up any leftover data from previous test runs
    if persistence_dir.exists() {
        let _ = std::fs::remove_dir_all(&persistence_dir);
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
        extra: HashMap::new(),
    };

    backend.initialize(config).await.unwrap();
    backend
}

#[tokio::test]
async fn test_save_and_query() {
    let backend = create_test_backend("save_query").await;
    let store = DittoStore::new(backend);

    // Save a document
    let doc = json!({
        "node_id": "test-node-1",
        "phase": "discovery",
        "health": "nominal"
    });

    let id = store.save("test_nodes", &doc).await.unwrap();
    assert!(!id.as_str().is_empty());

    // Query all documents
    let results = store.query("test_nodes", Query::all()).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["node_id"], "test-node-1");
    assert_eq!(results[0]["phase"], "discovery");
}

#[tokio::test]
async fn test_find_by_id() {
    let backend = create_test_backend("find_by_id").await;
    let store = DittoStore::new(backend);

    // Save a document
    let doc = json!({
        "cell_id": "cell-alpha",
        "leader": "node-1",
        "members": ["node-1", "node-2"]
    });

    let id = store.save("test_cells", &doc).await.unwrap();

    // Find by ID
    let found = store.find_by_id("test_cells", &id).await.unwrap();
    assert_eq!(found["cell_id"], "cell-alpha");
    assert_eq!(found["leader"], "node-1");
}

#[tokio::test]
async fn test_multiple_documents() {
    let backend = create_test_backend("multiple_docs").await;
    let store = DittoStore::new(backend);

    // Save multiple documents
    for i in 1..=5 {
        let doc = json!({
            "beacon_id": format!("beacon-{}", i),
            "geohash": "9q8yy",
            "operational": i % 2 == 0
        });
        store.save("test_beacons", &doc).await.unwrap();
    }

    // Query all
    let results = store.query("test_beacons", Query::all()).await.unwrap();
    assert_eq!(results.len(), 5);

    // Verify all documents are present
    let beacon_ids: Vec<String> = results
        .iter()
        .map(|doc| doc["beacon_id"].as_str().unwrap().to_string())
        .collect();

    for i in 1..=5 {
        let expected_id = format!("beacon-{}", i);
        assert!(
            beacon_ids.contains(&expected_id),
            "Missing beacon: {}",
            expected_id
        );
    }
}

#[tokio::test]
async fn test_store_info() {
    let backend = create_test_backend("store_info").await;
    let store = DittoStore::new(backend);

    let info = store.store_info();
    assert!(!info.name.is_empty());
    assert!(!info.version.is_empty());
    assert_eq!(info.properties.get("backend_type").unwrap(), "Ditto");
}

#[tokio::test]
async fn test_save_non_object_value() {
    let backend = create_test_backend("non_object").await;
    let store = DittoStore::new(backend);

    // Save a non-object value (should be wrapped)
    let doc = json!("simple string value");
    let id = store.save("test_values", &doc).await.unwrap();

    // Query and verify it was wrapped
    let found = store.find_by_id("test_values", &id).await.unwrap();
    assert_eq!(found["value"], "simple string value");
}
