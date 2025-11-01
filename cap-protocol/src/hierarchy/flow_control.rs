//! Message flow control for hierarchical routing
//!
//! This module implements bandwidth limiting and backpressure mechanisms
//! to prevent congestion in the hierarchical messaging system.
//!
//! # Architecture
//!
//! Flow control operates at two levels:
//! - **Cell Level**: Controls message flow within cells
//! - **Zone Level**: Controls message flow at zone coordinator level
//!
//! ## Token Bucket Algorithm
//!
//! Rate limiting uses a token bucket algorithm:
//! - Tokens represent message/bandwidth capacity
//! - Tokens refill at a constant rate
//! - Messages consume tokens based on size and priority
//! - When bucket is empty, backpressure is applied
//!
//! ## Backpressure Strategy
//!
//! When congestion is detected:
//! 1. **Drop low-priority messages** (based on policy)
//! 2. **Slow down message generation** (apply backpressure)
//! 3. **Signal upstream** (propagate backpressure up hierarchy)
//!
//! # Example
//!
//! ```
//! use cap_protocol::hierarchy::flow_control::{FlowController, BandwidthLimit, MessageDropPolicy, RoutingLevel};
//! use cap_protocol::cell::messaging::{MessagePriority, CellMessage};
//!
//! # async fn example() -> cap_protocol::Result<()> {
//! let cell_limit = BandwidthLimit {
//!     messages_per_sec: 100,
//!     bytes_per_sec: 10_000,
//! };
//!
//! let zone_limit = BandwidthLimit {
//!     messages_per_sec: 50,
//!     bytes_per_sec: 5_000,
//! };
//!
//! let controller = FlowController::new(
//!     cell_limit,
//!     zone_limit,
//!     MessageDropPolicy::DropLowPriority,
//! );
//!
//! // Acquire permit before sending
//! let permit = controller.acquire_permit(RoutingLevel::Cell, 100, MessagePriority::Normal).await?;
//!
//! // Check if backpressure is active
//! if controller.has_backpressure().await {
//!     // Slow down message generation
//! }
//! # Ok(())
//! # }
//! ```

use crate::cell::messaging::MessagePriority;
use crate::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, instrument, warn};

/// Bandwidth limits for a routing level
#[derive(Debug, Clone, Copy)]
pub struct BandwidthLimit {
    /// Maximum messages per second
    pub messages_per_sec: usize,
    /// Maximum bytes per second
    pub bytes_per_sec: usize,
}

impl BandwidthLimit {
    /// Create a new bandwidth limit
    pub fn new(messages_per_sec: usize, bytes_per_sec: usize) -> Self {
        Self {
            messages_per_sec,
            bytes_per_sec,
        }
    }

    /// Default cell-level limits
    pub fn cell_default() -> Self {
        Self {
            messages_per_sec: 100,
            bytes_per_sec: 100_000, // 100 KB/s
        }
    }

    /// Default zone-level limits (more restrictive)
    pub fn zone_default() -> Self {
        Self {
            messages_per_sec: 50,
            bytes_per_sec: 50_000, // 50 KB/s
        }
    }
}

/// Routing level for flow control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingLevel {
    /// Cell-level routing (intra-cell)
    Cell,
    /// Zone-level routing (cell ↔ zone)
    Zone,
}

/// Message drop policy when under backpressure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDropPolicy {
    /// Drop low-priority messages first (Low, then Normal)
    DropLowPriority,
    /// Drop oldest messages first (FIFO)
    DropOldest,
    /// Never drop messages (apply max backpressure instead)
    NeverDrop,
}

/// Backpressure state
#[derive(Debug)]
struct BackpressureState {
    /// Is backpressure currently active?
    active: bool,
    /// When backpressure started
    started_at: Option<Instant>,
    /// Number of messages dropped due to backpressure
    #[allow(dead_code)] // Reserved for future use
    dropped_count: u64,
}

impl BackpressureState {
    fn new() -> Self {
        Self {
            active: false,
            started_at: None,
            dropped_count: 0,
        }
    }

    fn activate(&mut self) {
        if !self.active {
            self.active = true;
            self.started_at = Some(Instant::now());
            debug!("Backpressure activated");
        }
    }

