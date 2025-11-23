//! Metrics and observability for topology management
//!
//! This module provides pluggable metrics collection for TopologyManager and
//! TopologyBuilder, enabling operators to monitor topology state, connection
//! health, failover events, and performance characteristics.
//!
//! # Architecture
//!
//! The metrics system follows the Ports & Adapters pattern (like BeaconStorage
//! and HierarchyStrategy), enabling integrators to choose their preferred
//! metrics backend or disable metrics entirely for resource-constrained devices.
//!
//! ```text
//! TopologyManager/Builder
//!         ↓
//!   MetricsCollector (Port)
//!         ↓
//!   ┌─────┴──────┐
//!   ↓            ↓
//! NoOpMetrics  InMemoryMetrics  (PrometheusMetrics future)
//! ```
//!
//! # Metric Categories
//!
//! ## Topology State Metrics
//! - Current parent ID
//! - Linked peer count (children)
//! - Lateral peer count (same-level peers)
//! - Current hierarchy level
//! - Current node role (Leader/Member/Standalone)
//!
//! ## Connection Health Metrics
//! - Parent connection state (connected/disconnected)
//! - Connection uptime
//! - Retry attempts
//! - Last successful connection timestamp
//!
//! ## Failover Metrics
//! - Parent switches (total count)
//! - Failover duration (time to recover)
//! - Retry attempts per failover
//! - Buffer usage during failover
//!
//! ## Performance Metrics
//! - Telemetry packets sent
//! - Telemetry packets buffered
//! - Buffer utilization (current/max)
//! - Topology evaluation duration
//! - Event processing latency
//!
//! # Example
//!
//! ```ignore
//! use hive_mesh::topology::{TopologyConfig, TopologyMetrics, InMemoryMetricsCollector};
//! use std::sync::Arc;
//!
//! // Create metrics collector
//! let metrics = Arc::new(InMemoryMetricsCollector::new());
//!
//! // Configure TopologyManager with metrics
//! let config = TopologyConfig {
//!     metrics_collector: Some(metrics.clone()),
//!     ..Default::default()
//! };
//!
//! // Later: query metrics for monitoring/debugging
//! let snapshot = metrics.snapshot();
//! println!("Parent switches: {}", snapshot.parent_switches);
//! println!("Buffer utilization: {}/{}", snapshot.buffer_current, snapshot.buffer_max);
//! ```

use crate::beacon::HierarchyLevel;
use crate::hierarchy::NodeRole;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Pluggable metrics collector trait (Port)
///
/// Enables integrators to implement custom metrics backends (Prometheus,
/// StatsD, CloudWatch, etc.) or use built-in implementations.
pub trait MetricsCollector: Send + Sync + std::fmt::Debug {
    // === Topology State Metrics ===

    /// Record current parent node ID
    fn set_parent_id(&self, parent_id: Option<String>);

    /// Record number of linked peers (children)
    fn set_linked_peer_count(&self, count: usize);

    /// Record number of lateral peers (same-level)
    fn set_lateral_peer_count(&self, count: usize);

    /// Record current hierarchy level
    fn set_hierarchy_level(&self, level: HierarchyLevel);

    /// Record current node role
    fn set_node_role(&self, role: NodeRole);

    // === Connection Health Metrics ===

    /// Record parent connection state change
    fn set_parent_connected(&self, connected: bool);

    /// Record connection uptime (only when connected)
    fn set_connection_uptime(&self, uptime: Duration);

    /// Increment retry attempts counter
    fn increment_retry_attempts(&self);

    /// Record last successful connection timestamp
    fn record_connection_success(&self, timestamp: Instant);

    // === Failover Metrics ===

    /// Increment parent switch counter
    fn increment_parent_switches(&self);

    /// Record failover duration (time from PeerLost to successful reconnection)
    fn record_failover_duration(&self, duration: Duration);

    /// Record retry attempts for a specific failover event
    fn record_failover_retry_attempts(&self, attempts: u32);

    /// Record buffer usage during failover
    fn record_failover_buffer_usage(&self, used: usize, max: usize);

    // === Performance Metrics ===

    /// Increment telemetry packets sent counter
    fn increment_telemetry_sent(&self);

