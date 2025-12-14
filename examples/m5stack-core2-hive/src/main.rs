//! HIVE Protocol Demo for M5Stack Core2
//!
//! Simple CRDT counter sync demo:
//! - Touch Button A to increment your counter
//! - Counter syncs to peer via BLE
//! - Both devices show same total
//!
//! ## Hardware
//!
//! - M5Stack Core2 (ESP32-D0WDQ6-V3)
//! - FT6336U capacitive touch controller (I2C 0x38)
//! - 320x240 ILI9342C LCD display
//! - Touch buttons A/B/C at bottom of screen
//!
//! ## CRDT Design
//!
//! Uses a G-Counter (grow-only counter) where each node tracks its own count.
//! Merge operation takes max of each node's count.
//! Total is sum of all node counts.
//!
//! ## Building
//!
//! ```bash
//! source ~/export-esp.sh
//! cargo build --release
//! espflash flash --monitor target/xtensa-esp32-espidf/release/m5stack-core2-hive
//! ```

use std::collections::HashMap;

use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::prelude::*;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use log::{info, warn, error, debug};

use hive_btle::discovery::HiveBeacon;
use hive_btle::{HierarchyLevel, NodeId};

// FT6336U Touch controller I2C address
const FT6336U_ADDR: u8 = 0x38;

// FT6336U registers
const FT6336U_REG_STATUS: u8 = 0x02;    // Number of touch points
const FT6336U_REG_TOUCH1_XH: u8 = 0x03; // Touch 1 X high byte + event
const FT6336U_REG_TOUCH1_XL: u8 = 0x04; // Touch 1 X low byte
const FT6336U_REG_TOUCH1_YH: u8 = 0x05; // Touch 1 Y high byte
const FT6336U_REG_TOUCH1_YL: u8 = 0x06; // Touch 1 Y low byte

// Touch button regions (M5Stack Core2 button bar at bottom)
// The touch panel extends below the visible LCD area
const BUTTON_Y_MIN: u16 = 240;  // Below visible screen
const BUTTON_Y_MAX: u16 = 320;
const BUTTON_A_X_MIN: u16 = 0;
const BUTTON_A_X_MAX: u16 = 109;
const BUTTON_B_X_MIN: u16 = 110;
const BUTTON_B_X_MAX: u16 = 219;
const BUTTON_C_X_MIN: u16 = 220;
const BUTTON_C_X_MAX: u16 = 320;

/// Touch event
#[derive(Debug, Clone, Copy, PartialEq)]
enum TouchButton {
    None,
    A,
    B,
    C,
    Screen(u16, u16), // Touch on main screen area
}

/// G-Counter CRDT
///
/// Each node has its own counter. Merge takes max per node.
/// Total is sum of all counters.
#[derive(Debug, Clone)]
struct GCounter {
    counts: HashMap<u32, u32>,
    our_id: u32,
}

impl GCounter {
    fn new(our_id: u32) -> Self {
        let mut counts = HashMap::new();
        counts.insert(our_id, 0);
        Self { counts, our_id }
    }

    fn increment(&mut self) {
        let count = self.counts.entry(self.our_id).or_insert(0);
        *count += 1;
    }

    fn our_count(&self) -> u32 {
        *self.counts.get(&self.our_id).unwrap_or(&0)
    }

    fn peer_count(&self) -> u32 {
        self.counts
            .iter()
            .filter(|(&id, _)| id != self.our_id)
            .map(|(_, &count)| count)
            .sum()
    }

    fn total(&self) -> u32 {
        self.counts.values().sum()
    }

    fn merge(&mut self, other: &GCounter) {
        for (&node_id, &count) in &other.counts {
            let our_count = self.counts.entry(node_id).or_insert(0);
            *our_count = (*our_count).max(count);
        }
    }