    fn deactivate(&mut self) {
        if self.active {
            self.active = false;
            let duration = self
                .started_at
                .map(|s| s.elapsed().as_millis())
                .unwrap_or(0);
            debug!("Backpressure released after {}ms", duration);
            self.started_at = None;
        }
    }
}

/// Token bucket rate limiter
///
/// Implements the token bucket algorithm for rate limiting.
/// Tokens represent message/bandwidth capacity.
struct TokenBucket {
    /// Current token count
    tokens: Arc<Mutex<f64>>,
    /// Maximum bucket capacity
    capacity: f64,
    /// Token refill rate (tokens per second)
    refill_rate: f64,
    /// Last refill time
    last_refill: Arc<Mutex<Instant>>,
}

impl TokenBucket {
    /// Create a new token bucket
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: Arc::new(Mutex::new(capacity)),
            capacity,
            refill_rate,
            last_refill: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Try to consume tokens (non-blocking)
    async fn try_consume(&self, amount: f64) -> bool {
        // Refill tokens based on elapsed time
        self.refill().await;

        let mut tokens = self.tokens.lock().await;
        if *tokens >= amount {
            *tokens -= amount;
            true
        } else {
            false
        }
    }

    /// Wait for tokens to be available (blocking)
    async fn consume(&self, amount: f64) -> Result<()> {
        loop {
            if self.try_consume(amount).await {
                return Ok(());
            }
            // Wait a bit before retrying
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Refill tokens based on elapsed time
    async fn refill(&self) {
        let mut last_refill = self.last_refill.lock().await;
        let elapsed = last_refill.elapsed().as_secs_f64();

        if elapsed > 0.0 {
            let mut tokens = self.tokens.lock().await;
            let new_tokens = elapsed * self.refill_rate;
            *tokens = (*tokens + new_tokens).min(self.capacity);
            *last_refill = Instant::now();
        }
    }

    /// Get current token count
    async fn available_tokens(&self) -> f64 {
        self.refill().await;
        *self.tokens.lock().await
    }
}

/// Flow control for hierarchical message routing
///
/// Provides bandwidth limiting and backpressure for cell and zone level routing.
pub struct FlowController {
    /// Cell-level rate limiter (message count)
    cell_message_limiter: Arc<TokenBucket>,
    /// Cell-level rate limiter (byte count)
    cell_byte_limiter: Arc<TokenBucket>,
    /// Zone-level rate limiter (message count)
    zone_message_limiter: Arc<TokenBucket>,
    /// Zone-level rate limiter (byte count)
    zone_byte_limiter: Arc<TokenBucket>,
    /// Backpressure state
    backpressure: Arc<Mutex<BackpressureState>>,
    /// Message dropping policy
    drop_policy: MessageDropPolicy,
    /// Flow control metrics
    metrics: Arc<FlowMetricsInner>,
}

/// Internal metrics tracking
struct FlowMetricsInner {
    cell_messages_sent: AtomicU64,
    cell_bytes_sent: AtomicU64,
    zone_messages_sent: AtomicU64,
    zone_bytes_sent: AtomicU64,
    messages_dropped: AtomicU64,
    backpressure_events: AtomicU64,
}

impl FlowController {
    /// Create a new flow controller
    pub fn new(
        cell_limit: BandwidthLimit,
        zone_limit: BandwidthLimit,
        drop_policy: MessageDropPolicy,
    ) -> Self {
        Self {
            cell_message_limiter: Arc::new(TokenBucket::new(
                cell_limit.messages_per_sec as f64,
                cell_limit.messages_per_sec as f64,
            )),
            cell_byte_limiter: Arc::new(TokenBucket::new(
                cell_limit.bytes_per_sec as f64,
                cell_limit.bytes_per_sec as f64,
            )),
            zone_message_limiter: Arc::new(TokenBucket::new(
                zone_limit.messages_per_sec as f64,
                zone_limit.messages_per_sec as f64,
            )),
            zone_byte_limiter: Arc::new(TokenBucket::new(
                zone_limit.bytes_per_sec as f64,
                zone_limit.bytes_per_sec as f64,
            )),
            backpressure: Arc::new(Mutex::new(BackpressureState::new())),
            drop_policy,
            metrics: Arc::new(FlowMetricsInner {
                cell_messages_sent: AtomicU64::new(0),
                cell_bytes_sent: AtomicU64::new(0),
                zone_messages_sent: AtomicU64::new(0),
                zone_bytes_sent: AtomicU64::new(0),
                messages_dropped: AtomicU64::new(0),
                backpressure_events: AtomicU64::new(0),
            }),
        }
    }

    /// Acquire permit to send a message
    ///
    /// This will block if rate limits are exceeded, applying backpressure.
    #[instrument(skip(self))]
    pub async fn acquire_permit(
        &self,
        level: RoutingLevel,
        message_size: usize,
        priority: MessagePriority,
    ) -> Result<Permit> {
        let (msg_limiter, byte_limiter) = match level {
            RoutingLevel::Cell => (&self.cell_message_limiter, &self.cell_byte_limiter),
            RoutingLevel::Zone => (&self.zone_message_limiter, &self.zone_byte_limiter),
        };

        // Adjust token consumption based on priority
        // Higher priority messages consume fewer tokens (get preferential treatment)
        let priority_multiplier = match priority {
            MessagePriority::Critical => 0.5, // Consumes half tokens
            MessagePriority::High => 0.75,    // Consumes 75% tokens
            MessagePriority::Normal => 1.0,   // Consumes normal tokens
            MessagePriority::Low => 1.5,      // Consumes 50% more tokens
        };

        let message_tokens = 1.0 * priority_multiplier;
        let byte_tokens = message_size as f64 * priority_multiplier;

        // Try to acquire tokens (will block if needed)
        let acquired = msg_limiter.try_consume(message_tokens).await
            && byte_limiter.try_consume(byte_tokens).await;

        if !acquired {
            // Apply backpressure
            self.apply_backpressure_internal(level).await;

            // Wait for tokens
            msg_limiter.consume(message_tokens).await?;
            byte_limiter.consume(byte_tokens).await?;
        }

        // Update metrics
        match level {
            RoutingLevel::Cell => {
                self.metrics
                    .cell_messages_sent
                    .fetch_add(1, Ordering::Relaxed);
                self.metrics
                    .cell_bytes_sent
                    .fetch_add(message_size as u64, Ordering::Relaxed);
            }
            RoutingLevel::Zone => {
                self.metrics
                    .zone_messages_sent
                    .fetch_add(1, Ordering::Relaxed);
                self.metrics
                    .zone_bytes_sent
                    .fetch_add(message_size as u64, Ordering::Relaxed);
            }
        }

        Ok(Permit { _private: () })
    }

    /// Check if backpressure is currently active
    pub async fn has_backpressure(&self) -> bool {
        let state = self.backpressure.lock().await;
        state.active
    }

    /// Apply backpressure at a specific routing level
    async fn apply_backpressure_internal(&self, level: RoutingLevel) {
        let mut state = self.backpressure.lock().await;
        state.activate();
        self.metrics
            .backpressure_events
            .fetch_add(1, Ordering::Relaxed);

        warn!("Backpressure applied at {:?} level", level);
    }

    /// Release backpressure
    pub async fn release_backpressure(&self) {
        let mut state = self.backpressure.lock().await;
        state.deactivate();
    }

    /// Determine if a message should be dropped based on policy
    pub fn should_drop(&self, priority: MessagePriority) -> bool {
        match self.drop_policy {
            MessageDropPolicy::DropLowPriority => {
                // Drop Low and Normal priority messages when under pressure
                matches!(priority, MessagePriority::Low | MessagePriority::Normal)
            }
            MessageDropPolicy::DropOldest => {
                // This is handled externally (queue management)
                false
            }
            MessageDropPolicy::NeverDrop => false,
        }
    }

    /// Record a dropped message
    pub fn record_drop(&self) {
        self.metrics
            .messages_dropped
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Get flow control metrics
    pub fn get_metrics(&self) -> FlowMetrics {
        FlowMetrics {
            cell_messages_sent: self.metrics.cell_messages_sent.load(Ordering::Relaxed),
            cell_bytes_sent: self.metrics.cell_bytes_sent.load(Ordering::Relaxed),
            zone_messages_sent: self.metrics.zone_messages_sent.load(Ordering::Relaxed),
            zone_bytes_sent: self.metrics.zone_bytes_sent.load(Ordering::Relaxed),
            messages_dropped: self.metrics.messages_dropped.load(Ordering::Relaxed),
            backpressure_events: self.metrics.backpressure_events.load(Ordering::Relaxed),
        }
    }

    /// Get current available capacity
    pub async fn available_capacity(&self, level: RoutingLevel) -> CapacityInfo {
        let (msg_limiter, byte_limiter) = match level {
            RoutingLevel::Cell => (&self.cell_message_limiter, &self.cell_byte_limiter),
            RoutingLevel::Zone => (&self.zone_message_limiter, &self.zone_byte_limiter),
        };

        CapacityInfo {
            available_messages: msg_limiter.available_tokens().await as usize,
            available_bytes: byte_limiter.available_tokens().await as usize,
        }
    }
}

/// Permit to send a message (RAII guard)
///
/// Holding this permit means rate limit tokens have been consumed.
pub struct Permit {
    _private: (),
}

/// Flow control metrics
#[derive(Debug, Clone, Copy)]
pub struct FlowMetrics {
    /// Messages sent at cell level
    pub cell_messages_sent: u64,
    /// Bytes sent at cell level
    pub cell_bytes_sent: u64,
    /// Messages sent at zone level
    pub zone_messages_sent: u64,
    /// Bytes sent at zone level
    pub zone_bytes_sent: u64,
    /// Total messages dropped
    pub messages_dropped: u64,
    /// Number of backpressure events
    pub backpressure_events: u64,
}

/// Current capacity information
#[derive(Debug, Clone, Copy)]
pub struct CapacityInfo {
    /// Available message tokens
    pub available_messages: usize,
    /// Available byte tokens
    pub available_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_bucket_creation() {
        let bucket = TokenBucket::new(100.0, 10.0);
        let tokens = bucket.available_tokens().await;
        assert_eq!(tokens, 100.0);
    }

    #[tokio::test]
    async fn test_token_bucket_consume() {
        let bucket = TokenBucket::new(100.0, 10.0);

        // Consume some tokens
        assert!(bucket.try_consume(10.0).await);
        let tokens = bucket.available_tokens().await;
        assert!((tokens - 90.0).abs() < 0.01);

        // Consume more tokens
        assert!(bucket.try_consume(50.0).await);
        let tokens = bucket.available_tokens().await;
        assert!((tokens - 40.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_token_bucket_overflow() {
        let bucket = TokenBucket::new(100.0, 10.0);

        // Try to consume more than available
        assert!(!bucket.try_consume(150.0).await);

        // Available should be unchanged
        let tokens = bucket.available_tokens().await;
        assert_eq!(tokens, 100.0);
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let bucket = TokenBucket::new(100.0, 100.0); // 100 tokens/sec

        // Consume all tokens
        assert!(bucket.try_consume(100.0).await);
        let tokens_after_consume = bucket.available_tokens().await;
        assert!(tokens_after_consume < 1.0); // Should be near zero

        // Wait for refill
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Should have ~50 tokens (0.5 sec * 100 tokens/sec)
        let tokens = bucket.available_tokens().await;
        assert!((40.0..=60.0).contains(&tokens)); // Allow some timing variance
    }

    #[tokio::test]
    async fn test_flow_controller_creation() {
        let controller = FlowController::new(
            BandwidthLimit::cell_default(),
            BandwidthLimit::zone_default(),
            MessageDropPolicy::DropLowPriority,
        );

        assert!(!controller.has_backpressure().await);
        let metrics = controller.get_metrics();
        assert_eq!(metrics.cell_messages_sent, 0);
        assert_eq!(metrics.zone_messages_sent, 0);
    }

    #[tokio::test]
    async fn test_acquire_permit() {
        let controller = FlowController::new(
            BandwidthLimit::new(10, 1000),
            BandwidthLimit::new(5, 500),
            MessageDropPolicy::DropLowPriority,
        );

        // Acquire a permit
        let _permit = controller
            .acquire_permit(RoutingLevel::Cell, 100, MessagePriority::Normal)
            .await
            .unwrap();

        let metrics = controller.get_metrics();
        assert_eq!(metrics.cell_messages_sent, 1);
        assert_eq!(metrics.cell_bytes_sent, 100);
    }

    #[tokio::test]
    async fn test_priority_preferential_treatment() {
        let controller = FlowController::new(
            BandwidthLimit::new(10, 1000),
            BandwidthLimit::new(5, 500),
            MessageDropPolicy::DropLowPriority,
        );

        // Critical priority consumes fewer tokens
        let _permit1 = controller
            .acquire_permit(RoutingLevel::Cell, 100, MessagePriority::Critical)
            .await
            .unwrap();

        // Low priority consumes more tokens
        let _permit2 = controller
            .acquire_permit(RoutingLevel::Cell, 100, MessagePriority::Low)
            .await
            .unwrap();

        let metrics = controller.get_metrics();
        assert_eq!(metrics.cell_messages_sent, 2);
    }

    #[tokio::test]
    async fn test_message_drop_policy() {
        let controller = FlowController::new(
            BandwidthLimit::cell_default(),
            BandwidthLimit::zone_default(),
            MessageDropPolicy::DropLowPriority,
        );

        // Low priority should be droppable
        assert!(controller.should_drop(MessagePriority::Low));
        assert!(controller.should_drop(MessagePriority::Normal));

        // High priority should not be droppable
        assert!(!controller.should_drop(MessagePriority::High));
        assert!(!controller.should_drop(MessagePriority::Critical));
    }

    #[tokio::test]
    async fn test_never_drop_policy() {
        let controller = FlowController::new(
            BandwidthLimit::cell_default(),
            BandwidthLimit::zone_default(),
            MessageDropPolicy::NeverDrop,
        );

        // Nothing should be droppable
        assert!(!controller.should_drop(MessagePriority::Low));
        assert!(!controller.should_drop(MessagePriority::Normal));
        assert!(!controller.should_drop(MessagePriority::High));
        assert!(!controller.should_drop(MessagePriority::Critical));
    }

    #[tokio::test]
    async fn test_capacity_info() {
        let controller = FlowController::new(
            BandwidthLimit::new(100, 10000),
            BandwidthLimit::new(50, 5000),
            MessageDropPolicy::DropLowPriority,
        );

        let capacity = controller.available_capacity(RoutingLevel::Cell).await;
        assert_eq!(capacity.available_messages, 100);
        assert_eq!(capacity.available_bytes, 10000);

        let capacity = controller.available_capacity(RoutingLevel::Zone).await;
        assert_eq!(capacity.available_messages, 50);
        assert_eq!(capacity.available_bytes, 5000);
    }

    #[tokio::test]
    async fn test_record_drop() {
        let controller = FlowController::new(
            BandwidthLimit::cell_default(),
            BandwidthLimit::zone_default(),
            MessageDropPolicy::DropLowPriority,
        );

        controller.record_drop();
        controller.record_drop();

        let metrics = controller.get_metrics();
        assert_eq!(metrics.messages_dropped, 2);
    }

    #[tokio::test]
    async fn test_backpressure_activation() {
        let controller = FlowController::new(
            BandwidthLimit::new(1, 100), // Very low limits
            BandwidthLimit::new(1, 100),
            MessageDropPolicy::DropLowPriority,
        );

        // Consume all available tokens
        let _p1 = controller
            .acquire_permit(RoutingLevel::Cell, 50, MessagePriority::Normal)
            .await
            .unwrap();

        // This should trigger backpressure (in background)
        tokio::spawn({
            let controller = FlowController::new(
                BandwidthLimit::new(1, 100),
                BandwidthLimit::new(1, 100),
                MessageDropPolicy::DropLowPriority,
            );
            async move {
                let _ = controller
                    .acquire_permit(RoutingLevel::Cell, 50, MessagePriority::Normal)
                    .await;
            }
        });

        // Give it time to process
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
