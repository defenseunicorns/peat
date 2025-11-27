//! Sync recovery with obsolescence filtering (ADR-019 Phase 3)
//!
//! This module handles network partition recovery with priority-ordered
//! synchronization and obsolescence filtering to discard stale data.
//!
//! # Obsolescence Windows
//!
//! Different data types have different obsolescence windows:
//!
//! | Data Type | Obsolescence | Rationale |
//! |-----------|--------------|-----------|
//! | PositionUpdate | 5 minutes | Stale position data is misleading |
//! | ContactReport | Never | Always valuable for situational awareness |
//! | HealthStatus | 10 minutes | Recent health more important |
//! | Image/Video | 1 hour | Still valuable for analysis |
//! | Telemetry | 30 seconds | Only latest matters |
//!
//! # Recovery Strategy
//!
//! When recovering from a network partition:
//! 1. Filter out obsolete data
//! 2. Sync P1 Critical first
//! 3. Then P2 High, P3 Normal, etc.
//! 4. Pause if bandwidth is exhausted
//!
//! # Example
//!
//! ```
//! use hive_protocol::qos::{QoSClass, DataType, SyncRecovery};
//!
//! let mut recovery = SyncRecovery::default_military();
//!
//! // Check if data is obsolete
//! use std::time::Duration;
//! assert!(recovery.is_obsolete(DataType::PositionUpdate, Duration::from_secs(600)));
//! assert!(!recovery.is_obsolete(DataType::ContactReport, Duration::from_secs(86400)));
//! ```

use super::classification::DataType;
use super::QoSClass;
use crate::Result;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

/// A batch of updates pending synchronization
#[derive(Debug, Clone)]
pub struct UpdateBatch {
    /// Unique identifier for this batch
    pub id: u64,

    /// The data to sync
    pub data: Vec<u8>,

    /// Data type of this batch
    pub data_type: DataType,

    /// When this data was created/captured
    pub created_at: Instant,

    /// QoS class for this batch
    pub qos_class: QoSClass,
}

impl UpdateBatch {
    /// Create a new update batch
    pub fn new(id: u64, data: Vec<u8>, data_type: DataType, qos_class: QoSClass) -> Self {
        Self {
            id,
            data,
            data_type,
            created_at: Instant::now(),
            qos_class,
        }
    }

    /// Create with explicit creation time
    pub fn with_time(
        id: u64,
        data: Vec<u8>,
        data_type: DataType,
        qos_class: QoSClass,
        created_at: Instant,
    ) -> Self {
        Self {
            id,
            data,
            data_type,
            qos_class,
            created_at,
        }
    }

    /// Get age of this batch
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Get size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// Sync recovery manager
///
/// Handles network partition recovery with priority ordering
/// and obsolescence filtering.
#[derive(Debug)]
pub struct SyncRecovery {
    /// Queued updates organized by QoS class
    queued_updates: BTreeMap<QoSClass, Vec<UpdateBatch>>,

    /// Obsolescence windows by data type
    obsolescence_windows: HashMap<DataType, Duration>,

    /// Total bytes queued
    total_bytes: usize,

    /// Number of batches filtered due to obsolescence
    obsolete_filtered: usize,

    /// Next batch ID
    next_batch_id: u64,

    /// Recovery in progress flag
    recovery_in_progress: bool,
}

impl SyncRecovery {
    /// Create a new sync recovery manager
    pub fn new() -> Self {
        Self {
            queued_updates: BTreeMap::new(),
            obsolescence_windows: HashMap::new(),
            total_bytes: 0,
            obsolete_filtered: 0,
            next_batch_id: 0,
            recovery_in_progress: false,
        }
    }