    /// Increment telemetry packets buffered counter
    fn increment_telemetry_buffered(&self);

    /// Record current buffer utilization
    fn set_buffer_utilization(&self, current: usize, max: usize);

    /// Record topology evaluation duration
    fn record_evaluation_duration(&self, duration: Duration);

    /// Record event processing latency
    fn record_event_latency(&self, event_type: &str, latency: Duration);

    // === Event Counters ===

    /// Increment counter for specific topology event type
    fn increment_event_counter(&self, event_type: &str);
}

/// Snapshot of topology metrics for querying
///
/// Provides point-in-time view of all metrics for monitoring dashboards,
/// debugging, or testing.
#[derive(Debug, Clone, Default)]
pub struct TopologyMetricsSnapshot {
    // Topology State
    pub parent_id: Option<String>,
    pub linked_peer_count: usize,
    pub lateral_peer_count: usize,
    pub hierarchy_level: Option<HierarchyLevel>,
    pub node_role: Option<NodeRole>,

    // Connection Health
    pub parent_connected: bool,
    pub connection_uptime: Duration,
    pub retry_attempts_total: u64,
    pub last_connection_success: Option<Instant>,

    // Failover
    pub parent_switches_total: u64,
    pub last_failover_duration: Option<Duration>,
    pub last_failover_retry_attempts: Option<u32>,
    pub last_failover_buffer_usage: Option<(usize, usize)>, // (used, max)

    // Performance
    pub telemetry_sent_total: u64,
    pub telemetry_buffered_total: u64,
    pub buffer_current: usize,
    pub buffer_max: usize,
    pub last_evaluation_duration: Option<Duration>,

    // Event Counters (selected events)
    pub peer_selected_count: u64,
    pub peer_lost_count: u64,
    pub peer_changed_count: u64,
    pub role_changed_count: u64,
    pub level_changed_count: u64,
}

/// No-op metrics collector (Adapter)
///
/// Disables metrics collection for resource-constrained devices or when
/// metrics are not needed. Zero runtime overhead.
#[derive(Debug, Default)]
pub struct NoOpMetricsCollector;

impl NoOpMetricsCollector {
    /// Create a new no-op metrics collector
    pub fn new() -> Self {
        Self
    }

    /// Return an Arc-wrapped instance for easy sharing
    pub fn arc() -> Arc<dyn MetricsCollector> {
        Arc::new(Self::new())
    }
}

impl MetricsCollector for NoOpMetricsCollector {
    fn set_parent_id(&self, _parent_id: Option<String>) {}
    fn set_linked_peer_count(&self, _count: usize) {}
    fn set_lateral_peer_count(&self, _count: usize) {}
    fn set_hierarchy_level(&self, _level: HierarchyLevel) {}
    fn set_node_role(&self, _role: NodeRole) {}
    fn set_parent_connected(&self, _connected: bool) {}
    fn set_connection_uptime(&self, _uptime: Duration) {}
    fn increment_retry_attempts(&self) {}
    fn record_connection_success(&self, _timestamp: Instant) {}
    fn increment_parent_switches(&self) {}
    fn record_failover_duration(&self, _duration: Duration) {}
    fn record_failover_retry_attempts(&self, _attempts: u32) {}
    fn record_failover_buffer_usage(&self, _used: usize, _max: usize) {}
    fn increment_telemetry_sent(&self) {}
    fn increment_telemetry_buffered(&self) {}
    fn set_buffer_utilization(&self, _current: usize, _max: usize) {}
    fn record_evaluation_duration(&self, _duration: Duration) {}
    fn record_event_latency(&self, _event_type: &str, _latency: Duration) {}
    fn increment_event_counter(&self, _event_type: &str) {}
}

/// In-memory metrics collector (Adapter)
///
/// Stores metrics in thread-safe containers for testing, debugging, and
/// monitoring dashboards. Provides snapshot() method to query all metrics.
#[derive(Debug)]
pub struct InMemoryMetricsCollector {
    // Topology State
    parent_id: Arc<std::sync::RwLock<Option<String>>>,
    linked_peer_count: Arc<std::sync::RwLock<usize>>,
    lateral_peer_count: Arc<std::sync::RwLock<usize>>,
    hierarchy_level: Arc<std::sync::RwLock<Option<HierarchyLevel>>>,
    node_role: Arc<std::sync::RwLock<Option<NodeRole>>>,

