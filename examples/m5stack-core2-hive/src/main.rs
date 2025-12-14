//! HIVE Protocol Demo for M5Stack Core2
//!
//! This firmware demonstrates HIVE-Lite mesh networking on ESP32 using BLE.
//! It implements a sensor node that:
//! - Advertises as a HIVE node via BLE beacons
//! - Exposes GATT characteristics for CRDT sync
//! - Displays peer status on the LCD
//! - Syncs state with other HIVE nodes
//!
//! ## Hardware
//!
//! - M5Stack Core2 (ESP32-D0WDQ6-V3)
//! - 320x240 ILI9342C LCD display
//! - Touch screen with 3 virtual buttons
//! - Built-in accelerometer (MPU6886)
//!
//! ## Node ID
//!
//! The Node ID is derived from the ESP32's unique MAC address (last 4 bytes).
//! This means each device automatically gets a unique ID without configuration.
//!
//! ## Building
//!
//! ```bash
//! # Install ESP toolchain
//! espup install
//! source ~/export-esp.sh
//!
//! # Build and flash
//! cargo build --release
//! espflash flash --monitor target/xtensa-esp32-espidf/release/m5stack-core2-hive
//! ```

use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{AnyOutputPin, PinDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::spi::{SpiDeviceDriver, SpiDriverConfig, SPI2};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use log::{info, warn};

use hive_btle::discovery::HiveBeacon;
use hive_btle::{HierarchyLevel, NodeId};

// M5Stack Core2 pin definitions
const LCD_CS_PIN: i32 = 5;
const LCD_DC_PIN: i32 = 15;
const LCD_RST_PIN: i32 = -1; // Not used on Core2, controlled by AXP192
const LCD_BL_PIN: i32 = -1; // Backlight controlled by AXP192
const LCD_MOSI_PIN: i32 = 23;
const LCD_CLK_PIN: i32 = 18;

/// Peer discovery state
#[derive(Debug, Clone, Default)]
struct PeerState {
    /// Discovered peer node IDs
    discovered: Vec<u32>,
    /// Connected peer node IDs
    connected: Vec<u32>,
    /// Last discovery update time
    last_update_ms: u64,
}

/// Display state for the UI
#[derive(Debug, Clone)]
struct DisplayState {
    /// Our node ID (last 4 bytes of MAC)
    our_id: u32,
    /// Peer state
    peers: PeerState,
    /// Status message
    status: &'static str,
    /// Uptime in seconds
    uptime_secs: u32,
}

impl DisplayState {
    fn new(our_id: u32) -> Self {
        Self {
            our_id,
            peers: PeerState::default(),
            status: "Initializing...",
            uptime_secs: 0,
        }
    }
}

/// Get unique Node ID from ESP32 MAC address
///
/// Uses the last 4 bytes of the base MAC address as a 32-bit Node ID.
/// Each ESP32 has a unique factory-burned MAC, so this gives unique IDs.
fn get_node_id_from_mac() -> u32 {
    let mut mac = [0u8; 6];

    // Get base MAC address from efuse
    unsafe {
        esp_idf_svc::sys::esp_efuse_mac_get_default(mac.as_mut_ptr());
    }

    info!(
        "ESP32 MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );

    // Use last 4 bytes as node ID (most unique part)
    let node_id = u32::from_be_bytes([mac[2], mac[3], mac[4], mac[5]]);

    info!("Derived Node ID: {:08X}", node_id);
    node_id
}

/// Simple LCD text output (placeholder for full graphics)
///
/// For initial testing, we use serial output to simulate display.
/// Full LCD driver will be added with embedded-graphics.
fn update_display(state: &DisplayState) {
    // Clear screen simulation
    info!("╔══════════════════════════════════════╗");
    info!("║     HIVE Protocol - M5Stack Core2    ║");
    info!("╠══════════════════════════════════════╣");
    info!("║  My ID: {:08X}                    ║", state.our_id);
    info!("╠══════════════════════════════════════╣");

    if state.peers.connected.is_empty() && state.peers.discovered.is_empty() {
        info!("║  Peers: (scanning...)                ║");
    } else {
        info!("║  Peers:                              ║");
        for peer_id in &state.peers.discovered {
            let status = if state.peers.connected.contains(peer_id) {
                "●" // Connected (would be green on LCD)
            } else {
                "○" // Discovered but not connected (yellow)
            };
            info!("║    {} {:08X}                      ║", status, peer_id);
        }
    }

    info!("╠══════════════════════════════════════╣");
    info!(
        "║  Status: {:<27} ║",
        state.status
    );
    info!(
        "║  Uptime: {:>6}s                      ║",
        state.uptime_secs
    );
    info!("╚══════════════════════════════════════╝");
}

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("===========================================");
    info!("  HIVE Protocol - M5Stack Core2 Demo");
    info!("===========================================");

    // Take peripherals
    let peripherals = Peripherals::take()?;
    let _sys_loop = EspSystemEventLoop::take()?;
    let _nvs = EspDefaultNvsPartition::take()?;

    // Get unique Node ID from MAC address
    let node_id_value = get_node_id_from_mac();
    let node_id = NodeId::new(node_id_value);

    // Initialize display state
    let mut display = DisplayState::new(node_id_value);
    display.status = "Starting BLE...";
    update_display(&display);

    // Create HIVE beacon for advertising
    let mut beacon = HiveBeacon::new(node_id);
    beacon.set_hierarchy_level(HierarchyLevel::Leaf);
    beacon.set_connection_capacity(0, 1); // Can accept 0-1 connections
    beacon.set_battery_level(100);

    info!("HIVE beacon configured:");
    info!("  - Node ID: {:08X}", node_id.as_u32());
    info!("  - Hierarchy: {:?}", HierarchyLevel::Leaf);
    info!("  - Beacon size: {} bytes", beacon.encode_compact().len());

    // TODO: Initialize NimBLE and start advertising
    // let adapter = Esp32Adapter::new(node_id, &format!("HIVE-{:08X}", node_id_value))?;
    // adapter.init(&BleConfig::hive_lite(node_id)).await?;
    // adapter.start().await?;

    display.status = "Advertising...";

    // Main loop
    let mut loop_count: u32 = 0;
    loop {
        display.uptime_secs = loop_count;

        // Update display every 5 seconds
        if loop_count % 5 == 0 {
            update_display(&display);
        }

        // TODO: Check for discovered peers from BLE scan
        // TODO: Update connected peers list
        // TODO: Handle button presses for manual actions

        // Simulate peer discovery for testing
        // In real implementation, this comes from BLE scan callbacks
        if loop_count == 10 {
            display.status = "Scanning...";
        }

        FreeRtos::delay_ms(1000);
        loop_count += 1;
    }
}
