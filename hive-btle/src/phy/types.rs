//! BLE PHY Types
//!
//! Defines the available Bluetooth Low Energy physical layer configurations
//! for different range/throughput tradeoffs.

/// BLE Physical Layer (PHY) options
///
/// BLE 5.0 introduced multiple PHY options for different use cases:
/// - LE 1M: Standard 1 Mbps rate, good range
/// - LE 2M: High speed 2 Mbps, reduced range
/// - LE Coded: Long range mode with error correction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlePhy {
    /// LE 1M PHY - 1 Mbps, ~100m range
    ///
    /// The standard BLE PHY, compatible with all BLE devices.
    /// Good balance of speed and range.
    #[default]
    Le1M,

    /// LE 2M PHY - 2 Mbps, ~50m range
    ///
    /// Double the data rate but reduced range.
    /// Use for high-throughput short-range links.
    Le2M,

    /// LE Coded S=2 PHY - 500 kbps, ~200m range
    ///
    /// Coded PHY with 2x redundancy.
    /// Good balance of range and throughput.
    LeCodedS2,

    /// LE Coded S=8 PHY - 125 kbps, ~400m range
    ///
    /// Coded PHY with 8x redundancy.
    /// Maximum range but lowest throughput.
    LeCodedS8,
}

impl BlePhy {
    /// Get the data rate in bits per second
    pub fn data_rate_bps(&self) -> u32 {
        match self {
            BlePhy::Le1M => 1_000_000,
            BlePhy::Le2M => 2_000_000,
            BlePhy::LeCodedS2 => 500_000,
            BlePhy::LeCodedS8 => 125_000,
        }
    }

    /// Get the data rate in kilobits per second
    pub fn data_rate_kbps(&self) -> u32 {
        self.data_rate_bps() / 1000
    }

    /// Get typical maximum range in meters (line of sight)
    pub fn typical_range_m(&self) -> u16 {
        match self {
            BlePhy::Le1M => 100,
            BlePhy::Le2M => 50,
            BlePhy::LeCodedS2 => 200,
            BlePhy::LeCodedS8 => 400,
        }
    }

    /// Get typical latency in milliseconds for a connection event
    pub fn typical_latency_ms(&self) -> u16 {
        match self {
            BlePhy::Le1M => 30,
            BlePhy::Le2M => 20,
            BlePhy::LeCodedS2 => 50,
            BlePhy::LeCodedS8 => 100,
        }
    }

    /// Check if this is a coded PHY (long range)
    pub fn is_coded(&self) -> bool {
        matches!(self, BlePhy::LeCodedS2 | BlePhy::LeCodedS8)
    }

    /// Check if this requires BLE 5.0
    pub fn requires_ble5(&self) -> bool {
        !matches!(self, BlePhy::Le1M)
    }

    /// Get the coding scheme (S value) for coded PHYs
    pub fn coding_scheme(&self) -> Option<u8> {
        match self {
            BlePhy::LeCodedS2 => Some(2),
            BlePhy::LeCodedS8 => Some(8),
            _ => None,
        }
    }

    /// Get PHY name as string
    pub fn name(&self) -> &'static str {
        match self {
            BlePhy::Le1M => "LE 1M",
            BlePhy::Le2M => "LE 2M",
            BlePhy::LeCodedS2 => "LE Coded S=2",
            BlePhy::LeCodedS8 => "LE Coded S=8",
        }
    }

    /// Calculate approximate time to transmit data
    pub fn transmit_time_us(&self, bytes: usize) -> u64 {
        // Bits to transmit (including overhead)
        let bits = (bytes + 10) * 8; // +10 for BLE overhead
        let rate = self.data_rate_bps() as u64;
        (bits as u64 * 1_000_000) / rate
    }

    /// Estimate power consumption relative to LE 1M (1.0 = baseline)
    pub fn relative_power(&self) -> f32 {
        match self {
            BlePhy::Le1M => 1.0,
            BlePhy::Le2M => 0.8,      // Shorter airtime
            BlePhy::LeCodedS2 => 1.5, // More processing
            BlePhy::LeCodedS8 => 2.0, // Much more processing
        }
    }
}

impl core::fmt::Display for BlePhy {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// PHY capabilities of a device
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhyCapabilities {
    /// Supports LE 2M PHY
    pub le_2m: bool,
    /// Supports LE Coded PHY
    pub le_coded: bool,
}

impl PhyCapabilities {
    /// Device supports only LE 1M (BLE 4.x)
    pub fn le_1m_only() -> Self {
        Self {
            le_2m: false,
            le_coded: false,
        }
    }

    /// Device supports all BLE 5.0 PHYs
    pub fn ble5_full() -> Self {
        Self {
            le_2m: true,
            le_coded: true,
        }
    }

    /// Device supports LE 1M and LE 2M only
    pub fn ble5_no_coded() -> Self {
        Self {
            le_2m: true,
            le_coded: false,
        }
    }

    /// Check if a specific PHY is supported
    pub fn supports(&self, phy: BlePhy) -> bool {
        match phy {
            BlePhy::Le1M => true, // Always supported
            BlePhy::Le2M => self.le_2m,
            BlePhy::LeCodedS2 | BlePhy::LeCodedS8 => self.le_coded,
        }
    }

    /// Get the best supported PHY for range
    pub fn best_for_range(&self) -> BlePhy {
        if self.le_coded {
            BlePhy::LeCodedS8
        } else {
            BlePhy::Le1M
        }
    }

