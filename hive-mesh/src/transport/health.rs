//! Connection health monitoring with heartbeat mechanism
//!
//! This module provides proactive health monitoring for mesh connections using
//! periodic heartbeats. It detects degraded or failing connections before they timeout,
//! enabling graceful failover and better user experience.
//!
//! ## Architecture
//!
//! The `HealthMonitor` runs as part of the transport's background task and:
//! 1. Sends periodic heartbeat pings to connected peers
//! 2. Measures round-trip time (RTT) from ping/pong
//! 3. Tracks missed heartbeats to detect connection failures
//! 4. Emits `PeerEvent::Degraded` when connections become unhealthy
//!
//! ## State Machine
//!
//! ```text
//! ┌─────────┐     high RTT      ┌──────────┐
//! │ Healthy ├──────────────────►│ Degraded │
//! └────┬────┘                   └────┬─────┘
//!      │                              │
//!      │ missed heartbeats           │ missed heartbeats
//!      ▼                              ▼
//! ┌─────────┐     more misses   ┌──────────┐
//! │ Suspect ├──────────────────►│   Dead   │
//! └─────────┘                   └──────────┘
//! ```

use super::{ConnectionHealth, ConnectionState, NodeId, PeerEvent, PeerEventSender};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, trace, warn};

/// Configuration for the heartbeat mechanism
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Interval between heartbeat pings
    pub interval: Duration,
    /// Number of missed heartbeats before marking connection as Suspect
    pub suspect_threshold: u32,
    /// Number of missed heartbeats before marking connection as Dead
    pub dead_threshold: u32,
    /// RTT threshold in milliseconds for Degraded state
    pub degraded_rtt_threshold_ms: u32,
    /// Packet loss percentage threshold for Degraded state (0-100)
    pub degraded_loss_threshold_percent: u8,
    /// Whether heartbeat monitoring is enabled
    pub enabled: bool,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            suspect_threshold: 2,                // 10s with default interval
            dead_threshold: 4,                   // 20s with default interval
            degraded_rtt_threshold_ms: 1000,     // 1 second
            degraded_loss_threshold_percent: 10, // 10% packet loss
            enabled: true,
        }
    }
}

impl HeartbeatConfig {
    /// Create a config with heartbeat disabled
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Create a config for aggressive monitoring (shorter intervals)
    pub fn aggressive() -> Self {
        Self {
            interval: Duration::from_secs(2),
            suspect_threshold: 2,
            dead_threshold: 3,
            degraded_rtt_threshold_ms: 500,
            degraded_loss_threshold_percent: 5,
            enabled: true,
        }
    }

    /// Create a config for relaxed monitoring (longer intervals)
    pub fn relaxed() -> Self {
        Self {
            interval: Duration::from_secs(15),
            suspect_threshold: 3,
            dead_threshold: 6,
            degraded_rtt_threshold_ms: 2000,
            degraded_loss_threshold_percent: 20,
            enabled: true,
        }
    }
}

/// Internal state for tracking a peer's health
#[derive(Debug, Clone)]
struct PeerHealthState {
    /// Last time we received a response from this peer
    last_response: Instant,
    /// Number of consecutive missed heartbeats
    missed_heartbeats: u32,
    /// Smoothed RTT in milliseconds (EWMA with alpha=0.125)
    smoothed_rtt_ms: f64,
    /// RTT variance in milliseconds
    rtt_variance_ms: f64,
    /// Total pings sent
    pings_sent: u64,
    /// Total pongs received
    pongs_received: u64,
    /// Current state
    state: ConnectionState,
    /// When monitoring started for this peer (for future uptime tracking)
    _monitoring_started: Instant,
    /// Pending ping (sent timestamp, sequence number)
    pending_ping: Option<(Instant, u64)>,
    /// Next sequence number
    next_seq: u64,
}

