//! Combined QoS + TTL lifecycle management (ADR-019 Phase 4 + ADR-016)
//!
//! Integrates QoS-aware storage management with the TTL lifecycle system
//! from ADR-016, providing unified eviction decisions that consider both
//! priority and time-to-live.
//!
//! # Decision Matrix
//!
//! | Storage Pressure | TTL Status | QoS Class | Action |
//! |------------------|------------|-----------|--------|
//! | Any | Expired | Any except P1 | Evict |
//! | Any | Valid | P1 Critical | Retain |
//! | > 95% | Valid | P2 High | May evict |
//! | > 85% | Valid | P3 Normal | May evict |
//! | > 75% | Valid | P4 Low | May evict |
//! | > 65% | Valid | P5 Bulk | May evict |

use super::{retention::RetentionPolicy, QoSClass, QoSPolicy};
use crate::storage::ttl::TtlConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Combined lifecycle policy integrating QoS and TTL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecyclePolicy {
    /// QoS policy (priority, latency, retention)
    pub qos_policy: QoSPolicy,

    /// TTL configuration from ADR-016
    pub ttl_seconds: Option<u64>,

    /// Soft-delete threshold (for high-churn data like beacons)
    pub soft_delete_after_seconds: Option<u64>,

    /// Whether to use soft-delete pattern (avoids husking)
    pub use_soft_delete: bool,
}

impl LifecyclePolicy {
    /// Create a lifecycle policy from QoS class using defaults
    pub fn for_qos_class(class: QoSClass) -> Self {
        let qos_policy = match class {
            QoSClass::Critical => QoSPolicy::critical(),
            QoSClass::High => QoSPolicy::high(),
            QoSClass::Normal => QoSPolicy::default(),
            QoSClass::Low => QoSPolicy::low(),
            QoSClass::Bulk => QoSPolicy::bulk(),
        };

        let (ttl, soft_delete_after, use_soft_delete) = match class {
            QoSClass::Critical => (None, None, false), // Never expire
            QoSClass::High => (Some(7 * 24 * 3600), Some(24 * 3600), false), // 7 days TTL
            QoSClass::Normal => (Some(24 * 3600), Some(3600), false), // 24 hours TTL
            QoSClass::Low => (Some(3600), Some(300), true), // 1 hour TTL, soft-delete pattern
            QoSClass::Bulk => (Some(300), Some(60), true), // 5 min TTL, soft-delete pattern
        };

        Self {
            qos_policy,
            ttl_seconds: ttl,
            soft_delete_after_seconds: soft_delete_after,
            use_soft_delete,
        }
    }

    /// Create a lifecycle policy from QoS policy and TTL config
    pub fn from_qos_and_ttl(qos_policy: QoSPolicy, ttl_config: &TtlConfig) -> Self {
        let ttl_seconds = qos_policy.ttl_seconds;

        // Use soft-delete for low/bulk priority data (high churn)
        let use_soft_delete = matches!(qos_policy.base_class, QoSClass::Low | QoSClass::Bulk);

        // Soft-delete threshold based on TTL config
        let soft_delete_after_seconds = if use_soft_delete {
            Some(ttl_config.position_ttl.as_secs())
        } else {
            None
        };

        Self {
            qos_policy,
            ttl_seconds,
            soft_delete_after_seconds,
            use_soft_delete,
        }
    }

    /// Create a custom lifecycle policy
    pub fn custom(
        qos_policy: QoSPolicy,
        ttl: Option<Duration>,
        soft_delete_after: Option<Duration>,
        use_soft_delete: bool,
    ) -> Self {
        Self {
            qos_policy,
            ttl_seconds: ttl.map(|d| d.as_secs()),
            soft_delete_after_seconds: soft_delete_after.map(|d| d.as_secs()),
            use_soft_delete,
        }
    }

    /// Determine if data should be evicted based on combined QoS + TTL policy
    ///
    /// This is the primary decision function for lifecycle management.
    ///
    /// # Arguments
    /// * `age_seconds` - Age of the data in seconds
    /// * `storage_pressure` - Current storage utilization (0.0 - 1.0)
    ///
    /// # Returns
    /// * `true` if data should be evicted
    /// * `false` if data should be retained
    pub fn should_evict(&self, age_seconds: u64, storage_pressure: f32) -> bool {
        // Rule 1: Critical data is NEVER evicted regardless of age or pressure
        if self.qos_policy.base_class == QoSClass::Critical {
            return false;
        }

        // Rule 2: Data exceeding TTL should be evicted (except critical)
        if let Some(ttl) = self.ttl_seconds {
            if age_seconds > ttl {
                return true;
            }
        }

        // Rule 3: Storage pressure-based eviction
        // Higher priority data requires higher pressure to evict
        let eviction_threshold = match self.qos_policy.base_class {
            QoSClass::Critical => f32::MAX, // Never
            QoSClass::High => 0.95,
            QoSClass::Normal => 0.85,
            QoSClass::Low => 0.75,
            QoSClass::Bulk => 0.65,
        };

        if storage_pressure > eviction_threshold {
            // Check minimum retention from QoS policy
            let retention = RetentionPolicy::for_qos_class(self.qos_policy.base_class);
            if age_seconds >= retention.min_retain_seconds {
                return true;
            }
        }

        false
    }

