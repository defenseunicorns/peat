//! TAK transport metrics

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Instant;

/// TAK transport metrics
#[derive(Debug, Default)]
pub struct TakMetrics {
    /// Total connections established
    pub connections: AtomicU64,

    /// Total messages sent
    pub messages_sent: AtomicU64,

    /// Total messages received
    pub messages_received: AtomicU64,

    /// Total bytes sent
    pub bytes_sent: AtomicU64,

    /// Total bytes received
    pub bytes_received: AtomicU64,

    /// Messages dropped (queue full)
    pub messages_dropped: AtomicU64,

    /// Reconnection attempts
    pub reconnect_attempts: AtomicU64,

    /// Last error message
    pub last_error: RwLock<Option<String>>,

    /// Connection uptime start
    pub connected_since: RwLock<Option<Instant>>,
}

impl TakMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful connection
    pub fn record_connect(&self) {
        self.connections.fetch_add(1, Ordering::Relaxed);
        *self
            .connected_since
            .write()
            .expect("connected_since lock poisoned") = Some(Instant::now());
    }

    /// Record a disconnection
    pub fn record_disconnect(&self) {
        *self
            .connected_since
            .write()
            .expect("connected_since lock poisoned") = None;
    }

    /// Record a sent message
    pub fn record_send(&self, bytes: usize) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Record a received message
    pub fn record_receive(&self, bytes: usize) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Record a dropped message
    pub fn record_drop(&self) {
        self.messages_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a reconnection attempt
    pub fn record_reconnect_attempt(&self) {
        self.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an error
    pub fn record_error(&self, error: &str) {
        *self.last_error.write().expect("last_error lock poisoned") = Some(error.to_string());
    }

    /// Get connection uptime in seconds
    pub fn uptime_secs(&self) -> Option<u64> {
        self.connected_since
            .read()
            .expect("connected_since lock poisoned")
            .map(|since| since.elapsed().as_secs())
    }

    /// Create a snapshot of current metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            connections: self.connections.load(Ordering::Relaxed),
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            messages_dropped: self.messages_dropped.load(Ordering::Relaxed),
            reconnect_attempts: self.reconnect_attempts.load(Ordering::Relaxed),
            last_error: self
                .last_error
                .read()
                .expect("last_error lock poisoned")
                .clone(),
            uptime_secs: self.uptime_secs(),
        }
    }
}

impl Clone for TakMetrics {
    fn clone(&self) -> Self {
        Self {
            connections: AtomicU64::new(self.connections.load(Ordering::Relaxed)),
            messages_sent: AtomicU64::new(self.messages_sent.load(Ordering::Relaxed)),
            messages_received: AtomicU64::new(self.messages_received.load(Ordering::Relaxed)),
            bytes_sent: AtomicU64::new(self.bytes_sent.load(Ordering::Relaxed)),
            bytes_received: AtomicU64::new(self.bytes_received.load(Ordering::Relaxed)),
            messages_dropped: AtomicU64::new(self.messages_dropped.load(Ordering::Relaxed)),
            reconnect_attempts: AtomicU64::new(self.reconnect_attempts.load(Ordering::Relaxed)),
            last_error: RwLock::new(
                self.last_error
                    .read()
                    .expect("last_error lock poisoned")
                    .clone(),
            ),
            connected_since: RwLock::new(
                *self
                    .connected_since
                    .read()
                    .expect("connected_since lock poisoned"),
            ),
        }
    }
}

/// Snapshot of metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub connections: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub messages_dropped: u64,
    pub reconnect_attempts: u64,
    pub last_error: Option<String>,
    pub uptime_secs: Option<u64>,
}

/// Queue depth metrics
#[derive(Debug, Clone, Default)]
pub struct QueueDepthMetrics {
    /// P1 (Critical) queue depth
    pub p1_depth: usize,
    /// P2 (High) queue depth
    pub p2_depth: usize,
    /// P3 (Normal) queue depth
    pub p3_depth: usize,
    /// P4 (Low) queue depth
    pub p4_depth: usize,
    /// P5 (Bulk) queue depth
    pub p5_depth: usize,
    /// Total bytes queued
    pub total_bytes: usize,
    /// Stale messages dropped
    pub stale_dropped: u64,
}

impl QueueDepthMetrics {
    /// Get total message count across all priorities
    pub fn total_messages(&self) -> usize {
        self.p1_depth + self.p2_depth + self.p3_depth + self.p4_depth + self.p5_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_record_send() {
        let metrics = TakMetrics::new();
        metrics.record_send(100);
        metrics.record_send(200);

        assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.bytes_sent.load(Ordering::Relaxed), 300);
    }

    #[test]
    fn test_metrics_snapshot() {
        let metrics = TakMetrics::new();
        metrics.record_connect();
        metrics.record_send(100);
        metrics.record_error("test error");

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.connections, 1);
        assert_eq!(snapshot.messages_sent, 1);
        assert_eq!(snapshot.last_error, Some("test error".to_string()));
        assert!(snapshot.uptime_secs.is_some());
    }

    #[test]
    fn test_queue_depth_total() {
        let metrics = QueueDepthMetrics {
            p1_depth: 10,
            p2_depth: 20,
            p3_depth: 30,
            p4_depth: 15,
            p5_depth: 5,
            total_bytes: 1000,
            stale_dropped: 2,
        };

        assert_eq!(metrics.total_messages(), 80);
    }
}
