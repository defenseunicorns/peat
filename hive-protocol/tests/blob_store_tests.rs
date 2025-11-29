//! BlobStore Unit/Integration Tests (Single-Node)
//!
//! These tests validate that both DittoBlobStore and IrohBlobStore implementations
//! conform to the BlobStore trait and produce identical behavior.
//!
//! **NOTE**: These are NOT end-to-end tests. They only test single-node operations.
//! For actual multi-node blob sync tests, see `blob_sync_e2e.rs`.
//!
//! # Test Strategy
//!
//! - Each test has two variants: one for Ditto, one for Iroh
//! - Tests use shared helper functions for identical test logic
//! - Validates that BlobStore trait abstraction works correctly
//!
//! # What This Proves
//!
//! 1. **Trait Abstraction Works**: Both backends implement BlobStore correctly
//! 2. **Content Addressing**: Blobs are identified by content hash
//! 3. **Metadata Handling**: Both backends preserve metadata correctly
//! 4. **CRUD Operations**: Create, read, delete operations work identically
//!
//! # What This Does NOT Prove
//!
//! - Blob transfer between mesh peers
//! - Attachment sync via Ditto's mesh protocol
//! - Remote blob fetch capabilities

use hive_protocol::storage::ditto_store::DittoConfig;
use hive_protocol::storage::{BlobMetadata, BlobStore, BlobStoreExt};
use std::sync::Arc;
use tempfile::TempDir;

// ============================================================================
// Ditto BlobStore Tests
// ============================================================================

/// Test basic blob operations with DittoBlobStore
#[tokio::test]
async fn test_ditto_blob_store_basic_operations() {
    dotenvy::dotenv().ok();

    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for Ditto tests");

    println!("=== BlobStore E2E: Ditto Basic Operations ===");

    // Create Ditto store
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("ditto_blobs");
    std::fs::create_dir_all(&blob_dir).unwrap();

    let ditto_store = create_ditto_store(&ditto_app_id, temp_dir.path());
    let blob_store =
        hive_protocol::storage::DittoBlobStore::with_blob_dir(Arc::new(ditto_store), blob_dir);

    run_basic_blob_operations_test(Arc::new(blob_store), "DittoBlobStore").await;
}