    /// Create with default military obsolescence windows
    pub fn default_military() -> Self {
        let mut recovery = Self::new();

        // P4 Low - routine telemetry with short windows
        recovery.set_obsolescence(DataType::PositionUpdate, Duration::from_secs(300)); // 5 min
        recovery.set_obsolescence(DataType::Heartbeat, Duration::from_secs(60)); // 1 min
        recovery.set_obsolescence(DataType::SensorTelemetry, Duration::from_secs(30)); // 30 sec
        recovery.set_obsolescence(DataType::EnvironmentData, Duration::from_secs(600)); // 10 min

        // P3 Normal - operational data with moderate windows
        recovery.set_obsolescence(DataType::HealthStatus, Duration::from_secs(600)); // 10 min
        recovery.set_obsolescence(DataType::CapabilityChange, Duration::from_secs(1800)); // 30 min
        recovery.set_obsolescence(DataType::FormationUpdate, Duration::from_secs(1800)); // 30 min
        recovery.set_obsolescence(DataType::TaskAssignment, Duration::from_secs(3600)); // 1 hour

        // P2 High - important data with longer windows
        recovery.set_obsolescence(DataType::TargetImage, Duration::from_secs(3600)); // 1 hour
        recovery.set_obsolescence(DataType::AudioIntercept, Duration::from_secs(3600)); // 1 hour
        recovery.set_obsolescence(DataType::MissionRetasking, Duration::from_secs(7200)); // 2 hours
        recovery.set_obsolescence(DataType::FormationChange, Duration::from_secs(3600)); // 1 hour

        // P1 Critical - never obsolete (no entries means never obsolete)
        // ContactReport, EmergencyAlert, AbortCommand, RoeUpdate

        // P5 Bulk - longer windows for archival data
        recovery.set_obsolescence(DataType::DebugLog, Duration::from_secs(86400)); // 24 hours
        recovery.set_obsolescence(DataType::HistoricalTrack, Duration::from_secs(604800)); // 1 week
                                                                                           // ModelUpdate and TrainingData don't become obsolete

        recovery
    }

    /// Set obsolescence window for a data type
    pub fn set_obsolescence(&mut self, data_type: DataType, window: Duration) {
        self.obsolescence_windows.insert(data_type, window);
    }

    /// Get obsolescence window for a data type
    pub fn get_obsolescence(&self, data_type: &DataType) -> Option<Duration> {
        self.obsolescence_windows.get(data_type).copied()
    }

    /// Check if data is obsolete
    ///
    /// Returns true if the data is older than its obsolescence window.
    /// Data types without a configured window are never obsolete.
    pub fn is_obsolete(&self, data_type: DataType, age: Duration) -> bool {
        self.obsolescence_windows
            .get(&data_type)
            .map(|window| age > *window)
            .unwrap_or(false) // No window = never obsolete
    }

    /// Queue an update batch for recovery sync
    pub fn queue_update(&mut self, data: Vec<u8>, data_type: DataType, qos_class: QoSClass) -> u64 {
        let id = self.next_batch_id;
        self.next_batch_id += 1;

        let batch = UpdateBatch::new(id, data.clone(), data_type, qos_class);
        self.total_bytes += batch.size();

        self.queued_updates
            .entry(qos_class)
            .or_default()
            .push(batch);

        id
    }

    /// Queue a batch with explicit creation time (for testing/replaying)
    pub fn queue_update_with_time(
        &mut self,
        data: Vec<u8>,
        data_type: DataType,
        qos_class: QoSClass,
        created_at: Instant,
    ) -> u64 {
        let id = self.next_batch_id;
        self.next_batch_id += 1;

        let batch = UpdateBatch::with_time(id, data.clone(), data_type, qos_class, created_at);
        self.total_bytes += batch.size();

        self.queued_updates
            .entry(qos_class)
            .or_default()
            .push(batch);

        id
    }

    /// Apply obsolescence filter to all queued updates
    ///
    /// Removes updates that have exceeded their obsolescence window.
    /// Returns the number of batches filtered.
    pub fn apply_obsolescence_filter(&mut self) -> usize {
        let mut filtered = 0;
        let mut bytes_removed = 0;

        // Collect the obsolescence windows upfront to avoid borrowing issues
        let windows = self.obsolescence_windows.clone();

        for batches in self.queued_updates.values_mut() {
            let before_len = batches.len();

            batches.retain(|batch| {
                let is_obsolete = windows
                    .get(&batch.data_type)
                    .map(|window| batch.age() > *window)
                    .unwrap_or(false);
                if is_obsolete {
                    bytes_removed += batch.size();
                }
                !is_obsolete
            });

            let removed = before_len - batches.len();
            filtered += removed;
        }

        self.total_bytes = self.total_bytes.saturating_sub(bytes_removed);
        self.obsolete_filtered += filtered;
        filtered
    }

