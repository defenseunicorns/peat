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

use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::prelude::*;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use log::{info, debug};

use hive_btle::discovery::HiveBeacon;
use hive_btle::sync::GCounter;  // Use HIVE-Lite's CRDT!
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

// Using hive_btle::sync::GCounter - the real HIVE-Lite CRDT!

/// Display state
struct DisplayState {
    our_node_id: NodeId,
    peer_id: Option<NodeId>,
    counter: GCounter,
    status: &'static str,
    ble_connected: bool,
}

impl DisplayState {
    fn new(node_id: NodeId) -> Self {
        Self {
            our_node_id: node_id,
            peer_id: None,
            counter: GCounter::new(),  // HIVE-Lite GCounter
            status: "Starting...",
            ble_connected: false,
        }
    }

    /// Get our tap count
    fn our_count(&self) -> u64 {
        self.counter.node_count(&self.our_node_id)
    }

    /// Get peer tap count (total - ours)
    fn peer_count(&self) -> u64 {
        self.counter.value() - self.our_count()
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
    info!("│  HIVE-Lite Counter Demo        │");
    info!("├────────────────────────────────┤");
    info!("│  My ID: {:08X}              │", state.our_node_id.as_u32());
    if let Some(ref peer) = state.peer_id {
        info!("│  Peer:  {:08X} {}           │", peer.as_u32(),
            if state.ble_connected { "●" } else { "○" });
    } else {
        info!("│  Peer:  (scanning...)          │");
    }
    info!("├────────────────────────────────┤");
    info!("│  My taps:   {:>4}               │", state.our_count());
    info!("│  Peer taps: {:>4}               │", state.peer_count());
    info!("│  ──────────────                │");
    info!("│  TOTAL:     {:>4}               │", state.counter.value());
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
    let node_id_value = get_node_id_from_mac();
    let node_id = NodeId::new(node_id_value);
    info!("Node ID: {:08X}", node_id.as_u32());

    // Initialize I2C for touch controller
    let i2c_config = I2cConfig::new().baudrate(400.kHz().into());
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio21, // SDA
        peripherals.pins.gpio22, // SCL
        &i2c_config,
    )?;

    info!("I2C initialized for touch controller");

    // Initialize display state with HIVE-Lite GCounter
    let mut state = DisplayState::new(node_id);
    state.status = "Tap screen!";

    // Create HIVE beacon
    let mut beacon = HiveBeacon::new(node_id);
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
            // Any touch increments counter using HIVE-Lite GCounter
            info!(">>> TOUCH! Incrementing counter...");
            state.counter.increment(&state.our_node_id, 1);
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

// Note: GCounter tests are in hive_btle::sync::crdt - no need to duplicate here
