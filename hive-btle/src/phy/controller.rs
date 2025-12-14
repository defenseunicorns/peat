//! PHY Controller
//!
//! Manages PHY selection and switching for BLE connections,
//! handling the state machine and platform-specific operations.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::strategy::{evaluate_phy_switch, PhyStrategy, PhySwitchDecision};
use super::types::{BlePhy, PhyCapabilities, PhyPreference};

/// PHY controller state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhyControllerState {
    /// Not initialized or no connection
    #[default]
    Idle,
    /// Negotiating PHY capabilities
    Negotiating,
    /// Operating with current PHY
    Active,
    /// Switching to a new PHY
    Switching,
    /// Error state
    Error,
}

/// PHY update result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyUpdateResult {
    /// PHY update succeeded
    Success {
        /// New TX PHY
        tx_phy: BlePhy,
        /// New RX PHY
        rx_phy: BlePhy,
    },
    /// PHY update rejected by peer
    Rejected,
    /// PHY update not supported
    NotSupported,
    /// PHY update timed out
    Timeout,
    /// PHY update failed
    Failed,
}

/// Event from the PHY controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyControllerEvent {
    /// PHY negotiation complete
    NegotiationComplete {
        /// Local capabilities
        local: PhyCapabilities,
        /// Peer capabilities
        peer: PhyCapabilities,
    },
    /// PHY switch recommended
    SwitchRecommended {
        /// Current PHY
        from: BlePhy,
        /// Recommended PHY
        to: BlePhy,
        /// Current RSSI that triggered recommendation
        rssi: i8,
    },
    /// PHY update completed
    UpdateComplete(PhyUpdateResult),
    /// RSSI measurement received
    RssiUpdate(i8),
}

/// PHY controller statistics
#[derive(Debug, Clone, Default)]
pub struct PhyStats {
    /// Number of PHY switches
    pub switches: u64,
    /// Successful switches
    pub successful_switches: u64,
    /// Failed switches
    pub failed_switches: u64,
    /// RSSI samples collected
    pub rssi_samples: u64,
    /// Time spent in each PHY (arbitrary units)
    pub time_in_le1m: u64,
    /// Time in LE 2M
    pub time_in_le2m: u64,
    /// Time in LE Coded
    pub time_in_coded: u64,
}

impl PhyStats {
    /// Get switch success rate
    pub fn success_rate(&self) -> f32 {
        if self.switches == 0 {
            1.0
        } else {
            self.successful_switches as f32 / self.switches as f32
        }
    }

    /// Record time in current PHY
    pub fn record_time(&mut self, phy: BlePhy, time_units: u64) {
        match phy {
            BlePhy::Le1M => self.time_in_le1m += time_units,
            BlePhy::Le2M => self.time_in_le2m += time_units,
            BlePhy::LeCodedS2 | BlePhy::LeCodedS8 => self.time_in_coded += time_units,
        }
    }
}

/// PHY controller configuration
#[derive(Debug, Clone)]
pub struct PhyControllerConfig {
    /// PHY selection strategy
    pub strategy: PhyStrategy,
    /// Minimum RSSI samples before considering switch
    pub min_samples_for_switch: usize,
    /// RSSI averaging window size
    pub rssi_window_size: usize,
    /// Minimum time between switches (milliseconds)
    pub switch_cooldown_ms: u64,
    /// Enable automatic PHY switching
    pub auto_switch: bool,
}

impl Default for PhyControllerConfig {
    fn default() -> Self {
        Self {
            strategy: PhyStrategy::default(),
            min_samples_for_switch: 5,
            rssi_window_size: 10,
            switch_cooldown_ms: 5000,
            auto_switch: true,
        }
    }
}

/// PHY Controller
///
/// Manages PHY selection, switching, and monitoring for a BLE connection.
#[derive(Debug)]
pub struct PhyController {
    /// Configuration
    config: PhyControllerConfig,
    /// Current state
    state: PhyControllerState,
    /// Current TX PHY
    tx_phy: BlePhy,
    /// Current RX PHY
    rx_phy: BlePhy,
    /// Local PHY capabilities
    local_caps: PhyCapabilities,
    /// Peer PHY capabilities
    peer_caps: PhyCapabilities,
    /// RSSI samples
    rssi_samples: Vec<i8>,
    /// Last switch time (ms timestamp)
    last_switch_time: u64,
    /// Statistics
    stats: PhyStats,
}

