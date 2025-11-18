//! Sync error handling and retry logic
//!
//! This module provides production-grade error handling for Automerge document
//! synchronization with exponential backoff, circuit breaker patterns, and
//! comprehensive error tracking.
//!
//! # FFI-Friendly Design
//!
//! All types are designed to be FFI-safe:
//! - Error codes are represented as `#[repr(C)]` enums
//! - Statistics use simple numeric types
//! - No complex Rust-specific types in public API
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │ SyncCoordinator │
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐      ┌──────────────┐
//! │  ErrorHandler   │─────▶│ RetryPolicy  │
//! └────────┬────────┘      └──────────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │ HealthMonitor   │
//! └─────────────────┘
//! ```

#[cfg(feature = "automerge-backend")]
use iroh::EndpointId;
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "automerge-backend")]
use std::time::{Duration, SystemTime};
#[cfg(feature = "automerge-backend")]
use thiserror::Error;

/// Sync-specific error types
///
/// FFI-friendly representation of sync errors with clear categorization.
#[cfg(feature = "automerge-backend")]
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum SyncError {
    /// Network transport error (connection lost, timeout, etc.)
    #[error("Network error: {0}")]
    Network(String),

    /// Automerge document error (corrupted document, invalid operation)
    #[error("Document error: {0}")]
    Document(String),

    /// Peer not found or disconnected
    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    /// Message encoding/decoding error
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Sync state inconsistency
    #[error("State error: {0}")]
    State(String),

    /// Resource exhaustion (memory, file descriptors, etc.)
    #[error("Resource exhaustion: {0}")]
    ResourceExhaustion(String),

    /// Circuit breaker open (too many failures)
    #[error("Circuit breaker open for peer")]
    CircuitBreakerOpen,
}

/// Error severity levels for prioritization
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub enum ErrorSeverity {
    /// Transient error, safe to retry immediately
    Transient = 1,
    /// Recoverable error, needs exponential backoff
    Recoverable = 2,
    /// Severe error, may require circuit breaker
    Severe = 3,
    /// Fatal error, cannot retry
    Fatal = 4,
}

#[cfg(feature = "automerge-backend")]
impl SyncError {
    /// Determine the severity of this error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            SyncError::Network(_) => ErrorSeverity::Recoverable,
            SyncError::PeerNotFound(_) => ErrorSeverity::Transient,
            SyncError::Protocol(_) => ErrorSeverity::Severe,
            SyncError::Document(_) => ErrorSeverity::Fatal,
            SyncError::State(_) => ErrorSeverity::Severe,
            SyncError::ResourceExhaustion(_) => ErrorSeverity::Severe,
            SyncError::CircuitBreakerOpen => ErrorSeverity::Transient,
        }
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.severity(),
            ErrorSeverity::Transient | ErrorSeverity::Recoverable
        )
    }
}

/// Retry policy configuration
///
/// Implements exponential backoff with jitter to prevent thundering herd.
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Initial retry delay
    pub initial_delay: Duration,
    /// Maximum retry delay (caps exponential growth)
    pub max_delay: Duration,
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Exponential backoff base (typically 2.0)
    pub backoff_multiplier: f64,
    /// Jitter factor (0.0 to 1.0) to randomize delays
    pub jitter_factor: f64,
}

#[cfg(feature = "automerge-backend")]
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            max_attempts: 5,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

#[cfg(feature = "automerge-backend")]
impl RetryPolicy {
    /// Create a new retry policy with custom settings
    pub fn new(
        initial_delay: Duration,
        max_delay: Duration,
        max_attempts: u32,
        backoff_multiplier: f64,
    ) -> Self {
        Self {
            initial_delay,
            max_delay,
            max_attempts,
            backoff_multiplier,
            jitter_factor: 0.1,
        }
    }

    /// Create a policy for transient errors (aggressive retries)
    pub fn transient() -> Self {
        Self {
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(5),
            max_attempts: 10,
            backoff_multiplier: 1.5,
            jitter_factor: 0.1,
        }
    }

    /// Create a policy for severe errors (conservative retries)
    pub fn severe() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            max_attempts: 3,
            backoff_multiplier: 3.0,
            jitter_factor: 0.2,
        }
    }

    /// Calculate delay for a given attempt number
    ///
    /// Uses exponential backoff with jitter: delay = base * multiplier^attempt + random_jitter
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        // Calculate base delay: initial_delay * (backoff_multiplier ^ attempt)
        let base_delay_ms = self.initial_delay.as_millis() as f64
            * self.backoff_multiplier.powi(attempt as i32 - 1);

        // Cap at max_delay
        let capped_delay_ms = base_delay_ms.min(self.max_delay.as_millis() as f64);

        // Add jitter: random value between 0 and jitter_factor * capped_delay
        let jitter = if self.jitter_factor > 0.0 {
            use rand::Rng;
            let mut rng = rand::rng();
            rng.random::<f64>() * self.jitter_factor * capped_delay_ms
        } else {
            0.0
        };

        Duration::from_millis((capped_delay_ms + jitter) as u64)
    }

    /// Check if we should retry based on attempt count
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }
}

