//! Integration tests for data sync backend implementations
//!
//! These tests verify that the sync backend abstraction layer correctly
//! delegates to the underlying Ditto SDK for single-node CRUD operations.
//! Unlike E2E tests, these don't test multi-peer synchronization.

use hive_protocol::sync::ditto::DittoBackend;
use hive_protocol::sync::{
    BackendConfig, DataSyncBackend, Document, Query, TransportConfig, Value,
};
use std::collections::HashMap;

/// Helper to create a test backend with real Ditto instance
/// Returns None if credentials are not available (test should skip)
async fn setup_backend() -> Option<DittoBackend> {
    dotenvy::dotenv().ok();

    // Skip if credentials not available (prefer HIVE_*, fallback to DITTO_*)
    let app_id = std::env::var("HIVE_APP_ID")
        .or_else(|_| std::env::var("DITTO_APP_ID"))
        .ok()
        .and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
    let Some(app_id) = app_id else {
        eprintln!("Skipping test: HIVE_APP_ID/DITTO_APP_ID not set");
        return None;
    };

    let shared_key = std::env::var("HIVE_SECRET_KEY")
        .or_else(|_| std::env::var("HIVE_SHARED_KEY"))
        .or_else(|_| std::env::var("DITTO_SHARED_KEY"))
        .ok()
        .and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
    let Some(shared_key) = shared_key else {
        eprintln!("Skipping test: HIVE_SECRET_KEY not set");
        return None;
    };

    let offline_token = match std::env::var("HIVE_OFFLINE_TOKEN")
        .or_else(|_| std::env::var("DITTO_OFFLINE_TOKEN"))
    {
        Ok(t) => t,
        Err(_) => {
            eprintln!("Skipping test: HIVE_OFFLINE_TOKEN not set");
            return None;
        }
    };

    let mut extra = HashMap::new();
    extra.insert("offline_token".to_string(), offline_token);

    let config = BackendConfig {
        app_id,
        persistence_dir: tempfile::tempdir()
            .expect("Failed to create temp dir")
            .path()
            .to_path_buf(),
        shared_key: Some(shared_key),
        transport: TransportConfig::default(),
        extra,
    };

    let backend = DittoBackend::new();
    backend
        .initialize(config)
        .await
        .expect("Failed to initialize backend");

    // Start sync (even though we won't test multi-peer sync here)
    backend
        .sync_engine()
        .start_sync()
        .await
        .expect("Failed to start sync");

    Some(backend)
}