    /// Encode for BLE: [num_entries: u8] [node_id: u32, count: u32]...
    fn encode(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(1 + self.counts.len() * 8);
        data.push(self.counts.len() as u8);
        for (&node_id, &count) in &self.counts {
            data.extend_from_slice(&node_id.to_le_bytes());
            data.extend_from_slice(&count.to_le_bytes());
        }
        data
    }

    fn decode(data: &[u8], our_id: u32) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        let num_entries = data[0] as usize;
        if data.len() < 1 + num_entries * 8 {
            return None;
        }
        let mut counts = HashMap::new();
        for i in 0..num_entries {
            let offset = 1 + i * 8;
            let node_id = u32::from_le_bytes([
                data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
            ]);
            let count = u32::from_le_bytes([
                data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7],
            ]);
            counts.insert(node_id, count);
        }
        Some(Self { counts, our_id })
    }
}

/// Display state
struct DisplayState {
    our_id: u32,
    peer_id: Option<u32>,
    counter: GCounter,
    status: &'static str,
    ble_connected: bool,
}

impl DisplayState {
    fn new(our_id: u32) -> Self {
        Self {
            our_id,
            peer_id: None,
            counter: GCounter::new(our_id),
            status: "Starting...",
            ble_connected: false,
        }
    }
}