/// Circuit breaker state for a peer
///
/// Implements the circuit breaker pattern to prevent cascading failures.
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum CircuitState {
    /// Circuit closed, operating normally
    Closed,
    /// Circuit open, rejecting requests
    Open,
    /// Circuit half-open, testing if peer recovered
    HalfOpen,
}

/// Circuit breaker configuration
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures to trigger circuit open
    pub failure_threshold: u32,
    /// Time window for counting failures
    pub failure_window: Duration,
    /// How long to keep circuit open before trying half-open
    pub open_timeout: Duration,
    /// Number of successful requests to close from half-open
    pub success_threshold: u32,
}

#[cfg(feature = "automerge-backend")]
impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            failure_window: Duration::from_secs(60),
            open_timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

/// Per-peer error tracking and health monitoring
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone)]
pub struct PeerHealthTracker {
    /// Current circuit breaker state
    pub circuit_state: CircuitState,
    /// Number of consecutive failures
    pub consecutive_failures: u32,
    /// Number of consecutive successes (for half-open state)
    pub consecutive_successes: u32,
    /// Timestamp of last failure
    pub last_failure_time: Option<SystemTime>,
    /// Timestamp when circuit was opened
    pub circuit_opened_at: Option<SystemTime>,
    /// Total failure count (all time)
    pub total_failures: u64,
    /// Last error encountered
    pub last_error: Option<SyncError>,
    /// Current retry attempt
    pub retry_attempt: u32,
}

#[cfg(feature = "automerge-backend")]
impl Default for PeerHealthTracker {
    fn default() -> Self {
        Self {
            circuit_state: CircuitState::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_failure_time: None,
            circuit_opened_at: None,
            total_failures: 0,
            last_error: None,
            retry_attempt: 0,
        }
    }
}

#[cfg(feature = "automerge-backend")]
impl PeerHealthTracker {
    /// Record a successful sync operation
    pub fn record_success(&mut self) {
        self.retry_attempt = 0;
        self.consecutive_failures = 0;
        self.consecutive_successes += 1;
    }

    /// Record a failed sync operation
    pub fn record_failure(&mut self, error: SyncError) {
        self.retry_attempt += 1;
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        self.total_failures += 1;
        self.last_failure_time = Some(SystemTime::now());
        self.last_error = Some(error);
    }