impl PhyController {
    /// Create a new PHY controller
    pub fn new(config: PhyControllerConfig, local_caps: PhyCapabilities) -> Self {
        Self {
            config,
            state: PhyControllerState::Idle,
            tx_phy: BlePhy::Le1M,
            rx_phy: BlePhy::Le1M,
            local_caps,
            peer_caps: PhyCapabilities::default(),
            rssi_samples: Vec::new(),
            last_switch_time: 0,
            stats: PhyStats::default(),
        }
    }

    /// Create with default config
    pub fn with_defaults(local_caps: PhyCapabilities) -> Self {
        Self::new(PhyControllerConfig::default(), local_caps)
    }

    /// Get current state
    pub fn state(&self) -> PhyControllerState {
        self.state
    }

    /// Get current TX PHY
    pub fn tx_phy(&self) -> BlePhy {
        self.tx_phy
    }

    /// Get current RX PHY
    pub fn rx_phy(&self) -> BlePhy {
        self.rx_phy
    }

    /// Get current PHY preference
    pub fn current_preference(&self) -> PhyPreference {
        PhyPreference {
            tx: self.tx_phy,
            rx: self.rx_phy,
        }
    }

    /// Get effective capabilities (intersection of local and peer)
    pub fn effective_capabilities(&self) -> PhyCapabilities {
        PhyCapabilities {
            le_2m: self.local_caps.le_2m && self.peer_caps.le_2m,
            le_coded: self.local_caps.le_coded && self.peer_caps.le_coded,
        }
    }

    /// Get statistics
    pub fn stats(&self) -> &PhyStats {
        &self.stats
    }

    /// Get config
    pub fn config(&self) -> &PhyControllerConfig {
        &self.config
    }

    /// Start PHY negotiation for a new connection
    pub fn start_negotiation(&mut self) {
        self.state = PhyControllerState::Negotiating;
        self.rssi_samples.clear();
    }

    /// Complete negotiation with peer capabilities
    pub fn complete_negotiation(&mut self, peer_caps: PhyCapabilities) -> PhyControllerEvent {
        self.peer_caps = peer_caps;
        self.state = PhyControllerState::Active;

        PhyControllerEvent::NegotiationComplete {
            local: self.local_caps,
            peer: peer_caps,
        }
    }

    /// Record an RSSI measurement
    pub fn record_rssi(&mut self, rssi: i8, current_time: u64) -> Option<PhyControllerEvent> {
        self.rssi_samples.push(rssi);
        self.stats.rssi_samples += 1;

        // Keep only recent samples
        if self.rssi_samples.len() > self.config.rssi_window_size {
            self.rssi_samples.remove(0);
        }

        // Check for PHY switch if enabled and have enough samples
        if self.config.auto_switch
            && self.state == PhyControllerState::Active
            && self.rssi_samples.len() >= self.config.min_samples_for_switch
            && current_time >= self.last_switch_time + self.config.switch_cooldown_ms
        {
            let avg_rssi = self.average_rssi();
            let decision = self.evaluate_switch(avg_rssi);

            if let PhySwitchDecision::Switch(to_phy) = decision {
                return Some(PhyControllerEvent::SwitchRecommended {
                    from: self.tx_phy,
                    to: to_phy,
                    rssi: avg_rssi,
                });
            }
        }

        None
    }

    /// Get average RSSI from samples
    pub fn average_rssi(&self) -> i8 {
        if self.rssi_samples.is_empty() {
            return -100;
        }
        let sum: i32 = self.rssi_samples.iter().map(|&r| r as i32).sum();
        (sum / self.rssi_samples.len() as i32) as i8
    }

    /// Evaluate whether to switch PHY
    pub fn evaluate_switch(&self, rssi: i8) -> PhySwitchDecision {
        let effective_caps = self.effective_capabilities();
        evaluate_phy_switch(&self.config.strategy, self.tx_phy, rssi, &effective_caps)
    }

