//! QoS-aware retention policies (ADR-019 Phase 4)
//!
//! Defines per-QoS-class retention policies that determine how long data should
//! be retained and when it should be evicted based on storage pressure.
//!
//! # Retention Table (ADR-019)
//!
//! | Class | Min Retain | Max Retain | Evict Priority | Compress |
//! |-------|------------|------------|----------------|----------|
//! | P1 Critical | 7 days | Forever | 5 (last) | No |
//! | P2 High | 24 hours | 7 days | 4 | Yes |
//! | P3 Normal | 1 hour | 24 hours | 3 | Yes |
//! | P4 Low | 5 minutes | 1 hour | 2 | Yes |
//! | P5 Bulk | 1 minute | 5 minutes | 1 (first) | Yes |

use super::QoSClass;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Retention policy for a QoS class
///
/// Defines how long data should be kept and when it can be evicted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// QoS class this policy applies to
    pub qos_class: QoSClass,

    /// Minimum time to keep data (seconds)
    ///
    /// Data younger than this will not be evicted regardless of storage pressure.
    pub min_retain_seconds: u64,

    /// Maximum time to keep data (seconds)
    ///
    /// Data older than this may be automatically evicted during cleanup.
    /// `u64::MAX` means "forever" (never auto-delete).
    pub max_retain_seconds: u64,

    /// Eviction priority (1-5, lower = evict first)
    ///
    /// When storage is full, data with lower eviction priority is evicted first.
    /// P5 (Bulk) has priority 1 (evict first), P1 (Critical) has priority 5 (never evict).
    pub eviction_priority: u8,

    /// Whether this data type can be compressed to save space
    ///
    /// Critical data should not be compressed to ensure fastest access.
    pub compression_eligible: bool,
}

impl RetentionPolicy {
    /// Create retention policy for a QoS class using ADR-019 defaults
    pub fn for_qos_class(class: QoSClass) -> Self {
        match class {
            QoSClass::Critical => Self {
                qos_class: class,
                min_retain_seconds: 7 * 24 * 3600, // 7 days minimum
                max_retain_seconds: u64::MAX,      // Never auto-delete
                eviction_priority: 5,              // Evict last (never in practice)
                compression_eligible: false,       // Never compress critical data
            },
            QoSClass::High => Self {
                qos_class: class,
                min_retain_seconds: 24 * 3600,     // 24 hours minimum
                max_retain_seconds: 7 * 24 * 3600, // 7 days max
                eviction_priority: 4,
                compression_eligible: true,
            },
            QoSClass::Normal => Self {
                qos_class: class,
                min_retain_seconds: 3600,      // 1 hour minimum
                max_retain_seconds: 24 * 3600, // 24 hours max
                eviction_priority: 3,
                compression_eligible: true,
            },
            QoSClass::Low => Self {
                qos_class: class,
                min_retain_seconds: 300,  // 5 minutes minimum
                max_retain_seconds: 3600, // 1 hour max
                eviction_priority: 2,
                compression_eligible: true,
            },
            QoSClass::Bulk => Self {
                qos_class: class,
                min_retain_seconds: 60,  // 1 minute minimum
                max_retain_seconds: 300, // 5 minutes max
                eviction_priority: 1,    // Evict first
                compression_eligible: true,
            },
        }
    }

    /// Create a custom retention policy
    pub fn custom(
        qos_class: QoSClass,
        min_retain: Duration,
        max_retain: Duration,
        eviction_priority: u8,
        compression_eligible: bool,
    ) -> Self {
        Self {
            qos_class,
            min_retain_seconds: min_retain.as_secs(),
            max_retain_seconds: max_retain.as_secs(),
            eviction_priority: eviction_priority.clamp(1, 5),
            compression_eligible,
        }
    }

