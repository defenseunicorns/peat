//! Radio Scheduler for HIVE-Lite
//!
//! Coordinates radio activities (scan, advertise, sync) to minimize
//! power consumption while maintaining connectivity.

#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(feature = "std")]
use std::collections::VecDeque;

use super::profile::{BatteryState, PowerProfile, RadioTiming};
use crate::NodeId;

/// Priority levels for sync operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SyncPriority {
    /// Background sync, can wait for next scheduled window
    Low = 0,
    /// Normal sync, should happen within current interval
    #[default]
    Normal = 1,
    /// High priority, sync at next opportunity
    High = 2,
    /// Critical data, trigger immediate sync
    Critical = 3,
}

/// A pending sync operation
#[derive(Debug, Clone)]
pub struct PendingSync {
    /// Target peer
    pub peer_id: NodeId,
    /// Priority level
    pub priority: SyncPriority,
    /// Size of data to sync (bytes)
    pub data_size: usize,
    /// When this sync was queued (ms timestamp)
    pub queued_at: u64,
    /// Maximum age before dropping (ms)
    pub max_age_ms: u64,
}

impl PendingSync {
    /// Create a new pending sync
    pub fn new(peer_id: NodeId, priority: SyncPriority, data_size: usize, queued_at: u64) -> Self {
        let max_age_ms = match priority {
            SyncPriority::Low => 60_000,     // 1 minute
            SyncPriority::Normal => 30_000,  // 30 seconds
            SyncPriority::High => 10_000,    // 10 seconds
            SyncPriority::Critical => 5_000, // 5 seconds
        };

        Self {
            peer_id,
            priority,
            data_size,
            queued_at,
            max_age_ms,
        }
    }

    /// Check if this sync has expired
    pub fn is_expired(&self, current_time: u64) -> bool {
        current_time > self.queued_at + self.max_age_ms
    }
}

/// Radio activity state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadioState {
    /// Radio is off/sleeping
    #[default]
    Idle,
    /// Scanning for advertisements
    Scanning,
    /// Sending advertisements
    Advertising,
    /// Connected and syncing
    Syncing,
    /// Transitioning between states
    Transitioning,
}

/// Event from the radio scheduler
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerEvent {
    /// Start scanning
    StartScan,
    /// Stop scanning
    StopScan,
    /// Start advertising
    StartAdvertising,
    /// Stop advertising
    StopAdvertising,
    /// Time to sync with a peer
    SyncNow,
    /// Profile changed
    ProfileChanged,
    /// Enter sleep mode
    EnterSleep,
}

/// Radio scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum pending syncs to queue
    pub max_pending_syncs: usize,
    /// Coalesce syncs within this window (ms)
    pub sync_coalesce_ms: u64,
    /// Minimum time between profile changes (ms)
    pub profile_change_cooldown_ms: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_pending_syncs: 16,
            sync_coalesce_ms: 500,
            profile_change_cooldown_ms: 10_000,
        }
    }
}

/// Radio activity scheduler
///
/// Coordinates scan, advertise, and sync activities according to
/// the current power profile.
#[derive(Debug)]
pub struct RadioScheduler {
    /// Current power profile
    profile: PowerProfile,
    /// Current radio timing
    timing: RadioTiming,
    /// Current radio state
    state: RadioState,
    /// Pending sync operations
    pending_syncs: VecDeque<PendingSync>,
    /// Configuration
    config: SchedulerConfig,
    /// Next scan window start time (ms)
    next_scan_time: u64,
    /// Next advertising event time (ms)
    next_adv_time: u64,
    /// Last state change time (ms)
    last_state_change: u64,
    /// Last profile change time (ms)
    last_profile_change: u64,
    /// Battery state for auto-adjustment
    battery: BatteryState,
    /// Whether auto-profile adjustment is enabled
    auto_adjust_enabled: bool,
    /// Statistics
    stats: SchedulerStats,
}