async fn cleanup_backend(backend: DittoBackend) {
    backend.shutdown().await.ok();
    // Give Ditto time to shut down
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_backend_lifecycle() {
    let Some(backend) = setup_backend().await else { return; };

    // Verify backend is ready
    assert!(backend.is_ready().await);

    // Verify sync is active
    assert!(backend.sync_engine().is_syncing().await.unwrap());

    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_document_upsert() {
    let Some(backend) = setup_backend().await else { return; };
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
async fn test_document_query() {
    let Some(backend) = setup_backend().await else { return; };
    let doc_store = backend.document_store();

    // Insert a test document
    let mut fields = HashMap::new();
    fields.insert(
        "test_id".to_string(),
        Value::String("query_test".to_string()),
    );
    fields.insert(
        "score".to_string(),
        Value::Number(serde_json::Number::from(85)),
    );

    let doc = Document::new(fields);
    let doc_id = doc_store
        .upsert("test_collection", doc)
        .await
        .expect("Upsert should succeed");

    // Query for the document
    let query = Query::Eq {
        field: "test_id".to_string(),
        value: Value::String("query_test".to_string()),
    };

    let results = doc_store
        .query("test_collection", &query)
        .await
        .expect("Query should succeed");

    assert_eq!(results.len(), 1, "Should find exactly one document");
    assert_eq!(
        results[0].get("test_id"),
        Some(&Value::String("query_test".to_string()))
    );
    assert_eq!(
        results[0].get("score"),
        Some(&Value::Number(serde_json::Number::from(85)))
    );

    // Clean up
    doc_store.remove("test_collection", &doc_id).await.ok();
    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_document_query_all() {
    let Some(backend) = setup_backend().await else { return; };
    let doc_store = backend.document_store();

    // Insert multiple documents
    let mut doc_ids = Vec::new();

    for i in 0..3 {
        let mut fields = HashMap::new();
        fields.insert(
            "test_batch".to_string(),
            Value::String("all_test".to_string()),
        );
        fields.insert(
            "index".to_string(),
            Value::Number(serde_json::Number::from(i)),
        );

        let doc = Document::new(fields);
        let doc_id = doc_store
            .upsert("test_collection_all", doc)
            .await
            .expect("Upsert should succeed");
        doc_ids.push(doc_id);
    }

    // Query all documents in collection
    let query = Query::All;
    let results = doc_store
        .query("test_collection_all", &query)
        .await
        .expect("Query should succeed");

    assert!(
        results.len() >= 3,
        "Should find at least 3 documents (may have more from other tests)"
    );

    // Clean up
    for doc_id in doc_ids {
        doc_store.remove("test_collection_all", &doc_id).await.ok();
    }
    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_document_query_with_comparison() {
    let Some(backend) = setup_backend().await else { return; };
    let doc_store = backend.document_store();

    // Insert documents with different scores
    let mut doc_ids = Vec::new();

    for score in [10, 50, 90] {
        let mut fields = HashMap::new();
        fields.insert(
            "test_type".to_string(),
            Value::String("comparison_test".to_string()),
        );
        fields.insert(
            "score".to_string(),
            Value::Number(serde_json::Number::from(score)),
        );

        let doc = Document::new(fields);
        let doc_id = doc_store
            .upsert("test_collection_compare", doc)
            .await
            .expect("Upsert should succeed");
        doc_ids.push(doc_id);
    }

    // Query for scores greater than 40
    let query = Query::And(vec![
        Query::Eq {
            field: "test_type".to_string(),
            value: Value::String("comparison_test".to_string()),
        },
        Query::Gt {
            field: "score".to_string(),
            value: Value::Number(serde_json::Number::from(40)),
        },
    ]);

    let results = doc_store
        .query("test_collection_compare", &query)
        .await
        .expect("Query should succeed");

    assert_eq!(results.len(), 2, "Should find 2 documents with score > 40");

    // Verify both have scores > 40
    for doc in &results {
        if let Some(Value::Number(score)) = doc.get("score") {
            assert!(
                score.as_i64().unwrap() > 40,
                "Document score should be > 40"
            );
        }
    }

    // Clean up
    for doc_id in doc_ids {
        doc_store
            .remove("test_collection_compare", &doc_id)
            .await
            .ok();
    }
    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_document_remove() {
    let Some(backend) = setup_backend().await else { return; };
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

    // Verify it exists
    let query = Query::Eq {
        field: "test_id".to_string(),
        value: Value::String("remove_test".to_string()),
    };
    let results = doc_store
        .query("test_collection_remove", &query)
        .await
        .expect("Query should succeed");
    assert_eq!(results.len(), 1, "Document should exist before removal");

    // Remove it
    doc_store
        .remove("test_collection_remove", &doc_id)
        .await
        .expect("Remove should succeed");

    // Verify it's gone
    let results = doc_store
        .query("test_collection_remove", &query)
        .await
        .expect("Query should succeed");
    assert_eq!(results.len(), 0, "Document should be removed");

    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_document_get() {
    let Some(backend) = setup_backend().await else { return; };
    let doc_store = backend.document_store();

    // Insert a document
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), Value::String("get_test".to_string()));

    let doc = Document::new(fields);
    let doc_id = doc_store
        .upsert("test_collection_get", doc)
        .await
        .expect("Upsert should succeed");

    // Get by ID (convenience method)
    let retrieved = doc_store
        .get("test_collection_get", &doc_id)
        .await
        .expect("Get should succeed");

    assert!(retrieved.is_some(), "Document should be found");
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, Some(doc_id.clone()));
    assert_eq!(
        retrieved.get("name"),
        Some(&Value::String("get_test".to_string()))
    );

    // Clean up
    doc_store.remove("test_collection_get", &doc_id).await.ok();
    cleanup_backend(backend).await;
}

#[tokio::test]
async fn test_document_count() {
    let Some(backend) = setup_backend().await else { return; };
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
    let Some(backend) = setup_backend().await else { return; };
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