impl PeerHealthState {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            last_response: now,
            missed_heartbeats: 0,
            smoothed_rtt_ms: 0.0,
            rtt_variance_ms: 0.0,
            pings_sent: 0,
            pongs_received: 0,
            state: ConnectionState::Healthy,
            _monitoring_started: now,
            pending_ping: None,
            next_seq: 0,
        }
    }

    /// Calculate packet loss percentage
    fn packet_loss_percent(&self) -> u8 {
        if self.pings_sent == 0 {
            return 0;
        }
        let loss_ratio = 1.0 - (self.pongs_received as f64 / self.pings_sent as f64);
        (loss_ratio * 100.0).round().min(100.0) as u8
    }

    /// Convert to ConnectionHealth for external use
    fn to_connection_health(&self) -> ConnectionHealth {
        ConnectionHealth {
            rtt_ms: self.smoothed_rtt_ms.round() as u32,
            rtt_variance_ms: self.rtt_variance_ms.round() as u32,
            packet_loss_percent: self.packet_loss_percent(),
            state: self.state,
            last_activity: self.last_response,
        }
    }

    /// Update RTT using Jacobson/Karels algorithm (like TCP)
    fn update_rtt(&mut self, sample_rtt_ms: f64) {
        const ALPHA: f64 = 0.125; // 1/8
        const BETA: f64 = 0.25; // 1/4

        if self.smoothed_rtt_ms == 0.0 {
            // First sample
            self.smoothed_rtt_ms = sample_rtt_ms;
            self.rtt_variance_ms = sample_rtt_ms / 2.0;
        } else {
            // EWMA update
            let diff = (sample_rtt_ms - self.smoothed_rtt_ms).abs();
            self.rtt_variance_ms = (1.0 - BETA) * self.rtt_variance_ms + BETA * diff;
            self.smoothed_rtt_ms = (1.0 - ALPHA) * self.smoothed_rtt_ms + ALPHA * sample_rtt_ms;
        }
    }
}

/// Health monitor for mesh connections
///
/// Tracks connection health using periodic heartbeats and emits events
/// when connections become degraded or fail.
pub struct HealthMonitor {
    /// Configuration
    config: HeartbeatConfig,
    /// Health state by peer
    health: Arc<RwLock<HashMap<NodeId, PeerHealthState>>>,
    /// Event sender for degradation/failure events
    event_senders: Arc<RwLock<Vec<PeerEventSender>>>,
}

impl HealthMonitor {
    /// Create a new health monitor with the given configuration
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            config,
            health: Arc::new(RwLock::new(HashMap::new())),
            event_senders: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create a health monitor with default configuration
    pub fn with_default_config() -> Self {
        Self::new(HeartbeatConfig::default())
    }

    /// Get the heartbeat configuration
    pub fn config(&self) -> &HeartbeatConfig {
        &self.config
    }

    /// Check if monitoring is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Start monitoring a peer
    ///
    /// Should be called when a new connection is established.
    pub fn start_monitoring(&self, peer_id: NodeId) {
        if !self.config.enabled {
            return;
        }

        let mut health = self.health.write().unwrap();
        health
            .entry(peer_id.clone())
            .or_insert_with(PeerHealthState::new);
        trace!("Started health monitoring for peer: {}", peer_id);
    }

    /// Stop monitoring a peer
    ///
    /// Should be called when a connection is closed.
    pub fn stop_monitoring(&self, peer_id: &NodeId) {
        let mut health = self.health.write().unwrap();
        if health.remove(peer_id).is_some() {
            trace!("Stopped health monitoring for peer: {}", peer_id);
        }
    }

    /// Get health status for a peer
    pub fn get_health(&self, peer_id: &NodeId) -> Option<ConnectionHealth> {
        let health = self.health.read().unwrap();
        health.get(peer_id).map(|s| s.to_connection_health())
    }