    /// Determine if data should be soft-deleted
    ///
    /// For high-churn data (positions, heartbeats), soft-delete is preferred
    /// to avoid CRDT husking issues.
    pub fn should_soft_delete(&self, age_seconds: u64) -> bool {
        if !self.use_soft_delete {
            return false;
        }

        if let Some(threshold) = self.soft_delete_after_seconds {
            return age_seconds > threshold;
        }

        false
    }

    /// Determine if data should be retained
    ///
    /// Inverse of `should_evict` with additional retention guarantees.
    pub fn should_retain(&self, age_seconds: u64, storage_pressure: f32) -> bool {
        !self.should_evict(age_seconds, storage_pressure)
    }

    /// Check if this policy allows infinite retention
    pub fn has_infinite_retention(&self) -> bool {
        self.qos_policy.base_class == QoSClass::Critical || self.ttl_seconds.is_none()
    }

    /// Get TTL as Duration
    pub fn ttl_duration(&self) -> Option<Duration> {
        self.ttl_seconds.map(Duration::from_secs)
    }

    /// Get soft-delete threshold as Duration
    pub fn soft_delete_duration(&self) -> Option<Duration> {
        self.soft_delete_after_seconds.map(Duration::from_secs)
    }

    /// Get the QoS class
    pub fn qos_class(&self) -> QoSClass {
        self.qos_policy.base_class
    }
}

/// Collection of lifecycle policies for common data types
#[derive(Debug, Clone)]
pub struct LifecyclePolicies {
    /// Policy for contact reports (P1 Critical)
    pub contact_report: LifecyclePolicy,

    /// Policy for images/media (P2 High)
    pub media: LifecyclePolicy,

    /// Policy for health status (P3 Normal)
    pub health_status: LifecyclePolicy,

    /// Policy for position updates (P4 Low)
    pub position: LifecyclePolicy,

    /// Policy for bulk data (P5 Bulk)
    pub bulk: LifecyclePolicy,
}

impl LifecyclePolicies {
    /// Create default tactical lifecycle policies
    pub fn default_tactical() -> Self {
        Self {
            contact_report: LifecyclePolicy::for_qos_class(QoSClass::Critical),
            media: LifecyclePolicy::for_qos_class(QoSClass::High),
            health_status: LifecyclePolicy::for_qos_class(QoSClass::Normal),
            position: LifecyclePolicy::for_qos_class(QoSClass::Low),
            bulk: LifecyclePolicy::for_qos_class(QoSClass::Bulk),
        }
    }

    /// Create lifecycle policies from TTL config
    pub fn from_ttl_config(ttl_config: &TtlConfig) -> Self {
        Self {
            contact_report: LifecyclePolicy::custom(
                QoSPolicy::critical(),
                None, // No TTL for critical data
                None,
                false,
            ),
            media: LifecyclePolicy::custom(
                QoSPolicy::high(),
                Some(Duration::from_secs(7 * 24 * 3600)),
                Some(Duration::from_secs(24 * 3600)),
                false,
            ),
            health_status: LifecyclePolicy::custom(
                QoSPolicy::default(),
                Some(Duration::from_secs(24 * 3600)),
                Some(Duration::from_secs(3600)),
                false,
            ),
            position: LifecyclePolicy::custom(
                QoSPolicy::low(),
                Some(ttl_config.position_ttl),
                Some(Duration::from_secs(ttl_config.position_ttl.as_secs() / 2)),
                true, // Use soft-delete for positions
            ),
            bulk: LifecyclePolicy::custom(
                QoSPolicy::bulk(),
                Some(Duration::from_secs(300)),
                Some(Duration::from_secs(60)),
                true, // Use soft-delete for bulk
            ),
        }
    }

    /// Get policy for a QoS class
    pub fn get(&self, class: QoSClass) -> &LifecyclePolicy {
        match class {
            QoSClass::Critical => &self.contact_report,
            QoSClass::High => &self.media,
            QoSClass::Normal => &self.health_status,
            QoSClass::Low => &self.position,
            QoSClass::Bulk => &self.bulk,
        }
    }
}