/// Scheduler statistics
#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    /// Total scan windows
    pub scan_windows: u64,
    /// Total advertising events
    pub adv_events: u64,
    /// Total syncs performed
    pub syncs_performed: u64,
    /// Syncs dropped due to expiration
    pub syncs_dropped: u64,
    /// Critical syncs triggered
    pub critical_syncs: u64,
    /// Profile changes
    pub profile_changes: u64,
}

impl RadioScheduler {
    /// Create a new radio scheduler with the given profile
    pub fn new(profile: PowerProfile, config: SchedulerConfig) -> Self {
        let timing = profile.timing();
        Self {
            profile,
            timing,
            state: RadioState::Idle,
            pending_syncs: VecDeque::new(),
            config,
            next_scan_time: 0,
            next_adv_time: 0,
            last_state_change: 0,
            last_profile_change: 0,
            battery: BatteryState::default(),
            auto_adjust_enabled: true,
            stats: SchedulerStats::default(),
        }
    }

    /// Create with default config
    pub fn with_profile(profile: PowerProfile) -> Self {
        Self::new(profile, SchedulerConfig::default())
    }

    /// Get current profile
    pub fn profile(&self) -> PowerProfile {
        self.profile
    }

    /// Get current timing
    pub fn timing(&self) -> &RadioTiming {
        &self.timing
    }

    /// Get current state
    pub fn state(&self) -> RadioState {
        self.state
    }

    /// Get pending sync count
    pub fn pending_sync_count(&self) -> usize {
        self.pending_syncs.len()
    }

    /// Get statistics
    pub fn stats(&self) -> &SchedulerStats {
        &self.stats
    }

    /// Set power profile
    pub fn set_profile(&mut self, profile: PowerProfile, current_time: u64) -> bool {
        // Check cooldown (skip if this is the first change or if enough time passed)
        let cooldown_elapsed = self.stats.profile_changes == 0
            || current_time >= self.last_profile_change + self.config.profile_change_cooldown_ms;

        if !cooldown_elapsed {
            return false;
        }

        self.profile = profile;
        self.timing = profile.timing();
        self.last_profile_change = current_time;
        self.stats.profile_changes += 1;
        true
    }

    /// Update battery state
    pub fn update_battery(&mut self, battery: BatteryState, current_time: u64) {
        self.battery = battery;

        if self.auto_adjust_enabled {
            let suggested = battery.suggested_profile(self.profile);
            if suggested != self.profile {
                self.set_profile(suggested, current_time);
            }
        }
    }

    /// Enable/disable auto profile adjustment
    pub fn set_auto_adjust(&mut self, enabled: bool) {
        self.auto_adjust_enabled = enabled;
    }

    /// Queue a sync operation
    pub fn queue_sync(
        &mut self,
        peer_id: NodeId,
        priority: SyncPriority,
        data_size: usize,
        current_time: u64,
    ) -> bool {
        // Check queue limit
        if self.pending_syncs.len() >= self.config.max_pending_syncs {
            // Find the lowest priority sync
            let lowest_priority = self
                .pending_syncs
                .iter()
                .map(|s| s.priority)
                .min()
                .unwrap_or(SyncPriority::Critical);

            if priority <= lowest_priority {
                return false;
            }

            // Remove ONE item with lowest priority (oldest first)
            if let Some(idx) = self
                .pending_syncs
                .iter()
                .position(|s| s.priority == lowest_priority)
            {
                self.pending_syncs.remove(idx);
                self.stats.syncs_dropped += 1;
            }
        }

        let sync = PendingSync::new(peer_id, priority, data_size, current_time);
        self.pending_syncs.push_back(sync);
        true
    }

    /// Check if there's a critical sync pending
    pub fn has_critical_sync(&self) -> bool {
        self.pending_syncs
            .iter()
            .any(|s| s.priority == SyncPriority::Critical)
    }