    /// Request a PHY update
    pub fn request_switch(&mut self, to_phy: BlePhy) -> Option<PhyPreference> {
        if self.state != PhyControllerState::Active {
            return None;
        }

        let effective_caps = self.effective_capabilities();
        if !effective_caps.supports(to_phy) {
            return None;
        }

        self.state = PhyControllerState::Switching;
        self.stats.switches += 1;

        Some(PhyPreference::symmetric(to_phy))
    }

    /// Handle PHY update result from stack
    pub fn handle_update_result(
        &mut self,
        result: PhyUpdateResult,
        current_time: u64,
    ) -> PhyControllerEvent {
        match result {
            PhyUpdateResult::Success { tx_phy, rx_phy } => {
                self.tx_phy = tx_phy;
                self.rx_phy = rx_phy;
                self.last_switch_time = current_time;
                self.state = PhyControllerState::Active;
                self.stats.successful_switches += 1;
            }
            PhyUpdateResult::Rejected
            | PhyUpdateResult::NotSupported
            | PhyUpdateResult::Timeout
            | PhyUpdateResult::Failed => {
                self.state = PhyControllerState::Active;
                self.stats.failed_switches += 1;
            }
        }

        PhyControllerEvent::UpdateComplete(result)
    }

    /// Reset controller state
    pub fn reset(&mut self) {
        self.state = PhyControllerState::Idle;
        self.tx_phy = BlePhy::Le1M;
        self.rx_phy = BlePhy::Le1M;
        self.peer_caps = PhyCapabilities::default();
        self.rssi_samples.clear();
        self.last_switch_time = 0;
    }

    /// Set PHY strategy
    pub fn set_strategy(&mut self, strategy: PhyStrategy) {
        self.config.strategy = strategy;
    }

