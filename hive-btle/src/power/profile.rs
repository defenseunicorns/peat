//! Power Profiles for HIVE-Lite
//!
//! Defines power consumption profiles for different use cases,
//! from aggressive (low latency) to low-power (maximum battery life).

/// Radio timing parameters for a power profile
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioTiming {
    /// Scan interval in milliseconds (time between scan windows)
    pub scan_interval_ms: u32,
    /// Scan window duration in milliseconds
    pub scan_window_ms: u32,
    /// Advertising interval in milliseconds
    pub adv_interval_ms: u32,
    /// Connection interval in milliseconds
    pub conn_interval_ms: u32,
    /// Supervision timeout in milliseconds
    pub supervision_timeout_ms: u32,
    /// Slave latency (number of connection events to skip)
    pub slave_latency: u16,
}

impl RadioTiming {
    /// Calculate approximate radio duty cycle as percentage
    pub fn duty_cycle_percent(&self) -> f32 {
        // Scan duty cycle
        let scan_duty = (self.scan_window_ms as f32 / self.scan_interval_ms as f32) * 100.0;

        // Advertising is typically ~2ms per event
        let adv_duration_ms = 2.0;
        let adv_duty = (adv_duration_ms / self.adv_interval_ms as f32) * 100.0;

        // Connection duty (simplified: ~2ms per connection event)
        let conn_duration_ms = 2.0;
        let effective_conn_interval =
            self.conn_interval_ms as f32 * (1.0 + self.slave_latency as f32);
        let conn_duty = (conn_duration_ms / effective_conn_interval) * 100.0;

        // Combined (assuming activities don't overlap perfectly)
        scan_duty + adv_duty + conn_duty
    }

    /// Estimate battery life in hours for a typical smartwatch (300mAh)
    pub fn estimated_battery_hours(&self, battery_capacity_mah: u16) -> f32 {
        // Typical BLE radio: ~15mA active, ~5µA sleep
        let active_current_ma = 15.0;
        let sleep_current_ma = 0.005;

        let duty = self.duty_cycle_percent() / 100.0;
        let average_current = (active_current_ma * duty) + (sleep_current_ma * (1.0 - duty));

        // Add MCU overhead (~5mA average for basic processing)
        let total_current = average_current + 5.0;

        battery_capacity_mah as f32 / total_current
    }
}

/// Power profile presets for different use cases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PowerProfile {
    /// 20% duty cycle, ~6 hour watch battery
    /// Use when low latency is critical (emergency response)
    Aggressive,

    /// 10% duty cycle, ~12 hour watch battery
    /// Good balance between responsiveness and battery
    Balanced,

    /// 2% duty cycle, ~20+ hour watch battery
    /// Default for HIVE-Lite, prioritizes battery life
    #[default]
    LowPower,

    /// Custom profile with user-defined timing
    Custom(RadioTiming),
}

impl PowerProfile {
    /// Get the radio timing for this profile
    pub fn timing(&self) -> RadioTiming {
        match self {
            PowerProfile::Aggressive => RadioTiming {
                scan_interval_ms: 100,
                scan_window_ms: 50,
                adv_interval_ms: 100,
                conn_interval_ms: 15,
                supervision_timeout_ms: 4000,
                slave_latency: 0,
            },
            PowerProfile::Balanced => RadioTiming {
                scan_interval_ms: 500,
                scan_window_ms: 50,
                adv_interval_ms: 500,
                conn_interval_ms: 30,
                supervision_timeout_ms: 4000,
                slave_latency: 2,
            },
            PowerProfile::LowPower => RadioTiming {
                scan_interval_ms: 5000,
                scan_window_ms: 100,
                adv_interval_ms: 2000,
                conn_interval_ms: 100,
                supervision_timeout_ms: 6000,
                slave_latency: 4,
            },
            PowerProfile::Custom(timing) => *timing,
        }
    }

    /// Get the duty cycle for this profile
    pub fn duty_cycle_percent(&self) -> f32 {
        self.timing().duty_cycle_percent()
    }

    /// Get estimated battery life in hours
    pub fn estimated_battery_hours(&self, battery_capacity_mah: u16) -> f32 {
        self.timing().estimated_battery_hours(battery_capacity_mah)
    }

    /// Create a custom profile with specific timing
    pub fn custom(timing: RadioTiming) -> Self {
        PowerProfile::Custom(timing)
    }

    /// Get profile name as string
    pub fn name(&self) -> &'static str {
        match self {
            PowerProfile::Aggressive => "aggressive",
            PowerProfile::Balanced => "balanced",
            PowerProfile::LowPower => "low_power",
            PowerProfile::Custom(_) => "custom",
        }
    }
}

/// Battery state for adaptive profile adjustment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryState {
    /// Current battery level (0-100)
    pub level_percent: u8,
    /// Whether device is charging
    pub is_charging: bool,
    /// Low battery threshold
    pub low_threshold: u8,
    /// Critical battery threshold
    pub critical_threshold: u8,
}

impl Default for BatteryState {
    fn default() -> Self {
        Self {
            level_percent: 100,
            is_charging: false,
            low_threshold: 20,
            critical_threshold: 10,
        }
    }
}

impl BatteryState {
    /// Create a new battery state
    pub fn new(level_percent: u8, is_charging: bool) -> Self {
        Self {
            level_percent: level_percent.min(100),
            is_charging,
            ..Default::default()
        }
    }