    /// Recover from network partition
    ///
    /// This processes queued updates in priority order:
    /// 1. Filter obsolete data
    /// 2. Yield batches in priority order (P1 first)
    ///
    /// Returns an iterator over batches to sync.
    pub async fn recover_from_partition(&mut self) -> Result<RecoveryIterator<'_>> {
        self.recovery_in_progress = true;

        // First, filter obsolete data
        self.apply_obsolescence_filter();

        // Create iterator that yields batches in priority order
        Ok(RecoveryIterator::new(self))
    }

    /// Get next batch to sync during recovery
    ///
    /// Returns batches in priority order (P1 Critical first).
    pub fn next_recovery_batch(&mut self) -> Option<UpdateBatch> {
        // BTreeMap iterates in key order, and QoSClass Ord puts Critical first
        // However, we need to iterate in actual priority order (Critical > High > Normal > Low > Bulk)
        // which means we need to reverse the natural ordering

        for class in QoSClass::all_by_priority() {
            if let Some(batches) = self.queued_updates.get_mut(class) {
                if !batches.is_empty() {
                    let batch = batches.remove(0);
                    self.total_bytes = self.total_bytes.saturating_sub(batch.size());
                    return Some(batch);
                }
            }
        }

        None
    }

    /// Mark recovery as complete
    pub fn complete_recovery(&mut self) {
        self.recovery_in_progress = false;
    }

    /// Check if recovery is in progress
    pub fn is_recovering(&self) -> bool {
        self.recovery_in_progress
    }

    /// Get total bytes queued for recovery
    pub fn total_bytes_queued(&self) -> usize {
        self.total_bytes
    }

    /// Get count of batches queued by class
    pub fn queued_count_by_class(&self, class: QoSClass) -> usize {
        self.queued_updates
            .get(&class)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Get total count of queued batches
    pub fn total_queued(&self) -> usize {
        self.queued_updates.values().map(|v| v.len()).sum()
    }

    /// Get count of batches filtered due to obsolescence
    pub fn obsolete_filtered_count(&self) -> usize {
        self.obsolete_filtered
    }

    /// Get recovery statistics
    pub fn stats(&self) -> RecoveryStats {
        let mut by_class = HashMap::new();
        for class in QoSClass::all_by_priority() {
            by_class.insert(*class, self.queued_count_by_class(*class));
        }

        RecoveryStats {
            total_queued: self.total_queued(),
            total_bytes: self.total_bytes,
            by_class,
            obsolete_filtered: self.obsolete_filtered,
            recovery_in_progress: self.recovery_in_progress,
        }
    }

    /// Clear all queued updates
    pub fn clear(&mut self) {
        self.queued_updates.clear();
        self.total_bytes = 0;
    }
}

impl Default for SyncRecovery {
    fn default() -> Self {
        Self::default_military()
    }
}

/// Iterator for recovery batches
pub struct RecoveryIterator<'a> {
    recovery: &'a mut SyncRecovery,
}

impl<'a> RecoveryIterator<'a> {
    fn new(recovery: &'a mut SyncRecovery) -> Self {
        Self { recovery }
    }

    /// Get next batch, applying bandwidth limits
    pub fn next_with_limit(&mut self, max_bytes: usize) -> Option<UpdateBatch> {
        // Peek at next batch to check size
        let peek_size = self.peek_size()?;

        if peek_size > max_bytes {
            return None;
        }

        self.recovery.next_recovery_batch()
    }

    /// Peek at the size of the next batch
    pub fn peek_size(&self) -> Option<usize> {
        for class in QoSClass::all_by_priority() {
            if let Some(batches) = self.recovery.queued_updates.get(class) {
                if let Some(batch) = batches.first() {
                    return Some(batch.size());
                }
            }
        }
        None
    }

    /// Check if there are more batches
    pub fn has_more(&self) -> bool {
        self.recovery.total_queued() > 0
    }
}

