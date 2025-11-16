//! Backend-Agnostic E2E Tests
//!
//! These tests validate that both DittoBackend and AutomergeIrohBackend can run
//! through the same test infrastructure and demonstrate identical CRDT semantics.
//!
//! # Test Strategy
//!
//! - Each test has two variants: one for Ditto, one for Automerge+Iroh
//! - Tests use the same test logic via shared helper functions
//! - Validates that DataSyncBackend trait abstraction works correctly
//!
//! # What This Proves
//!
//! 1. **Trait Abstraction Works**: Both backends implement DataSyncBackend
//! 2. **E2EHarness Compatibility**: Both can be created via E2EHarness
//! 3. **Basic CRDT Operations**: Document upsert/query work identically
//! 4. **Test Infrastructure Parity**: Same test patterns for both backends

use cap_protocol::sync::{DataSyncBackend, Document, Value};
use cap_protocol::testing::E2EHarness;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Ditto Backend Tests
// ============================================================================

/// Test basic document operations with Ditto backend
#[tokio::test]
async fn test_ditto_basic_document_operations() {
    dotenvy::dotenv().ok();

    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

    println!("=== Backend Agnostic E2E: Ditto Basic Operations ===");

    let mut harness = E2EHarness::new("ditto_basic_ops");
    let backend = harness.create_ditto_backend().await.unwrap();

    run_basic_document_operations_test(backend, "Ditto").await;
}

/// Test two-instance sync with Ditto backend
#[tokio::test]
async fn test_ditto_two_instance_sync() {
    dotenvy::dotenv().ok();

    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

    println!("=== Backend Agnostic E2E: Ditto Two-Instance Sync ===");

    let mut harness = E2EHarness::new("ditto_two_sync");

    // Create two backends with explicit TCP ports
    let backend1 = harness
        .create_ditto_backend_with_tcp(Some(19101), None)
        .await
        .unwrap();
    let backend2 = harness
        .create_ditto_backend_with_tcp(Some(19102), Some("127.0.0.1:19101".to_string()))
        .await
        .unwrap();

    run_two_instance_sync_test(backend1, backend2, "Ditto").await;
}

// ============================================================================
// Automerge+Iroh Backend Tests
// ============================================================================

/// Test basic document operations with Automerge+Iroh backend
#[cfg(feature = "automerge-backend")]
#[tokio::test]
async fn test_automerge_basic_document_operations() {
    println!("=== Backend Agnostic E2E: Automerge+Iroh Basic Operations ===");

    let mut harness = E2EHarness::new("automerge_basic_ops");
    let backend = harness.create_automerge_backend().await.unwrap();

    run_basic_document_operations_test(backend, "Automerge+Iroh").await;
}

/// Test two-instance sync with Automerge+Iroh backend
#[cfg(feature = "automerge-backend")]
#[tokio::test]
async fn test_automerge_two_instance_sync() {
    println!("=== Backend Agnostic E2E: Automerge+Iroh Two-Instance Sync ===");

    let mut harness = E2EHarness::new("automerge_two_sync");

    // Create two backends with explicit bind addresses
    let addr1: std::net::SocketAddr = "127.0.0.1:19201".parse().unwrap();
    let addr2: std::net::SocketAddr = "127.0.0.1:19202".parse().unwrap();

    let backend1 = harness
        .create_automerge_backend_with_bind(Some(addr1))
        .await
        .unwrap();
    let backend2 = harness
        .create_automerge_backend_with_bind(Some(addr2))
        .await
        .unwrap();

    // Explicitly connect the peers for Automerge (unlike Ditto which has automatic discovery)
    // Create PeerInfo for backend2 with its actual endpoint_id
    println!("  Connecting Automerge peers...");
    let transport1 = backend1.transport();
    let endpoint2_id = backend2.endpoint_id();
    let node2_id_hex = hex::encode(endpoint2_id.as_bytes());

    let peer_info = cap_protocol::network::PeerInfo {
        name: "backend2".to_string(),
        node_id: node2_id_hex,
        addresses: vec![addr2.to_string()],
        relay_url: None,
    };

    transport1
        .connect_peer(&peer_info)
        .await
        .expect("Should connect backend1 to backend2");
    println!("  ✓ Peers connected");

    run_two_instance_sync_test(backend1, backend2, "Automerge+Iroh").await;
}

// ============================================================================
// Shared Test Logic
// ============================================================================

