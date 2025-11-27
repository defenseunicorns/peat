//! Preemption control for QoS-aware transfers (ADR-019 Phase 3)
//!
//! This module manages transfer preemption, allowing high-priority data
//! to pause or cancel lower-priority transfers when bandwidth is constrained.
//!
//! # Preemption Rules
//!
//! - P1 Critical: Can preempt all lower priorities (P2-P5)
//! - P2 High: Can preempt P3-P5
//! - P3-P5: Cannot preempt (must wait for bandwidth)
//!
//! # Architecture
//!
//! The `PreemptionController` tracks active transfers and coordinates
//! preemption decisions:
//!
//! 1. When critical data arrives, check if preemption is needed
//! 2. Identify preemptable transfers (lower priority, pausable)
//! 3. Pause transfers and release their bandwidth
//! 4. Resume paused transfers when bandwidth becomes available
//!
//! # Example
//!
//! ```
//! use hive_protocol::qos::{QoSClass, PreemptionController, ActiveTransfer};
//! use uuid::Uuid;
//!
//! # async fn example() {
//! let controller = PreemptionController::new();
//!
//! // Register an active transfer
//! let transfer_id = controller.register_transfer(
//!     QoSClass::Low,
//!     10000,  // 10KB
//!     true,   // can pause
//! ).await;
//!
//! // Check if preemption is needed for critical data
//! if controller.should_preempt(QoSClass::Critical).await {
//!     // Pause lower priority transfers
//!     let paused = controller.pause_transfers_below(QoSClass::Critical).await;
//!     // ... transmit critical data ...
//!     // Resume paused transfers
//!     controller.resume_transfers(paused).await;
//! }
//! # }
//! ```

use super::QoSClass;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Unique identifier for a transfer
pub type TransferId = Uuid;

/// An active data transfer being tracked by the preemption controller
#[derive(Debug)]
pub struct ActiveTransfer {
    /// Unique transfer identifier
    pub id: TransferId,

    /// QoS class of this transfer
    pub class: QoSClass,

    /// Total bytes to transfer
    pub bytes_total: usize,

    /// Bytes sent so far
    pub bytes_sent: AtomicUsize,

    /// Whether this transfer can be paused
    pub can_pause: bool,

    /// Whether this transfer is currently paused
    pub is_paused: AtomicBool,

    /// When the transfer started
    pub started_at: Instant,

    /// When the transfer was paused (if applicable)
    pub paused_at: RwLock<Option<Instant>>,
}

impl ActiveTransfer {
    /// Create a new active transfer
    pub fn new(class: QoSClass, bytes_total: usize, can_pause: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            class,
            bytes_total,
            bytes_sent: AtomicUsize::new(0),
            can_pause,
            is_paused: AtomicBool::new(false),
            started_at: Instant::now(),
            paused_at: RwLock::new(None),
        }
    }

    /// Get progress as percentage (0.0 - 1.0)
    pub fn progress(&self) -> f64 {
        if self.bytes_total == 0 {
            1.0
        } else {
            self.bytes_sent.load(Ordering::Relaxed) as f64 / self.bytes_total as f64
        }
    }

    /// Get remaining bytes to transfer
    pub fn bytes_remaining(&self) -> usize {
        let sent = self.bytes_sent.load(Ordering::Relaxed);
        self.bytes_total.saturating_sub(sent)
    }

    /// Record bytes sent
    pub fn record_sent(&self, bytes: usize) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Check if transfer is complete
    pub fn is_complete(&self) -> bool {
        self.bytes_sent.load(Ordering::Relaxed) >= self.bytes_total
    }

    /// Check if this transfer can be preempted by the given class
    pub fn can_be_preempted_by(&self, class: QoSClass) -> bool {
        self.can_pause && class.can_preempt(&self.class)
    }

    /// Pause this transfer
    pub async fn pause(&self) {
        if self.can_pause && !self.is_paused.load(Ordering::Relaxed) {
            self.is_paused.store(true, Ordering::Relaxed);
            *self.paused_at.write().await = Some(Instant::now());
        }
    }

    /// Resume this transfer
    pub async fn resume(&self) {
        self.is_paused.store(false, Ordering::Relaxed);
        *self.paused_at.write().await = None;
    }

    /// Get time spent paused (if currently paused)
    pub async fn paused_duration(&self) -> Option<std::time::Duration> {
        if self.is_paused.load(Ordering::Relaxed) {
            self.paused_at.read().await.map(|t| t.elapsed())
        } else {
            None
        }
    }
}

