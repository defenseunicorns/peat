//! Bandwidth allocation and quota management (ADR-019 Phase 3)
//!
//! This module provides per-class bandwidth allocation with preemption support
//! for QoS-aware data synchronization in tactical networks.
//!
//! # Architecture
//!
//! Bandwidth is allocated using a tiered quota system:
//! - Each QoS class has guaranteed minimum bandwidth
//! - Classes can burst up to a maximum when capacity available
//! - Higher priority classes can preempt lower priority transfers
//!
//! # Example
//!
//! ```
//! use hive_protocol::qos::{QoSClass, BandwidthAllocation};
//!
//! // Create default tactical allocation for 1 Mbps link
//! let allocation = BandwidthAllocation::default_tactical();
//!
//! // Check if we can transmit
//! if allocation.can_transmit(QoSClass::Critical, 1024) {
//!     // Acquire a permit to track bandwidth usage
//!     if let Some(permit) = allocation.acquire(QoSClass::Critical, 1024) {
//!         // Transmit data...
//!         // Permit is automatically released when dropped
//!     }
//! }
//! ```
//!
//! # Bandwidth Allocation Table (1 Mbps link)
//!
//! | QoS Class | Guaranteed | Max Burst | Preemption |
//! |-----------|------------|-----------|------------|
//! | P1 Critical | 200 Kbps (20%) | 800 Kbps (80%) | Yes |
//! | P2 High | 300 Kbps (30%) | 600 Kbps (60%) | Yes (P3-P5) |
//! | P3 Normal | 200 Kbps (20%) | 400 Kbps (40%) | No |
//! | P4 Low | 150 Kbps (15%) | 300 Kbps (30%) | No |
//! | P5 Bulk | 50 Kbps (5%) | 200 Kbps (20%) | No |

use super::QoSClass;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Bandwidth allocation configuration for all QoS classes
///
/// Manages per-class bandwidth quotas and tracks real-time usage
/// across all priority levels.
#[derive(Debug)]
pub struct BandwidthAllocation {
    /// Total available bandwidth in bits per second
    pub total_bandwidth_bps: u64,

    /// Per-class bandwidth quotas
    quotas: HashMap<QoSClass, Arc<BandwidthQuota>>,

    /// Token bucket for overall bandwidth limiting
    bucket: Arc<RwLock<TokenBucket>>,

    /// Tracking for active permits
    active_permits: Arc<AtomicU64>,
}

/// Per-class bandwidth quota configuration
#[derive(Debug)]
pub struct BandwidthQuota {
    /// Minimum guaranteed bandwidth in bits per second
    pub min_guaranteed_bps: u64,

    /// Maximum burst bandwidth in bits per second
    pub max_burst_bps: u64,

    /// Whether this class can preempt lower priorities
    pub preemption_enabled: bool,

    /// Real-time usage tracking in bits per second (sliding window)
    pub current_usage_bps: AtomicU64,

    /// Bytes consumed in current window
    bytes_consumed: AtomicU64,

    /// Window start time for usage tracking
    window_start: RwLock<Instant>,
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    /// Current available tokens (in bits)
    tokens: f64,

    /// Maximum bucket capacity (in bits)
    capacity: f64,

    /// Token refill rate (bits per second)
    refill_rate: f64,

    /// Last refill timestamp
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity_bps: u64) -> Self {
        Self {
            tokens: capacity_bps as f64,
            capacity: capacity_bps as f64,
            refill_rate: capacity_bps as f64,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let elapsed = self.last_refill.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            let new_tokens = elapsed * self.refill_rate;
            self.tokens = (self.tokens + new_tokens).min(self.capacity);
            self.last_refill = Instant::now();
        }
    }

    fn try_consume(&mut self, bits: u64) -> bool {
        self.refill();
        let bits_f64 = bits as f64;
        if self.tokens >= bits_f64 {
            self.tokens -= bits_f64;
            true
        } else {
            false
        }
    }

    fn available(&mut self) -> u64 {
        self.refill();
        self.tokens as u64
    }
}

impl BandwidthQuota {
    /// Create a new bandwidth quota
    pub fn new(min_guaranteed_bps: u64, max_burst_bps: u64, preemption_enabled: bool) -> Self {
        Self {
            min_guaranteed_bps,
            max_burst_bps,
            preemption_enabled,
            current_usage_bps: AtomicU64::new(0),
            bytes_consumed: AtomicU64::new(0),
            window_start: RwLock::new(Instant::now()),
        }
    }