    /// Get the next scheduled event
    pub fn next_event(&self, current_time: u64) -> Option<(SchedulerEvent, u64)> {
        // Critical syncs take priority
        if self.has_critical_sync() {
            return Some((SchedulerEvent::SyncNow, current_time));
        }

        match self.state {
            RadioState::Idle => {
                // Determine next activity
                let scan_due = current_time >= self.next_scan_time;
                let adv_due = current_time >= self.next_adv_time;
                let sync_due = !self.pending_syncs.is_empty();

                if scan_due {
                    Some((SchedulerEvent::StartScan, current_time))
                } else if adv_due {
                    Some((SchedulerEvent::StartAdvertising, current_time))
                } else if sync_due {
                    Some((SchedulerEvent::SyncNow, current_time))
                } else {
                    // Calculate next wake time
                    let next_time = self.next_scan_time.min(self.next_adv_time);
                    Some((SchedulerEvent::EnterSleep, next_time))
                }
            }
            RadioState::Scanning => {
                // Check if scan window should end
                let scan_end = self.last_state_change + self.timing.scan_window_ms as u64;
                if current_time >= scan_end {
                    Some((SchedulerEvent::StopScan, current_time))
                } else {
                    None
                }
            }
            RadioState::Advertising => {
                // Single advertisement event, then stop
                Some((SchedulerEvent::StopAdvertising, current_time))
            }
            RadioState::Syncing => {
                // Sync completion handled externally
                None
            }
            RadioState::Transitioning => None,
        }
    }

    /// Process a scheduler event
    pub fn process_event(&mut self, event: SchedulerEvent, current_time: u64) {
        match event {
            SchedulerEvent::StartScan => {
                self.state = RadioState::Scanning;
                self.last_state_change = current_time;
                self.stats.scan_windows += 1;
            }
            SchedulerEvent::StopScan => {
                self.state = RadioState::Idle;
                self.next_scan_time = current_time + self.timing.scan_interval_ms as u64;
                self.last_state_change = current_time;
            }
            SchedulerEvent::StartAdvertising => {
                self.state = RadioState::Advertising;
                self.last_state_change = current_time;
                self.stats.adv_events += 1;
            }
            SchedulerEvent::StopAdvertising => {
                self.state = RadioState::Idle;
                self.next_adv_time = current_time + self.timing.adv_interval_ms as u64;
                self.last_state_change = current_time;
            }
            SchedulerEvent::SyncNow => {
                self.state = RadioState::Syncing;
                self.last_state_change = current_time;
            }
            SchedulerEvent::ProfileChanged => {
                // Already handled in set_profile
            }
            SchedulerEvent::EnterSleep => {
                self.state = RadioState::Idle;
            }
        }
    }

    /// Get the next pending sync (highest priority, oldest first)
    pub fn next_pending_sync(&mut self, current_time: u64) -> Option<PendingSync> {
        // Remove expired syncs
        let stats = &mut self.stats;
        let initial_len = self.pending_syncs.len();
        self.pending_syncs.retain(|s| !s.is_expired(current_time));
        stats.syncs_dropped += (initial_len - self.pending_syncs.len()) as u64;

        // Find highest priority sync
        let max_priority = self.pending_syncs.iter().map(|s| s.priority).max()?;

        // Find index of oldest sync with max priority
        let idx = self
            .pending_syncs
            .iter()
            .position(|s| s.priority == max_priority)?;

        let sync = self.pending_syncs.remove(idx)?;

        if sync.priority == SyncPriority::Critical {
            self.stats.critical_syncs += 1;
        }
        self.stats.syncs_performed += 1;

        Some(sync)
    }

    /// Mark sync as complete
    pub fn complete_sync(&mut self, current_time: u64) {
        self.state = RadioState::Idle;
        self.last_state_change = current_time;
    }

    /// Reset scheduler state
    pub fn reset(&mut self, current_time: u64) {
        self.state = RadioState::Idle;
        self.pending_syncs.clear();
        self.next_scan_time = current_time;
        self.next_adv_time = current_time;
        self.last_state_change = current_time;
    }

