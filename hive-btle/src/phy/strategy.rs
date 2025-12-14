//! PHY Selection Strategy
//!
//! Defines strategies for automatic PHY selection based on signal quality,
//! distance estimation, and application requirements.

use super::types::{BlePhy, PhyCapabilities};

/// Strategy for automatic PHY selection
#[derive(Debug, Clone, PartialEq)]
pub enum PhyStrategy {
    /// Use a fixed PHY regardless of conditions
    Fixed(BlePhy),

    /// Adaptively select PHY based on RSSI
    Adaptive {
        /// Switch to LE 2M above this RSSI (stronger signal)
        rssi_threshold_high: i8,
        /// Switch to Coded PHY below this RSSI (weaker signal)
        rssi_threshold_low: i8,
        /// RSSI difference required to trigger switch (prevent oscillation)
        hysteresis_db: u8,
        /// Preferred coded PHY when switching to long range
        coded_phy: BlePhy,
    },

    /// Always use maximum range PHY
    MaxRange,

    /// Always use maximum throughput PHY
    MaxThroughput,

    /// Power-optimized: prefer faster PHYs when signal is strong
    PowerOptimized {
        /// Switch to 2M above this RSSI
        rssi_threshold: i8,
    },
}

impl Default for PhyStrategy {
    fn default() -> Self {
        PhyStrategy::Adaptive {
            rssi_threshold_high: -50,
            rssi_threshold_low: -75,
            hysteresis_db: 5,
            coded_phy: BlePhy::LeCodedS2,
        }
    }
}

impl PhyStrategy {
    /// Create a fixed strategy using specified PHY
    pub fn fixed(phy: BlePhy) -> Self {
        PhyStrategy::Fixed(phy)
    }

    /// Create adaptive strategy with custom thresholds
    pub fn adaptive(high_threshold: i8, low_threshold: i8, hysteresis: u8) -> Self {
        PhyStrategy::Adaptive {
            rssi_threshold_high: high_threshold,
            rssi_threshold_low: low_threshold,
            hysteresis_db: hysteresis,
            coded_phy: BlePhy::LeCodedS2,
        }
    }

    /// Create adaptive strategy for maximum range fallback
    pub fn adaptive_max_range() -> Self {
        PhyStrategy::Adaptive {
            rssi_threshold_high: -50,
            rssi_threshold_low: -70,
            hysteresis_db: 5,
            coded_phy: BlePhy::LeCodedS8,
        }
    }

    /// Select appropriate PHY based on current conditions
    pub fn select_phy(
        &self,
        current_phy: BlePhy,
        rssi: i8,
        capabilities: &PhyCapabilities,
    ) -> BlePhy {
        let selected = match self {
            PhyStrategy::Fixed(phy) => *phy,
            PhyStrategy::Adaptive {
                rssi_threshold_high,
                rssi_threshold_low,
                hysteresis_db,
                coded_phy,
            } => {
                // Apply hysteresis based on current PHY
                let (high_thresh, low_thresh) = if current_phy == BlePhy::Le2M {
                    // Currently on 2M, need stronger signal to stay
                    (
                        *rssi_threshold_high - *hysteresis_db as i8,
                        *rssi_threshold_low,
                    )
                } else if current_phy.is_coded() {
                    // Currently on coded, need weaker signal to stay
                    (
                        *rssi_threshold_high,
                        *rssi_threshold_low + *hysteresis_db as i8,
                    )
                } else {
                    (*rssi_threshold_high, *rssi_threshold_low)
                };

                if rssi > high_thresh {
                    BlePhy::Le2M
                } else if rssi < low_thresh {
                    *coded_phy
                } else {
                    BlePhy::Le1M
                }
            }
            PhyStrategy::MaxRange => {
                if capabilities.le_coded {
                    BlePhy::LeCodedS8
                } else {
                    BlePhy::Le1M
                }
            }
            PhyStrategy::MaxThroughput => {
                if capabilities.le_2m {
                    BlePhy::Le2M
                } else {
                    BlePhy::Le1M
                }
            }
            PhyStrategy::PowerOptimized { rssi_threshold } => {
                if rssi > *rssi_threshold && capabilities.le_2m {
                    BlePhy::Le2M // Faster = shorter airtime = less power
                } else {
                    BlePhy::Le1M
                }
            }
        };

        // Validate against capabilities
        if capabilities.supports(selected) {
            selected
        } else {
            BlePhy::Le1M // Fallback to always-supported PHY
        }
    }