impl Iterator for RecoveryIterator<'_> {
    type Item = UpdateBatch;

    fn next(&mut self) -> Option<Self::Item> {
        self.recovery.next_recovery_batch()
    }
}

/// Recovery statistics
#[derive(Debug, Clone)]
pub struct RecoveryStats {
    /// Total batches queued
    pub total_queued: usize,

    /// Total bytes queued
    pub total_bytes: usize,

    /// Batches queued by class
    pub by_class: HashMap<QoSClass, usize>,

    /// Number of batches filtered due to obsolescence
    pub obsolete_filtered: usize,

    /// Whether recovery is in progress
    pub recovery_in_progress: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_batch_creation() {
        let batch = UpdateBatch::new(
            1,
            vec![1, 2, 3],
            DataType::ContactReport,
            QoSClass::Critical,
        );

        assert_eq!(batch.id, 1);
        assert_eq!(batch.size(), 3);
        assert_eq!(batch.qos_class, QoSClass::Critical);
    }

    #[test]
    fn test_recovery_creation() {
        let recovery = SyncRecovery::new();

        assert_eq!(recovery.total_queued(), 0);
        assert_eq!(recovery.total_bytes_queued(), 0);
        assert!(!recovery.is_recovering());
    }

    #[test]
    fn test_obsolescence_windows() {
        let recovery = SyncRecovery::default_military();

        // Position updates obsolete after 5 minutes
        assert!(recovery.is_obsolete(DataType::PositionUpdate, Duration::from_secs(400)));
        assert!(!recovery.is_obsolete(DataType::PositionUpdate, Duration::from_secs(200)));

        // Contact reports never obsolete
        assert!(!recovery.is_obsolete(DataType::ContactReport, Duration::from_secs(86400)));
    }

    #[test]
    fn test_queue_update() {
        let mut recovery = SyncRecovery::new();

        let id = recovery.queue_update(vec![1, 2, 3], DataType::HealthStatus, QoSClass::Normal);

        assert_eq!(id, 0);
        assert_eq!(recovery.total_queued(), 1);
        assert_eq!(recovery.total_bytes_queued(), 3);
        assert_eq!(recovery.queued_count_by_class(QoSClass::Normal), 1);
    }

    #[test]
    fn test_priority_ordering() {
        let mut recovery = SyncRecovery::new();

        // Queue in reverse priority order
        recovery.queue_update(vec![5], DataType::DebugLog, QoSClass::Bulk);
        recovery.queue_update(vec![1], DataType::ContactReport, QoSClass::Critical);
        recovery.queue_update(vec![3], DataType::HealthStatus, QoSClass::Normal);

        // Should dequeue in priority order
        let batch1 = recovery.next_recovery_batch().unwrap();
        assert_eq!(batch1.qos_class, QoSClass::Critical);

        let batch2 = recovery.next_recovery_batch().unwrap();
        assert_eq!(batch2.qos_class, QoSClass::Normal);

        let batch3 = recovery.next_recovery_batch().unwrap();
        assert_eq!(batch3.qos_class, QoSClass::Bulk);
    }

    #[test]
    fn test_obsolescence_filter() {
        let mut recovery = SyncRecovery::default_military();

        // Create an old position update (should be filtered)
        let old_time = Instant::now() - Duration::from_secs(600); // 10 min ago
        recovery.queue_update_with_time(vec![1], DataType::PositionUpdate, QoSClass::Low, old_time);

        // Create a fresh contact report (should not be filtered)
        recovery.queue_update(vec![2], DataType::ContactReport, QoSClass::Critical);

        assert_eq!(recovery.total_queued(), 2);

        let filtered = recovery.apply_obsolescence_filter();

        assert_eq!(filtered, 1);
        assert_eq!(recovery.total_queued(), 1);

        // The remaining batch should be the contact report
        let batch = recovery.next_recovery_batch().unwrap();
        assert_eq!(batch.data_type, DataType::ContactReport);
    }

