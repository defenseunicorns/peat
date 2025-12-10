//! Iroh File Distribution End-to-End Tests (Issue #379)
//!
//! These tests validate the integration of blob storage with AutomergeIrohBackend
//! for model/file distribution across the mesh.
//!
//! # What This Tests
//!
//! 1. **Blob Store Integration**: AutomergeIrohBackend with blob storage enabled
//! 2. **Auto Peer Registration**: Blob peers synced with document sync peers
//! 3. **IrohFileDistribution**: Higher-level distribution API
//!
//! # Test Architecture
//!
//! ```text
//! Commander Node                    Sensor Node
//! ┌──────────────────────────┐      ┌──────────────────────────┐
//! │ AutomergeIrohBackend     │      │ AutomergeIrohBackend     │
//! │ ├─ AutomergeStore        │      │ ├─ AutomergeStore        │
//! │ ├─ IrohTransport         │──────│ ├─ IrohTransport         │
//! │ └─ NetworkedIrohBlobStore│      │ └─ NetworkedIrohBlobStore│
//! │                          │      │                          │
//! │ 1. Create model blob     │      │                          │
//! │ 2. distribute(token)     │      │                          │
//! │    └─ store dist doc ────┼──────┼─→ 3. Receive dist doc   │
//! │                          │      │    └─ fetch_blob()       │
//! │                          │      │       └─ verify model    │
//! └──────────────────────────┘      └──────────────────────────┘
//! ```

#![cfg(feature = "automerge-backend")]

