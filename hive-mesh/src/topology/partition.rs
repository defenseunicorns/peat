//! Network partition detection and autonomous operation
//!
//! This module detects when a node is isolated from ALL higher hierarchy levels
//! (network partition) as distinguished from temporary single parent failover.
//!
//! ## Partition vs Failover
//!
//! - **Parent Failover**: One parent becomes unavailable, but other parents exist
//!   - Response: Select backup parent from remaining candidates
//!   - Handled by TopologyBuilder's peer selection
//!
//! - **Network Partition**: Node is isolated from ALL higher hierarchy levels
//!   - Response: Enter autonomous operation mode
//!   - Handled by PartitionDetector
//!
//! ## Detection Strategy
//!
//! PartitionDetector uses exponential backoff with multiple attempts to avoid
//! false positives from transient network issues:
//!
//! 1. Check beacon visibility for higher hierarchy levels
//! 2. If no higher-level beacons visible, retry with exponential backoff
//! 3. After N failed attempts, emit PartitionDetected event
//! 4. Monitor for beacon recovery to emit PartitionHealed event
//!
//! ## Architecture
//!
//! ```text
//! PartitionDetector
//! ├── PartitionConfig (detection thresholds)
//! ├── PartitionEvent (state change notifications)
//! └── PartitionHandler trait (pluggable response strategies)
//! ```

use crate::beacon::{GeographicBeacon, HierarchyLevel};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Configuration for partition detection
#[derive(Debug, Clone)]
pub struct PartitionConfig {
    /// Minimum number of detection attempts before declaring partition
    pub min_detection_attempts: u32,

    /// Initial backoff duration between detection attempts
    pub initial_backoff: Duration,

    /// Maximum backoff duration cap
    pub max_backoff: Duration,

    /// Backoff multiplier for exponential growth
    pub backoff_multiplier: f64,

    /// Minimum number of higher-level beacons required to consider connected
    pub min_higher_level_beacons: usize,
}

impl Default for PartitionConfig {
    fn default() -> Self {
        Self {
            min_detection_attempts: 3,
            initial_backoff: Duration::from_secs(2),
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            min_higher_level_beacons: 1,
        }
    }
}

impl PartitionConfig {
    /// Calculate backoff duration for a given attempt number
    pub fn calculate_backoff(&self, attempt: u32) -> Duration {
        let multiplier = self.backoff_multiplier.powi(attempt as i32);
        let backoff_secs = self.initial_backoff.as_secs_f64() * multiplier;
        let max_secs = self.max_backoff.as_secs_f64();
        Duration::from_secs_f64(backoff_secs.min(max_secs))
    }
}

/// Partition detection events
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionEvent {
    /// Network partition detected - node is isolated from all higher levels
    PartitionDetected {
        /// Number of detection attempts that confirmed partition
        attempts: u32,
        /// Duration of detection process
        detection_duration: Duration,
    },

    /// Network partition healed - higher-level beacons visible again
    PartitionHealed {
        /// Number of higher-level beacons now visible
        visible_beacons: usize,
        /// Duration of partition
        partition_duration: Duration,
    },
}

/// Handler for partition events
///
/// Follows Ports & Adapters pattern like BeaconStorage and HierarchyStrategy.
pub trait PartitionHandler: Send + Sync + std::fmt::Debug {
    /// Handle partition detected event
    fn on_partition_detected(&self, event: &PartitionEvent);

    /// Handle partition healed event
    fn on_partition_healed(&self, event: &PartitionEvent);
}

/// Network partition detector
///
/// Monitors beacon visibility to detect isolation from higher hierarchy levels.
#[derive(Debug)]
pub struct PartitionDetector {
    config: PartitionConfig,
    current_level: HierarchyLevel,
    handler: Option<Arc<dyn PartitionHandler>>,

    // Detection state
    partitioned: bool,
    detection_attempts: u32,
    last_detection_attempt: Option<Instant>,
    partition_start: Option<Instant>,
}

impl PartitionDetector {
    /// Create a new partition detector
    pub fn new(current_level: HierarchyLevel, config: PartitionConfig) -> Self {
        Self {
            config,
            current_level,
            handler: None,
            partitioned: false,
            detection_attempts: 0,
            last_detection_attempt: None,
            partition_start: None,
        }
    }

    /// Set the partition event handler
    pub fn with_handler(mut self, handler: Arc<dyn PartitionHandler>) -> Self {
        self.handler = Some(handler);
        self
    }

    /// Check for partition based on current beacon visibility
    ///
    /// # Arguments
    /// * `beacons` - Currently visible beacons from BeaconObserver
    ///
    /// # Returns
    /// Optional PartitionEvent if state changed
    pub fn check_partition(
        &mut self,
        beacons: &HashMap<String, GeographicBeacon>,
    ) -> Option<PartitionEvent> {
        let higher_level_count = self.count_higher_level_beacons(beacons);
        let has_connectivity = higher_level_count >= self.config.min_higher_level_beacons;

        if has_connectivity {
            // We have connectivity - check if we were previously partitioned
            if self.partitioned {
                return self.handle_partition_healed(higher_level_count);
            } else {
                // Reset detection state since we have connectivity
                self.reset_detection_state();
                return None;
            }
        }

        // No connectivity - check if we should attempt detection
        if !self.should_attempt_detection() {
            return None;
        }

        self.detection_attempts += 1;
        self.last_detection_attempt = Some(Instant::now());

        // Have we reached minimum attempts threshold?
        if self.detection_attempts >= self.config.min_detection_attempts {
            return self.handle_partition_detected();
        }

        None
    }

