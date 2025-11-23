//! Integration tests for metrics and observability
//!
//! These tests demonstrate how the metrics system works with TopologyBuilder
//! to provide insights into topology formation, connection health, and performance.

use hive_mesh::beacon::HierarchyLevel;
use hive_mesh::topology::{
    InMemoryMetricsCollector, MetricsCollector, NoOpMetricsCollector, TopologyConfig,
};
use std::time::Duration;

#[test]
fn test_noop_metrics_collector_has_zero_overhead() {
    // NoOpMetricsCollector is a zero-sized type (ZST)
    // All methods are no-ops, resulting in zero runtime overhead
    let metrics = NoOpMetricsCollector;

    // These calls compile to nothing (optimized away)
    metrics.set_parent_id(Some("parent-1".to_string()));
    metrics.set_linked_peer_count(5);
    metrics.increment_event_counter("test");

    // Verify it's a ZST
    assert_eq!(std::mem::size_of_val(&metrics), 0);
}

#[test]
fn test_in_memory_metrics_collector_captures_topology_state() {
    // InMemoryMetricsCollector stores metrics in memory for testing/monitoring
    let metrics = InMemoryMetricsCollector::new();

    // Simulate topology state changes
    metrics.set_parent_id(Some("parent-node-1".to_string()));
    metrics.set_linked_peer_count(3);
    metrics.set_lateral_peer_count(2);
    metrics.set_hierarchy_level(HierarchyLevel::Squad);
    metrics.set_parent_connected(true);

    // Take a snapshot of metrics
    let snapshot = metrics.snapshot();

    // Verify topology state metrics
    assert_eq!(snapshot.parent_id, Some("parent-node-1".to_string()));
    assert_eq!(snapshot.linked_peer_count, 3);
    assert_eq!(snapshot.lateral_peer_count, 2);
    assert_eq!(snapshot.hierarchy_level, Some(HierarchyLevel::Squad));
    assert!(snapshot.parent_connected);
}

#[test]
fn test_metrics_collector_records_connection_health() {
    let metrics = InMemoryMetricsCollector::new();

    // Simulate connection lifecycle
    metrics.set_parent_connected(false); // Initially disconnected
    metrics.increment_retry_attempts(); // First retry
    metrics.increment_retry_attempts(); // Second retry
    metrics.increment_retry_attempts(); // Third retry

    let start = std::time::Instant::now();
    std::thread::sleep(Duration::from_millis(10));
    metrics.record_connection_success(start); // Finally connected
    metrics.set_parent_connected(true);

    let snapshot = metrics.snapshot();

    // Verify connection health metrics
    assert!(snapshot.parent_connected);
    assert_eq!(snapshot.retry_attempts_total, 3);
    assert!(snapshot.last_connection_success.is_some());
}

#[test]
fn test_metrics_collector_records_failover_events() {
    let metrics = InMemoryMetricsCollector::new();

    // Simulate failover scenario
    metrics.set_parent_id(Some("parent-1".to_string()));
    metrics.set_parent_connected(true);

    // Parent fails
    metrics.set_parent_connected(false);
    metrics.increment_parent_switches();

    // Buffer telemetry during failover
    metrics.record_failover_buffer_usage(5, 100); // 5 packets buffered out of 100 max

    // Failover completes after 3 retry attempts
    metrics.record_failover_retry_attempts(3);
    metrics.record_failover_duration(Duration::from_secs(7));

    // New parent selected
    metrics.set_parent_id(Some("parent-2".to_string()));
    metrics.set_parent_connected(true);

    let snapshot = metrics.snapshot();

    // Verify failover metrics
    assert_eq!(snapshot.parent_switches_total, 1);
    assert_eq!(snapshot.last_failover_retry_attempts, Some(3));
    assert_eq!(
        snapshot.last_failover_duration,
        Some(Duration::from_secs(7))
    );
    assert_eq!(snapshot.last_failover_buffer_usage, Some((5, 100)));
    assert_eq!(snapshot.parent_id, Some("parent-2".to_string()));
}