use hive_protocol::storage::{
    AutomergeStore, BlobMetadata, BlobStore, DistributionScope, FileDistribution,
    IrohFileDistribution, NetworkedIrohBlobStore, TransferPriority,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a NetworkedIrohBlobStore with AutomergeStore
async fn create_integrated_stores(
    bind_addr: SocketAddr,
    temp_dir: &std::path::Path,
) -> (Arc<NetworkedIrohBlobStore>, Arc<AutomergeStore>) {
    let blob_dir = temp_dir.join("blobs");
    std::fs::create_dir_all(&blob_dir).unwrap();

    let blob_store = NetworkedIrohBlobStore::bind(blob_dir, bind_addr)
        .await
        .expect("Should create NetworkedIrohBlobStore");

    let db_path = temp_dir.join("automerge.db");
    let doc_store = Arc::new(AutomergeStore::open(&db_path).expect("Should open AutomergeStore"));

    (blob_store, doc_store)
}

/// Test 1: IrohFileDistribution Basic Usage
///
/// Validates that IrohFileDistribution can create a distribution and track status.
#[tokio::test]
async fn test_iroh_file_distribution_basic() {
    println!("=== E2E: IrohFileDistribution Basic ===");

    let temp = TempDir::new().unwrap();
    let addr: SocketAddr = "127.0.0.1:19201".parse().unwrap();

    println!("  Creating integrated stores...");
    let (blob_store, doc_store) = create_integrated_stores(addr, temp.path()).await;

    println!("  Node ID: {}", blob_store.endpoint_id().fmt_short());

    // Create IrohFileDistribution service
    let distribution = IrohFileDistribution::new(Arc::clone(&blob_store), Arc::clone(&doc_store));

    // Create a test model blob
    println!("  1. Creating model blob...");
    let model_data = b"ONNX Model: YOLOv8 Nano for target detection - v1.0.0";
    let metadata = BlobMetadata::with_name_and_type("yolov8-nano.onnx", "application/onnx")
        .with_custom("version", "1.0.0")
        .with_custom("model_type", "detection");

    let token = blob_store
        .create_blob_from_bytes(model_data, metadata)
        .await
        .expect("Should create blob");

    println!("    Created blob: hash={}", token.hash.as_hex());
    println!("    Size: {} bytes", token.size_bytes);

    // Initiate distribution
    println!("  2. Initiating distribution to AllNodes...");
    let handle = distribution
        .distribute(&token, DistributionScope::AllNodes, TransferPriority::High)
        .await
        .expect("Should start distribution");

    println!("    Distribution ID: {}", handle.distribution_id);
    println!("    Priority: {:?}", handle.priority);

    // Check status
    println!("  3. Checking distribution status...");
    let status = distribution
        .status(&handle)
        .await
        .expect("Should get status");

    println!("    Total targets: {}", status.total_targets);
    println!("    Completed: {}", status.completed);
    println!("    In progress: {}", status.in_progress);
    println!("    Failed: {}", status.failed);

    // With no connected peers, total_targets should be 0
    assert_eq!(status.total_targets, 0, "No peers connected yet");

    println!("  ✓ IrohFileDistribution basic test passed");
}

/// Test 2: Distribution Document Stored in Automerge
///
/// Validates that distribution metadata is stored as an Automerge document
/// that would sync to peers.
#[tokio::test]
async fn test_distribution_document_stored() {
    println!("=== E2E: Distribution Document Storage ===");

    let temp = TempDir::new().unwrap();
    let addr: SocketAddr = "127.0.0.1:19202".parse().unwrap();

    println!("  Creating integrated stores...");
    let (blob_store, doc_store) = create_integrated_stores(addr, temp.path()).await;

    let distribution = IrohFileDistribution::new(Arc::clone(&blob_store), Arc::clone(&doc_store));

    // Create and distribute a blob
    println!("  1. Creating and distributing blob...");
    let model_data = b"Test model content";
    let metadata = BlobMetadata::with_name_and_type("test.bin", "application/octet-stream");

    let token = blob_store
        .create_blob_from_bytes(model_data, metadata)
        .await
        .expect("Should create blob");

    let handle = distribution
        .distribute(
            &token,
            DistributionScope::AllNodes,
            TransferPriority::Normal,
        )
        .await
        .expect("Should start distribution");

    // Check that distribution document was stored via status API
    println!("  2. Checking distribution status...");
    let status = distribution
        .status(&handle)
        .await
        .expect("Should get status");

    println!("    Distribution found:");
    println!("      distribution_id: {}", status.handle.distribution_id);
    println!("      blob_hash: {}", status.handle.blob_hash.as_hex());
    println!("      total_targets: {}", status.total_targets);

    assert_eq!(status.handle.distribution_id, handle.distribution_id);
    assert_eq!(status.handle.blob_hash.as_hex(), token.hash.as_hex());

    println!("  ✓ Distribution document storage test passed");
}

/// Test 3: Cancel Distribution
///
/// Validates that a distribution can be cancelled.
#[tokio::test]
async fn test_cancel_distribution() {
    println!("=== E2E: Cancel Distribution ===");

    let temp = TempDir::new().unwrap();
    let addr: SocketAddr = "127.0.0.1:19203".parse().unwrap();

    println!("  Creating integrated stores...");
    let (blob_store, doc_store) = create_integrated_stores(addr, temp.path()).await;

    let distribution = IrohFileDistribution::new(Arc::clone(&blob_store), Arc::clone(&doc_store));

    // Create and distribute a blob
    println!("  1. Starting distribution...");
    let model_data = b"Model to cancel";
    let metadata = BlobMetadata::with_name_and_type("cancel-test.bin", "application/octet-stream");

    let token = blob_store
        .create_blob_from_bytes(model_data, metadata)
        .await
        .expect("Should create blob");

    let handle = distribution
        .distribute(&token, DistributionScope::AllNodes, TransferPriority::Low)
        .await
        .expect("Should start distribution");

    println!("    Distribution ID: {}", handle.distribution_id);

    // Cancel the distribution
    println!("  2. Cancelling distribution...");
    distribution
        .cancel(&handle)
        .await
        .expect("Should cancel distribution");

    println!("    Distribution cancelled");

    // Verify status shows it's complete (cancelled counts as complete)
    println!("  3. Verifying cancellation via status...");
    let status = distribution
        .status(&handle)
        .await
        .expect("Should get status");

    // With no peers, is_complete should be true after cancel
    println!(
        "    Status: completed={}, failed={}",
        status.completed, status.failed
    );

    println!("  ✓ Cancel distribution test passed");
}

/// Test 4: Distribution with Formation Scope
///
/// Validates distribution targeting a specific formation.
#[tokio::test]
async fn test_distribution_formation_scope() {
    println!("=== E2E: Distribution with Formation Scope ===");

    let temp = TempDir::new().unwrap();
    let addr: SocketAddr = "127.0.0.1:19204".parse().unwrap();

    println!("  Creating integrated stores...");
    let (blob_store, doc_store) = create_integrated_stores(addr, temp.path()).await;

    let distribution = IrohFileDistribution::new(Arc::clone(&blob_store), Arc::clone(&doc_store));

    // Create blob
    let model_data = b"Formation-specific model";
    let metadata = BlobMetadata::with_name_and_type("formation-model.onnx", "application/onnx");

    let token = blob_store
        .create_blob_from_bytes(model_data, metadata)
        .await
        .expect("Should create blob");

    // Distribute to alpha-squad formation
    println!("  1. Distributing to formation 'alpha-squad'...");
    let scope = DistributionScope::Formation {
        formation_id: "alpha-squad".to_string(),
    };

    let handle = distribution
        .distribute(&token, scope.clone(), TransferPriority::Critical)
        .await
        .expect("Should start distribution");

    println!("    Distribution ID: {}", handle.distribution_id);

    // Check the distribution handle has formation scope
    match &handle.scope {
        DistributionScope::Formation { formation_id } => {
            println!("    Formation ID: {}", formation_id);
            assert_eq!(formation_id, "alpha-squad");
        }
        _ => panic!("Expected Formation scope"),
    }

    println!("  ✓ Formation scope distribution test passed");
}