    /// Count beacons at higher hierarchy levels than current node
    fn count_higher_level_beacons(&self, beacons: &HashMap<String, GeographicBeacon>) -> usize {
        beacons
            .values()
            .filter(|beacon| beacon.operational && beacon.hierarchy_level > self.current_level)
            .count()
    }

    /// Check if we should attempt detection now based on backoff timing
    fn should_attempt_detection(&self) -> bool {
        match self.last_detection_attempt {
            None => true, // First attempt
            Some(last_attempt) => {
                let backoff = self.config.calculate_backoff(self.detection_attempts);
                last_attempt.elapsed() >= backoff
            }
        }
    }

    /// Handle partition detected
    fn handle_partition_detected(&mut self) -> Option<PartitionEvent> {
        if self.partitioned {
            return None; // Already partitioned
        }

        let partition_start = self.last_detection_attempt.unwrap_or_else(Instant::now);

        let detection_duration = partition_start.elapsed();

        let event = PartitionEvent::PartitionDetected {
            attempts: self.detection_attempts,
            detection_duration,
        };

        self.partitioned = true;
        self.partition_start = Some(partition_start);

        if let Some(ref handler) = self.handler {
            handler.on_partition_detected(&event);
        }

        Some(event)
    }

    /// Handle partition healed
    fn handle_partition_healed(&mut self, visible_beacons: usize) -> Option<PartitionEvent> {
        if !self.partitioned {
            return None; // Wasn't partitioned
        }

        let partition_duration = self
            .partition_start
            .map(|start| start.elapsed())
            .unwrap_or(Duration::ZERO);

        let event = PartitionEvent::PartitionHealed {
            visible_beacons,
            partition_duration,
        };

        self.reset_detection_state();

        if let Some(ref handler) = self.handler {
            handler.on_partition_healed(&event);
        }

        Some(event)
    }

    /// Reset detection state
    fn reset_detection_state(&mut self) {
        self.partitioned = false;
        self.detection_attempts = 0;
        self.last_detection_attempt = None;
        self.partition_start = None;
    }

    /// Check if currently in partitioned state
    pub fn is_partitioned(&self) -> bool {
        self.partitioned
    }

    /// Get current detection attempt count
    pub fn detection_attempts(&self) -> u32 {
        self.detection_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_beacon(node_id: &str, level: HierarchyLevel, operational: bool) -> GeographicBeacon {
        let position = crate::beacon::GeoPosition::new(37.7749, -122.4194);
        let mut beacon = GeographicBeacon::new(node_id.to_string(), position, level);
        beacon.operational = operational;
        beacon
    }

    #[test]
    fn test_partition_config_default() {
        let config = PartitionConfig::default();
        assert_eq!(config.min_detection_attempts, 3);
        assert_eq!(config.initial_backoff, Duration::from_secs(2));
        assert_eq!(config.max_backoff, Duration::from_secs(30));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.min_higher_level_beacons, 1);
    }

    #[test]
    fn test_backoff_calculation() {
        let config = PartitionConfig::default();

        // Attempt 0: 2s * 2^0 = 2s
        assert_eq!(config.calculate_backoff(0), Duration::from_secs(2));

        // Attempt 1: 2s * 2^1 = 4s
        assert_eq!(config.calculate_backoff(1), Duration::from_secs(4));

        // Attempt 2: 2s * 2^2 = 8s
        assert_eq!(config.calculate_backoff(2), Duration::from_secs(8));

        // Attempt 10: Would be 2048s, but capped at max_backoff (30s)
        assert_eq!(config.calculate_backoff(10), Duration::from_secs(30));
    }

    #[test]
    fn test_count_higher_level_beacons() {
        let detector = PartitionDetector::new(HierarchyLevel::Platform, PartitionConfig::default());

        let mut beacons = HashMap::new();
        beacons.insert(
            "squad1".to_string(),
            create_beacon("squad1", HierarchyLevel::Squad, true),
        );
        beacons.insert(
            "platoon1".to_string(),
            create_beacon("platoon1", HierarchyLevel::Platoon, true),
        );
        beacons.insert(
            "platform2".to_string(),
            create_beacon("platform2", HierarchyLevel::Platform, true),
        );

        // Platform level node should see Squad and Platoon as higher levels
        assert_eq!(detector.count_higher_level_beacons(&beacons), 2);
    }