    /// Enable/disable auto switching
    pub fn set_auto_switch(&mut self, enabled: bool) {
        self.config.auto_switch = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_controller() -> PhyController {
        let caps = PhyCapabilities::ble5_full();
        PhyController::with_defaults(caps)
    }

    #[test]
    fn test_controller_creation() {
        let ctrl = make_controller();
        assert_eq!(ctrl.state(), PhyControllerState::Idle);
        assert_eq!(ctrl.tx_phy(), BlePhy::Le1M);
        assert_eq!(ctrl.rx_phy(), BlePhy::Le1M);
    }

    #[test]
    fn test_negotiation_flow() {
        let mut ctrl = make_controller();

        ctrl.start_negotiation();
        assert_eq!(ctrl.state(), PhyControllerState::Negotiating);

        let event = ctrl.complete_negotiation(PhyCapabilities::ble5_full());
        assert_eq!(ctrl.state(), PhyControllerState::Active);

        if let PhyControllerEvent::NegotiationComplete { local, peer } = event {
            assert!(local.le_2m);
            assert!(peer.le_coded);
        } else {
            panic!("Expected NegotiationComplete event");
        }
    }

    #[test]
    fn test_effective_capabilities() {
        let mut ctrl = make_controller();
        ctrl.complete_negotiation(PhyCapabilities::ble5_no_coded());

        let effective = ctrl.effective_capabilities();
        assert!(effective.le_2m);
        assert!(!effective.le_coded); // Peer doesn't support
    }

    #[test]
    fn test_rssi_recording() {
        let mut ctrl = make_controller();
        ctrl.complete_negotiation(PhyCapabilities::ble5_full());

        for i in 0..5 {
            ctrl.record_rssi(-50 - i, 1000 + i as u64 * 100);
        }

        let avg = ctrl.average_rssi();
        assert!((-55..=-50).contains(&avg));
    }

    #[test]
    fn test_rssi_window_limit() {
        let mut ctrl = make_controller();
        ctrl.complete_negotiation(PhyCapabilities::ble5_full());

        // Add more samples than window size
        for i in 0..20 {
            ctrl.record_rssi(-50, i * 100);
        }

        assert_eq!(ctrl.rssi_samples.len(), ctrl.config.rssi_window_size);
    }

    #[test]
    fn test_switch_request() {
        let mut ctrl = make_controller();
        ctrl.complete_negotiation(PhyCapabilities::ble5_full());

        let pref = ctrl.request_switch(BlePhy::Le2M);
        assert!(pref.is_some());
        assert_eq!(ctrl.state(), PhyControllerState::Switching);
    }

    #[test]
    fn test_switch_request_unsupported() {
        let mut ctrl = make_controller();
        ctrl.complete_negotiation(PhyCapabilities::le_1m_only());

        let pref = ctrl.request_switch(BlePhy::LeCodedS8);
        assert!(pref.is_none()); // Peer doesn't support
    }

    #[test]
    fn test_update_result_success() {
        let mut ctrl = make_controller();
        ctrl.complete_negotiation(PhyCapabilities::ble5_full());
        ctrl.request_switch(BlePhy::Le2M);

        let result = PhyUpdateResult::Success {
            tx_phy: BlePhy::Le2M,
            rx_phy: BlePhy::Le2M,
        };
        ctrl.handle_update_result(result, 5000);

        assert_eq!(ctrl.state(), PhyControllerState::Active);
        assert_eq!(ctrl.tx_phy(), BlePhy::Le2M);
        assert_eq!(ctrl.rx_phy(), BlePhy::Le2M);
        assert_eq!(ctrl.stats().successful_switches, 1);
    }

    #[test]
    fn test_update_result_rejected() {
        let mut ctrl = make_controller();
        ctrl.complete_negotiation(PhyCapabilities::ble5_full());
        ctrl.request_switch(BlePhy::Le2M);

        ctrl.handle_update_result(PhyUpdateResult::Rejected, 5000);

        assert_eq!(ctrl.state(), PhyControllerState::Active);
        assert_eq!(ctrl.tx_phy(), BlePhy::Le1M); // Unchanged
        assert_eq!(ctrl.stats().failed_switches, 1);
    }

    #[test]
    fn test_auto_switch_recommendation() {
        let config = PhyControllerConfig {
            min_samples_for_switch: 3,
            switch_cooldown_ms: 0, // No cooldown for test
            ..Default::default()
        };
        let caps = PhyCapabilities::ble5_full();
        let mut ctrl = PhyController::new(config, caps);
        ctrl.complete_negotiation(PhyCapabilities::ble5_full());

        // Record strong RSSI samples
        for i in 0..5 {
            let event = ctrl.record_rssi(-40, i * 100);
            if i >= 2 {
                // After min_samples_for_switch
                if let Some(PhyControllerEvent::SwitchRecommended { to, .. }) = event {
                    assert_eq!(to, BlePhy::Le2M);
                    return; // Test passed
                }
            }
        }

        panic!("Expected switch recommendation for strong signal");
    }

    #[test]
    fn test_switch_cooldown() {
        let config = PhyControllerConfig {
            min_samples_for_switch: 2,
            switch_cooldown_ms: 5000,
            ..Default::default()
        };
        let caps = PhyCapabilities::ble5_full();
        let mut ctrl = PhyController::new(config, caps);
        ctrl.complete_negotiation(PhyCapabilities::ble5_full());

        // Simulate a recent switch
        ctrl.last_switch_time = 1000;

        // Record samples at time 2000 (within cooldown)
        let event = ctrl.record_rssi(-40, 2000);
        assert!(event.is_none()); // Cooldown not expired

        let event = ctrl.record_rssi(-40, 2100);
        assert!(event.is_none()); // Still in cooldown
    }

    #[test]
    fn test_reset() {
        let mut ctrl = make_controller();
        ctrl.complete_negotiation(PhyCapabilities::ble5_full());
        ctrl.record_rssi(-50, 1000);

        ctrl.reset();

        assert_eq!(ctrl.state(), PhyControllerState::Idle);
        assert_eq!(ctrl.tx_phy(), BlePhy::Le1M);
        assert!(ctrl.rssi_samples.is_empty());
    }

    #[test]
    fn test_stats_success_rate() {
        let mut stats = PhyStats::default();
        assert_eq!(stats.success_rate(), 1.0);

        stats.switches = 10;
        stats.successful_switches = 8;
        stats.failed_switches = 2;
        assert!((stats.success_rate() - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_stats_record_time() {
        let mut stats = PhyStats::default();

        stats.record_time(BlePhy::Le1M, 100);
        stats.record_time(BlePhy::Le2M, 50);
        stats.record_time(BlePhy::LeCodedS8, 200);

        assert_eq!(stats.time_in_le1m, 100);
        assert_eq!(stats.time_in_le2m, 50);
        assert_eq!(stats.time_in_coded, 200);
    }
}