    /// Reset the tracker to default state
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Sync error handler with retry logic and circuit breaker
///
/// Provides centralized error handling for all sync operations with:
/// - Exponential backoff retry
/// - Circuit breaker per peer
/// - Health monitoring
/// - FFI-friendly statistics
#[cfg(feature = "automerge-backend")]
pub struct SyncErrorHandler {
    /// Retry policy configuration
    retry_policy: RetryPolicy,
    /// Circuit breaker configuration
    circuit_config: CircuitBreakerConfig,
    /// Per-peer health tracking
    peer_health: Arc<RwLock<HashMap<EndpointId, PeerHealthTracker>>>,
}

#[cfg(feature = "automerge-backend")]
impl SyncErrorHandler {
    /// Create a new error handler with default policies
    pub fn new() -> Self {
        Self {
            retry_policy: RetryPolicy::default(),
            circuit_config: CircuitBreakerConfig::default(),
            peer_health: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create an error handler with custom policies
    pub fn with_policies(retry_policy: RetryPolicy, circuit_config: CircuitBreakerConfig) -> Self {
        Self {
            retry_policy,
            circuit_config,
            peer_health: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Handle a sync error for a peer
    ///
    /// Returns Ok(Some(delay)) if should retry after delay, Ok(None) if should not retry,
    /// or Err if circuit breaker is open.
    pub fn handle_error(
        &self,
        peer_id: &EndpointId,
        error: SyncError,
    ) -> Result<Option<Duration>, SyncError> {
        let mut health_map = self.peer_health.write().unwrap();
        let health = health_map.entry(*peer_id).or_default();

        // Record the failure
        health.record_failure(error.clone());

        // Check circuit breaker state
        match health.circuit_state {
            CircuitState::Open => {
                // Check if we should try half-open
                if let Some(opened_at) = health.circuit_opened_at {
                    if SystemTime::now()
                        .duration_since(opened_at)
                        .unwrap_or(Duration::ZERO)
                        >= self.circuit_config.open_timeout
                    {
                        tracing::info!(
                            "Circuit breaker for peer {:?} transitioning to half-open",
                            peer_id
                        );
                        health.circuit_state = CircuitState::HalfOpen;
                        health.retry_attempt = 0;
                        return Ok(Some(Duration::ZERO));
                    }
                }
                return Err(SyncError::CircuitBreakerOpen);
            }
            CircuitState::HalfOpen => {
                // In half-open, allow limited retries
                if health.consecutive_failures >= 1 {
                    tracing::warn!("Circuit breaker for peer {:?} reopening", peer_id);
                    health.circuit_state = CircuitState::Open;
                    health.circuit_opened_at = Some(SystemTime::now());
                    return Err(SyncError::CircuitBreakerOpen);
                }
            }
            CircuitState::Closed => {
                // Check if we should open circuit
                if health.consecutive_failures >= self.circuit_config.failure_threshold {
                    tracing::warn!(
                        "Opening circuit breaker for peer {:?} after {} failures",
                        peer_id,
                        health.consecutive_failures
                    );
                    health.circuit_state = CircuitState::Open;
                    health.circuit_opened_at = Some(SystemTime::now());
                    return Err(SyncError::CircuitBreakerOpen);
                }
            }
        }

        // Determine if error is retryable
        if !error.is_retryable() {
            tracing::error!("Non-retryable sync error for peer {:?}: {}", peer_id, error);
            return Ok(None);
        }

        // Check retry policy
        let policy = match error.severity() {
            ErrorSeverity::Transient => RetryPolicy::transient(),
            ErrorSeverity::Severe => RetryPolicy::severe(),
            _ => self.retry_policy.clone(),
        };

        if !policy.should_retry(health.retry_attempt) {
            tracing::warn!(
                "Max retry attempts ({}) exceeded for peer {:?}",
                policy.max_attempts,
                peer_id
            );
            return Ok(None);
        }

        // Calculate retry delay
        let delay = policy.delay_for_attempt(health.retry_attempt);
        tracing::debug!(
            "Will retry sync with peer {:?} after {:?} (attempt {})",
            peer_id,
            delay,
            health.retry_attempt
        );

        Ok(Some(delay))
    }

    /// Record a successful sync operation
    pub fn record_success(&self, peer_id: &EndpointId) {
        let mut health_map = self.peer_health.write().unwrap();
        let health = health_map.entry(*peer_id).or_default();

        health.record_success();

        // Handle half-open -> closed transition
        if health.circuit_state == CircuitState::HalfOpen
            && health.consecutive_successes >= self.circuit_config.success_threshold
        {
            tracing::info!("Closing circuit breaker for peer {:?}", peer_id);
            health.circuit_state = CircuitState::Closed;
            health.circuit_opened_at = None;
        }
    }

    /// Get health status for a peer
    pub fn peer_health(&self, peer_id: &EndpointId) -> Option<PeerHealthTracker> {
        self.peer_health.read().unwrap().get(peer_id).cloned()
    }

    /// Get health status for all peers
    pub fn all_peer_health(&self) -> HashMap<EndpointId, PeerHealthTracker> {
        self.peer_health.read().unwrap().clone()
    }

    /// Reset health tracking for a peer
    pub fn reset_peer(&self, peer_id: &EndpointId) {
        let mut health_map = self.peer_health.write().unwrap();
        if let Some(health) = health_map.get_mut(peer_id) {
            health.reset();
        }
    }

    /// Check if circuit breaker is open for a peer
    pub fn is_circuit_open(&self, peer_id: &EndpointId) -> bool {
        self.peer_health
            .read()
            .unwrap()
            .get(peer_id)
            .map(|h| h.circuit_state == CircuitState::Open)
            .unwrap_or(false)
    }
}

#[cfg(feature = "automerge-backend")]
impl Default for SyncErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_delay_calculation() {
        let policy = RetryPolicy {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            max_attempts: 5,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // Disable jitter for predictable tests
        };

        // Attempt 0 should have zero delay
        assert_eq!(policy.delay_for_attempt(0), Duration::ZERO);

        // Attempt 1: 100ms
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(100));

        // Attempt 2: 100ms * 2 = 200ms
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(200));

        // Attempt 3: 100ms * 4 = 400ms
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(400));

        // Should respect max_delay
        let long_policy = RetryPolicy {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(5),
            max_attempts: 10,
            backoff_multiplier: 10.0,
            jitter_factor: 0.0,
        };
        let delay = long_policy.delay_for_attempt(5);
        assert!(delay <= Duration::from_secs(5));
    }

