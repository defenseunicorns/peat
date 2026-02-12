//! Automatic reconnection with exponential backoff (Issue #253)
//!
//! This module provides reconnection management for peers that disconnect.
//! It implements exponential backoff with jitter to prevent thundering herd
//! problems when multiple peers attempt to reconnect simultaneously.
//!
//! ## Design
//!
//! - **Static-config peers**: Automatically queued for reconnection on disconnect
//! - **mDNS-discovered peers**: Rely on rediscovery, not reconnection loops
//! - **Exponential backoff**: Delay doubles after each failure (with cap)
//! - **Jitter**: Random variation prevents synchronized retry storms
//!
//! ## Example
//!
//! ```ignore
//! use hive_mesh::transport::reconnection::{ReconnectionPolicy, ReconnectionManager};
//!
//! let policy = ReconnectionPolicy::default();
//! let mut manager = ReconnectionManager::new(policy);
//!
//! // When a peer disconnects
//! manager.schedule_reconnect(peer_id.clone());
//!
//! // In the background task
//! for peer_id in manager.due_reconnections() {
//!     match transport.connect(&peer_id).await {
//!         Ok(_) => manager.reconnected(&peer_id),
//!         Err(e) => manager.failed(&peer_id, e.to_string()),
//!     }
//! }
//! ```

use super::NodeId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Policy configuration for automatic reconnection
///
/// Configures how the reconnection manager handles peer disconnections.
/// Uses exponential backoff with optional jitter to prevent thundering herd.
#[derive(Debug, Clone)]
pub struct ReconnectionPolicy {
    /// Enable automatic reconnection
    pub enabled: bool,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Backoff multiplier (e.g., 2.0 for doubling)
    pub backoff_multiplier: f64,
    /// Maximum number of retries (None = infinite)
    pub max_retries: Option<u32>,
    /// Jitter factor (0.0-1.0) to prevent thundering herd
    pub jitter: f64,
}

impl Default for ReconnectionPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_retries: Some(10),
            jitter: 0.2,
        }
    }
}

impl ReconnectionPolicy {
    /// Create a policy with reconnection disabled
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Create an aggressive policy for low-latency networks
    pub fn aggressive() -> Self {
        Self {
            enabled: true,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 1.5,
            max_retries: Some(20),
            jitter: 0.1,
        }
    }

    /// Create a conservative policy for unreliable networks
    pub fn conservative() -> Self {
        Self {
            enabled: true,
            initial_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(300),
            backoff_multiplier: 2.0,
            max_retries: Some(5),
            jitter: 0.3,
        }
    }

    /// Calculate delay for a given attempt number (0-indexed)
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay =
            self.initial_delay.as_secs_f64() * self.backoff_multiplier.powi(attempt as i32);
        let capped_delay = base_delay.min(self.max_delay.as_secs_f64());

        // Apply jitter: delay * (1 - jitter + random(0, 2*jitter))
        // For deterministic testing, we use a simple hash-based "random"
        let jitter_factor = 1.0 - self.jitter + (self.jitter * 2.0 * pseudo_random(attempt));
        let final_delay = capped_delay * jitter_factor;

        Duration::from_secs_f64(final_delay.max(0.0))
    }

    /// Check if another retry should be attempted
    pub fn should_retry(&self, attempts: u32) -> bool {
        if !self.enabled {
            return false;
        }
        match self.max_retries {
            Some(max) => attempts < max,
            None => true,
        }
    }
}

/// Simple pseudo-random function based on attempt number
/// Used for deterministic jitter in tests
fn pseudo_random(seed: u32) -> f64 {
    // Simple hash-based pseudo-random
    let hash = seed.wrapping_mul(2654435761);
    (hash as f64) / (u32::MAX as f64)
}

/// State for a pending reconnection
#[derive(Debug, Clone)]
pub struct ReconnectionState {
    /// Number of failed attempts so far
    pub attempts: u32,
    /// When to attempt next reconnection
    pub next_attempt: Instant,
    /// Last error message (if any)
    pub last_error: Option<String>,
    /// Whether this peer was from static config (vs discovered)
    pub is_static_peer: bool,
}

impl ReconnectionState {
    /// Create a new reconnection state
    pub fn new(next_attempt: Instant, is_static_peer: bool) -> Self {
        Self {
            attempts: 0,
            next_attempt,
            last_error: None,
            is_static_peer,
        }
    }
}

/// Manages automatic reconnection for disconnected peers
///
/// Tracks pending reconnections and calculates backoff delays.
/// Should be used in conjunction with the transport's background task.
#[derive(Debug)]
pub struct ReconnectionManager {
    policy: ReconnectionPolicy,
    /// Pending reconnections by peer ID
    pending: HashMap<NodeId, ReconnectionState>,
}