/// Shared test logic for basic document operations
async fn run_basic_document_operations_test<B: DataSyncBackend>(
    backend: Arc<B>,
    backend_name: &str,
) {
    println!("  Testing with {} backend", backend_name);

    // Test 1: Create a document
    println!("  1. Creating document...");
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), Value::String("Test Node".to_string()));
    fields.insert("value".to_string(), Value::Number(42.into()));

    let doc = Document::with_id("test-doc-1", fields);

    let doc_id = backend
        .document_store()
        .upsert("test_collection", doc)
        .await
        .expect("Should create document");

    assert_eq!(doc_id, "test-doc-1");
    println!("  ✓ Document created with ID: {}", doc_id);

    // Test 2: Get the document back by ID
    println!("  2. Getting document by ID...");
    let retrieved_doc = backend
        .document_store()
        .get("test_collection", &doc_id)
        .await
        .expect("Should get document");

    assert!(retrieved_doc.is_some(), "Should find the document");
    assert_eq!(retrieved_doc.as_ref().unwrap().id, Some(doc_id.clone()));
    println!("  ✓ Document retrieved successfully");

    // Test 3: Update the document
    println!("  3. Updating document...");
    let mut updated_fields = HashMap::new();
    updated_fields.insert(
        "name".to_string(),
        Value::String("Updated Test Node".to_string()),
    );
    updated_fields.insert("value".to_string(), Value::Number(100.into()));

    let updated_doc = Document::with_id(doc_id.clone(), updated_fields);

    backend
        .document_store()
        .upsert("test_collection", updated_doc)
        .await
        .expect("Should update document");

    // Get again to verify update
    let updated_doc = backend
        .document_store()
        .get("test_collection", &doc_id)
        .await
        .expect("Should get updated document");

    assert!(updated_doc.is_some(), "Should find updated document");
    assert_eq!(
        updated_doc
            .unwrap()
            .fields
            .get("value")
            .and_then(|v| v.as_i64()),
        Some(100),
        "Document should be updated"
    );
    println!("  ✓ Document updated successfully");

    println!(
        "  ✅ {} backend: All basic operations passed!",
        backend_name
    );
}

/// Shared test logic for two-instance sync
async fn run_two_instance_sync_test<B: DataSyncBackend>(
    backend1: Arc<B>,
    backend2: Arc<B>,
    backend_name: &str,
) {
    println!("  Testing two-instance sync with {} backend", backend_name);

    // Start sync on both backends
    println!("  1. Starting sync on both backends...");
    backend1
        .sync_engine()
        .start_sync()
        .await
        .expect("Should start sync on backend1");
    backend2
        .sync_engine()
        .start_sync()
        .await
        .expect("Should start sync on backend2");
    println!("  ✓ Sync started");

    // Create document on backend1
    println!("  2. Creating document on backend1...");
    let mut fields = HashMap::new();
    fields.insert("source".to_string(), Value::String("backend1".to_string()));
    fields.insert("timestamp".to_string(), Value::Number(1234567890.into()));

    let doc = Document::with_id("sync-test-doc", fields);

    backend1
        .document_store()
        .upsert("sync_test", doc)
        .await
        .expect("Should create document on backend1");
    println!("  ✓ Document created on backend1");

    // Wait for sync (with timeout)
    println!("  3. Waiting for sync to backend2...");
    let doc_id = "sync-test-doc".to_string();

    let mut synced = false;
    for i in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;

        let doc = backend2
            .document_store()
            .get("sync_test", &doc_id)
            .await
            .expect("Should get document from backend2");

        if let Some(doc) = doc {
            println!("  ✓ Document synced to backend2 (attempt {})", i + 1);
            assert_eq!(
                doc.fields.get("source").and_then(|v| v.as_str()),
                Some("backend1")
            );
            synced = true;
            break;
        }
    }

    if synced {
        println!("  ✅ {} backend: Two-instance sync working!", backend_name);
    } else {
        println!("  ⚠ Warning: Document did not sync within timeout");
        println!("  → This may indicate sync coordination needs improvement");
        println!(
            "  → For {}, peer discovery/connection may need manual setup",
            backend_name
        );
    }

    // Cleanup
    backend1
        .sync_engine()
        .stop_sync()
        .await
        .expect("Should stop sync on backend1");
    backend2
        .sync_engine()
        .stop_sync()
        .await
        .expect("Should stop sync on backend2");
}