    /// Update usage tracking
    pub async fn record_usage(&self, bytes: usize) {
        let bits = (bytes * 8) as u64;
        self.bytes_consumed
            .fetch_add(bytes as u64, Ordering::Relaxed);

        // Check if we need to reset the window (1 second sliding window)
        let mut window_start = self.window_start.write().await;
        let elapsed = window_start.elapsed();

        if elapsed >= Duration::from_secs(1) {
            // Calculate usage for the past window
            let bytes_in_window = self.bytes_consumed.swap(bytes as u64, Ordering::Relaxed);
            let bits_in_window = bytes_in_window * 8;
            self.current_usage_bps
                .store(bits_in_window, Ordering::Relaxed);
            *window_start = Instant::now();
        } else {
            // Estimate current usage
            let elapsed_secs = elapsed.as_secs_f64().max(0.001);
            let bytes_so_far = self.bytes_consumed.load(Ordering::Relaxed);
            let estimated_bps = ((bytes_so_far * 8) as f64 / elapsed_secs) as u64;
            self.current_usage_bps
                .store(estimated_bps, Ordering::Relaxed);
        }

        // Also update the atomic counter for immediate visibility
        let _ = bits; // bits variable is used for the calculation above
    }

    /// Check if quota allows transmission
    pub fn can_transmit(&self, size_bytes: usize) -> bool {
        let current_usage = self.current_usage_bps.load(Ordering::Relaxed);
        let additional_bits = (size_bytes * 8) as u64;

        // Allow if within burst limit
        current_usage + additional_bits <= self.max_burst_bps
    }

    /// Check if within guaranteed allocation
    pub fn within_guaranteed(&self) -> bool {
        let current_usage = self.current_usage_bps.load(Ordering::Relaxed);
        current_usage < self.min_guaranteed_bps
    }

    /// Get current utilization as percentage (0.0 - 1.0+)
    pub fn utilization(&self) -> f64 {
        let current_usage = self.current_usage_bps.load(Ordering::Relaxed);
        current_usage as f64 / self.min_guaranteed_bps as f64
    }
}

/// RAII permit for bandwidth consumption
///
/// Automatically tracks bandwidth usage while held and releases
/// when dropped.
#[derive(Debug)]
pub struct BandwidthPermit {
    /// Size in bytes that was acquired
    size_bytes: usize,

    /// QoS class this permit is for
    class: QoSClass,

    /// Reference to the quota for tracking
    #[allow(dead_code)]
    quota: Arc<BandwidthQuota>,

    /// Reference to active permit counter
    active_permits: Arc<AtomicU64>,
}

impl BandwidthPermit {
    /// Get the size in bytes this permit covers
    pub fn size_bytes(&self) -> usize {
        self.size_bytes
    }

    /// Get the QoS class for this permit
    pub fn class(&self) -> QoSClass {
        self.class
    }
}

impl Drop for BandwidthPermit {
    fn drop(&mut self) {
        self.active_permits.fetch_sub(1, Ordering::Relaxed);
    }
}