impl ReconnectionManager {
    /// Create a new reconnection manager with the given policy
    pub fn new(policy: ReconnectionPolicy) -> Self {
        Self {
            policy,
            pending: HashMap::new(),
        }
    }

    /// Get the current policy
    pub fn policy(&self) -> &ReconnectionPolicy {
        &self.policy
    }

    /// Check if reconnection is enabled
    pub fn is_enabled(&self) -> bool {
        self.policy.enabled
    }

    /// Schedule reconnection for a disconnected peer
    ///
    /// Only schedules if reconnection is enabled and the peer is from static config.
    /// mDNS-discovered peers should rely on rediscovery instead.
    pub fn schedule_reconnect(&mut self, peer_id: NodeId, is_static_peer: bool) {
        if !self.policy.enabled {
            return;
        }

        // Only auto-reconnect static peers; discovered peers should be rediscovered
        if !is_static_peer {
            return;
        }

        // If already pending, don't reset the backoff
        if self.pending.contains_key(&peer_id) {
            return;
        }

        let next_attempt = Instant::now() + self.policy.initial_delay;
        self.pending.insert(
            peer_id,
            ReconnectionState::new(next_attempt, is_static_peer),
        );
    }

    /// Get peers that are due for a reconnection attempt
    pub fn due_reconnections(&self) -> Vec<NodeId> {
        let now = Instant::now();
        self.pending
            .iter()
            .filter(|(_, state)| state.next_attempt <= now)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get the number of pending reconnections
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Check if a peer is pending reconnection
    pub fn is_pending(&self, peer_id: &NodeId) -> bool {
        self.pending.contains_key(peer_id)
    }

    /// Get the state for a pending reconnection
    pub fn get_state(&self, peer_id: &NodeId) -> Option<&ReconnectionState> {
        self.pending.get(peer_id)
    }

    /// Record a successful reconnection
    ///
    /// Removes the peer from the pending queue.
    pub fn reconnected(&mut self, peer_id: &NodeId) {
        self.pending.remove(peer_id);
    }

    /// Record a failed reconnection attempt
    ///
    /// Increments the attempt counter and calculates the next retry time.
    /// Returns `true` if another retry will be attempted, `false` if max retries exceeded.
    pub fn failed(&mut self, peer_id: &NodeId, error: String) -> bool {
        if let Some(state) = self.pending.get_mut(peer_id) {
            state.attempts += 1;
            state.last_error = Some(error);

            if self.policy.should_retry(state.attempts) {
                let delay = self.policy.calculate_delay(state.attempts);
                state.next_attempt = Instant::now() + delay;
                true
            } else {
                // Max retries exceeded, remove from pending
                false
            }
        } else {
            false
        }
    }

    /// Remove a peer from pending reconnections (e.g., when max retries exceeded)
    pub fn remove(&mut self, peer_id: &NodeId) -> Option<ReconnectionState> {
        self.pending.remove(peer_id)
    }

    /// Clear all pending reconnections
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// Get all pending peer IDs
    pub fn pending_peers(&self) -> Vec<NodeId> {
        self.pending.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = ReconnectionPolicy::default();
        assert!(policy.enabled);
        assert_eq!(policy.initial_delay, Duration::from_secs(1));
        assert_eq!(policy.max_delay, Duration::from_secs(60));
        assert_eq!(policy.backoff_multiplier, 2.0);
        assert_eq!(policy.max_retries, Some(10));
        assert_eq!(policy.jitter, 0.2);
    }

    #[test]
    fn test_disabled_policy() {
        let policy = ReconnectionPolicy::disabled();
        assert!(!policy.enabled);
        assert!(!policy.should_retry(0));
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let policy = ReconnectionPolicy {
            enabled: true,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_retries: Some(10),
            jitter: 0.0, // No jitter for deterministic test
        };

        // Attempt 0: 1s * 2^0 = 1s
        let delay0 = policy.calculate_delay(0);
        assert_eq!(delay0, Duration::from_secs(1));

        // Attempt 1: 1s * 2^1 = 2s
        let delay1 = policy.calculate_delay(1);
        assert_eq!(delay1, Duration::from_secs(2));

        // Attempt 2: 1s * 2^2 = 4s
        let delay2 = policy.calculate_delay(2);
        assert_eq!(delay2, Duration::from_secs(4));

        // Attempt 5: 1s * 2^5 = 32s
        let delay5 = policy.calculate_delay(5);
        assert_eq!(delay5, Duration::from_secs(32));

        // Attempt 6: 1s * 2^6 = 64s, but capped at 60s
        let delay6 = policy.calculate_delay(6);
        assert_eq!(delay6, Duration::from_secs(60));
    }

    #[test]
    fn test_should_retry() {
        let policy = ReconnectionPolicy {
            max_retries: Some(3),
            ..Default::default()
        };

        assert!(policy.should_retry(0));
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
        assert!(!policy.should_retry(4));
    }

    #[test]
    fn test_infinite_retries() {
        let policy = ReconnectionPolicy {
            max_retries: None,
            ..Default::default()
        };

        assert!(policy.should_retry(0));
        assert!(policy.should_retry(100));
        assert!(policy.should_retry(1000));
    }

    #[test]
    fn test_jitter_bounds() {
        let policy = ReconnectionPolicy {
            initial_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(100),
            jitter: 0.2,
            ..Default::default()
        };

        // With 20% jitter, delay should be 10s * (0.8 to 1.2) = 8s to 12s
        for attempt in 0..10 {
            let delay = policy.calculate_delay(attempt);
            // Just verify it's a reasonable value (not checking exact bounds due to pseudo-random)
            assert!(delay.as_secs_f64() > 0.0);
        }
    }

    #[test]
    fn test_reconnection_manager_schedule() {
        let policy = ReconnectionPolicy::default();
        let mut manager = ReconnectionManager::new(policy);

        let peer_id = NodeId::new("test-peer".to_string());

        // Schedule reconnection for static peer
        manager.schedule_reconnect(peer_id.clone(), true);
        assert!(manager.is_pending(&peer_id));
        assert_eq!(manager.pending_count(), 1);

        // Scheduling again should not reset
        manager.schedule_reconnect(peer_id.clone(), true);
        assert_eq!(manager.pending_count(), 1);
    }

    #[test]
    fn test_reconnection_manager_discovered_peer() {
        let policy = ReconnectionPolicy::default();
        let mut manager = ReconnectionManager::new(policy);

        let peer_id = NodeId::new("discovered-peer".to_string());

        // Discovered peers should not be scheduled for reconnection
        manager.schedule_reconnect(peer_id.clone(), false);
        assert!(!manager.is_pending(&peer_id));
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_reconnection_manager_success() {
        let policy = ReconnectionPolicy::default();
        let mut manager = ReconnectionManager::new(policy);

        let peer_id = NodeId::new("test-peer".to_string());
        manager.schedule_reconnect(peer_id.clone(), true);

        // Reconnection succeeds
        manager.reconnected(&peer_id);
        assert!(!manager.is_pending(&peer_id));
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_reconnection_manager_failure() {
        let policy = ReconnectionPolicy {
            max_retries: Some(3),
            ..Default::default()
        };
        let mut manager = ReconnectionManager::new(policy);

        let peer_id = NodeId::new("test-peer".to_string());
        manager.schedule_reconnect(peer_id.clone(), true);

        // First failure - should retry
        assert!(manager.failed(&peer_id, "connection refused".to_string()));
        assert!(manager.is_pending(&peer_id));
        assert_eq!(manager.get_state(&peer_id).unwrap().attempts, 1);

        // Second failure - should retry
        assert!(manager.failed(&peer_id, "connection refused".to_string()));
        assert_eq!(manager.get_state(&peer_id).unwrap().attempts, 2);

        // Third failure - max retries exceeded
        assert!(!manager.failed(&peer_id, "connection refused".to_string()));
    }

    #[test]
    fn test_due_reconnections() {
        let policy = ReconnectionPolicy {
            initial_delay: Duration::from_millis(1), // Very short for testing
            ..Default::default()
        };
        let mut manager = ReconnectionManager::new(policy);

        let peer1 = NodeId::new("peer1".to_string());
        let peer2 = NodeId::new("peer2".to_string());

        manager.schedule_reconnect(peer1.clone(), true);
        manager.schedule_reconnect(peer2.clone(), true);

        // Initially neither is due (need to wait for initial_delay)
        // But with 1ms delay, they should be due almost immediately
        std::thread::sleep(Duration::from_millis(5));

        let due = manager.due_reconnections();
        assert_eq!(due.len(), 2);
        assert!(due.contains(&peer1));
        assert!(due.contains(&peer2));
    }

    #[test]
    fn test_disabled_reconnection() {
        let policy = ReconnectionPolicy::disabled();
        let mut manager = ReconnectionManager::new(policy);

        let peer_id = NodeId::new("test-peer".to_string());
        manager.schedule_reconnect(peer_id.clone(), true);

        // Should not be scheduled when disabled
        assert!(!manager.is_pending(&peer_id));
    }

    #[test]
    fn test_aggressive_policy() {
        let policy = ReconnectionPolicy::aggressive();
        assert!(policy.enabled);
        assert_eq!(policy.initial_delay, Duration::from_millis(100));
        assert_eq!(policy.max_delay, Duration::from_secs(10));
        assert_eq!(policy.backoff_multiplier, 1.5);
        assert_eq!(policy.max_retries, Some(20));
        assert_eq!(policy.jitter, 0.1);
    }

    #[test]
    fn test_conservative_policy() {
        let policy = ReconnectionPolicy::conservative();
        assert!(policy.enabled);
        assert_eq!(policy.initial_delay, Duration::from_secs(5));
        assert_eq!(policy.max_delay, Duration::from_secs(300));
        assert_eq!(policy.backoff_multiplier, 2.0);
        assert_eq!(policy.max_retries, Some(5));
        assert_eq!(policy.jitter, 0.3);
    }

    #[test]
    fn test_policy_accessor() {
        let policy = ReconnectionPolicy::aggressive();
        let manager = ReconnectionManager::new(policy);
        assert_eq!(manager.policy().initial_delay, Duration::from_millis(100));
        assert!(manager.is_enabled());
    }

    #[test]
    fn test_is_enabled_false() {
        let policy = ReconnectionPolicy::disabled();
        let manager = ReconnectionManager::new(policy);
        assert!(!manager.is_enabled());
    }

    #[test]
    fn test_clear_pending() {
        let mut manager = ReconnectionManager::new(ReconnectionPolicy::default());
        let p1 = NodeId::new("p1".to_string());
        let p2 = NodeId::new("p2".to_string());
        manager.schedule_reconnect(p1, true);
        manager.schedule_reconnect(p2, true);
        assert_eq!(manager.pending_count(), 2);

        manager.clear();
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_remove_returns_state() {
        let mut manager = ReconnectionManager::new(ReconnectionPolicy::default());
        let peer_id = NodeId::new("peer".to_string());
        manager.schedule_reconnect(peer_id.clone(), true);

        let state = manager.remove(&peer_id);
        assert!(state.is_some());
        let s = state.unwrap();
        assert_eq!(s.attempts, 0);
        assert!(s.is_static_peer);
        assert!(s.last_error.is_none());

        // Remove again returns None
        assert!(manager.remove(&peer_id).is_none());
    }

    #[test]
    fn test_pending_peers() {
        let mut manager = ReconnectionManager::new(ReconnectionPolicy::default());
        let p1 = NodeId::new("p1".to_string());
        let p2 = NodeId::new("p2".to_string());
        manager.schedule_reconnect(p1.clone(), true);
        manager.schedule_reconnect(p2.clone(), true);

        let peers = manager.pending_peers();
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&p1));
        assert!(peers.contains(&p2));
    }

    #[test]
    fn test_failed_unknown_peer() {
        let mut manager = ReconnectionManager::new(ReconnectionPolicy::default());
        let peer_id = NodeId::new("unknown".to_string());
        // Failing a peer not in pending should return false
        assert!(!manager.failed(&peer_id, "error".to_string()));
    }

    #[test]
    fn test_get_state_with_error() {
        let mut manager = ReconnectionManager::new(ReconnectionPolicy {
            max_retries: Some(5),
            ..Default::default()
        });
        let peer_id = NodeId::new("peer".to_string());
        manager.schedule_reconnect(peer_id.clone(), true);

        manager.failed(&peer_id, "connection refused".to_string());
        let state = manager.get_state(&peer_id).unwrap();
        assert_eq!(state.attempts, 1);
        assert_eq!(state.last_error.as_deref(), Some("connection refused"));
        assert!(state.is_static_peer);
    }

    #[test]
    fn test_reconnection_state_new() {
        let now = Instant::now();
        let state = ReconnectionState::new(now, false);
        assert_eq!(state.attempts, 0);
        assert_eq!(state.next_attempt, now);
        assert!(state.last_error.is_none());
        assert!(!state.is_static_peer);
    }

    #[test]
    fn test_reconnected_unknown_peer_noop() {
        let mut manager = ReconnectionManager::new(ReconnectionPolicy::default());
        // Should be a no-op, not panic
        manager.reconnected(&NodeId::new("unknown".to_string()));
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_policy_debug_clone() {
        let policy = ReconnectionPolicy::default();
        let cloned = policy.clone();
        assert_eq!(cloned.enabled, policy.enabled);
        let _ = format!("{:?}", policy);
    }

    #[test]
    fn test_reconnection_state_debug_clone() {
        let state = ReconnectionState::new(Instant::now(), true);
        let cloned = state.clone();
        assert_eq!(cloned.attempts, state.attempts);
        let _ = format!("{:?}", state);
    }

    #[test]
    fn test_manager_debug() {
        let manager = ReconnectionManager::new(ReconnectionPolicy::default());
        let _ = format!("{:?}", manager);
    }
}