    #[test]
    fn test_retry_policy_should_retry() {
        let policy = RetryPolicy {
            max_attempts: 3,
            ..Default::default()
        };

        assert!(policy.should_retry(0));
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
        assert!(!policy.should_retry(4));
    }

    #[test]
    fn test_sync_error_severity() {
        assert_eq!(
            SyncError::Network("test".to_string()).severity(),
            ErrorSeverity::Recoverable
        );
        assert_eq!(
            SyncError::PeerNotFound("test".to_string()).severity(),
            ErrorSeverity::Transient
        );
        assert_eq!(
            SyncError::Document("test".to_string()).severity(),
            ErrorSeverity::Fatal
        );
        assert_eq!(
            SyncError::Protocol("test".to_string()).severity(),
            ErrorSeverity::Severe
        );
    }

    #[test]
    fn test_sync_error_retryable() {
        assert!(SyncError::Network("test".to_string()).is_retryable());
        assert!(SyncError::PeerNotFound("test".to_string()).is_retryable());
        assert!(!SyncError::Document("test".to_string()).is_retryable());
    }

    #[test]
    fn test_peer_health_tracker() {
        let mut tracker = PeerHealthTracker::default();

        // Record success
        tracker.record_success();
        assert_eq!(tracker.consecutive_successes, 1);
        assert_eq!(tracker.consecutive_failures, 0);
        assert_eq!(tracker.retry_attempt, 0);

        // Record failure
        tracker.record_failure(SyncError::Network("timeout".to_string()));
        assert_eq!(tracker.consecutive_successes, 0);
        assert_eq!(tracker.consecutive_failures, 1);
        assert_eq!(tracker.retry_attempt, 1);
        assert_eq!(tracker.total_failures, 1);

        // Record another failure
        tracker.record_failure(SyncError::Network("timeout".to_string()));
        assert_eq!(tracker.consecutive_failures, 2);
        assert_eq!(tracker.retry_attempt, 2);
        assert_eq!(tracker.total_failures, 2);

        // Reset
        tracker.reset();
        assert_eq!(tracker.consecutive_failures, 0);
        assert_eq!(tracker.total_failures, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_threshold() {
        let handler = SyncErrorHandler::with_policies(
            RetryPolicy::default(),
            CircuitBreakerConfig {
                failure_threshold: 3,
                ..Default::default()
            },
        );

        // Create a test EndpointId using SecretKey generation
        use iroh::SecretKey;
        let mut rng = rand::rng();
        let peer_id = SecretKey::generate(&mut rng).public();

        // First 2 failures should allow retry (consecutive_failures will be 1, then 2)
        for i in 0..2 {
            let result = handler.handle_error(&peer_id, SyncError::Network("test".to_string()));
            assert!(result.is_ok(), "Attempt {} should succeed", i);
        }

        // 3rd failure should open circuit (consecutive_failures reaches 3)
        let result = handler.handle_error(&peer_id, SyncError::Network("test".to_string()));
        assert!(
            matches!(result, Err(SyncError::CircuitBreakerOpen)),
            "Circuit should open on 3rd failure"
        );

        // Verify circuit is open
        assert!(handler.is_circuit_open(&peer_id));
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_transition() {
        let handler = SyncErrorHandler::with_policies(
            RetryPolicy::default(),
            CircuitBreakerConfig {
                failure_threshold: 2,
                open_timeout: Duration::from_millis(100),
                success_threshold: 2,
                ..Default::default()
            },
        );

        // Create a test EndpointId using SecretKey generation
        use iroh::SecretKey;
        let mut rng = rand::rng();
        let peer_id = SecretKey::generate(&mut rng).public();

        // Trigger circuit open
        handler
            .handle_error(&peer_id, SyncError::Network("test".to_string()))
            .ok();
        handler
            .handle_error(&peer_id, SyncError::Network("test".to_string()))
            .ok();
        handler
            .handle_error(&peer_id, SyncError::Network("test".to_string()))
            .ok();

        assert!(handler.is_circuit_open(&peer_id));

        // Wait for open timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Next error should transition to half-open
        let result = handler.handle_error(&peer_id, SyncError::Network("test".to_string()));
        assert!(result.is_ok());

        let health = handler.peer_health(&peer_id).unwrap();
        assert_eq!(health.circuit_state, CircuitState::HalfOpen);

        // Record successes to close circuit
        handler.record_success(&peer_id);
        handler.record_success(&peer_id);

        let health = handler.peer_health(&peer_id).unwrap();
        assert_eq!(health.circuit_state, CircuitState::Closed);
    }
}