    /// Get strategy name
    pub fn name(&self) -> &'static str {
        match self {
            PhyStrategy::Fixed(_) => "fixed",
            PhyStrategy::Adaptive { .. } => "adaptive",
            PhyStrategy::MaxRange => "max_range",
            PhyStrategy::MaxThroughput => "max_throughput",
            PhyStrategy::PowerOptimized { .. } => "power_optimized",
        }
    }

    /// Check if strategy requires capability negotiation
    pub fn requires_capability_check(&self) -> bool {
        !matches!(self, PhyStrategy::Fixed(BlePhy::Le1M))
    }
}

/// PHY switching decision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhySwitchDecision {
    /// Keep current PHY
    Keep,
    /// Switch to new PHY
    Switch(BlePhy),
}

impl PhySwitchDecision {
    /// Check if a switch is recommended
    pub fn should_switch(&self) -> bool {
        matches!(self, PhySwitchDecision::Switch(_))
    }

    /// Get the target PHY if switching
    pub fn target(&self) -> Option<BlePhy> {
        match self {
            PhySwitchDecision::Keep => None,
            PhySwitchDecision::Switch(phy) => Some(*phy),
        }
    }
}

/// Evaluate whether to switch PHY based on strategy
pub fn evaluate_phy_switch(
    strategy: &PhyStrategy,
    current_phy: BlePhy,
    rssi: i8,
    capabilities: &PhyCapabilities,
) -> PhySwitchDecision {
    let recommended = strategy.select_phy(current_phy, rssi, capabilities);
    if recommended != current_phy {
        PhySwitchDecision::Switch(recommended)
    } else {
        PhySwitchDecision::Keep
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_default() {
        let strategy = PhyStrategy::default();
        assert_eq!(strategy.name(), "adaptive");
    }

    #[test]
    fn test_fixed_strategy() {
        let strategy = PhyStrategy::fixed(BlePhy::LeCodedS8);
        let caps = PhyCapabilities::ble5_full();

        // Always returns the fixed PHY regardless of RSSI
        assert_eq!(
            strategy.select_phy(BlePhy::Le1M, -30, &caps),
            BlePhy::LeCodedS8
        );
        assert_eq!(
            strategy.select_phy(BlePhy::Le1M, -90, &caps),
            BlePhy::LeCodedS8
        );
    }

    #[test]
    fn test_fixed_strategy_capability_fallback() {
        let strategy = PhyStrategy::fixed(BlePhy::LeCodedS8);
        let caps = PhyCapabilities::le_1m_only();

        // Falls back to LE 1M if coded not supported
        assert_eq!(strategy.select_phy(BlePhy::Le1M, -50, &caps), BlePhy::Le1M);
    }

    #[test]
    fn test_adaptive_strong_signal() {
        let strategy = PhyStrategy::default();
        let caps = PhyCapabilities::ble5_full();

        // Strong signal (-40 dBm) should use 2M
        assert_eq!(strategy.select_phy(BlePhy::Le1M, -40, &caps), BlePhy::Le2M);
    }

    #[test]
    fn test_adaptive_medium_signal() {
        let strategy = PhyStrategy::default();
        let caps = PhyCapabilities::ble5_full();

        // Medium signal (-60 dBm) should use 1M
        assert_eq!(strategy.select_phy(BlePhy::Le1M, -60, &caps), BlePhy::Le1M);
    }

    #[test]
    fn test_adaptive_weak_signal() {
        let strategy = PhyStrategy::default();
        let caps = PhyCapabilities::ble5_full();

        // Weak signal (-80 dBm) should use Coded
        assert!(strategy.select_phy(BlePhy::Le1M, -80, &caps).is_coded());
    }

    #[test]
    fn test_adaptive_hysteresis() {
        let strategy = PhyStrategy::Adaptive {
            rssi_threshold_high: -50,
            rssi_threshold_low: -75,
            hysteresis_db: 5,
            coded_phy: BlePhy::LeCodedS2,
        };
        let caps = PhyCapabilities::ble5_full();

        // Hysteresis prevents oscillation:
        // - From 1M: threshold is -50, so -48 > -50 → switch to 2M
        // - From 2M: threshold is -55 (with hysteresis), so -48 > -55 → stay on 2M
        let from_1m = strategy.select_phy(BlePhy::Le1M, -48, &caps);
        let from_2m = strategy.select_phy(BlePhy::Le2M, -48, &caps);

        assert_eq!(from_1m, BlePhy::Le2M);
        assert_eq!(from_2m, BlePhy::Le2M); // Hysteresis keeps it on 2M

        // At -52 (below threshold -50 but above hysteresis -55):
        // - From 1M: threshold is -50, so -52 < -50 → stay on 1M
        // - From 2M: threshold is -55, so -52 > -55 → stay on 2M
        let at_52_from_1m = strategy.select_phy(BlePhy::Le1M, -52, &caps);
        let at_52_from_2m = strategy.select_phy(BlePhy::Le2M, -52, &caps);

        assert_eq!(at_52_from_1m, BlePhy::Le1M);
        assert_eq!(at_52_from_2m, BlePhy::Le2M);
    }

    #[test]
    fn test_max_range() {
        let strategy = PhyStrategy::MaxRange;
        let caps = PhyCapabilities::ble5_full();

        assert_eq!(
            strategy.select_phy(BlePhy::Le1M, -30, &caps),
            BlePhy::LeCodedS8
        );
    }

    #[test]
    fn test_max_range_no_coded() {
        let strategy = PhyStrategy::MaxRange;
        let caps = PhyCapabilities::ble5_no_coded();

        assert_eq!(strategy.select_phy(BlePhy::Le1M, -30, &caps), BlePhy::Le1M);
    }

    #[test]
    fn test_max_throughput() {
        let strategy = PhyStrategy::MaxThroughput;
        let caps = PhyCapabilities::ble5_full();

        assert_eq!(strategy.select_phy(BlePhy::Le1M, -80, &caps), BlePhy::Le2M);
    }

    #[test]
    fn test_power_optimized_strong() {
        let strategy = PhyStrategy::PowerOptimized {
            rssi_threshold: -55,
        };
        let caps = PhyCapabilities::ble5_full();

        // Strong signal uses 2M for power savings
        assert_eq!(strategy.select_phy(BlePhy::Le1M, -40, &caps), BlePhy::Le2M);
    }

    #[test]
    fn test_power_optimized_weak() {
        let strategy = PhyStrategy::PowerOptimized {
            rssi_threshold: -55,
        };
        let caps = PhyCapabilities::ble5_full();

        // Weak signal uses 1M (more reliable)
        assert_eq!(strategy.select_phy(BlePhy::Le1M, -70, &caps), BlePhy::Le1M);
    }

    #[test]
    fn test_switch_decision_keep() {
        let strategy = PhyStrategy::fixed(BlePhy::Le1M);
        let caps = PhyCapabilities::ble5_full();

        let decision = evaluate_phy_switch(&strategy, BlePhy::Le1M, -50, &caps);
        assert_eq!(decision, PhySwitchDecision::Keep);
        assert!(!decision.should_switch());
        assert!(decision.target().is_none());
    }

    #[test]
    fn test_switch_decision_switch() {
        let strategy = PhyStrategy::MaxThroughput;
        let caps = PhyCapabilities::ble5_full();

        let decision = evaluate_phy_switch(&strategy, BlePhy::Le1M, -50, &caps);
        assert_eq!(decision, PhySwitchDecision::Switch(BlePhy::Le2M));
        assert!(decision.should_switch());
        assert_eq!(decision.target(), Some(BlePhy::Le2M));
    }

    #[test]
    fn test_strategy_names() {
        assert_eq!(PhyStrategy::fixed(BlePhy::Le1M).name(), "fixed");
        assert_eq!(PhyStrategy::MaxRange.name(), "max_range");
        assert_eq!(PhyStrategy::MaxThroughput.name(), "max_throughput");
    }

    #[test]
    fn test_requires_capability_check() {
        assert!(!PhyStrategy::fixed(BlePhy::Le1M).requires_capability_check());
        assert!(PhyStrategy::fixed(BlePhy::Le2M).requires_capability_check());
        assert!(PhyStrategy::MaxRange.requires_capability_check());
    }
}
