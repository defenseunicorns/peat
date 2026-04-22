//! BlobStore Unit/Integration Tests (Single-Node)
//!
//! These tests validate that the IrohBlobStore implementation conforms to the
//! BlobStore trait.
//!
//! **NOTE**: These are NOT end-to-end tests. They only test single-node operations.
//! For actual multi-node blob sync tests, see `blob_sync_e2e.rs`.
//!
//! # Test Strategy
//!
//! - Tests use shared helper functions for identical test logic
//! - Validates that BlobStore trait abstraction works correctly
//!
//! # What This Proves
//!
//! 1. **Trait Abstraction Works**: IrohBlobStore implements BlobStore correctly
//! 2. **Content Addressing**: Blobs are identified by content hash
//! 3. **Metadata Handling**: Metadata is preserved correctly
//! 4. **CRUD Operations**: Create, read, delete operations work
//!
//! # What This Does NOT Prove
//!
//! - Blob transfer between mesh peers
//! - Remote blob fetch capabilities

#![cfg(feature = "automerge-backend")]

use peat_protocol::storage::{BlobMetadata, BlobStore, BlobStoreExt};
use std::sync::Arc;
use tempfile::TempDir;

// ============================================================================
// Iroh BlobStore Tests
// ============================================================================

/// Test basic blob operations with IrohBlobStore
#[tokio::test]
async fn test_iroh_blob_store_basic_operations() {
    println!("=== BlobStore E2E: Iroh Basic Operations ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("iroh_blobs");

    let blob_store = peat_protocol::storage::IrohBlobStore::new_in_memory(blob_dir)
        .await
        .expect("Failed to create IrohBlobStore");

    run_basic_blob_operations_test(Arc::new(blob_store), "IrohBlobStore").await;
}

/// Test blob metadata with IrohBlobStore
#[tokio::test]
async fn test_iroh_blob_store_metadata() {
    println!("=== BlobStore E2E: Iroh Metadata ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("iroh_blobs");

    let blob_store = peat_protocol::storage::IrohBlobStore::new_in_memory(blob_dir)
        .await
        .expect("Failed to create IrohBlobStore");

    run_metadata_test(Arc::new(blob_store), "IrohBlobStore").await;
}

/// Test blob from file with IrohBlobStore
#[tokio::test]
async fn test_iroh_blob_store_from_file() {
    println!("=== BlobStore E2E: Iroh From File ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("iroh_blobs");

    let blob_store = peat_protocol::storage::IrohBlobStore::new_in_memory(blob_dir)
        .await
        .expect("Failed to create IrohBlobStore");

    run_file_blob_test(Arc::new(blob_store), temp_dir.path(), "IrohBlobStore").await;
}

/// Test storage summary with IrohBlobStore
#[tokio::test]
async fn test_iroh_blob_store_storage_summary() {
    println!("=== BlobStore E2E: Iroh Storage Summary ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("iroh_blobs");

    let blob_store = peat_protocol::storage::IrohBlobStore::new_in_memory(blob_dir)
        .await
        .expect("Failed to create IrohBlobStore");

    run_storage_summary_test(Arc::new(blob_store), "IrohBlobStore").await;
}

// ============================================================================
// Shared Test Logic
// ============================================================================

/// Shared test logic for basic blob operations
async fn run_basic_blob_operations_test<B: BlobStore + 'static>(
    blob_store: Arc<B>,
    backend_name: &str,
) {
    println!("  Testing with {} backend", backend_name);

    // Test 1: Create blob from bytes
    println!("  1. Creating blob from bytes...");
    let test_data = b"Hello, this is test blob content!";
    let metadata = BlobMetadata::with_name("test-blob.txt");

    let token = blob_store
        .create_blob_from_bytes(test_data, metadata)
        .await
        .expect("Should create blob from bytes");

    assert!(!token.hash.as_hex().is_empty(), "Hash should not be empty");
    assert_eq!(token.size_bytes, test_data.len() as u64);
    assert_eq!(token.metadata.name, Some("test-blob.txt".to_string()));
    println!("  ✓ Blob created: hash={}", token.hash);

    // Test 2: Check blob exists locally
    println!("  2. Checking blob exists locally...");
    // After creation, blob should be known (cached)
    let info = blob_store.blob_info(&token.hash);
    assert!(info.is_some(), "Should have blob info after creation");
    println!("  ✓ Blob info available");

    // Test 3: List local blobs
    println!("  3. Listing local blobs...");
    let local_blobs = blob_store.list_local_blobs();
    assert!(
        !local_blobs.is_empty(),
        "Should have at least one local blob"
    );
    println!("  ✓ Found {} local blob(s)", local_blobs.len());

    // Test 4: Check storage bytes
    println!("  4. Checking storage bytes...");
    let storage_bytes = blob_store.local_storage_bytes();
    assert!(
        storage_bytes >= test_data.len() as u64,
        "Storage should include our blob"
    );
    println!("  ✓ Storage: {} bytes", storage_bytes);

    // Test 5: Delete blob
    println!("  5. Deleting blob...");
    blob_store
        .delete_blob(&token.hash)
        .await
        .expect("Should delete blob");

    // After deletion, blob_info should return None
    let info_after = blob_store.blob_info(&token.hash);
    assert!(
        info_after.is_none(),
        "Blob info should be gone after delete"
    );
    println!("  ✓ Blob deleted");

    println!(
        "  ✅ {} backend: All basic operations passed!",
        backend_name
    );
}

/// Shared test logic for metadata handling
async fn run_metadata_test<B: BlobStore + 'static>(blob_store: Arc<B>, backend_name: &str) {
    println!("  Testing metadata with {} backend", backend_name);

    // Create blob with rich metadata
    println!("  1. Creating blob with rich metadata...");
    let test_data = b"Model weights data here...";
    let metadata = BlobMetadata::with_name_and_type("yolov8.onnx", "application/onnx")
        .with_custom("version", "1.0.0")
        .with_custom("precision", "fp16")
        .with_custom("model_id", "target_recognition");

    let token = blob_store
        .create_blob_from_bytes(test_data, metadata)
        .await
        .expect("Should create blob with metadata");

    // Verify metadata is preserved
    println!("  2. Verifying metadata preservation...");
    assert_eq!(token.metadata.name, Some("yolov8.onnx".to_string()));
    assert_eq!(
        token.metadata.content_type,
        Some("application/onnx".to_string())
    );
    assert_eq!(
        token.metadata.custom.get("version"),
        Some(&"1.0.0".to_string())
    );
    assert_eq!(
        token.metadata.custom.get("precision"),
        Some(&"fp16".to_string())
    );
    assert_eq!(
        token.metadata.custom.get("model_id"),
        Some(&"target_recognition".to_string())
    );
    println!("  ✓ All metadata preserved");

    // Verify blob_info returns same metadata
    println!("  3. Verifying blob_info returns metadata...");
    let info = blob_store.blob_info(&token.hash);
    assert!(info.is_some(), "Should have blob info");
    let info = info.unwrap();
    assert_eq!(info.metadata.name, token.metadata.name);
    assert_eq!(info.metadata.content_type, token.metadata.content_type);
    println!("  ✓ blob_info metadata matches");

    println!("  ✅ {} backend: Metadata test passed!", backend_name);
}

/// Shared test logic for file-based blob creation
async fn run_file_blob_test<B: BlobStore + 'static>(
    blob_store: Arc<B>,
    temp_path: &std::path::Path,
    backend_name: &str,
) {
    println!("  Testing file blob with {} backend", backend_name);

    // Create a test file
    println!("  1. Creating test file...");
    let test_file = temp_path.join("test_model.onnx");
    let file_content =
        b"This simulates ONNX model binary content with more data to make it realistic...";
    std::fs::write(&test_file, file_content).expect("Should write test file");
    println!("  ✓ Test file created: {:?}", test_file);

    // Create blob from file
    println!("  2. Creating blob from file...");
    let metadata = BlobMetadata::with_name_and_type("test_model.onnx", "application/onnx");

    let token = blob_store
        .create_blob(&test_file, metadata)
        .await
        .expect("Should create blob from file");

    assert!(!token.hash.as_hex().is_empty());
    assert_eq!(token.size_bytes, file_content.len() as u64);
    assert_eq!(token.metadata.name, Some("test_model.onnx".to_string()));
    println!("  ✓ Blob created from file: hash={}", token.hash);

    // Verify blob is tracked
    println!("  3. Verifying blob is tracked...");
    let info = blob_store.blob_info(&token.hash);
    assert!(info.is_some(), "Should track blob after file creation");
    println!("  ✓ Blob tracked in store");

    println!("  ✅ {} backend: File blob test passed!", backend_name);
}

/// Shared test logic for storage summary
async fn run_storage_summary_test<B: BlobStore + 'static>(blob_store: Arc<B>, backend_name: &str) {
    println!("  Testing storage summary with {} backend", backend_name);

    // Create multiple blobs
    println!("  1. Creating multiple blobs...");
    let blobs_to_create = vec![
        (b"Small blob content".to_vec(), "small.txt"),
        (b"Medium blob with more content here".to_vec(), "medium.txt"),
        (vec![0u8; 1000], "larger.bin"), // 1KB blob
    ];

    let mut total_size = 0u64;
    for (data, name) in &blobs_to_create {
        let metadata = BlobMetadata::with_name(*name);
        blob_store
            .create_blob_from_bytes(data, metadata)
            .await
            .expect("Should create blob");
        total_size += data.len() as u64;
    }
    println!(
        "  ✓ Created {} blobs, total {} bytes",
        blobs_to_create.len(),
        total_size
    );

    // Get storage summary
    println!("  2. Getting storage summary...");
    let summary = blob_store.storage_summary();
    println!("    Blob count: {}", summary.blob_count);
    println!("    Total bytes: {}", summary.total_bytes);
    println!("    Largest blob: {:?}", summary.largest_blob);

    assert_eq!(summary.blob_count, blobs_to_create.len());
    assert!(
        summary.total_bytes >= total_size,
        "Total should be at least {}",
        total_size
    );
    assert!(summary.largest_blob.is_some());
    assert!(
        summary.largest_blob.unwrap() >= 1000,
        "Largest should be at least 1KB"
    );

    println!(
        "  ✅ {} backend: Storage summary test passed!",
        backend_name
    );
}