/// Test blob metadata with DittoBlobStore
#[tokio::test]
async fn test_ditto_blob_store_metadata() {
    dotenvy::dotenv().ok();

    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for Ditto tests");

    println!("=== BlobStore E2E: Ditto Metadata ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("ditto_blobs");
    std::fs::create_dir_all(&blob_dir).unwrap();

    let ditto_store = create_ditto_store(&ditto_app_id, temp_dir.path());
    let blob_store =
        hive_protocol::storage::DittoBlobStore::with_blob_dir(Arc::new(ditto_store), blob_dir);

    run_metadata_test(Arc::new(blob_store), "DittoBlobStore").await;
}

/// Test blob from file with DittoBlobStore
#[tokio::test]
async fn test_ditto_blob_store_from_file() {
    dotenvy::dotenv().ok();

    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for Ditto tests");

    println!("=== BlobStore E2E: Ditto From File ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("ditto_blobs");
    std::fs::create_dir_all(&blob_dir).unwrap();

    let ditto_store = create_ditto_store(&ditto_app_id, temp_dir.path());
    let blob_store =
        hive_protocol::storage::DittoBlobStore::with_blob_dir(Arc::new(ditto_store), blob_dir);

    run_file_blob_test(Arc::new(blob_store), temp_dir.path(), "DittoBlobStore").await;
}

// ============================================================================
// Iroh BlobStore Tests
// ============================================================================

/// Test basic blob operations with IrohBlobStore
#[cfg(feature = "automerge-backend")]
#[tokio::test]
async fn test_iroh_blob_store_basic_operations() {
    println!("=== BlobStore E2E: Iroh Basic Operations ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("iroh_blobs");

    let blob_store = hive_protocol::storage::IrohBlobStore::new_in_memory(blob_dir)
        .await
        .expect("Failed to create IrohBlobStore");

    run_basic_blob_operations_test(Arc::new(blob_store), "IrohBlobStore").await;
}

/// Test blob metadata with IrohBlobStore
#[cfg(feature = "automerge-backend")]
#[tokio::test]
async fn test_iroh_blob_store_metadata() {
    println!("=== BlobStore E2E: Iroh Metadata ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("iroh_blobs");

    let blob_store = hive_protocol::storage::IrohBlobStore::new_in_memory(blob_dir)
        .await
        .expect("Failed to create IrohBlobStore");

    run_metadata_test(Arc::new(blob_store), "IrohBlobStore").await;
}

/// Test blob from file with IrohBlobStore
#[cfg(feature = "automerge-backend")]
#[tokio::test]
async fn test_iroh_blob_store_from_file() {
    println!("=== BlobStore E2E: Iroh From File ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("iroh_blobs");

    let blob_store = hive_protocol::storage::IrohBlobStore::new_in_memory(blob_dir)
        .await
        .expect("Failed to create IrohBlobStore");

    run_file_blob_test(Arc::new(blob_store), temp_dir.path(), "IrohBlobStore").await;
}

/// Test storage summary with IrohBlobStore
#[cfg(feature = "automerge-backend")]
#[tokio::test]
async fn test_iroh_blob_store_storage_summary() {
    println!("=== BlobStore E2E: Iroh Storage Summary ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let blob_dir = temp_dir.path().join("iroh_blobs");

    let blob_store = hive_protocol::storage::IrohBlobStore::new_in_memory(blob_dir)
        .await
        .expect("Failed to create IrohBlobStore");

    run_storage_summary_test(Arc::new(blob_store), "IrohBlobStore").await;
}

// ============================================================================
// Backend Comparison Tests
// ============================================================================

/// Test that both backends produce valid content hashes
#[cfg(feature = "automerge-backend")]
#[tokio::test]
async fn test_both_backends_content_addressing() {
    dotenvy::dotenv().ok();

    println!("=== BlobStore E2E: Content Addressing Comparison ===");

    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for Ditto tests");

    let test_data = b"Test content for both backends";
    let metadata = BlobMetadata::with_name("test.txt");

    // Create Ditto blob
    let temp_dir_ditto = TempDir::new().unwrap();
    let blob_dir_ditto = temp_dir_ditto.path().join("ditto_blobs");
    std::fs::create_dir_all(&blob_dir_ditto).unwrap();
    let ditto_store = create_ditto_store(&ditto_app_id, temp_dir_ditto.path());
    let ditto_blob_store = hive_protocol::storage::DittoBlobStore::with_blob_dir(
        Arc::new(ditto_store),
        blob_dir_ditto,
    );

    let ditto_token = ditto_blob_store
        .create_blob_from_bytes(test_data, metadata.clone())
        .await
        .expect("Ditto should create blob");

    // Create Iroh blob
    let temp_dir_iroh = TempDir::new().unwrap();
    let blob_dir_iroh = temp_dir_iroh.path().join("iroh_blobs");
    let iroh_blob_store = hive_protocol::storage::IrohBlobStore::new_in_memory(blob_dir_iroh)
        .await
        .expect("Failed to create IrohBlobStore");

    let iroh_token = iroh_blob_store
        .create_blob_from_bytes(test_data, metadata)
        .await
        .expect("Iroh should create blob");

    println!("  Ditto hash: {}", ditto_token.hash.as_hex());
    println!("  Iroh hash:  {}", iroh_token.hash.as_hex());

    // Verify both produce valid hashes (different algorithms: SHA256 vs BLAKE3)
    assert!(
        !ditto_token.hash.as_hex().is_empty(),
        "Ditto hash should not be empty"
    );
    assert!(
        !iroh_token.hash.as_hex().is_empty(),
        "Iroh hash should not be empty"
    );

    // Both should have same size
    assert_eq!(
        ditto_token.size_bytes, iroh_token.size_bytes,
        "Both should report same size"
    );
    assert_eq!(
        ditto_token.size_bytes,
        test_data.len() as u64,
        "Size should match data length"
    );

    // Both should preserve metadata name
    assert_eq!(ditto_token.metadata.name, Some("test.txt".to_string()));
    assert_eq!(iroh_token.metadata.name, Some("test.txt".to_string()));

    println!("  ✅ Both backends produce valid content-addressed hashes!");
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
#[allow(dead_code)]
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

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a Ditto store for testing
/// Panics if DITTO_SHARED_KEY is not set (tests must fail, not skip)
fn create_ditto_store(
    app_id: &str,
    base_path: &std::path::Path,
) -> hive_protocol::storage::DittoStore {
    use hive_protocol::storage::DittoStore;

    // Get shared key from environment (required for Ditto)
    let shared_key =
        std::env::var("DITTO_SHARED_KEY").expect("DITTO_SHARED_KEY must be set for Ditto tests");

    assert!(
        !shared_key.trim().is_empty(),
        "DITTO_SHARED_KEY must not be empty"
    );
    let shared_key = shared_key.trim().to_string();

    let persistence_dir = base_path.join("ditto_data");
    std::fs::create_dir_all(&persistence_dir).unwrap();

    let config = DittoConfig {
        app_id: app_id.to_string(),
        persistence_dir,
        shared_key,
        tcp_listen_port: None,
        tcp_connect_address: None,
    };

    DittoStore::new(config).expect("Failed to create DittoStore")
}
