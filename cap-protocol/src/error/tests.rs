//! Tests for error handling functionality

use crate::error::{Error, ErrorSeverity};

#[test]
fn test_error_is_recoverable() {
    // Recoverable errors
    assert!(Error::timeout_error("operation", 1000).is_recoverable());
    assert!(Error::network_error("test", None).is_recoverable());
    assert!(Error::storage_error("query failed", "query", None).is_recoverable());
    assert!(Error::storage_error("retrieve failed", "retrieve", None).is_recoverable());

    // Non-recoverable errors
    assert!(!Error::Internal("critical".to_string()).is_recoverable());
    assert!(!Error::config_error("bad config", None).is_recoverable());
    assert!(!Error::storage_error("write failed", "write", None).is_recoverable());
}

#[test]
fn test_error_severity() {
    // Critical
    assert_eq!(
        Error::Internal("test".to_string()).severity(),
        ErrorSeverity::Critical
    );
    assert_eq!(
        Error::config_error("test", None).severity(),
        ErrorSeverity::Critical
    );

    // Error level
    assert_eq!(
        Error::InvalidTransition {
            from: "A".to_string(),
            to: "B".to_string(),
            reason: "test".to_string()
        }
        .severity(),
        ErrorSeverity::Error
    );

    // Warning
    assert_eq!(
        Error::timeout_error("test", 100).severity(),
        ErrorSeverity::Warning
    );
    assert_eq!(
        Error::network_error("test", None).severity(),
        ErrorSeverity::Warning
    );

    // Info
    assert_eq!(
        Error::NotFound {
            resource_type: "test".to_string(),
            id: "123".to_string()
        }
        .severity(),
        ErrorSeverity::Info
    );
}

#[test]
fn test_error_context_storage() {
    let err = Error::storage_error("test", "query", Some("collection".to_string()));
    let ctx = err.context();

    assert_eq!(ctx.key, Some("collection".to_string()));
    assert_eq!(ctx.operation, Some("query".to_string()));
    assert_eq!(ctx.peer_id, None);
    assert_eq!(ctx.squad_id, None);
}

#[test]
fn test_error_context_network() {
    let err = Error::network_error("test", Some("peer_123".to_string()));
    let ctx = err.context();

    assert_eq!(ctx.peer_id, Some("peer_123".to_string()));
    assert_eq!(ctx.key, None);
    assert_eq!(ctx.operation, None);
}

#[test]
fn test_error_context_timeout() {
    let err = Error::timeout_error("bootstrap", 5000);
    let ctx = err.context();

    assert_eq!(ctx.operation, Some("bootstrap".to_string()));
    assert_eq!(ctx.duration_ms, Some(5000));
}

#[test]
fn test_error_display() {
    let err = Error::storage_error("query failed", "query", Some("nodes".to_string()));
    let display = format!("{}", err);
    assert!(display.contains("Storage error"));
    assert!(display.contains("query failed"));
}

#[test]
fn test_error_helper_constructors() {
    // Storage error
    let err = Error::storage_error("msg", "op", Some("key".to_string()));
    match err {
        Error::Storage {
            message,
            operation,
            key,
            ..
        } => {
            assert_eq!(message, "msg");
            assert_eq!(operation, Some("op".to_string()));
            assert_eq!(key, Some("key".to_string()));
        }
        _ => panic!("Wrong error type"),
    }

    // Network error
    let err = Error::network_error("msg", Some("peer".to_string()));
    match err {
        Error::Network {
            message, peer_id, ..
        } => {
            assert_eq!(message, "msg");
            assert_eq!(peer_id, Some("peer".to_string()));
        }
        _ => panic!("Wrong error type"),
    }

    // Timeout error
    let err = Error::timeout_error("op", 1000);
    match err {
        Error::Timeout {
            operation,
            duration_ms,
        } => {
            assert_eq!(operation, "op");
            assert_eq!(duration_ms, 1000);
        }
        _ => panic!("Wrong error type"),
    }

    // Config error
    let err = Error::config_error("msg", Some("key".to_string()));
    match err {
        Error::Configuration {
            message,
            config_key,
            ..
        } => {
            assert_eq!(message, "msg");
            assert_eq!(config_key, Some("key".to_string()));
        }
        _ => panic!("Wrong error type"),
    }
}

#[test]
fn test_error_source_chain() {
    // Test that errors can be chained (source is preserved)
    let err = Error::storage_error("test", "op", None);

    // The error should implement std::error::Error
    let _: &dyn std::error::Error = &err;
}

#[test]
fn test_invalid_transition_error() {
    let err = Error::InvalidTransition {
        from: "Bootstrap".to_string(),
        to: "Hierarchical".to_string(),
        reason: "Must go through Cell phase".to_string(),
    };

    let display = format!("{}", err);
    assert!(display.contains("Bootstrap"));
    assert!(display.contains("Hierarchical"));
    assert!(display.contains("Must go through Cell phase"));
}

#[test]
fn test_not_found_error() {
    let err = Error::NotFound {
        resource_type: "Platform".to_string(),
        id: "uav_001".to_string(),
    };

    let display = format!("{}", err);
    assert!(display.contains("Platform"));
    assert!(display.contains("uav_001"));
}

#[test]
fn test_serialization_error() {
    let json_err = serde_json::from_str::<serde_json::Value>("{invalid json")
        .expect_err("Should fail to parse");
    let err: Error = json_err.into();

    match err {
        Error::Serialization(_) => { /* Expected */ }
        _ => panic!("Should convert to Serialization error"),
    }
}