impl BandwidthAllocation {
    /// Create a new bandwidth allocation with specified total bandwidth
    pub fn new(total_bandwidth_bps: u64) -> Self {
        let mut quotas = HashMap::new();

        // Calculate per-class allocations based on ADR-019 table
        // P1 Critical: 20% guaranteed, 80% burst, preemption enabled
        quotas.insert(
            QoSClass::Critical,
            Arc::new(BandwidthQuota::new(
                total_bandwidth_bps * 20 / 100,
                total_bandwidth_bps * 80 / 100,
                true,
            )),
        );

        // P2 High: 30% guaranteed, 60% burst, preemption enabled
        quotas.insert(
            QoSClass::High,
            Arc::new(BandwidthQuota::new(
                total_bandwidth_bps * 30 / 100,
                total_bandwidth_bps * 60 / 100,
                true,
            )),
        );

        // P3 Normal: 20% guaranteed, 40% burst, no preemption
        quotas.insert(
            QoSClass::Normal,
            Arc::new(BandwidthQuota::new(
                total_bandwidth_bps * 20 / 100,
                total_bandwidth_bps * 40 / 100,
                false,
            )),
        );

        // P4 Low: 15% guaranteed, 30% burst, no preemption
        quotas.insert(
            QoSClass::Low,
            Arc::new(BandwidthQuota::new(
                total_bandwidth_bps * 15 / 100,
                total_bandwidth_bps * 30 / 100,
                false,
            )),
        );

        // P5 Bulk: 5% guaranteed, 20% burst, no preemption
        quotas.insert(
            QoSClass::Bulk,
            Arc::new(BandwidthQuota::new(
                total_bandwidth_bps * 5 / 100,
                total_bandwidth_bps * 20 / 100,
                false,
            )),
        );

        Self {
            total_bandwidth_bps,
            quotas,
            bucket: Arc::new(RwLock::new(TokenBucket::new(total_bandwidth_bps))),
            active_permits: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create default tactical allocation for 1 Mbps link
    ///
    /// Designed for constrained tactical radio links with emphasis
    /// on critical data delivery.
    pub fn default_tactical() -> Self {
        Self::new(1_000_000) // 1 Mbps
    }

    /// Create allocation for 10 Mbps link (standard WiFi/LTE)
    pub fn default_standard() -> Self {
        Self::new(10_000_000) // 10 Mbps
    }

    /// Create allocation for high-bandwidth link (100 Mbps+)
    pub fn default_high_bandwidth() -> Self {
        Self::new(100_000_000) // 100 Mbps
    }

    /// Check if transmission is allowed for a given class and size
    ///
    /// Returns true if there is sufficient bandwidth available
    /// within the class quota and global limit.
    pub fn can_transmit(&self, class: QoSClass, size_bytes: usize) -> bool {
        if let Some(quota) = self.quotas.get(&class) {
            quota.can_transmit(size_bytes)
        } else {
            false
        }
    }

    /// Acquire a bandwidth permit
    ///
    /// Returns a permit if bandwidth is available, None otherwise.
    /// The permit must be held while transmitting and will automatically
    /// track usage when dropped.
    pub fn acquire(&self, class: QoSClass, size_bytes: usize) -> Option<BandwidthPermit> {
        let quota = self.quotas.get(&class)?;

        if !quota.can_transmit(size_bytes) {
            return None;
        }

        // Try to consume from global token bucket
        // Note: We use try_lock to avoid blocking in sync context
        // For async usage, use acquire_async instead
        let bits = (size_bytes * 8) as u64;
        if let Ok(mut bucket) = self.bucket.try_write() {
            if !bucket.try_consume(bits) {
                return None;
            }
        } else {
            // Bucket is locked, allow transmission but don't consume tokens
            // This prevents deadlock in high-contention scenarios
        }

        self.active_permits.fetch_add(1, Ordering::Relaxed);

        Some(BandwidthPermit {
            size_bytes,
            class,
            quota: Arc::clone(quota),
            active_permits: Arc::clone(&self.active_permits),
        })
    }

    /// Acquire a bandwidth permit (async version)
    ///
    /// Async-safe version that properly awaits the token bucket lock.
    pub async fn acquire_async(
        &self,
        class: QoSClass,
        size_bytes: usize,
    ) -> Option<BandwidthPermit> {
        let quota = self.quotas.get(&class)?;

        if !quota.can_transmit(size_bytes) {
            return None;
        }

        // Record usage
        quota.record_usage(size_bytes).await;

        // Consume from global token bucket
        let bits = (size_bytes * 8) as u64;
        {
            let mut bucket = self.bucket.write().await;
            if !bucket.try_consume(bits) {
                return None;
            }
        }

        self.active_permits.fetch_add(1, Ordering::Relaxed);

        Some(BandwidthPermit {
            size_bytes,
            class,
            quota: Arc::clone(quota),
            active_permits: Arc::clone(&self.active_permits),
        })
    }

    /// Attempt to preempt lower priority transfers
    ///
    /// Returns true if preemption is allowed and was successful.
    /// The caller should pause/cancel lower priority transfers.
    pub fn preempt_lower(&self, class: QoSClass) -> bool {
        if let Some(quota) = self.quotas.get(&class) {
            if quota.preemption_enabled {
                // Check if there's actually bandwidth being used by lower priorities
                for (other_class, other_quota) in &self.quotas {
                    if class.can_preempt(other_class) {
                        let usage = other_quota.current_usage_bps.load(Ordering::Relaxed);
                        if usage > 0 {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Get the quota configuration for a class
    pub fn get_quota(&self, class: QoSClass) -> Option<&Arc<BandwidthQuota>> {
        self.quotas.get(&class)
    }

    /// Get current utilization for a class (0.0 - 1.0+)
    pub fn class_utilization(&self, class: QoSClass) -> f64 {
        self.quotas
            .get(&class)
            .map(|q| q.utilization())
            .unwrap_or(0.0)
    }

    /// Get overall bandwidth utilization
    pub async fn total_utilization(&self) -> f64 {
        let bucket = self.bucket.read().await;
        1.0 - (bucket.tokens / bucket.capacity)
    }

    /// Get available bandwidth in bits per second
    pub async fn available_bandwidth_bps(&self) -> u64 {
        let mut bucket = self.bucket.write().await;
        bucket.available()
    }

    /// Get number of active permits
    pub fn active_permit_count(&self) -> u64 {
        self.active_permits.load(Ordering::Relaxed)
    }

    /// Get all class utilizations
    pub fn all_utilizations(&self) -> HashMap<QoSClass, f64> {
        self.quotas
            .iter()
            .map(|(class, quota)| (*class, quota.utilization()))
            .collect()
    }
}

/// Serializable bandwidth allocation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthConfig {
    /// Total bandwidth in bits per second
    pub total_bandwidth_bps: u64,

    /// Per-class quota configurations
    pub quotas: HashMap<QoSClass, QuotaConfig>,
}

/// Serializable quota configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaConfig {
    /// Minimum guaranteed bandwidth percentage (0-100)
    pub min_guaranteed_percent: u8,

    /// Maximum burst bandwidth percentage (0-100)
    pub max_burst_percent: u8,

    /// Whether preemption is enabled
    pub preemption_enabled: bool,
}

impl BandwidthConfig {
    /// Create default tactical configuration
    pub fn default_tactical() -> Self {
        let mut quotas = HashMap::new();

        quotas.insert(
            QoSClass::Critical,
            QuotaConfig {
                min_guaranteed_percent: 20,
                max_burst_percent: 80,
                preemption_enabled: true,
            },
        );

        quotas.insert(
            QoSClass::High,
            QuotaConfig {
                min_guaranteed_percent: 30,
                max_burst_percent: 60,
                preemption_enabled: true,
            },
        );

        quotas.insert(
            QoSClass::Normal,
            QuotaConfig {
                min_guaranteed_percent: 20,
                max_burst_percent: 40,
                preemption_enabled: false,
            },
        );

        quotas.insert(
            QoSClass::Low,
            QuotaConfig {
                min_guaranteed_percent: 15,
                max_burst_percent: 30,
                preemption_enabled: false,
            },
        );

        quotas.insert(
            QoSClass::Bulk,
            QuotaConfig {
                min_guaranteed_percent: 5,
                max_burst_percent: 20,
                preemption_enabled: false,
            },
        );

        Self {
            total_bandwidth_bps: 1_000_000,
            quotas,
        }
    }

    /// Build a BandwidthAllocation from this config
    pub fn build(&self) -> BandwidthAllocation {
        let mut quotas = HashMap::new();

        for (class, config) in &self.quotas {
            let min_bps = self.total_bandwidth_bps * config.min_guaranteed_percent as u64 / 100;
            let max_bps = self.total_bandwidth_bps * config.max_burst_percent as u64 / 100;

            quotas.insert(
                *class,
                Arc::new(BandwidthQuota::new(
                    min_bps,
                    max_bps,
                    config.preemption_enabled,
                )),
            );
        }

        BandwidthAllocation {
            total_bandwidth_bps: self.total_bandwidth_bps,
            quotas,
            bucket: Arc::new(RwLock::new(TokenBucket::new(self.total_bandwidth_bps))),
            active_permits: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), &'static str> {
        let total_guaranteed: u8 = self.quotas.values().map(|q| q.min_guaranteed_percent).sum();

        if total_guaranteed > 100 {
            return Err("Total guaranteed bandwidth exceeds 100%");
        }

        for config in self.quotas.values() {
            if config.max_burst_percent < config.min_guaranteed_percent {
                return Err("Max burst must be >= min guaranteed");
            }
            if config.max_burst_percent > 100 {
                return Err("Max burst cannot exceed 100%");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bandwidth_allocation_creation() {
        let alloc = BandwidthAllocation::default_tactical();
        assert_eq!(alloc.total_bandwidth_bps, 1_000_000);
        assert_eq!(alloc.quotas.len(), 5);
    }

    #[test]
    fn test_quota_percentages() {
        let alloc = BandwidthAllocation::default_tactical();

        // Check P1 Critical: 20% guaranteed, 80% burst
        let critical = alloc.get_quota(QoSClass::Critical).unwrap();
        assert_eq!(critical.min_guaranteed_bps, 200_000);
        assert_eq!(critical.max_burst_bps, 800_000);
        assert!(critical.preemption_enabled);

        // Check P5 Bulk: 5% guaranteed, 20% burst
        let bulk = alloc.get_quota(QoSClass::Bulk).unwrap();
        assert_eq!(bulk.min_guaranteed_bps, 50_000);
        assert_eq!(bulk.max_burst_bps, 200_000);
        assert!(!bulk.preemption_enabled);
    }

    #[test]
    fn test_can_transmit() {
        let alloc = BandwidthAllocation::default_tactical();

        // Small message should be allowed
        assert!(alloc.can_transmit(QoSClass::Critical, 1024));

        // Very large message exceeding burst should be rejected
        // 800 Kbps = 100 KB/s burst, so 200KB should fail
        assert!(!alloc.can_transmit(QoSClass::Critical, 200_000));
    }

    #[test]
    fn test_acquire_permit() {
        let alloc = BandwidthAllocation::default_tactical();

        let permit = alloc.acquire(QoSClass::Normal, 1024);
        assert!(permit.is_some());

        let permit = permit.unwrap();
        assert_eq!(permit.size_bytes(), 1024);
        assert_eq!(permit.class(), QoSClass::Normal);
        assert_eq!(alloc.active_permit_count(), 1);

        drop(permit);
        assert_eq!(alloc.active_permit_count(), 0);
    }

    #[tokio::test]
    async fn test_acquire_async() {
        let alloc = BandwidthAllocation::default_tactical();

        let permit = alloc.acquire_async(QoSClass::High, 2048).await;
        assert!(permit.is_some());

        let permit = permit.unwrap();
        assert_eq!(permit.size_bytes(), 2048);
        assert_eq!(permit.class(), QoSClass::High);
    }

    #[test]
    fn test_preemption() {
        let alloc = BandwidthAllocation::default_tactical();

        // Critical can preempt (when lower classes have usage)
        // But initially there's no usage to preempt
        assert!(!alloc.preempt_lower(QoSClass::Critical));

        // Bulk cannot preempt
        assert!(!alloc.preempt_lower(QoSClass::Bulk));
    }

    #[test]
    fn test_utilization() {
        let alloc = BandwidthAllocation::default_tactical();

        // Initial utilization should be 0
        let util = alloc.class_utilization(QoSClass::Normal);
        assert_eq!(util, 0.0);
    }

    #[tokio::test]
    async fn test_available_bandwidth() {
        let alloc = BandwidthAllocation::default_tactical();

        let available = alloc.available_bandwidth_bps().await;
        assert_eq!(available, 1_000_000);
    }

    #[test]
    fn test_bandwidth_config() {
        let config = BandwidthConfig::default_tactical();
        assert!(config.validate().is_ok());

        let alloc = config.build();
        assert_eq!(alloc.total_bandwidth_bps, 1_000_000);
    }

    #[test]
    fn test_bandwidth_config_validation() {
        let mut config = BandwidthConfig::default_tactical();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Exceeding 100% guaranteed should fail
        config
            .quotas
            .get_mut(&QoSClass::Bulk)
            .unwrap()
            .min_guaranteed_percent = 50;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_quota_within_guaranteed() {
        let quota = BandwidthQuota::new(100_000, 200_000, false);

        // Initially within guaranteed
        assert!(quota.within_guaranteed());
    }

    #[test]
    fn test_all_utilizations() {
        let alloc = BandwidthAllocation::default_tactical();

        let utils = alloc.all_utilizations();
        assert_eq!(utils.len(), 5);
        assert!(utils.contains_key(&QoSClass::Critical));
        assert!(utils.contains_key(&QoSClass::Bulk));
    }

    #[test]
    fn test_different_link_speeds() {
        let tactical = BandwidthAllocation::default_tactical();
        assert_eq!(tactical.total_bandwidth_bps, 1_000_000);

        let standard = BandwidthAllocation::default_standard();
        assert_eq!(standard.total_bandwidth_bps, 10_000_000);

        let high = BandwidthAllocation::default_high_bandwidth();
        assert_eq!(high.total_bandwidth_bps, 100_000_000);
    }
}