    /// Check if data should be retained based on age and storage pressure
    ///
    /// Returns `true` if data should be kept, `false` if it can be evicted.
    ///
    /// # Arguments
    /// * `age_seconds` - Age of the data in seconds
    /// * `storage_pressure` - Current storage utilization (0.0 - 1.0)
    pub fn should_retain(&self, age_seconds: u64, storage_pressure: f32) -> bool {
        // Always retain data younger than minimum retention time
        if age_seconds < self.min_retain_seconds {
            return true;
        }

        // Always evict data older than maximum retention time
        if age_seconds > self.max_retain_seconds {
            return false;
        }

        // Between min and max: use storage pressure to decide
        // Higher pressure = more aggressive eviction of lower priority data
        match self.qos_class {
            QoSClass::Critical => true, // Never evict critical data
            QoSClass::High => storage_pressure < 0.95,
            QoSClass::Normal => storage_pressure < 0.85,
            QoSClass::Low => storage_pressure < 0.75,
            QoSClass::Bulk => storage_pressure < 0.65,
        }
    }

    /// Check if data should be evicted based on age and storage pressure
    ///
    /// Inverse of `should_retain`.
    pub fn should_evict(&self, age_seconds: u64, storage_pressure: f32) -> bool {
        !self.should_retain(age_seconds, storage_pressure)
    }

    /// Get minimum retention as Duration
    pub fn min_retain_duration(&self) -> Duration {
        Duration::from_secs(self.min_retain_seconds)
    }

    /// Get maximum retention as Duration
    pub fn max_retain_duration(&self) -> Option<Duration> {
        if self.max_retain_seconds == u64::MAX {
            None
        } else {
            Some(Duration::from_secs(self.max_retain_seconds))
        }
    }

    /// Check if this policy allows infinite retention
    pub fn has_infinite_retention(&self) -> bool {
        self.max_retain_seconds == u64::MAX
    }

    /// Check if this data should be compressed when storage pressure is high
    pub fn should_compress(&self, storage_pressure: f32) -> bool {
        self.compression_eligible && storage_pressure > 0.7
    }
}

/// Collection of retention policies for all QoS classes
#[derive(Debug, Clone)]
pub struct RetentionPolicies {
    policies: [RetentionPolicy; 5],
}

impl RetentionPolicies {
    /// Create default retention policies from ADR-019
    pub fn default_tactical() -> Self {
        Self {
            policies: [
                RetentionPolicy::for_qos_class(QoSClass::Critical),
                RetentionPolicy::for_qos_class(QoSClass::High),
                RetentionPolicy::for_qos_class(QoSClass::Normal),
                RetentionPolicy::for_qos_class(QoSClass::Low),
                RetentionPolicy::for_qos_class(QoSClass::Bulk),
            ],
        }
    }

    /// Create aggressive retention policies for storage-constrained devices
    pub fn storage_constrained() -> Self {
        Self {
            policies: [
                // Critical: still retain for 7 days
                RetentionPolicy::for_qos_class(QoSClass::Critical),
                // High: reduce to 12 hours
                RetentionPolicy::custom(
                    QoSClass::High,
                    Duration::from_secs(12 * 3600),
                    Duration::from_secs(3 * 24 * 3600),
                    4,
                    true,
                ),
                // Normal: reduce to 30 minutes
                RetentionPolicy::custom(
                    QoSClass::Normal,
                    Duration::from_secs(30 * 60),
                    Duration::from_secs(12 * 3600),
                    3,
                    true,
                ),
                // Low: reduce to 2 minutes
                RetentionPolicy::custom(
                    QoSClass::Low,
                    Duration::from_secs(120),
                    Duration::from_secs(30 * 60),
                    2,
                    true,
                ),
                // Bulk: reduce to 30 seconds
                RetentionPolicy::custom(
                    QoSClass::Bulk,
                    Duration::from_secs(30),
                    Duration::from_secs(120),
                    1,
                    true,
                ),
            ],
        }
    }

    /// Get retention policy for a specific QoS class
    pub fn get(&self, class: QoSClass) -> &RetentionPolicy {
        &self.policies[class.as_u8() as usize - 1]
    }

    /// Get mutable retention policy for a specific QoS class
    pub fn get_mut(&mut self, class: QoSClass) -> &mut RetentionPolicy {
        &mut self.policies[class.as_u8() as usize - 1]
    }

    /// Iterate over all policies in eviction priority order (P5 first, P1 last)
    pub fn by_eviction_priority(&self) -> impl Iterator<Item = &RetentionPolicy> {
        // Sort by eviction_priority (lower = evict first)
        let mut sorted: Vec<&RetentionPolicy> = self.policies.iter().collect();
        sorted.sort_by_key(|p| p.eviction_priority);
        sorted.into_iter()
    }