    /// Calculate time until next activity
    pub fn time_until_next_activity(&self, current_time: u64) -> u64 {
        if self.has_critical_sync() {
            return 0;
        }

        if !self.pending_syncs.is_empty() {
            return 0;
        }

        let next_scan = self.next_scan_time.saturating_sub(current_time);
        let next_adv = self.next_adv_time.saturating_sub(current_time);

        next_scan.min(next_adv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_creation() {
        let scheduler = RadioScheduler::with_profile(PowerProfile::LowPower);
        assert_eq!(scheduler.profile(), PowerProfile::LowPower);
        assert_eq!(scheduler.state(), RadioState::Idle);
        assert_eq!(scheduler.pending_sync_count(), 0);
    }

    #[test]
    fn test_queue_sync() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Balanced);
        let peer = NodeId::new(0x1234);

        assert!(scheduler.queue_sync(peer, SyncPriority::Normal, 100, 1000));
        assert_eq!(scheduler.pending_sync_count(), 1);
    }

    #[test]
    fn test_critical_sync_priority() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::LowPower);
        let peer = NodeId::new(0x1234);

        scheduler.queue_sync(peer, SyncPriority::Critical, 50, 1000);
        assert!(scheduler.has_critical_sync());

        let event = scheduler.next_event(1000);
        assert_eq!(event, Some((SchedulerEvent::SyncNow, 1000)));
    }

    #[test]
    fn test_sync_priority_ordering() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Balanced);
        let peer1 = NodeId::new(0x1111);
        let peer2 = NodeId::new(0x2222);
        let peer3 = NodeId::new(0x3333);

        scheduler.queue_sync(peer1, SyncPriority::Low, 100, 1000);
        scheduler.queue_sync(peer2, SyncPriority::High, 100, 1001);
        scheduler.queue_sync(peer3, SyncPriority::Normal, 100, 1002);

        // Should get high priority first
        let sync = scheduler.next_pending_sync(1005).unwrap();
        assert_eq!(sync.peer_id, peer2);

        // Then normal
        let sync = scheduler.next_pending_sync(1005).unwrap();
        assert_eq!(sync.peer_id, peer3);

        // Then low
        let sync = scheduler.next_pending_sync(1005).unwrap();
        assert_eq!(sync.peer_id, peer1);
    }

    #[test]
    fn test_sync_expiration() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Balanced);
        let peer = NodeId::new(0x1234);

        // Queue a low priority sync (1 minute max age)
        scheduler.queue_sync(peer, SyncPriority::Low, 100, 1000);
        assert_eq!(scheduler.pending_sync_count(), 1);

        // After expiration
        let sync = scheduler.next_pending_sync(70_000);
        assert!(sync.is_none());
        assert_eq!(scheduler.stats().syncs_dropped, 1);
    }

    #[test]
    fn test_scan_window_scheduling() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::LowPower);

        // Initial state should trigger scan
        let event = scheduler.next_event(0);
        assert_eq!(event, Some((SchedulerEvent::StartScan, 0)));

        scheduler.process_event(SchedulerEvent::StartScan, 0);
        assert_eq!(scheduler.state(), RadioState::Scanning);

        // After scan window (100ms for LowPower)
        let event = scheduler.next_event(100);
        assert_eq!(event, Some((SchedulerEvent::StopScan, 100)));

        scheduler.process_event(SchedulerEvent::StopScan, 100);
        assert_eq!(scheduler.state(), RadioState::Idle);

        // Next scan should be at interval (5000ms for LowPower)
        assert_eq!(scheduler.next_scan_time, 5100);
    }

    #[test]
    fn test_profile_change() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Balanced);

        assert!(scheduler.set_profile(PowerProfile::LowPower, 1000));
        assert_eq!(scheduler.profile(), PowerProfile::LowPower);
        assert_eq!(scheduler.stats().profile_changes, 1);
    }

    #[test]
    fn test_profile_change_cooldown() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Balanced);

        assert!(scheduler.set_profile(PowerProfile::LowPower, 1000));

        // Too soon - should be rejected
        assert!(!scheduler.set_profile(PowerProfile::Aggressive, 5000));
        assert_eq!(scheduler.profile(), PowerProfile::LowPower);

        // After cooldown
        assert!(scheduler.set_profile(PowerProfile::Aggressive, 15000));
        assert_eq!(scheduler.profile(), PowerProfile::Aggressive);
    }

    #[test]
    fn test_battery_auto_adjust() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Aggressive);
        scheduler.set_auto_adjust(true);

        // Critical battery should force low power
        let battery = BatteryState::new(5, false);
        scheduler.update_battery(battery, 15000);

        assert_eq!(scheduler.profile(), PowerProfile::LowPower);
    }

    #[test]
    fn test_battery_charging_no_change() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Aggressive);
        scheduler.set_auto_adjust(true);

        // Charging should not downgrade
        let battery = BatteryState::new(5, true);
        scheduler.update_battery(battery, 15000);

        assert_eq!(scheduler.profile(), PowerProfile::Aggressive);
    }

    #[test]
    fn test_queue_overflow_priority() {
        let config = SchedulerConfig {
            max_pending_syncs: 2,
            ..Default::default()
        };
        let mut scheduler = RadioScheduler::new(PowerProfile::Balanced, config);

        let peer1 = NodeId::new(0x1111);
        let peer2 = NodeId::new(0x2222);
        let peer3 = NodeId::new(0x3333);

        scheduler.queue_sync(peer1, SyncPriority::Low, 100, 1000);
        scheduler.queue_sync(peer2, SyncPriority::Low, 100, 1001);

        // Queue is full, but high priority should bump low
        assert!(scheduler.queue_sync(peer3, SyncPriority::High, 100, 1002));
        assert_eq!(scheduler.pending_sync_count(), 2);

        // The high priority one should be there
        assert!(scheduler.pending_syncs.iter().any(|s| s.peer_id == peer3));
    }

    #[test]
    fn test_time_until_next_activity() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::LowPower);

        // Start and stop scan to set next_scan_time
        scheduler.process_event(SchedulerEvent::StartScan, 0);
        scheduler.process_event(SchedulerEvent::StopScan, 100);
        // Start and stop advertising to set next_adv_time
        scheduler.process_event(SchedulerEvent::StartAdvertising, 100);
        scheduler.process_event(SchedulerEvent::StopAdvertising, 102);

        // next_scan_time = 5100, next_adv_time = 2102 (LowPower adv_interval = 2000)
        let wait = scheduler.time_until_next_activity(1000);
        assert!(wait > 0, "wait should be > 0, got {}", wait);
        // Should be about 1102ms until next adv (2102 - 1000)
        assert!(wait <= 2000, "wait should be <= 2000, got {}", wait);
    }

    #[test]
    fn test_reset() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Balanced);
        let peer = NodeId::new(0x1234);

        scheduler.queue_sync(peer, SyncPriority::Normal, 100, 1000);
        scheduler.process_event(SchedulerEvent::StartScan, 1000);

        scheduler.reset(2000);

        assert_eq!(scheduler.state(), RadioState::Idle);
        assert_eq!(scheduler.pending_sync_count(), 0);
    }

    #[test]
    fn test_complete_sync() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Balanced);

        scheduler.process_event(SchedulerEvent::SyncNow, 1000);
        assert_eq!(scheduler.state(), RadioState::Syncing);

        scheduler.complete_sync(1500);
        assert_eq!(scheduler.state(), RadioState::Idle);
    }

    #[test]
    fn test_pending_sync_expiry_timing() {
        let sync = PendingSync::new(NodeId::new(0x1234), SyncPriority::Critical, 100, 1000);
        // Critical syncs have 5 second max age
        assert!(!sync.is_expired(5000));
        assert!(sync.is_expired(7000));
    }

    #[test]
    fn test_stats_tracking() {
        let mut scheduler = RadioScheduler::with_profile(PowerProfile::Balanced);

        scheduler.process_event(SchedulerEvent::StartScan, 0);
        scheduler.process_event(SchedulerEvent::StartScan, 100);
        scheduler.process_event(SchedulerEvent::StartAdvertising, 200);

        let stats = scheduler.stats();
        assert_eq!(stats.scan_windows, 2);
        assert_eq!(stats.adv_events, 1);
    }
}