    /// Check if battery is low
    pub fn is_low(&self) -> bool {
        !self.is_charging && self.level_percent <= self.low_threshold
    }

    /// Check if battery is critical
    pub fn is_critical(&self) -> bool {
        !self.is_charging && self.level_percent <= self.critical_threshold
    }

    /// Suggest a power profile based on battery state
    pub fn suggested_profile(&self, current: PowerProfile) -> PowerProfile {
        if self.is_charging {
            // When charging, can use more aggressive profile
            current
        } else if self.is_critical() {
            // Critical: force low power
            PowerProfile::LowPower
        } else if self.is_low() {
            // Low: step down if not already at low power
            match current {
                PowerProfile::Aggressive => PowerProfile::Balanced,
                _ => PowerProfile::LowPower,
            }
        } else {
            current
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_defaults() {
        assert_eq!(PowerProfile::default(), PowerProfile::LowPower);
    }

    #[test]
    fn test_aggressive_timing() {
        let timing = PowerProfile::Aggressive.timing();
        assert_eq!(timing.scan_interval_ms, 100);
        assert_eq!(timing.scan_window_ms, 50);
        assert_eq!(timing.adv_interval_ms, 100);
        assert_eq!(timing.conn_interval_ms, 15);
    }

    #[test]
    fn test_balanced_timing() {
        let timing = PowerProfile::Balanced.timing();
        assert_eq!(timing.scan_interval_ms, 500);
        assert_eq!(timing.adv_interval_ms, 500);
    }

    #[test]
    fn test_low_power_timing() {
        let timing = PowerProfile::LowPower.timing();
        assert_eq!(timing.scan_interval_ms, 5000);
        assert_eq!(timing.scan_window_ms, 100);
        assert_eq!(timing.adv_interval_ms, 2000);
    }

    #[test]
    fn test_custom_profile() {
        let custom_timing = RadioTiming {
            scan_interval_ms: 1000,
            scan_window_ms: 100,
            adv_interval_ms: 1000,
            conn_interval_ms: 50,
            supervision_timeout_ms: 5000,
            slave_latency: 3,
        };
        let profile = PowerProfile::custom(custom_timing);
        assert_eq!(profile.timing(), custom_timing);
        assert_eq!(profile.name(), "custom");
    }

    #[test]
    fn test_duty_cycle_ordering() {
        // Aggressive should have highest duty cycle
        let aggressive = PowerProfile::Aggressive.duty_cycle_percent();
        let balanced = PowerProfile::Balanced.duty_cycle_percent();
        let low_power = PowerProfile::LowPower.duty_cycle_percent();

        assert!(aggressive > balanced, "aggressive > balanced");
        assert!(balanced > low_power, "balanced > low_power");
    }

    #[test]
    fn test_low_power_duty_cycle() {
        // Low power should be under 5%
        let duty = PowerProfile::LowPower.duty_cycle_percent();
        assert!(duty < 5.0, "LowPower duty cycle {} should be < 5%", duty);
    }

    #[test]
    fn test_battery_life_ordering() {
        let battery_mah = 300;

        let aggressive = PowerProfile::Aggressive.estimated_battery_hours(battery_mah);
        let balanced = PowerProfile::Balanced.estimated_battery_hours(battery_mah);
        let low_power = PowerProfile::LowPower.estimated_battery_hours(battery_mah);

        // Lower duty cycle = longer battery life
        assert!(low_power > balanced, "low_power > balanced battery life");
        assert!(balanced > aggressive, "balanced > aggressive battery life");
    }

    #[test]
    fn test_battery_state_default() {
        let state = BatteryState::default();
        assert_eq!(state.level_percent, 100);
        assert!(!state.is_charging);
        assert!(!state.is_low());
        assert!(!state.is_critical());
    }

    #[test]
    fn test_battery_state_low() {
        let state = BatteryState::new(20, false);
        assert!(state.is_low());
        assert!(!state.is_critical());
    }

    #[test]
    fn test_battery_state_critical() {
        let state = BatteryState::new(5, false);
        assert!(state.is_low());
        assert!(state.is_critical());
    }

    #[test]
    fn test_battery_charging_not_low() {
        let state = BatteryState::new(10, true);
        assert!(!state.is_low(), "charging should not be considered low");
        assert!(
            !state.is_critical(),
            "charging should not be considered critical"
        );
    }

    #[test]
    fn test_suggested_profile_critical() {
        let state = BatteryState::new(5, false);
        let suggested = state.suggested_profile(PowerProfile::Aggressive);
        assert_eq!(suggested, PowerProfile::LowPower);
    }

    #[test]
    fn test_suggested_profile_low() {
        let state = BatteryState::new(15, false);
        let suggested = state.suggested_profile(PowerProfile::Aggressive);
        assert_eq!(suggested, PowerProfile::Balanced);
    }

    #[test]
    fn test_suggested_profile_charging() {
        let state = BatteryState::new(10, true);
        let suggested = state.suggested_profile(PowerProfile::Aggressive);
        assert_eq!(
            suggested,
            PowerProfile::Aggressive,
            "charging keeps current profile"
        );
    }

    #[test]
    fn test_profile_names() {
        assert_eq!(PowerProfile::Aggressive.name(), "aggressive");
        assert_eq!(PowerProfile::Balanced.name(), "balanced");
        assert_eq!(PowerProfile::LowPower.name(), "low_power");
    }
}