impl Default for LifecyclePolicies {
    fn default() -> Self {
        Self::default_tactical()
    }
}

/// Lifecycle decision for a specific document
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleDecision {
    /// Document should be retained
    Retain,
    /// Document should be soft-deleted (mark as deleted, don't remove)
    SoftDelete,
    /// Document should be evicted (hard delete)
    Evict,
    /// Document TTL has expired, should be removed
    TtlExpired,
}

impl LifecycleDecision {
    /// Check if this decision results in data removal
    pub fn removes_data(&self) -> bool {
        matches!(self, Self::Evict | Self::TtlExpired)
    }

    /// Check if this decision results in data retention
    pub fn retains_data(&self) -> bool {
        matches!(self, Self::Retain)
    }
}

impl std::fmt::Display for LifecycleDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retain => write!(f, "RETAIN"),
            Self::SoftDelete => write!(f, "SOFT_DELETE"),
            Self::Evict => write!(f, "EVICT"),
            Self::TtlExpired => write!(f, "TTL_EXPIRED"),
        }
    }
}

/// Make a lifecycle decision for a document
///
/// Considers both QoS policy and TTL to determine the appropriate action.
pub fn make_lifecycle_decision(
    policy: &LifecyclePolicy,
    age_seconds: u64,
    storage_pressure: f32,
) -> LifecycleDecision {
    // Check TTL expiration first
    if let Some(ttl) = policy.ttl_seconds {
        if age_seconds > ttl && policy.qos_policy.base_class != QoSClass::Critical {
            return LifecycleDecision::TtlExpired;
        }
    }

    // Check for soft-delete eligibility
    if policy.should_soft_delete(age_seconds) {
        return LifecycleDecision::SoftDelete;
    }

    // Check for eviction based on storage pressure
    if policy.should_evict(age_seconds, storage_pressure) {
        return LifecycleDecision::Evict;
    }

    LifecycleDecision::Retain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_policy_critical() {
        let policy = LifecyclePolicy::for_qos_class(QoSClass::Critical);

        // Critical data should never be evicted
        assert!(!policy.should_evict(1_000_000, 0.99));
        assert!(!policy.should_evict(10_000_000, 1.0));
        assert!(policy.has_infinite_retention());
        assert!(!policy.use_soft_delete);
    }

    #[test]
    fn test_lifecycle_policy_bulk() {
        let policy = LifecyclePolicy::for_qos_class(QoSClass::Bulk);

        // Bulk data uses soft-delete pattern
        assert!(policy.use_soft_delete);
        assert!(policy.soft_delete_after_seconds.is_some());

        // Should be evicted at moderate pressure after TTL
        assert!(policy.should_evict(400, 0.70)); // After 5min TTL
        assert!(!policy.has_infinite_retention());
    }

    #[test]
    fn test_ttl_based_eviction() {
        let policy = LifecyclePolicy::custom(
            QoSPolicy::default(),
            Some(Duration::from_secs(3600)), // 1 hour TTL
            None,
            false,
        );

        // Before TTL: don't evict unless pressure
        assert!(!policy.should_evict(1800, 0.50)); // 30 min, low pressure

        // After TTL: evict
        assert!(policy.should_evict(4000, 0.50)); // Past TTL
    }

    #[test]
    fn test_pressure_based_eviction() {
        let policy = LifecyclePolicy::for_qos_class(QoSClass::Normal);

        // Below threshold: retain
        assert!(!policy.should_evict(7200, 0.80)); // 2hr, below 85%

        // Above threshold with sufficient age: evict
        assert!(policy.should_evict(7200, 0.90)); // 2hr, above 85%
    }

    #[test]
    fn test_soft_delete_decision() {
        let policy = LifecyclePolicy::for_qos_class(QoSClass::Low);

        // Before soft-delete threshold
        assert!(!policy.should_soft_delete(200));

        // After soft-delete threshold (300s for Low)
        assert!(policy.should_soft_delete(400));
    }

    #[test]
    fn test_make_lifecycle_decision() {
        // Critical data: always retain
        let critical = LifecyclePolicy::for_qos_class(QoSClass::Critical);
        assert_eq!(
            make_lifecycle_decision(&critical, 1_000_000, 0.99),
            LifecycleDecision::Retain
        );

        // Bulk data past TTL: expired
        let bulk = LifecyclePolicy::for_qos_class(QoSClass::Bulk);
        assert_eq!(
            make_lifecycle_decision(&bulk, 400, 0.50),
            LifecycleDecision::TtlExpired
        );

        // Low data past soft-delete threshold: soft delete
        let low = LifecyclePolicy::for_qos_class(QoSClass::Low);
        assert_eq!(
            make_lifecycle_decision(&low, 400, 0.50),
            LifecycleDecision::SoftDelete
        );
    }

    #[test]
    fn test_lifecycle_decision_helpers() {
        assert!(LifecycleDecision::Evict.removes_data());
        assert!(LifecycleDecision::TtlExpired.removes_data());
        assert!(!LifecycleDecision::Retain.removes_data());
        assert!(!LifecycleDecision::SoftDelete.removes_data());

        assert!(LifecycleDecision::Retain.retains_data());
        assert!(!LifecycleDecision::Evict.retains_data());
    }

    #[test]
    fn test_lifecycle_policies_default() {
        let policies = LifecyclePolicies::default_tactical();

        assert_eq!(
            policies.get(QoSClass::Critical).qos_class(),
            QoSClass::Critical
        );
        assert_eq!(policies.get(QoSClass::High).qos_class(), QoSClass::High);
        assert_eq!(policies.get(QoSClass::Normal).qos_class(), QoSClass::Normal);
        assert_eq!(policies.get(QoSClass::Low).qos_class(), QoSClass::Low);
        assert_eq!(policies.get(QoSClass::Bulk).qos_class(), QoSClass::Bulk);
    }

    #[test]
    fn test_lifecycle_policies_from_ttl_config() {
        let ttl_config = TtlConfig::tactical();
        let policies = LifecyclePolicies::from_ttl_config(&ttl_config);

        // Position should use TTL from config
        assert!(policies.position.use_soft_delete);
        assert_eq!(
            policies.position.ttl_seconds,
            Some(ttl_config.position_ttl.as_secs())
        );
    }

    #[test]
    fn test_duration_getters() {
        let policy = LifecyclePolicy::for_qos_class(QoSClass::Normal);

        assert!(policy.ttl_duration().is_some());
        assert_eq!(policy.ttl_duration(), Some(Duration::from_secs(24 * 3600)));

        let critical = LifecyclePolicy::for_qos_class(QoSClass::Critical);
        assert!(critical.ttl_duration().is_none());
    }

    #[test]
    fn test_from_qos_and_ttl() {
        let qos_policy = QoSPolicy::low();
        let ttl_config = TtlConfig::tactical();

        let lifecycle = LifecyclePolicy::from_qos_and_ttl(qos_policy.clone(), &ttl_config);

        assert_eq!(lifecycle.qos_class(), QoSClass::Low);
        assert!(lifecycle.use_soft_delete);
        assert_eq!(
            lifecycle.soft_delete_after_seconds,
            Some(ttl_config.position_ttl.as_secs())
        );
    }

    #[test]
    fn test_decision_display() {
        assert_eq!(LifecycleDecision::Retain.to_string(), "RETAIN");
        assert_eq!(LifecycleDecision::SoftDelete.to_string(), "SOFT_DELETE");
        assert_eq!(LifecycleDecision::Evict.to_string(), "EVICT");
        assert_eq!(LifecycleDecision::TtlExpired.to_string(), "TTL_EXPIRED");
    }

    #[test]
    fn test_eviction_order_by_pressure() {
        // Test that higher priority data requires higher pressure to evict
        let high = LifecyclePolicy::for_qos_class(QoSClass::High);
        let normal = LifecyclePolicy::for_qos_class(QoSClass::Normal);
        let low = LifecyclePolicy::for_qos_class(QoSClass::Low);
        let bulk = LifecyclePolicy::for_qos_class(QoSClass::Bulk);

        // Ages must exceed minimum retention for each class:
        // High: 24h = 86400s, Normal: 1h = 3600s, Low: 5min = 300s, Bulk: 1min = 60s
        let age_high = 100_000; // > 24 hours
        let age_normal = 7200; // > 1 hour
        let age_low = 600; // > 5 min
        let age_bulk = 120; // > 1 min

        // At 80% pressure: only Bulk and Low should evict (when old enough)
        assert!(!high.should_evict(age_high, 0.80));
        assert!(!normal.should_evict(age_normal, 0.80));
        assert!(low.should_evict(age_low, 0.80));
        assert!(bulk.should_evict(age_bulk, 0.80));

        // At 90% pressure: Normal and below should evict (when old enough)
        assert!(!high.should_evict(age_high, 0.90));
        assert!(normal.should_evict(age_normal, 0.90));
        assert!(low.should_evict(age_low, 0.90));
        assert!(bulk.should_evict(age_bulk, 0.90));

        // At 98% pressure: even High should evict (when old enough)
        assert!(high.should_evict(age_high, 0.98));
        assert!(normal.should_evict(age_normal, 0.98));
        assert!(low.should_evict(age_low, 0.98));
        assert!(bulk.should_evict(age_bulk, 0.98));
    }
}