    /// Get classes that should be evicted at given storage pressure, in order
    pub fn eviction_candidates(&self, storage_pressure: f32) -> Vec<QoSClass> {
        let mut candidates = Vec::new();

        // Add classes based on pressure thresholds (P5 first, then P4, etc.)
        for class in [
            QoSClass::Bulk,
            QoSClass::Low,
            QoSClass::Normal,
            QoSClass::High,
        ] {
            let policy = self.get(class);
            // Use a base age to determine if eviction is appropriate
            // At high pressure, even recent data may be evicted
            let age = if storage_pressure > 0.9 {
                policy.min_retain_seconds + 1
            } else {
                policy.max_retain_seconds / 2
            };

            if policy.should_evict(age, storage_pressure) {
                candidates.push(class);
            }
        }

        candidates
    }
}

impl Default for RetentionPolicies {
    fn default() -> Self {
        Self::default_tactical()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_policy_critical() {
        let policy = RetentionPolicy::for_qos_class(QoSClass::Critical);

        assert_eq!(policy.min_retain_seconds, 7 * 24 * 3600); // 7 days
        assert_eq!(policy.max_retain_seconds, u64::MAX); // Forever
        assert_eq!(policy.eviction_priority, 5); // Last
        assert!(!policy.compression_eligible);
        assert!(policy.has_infinite_retention());
    }

    #[test]
    fn test_retention_policy_bulk() {
        let policy = RetentionPolicy::for_qos_class(QoSClass::Bulk);

        assert_eq!(policy.min_retain_seconds, 60); // 1 minute
        assert_eq!(policy.max_retain_seconds, 300); // 5 minutes
        assert_eq!(policy.eviction_priority, 1); // First
        assert!(policy.compression_eligible);
        assert!(!policy.has_infinite_retention());
    }

    #[test]
    fn test_should_retain_respects_min_age() {
        let policy = RetentionPolicy::for_qos_class(QoSClass::Low);

        // Data younger than min (5 min) should always be retained
        assert!(policy.should_retain(60, 0.99)); // 1 min old, high pressure
        assert!(policy.should_retain(200, 0.99)); // ~3 min old, high pressure

        // Data older than min can be evicted based on pressure
        assert!(!policy.should_retain(400, 0.80)); // ~6 min old, moderate pressure
    }

    #[test]
    fn test_should_retain_respects_max_age() {
        let policy = RetentionPolicy::for_qos_class(QoSClass::Normal);

        // Data older than max (24 hours) should always be evicted
        assert!(!policy.should_retain(25 * 3600, 0.0)); // Low pressure, still evict
        assert!(!policy.should_retain(30 * 3600, 0.0)); // Very old
    }

    #[test]
    fn test_critical_never_evicted() {
        let policy = RetentionPolicy::for_qos_class(QoSClass::Critical);

        // Critical data should never be evicted regardless of pressure
        assert!(policy.should_retain(1_000_000, 0.99)); // Very old, high pressure
        assert!(policy.should_retain(10_000_000, 1.0)); // Extremely old, max pressure
    }

    #[test]
    fn test_storage_pressure_thresholds() {
        let policies = RetentionPolicies::default_tactical();

        // P5 Bulk: evict at 65%+
        let bulk = policies.get(QoSClass::Bulk);
        assert!(bulk.should_retain(120, 0.60)); // Below threshold
        assert!(!bulk.should_retain(120, 0.70)); // Above threshold

        // P4 Low: evict at 75%+
        let low = policies.get(QoSClass::Low);
        assert!(low.should_retain(600, 0.70)); // Below threshold
        assert!(!low.should_retain(600, 0.80)); // Above threshold

        // P3 Normal: evict at 85%+
        let normal = policies.get(QoSClass::Normal);
        assert!(normal.should_retain(7200, 0.80)); // Below threshold
        assert!(!normal.should_retain(7200, 0.90)); // Above threshold

        // P2 High: evict at 95%+
        let high = policies.get(QoSClass::High);
        assert!(high.should_retain(86400, 0.90)); // Below threshold
        assert!(!high.should_retain(86400, 0.96)); // Above threshold
    }

    #[test]
    fn test_eviction_candidates() {
        let policies = RetentionPolicies::default_tactical();

        // Low pressure: no eviction
        let candidates = policies.eviction_candidates(0.5);
        assert!(candidates.is_empty());

        // Moderate pressure: start with Bulk
        let candidates = policies.eviction_candidates(0.70);
        assert!(candidates.contains(&QoSClass::Bulk));

        // High pressure: Bulk + Low
        let candidates = policies.eviction_candidates(0.80);
        assert!(candidates.contains(&QoSClass::Bulk));
        assert!(candidates.contains(&QoSClass::Low));

        // Very high pressure: Bulk + Low + Normal
        let candidates = policies.eviction_candidates(0.90);
        assert!(candidates.contains(&QoSClass::Bulk));
        assert!(candidates.contains(&QoSClass::Low));
        assert!(candidates.contains(&QoSClass::Normal));
    }

    #[test]
    fn test_compression_eligibility() {
        let policies = RetentionPolicies::default_tactical();

        // Critical: never compress
        assert!(!policies.get(QoSClass::Critical).should_compress(0.99));

        // Others: compress at high pressure
        assert!(!policies.get(QoSClass::Normal).should_compress(0.50)); // Low pressure
        assert!(policies.get(QoSClass::Normal).should_compress(0.80)); // High pressure
        assert!(policies.get(QoSClass::Bulk).should_compress(0.75));
    }

    #[test]
    fn test_custom_retention_policy() {
        let policy = RetentionPolicy::custom(
            QoSClass::Normal,
            Duration::from_secs(600),  // 10 min
            Duration::from_secs(7200), // 2 hours
            3,
            true,
        );

        assert_eq!(policy.min_retain_seconds, 600);
        assert_eq!(policy.max_retain_seconds, 7200);
        assert_eq!(policy.eviction_priority, 3);
    }

    #[test]
    fn test_storage_constrained_policies() {
        let policies = RetentionPolicies::storage_constrained();

        // Bulk should have very short retention
        let bulk = policies.get(QoSClass::Bulk);
        assert_eq!(bulk.min_retain_seconds, 30); // 30 seconds
        assert_eq!(bulk.max_retain_seconds, 120); // 2 minutes

        // Critical should still be 7 days
        let critical = policies.get(QoSClass::Critical);
        assert_eq!(critical.min_retain_seconds, 7 * 24 * 3600);
    }

    #[test]
    fn test_by_eviction_priority_order() {
        let policies = RetentionPolicies::default_tactical();

        let ordered: Vec<QoSClass> = policies
            .by_eviction_priority()
            .map(|p| p.qos_class)
            .collect();

        // Should be in eviction order: Bulk (1) first, Critical (5) last
        assert_eq!(ordered[0], QoSClass::Bulk);
        assert_eq!(ordered[1], QoSClass::Low);
        assert_eq!(ordered[2], QoSClass::Normal);
        assert_eq!(ordered[3], QoSClass::High);
        assert_eq!(ordered[4], QoSClass::Critical);
    }

    #[test]
    fn test_duration_getters() {
        let policy = RetentionPolicy::for_qos_class(QoSClass::Normal);

        assert_eq!(policy.min_retain_duration(), Duration::from_secs(3600));
        assert_eq!(
            policy.max_retain_duration(),
            Some(Duration::from_secs(24 * 3600))
        );

        let critical = RetentionPolicy::for_qos_class(QoSClass::Critical);
        assert!(critical.max_retain_duration().is_none()); // Infinite
    }

    #[test]
    fn test_eviction_priority_clamping() {
        let policy = RetentionPolicy::custom(
            QoSClass::Normal,
            Duration::from_secs(60),
            Duration::from_secs(3600),
            10, // Out of range, should clamp to 5
            true,
        );
        assert_eq!(policy.eviction_priority, 5);

        let policy2 = RetentionPolicy::custom(
            QoSClass::Normal,
            Duration::from_secs(60),
            Duration::from_secs(3600),
            0, // Out of range, should clamp to 1
            true,
        );
        assert_eq!(policy2.eviction_priority, 1);
    }
}