    // Connection Health
    parent_connected: Arc<std::sync::RwLock<bool>>,
    connection_uptime: Arc<std::sync::RwLock<Duration>>,
    retry_attempts_total: Arc<std::sync::RwLock<u64>>,
    last_connection_success: Arc<std::sync::RwLock<Option<Instant>>>,

    // Failover
    parent_switches_total: Arc<std::sync::RwLock<u64>>,
    last_failover_duration: Arc<std::sync::RwLock<Option<Duration>>>,
    last_failover_retry_attempts: Arc<std::sync::RwLock<Option<u32>>>,
    last_failover_buffer_usage: Arc<std::sync::RwLock<Option<(usize, usize)>>>,

    // Performance
    telemetry_sent_total: Arc<std::sync::RwLock<u64>>,
    telemetry_buffered_total: Arc<std::sync::RwLock<u64>>,
    buffer_current: Arc<std::sync::RwLock<usize>>,
    buffer_max: Arc<std::sync::RwLock<usize>>,
    last_evaluation_duration: Arc<std::sync::RwLock<Option<Duration>>>,

    // Event Counters
    event_counters: Arc<std::sync::RwLock<std::collections::HashMap<String, u64>>>,
}

impl InMemoryMetricsCollector {
    /// Create a new in-memory metrics collector
    pub fn new() -> Self {
        Self {
            parent_id: Arc::new(std::sync::RwLock::new(None)),
            linked_peer_count: Arc::new(std::sync::RwLock::new(0)),
            lateral_peer_count: Arc::new(std::sync::RwLock::new(0)),
            hierarchy_level: Arc::new(std::sync::RwLock::new(None)),
            node_role: Arc::new(std::sync::RwLock::new(None)),
            parent_connected: Arc::new(std::sync::RwLock::new(false)),
            connection_uptime: Arc::new(std::sync::RwLock::new(Duration::ZERO)),
            retry_attempts_total: Arc::new(std::sync::RwLock::new(0)),
            last_connection_success: Arc::new(std::sync::RwLock::new(None)),
            parent_switches_total: Arc::new(std::sync::RwLock::new(0)),
            last_failover_duration: Arc::new(std::sync::RwLock::new(None)),
            last_failover_retry_attempts: Arc::new(std::sync::RwLock::new(None)),
            last_failover_buffer_usage: Arc::new(std::sync::RwLock::new(None)),
            telemetry_sent_total: Arc::new(std::sync::RwLock::new(0)),
            telemetry_buffered_total: Arc::new(std::sync::RwLock::new(0)),
            buffer_current: Arc::new(std::sync::RwLock::new(0)),
            buffer_max: Arc::new(std::sync::RwLock::new(0)),
            last_evaluation_duration: Arc::new(std::sync::RwLock::new(None)),
            event_counters: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Return an Arc-wrapped instance for easy sharing
    pub fn arc() -> Arc<dyn MetricsCollector> {
        Arc::new(Self::new())
    }

    /// Get a point-in-time snapshot of all metrics
    pub fn snapshot(&self) -> TopologyMetricsSnapshot {
        TopologyMetricsSnapshot {
            // Topology State
            parent_id: self.parent_id.read().unwrap().clone(),
            linked_peer_count: *self.linked_peer_count.read().unwrap(),
            lateral_peer_count: *self.lateral_peer_count.read().unwrap(),
            hierarchy_level: *self.hierarchy_level.read().unwrap(),
            node_role: *self.node_role.read().unwrap(),

            // Connection Health
            parent_connected: *self.parent_connected.read().unwrap(),
            connection_uptime: *self.connection_uptime.read().unwrap(),
            retry_attempts_total: *self.retry_attempts_total.read().unwrap(),
            last_connection_success: *self.last_connection_success.read().unwrap(),

            // Failover
            parent_switches_total: *self.parent_switches_total.read().unwrap(),
            last_failover_duration: *self.last_failover_duration.read().unwrap(),
            last_failover_retry_attempts: *self.last_failover_retry_attempts.read().unwrap(),
            last_failover_buffer_usage: *self.last_failover_buffer_usage.read().unwrap(),

            // Performance
            telemetry_sent_total: *self.telemetry_sent_total.read().unwrap(),
            telemetry_buffered_total: *self.telemetry_buffered_total.read().unwrap(),
            buffer_current: *self.buffer_current.read().unwrap(),
            buffer_max: *self.buffer_max.read().unwrap(),
            last_evaluation_duration: *self.last_evaluation_duration.read().unwrap(),

            // Event Counters
            peer_selected_count: self
                .event_counters
                .read()
                .unwrap()
                .get("PeerSelected")
                .copied()
                .unwrap_or(0),
            peer_lost_count: self
                .event_counters
                .read()
                .unwrap()
                .get("PeerLost")
                .copied()
                .unwrap_or(0),
            peer_changed_count: self
                .event_counters
                .read()
                .unwrap()
                .get("PeerChanged")
                .copied()
                .unwrap_or(0),
            role_changed_count: self
                .event_counters
                .read()
                .unwrap()
                .get("RoleChanged")
                .copied()
                .unwrap_or(0),
            level_changed_count: self
                .event_counters
                .read()
                .unwrap()
                .get("LevelChanged")
                .copied()
                .unwrap_or(0),
        }
    }
}

impl Default for InMemoryMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector for InMemoryMetricsCollector {
    fn set_parent_id(&self, parent_id: Option<String>) {
        *self.parent_id.write().unwrap() = parent_id;
    }

    fn set_linked_peer_count(&self, count: usize) {
        *self.linked_peer_count.write().unwrap() = count;
    }

    fn set_lateral_peer_count(&self, count: usize) {
        *self.lateral_peer_count.write().unwrap() = count;
    }

    fn set_hierarchy_level(&self, level: HierarchyLevel) {
        *self.hierarchy_level.write().unwrap() = Some(level);
    }

    fn set_node_role(&self, role: NodeRole) {
        *self.node_role.write().unwrap() = Some(role);
    }

    fn set_parent_connected(&self, connected: bool) {
        *self.parent_connected.write().unwrap() = connected;
    }

    fn set_connection_uptime(&self, uptime: Duration) {
        *self.connection_uptime.write().unwrap() = uptime;
    }

    fn increment_retry_attempts(&self) {
        *self.retry_attempts_total.write().unwrap() += 1;
    }

    fn record_connection_success(&self, timestamp: Instant) {
        *self.last_connection_success.write().unwrap() = Some(timestamp);
    }

    fn increment_parent_switches(&self) {
        *self.parent_switches_total.write().unwrap() += 1;
    }

    fn record_failover_duration(&self, duration: Duration) {
        *self.last_failover_duration.write().unwrap() = Some(duration);
    }

    fn record_failover_retry_attempts(&self, attempts: u32) {
        *self.last_failover_retry_attempts.write().unwrap() = Some(attempts);
    }

    fn record_failover_buffer_usage(&self, used: usize, max: usize) {
        *self.last_failover_buffer_usage.write().unwrap() = Some((used, max));
    }

    fn increment_telemetry_sent(&self) {
        *self.telemetry_sent_total.write().unwrap() += 1;
    }

    fn increment_telemetry_buffered(&self) {
        *self.telemetry_buffered_total.write().unwrap() += 1;
    }

    fn set_buffer_utilization(&self, current: usize, max: usize) {
        *self.buffer_current.write().unwrap() = current;
        *self.buffer_max.write().unwrap() = max;
    }

    fn record_evaluation_duration(&self, duration: Duration) {
        *self.last_evaluation_duration.write().unwrap() = Some(duration);
    }

    fn record_event_latency(&self, _event_type: &str, _latency: Duration) {
        // TODO: Could track per-event latencies in future
        // For now, only track event counts
    }

    fn increment_event_counter(&self, event_type: &str) {
        self.event_counters
            .write()
            .unwrap()
            .entry(event_type.to_string())
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_collector_creation() {
        let collector = NoOpMetricsCollector::new();
        assert!(std::mem::size_of_val(&collector) == 0); // ZST

        let arc_collector = NoOpMetricsCollector::arc();
        assert!(Arc::strong_count(&arc_collector) == 1);
    }

    #[test]
    fn test_noop_collector_methods() {
        let collector = NoOpMetricsCollector::new();

        // All methods should be no-op (no panics)
        collector.set_parent_id(Some("parent-1".to_string()));
        collector.set_linked_peer_count(5);
        collector.set_lateral_peer_count(3);
        collector.set_hierarchy_level(HierarchyLevel::Squad);
        collector.set_node_role(NodeRole::Leader);
        collector.set_parent_connected(true);
        collector.set_connection_uptime(Duration::from_secs(10));
        collector.increment_retry_attempts();
        collector.record_connection_success(Instant::now());
        collector.increment_parent_switches();
        collector.record_failover_duration(Duration::from_secs(5));
        collector.record_failover_retry_attempts(3);
        collector.record_failover_buffer_usage(10, 100);
        collector.increment_telemetry_sent();
        collector.increment_telemetry_buffered();
        collector.set_buffer_utilization(5, 100);
        collector.record_evaluation_duration(Duration::from_millis(100));
        collector.record_event_latency("PeerSelected", Duration::from_millis(50));
        collector.increment_event_counter("PeerLost");
    }

    #[test]
    fn test_metrics_snapshot_default() {
        let snapshot = TopologyMetricsSnapshot::default();

        assert_eq!(snapshot.parent_id, None);
        assert_eq!(snapshot.linked_peer_count, 0);
        assert_eq!(snapshot.lateral_peer_count, 0);
        assert_eq!(snapshot.hierarchy_level, None);
        assert_eq!(snapshot.node_role, None);
        assert!(!snapshot.parent_connected);
        assert_eq!(snapshot.connection_uptime, Duration::ZERO);
        assert_eq!(snapshot.retry_attempts_total, 0);
        assert_eq!(snapshot.last_connection_success, None);
        assert_eq!(snapshot.parent_switches_total, 0);
        assert_eq!(snapshot.last_failover_duration, None);
        assert_eq!(snapshot.last_failover_retry_attempts, None);
        assert_eq!(snapshot.last_failover_buffer_usage, None);
        assert_eq!(snapshot.telemetry_sent_total, 0);
        assert_eq!(snapshot.telemetry_buffered_total, 0);
        assert_eq!(snapshot.buffer_current, 0);
        assert_eq!(snapshot.buffer_max, 0);
        assert_eq!(snapshot.last_evaluation_duration, None);
        assert_eq!(snapshot.peer_selected_count, 0);
        assert_eq!(snapshot.peer_lost_count, 0);
        assert_eq!(snapshot.peer_changed_count, 0);
        assert_eq!(snapshot.role_changed_count, 0);
        assert_eq!(snapshot.level_changed_count, 0);
    }

    #[test]
    fn test_inmemory_collector_creation() {
        let collector = InMemoryMetricsCollector::new();

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.parent_id, None);
        assert_eq!(snapshot.linked_peer_count, 0);
        assert!(!snapshot.parent_connected);
        assert_eq!(snapshot.retry_attempts_total, 0);

        let arc_collector = InMemoryMetricsCollector::arc();
        assert!(Arc::strong_count(&arc_collector) == 1);
    }

    #[test]
    fn test_inmemory_topology_state_metrics() {
        let collector = InMemoryMetricsCollector::new();

        // Set topology state
        collector.set_parent_id(Some("parent-1".to_string()));
        collector.set_linked_peer_count(5);
        collector.set_lateral_peer_count(3);
        collector.set_hierarchy_level(HierarchyLevel::Squad);
        collector.set_node_role(NodeRole::Leader);

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.parent_id, Some("parent-1".to_string()));
        assert_eq!(snapshot.linked_peer_count, 5);
        assert_eq!(snapshot.lateral_peer_count, 3);
        assert_eq!(snapshot.hierarchy_level, Some(HierarchyLevel::Squad));
        assert_eq!(snapshot.node_role, Some(NodeRole::Leader));
    }

    #[test]
    fn test_inmemory_connection_health_metrics() {
        let collector = InMemoryMetricsCollector::new();

        collector.set_parent_connected(true);
        collector.set_connection_uptime(Duration::from_secs(120));
        collector.increment_retry_attempts();
        collector.increment_retry_attempts();
        collector.increment_retry_attempts();

        let now = Instant::now();
        collector.record_connection_success(now);

        let snapshot = collector.snapshot();
        assert!(snapshot.parent_connected);
        assert_eq!(snapshot.connection_uptime, Duration::from_secs(120));
        assert_eq!(snapshot.retry_attempts_total, 3);
        assert_eq!(snapshot.last_connection_success, Some(now));
    }

    #[test]
    fn test_inmemory_failover_metrics() {
        let collector = InMemoryMetricsCollector::new();

        collector.increment_parent_switches();
        collector.increment_parent_switches();
        collector.record_failover_duration(Duration::from_secs(5));
        collector.record_failover_retry_attempts(3);
        collector.record_failover_buffer_usage(10, 100);

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.parent_switches_total, 2);
        assert_eq!(
            snapshot.last_failover_duration,
            Some(Duration::from_secs(5))
        );
        assert_eq!(snapshot.last_failover_retry_attempts, Some(3));
        assert_eq!(snapshot.last_failover_buffer_usage, Some((10, 100)));
    }