/// Get unique Node ID from ESP32 MAC address
fn get_node_id_from_mac() -> u32 {
    let mut mac = [0u8; 6];
    unsafe {
        esp_idf_svc::sys::esp_efuse_mac_get_default(mac.as_mut_ptr());
    }
    info!(
        "MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    u32::from_be_bytes([mac[2], mac[3], mac[4], mac[5]])
}

/// Read touch input from FT6336U
fn read_touch(i2c: &mut I2cDriver) -> TouchButton {
    let mut buf = [0u8; 5];

    // Read touch status and first touch point
    if i2c.write_read(FT6336U_ADDR, &[FT6336U_REG_STATUS], &mut buf).is_err() {
        return TouchButton::None;
    }

    let num_touches = buf[0] & 0x0F;
    if num_touches == 0 {
        return TouchButton::None;
    }

    // Read touch coordinates
    let mut coord_buf = [0u8; 4];
    if i2c.write_read(FT6336U_ADDR, &[FT6336U_REG_TOUCH1_XH], &mut coord_buf).is_err() {
        return TouchButton::None;
    }

    // Extract X and Y (12-bit values)
    let x = (((coord_buf[0] & 0x0F) as u16) << 8) | (coord_buf[1] as u16);
    let y = (((coord_buf[2] & 0x0F) as u16) << 8) | (coord_buf[3] as u16);

    debug!("Touch at ({}, {})", x, y);

    // Determine which button (if any)
    if y >= BUTTON_Y_MIN && y <= BUTTON_Y_MAX {
        if x >= BUTTON_A_X_MIN && x <= BUTTON_A_X_MAX {
            return TouchButton::A;
        } else if x >= BUTTON_B_X_MIN && x <= BUTTON_B_X_MAX {
            return TouchButton::B;
        } else if x >= BUTTON_C_X_MIN && x <= BUTTON_C_X_MAX {
            return TouchButton::C;
        }
    }

    // Touch on main screen area
    if y < BUTTON_Y_MIN {
        return TouchButton::Screen(x, y);
    }

    TouchButton::None
}

/// Update display (serial output for now, LCD driver to come)
fn update_display(state: &DisplayState) {
    info!("┌────────────────────────────────┐");
    info!("│  HIVE Counter Demo             │");
    info!("├────────────────────────────────┤");
    info!("│  My ID: {:08X}              │", state.our_id);
    if let Some(peer) = state.peer_id {
        info!("│  Peer:  {:08X} {}           │", peer,
            if state.ble_connected { "●" } else { "○" });
    } else {
        info!("│  Peer:  (scanning...)          │");
    }
    info!("├────────────────────────────────┤");
    info!("│  My taps:   {:>4}               │", state.counter.our_count());
    info!("│  Peer taps: {:>4}               │", state.counter.peer_count());
    info!("│  ──────────────                │");
    info!("│  TOTAL:     {:>4}               │", state.counter.total());
    info!("├────────────────────────────────┤");
    info!("│  TAP SCREEN = +1               │");
    info!("│  Status: {:<20} │", state.status);
    info!("└────────────────────────────────┘");
}

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("=========================================");
    info!("  HIVE Counter Demo - M5Stack Core2");
    info!("=========================================");

    // Take peripherals
    let peripherals = Peripherals::take()?;
    let _sys_loop = EspSystemEventLoop::take()?;
    let _nvs = EspDefaultNvsPartition::take()?;

    // Get unique Node ID from MAC
    let node_id = get_node_id_from_mac();
    info!("Node ID: {:08X}", node_id);

    // Initialize I2C for touch controller
    let i2c_config = I2cConfig::new().baudrate(400.kHz().into());
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio21, // SDA
        peripherals.pins.gpio22, // SCL
        &i2c_config,
    )?;

    info!("I2C initialized for touch controller");

    // Initialize display state
    let mut state = DisplayState::new(node_id);
    state.status = "Tap screen!";

    // Create HIVE beacon
    let node_id_obj = NodeId::new(node_id);
    let mut beacon = HiveBeacon::new(node_id_obj);
    beacon.set_hierarchy_level(HierarchyLevel::Leaf);
    beacon.set_battery_level(100);

    info!("Beacon configured, {} bytes", beacon.encode_compact().len());

    // Initial display
    update_display(&state);

    // TODO: Initialize BLE advertising with counter state in service data
    // TODO: Start BLE scan for peer HIVE nodes
    // TODO: On peer discovery, connect and sync counter state

    // Track button state for edge detection
    let mut last_button = TouchButton::None;
    let mut loop_count: u32 = 0;
    let mut last_display_update = 0u32;

    loop {
        // Read touch input
        let button = read_touch(&mut i2c);

        // Detect touch (rising edge - only trigger on new touch)
        let is_touching = button != TouchButton::None;
        let was_touching = last_button != TouchButton::None;

        if is_touching && !was_touching {
            // Any touch increments counter
            info!(">>> TOUCH! Incrementing counter...");
            state.counter.increment();
            state.status = "Tapped! +1";
            update_display(&state);
            // TODO: Trigger BLE sync to peer
        }

        last_button = button;

        // Update display periodically (every ~5 seconds)
        if loop_count - last_display_update >= 50 {
            update_display(&state);
            last_display_update = loop_count;
        }

        // TODO: Check for incoming BLE sync data
        // - If peer counter received, merge with ours
        // - Update display

        FreeRtos::delay_ms(100); // 10Hz loop for responsive touch
        loop_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcounter_increment() {
        let mut counter = GCounter::new(0x1234);
        assert_eq!(counter.our_count(), 0);
        counter.increment();
        assert_eq!(counter.our_count(), 1);
        counter.increment();
        assert_eq!(counter.our_count(), 2);
    }

    #[test]
    fn test_gcounter_merge() {
        let mut counter_a = GCounter::new(0x1111);
        let mut counter_b = GCounter::new(0x2222);

        counter_a.increment();
        counter_a.increment();
        counter_b.increment();

        assert_eq!(counter_a.total(), 2);
        assert_eq!(counter_b.total(), 1);

        counter_a.merge(&counter_b);
        assert_eq!(counter_a.total(), 3);
        assert_eq!(counter_a.our_count(), 2);
        assert_eq!(counter_a.peer_count(), 1);
    }

    #[test]
    fn test_gcounter_encode_decode() {
        let mut counter = GCounter::new(0x12345678);
        counter.increment();
        counter.increment();

        let encoded = counter.encode();
        let decoded = GCounter::decode(&encoded, 0x12345678).unwrap();

        assert_eq!(decoded.our_count(), 2);
    }
}