/// Controller for managing transfer preemption
///
/// Tracks active transfers and coordinates preemption decisions
/// to ensure high-priority data can preempt lower priorities.
#[derive(Debug)]
pub struct PreemptionController {
    /// Active transfers indexed by ID
    active_transfers: RwLock<HashMap<TransferId, Arc<ActiveTransfer>>>,

    /// Number of preemption events
    preemption_count: AtomicUsize,

    /// Number of transfers currently paused
    paused_count: AtomicUsize,
}

impl PreemptionController {
    /// Create a new preemption controller
    pub fn new() -> Self {
        Self {
            active_transfers: RwLock::new(HashMap::new()),
            preemption_count: AtomicUsize::new(0),
            paused_count: AtomicUsize::new(0),
        }
    }

    /// Register a new active transfer
    ///
    /// Returns the transfer ID for tracking.
    pub async fn register_transfer(
        &self,
        class: QoSClass,
        bytes_total: usize,
        can_pause: bool,
    ) -> TransferId {
        let transfer = Arc::new(ActiveTransfer::new(class, bytes_total, can_pause));
        let id = transfer.id;

        let mut transfers = self.active_transfers.write().await;
        transfers.insert(id, transfer);

        id
    }

    /// Unregister a completed or cancelled transfer
    pub async fn unregister_transfer(&self, id: TransferId) {
        let mut transfers = self.active_transfers.write().await;
        if let Some(transfer) = transfers.remove(&id) {
            if transfer.is_paused.load(Ordering::Relaxed) {
                self.paused_count.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }

    /// Get a transfer by ID
    pub async fn get_transfer(&self, id: TransferId) -> Option<Arc<ActiveTransfer>> {
        let transfers = self.active_transfers.read().await;
        transfers.get(&id).cloned()
    }

    /// Check if preemption is needed for incoming data of given class
    ///
    /// Returns true if there are lower-priority transfers that can be preempted.
    pub async fn should_preempt(&self, incoming_class: QoSClass) -> bool {
        // Only P1 and P2 can preempt
        if !matches!(incoming_class, QoSClass::Critical | QoSClass::High) {
            return false;
        }

        let transfers = self.active_transfers.read().await;
        for transfer in transfers.values() {
            if transfer.can_be_preempted_by(incoming_class)
                && !transfer.is_paused.load(Ordering::Relaxed)
            {
                return true;
            }
        }
        false
    }

    /// Pause all transfers below a given priority
    ///
    /// Returns the IDs of paused transfers for later resumption.
    pub async fn pause_transfers_below(&self, class: QoSClass) -> Vec<TransferId> {
        let transfers = self.active_transfers.read().await;
        let mut paused = Vec::new();

        for transfer in transfers.values() {
            if transfer.can_be_preempted_by(class) {
                transfer.pause().await;
                paused.push(transfer.id);
                self.paused_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        if !paused.is_empty() {
            self.preemption_count.fetch_add(1, Ordering::Relaxed);
        }

        paused
    }

    /// Resume previously paused transfers
    pub async fn resume_transfers(&self, transfers_to_resume: Vec<TransferId>) {
        let transfers = self.active_transfers.read().await;

        for id in transfers_to_resume {
            if let Some(transfer) = transfers.get(&id) {
                if transfer.is_paused.load(Ordering::Relaxed) {
                    transfer.resume().await;
                    self.paused_count.fetch_sub(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Resume all paused transfers
    pub async fn resume_all(&self) {
        let transfers = self.active_transfers.read().await;

        for transfer in transfers.values() {
            if transfer.is_paused.load(Ordering::Relaxed) {
                transfer.resume().await;
            }
        }

        self.paused_count.store(0, Ordering::Relaxed);
    }

    /// Get number of active transfers
    pub async fn active_count(&self) -> usize {
        self.active_transfers.read().await.len()
    }

    /// Get number of paused transfers
    pub fn paused_count(&self) -> usize {
        self.paused_count.load(Ordering::Relaxed)
    }

    /// Get total preemption events
    pub fn preemption_count(&self) -> usize {
        self.preemption_count.load(Ordering::Relaxed)
    }

    /// Get transfers by class
    pub async fn transfers_by_class(&self, class: QoSClass) -> Vec<Arc<ActiveTransfer>> {
        let transfers = self.active_transfers.read().await;
        transfers
            .values()
            .filter(|t| t.class == class)
            .cloned()
            .collect()
    }

    /// Get all preemptable transfers for a given class
    pub async fn preemptable_transfers(&self, by_class: QoSClass) -> Vec<Arc<ActiveTransfer>> {
        let transfers = self.active_transfers.read().await;
        transfers
            .values()
            .filter(|t| t.can_be_preempted_by(by_class))
            .cloned()
            .collect()
    }

    /// Calculate bandwidth currently used by transfers below a priority
    pub async fn bandwidth_used_below(&self, class: QoSClass) -> usize {
        let transfers = self.active_transfers.read().await;
        transfers
            .values()
            .filter(|t| class.can_preempt(&t.class) && !t.is_paused.load(Ordering::Relaxed))
            .map(|t| t.bytes_remaining())
            .sum()
    }

    /// Clean up completed transfers
    pub async fn cleanup_completed(&self) -> usize {
        let mut transfers = self.active_transfers.write().await;
        let initial_len = transfers.len();

        transfers.retain(|_, t| !t.is_complete());

        initial_len - transfers.len()
    }

    /// Get controller statistics
    pub async fn stats(&self) -> PreemptionStats {
        let transfers = self.active_transfers.read().await;

        let mut by_class = HashMap::new();
        for class in QoSClass::all_by_priority() {
            by_class.insert(*class, 0usize);
        }

        for transfer in transfers.values() {
            *by_class.entry(transfer.class).or_insert(0) += 1;
        }

        PreemptionStats {
            active_transfers: transfers.len(),
            paused_transfers: self.paused_count.load(Ordering::Relaxed),
            preemption_events: self.preemption_count.load(Ordering::Relaxed),
            transfers_by_class: by_class,
        }
    }
}

impl Default for PreemptionController {
    fn default() -> Self {
        Self::new()
    }
}

/// Preemption controller statistics
#[derive(Debug, Clone)]
pub struct PreemptionStats {
    /// Number of active transfers
    pub active_transfers: usize,

    /// Number of paused transfers
    pub paused_transfers: usize,

    /// Total preemption events
    pub preemption_events: usize,

    /// Transfers by QoS class
    pub transfers_by_class: HashMap<QoSClass, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_transfer_creation() {
        let transfer = ActiveTransfer::new(QoSClass::Normal, 1000, true);

        assert_eq!(transfer.class, QoSClass::Normal);
        assert_eq!(transfer.bytes_total, 1000);
        assert!(transfer.can_pause);
        assert!(!transfer.is_paused.load(Ordering::Relaxed));
        assert_eq!(transfer.bytes_remaining(), 1000);
    }

    #[test]
    fn test_transfer_progress() {
        let transfer = ActiveTransfer::new(QoSClass::Normal, 1000, true);

        assert_eq!(transfer.progress(), 0.0);

        transfer.record_sent(500);
        assert!((transfer.progress() - 0.5).abs() < 0.001);

        transfer.record_sent(500);
        assert!((transfer.progress() - 1.0).abs() < 0.001);
        assert!(transfer.is_complete());
    }

    #[test]
    fn test_preemption_eligibility() {
        let low_transfer = ActiveTransfer::new(QoSClass::Low, 1000, true);
        let critical_transfer = ActiveTransfer::new(QoSClass::Critical, 1000, true);

        // Critical can preempt Low
        assert!(low_transfer.can_be_preempted_by(QoSClass::Critical));

        // Low cannot preempt Critical
        assert!(!critical_transfer.can_be_preempted_by(QoSClass::Low));

        // Same class cannot preempt
        assert!(!low_transfer.can_be_preempted_by(QoSClass::Low));
    }

    #[test]
    fn test_non_pausable_transfer() {
        let transfer = ActiveTransfer::new(QoSClass::Low, 1000, false);

        // Even Critical cannot preempt non-pausable transfers
        assert!(!transfer.can_be_preempted_by(QoSClass::Critical));
    }

    #[tokio::test]
    async fn test_controller_register_unregister() {
        let controller = PreemptionController::new();

        let id = controller
            .register_transfer(QoSClass::Normal, 1000, true)
            .await;

        assert_eq!(controller.active_count().await, 1);

        controller.unregister_transfer(id).await;

        assert_eq!(controller.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_should_preempt() {
        let controller = PreemptionController::new();

        // No transfers, no preemption needed
        assert!(!controller.should_preempt(QoSClass::Critical).await);

        // Add a low priority pausable transfer
        controller
            .register_transfer(QoSClass::Low, 1000, true)
            .await;

        // Critical should be able to preempt
        assert!(controller.should_preempt(QoSClass::Critical).await);

        // Bulk should not be able to preempt
        assert!(!controller.should_preempt(QoSClass::Bulk).await);
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let controller = PreemptionController::new();

        let id1 = controller
            .register_transfer(QoSClass::Low, 1000, true)
            .await;
        let id2 = controller
            .register_transfer(QoSClass::Bulk, 1000, true)
            .await;
        let _id3 = controller
            .register_transfer(QoSClass::Critical, 1000, true)
            .await;

        // Pause transfers below Critical
        let paused = controller.pause_transfers_below(QoSClass::Critical).await;

        assert_eq!(paused.len(), 2);
        assert!(paused.contains(&id1));
        assert!(paused.contains(&id2));
        assert_eq!(controller.paused_count(), 2);

        // Resume them
        controller.resume_transfers(paused).await;

        assert_eq!(controller.paused_count(), 0);
    }

    #[tokio::test]
    async fn test_preemption_count() {
        let controller = PreemptionController::new();

        controller
            .register_transfer(QoSClass::Bulk, 1000, true)
            .await;

        assert_eq!(controller.preemption_count(), 0);

        controller.pause_transfers_below(QoSClass::Critical).await;

        assert_eq!(controller.preemption_count(), 1);
    }

    #[tokio::test]
    async fn test_transfers_by_class() {
        let controller = PreemptionController::new();

        controller
            .register_transfer(QoSClass::Normal, 1000, true)
            .await;
        controller
            .register_transfer(QoSClass::Normal, 2000, true)
            .await;
        controller
            .register_transfer(QoSClass::High, 3000, true)
            .await;

        let normal = controller.transfers_by_class(QoSClass::Normal).await;
        assert_eq!(normal.len(), 2);

        let high = controller.transfers_by_class(QoSClass::High).await;
        assert_eq!(high.len(), 1);
    }

    #[tokio::test]
    async fn test_cleanup_completed() {
        let controller = PreemptionController::new();

        let id = controller
            .register_transfer(QoSClass::Normal, 100, true)
            .await;

        let transfer = controller.get_transfer(id).await.unwrap();
        transfer.record_sent(100);

        assert!(transfer.is_complete());

        let cleaned = controller.cleanup_completed().await;
        assert_eq!(cleaned, 1);
        assert_eq!(controller.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_bandwidth_used_below() {
        let controller = PreemptionController::new();

        let id = controller
            .register_transfer(QoSClass::Low, 1000, true)
            .await;
        controller
            .register_transfer(QoSClass::Bulk, 2000, true)
            .await;
        controller
            .register_transfer(QoSClass::Critical, 3000, true)
            .await;

        // Bandwidth below Critical = Low (1000) + Bulk (2000) = 3000
        let bw = controller.bandwidth_used_below(QoSClass::Critical).await;
        assert_eq!(bw, 3000);

        // Pause Low
        controller.pause_transfers_below(QoSClass::High).await;

        // Check transfer is paused
        let transfer = controller.get_transfer(id).await.unwrap();
        assert!(transfer.is_paused.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_stats() {
        let controller = PreemptionController::new();

        controller
            .register_transfer(QoSClass::Critical, 1000, true)
            .await;
        controller
            .register_transfer(QoSClass::Normal, 1000, true)
            .await;
        controller
            .register_transfer(QoSClass::Bulk, 1000, true)
            .await;

        let stats = controller.stats().await;

        assert_eq!(stats.active_transfers, 3);
        assert_eq!(stats.paused_transfers, 0);
        assert_eq!(
            *stats.transfers_by_class.get(&QoSClass::Critical).unwrap(),
            1
        );
        assert_eq!(*stats.transfers_by_class.get(&QoSClass::Normal).unwrap(), 1);
        assert_eq!(*stats.transfers_by_class.get(&QoSClass::Bulk).unwrap(), 1);
    }

    #[tokio::test]
    async fn test_resume_all() {
        let controller = PreemptionController::new();

        controller
            .register_transfer(QoSClass::Low, 1000, true)
            .await;
        controller
            .register_transfer(QoSClass::Bulk, 1000, true)
            .await;

        controller.pause_transfers_below(QoSClass::Critical).await;
        assert_eq!(controller.paused_count(), 2);

        controller.resume_all().await;
        assert_eq!(controller.paused_count(), 0);
    }
}