    #[test]
    fn test_inmemory_performance_metrics() {
        let collector = InMemoryMetricsCollector::new();

        collector.increment_telemetry_sent();
        collector.increment_telemetry_sent();
        collector.increment_telemetry_sent();
        collector.increment_telemetry_buffered();
        collector.increment_telemetry_buffered();
        collector.set_buffer_utilization(5, 100);
        collector.record_evaluation_duration(Duration::from_millis(150));

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.telemetry_sent_total, 3);
        assert_eq!(snapshot.telemetry_buffered_total, 2);
        assert_eq!(snapshot.buffer_current, 5);
        assert_eq!(snapshot.buffer_max, 100);
        assert_eq!(
            snapshot.last_evaluation_duration,
            Some(Duration::from_millis(150))
        );
    }

    #[test]
    fn test_inmemory_event_counters() {
        let collector = InMemoryMetricsCollector::new();

        // Increment various event counters
        collector.increment_event_counter("PeerSelected");
        collector.increment_event_counter("PeerSelected");
        collector.increment_event_counter("PeerLost");
        collector.increment_event_counter("PeerChanged");
        collector.increment_event_counter("RoleChanged");
        collector.increment_event_counter("RoleChanged");
        collector.increment_event_counter("RoleChanged");
        collector.increment_event_counter("LevelChanged");

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.peer_selected_count, 2);
        assert_eq!(snapshot.peer_lost_count, 1);
        assert_eq!(snapshot.peer_changed_count, 1);
        assert_eq!(snapshot.role_changed_count, 3);
        assert_eq!(snapshot.level_changed_count, 1);
    }

    #[test]
    fn test_inmemory_metrics_updates() {
        let collector = InMemoryMetricsCollector::new();

        // Initial state
        collector.set_parent_id(Some("parent-1".to_string()));
        collector.set_linked_peer_count(5);

        let snapshot1 = collector.snapshot();
        assert_eq!(snapshot1.parent_id, Some("parent-1".to_string()));
        assert_eq!(snapshot1.linked_peer_count, 5);

        // Update state
        collector.set_parent_id(Some("parent-2".to_string()));
        collector.set_linked_peer_count(8);

        let snapshot2 = collector.snapshot();
        assert_eq!(snapshot2.parent_id, Some("parent-2".to_string()));
        assert_eq!(snapshot2.linked_peer_count, 8);
    }

    #[test]
    fn test_inmemory_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let collector = Arc::new(InMemoryMetricsCollector::new());
        let mut handles = vec![];

        // Spawn multiple threads incrementing counters
        for _ in 0..10 {
            let collector_clone = collector.clone();
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    collector_clone.increment_telemetry_sent();
                    collector_clone.increment_retry_attempts();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.telemetry_sent_total, 1000); // 10 threads * 100 increments
        assert_eq!(snapshot.retry_attempts_total, 1000);
    }
}
