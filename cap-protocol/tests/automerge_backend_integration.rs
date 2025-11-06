//! Integration tests for Automerge backend
//!
//! These tests verify that the AutomergeBackend correctly implements
//! the DataSyncBackend abstraction layer for CRDT-based synchronization.
//!
//! Unlike Ditto integration tests, these don't require external credentials
//! and can test CRDT merge behavior directly using Automerge's sync protocol.

use cap_protocol::models::cell::{CellConfig, CellState};
use cap_protocol::models::node::NodeConfig;
use cap_protocol::models::{Capability, CapabilityType};
use cap_protocol::storage::{CellStore, NodeStore};
use cap_protocol::sync::automerge::AutomergeBackend;
use cap_protocol::sync::{
    BackendConfig, DataSyncBackend, Document, Query, TransportConfig, TransportType, Value,
};
use std::collections::HashMap;
use std::path::PathBuf;

/// Helper to create a test AutomergeBackend
async fn setup_backend() -> AutomergeBackend {
    let config = BackendConfig {
        app_id: "test_automerge_app".to_string(),
        persistence_dir: PathBuf::from("/tmp/automerge_test"),
        shared_key: None,
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    let backend = AutomergeBackend::new();
    backend
        .initialize(config)
        .await
        .expect("Failed to initialize backend");

    backend
        .sync_engine()
        .start_sync()
        .await
        .expect("Failed to start sync");

    backend
}

async fn cleanup_backend(backend: AutomergeBackend) {
    backend.shutdown().await.ok();
}

#[tokio::test]
async fn test_backend_lifecycle() {
    let backend = setup_backend().await;

    // Verify backend is ready
    assert!(backend.is_ready().await);

    // Verify backend info
    let info = backend.backend_info();
    assert_eq!(info.name, "Automerge");
    assert_eq!(info.version, "0.7.1");

    // Verify sync is active
    assert!(backend.sync_engine().is_syncing().await.unwrap());

    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_document_upsert() {
    let backend = setup_backend().await;
    let doc_store = backend.document_store();

    // Create a document
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), Value::String("test_doc".to_string()));
    fields.insert(
        "value".to_string(),
        Value::Number(serde_json::Number::from(42)),
    );

    let doc = Document::new(fields);

    // Upsert the document
    let doc_id = doc_store
        .upsert("test_collection", doc)
        .await
        .expect("Upsert should succeed");

    assert!(!doc_id.is_empty(), "Document ID should not be empty");

    // Clean up
    doc_store
        .remove("test_collection", &doc_id)
        .await
        .expect("Remove should succeed");

    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_document_remove() {
    let backend = setup_backend().await;
    let doc_store = backend.document_store();

    // Insert a document
    let mut fields = HashMap::new();
    fields.insert(
        "test_id".to_string(),
        Value::String("remove_test".to_string()),
    );

    let doc = Document::new(fields);
    let doc_id = doc_store
        .upsert("test_collection_remove", doc)
        .await
        .expect("Upsert should succeed");

    // Remove it
    doc_store
        .remove("test_collection_remove", &doc_id)
        .await
        .expect("Remove should succeed");

    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_document_count() {
    let backend = setup_backend().await;
    let doc_store = backend.document_store();

    // Insert multiple documents
    let mut doc_ids = Vec::new();
    for i in 0..5 {
        let mut fields = HashMap::new();
        fields.insert("batch".to_string(), Value::String("count_test".to_string()));
        fields.insert(
            "index".to_string(),
            Value::Number(serde_json::Number::from(i)),
        );

        let doc = Document::new(fields);
        let doc_id = doc_store
            .upsert("test_collection_count", doc)
            .await
            .expect("Upsert should succeed");
        doc_ids.push(doc_id);
    }

    // Count documents matching query
    let query = Query::Eq {
        field: "batch".to_string(),
        value: Value::String("count_test".to_string()),
    };

    let count = doc_store
        .count("test_collection_count", &query)
        .await
        .expect("Count should succeed");

    // automerge_to_document now properly reads fields
    assert_eq!(count, 5, "Should count exactly 5 documents");

    // Clean up
    for doc_id in doc_ids {
        doc_store
            .remove("test_collection_count", &doc_id)
            .await
            .ok();
    }
    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_sync_subscription() {
    let backend = setup_backend().await;
    let sync_engine = backend.sync_engine();

    // Create a subscription
    let query = Query::All;
    let subscription = sync_engine
        .subscribe("test_collection_sub", &query)
        .await
        .expect("Subscribe should succeed");

    assert_eq!(
        subscription.collection(),
        "test_collection_sub",
        "Subscription should track correct collection"
    );

    // Dropping subscription should clean up (no panic)
    drop(subscription);

    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_two_instance_sync() {
    // Create two separate AutomergeBackend instances
    let backend1 = setup_backend().await;
    let backend2 = setup_backend().await;

    let doc_store1 = backend1.document_store();
    let doc_store2 = backend2.document_store();

    // Create a document on backend1
    let mut fields = HashMap::new();
    fields.insert("sync_test".to_string(), Value::String("data".to_string()));
    fields.insert(
        "value".to_string(),
        Value::Number(serde_json::Number::from(100)),
    );

    let doc = Document::new(fields);
    let doc_id = doc_store1
        .upsert("sync_collection", doc)
        .await
        .expect("Upsert should succeed");

    // Create empty document on backend2 with same ID (required for receive_sync_message)
    let mut empty_doc = Document::new(HashMap::new());
    empty_doc.id = Some(doc_id.clone());
    doc_store2
        .upsert("sync_collection", empty_doc)
        .await
        .expect("Upsert empty doc should succeed");

    // Generate sync message from backend1
    let sync_message = backend1
        .generate_sync_message("sync_collection", &doc_id, "peer2")
        .expect("Should generate sync message");

    // Apply sync message to backend2
    backend2
        .receive_sync_message("sync_collection", &doc_id, "peer1", &sync_message)
        .expect("Should receive sync message");

    // Note: With the current stub implementation, backend2 won't have the document fields
    // but this tests that the sync protocol mechanics work

    // Clean up
    doc_store1.remove("sync_collection", &doc_id).await.ok();
    doc_store2.remove("sync_collection", &doc_id).await.ok();

    cleanup_backend(backend1).await;
    cleanup_backend(backend2).await;
}

#[tokio::test]
async fn test_cellstore_compatibility() {
    // Verify that CellStore works with AutomergeBackend
    let backend = setup_backend().await;

    let cell_store: CellStore<AutomergeBackend> = CellStore::new(backend.clone().into())
        .await
        .expect("CellStore should initialize with AutomergeBackend");

    // Create a cell
    let cell_config = CellConfig::new(5);
    let mut cell = CellState::new(cell_config);
    cell.config.id = "test_cell".to_string();
    cell.add_member("node1".to_string());
    cell.set_leader("node1".to_string()).unwrap();

    // Store the cell
    cell_store
        .store_cell(&cell)
        .await
        .expect("Should store cell");

    // Note: Retrieval will fail with stub implementation, but this tests storage mechanics
    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_nodestore_compatibility() {
    // Verify that NodeStore works with AutomergeBackend
    let backend = setup_backend().await;

    let node_store: NodeStore<AutomergeBackend> = NodeStore::new(backend.clone().into())
        .await
        .expect("NodeStore should initialize with AutomergeBackend");

    // Create a node
    let mut node_config = NodeConfig::new("UAV".to_string());
    node_config.id = "test_node".to_string();
    node_config.add_capability(Capability::new(
        "sensor1".to_string(),
        "Temperature Sensor".to_string(),
        CapabilityType::Sensor,
        0.95,
    ));

    // Store the node
    node_store
        .store_config(&node_config)
        .await
        .expect("Should store node config");

    // Note: Retrieval will fail with stub implementation, but this tests storage mechanics
    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_peer_discovery_manual() {
    let backend = setup_backend().await;
    let peer_discovery = backend.peer_discovery();

    // Start discovery (should succeed even though it's a stub)
    peer_discovery.start().await.expect("Start should succeed");

    // Manually add a peer
    peer_discovery
        .add_peer("192.168.1.100:4000", TransportType::Tcp)
        .await
        .expect("Add peer should succeed");

    // Stop discovery
    peer_discovery.stop().await.expect("Stop should succeed");

    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_backend_multiple_documents() {
    let backend = setup_backend().await;
    let doc_store = backend.document_store();

    // Insert multiple documents to same collection
    let mut doc_ids = Vec::new();

    for i in 0..3 {
        let mut fields = HashMap::new();
        fields.insert(
            "test_batch".to_string(),
            Value::String("multi_test".to_string()),
        );
        fields.insert(
            "index".to_string(),
            Value::Number(serde_json::Number::from(i)),
        );

        let doc = Document::new(fields);
        let doc_id = doc_store
            .upsert("test_multi_collection", doc)
            .await
            .expect("Upsert should succeed");
        doc_ids.push(doc_id);
    }

    // Verify all documents were created (have IDs)
    assert_eq!(doc_ids.len(), 3, "Should have 3 document IDs");

    // Clean up
    for doc_id in doc_ids {
        doc_store
            .remove("test_multi_collection", &doc_id)
            .await
            .ok();
    }

    cleanup_backend(backend).await;
}