    #[test]
    fn test_count_excludes_non_operational_beacons() {
        let detector = PartitionDetector::new(HierarchyLevel::Platform, PartitionConfig::default());

        let mut beacons = HashMap::new();
        beacons.insert(
            "squad1".to_string(),
            create_beacon("squad1", HierarchyLevel::Squad, true),
        );
        beacons.insert(
            "squad2".to_string(),
            create_beacon("squad2", HierarchyLevel::Squad, false),
        );

        // Only operational beacons should be counted
        assert_eq!(detector.count_higher_level_beacons(&beacons), 1);
    }

    #[test]
    fn test_no_partition_with_connectivity() {
        let mut detector =
            PartitionDetector::new(HierarchyLevel::Platform, PartitionConfig::default());

        let mut beacons = HashMap::new();
        beacons.insert(
            "squad1".to_string(),
            create_beacon("squad1", HierarchyLevel::Squad, true),
        );

        // Should return None - no partition detected
        let event = detector.check_partition(&beacons);
        assert!(event.is_none());
        assert!(!detector.is_partitioned());
        assert_eq!(detector.detection_attempts(), 0);
    }

    #[test]
    fn test_partition_detection_after_min_attempts() {
        let config = PartitionConfig {
            min_detection_attempts: 2,
            initial_backoff: Duration::from_millis(1), // Very short for testing
            ..Default::default()
        };

        let mut detector = PartitionDetector::new(HierarchyLevel::Platform, config);

        let beacons = HashMap::new(); // No beacons visible

        // First attempt - should return None
        let event1 = detector.check_partition(&beacons);
        assert!(event1.is_none());
        assert_eq!(detector.detection_attempts(), 1);

        // Wait for backoff
        std::thread::sleep(Duration::from_millis(2));

        // Second attempt - should detect partition
        let event2 = detector.check_partition(&beacons);
        assert!(event2.is_some());
        assert!(detector.is_partitioned());

        if let Some(PartitionEvent::PartitionDetected { attempts, .. }) = event2 {
            assert_eq!(attempts, 2);
        } else {
            panic!("Expected PartitionDetected event");
        }
    }

    #[test]
    fn test_partition_healed_event() {
        let config = PartitionConfig {
            min_detection_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            ..Default::default()
        };

        let mut detector = PartitionDetector::new(HierarchyLevel::Platform, config);

        // Detect partition
        let beacons_empty = HashMap::new();
        let _ = detector.check_partition(&beacons_empty);
        assert!(detector.is_partitioned());

        // Heal partition
        let mut beacons_with_parent = HashMap::new();
        beacons_with_parent.insert(
            "squad1".to_string(),
            create_beacon("squad1", HierarchyLevel::Squad, true),
        );

        let event = detector.check_partition(&beacons_with_parent);
        assert!(event.is_some());

        if let Some(PartitionEvent::PartitionHealed {
            visible_beacons, ..
        }) = event
        {
            assert_eq!(visible_beacons, 1);
        } else {
            panic!("Expected PartitionHealed event");
        }

        assert!(!detector.is_partitioned());
        assert_eq!(detector.detection_attempts(), 0);
    }

    #[test]
    fn test_backoff_prevents_rapid_detection_attempts() {
        let config = PartitionConfig {
            min_detection_attempts: 3,
            initial_backoff: Duration::from_secs(10), // Long backoff
            ..Default::default()
        };

        let mut detector = PartitionDetector::new(HierarchyLevel::Platform, config);

        let beacons = HashMap::new();

        // First attempt
        let _ = detector.check_partition(&beacons);
        assert_eq!(detector.detection_attempts(), 1);

        // Immediate second call should not increment attempts (backoff not elapsed)
        let _ = detector.check_partition(&beacons);
        assert_eq!(detector.detection_attempts(), 1);

        // Still should be 1 attempt
        assert_eq!(detector.detection_attempts(), 1);
    }

    #[test]
    fn test_company_level_node_has_no_higher_levels() {
        let detector = PartitionDetector::new(HierarchyLevel::Company, PartitionConfig::default());

        let mut beacons = HashMap::new();
        beacons.insert(
            "platoon1".to_string(),
            create_beacon("platoon1", HierarchyLevel::Platoon, true),
        );
        beacons.insert(
            "squad1".to_string(),
            create_beacon("squad1", HierarchyLevel::Squad, true),
        );

        // Company is top level, so no beacons are higher
        assert_eq!(detector.count_higher_level_beacons(&beacons), 0);
    }

    #[test]
    fn test_min_higher_level_beacons_threshold() {
        let config = PartitionConfig {
            min_higher_level_beacons: 2, // Require at least 2 higher-level beacons
            min_detection_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            ..Default::default()
        };

        let mut detector = PartitionDetector::new(HierarchyLevel::Platform, config);

        // Only 1 higher-level beacon (below threshold)
        let mut beacons = HashMap::new();
        beacons.insert(
            "squad1".to_string(),
            create_beacon("squad1", HierarchyLevel::Squad, true),
        );

        let event = detector.check_partition(&beacons);
        // Should detect partition since we have 1 beacon but need 2
        assert!(event.is_some());
    }
}