    #[test]
    fn test_stats() {
        let mut recovery = SyncRecovery::new();

        recovery.queue_update(vec![0; 100], DataType::ContactReport, QoSClass::Critical);
        recovery.queue_update(vec![0; 200], DataType::HealthStatus, QoSClass::Normal);
        recovery.queue_update(vec![0; 50], DataType::DebugLog, QoSClass::Bulk);

        let stats = recovery.stats();

        assert_eq!(stats.total_queued, 3);
        assert_eq!(stats.total_bytes, 350);
        assert_eq!(*stats.by_class.get(&QoSClass::Critical).unwrap(), 1);
        assert_eq!(*stats.by_class.get(&QoSClass::Normal).unwrap(), 1);
        assert_eq!(*stats.by_class.get(&QoSClass::Bulk).unwrap(), 1);
    }

    #[tokio::test]
    async fn test_recover_from_partition() {
        let mut recovery = SyncRecovery::default_military();

        recovery.queue_update(vec![1], DataType::ContactReport, QoSClass::Critical);
        recovery.queue_update(vec![2], DataType::HealthStatus, QoSClass::Normal);

        let mut iter = recovery.recover_from_partition().await.unwrap();

        // Check iter has more and consume it
        assert!(iter.has_more());

        // Consume all items to release the borrow
        let batch1 = iter.next();
        assert!(batch1.is_some());
        assert_eq!(batch1.unwrap().qos_class, QoSClass::Critical);

        let batch2 = iter.next();
        assert!(batch2.is_some());

        assert!(!iter.has_more());
    }

    #[test]
    fn test_clear() {
        let mut recovery = SyncRecovery::new();

        recovery.queue_update(vec![0; 100], DataType::ContactReport, QoSClass::Critical);
        recovery.queue_update(vec![0; 200], DataType::HealthStatus, QoSClass::Normal);

        recovery.clear();

        assert_eq!(recovery.total_queued(), 0);
        assert_eq!(recovery.total_bytes_queued(), 0);
    }

    #[test]
    fn test_custom_obsolescence() {
        let mut recovery = SyncRecovery::new();

        // Set custom window
        recovery.set_obsolescence(DataType::HealthStatus, Duration::from_secs(60));

        assert!(recovery.is_obsolete(DataType::HealthStatus, Duration::from_secs(120)));
        assert!(!recovery.is_obsolete(DataType::HealthStatus, Duration::from_secs(30)));
    }

    #[test]
    fn test_batch_age() {
        let batch = UpdateBatch::new(1, vec![1], DataType::ContactReport, QoSClass::Critical);

        // Age should be very small
        assert!(batch.age() < Duration::from_secs(1));
    }

    #[test]
    fn test_multiple_batches_same_class() {
        let mut recovery = SyncRecovery::new();

        recovery.queue_update(vec![1], DataType::ContactReport, QoSClass::Critical);
        recovery.queue_update(vec![2], DataType::EmergencyAlert, QoSClass::Critical);
        recovery.queue_update(vec![3], DataType::AbortCommand, QoSClass::Critical);

        // Should get them in FIFO order within the class
        let batch1 = recovery.next_recovery_batch().unwrap();
        assert_eq!(batch1.data, vec![1]);

        let batch2 = recovery.next_recovery_batch().unwrap();
        assert_eq!(batch2.data, vec![2]);

        let batch3 = recovery.next_recovery_batch().unwrap();
        assert_eq!(batch3.data, vec![3]);
    }

    #[test]
    fn test_get_obsolescence() {
        let recovery = SyncRecovery::default_military();

        assert!(recovery
            .get_obsolescence(&DataType::PositionUpdate)
            .is_some());
        assert!(recovery
            .get_obsolescence(&DataType::ContactReport)
            .is_none());
    }

    #[test]
    fn test_obsolete_filtered_count() {
        let mut recovery = SyncRecovery::default_military();

        let old_time = Instant::now() - Duration::from_secs(600);
        recovery.queue_update_with_time(vec![1], DataType::PositionUpdate, QoSClass::Low, old_time);

        assert_eq!(recovery.obsolete_filtered_count(), 0);

        recovery.apply_obsolescence_filter();

        assert_eq!(recovery.obsolete_filtered_count(), 1);
    }
}