#[test]
fn test_metrics_collector_records_telemetry_performance() {
    let metrics = InMemoryMetricsCollector::new();

    // Simulate telemetry sending
    metrics.increment_telemetry_sent(); // Sent immediately
    metrics.increment_telemetry_sent();
    metrics.increment_telemetry_sent();

    // Parent disconnects, start buffering
    metrics.increment_telemetry_buffered();
    metrics.increment_telemetry_buffered();
    metrics.set_buffer_utilization(2, 100);

    let snapshot = metrics.snapshot();

    // Verify telemetry metrics
    assert_eq!(snapshot.telemetry_sent_total, 3);
    assert_eq!(snapshot.telemetry_buffered_total, 2);
    assert_eq!(snapshot.buffer_current, 2);
    assert_eq!(snapshot.buffer_max, 100);
}

#[test]
fn test_topology_config_with_metrics_collector() {
    // Create an InMemoryMetricsCollector
    let metrics = std::sync::Arc::new(InMemoryMetricsCollector::new());

    // Create TopologyConfig with metrics collector
    let config = TopologyConfig {
        metrics_collector: Some(metrics.clone()),
        ..Default::default()
    };

    // Verify metrics collector is configured
    assert!(config.metrics_collector.is_some());

    // Future work: TopologyBuilder evaluation loop would call metrics methods
    // e.g., metrics.set_hierarchy_level(HierarchyLevel::Squad)
    // e.g., metrics.set_linked_peer_count(state.linked_peers.len())
}

#[test]
fn test_topology_config_without_metrics_collector() {
    // Default configuration has no metrics collector
    let config = TopologyConfig::default();
    assert!(config.metrics_collector.is_none());
}

#[test]
fn test_metrics_snapshot_includes_all_fields() {
    let metrics = InMemoryMetricsCollector::new();

    // Set all metrics to non-default values
    metrics.set_parent_id(Some("parent-1".to_string()));
    metrics.set_linked_peer_count(10);
    metrics.set_lateral_peer_count(5);
    metrics.set_hierarchy_level(HierarchyLevel::Platoon);
    metrics.set_parent_connected(true);
    metrics.set_connection_uptime(Duration::from_secs(3600));
    metrics.increment_retry_attempts();
    metrics.record_connection_success(std::time::Instant::now());
    metrics.increment_parent_switches();
    metrics.record_failover_duration(Duration::from_secs(10));
    metrics.record_failover_retry_attempts(2);
    metrics.record_failover_buffer_usage(15, 100);
    metrics.increment_telemetry_sent();
    metrics.increment_telemetry_buffered();
    metrics.set_buffer_utilization(5, 100);
    metrics.record_evaluation_duration(Duration::from_millis(50));

    let snapshot = metrics.snapshot();

    // Verify all fields are captured in snapshot
    assert!(snapshot.parent_id.is_some());
    assert_eq!(snapshot.linked_peer_count, 10);
    assert_eq!(snapshot.lateral_peer_count, 5);
    assert_eq!(snapshot.hierarchy_level, Some(HierarchyLevel::Platoon));
    assert!(snapshot.parent_connected);
    assert_eq!(snapshot.connection_uptime, Duration::from_secs(3600));
    assert_eq!(snapshot.retry_attempts_total, 1);
    assert!(snapshot.last_connection_success.is_some());
    assert_eq!(snapshot.parent_switches_total, 1);
    assert_eq!(
        snapshot.last_failover_duration,
        Some(Duration::from_secs(10))
    );
    assert_eq!(snapshot.last_failover_retry_attempts, Some(2));
    assert_eq!(snapshot.last_failover_buffer_usage, Some((15, 100)));
    assert_eq!(snapshot.telemetry_sent_total, 1);
    assert_eq!(snapshot.telemetry_buffered_total, 1);
    assert_eq!(snapshot.buffer_current, 5);
    assert_eq!(snapshot.buffer_max, 100);
    assert_eq!(
        snapshot.last_evaluation_duration,
        Some(Duration::from_millis(50))
    );
}
