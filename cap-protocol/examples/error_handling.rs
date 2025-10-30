//! Error Handling and Logging Example
//!
//! This example demonstrates the comprehensive error handling and structured logging
//! capabilities in the CAP protocol. It shows:
//!
//! - Error context extraction
//! - Error severity classification
//! - Recovery strategy determination
//! - Structured logging with tracing
//! - Error propagation patterns
//!
//! Run with: cargo run --example error_handling

use cap_protocol::storage::ditto_store::DittoStore;
use cap_protocol::{Error, Result};
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    info!("=== CAP Protocol Error Handling Example ===");

    // Example 1: Configuration errors
    println!("\n=== Example 1: Configuration Errors ===");
    demonstrate_config_errors();

    // Example 2: Storage errors with context
    println!("\n=== Example 2: Storage Errors ===");
    demonstrate_storage_errors().await;

    // Example 3: Error recovery strategies
    println!("\n=== Example 3: Error Recovery ===");
    demonstrate_error_recovery();

    // Example 4: Error severity classification
    println!("\n=== Example 4: Error Severity ===");
    demonstrate_error_severity();

    // Example 5: Error context extraction
    println!("\n=== Example 5: Error Context ===");
    demonstrate_error_context();

    info!("=== Example Complete ===");
    Ok(())
}

/// Demonstrates configuration error handling
fn demonstrate_config_errors() {
    println!("Testing configuration error handling...");

    // Simulate missing config
    let result: Result<()> = Err(Error::config_error(
        "Required configuration value not found",
        Some("DITTO_APP_ID".to_string()),
    ));

    if let Err(e) = result {
        error!("Configuration error: {}", e);
        println!("  Error: {}", e);
        println!("  Severity: {:?}", e.severity());
        println!("  Recoverable: {}", e.is_recoverable());

        let context = e.context();
        if let Some(key) = context.operation {
            println!("  Config key: {}", key);
        }
    }
}

/// Demonstrates storage error handling with context
async fn demonstrate_storage_errors() {
    println!("Testing storage error handling...");

    // Simulate storage operation failure
    let result: Result<()> = Err(Error::storage_error(
        "Failed to query collection",
        "query",
        Some("platform_state".to_string()),
    ));

    if let Err(e) = result {
        error!("Storage error: {}", e);
        println!("  Error: {}", e);
        println!("  Severity: {:?}", e.severity());
        println!("  Recoverable: {}", e.is_recoverable());

        let context = e.context();
        if let Some(key) = context.key {
            println!("  Collection: {}", key);
        }
        if let Some(op) = context.operation {
            println!("  Operation: {}", op);
        }
    }

    // Try to initialize DittoStore from environment (may fail if not configured)
    println!("\nAttempting to initialize DittoStore from environment...");
    match DittoStore::from_env() {
        Ok(_store) => {
            info!("DittoStore initialized successfully");
            println!("  ✓ DittoStore initialized");
        }
        Err(e) => {
            warn!("Failed to initialize DittoStore: {}", e);
            println!("  ✗ Failed: {}", e);
            println!("  This is expected if DITTO_* env vars are not set");
        }
    }
}

/// Demonstrates error recovery strategies
fn demonstrate_error_recovery() {
    println!("Testing error recovery strategies...");

    let errors = [
        Error::timeout_error("peer_discovery", 5000),
        Error::network_error("Connection refused", Some("peer_123".to_string())),
        Error::storage_error("Query failed", "query", Some("platforms".to_string())),
        Error::Internal("Critical system failure".to_string()),
    ];

    for (i, err) in errors.iter().enumerate() {
        println!("\nError {}: {}", i + 1, err);
        println!("  Severity: {:?}", err.severity());
        println!("  Recoverable: {}", err.is_recoverable());

        if err.is_recoverable() {
            println!("  → Recovery strategy: Retry with exponential backoff");
        } else {
            println!("  → Recovery strategy: Fail fast and report");
        }
    }
}

/// Demonstrates error severity classification
fn demonstrate_error_severity() {
    println!("Testing error severity classification...");

    let errors = vec![
        (
            "Critical",
            Error::Internal("System invariant violated".to_string()),
        ),
        (
            "Critical",
            Error::config_error("Invalid configuration", Some("APP_ID".to_string())),
        ),
        (
            "Error",
            Error::InvalidTransition {
                from: "Bootstrap".to_string(),
                to: "Hierarchical".to_string(),
                reason: "Must transition through Squad phase".to_string(),
            },
        ),
        ("Warning", Error::timeout_error("sync_operation", 3000)),
        (
            "Warning",
            Error::network_error("Peer unreachable", Some("peer_456".to_string())),
        ),
        (
            "Info",
            Error::NotFound {
                resource_type: "Platform".to_string(),
                id: "uav_001".to_string(),
            },
        ),
    ];

    for (expected, err) in errors {
        let severity = err.severity();
        println!("\n{:?} error: {}", severity, err);
        println!("  Expected: {}, Got: {:?}", expected, severity);
        assert_eq!(
            format!("{:?}", severity),
            expected,
            "Severity mismatch for: {}",
            err
        );
    }
}

/// Demonstrates error context extraction
fn demonstrate_error_context() {
    println!("Testing error context extraction...");

    // Storage error with context
    let storage_err =
        Error::storage_error("Query timeout", "query", Some("squad_members".to_string()));
    let ctx = storage_err.context();
    println!("\nStorage Error Context:");
    println!("  Key: {:?}", ctx.key);
    println!("  Operation: {:?}", ctx.operation);

    // Network error with peer ID
    let network_err = Error::network_error("Connection lost", Some("peer_789".to_string()));
    let ctx = network_err.context();
    println!("\nNetwork Error Context:");
    println!("  Peer ID: {:?}", ctx.peer_id);

    // Timeout error with duration
    let timeout_err = Error::timeout_error("bootstrap", 10000);
    let ctx = timeout_err.context();
    println!("\nTimeout Error Context:");
    println!("  Operation: {:?}", ctx.operation);
    println!("  Duration (ms): {:?}", ctx.duration_ms);
}