    /// Get the best supported PHY for throughput
    pub fn best_for_throughput(&self) -> BlePhy {
        if self.le_2m {
            BlePhy::Le2M
        } else {
            BlePhy::Le1M
        }
    }
}

/// PHY preference for connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhyPreference {
    /// Preferred TX PHY
    pub tx: BlePhy,
    /// Preferred RX PHY
    pub rx: BlePhy,
}

impl Default for PhyPreference {
    fn default() -> Self {
        Self {
            tx: BlePhy::Le1M,
            rx: BlePhy::Le1M,
        }
    }
}

impl PhyPreference {
    /// Create symmetric preference (same PHY for TX and RX)
    pub fn symmetric(phy: BlePhy) -> Self {
        Self { tx: phy, rx: phy }
    }

    /// Check if TX and RX PHYs match
    pub fn is_symmetric(&self) -> bool {
        self.tx == self.rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phy_default() {
        assert_eq!(BlePhy::default(), BlePhy::Le1M);
    }

    #[test]
    fn test_phy_data_rates() {
        assert_eq!(BlePhy::Le1M.data_rate_kbps(), 1000);
        assert_eq!(BlePhy::Le2M.data_rate_kbps(), 2000);
        assert_eq!(BlePhy::LeCodedS2.data_rate_kbps(), 500);
        assert_eq!(BlePhy::LeCodedS8.data_rate_kbps(), 125);
    }

    #[test]
    fn test_phy_ranges() {
        assert_eq!(BlePhy::Le1M.typical_range_m(), 100);
        assert_eq!(BlePhy::Le2M.typical_range_m(), 50);
        assert_eq!(BlePhy::LeCodedS2.typical_range_m(), 200);
        assert_eq!(BlePhy::LeCodedS8.typical_range_m(), 400);
    }

    #[test]
    fn test_phy_is_coded() {
        assert!(!BlePhy::Le1M.is_coded());
        assert!(!BlePhy::Le2M.is_coded());
        assert!(BlePhy::LeCodedS2.is_coded());
        assert!(BlePhy::LeCodedS8.is_coded());
    }

    #[test]
    fn test_phy_requires_ble5() {
        assert!(!BlePhy::Le1M.requires_ble5());
        assert!(BlePhy::Le2M.requires_ble5());
        assert!(BlePhy::LeCodedS2.requires_ble5());
        assert!(BlePhy::LeCodedS8.requires_ble5());
    }

    #[test]
    fn test_phy_coding_scheme() {
        assert_eq!(BlePhy::Le1M.coding_scheme(), None);
        assert_eq!(BlePhy::Le2M.coding_scheme(), None);
        assert_eq!(BlePhy::LeCodedS2.coding_scheme(), Some(2));
        assert_eq!(BlePhy::LeCodedS8.coding_scheme(), Some(8));
    }

    #[test]
    fn test_phy_display() {
        assert_eq!(format!("{}", BlePhy::Le1M), "LE 1M");
        assert_eq!(format!("{}", BlePhy::LeCodedS8), "LE Coded S=8");
    }

    #[test]
    fn test_phy_transmit_time() {
        // 100 bytes at 1 Mbps = ~880 bits / 1_000_000 = ~880 µs
        let time_1m = BlePhy::Le1M.transmit_time_us(100);
        let time_2m = BlePhy::Le2M.transmit_time_us(100);

        // 2M should be faster
        assert!(time_2m < time_1m);
    }

    #[test]
    fn test_phy_capabilities_default() {
        let caps = PhyCapabilities::default();
        assert!(!caps.le_2m);
        assert!(!caps.le_coded);
        assert!(caps.supports(BlePhy::Le1M));
        assert!(!caps.supports(BlePhy::Le2M));
    }

    #[test]
    fn test_phy_capabilities_ble5() {
        let caps = PhyCapabilities::ble5_full();
        assert!(caps.supports(BlePhy::Le1M));
        assert!(caps.supports(BlePhy::Le2M));
        assert!(caps.supports(BlePhy::LeCodedS2));
        assert!(caps.supports(BlePhy::LeCodedS8));
    }

    #[test]
    fn test_phy_capabilities_best_for_range() {
        let caps = PhyCapabilities::ble5_full();
        assert_eq!(caps.best_for_range(), BlePhy::LeCodedS8);

        let caps_no_coded = PhyCapabilities::ble5_no_coded();
        assert_eq!(caps_no_coded.best_for_range(), BlePhy::Le1M);
    }

    #[test]
    fn test_phy_capabilities_best_for_throughput() {
        let caps = PhyCapabilities::ble5_full();
        assert_eq!(caps.best_for_throughput(), BlePhy::Le2M);

        let caps_basic = PhyCapabilities::le_1m_only();
        assert_eq!(caps_basic.best_for_throughput(), BlePhy::Le1M);
    }

    #[test]
    fn test_phy_preference_symmetric() {
        let pref = PhyPreference::symmetric(BlePhy::LeCodedS8);
        assert_eq!(pref.tx, BlePhy::LeCodedS8);
        assert_eq!(pref.rx, BlePhy::LeCodedS8);
        assert!(pref.is_symmetric());
    }

    #[test]
    fn test_phy_preference_asymmetric() {
        let pref = PhyPreference {
            tx: BlePhy::Le2M,
            rx: BlePhy::LeCodedS2,
        };
        assert!(!pref.is_symmetric());
    }

    #[test]
    fn test_relative_power() {
        assert!(BlePhy::Le2M.relative_power() < BlePhy::Le1M.relative_power());
        assert!(BlePhy::LeCodedS8.relative_power() > BlePhy::Le1M.relative_power());
    }
}