    /// Get all peers with unhealthy connections (Degraded or worse)
    pub fn unhealthy_peers(&self) -> Vec<NodeId> {
        let health = self.health.read().unwrap();
        health
            .iter()
            .filter(|(_, state)| state.state != ConnectionState::Healthy)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get all peers marked as Dead
    pub fn dead_peers(&self) -> Vec<NodeId> {
        let health = self.health.read().unwrap();
        health
            .iter()
            .filter(|(_, state)| state.state == ConnectionState::Dead)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Subscribe to health events (degradation, failure)
    pub fn subscribe(&self, sender: PeerEventSender) {
        self.event_senders.write().unwrap().push(sender);
    }

    /// Get peers that need a heartbeat ping
    ///
    /// Returns peers that haven't been pinged recently.
    pub fn peers_needing_ping(&self) -> Vec<NodeId> {
        if !self.config.enabled {
            return Vec::new();
        }

        let health = self.health.read().unwrap();
        let now = Instant::now();

        health
            .iter()
            .filter(|(_, state)| {
                // Don't ping dead peers
                if state.state == ConnectionState::Dead {
                    return false;
                }
                // Check if we have a pending ping
                if let Some((sent_at, _)) = state.pending_ping {
                    // Don't send another ping if one is pending
                    sent_at.elapsed() > self.config.interval * 2
                } else {
                    // No pending ping, check if interval has passed
                    now.duration_since(state.last_response) >= self.config.interval
                }
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Record that a ping was sent to a peer
    ///
    /// Returns the sequence number for this ping.
    pub fn record_ping_sent(&self, peer_id: &NodeId) -> Option<u64> {
        let mut health = self.health.write().unwrap();
        if let Some(state) = health.get_mut(peer_id) {
            let seq = state.next_seq;
            state.next_seq += 1;
            state.pings_sent += 1;
            state.pending_ping = Some((Instant::now(), seq));
            Some(seq)
        } else {
            None
        }
    }

    /// Record that a pong was received from a peer
    ///
    /// Updates RTT measurements and resets missed heartbeat count.
    pub fn record_pong_received(&self, peer_id: &NodeId, seq: u64) {
        let mut health = self.health.write().unwrap();
        if let Some(state) = health.get_mut(peer_id) {
            // Check if this pong matches our pending ping
            if let Some((sent_at, expected_seq)) = state.pending_ping.take() {
                if seq == expected_seq {
                    let rtt_ms = sent_at.elapsed().as_secs_f64() * 1000.0;
                    state.update_rtt(rtt_ms);
                    state.pongs_received += 1;
                    state.last_response = Instant::now();
                    state.missed_heartbeats = 0;

                    // Check if we should transition to Healthy
                    let old_state = state.state;
                    state.state = self.evaluate_state(state);

                    if old_state != state.state && state.state == ConnectionState::Healthy {
                        trace!("Peer {} recovered to Healthy state", peer_id);
                    }
                }
            }
        }
    }

    /// Check for timeouts and update connection states
    ///
    /// Should be called periodically (e.g., every heartbeat interval).
    /// Returns list of peers that transitioned to Dead state.
    pub fn check_timeouts(&self) -> Vec<NodeId> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut health = self.health.write().unwrap();
        let mut newly_dead = Vec::new();
        let mut degraded_events = Vec::new();

        for (peer_id, state) in health.iter_mut() {
            // Check for pending ping timeout
            if let Some((sent_at, _)) = state.pending_ping {
                if sent_at.elapsed() >= self.config.interval {
                    // Ping timed out
                    state.missed_heartbeats += 1;
                    state.pending_ping = None; // Clear pending so we can send again

                    trace!(
                        "Peer {} missed heartbeat #{}",
                        peer_id,
                        state.missed_heartbeats
                    );
                }
            }

            // Evaluate state
            let old_state = state.state;
            state.state = self.evaluate_state(state);

            // Handle state transitions
            if old_state != state.state {
                match state.state {
                    ConnectionState::Degraded => {
                        warn!(
                            "Peer {} degraded: RTT={}ms, loss={}%",
                            peer_id,
                            state.smoothed_rtt_ms.round() as u32,
                            state.packet_loss_percent()
                        );
                        degraded_events.push((peer_id.clone(), state.to_connection_health()));
                    }
                    ConnectionState::Suspect => {
                        warn!(
                            "Peer {} suspected dead: {} missed heartbeats",
                            peer_id, state.missed_heartbeats
                        );
                    }
                    ConnectionState::Dead => {
                        warn!(
                            "Peer {} confirmed dead: {} missed heartbeats",
                            peer_id, state.missed_heartbeats
                        );
                        newly_dead.push(peer_id.clone());
                    }
                    ConnectionState::Healthy => {
                        debug!("Peer {} recovered to healthy state", peer_id);
                    }
                }
            }
        }

        drop(health);

        // Emit degraded events
        for (peer_id, health) in degraded_events {
            self.emit_event(PeerEvent::Degraded { peer_id, health });
        }

        newly_dead
    }

    /// Evaluate what state a peer should be in based on current metrics
    fn evaluate_state(&self, state: &PeerHealthState) -> ConnectionState {
        // Check for dead first (highest priority)
        if state.missed_heartbeats >= self.config.dead_threshold {
            return ConnectionState::Dead;
        }

        // Check for suspect
        if state.missed_heartbeats >= self.config.suspect_threshold {
            return ConnectionState::Suspect;
        }

        // Check for degraded based on RTT
        if state.smoothed_rtt_ms > self.config.degraded_rtt_threshold_ms as f64 {
            return ConnectionState::Degraded;
        }

        // Check for degraded based on packet loss
        if state.packet_loss_percent() > self.config.degraded_loss_threshold_percent {
            return ConnectionState::Degraded;
        }

        ConnectionState::Healthy
    }

    /// Emit an event to all subscribers
    fn emit_event(&self, event: PeerEvent) {
        let mut senders = self.event_senders.write().unwrap();
        senders.retain(|sender| match sender.try_send(event.clone()) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("Health event channel full");
                true
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        });
    }

    /// Get the number of peers being monitored
    pub fn monitored_peer_count(&self) -> usize {
        self.health.read().unwrap().len()
    }

    /// Clear all monitoring state
    pub fn clear(&self) {
        self.health.write().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_config_default() {
        let config = HeartbeatConfig::default();
        assert!(config.enabled);
        assert_eq!(config.interval, Duration::from_secs(5));
        assert_eq!(config.suspect_threshold, 2);
        assert_eq!(config.dead_threshold, 4);
        assert_eq!(config.degraded_rtt_threshold_ms, 1000);
    }

    #[test]
    fn test_heartbeat_config_disabled() {
        let config = HeartbeatConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_health_monitor_creation() {
        let monitor = HealthMonitor::with_default_config();
        assert!(monitor.is_enabled());
        assert_eq!(monitor.monitored_peer_count(), 0);
    }

    #[test]
    fn test_start_stop_monitoring() {
        let monitor = HealthMonitor::with_default_config();
        let peer_id = NodeId::new("test-peer".to_string());

        monitor.start_monitoring(peer_id.clone());
        assert_eq!(monitor.monitored_peer_count(), 1);
        assert!(monitor.get_health(&peer_id).is_some());

        monitor.stop_monitoring(&peer_id);
        assert_eq!(monitor.monitored_peer_count(), 0);
        assert!(monitor.get_health(&peer_id).is_none());
    }

    #[test]
    fn test_initial_health_is_healthy() {
        let monitor = HealthMonitor::with_default_config();
        let peer_id = NodeId::new("test-peer".to_string());

        monitor.start_monitoring(peer_id.clone());
        let health = monitor.get_health(&peer_id).unwrap();

        assert_eq!(health.state, ConnectionState::Healthy);
        assert_eq!(health.rtt_ms, 0);
        assert_eq!(health.packet_loss_percent, 0);
    }

    #[test]
    fn test_ping_pong_updates_rtt() {
        let monitor = HealthMonitor::with_default_config();
        let peer_id = NodeId::new("test-peer".to_string());

        monitor.start_monitoring(peer_id.clone());

        // Send ping
        let seq = monitor.record_ping_sent(&peer_id).unwrap();

        // Simulate some delay
        std::thread::sleep(Duration::from_millis(10));

        // Receive pong
        monitor.record_pong_received(&peer_id, seq);

        let health = monitor.get_health(&peer_id).unwrap();
        assert!(health.rtt_ms > 0);
        assert_eq!(health.state, ConnectionState::Healthy);
    }

    #[test]
    fn test_missed_heartbeats_lead_to_suspect() {
        let config = HeartbeatConfig {
            interval: Duration::from_millis(10),
            suspect_threshold: 2,
            dead_threshold: 4,
            ..Default::default()
        };
        let monitor = HealthMonitor::new(config);
        let peer_id = NodeId::new("test-peer".to_string());

        monitor.start_monitoring(peer_id.clone());

        // Simulate missed heartbeats by sending pings that timeout
        for _ in 0..2 {
            monitor.record_ping_sent(&peer_id);
            std::thread::sleep(Duration::from_millis(15));
            monitor.check_timeouts();
        }

        let health = monitor.get_health(&peer_id).unwrap();
        assert_eq!(health.state, ConnectionState::Suspect);
    }

    #[test]
    fn test_missed_heartbeats_lead_to_dead() {
        let config = HeartbeatConfig {
            interval: Duration::from_millis(10),
            suspect_threshold: 2,
            dead_threshold: 4,
            ..Default::default()
        };
        let monitor = HealthMonitor::new(config);
        let peer_id = NodeId::new("test-peer".to_string());

        monitor.start_monitoring(peer_id.clone());

        // Simulate enough missed heartbeats to reach Dead
        for _ in 0..4 {
            monitor.record_ping_sent(&peer_id);
            std::thread::sleep(Duration::from_millis(15));
            monitor.check_timeouts();
        }

        let health = monitor.get_health(&peer_id).unwrap();
        assert_eq!(health.state, ConnectionState::Dead);

        let dead = monitor.dead_peers();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0], peer_id);
    }

    #[test]
    fn test_unhealthy_peers_list() {
        let config = HeartbeatConfig {
            interval: Duration::from_millis(10),
            suspect_threshold: 1,
            ..Default::default()
        };
        let monitor = HealthMonitor::new(config);

        let healthy_peer = NodeId::new("healthy".to_string());
        let unhealthy_peer = NodeId::new("unhealthy".to_string());

        monitor.start_monitoring(healthy_peer.clone());
        monitor.start_monitoring(unhealthy_peer.clone());

        // Make one peer unhealthy
        monitor.record_ping_sent(&unhealthy_peer);
        std::thread::sleep(Duration::from_millis(15));
        monitor.check_timeouts();

        let unhealthy = monitor.unhealthy_peers();
        assert_eq!(unhealthy.len(), 1);
        assert_eq!(unhealthy[0], unhealthy_peer);
    }

    #[test]
    fn test_disabled_monitor_does_nothing() {
        let monitor = HealthMonitor::new(HeartbeatConfig::disabled());
        let peer_id = NodeId::new("test-peer".to_string());

        monitor.start_monitoring(peer_id.clone());

        // Monitor should not track anything when disabled
        assert_eq!(monitor.monitored_peer_count(), 0);
        assert!(monitor.peers_needing_ping().is_empty());
    }

    #[test]
    fn test_rtt_ewma_calculation() {
        let mut state = PeerHealthState::new();

        // First sample sets initial values
        state.update_rtt(100.0);
        assert_eq!(state.smoothed_rtt_ms, 100.0);

        // Subsequent samples use EWMA
        state.update_rtt(200.0);
        // EWMA: (1 - 0.125) * 100 + 0.125 * 200 = 87.5 + 25 = 112.5
        assert!((state.smoothed_rtt_ms - 112.5).abs() < 0.1);
    }

    #[test]
    fn test_packet_loss_calculation() {
        let mut state = PeerHealthState::new();
        state.pings_sent = 10;
        state.pongs_received = 8;

        assert_eq!(state.packet_loss_percent(), 20);
    }

    #[test]
    fn test_heartbeat_config_aggressive() {
        let config = HeartbeatConfig::aggressive();
        assert!(config.enabled);
        assert_eq!(config.interval, Duration::from_secs(2));
        assert_eq!(config.suspect_threshold, 2);
        assert_eq!(config.dead_threshold, 3);
        assert_eq!(config.degraded_rtt_threshold_ms, 500);
        assert_eq!(config.degraded_loss_threshold_percent, 5);
    }

    #[test]
    fn test_heartbeat_config_relaxed() {
        let config = HeartbeatConfig::relaxed();
        assert!(config.enabled);
        assert_eq!(config.interval, Duration::from_secs(15));
        assert_eq!(config.suspect_threshold, 3);
        assert_eq!(config.dead_threshold, 6);
        assert_eq!(config.degraded_rtt_threshold_ms, 2000);
        assert_eq!(config.degraded_loss_threshold_percent, 20);
    }

    #[test]
    fn test_monitor_clear() {
        let monitor = HealthMonitor::with_default_config();
        let peer1 = NodeId::new("p1".to_string());
        let peer2 = NodeId::new("p2".to_string());

        monitor.start_monitoring(peer1);
        monitor.start_monitoring(peer2);
        assert_eq!(monitor.monitored_peer_count(), 2);

        monitor.clear();
        assert_eq!(monitor.monitored_peer_count(), 0);
    }

    #[test]
    fn test_subscribe_receives_degraded_event() {
        let config = HeartbeatConfig {
            interval: Duration::from_millis(10),
            suspect_threshold: 100, // high to avoid suspect
            dead_threshold: 200,
            degraded_rtt_threshold_ms: 0, // any RTT triggers degraded
            degraded_loss_threshold_percent: 100, // disable loss-based degradation
            enabled: true,
        };
        let monitor = HealthMonitor::new(config);
        let peer_id = NodeId::new("test-peer".to_string());

        let (tx, mut rx) = mpsc::channel(16);
        monitor.subscribe(tx);
        monitor.start_monitoring(peer_id.clone());

        // Send a ping and receive pong with some RTT
        let seq = monitor.record_ping_sent(&peer_id).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        monitor.record_pong_received(&peer_id, seq);

        // Now check timeouts — RTT > 0 should trigger degraded since threshold is 0
        monitor.check_timeouts();

        // Try to receive the degraded event
        if let Ok(PeerEvent::Degraded { peer_id: id, .. }) = rx.try_recv() {
            assert_eq!(id, peer_id);
        }
    }

    #[test]
    fn test_record_ping_sent_unknown_peer() {
        let monitor = HealthMonitor::with_default_config();
        let peer_id = NodeId::new("unknown".to_string());
        assert!(monitor.record_ping_sent(&peer_id).is_none());
    }

    #[test]
    fn test_record_pong_wrong_sequence() {
        let monitor = HealthMonitor::with_default_config();
        let peer_id = NodeId::new("p1".to_string());

        monitor.start_monitoring(peer_id.clone());
        let seq = monitor.record_ping_sent(&peer_id).unwrap();

        // Send pong with wrong sequence number
        monitor.record_pong_received(&peer_id, seq + 999);

        // RTT should still be 0 since the wrong seq was ignored
        let health = monitor.get_health(&peer_id).unwrap();
        assert_eq!(health.rtt_ms, 0);
    }

    #[test]
    fn test_peers_needing_ping_skips_dead() {
        let config = HeartbeatConfig {
            interval: Duration::from_millis(1),
            suspect_threshold: 1,
            dead_threshold: 2,
            ..Default::default()
        };
        let monitor = HealthMonitor::new(config);
        let peer_id = NodeId::new("dead-peer".to_string());

        monitor.start_monitoring(peer_id.clone());

        // Drive to dead state
        monitor.record_ping_sent(&peer_id);
        std::thread::sleep(Duration::from_millis(5));
        monitor.check_timeouts();
        monitor.record_ping_sent(&peer_id);
        std::thread::sleep(Duration::from_millis(5));
        monitor.check_timeouts();

        let health = monitor.get_health(&peer_id).unwrap();
        assert_eq!(health.state, ConnectionState::Dead);

        // Dead peers should not need ping
        let needing_ping = monitor.peers_needing_ping();
        assert!(!needing_ping.contains(&peer_id));
    }

    #[test]
    fn test_check_timeouts_disabled() {
        let monitor = HealthMonitor::new(HeartbeatConfig::disabled());
        let result = monitor.check_timeouts();
        assert!(result.is_empty());
    }

    #[test]
    fn test_peers_needing_ping_disabled() {
        let monitor = HealthMonitor::new(HeartbeatConfig::disabled());
        assert!(monitor.peers_needing_ping().is_empty());
    }

    #[test]
    fn test_packet_loss_zero_pings() {
        let state = PeerHealthState::new();
        assert_eq!(state.packet_loss_percent(), 0);
    }

    #[test]
    fn test_rtt_variance_update() {
        let mut state = PeerHealthState::new();
        // First sample
        state.update_rtt(100.0);
        assert_eq!(state.rtt_variance_ms, 50.0); // initial variance = rtt/2

        // Second sample with different value
        state.update_rtt(200.0);
        // variance = (1-0.25)*50 + 0.25*|200-100| = 37.5 + 25 = 62.5
        assert!((state.rtt_variance_ms - 62.5).abs() < 0.1);
    }

    #[test]
    fn test_stop_monitoring_unknown_peer() {
        let monitor = HealthMonitor::with_default_config();
        let peer_id = NodeId::new("unknown".to_string());
        // Should not panic
        monitor.stop_monitoring(&peer_id);
        assert_eq!(monitor.monitored_peer_count(), 0);
    }

    #[test]
    fn test_get_health_unknown_peer() {
        let monitor = HealthMonitor::with_default_config();
        let peer_id = NodeId::new("unknown".to_string());
        assert!(monitor.get_health(&peer_id).is_none());
    }

    #[test]
    fn test_monitor_config_accessor() {
        let config = HeartbeatConfig::aggressive();
        let monitor = HealthMonitor::new(config.clone());
        assert_eq!(monitor.config().interval, Duration::from_secs(2));
    }

    #[test]
    fn test_recovery_from_suspect() {
        let config = HeartbeatConfig {
            interval: Duration::from_millis(10),
            suspect_threshold: 2,
            // High packet loss threshold so only missed heartbeats matter
            degraded_loss_threshold_percent: 100,
            ..Default::default()
        };
        let monitor = HealthMonitor::new(config);
        let peer_id = NodeId::new("test-peer".to_string());

        monitor.start_monitoring(peer_id.clone());

        // Miss two heartbeats to go to Suspect
        monitor.record_ping_sent(&peer_id);
        std::thread::sleep(Duration::from_millis(15));
        monitor.check_timeouts();
        monitor.record_ping_sent(&peer_id);
        std::thread::sleep(Duration::from_millis(15));
        monitor.check_timeouts();

        let health = monitor.get_health(&peer_id).unwrap();
        assert_eq!(health.state, ConnectionState::Suspect);

        // Successful pong should recover - resets missed_heartbeats to 0
        let seq = monitor.record_ping_sent(&peer_id).unwrap();
        monitor.record_pong_received(&peer_id, seq);

        let health = monitor.get_health(&peer_id).unwrap();
        assert_eq!(health.state, ConnectionState::Healthy);
    }
}
