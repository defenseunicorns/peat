//! HIVE-Lite BLE Alert/Ack Demo for M5Stack Core2
//!
//! Tactical alert system over BLE mesh:
//! - Double tap: Send EMERGENCY alert to all peers (they buzz)
//! - Single tap: Acknowledge (silence local buzz, send ACK)
//! - Long press (3s): Reset counter
//!
//! Uses centralized HiveMesh from hive-btle for peer management and document sync.
//!
//! ## Building
//!
//! ```bash
//! source ~/export-esp.sh
//! cargo build --release
//! espflash flash --monitor target/xtensa-esp32-espidf/release/m5stack-core2-hive
//! ```

mod nimble;

use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::PinDriver;
use esp_idf_hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi::{SpiConfig, SpiDeviceDriver, SpiDriver, SpiDriverConfig};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use log::{error, info, warn};

use display_interface_spi::SPIInterface;
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use mipidsi::models::ILI9342CRgb565;
use mipidsi::options::Orientation;
use mipidsi::Builder;

use hive_btle::hive_mesh::{HiveMesh, HiveMeshConfig};
use hive_btle::sync::PeripheralType;
use hive_btle::NodeId;

// NVS storage
const NVS_NAMESPACE: &str = "hive";
const NVS_KEY_COUNTER: &str = "counter";

// FT6336U Touch controller on M5Stack Core2
const FT6336U_ADDR: u8 = 0x38;
const FT6336U_REG_STATUS: u8 = 0x02;

/// Get current timestamp in milliseconds
fn now_ms() -> u64 {
    unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 / 1000 }
}

/// NVS persistence for HiveMesh documents
struct DocumentStore {
    nvs: EspNvs<NvsDefault>,
}

impl DocumentStore {
    fn new(nvs_partition: EspDefaultNvsPartition) -> anyhow::Result<Self> {
        let nvs = EspNvs::new(nvs_partition, NVS_NAMESPACE, true)?;
        Ok(Self { nvs })
    }

    /// Save HiveMesh document to NVS
    fn save(&mut self, mesh: &HiveMesh) -> anyhow::Result<()> {
        let data = mesh.build_document();
        self.nvs.set_raw(NVS_KEY_COUNTER, &data)?;
        info!("NVS: Saved {} bytes", data.len());
        Ok(())
    }

    /// Load document bytes from NVS (for initial merge into HiveMesh)
    fn load_raw(&self) -> Option<Vec<u8>> {
        let mut buf = [0u8; 256];
        match self.nvs.get_raw(NVS_KEY_COUNTER, &mut buf) {
            Ok(Some(data)) => {
                info!("NVS: Loaded {} raw bytes", data.len());
                Some(data.to_vec())
            }
            Ok(None) => {
                info!("NVS: No saved document");
                None
            }
            Err(e) => {
                warn!("NVS load error: {:?}", e);
                None
            }
        }
    }
}

/// Button press state
#[derive(PartialEq, Clone, Copy, Debug)]
enum Button {
    None,
    BtnA,  // Left button - ACK
    BtnB,  // Middle button - (unused)
    BtnC,  // Right button - EMERGENCY
}

/// Read touch and map to buttons
/// M5Stack Core2 touch buttons are at Y > 240:
/// - Button A: X = 0-106
/// - Button B: X = 107-213
/// - Button C: X = 214-320
fn read_button(i2c: &mut I2cDriver) -> Button {
    let mut buf = [0u8; 5];
    // Read touch status and first touch point (registers 0x02-0x06)
    if i2c
        .write_read(FT6336U_ADDR, &[FT6336U_REG_STATUS], &mut buf, 100)
        .is_ok()
    {
        let num_points = buf[0] & 0x0F;
        if num_points > 0 {
            // Touch point 1: X high bits in [1], X low in [2], Y high in [3], Y low in [4]
            let x = ((buf[1] as u16 & 0x0F) << 8) | buf[2] as u16;
            let y = ((buf[3] as u16 & 0x0F) << 8) | buf[4] as u16;

            // Debug: log all touches
            info!("Touch: x={}, y={}", x, y);

            // Only count touches in the button area (y > 240)
            if y > 240 {
                if x < 107 {
                    info!(">>> Button A detected");
                    return Button::BtnA;
                } else if x < 214 {
                    info!(">>> Button B detected");
                    return Button::BtnB;
                } else {
                    info!(">>> Button C detected");
                    return Button::BtnC;
                }
            }
        }
    }
    Button::None
}

fn get_node_id_from_mac() -> NodeId {
    let mut mac = [0u8; 6];
    unsafe {
        esp_idf_svc::sys::esp_efuse_mac_get_default(mac.as_mut_ptr());
    }
    info!(
        "MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    // Use last 4 bytes of MAC as node ID
    NodeId::new(u32::from_be_bytes([mac[2], mac[3], mac[4], mac[5]]))
}

fn print_status(mesh: &HiveMesh, connected: bool, status: &str) {
    let conn_sym = if connected { "●" } else { "○" };
    info!("========================================");
    info!("  HIVE-Lite BLE Sync Demo (HiveMesh)");
    info!("----------------------------------------");
    info!(
        "  Node: {:08X}  v{}  BLE:{}",
        mesh.node_id().as_u32(),
        mesh.version(),
        conn_sym
    );
    info!("----------------------------------------");
    info!("  Taps: {}", mesh.total_count());
    info!("----------------------------------------");
    info!("  {}", status);
    info!("========================================");
}

/// Power management IC address (same for AXP192 and AXP2101)
const AXP_ADDR: u8 = 0x34;

/// Hardware version detected at runtime
#[derive(Clone, Copy, PartialEq, Debug)]
enum HardwareVersion {
    Core2V10,  // AXP192
    Core2V11,  // AXP2101
}

/// Global hardware version (set during init)
static mut HARDWARE_VERSION: HardwareVersion = HardwareVersion::Core2V10;

/// Detect hardware version by checking AXP chip ID
fn detect_hardware_version(i2c: &mut I2cDriver) -> HardwareVersion {
    let mut buf = [0u8; 1];

    // AXP2101 has chip ID at register 0x03
    // AXP192 has different register layout

    // Try reading AXP2101 chip ID register (0x03)
    if i2c.write_read(AXP_ADDR, &[0x03], &mut buf, 100).is_ok() {
        // AXP2101 chip ID is 0x4A (or similar)
        if buf[0] == 0x4A || buf[0] == 0x4B {
            info!("Detected AXP2101 (chip ID 0x{:02X}) - Core2 v1.1", buf[0]);
            return HardwareVersion::Core2V11;
        }
    }

    // Try AXP192 - read power status register (0x00)
    // AXP192 should respond to this
    if i2c.write_read(AXP_ADDR, &[0x00], &mut buf, 100).is_ok() {
        info!("Detected AXP192 (status 0x{:02X}) - Core2 v1.0", buf[0]);
        return HardwareVersion::Core2V10;
    }

    warn!("Could not detect AXP chip, assuming Core2 v1.0");
    HardwareVersion::Core2V10
}

fn axp_init(i2c: &mut I2cDriver) -> anyhow::Result<HardwareVersion> {
    let mut buf = [0u8; 1];

    // Detect hardware version first
    let version = detect_hardware_version(i2c);
    unsafe { HARDWARE_VERSION = version; }

    match version {
        HardwareVersion::Core2V10 => {
            info!("Initializing AXP192 for Core2 v1.0");
            // Read current ADC config
            if i2c.write_read(AXP_ADDR, &[0x82], &mut buf, 100).is_ok() {
                info!("AXP192: ADC config=0x{:02X} (need bit7 set for battery)", buf[0]);

                // Enable battery voltage ADC (bit 7) if not already set
                if (buf[0] & 0x80) == 0 {
                    let new_val = buf[0] | 0x80;
                    info!("AXP192: Enabling battery ADC: 0x{:02X} -> 0x{:02X}", buf[0], new_val);
                    let _ = i2c.write(AXP_ADDR, &[0x82, new_val], 1000);
                    FreeRtos::delay_ms(100);
                }
            }

            if i2c.write_read(AXP_ADDR, &[0x00], &mut buf, 100).is_ok() {
                info!("AXP192: Power status=0x{:02X}", buf[0]);
            }
        }
        HardwareVersion::Core2V11 => {
            info!("Initializing AXP2101 for Core2 v1.1");

            // AXP2101 register map (from Tasmota/M5Stack drivers):
            // 0x90 = LDOS ON/OFF control
            // 0x92 = ALDO1 voltage (not used)
            // 0x93 = ALDO2 voltage (LCD power, 3.3V = 0x1C)
            // 0x94 = ALDO3 voltage (Speaker)
            // 0x95 = ALDO4 voltage
            // 0x96 = BLDO1 voltage (Backlight)
            // 0x97 = BLDO2 voltage
            // 0x99 = DLDO1 voltage (Vibration motor, 0.5V = 0x00, higher for stronger)

            // Enable ALDO2 for LCD (3.3V)
            // ALDO2 voltage: (val * 0.1 + 0.5)V, so 0x1C = 3.3V
            let _ = i2c.write(AXP_ADDR, &[0x93, 0x1C], 100);
            info!("AXP2101: ALDO2 set to 3.3V for LCD");

            // Enable BLDO1 for backlight (typically 2.8-3.0V)
            // BLDO voltage: (val * 0.1 + 0.5)V, so 0x17 = 2.8V
            let _ = i2c.write(AXP_ADDR, &[0x96, 0x17], 100);
            info!("AXP2101: BLDO1 set to 2.8V for backlight");

            // Set DLDO1 voltage for vibration (0.5V base + 0.1V * val)
            // val=0x0A gives 1.5V - good for vibration motor
            let _ = i2c.write(AXP_ADDR, &[0x99, 0x0A], 100);
            info!("AXP2101: DLDO1 set to 1.5V for vibration motor");

            // Read LDO control register
            if i2c.write_read(AXP_ADDR, &[0x90], &mut buf, 100).is_ok() {
                info!("AXP2101: LDO control (0x90) = 0x{:02X}", buf[0]);
                // Enable ALDO2 (bit 1), BLDO1 (bit 4)
                // Bit 0: ALDO1, Bit 1: ALDO2, Bit 2: ALDO3, Bit 3: ALDO4
                // Bit 4: BLDO1, Bit 5: BLDO2, Bit 6: DLDO1, Bit 7: DLDO2
                let new_val = buf[0] | 0x12; // Enable ALDO2 + BLDO1
                let _ = i2c.write(AXP_ADDR, &[0x90, new_val], 100);
                info!("AXP2101: LDO control enabled: 0x{:02X}", new_val);
            }

            FreeRtos::delay_ms(100); // Let power stabilize
        }
    }

    Ok(version)
}

/// Read battery voltage (returns millivolts)
/// - Core2 v1.0 (AXP192): Registers 0x78-0x79
/// - Core2 v1.1 (AXP2101): Registers 0x34-0x35
fn axp_read_battery_voltage(i2c: &mut I2cDriver) -> Option<u16> {
    let mut buf = [0u8; 2];
    let version = unsafe { HARDWARE_VERSION };

    match version {
        HardwareVersion::Core2V10 => {
            // AXP192: Register 0x78-0x79: Battery voltage ADC (12-bit, 1.1mV/step)
            if i2c.write_read(AXP_ADDR, &[0x78], &mut buf, 100).is_ok() {
                let raw = ((buf[0] as u16) << 4) | ((buf[1] as u16) >> 4);
                let mv = (raw as u32 * 1100 / 1000) as u16;
                info!("Battery ADC (AXP192): raw=0x{:03X} => {}mV", raw, mv);
                Some(mv)
            } else {
                None
            }
        }
        HardwareVersion::Core2V11 => {
            // AXP2101: Battery voltage at 0x34-0x35 (14-bit, 1mV/step)
            if i2c.write_read(AXP_ADDR, &[0x34], &mut buf, 100).is_ok() {
                let raw = ((buf[0] as u16) << 8) | (buf[1] as u16);
                let mv = raw & 0x3FFF; // 14-bit value, 1mV per step
                if mv > 0 && mv < 5000 {
                    info!("Battery ADC (AXP2101): raw=0x{:04X} => {}mV", raw, mv);
                    Some(mv)
                } else {
                    // Try alternate register (some docs show 0x38-0x39)
                    if i2c.write_read(AXP_ADDR, &[0x38], &mut buf, 100).is_ok() {
                        let raw = ((buf[0] as u16) << 8) | (buf[1] as u16);
                        let mv = raw & 0x3FFF;
                        if mv > 0 && mv < 5000 {
                            info!("Battery ADC (AXP2101 alt): raw=0x{:04X} => {}mV", raw, mv);
                            return Some(mv);
                        }
                    }
                    // Return a default for v1.1 if ADC not working
                    info!("Battery ADC (AXP2101): unable to read, assuming 4000mV");
                    Some(4000)
                }
            } else {
                Some(4000) // Default for v1.1
            }
        }
    }
}

/// Estimate battery percentage from voltage (rough approximation)
fn battery_percent_from_voltage(mv: u16) -> u8 {
    // M5Stack Core2 battery: 3.7V nominal, ~4.2V full, ~3.0V empty
    if mv >= 4150 {
        100
    } else if mv <= 3000 {
        0
    } else {
        // Linear interpolation between 3.0V and 4.15V
        ((mv - 3000) as u32 * 100 / 1150) as u8
    }
}

/// Check if running on battery (no USB power)
fn axp_on_battery(i2c: &mut I2cDriver) -> bool {
    let mut buf = [0u8; 1];
    let version = unsafe { HARDWARE_VERSION };

    match version {
        HardwareVersion::Core2V10 => {
            // AXP192: Register 0x00, Bit 7: ACIN, Bit 5: VBUS
            if i2c.write_read(AXP_ADDR, &[0x00], &mut buf, 100).is_ok() {
                let acin = (buf[0] & 0x80) != 0;
                let vbus = (buf[0] & 0x20) != 0;
                !acin && !vbus
            } else {
                false
            }
        }
        HardwareVersion::Core2V11 => {
            // AXP2101: Register 0x00, check power source bits
            if i2c.write_read(AXP_ADDR, &[0x00], &mut buf, 100).is_ok() {
                // Bit 5: VBUS good
                let vbus = (buf[0] & 0x20) != 0;
                !vbus
            } else {
                false
            }
        }
    }
}

/// Enable/disable vibration motor
/// - Core2 v1.0 (AXP192): LDO3 (register 0x12, bit 3)
/// - Core2 v1.1 (AXP2101): DLDO1 (register 0x90, bit 6)
fn axp_set_vibration(i2c: &mut I2cDriver, enable: bool) {
    let mut buf = [0u8; 1];
    let version = unsafe { HARDWARE_VERSION };

    match version {
        HardwareVersion::Core2V10 => {
            // AXP192: Register 0x12, Bit 3 = LDO3 enable
            if i2c.write_read(AXP_ADDR, &[0x12], &mut buf, 100).is_ok() {
                let new_val = if enable {
                    buf[0] | 0x08  // Set bit 3
                } else {
                    buf[0] & !0x08  // Clear bit 3
                };
                if new_val != buf[0] {
                    let _ = i2c.write(AXP_ADDR, &[0x12, new_val], 100);
                    info!("Vibration (AXP192): {} (0x{:02X} -> 0x{:02X})",
                          if enable { "ON" } else { "OFF" }, buf[0], new_val);
                }
            }
        }
        HardwareVersion::Core2V11 => {
            // AXP2101: Register 0x90, Bit 6 = DLDO1 enable
            if i2c.write_read(AXP_ADDR, &[0x90], &mut buf, 100).is_ok() {
                let new_val = if enable {
                    buf[0] | 0x40  // Set bit 6 (DLDO1)
                } else {
                    buf[0] & !0x40  // Clear bit 6
                };
                if new_val != buf[0] {
                    let _ = i2c.write(AXP_ADDR, &[0x90, new_val], 100);
                    info!("Vibration (AXP2101): {} (0x{:02X} -> 0x{:02X})",
                          if enable { "ON" } else { "OFF" }, buf[0], new_val);
                }
            }
        }
    }
}

/// Build number for tracking firmware versions
const BUILD_NUM: u32 = 33;

/// Mesh ID for this device
const MESH_ID: &str = "DEMO";

/// Display state for tracking changes (to avoid flickering redraws)
#[derive(Default, Clone, PartialEq)]
struct DisplayState {
    num_connections: usize,
    battery_pct: u8,
    alert_active: bool,
    peer_count: usize,
    peer_ids: Vec<u32>,           // Node IDs of all known peers
    connected_peers: Vec<u32>,    // Node IDs of currently connected peers
    acked_peers: Vec<u32>,        // Node IDs that have ACK'd the current emergency
}

/// Draw initial static UI elements (call once at startup)
fn draw_static_ui<D>(display: &mut D, _node_id: u32)
where
    D: DrawTarget<Color = Rgb565>,
{
    let _ = display.clear(Rgb565::BLACK);

    let white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let gray = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_GRAY);
    let cyan = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);

    // Top bar: battery | HIVE:MESH | build
    let title = format!("HIVE:{}", MESH_ID);
    let _ = Text::new(&title, Point::new(110, 25), cyan).draw(display);
    let build_str = format!("b{}", BUILD_NUM);
    let _ = Text::new(&build_str, Point::new(250, 25), gray).draw(display);

    // Separator below top bar
    let _ = Rectangle::new(Point::new(0, 35), Size::new(320, 1))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_DARK_GRAY))
        .draw(display);

    // Separator above buttons
    let _ = Rectangle::new(Point::new(0, 205), Size::new(320, 1))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_DARK_GRAY))
        .draw(display);

    // Button labels drawn by update_button_labels based on alert state
}

/// Update button labels based on alert state
fn update_button_labels<D>(display: &mut D, alert_active: bool)
where
    D: DrawTarget<Color = Rgb565>,
{
    // Clear button area
    let _ = Rectangle::new(Point::new(0, 210), Size::new(320, 30))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display);

    let gray = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_DARK_GRAY);
    let green = MonoTextStyle::new(&FONT_10X20, Rgb565::GREEN);
    let cyan = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);
    let red = MonoTextStyle::new(&FONT_10X20, Rgb565::RED);

    // ACK is green only when alert active, otherwise grey
    if alert_active {
        let _ = Text::new("ACK", Point::new(35, 230), green).draw(display);
    } else {
        let _ = Text::new("ACK", Point::new(35, 230), gray).draw(display);
    }
    let _ = Text::new("RST", Point::new(140, 230), cyan).draw(display);
    let _ = Text::new("EMERG", Point::new(230, 230), red).draw(display);
}


/// Update only changed parts of the display (minimizes flicker)
/// Returns the new display state for comparison on next update
fn update_display<D>(
    display: &mut D,
    mesh: &HiveMesh,
    alert_active: bool,
    battery_pct: Option<u8>,
    _status: &'static str,
    prev: &DisplayState,
    acked_peers: &[u32],
) -> DisplayState
where
    D: DrawTarget<Color = Rgb565>,
{
    // Get peer IDs and connection status from nimble (not mesh)
    let peers = mesh.get_peers();
    let peer_ids: Vec<u32> = peers.iter().map(|p| p.node_id.as_u32()).collect();
    // Use nimble's connection tracking for accurate is_connected status
    let connected_peers: Vec<u32> = nimble::get_connected_node_ids();

    // Build current state
    let current = DisplayState {
        num_connections: nimble::connection_count(),
        battery_pct: battery_pct.unwrap_or(0),
        alert_active,
        peer_count: peers.len(),
        peer_ids: peer_ids.clone(),
        connected_peers: connected_peers.clone(),
        acked_peers: acked_peers.to_vec(),
    };

    // Skip if nothing changed
    if current == *prev {
        return current;
    }

    // Battery indicator (top left) - only update if changed
    if current.battery_pct != prev.battery_pct {
        let _ = Rectangle::new(Point::new(5, 5), Size::new(50, 28))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
            .draw(display);
        if current.battery_pct > 0 {
            let batt_color = if current.battery_pct > 20 { Rgb565::GREEN } else { Rgb565::RED };
            let batt_style = MonoTextStyle::new(&FONT_10X20, batt_color);
            let batt_str = format!("{}%", current.battery_pct);
            let _ = Text::new(&batt_str, Point::new(10, 25), batt_style).draw(display);
        }
    }

    // Connection indicator (top right, next to build#) - small dot
    if current.num_connections != prev.num_connections {
        let conn_color = if current.num_connections > 0 { Rgb565::GREEN } else { Rgb565::CSS_DARK_GRAY };
        let _ = Circle::new(Point::new(300, 12), 12)
            .into_styled(PrimitiveStyle::with_fill(conn_color))
            .draw(display);
    }

    // Main content area - update if alert state, peers, connections, or acks changed
    let content_changed = current.alert_active != prev.alert_active
        || current.peer_ids != prev.peer_ids
        || current.connected_peers != prev.connected_peers
        || current.acked_peers != prev.acked_peers;

    if content_changed {
        // Clear main content area
        let _ = Rectangle::new(Point::new(0, 40), Size::new(320, 160))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
            .draw(display);

        if current.alert_active {
            // ALERT MODE - big red box with EMERGENCY and ACK status
            let _ = Rectangle::new(Point::new(20, 50), Size::new(280, 150))
                .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
                .draw(display);
            let white_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
            let green_style = MonoTextStyle::new(&FONT_10X20, Rgb565::GREEN);
            let _ = Text::new("EMERGENCY", Point::new(90, 80), white_style).draw(display);

            // Show ACK and connection status for each peer
            let gray_style = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_GRAY);
            if !peer_ids.is_empty() {
                let mut y = 110;
                for id in peer_ids.iter().take(3) {
                    let is_connected = current.connected_peers.contains(id);
                    let acked = current.acked_peers.contains(id);
                    let (status, style) = if !is_connected {
                        (format!("{:08X} [--]", id), gray_style)  // Disconnected
                    } else if acked {
                        (format!("{:08X} [ACK]", id), green_style)  // Connected + ACK'd
                    } else {
                        (format!("{:08X} ...", id), white_style)  // Connected, waiting
                    };
                    let _ = Text::new(&status, Point::new(70, y), style).draw(display);
                    y += 25;
                }
            } else {
                let _ = Text::new("No peers known", Point::new(80, 120), white_style).draw(display);
            }

            let _ = Text::new("Tap ACK to clear", Point::new(70, 185), white_style).draw(display);
        } else {
            // READY MODE with peer info
            let green = MonoTextStyle::new(&FONT_10X20, Rgb565::GREEN);
            let white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
            let gray = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_GRAY);

            // Show local node ID above READY
            let node_id_str = format!("{:08X}", mesh.node_id().as_u32());
            let _ = Text::new(&node_id_str, Point::new(110, 55), white).draw(display);
            let _ = Text::new("READY", Point::new(125, 80), green).draw(display);

            // Show peer details
            if !peer_ids.is_empty() {
                // Show connected/total count
                let connected_count = current.connected_peers.len();
                let total_count = peer_ids.len();
                let count_str = format!("{}/{} peers:", connected_count, total_count);
                let _ = Text::new(&count_str, Point::new(100, 120), white).draw(display);

                // Show up to 3 peer IDs with connection status
                let mut y = 145;
                for id in peer_ids.iter().take(3) {
                    let is_connected = current.connected_peers.contains(id);
                    let id_str = format!("{:08X}", id);
                    let style = if is_connected { green } else { gray };
                    // Center single peer, spread multiple
                    let x = if peer_ids.len() == 1 { 115 } else { 60 };
                    let _ = Text::new(&id_str, Point::new(x, y), style).draw(display);
                    // Show status indicator
                    let status = if is_connected { " [OK]" } else { " [--]" };
                    let _ = Text::new(status, Point::new(x + 85, y), style).draw(display);
                    y += 25;
                }
                if peer_ids.len() > 3 {
                    let _ = Text::new(&format!("+{} more", peer_ids.len() - 3), Point::new(100, y), gray).draw(display);
                }
            } else {
                let _ = Text::new("Scanning...", Point::new(100, 130), gray).draw(display);
            }
        }
    }

    // Update button labels when alert state changes
    if current.alert_active != prev.alert_active {
        update_button_labels(display, current.alert_active);
    }

    current
}

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("");
    info!("=========================================");
    info!("  HIVE-Lite BLE Sync - M5Stack Core2");
    info!("=========================================");
    info!("");

    // Take peripherals
    let peripherals = Peripherals::take()?;
    info!("Peripherals taken");

    let _sys_loop = EspSystemEventLoop::take()?;
    info!("Event loop taken");

    let nvs_partition = EspDefaultNvsPartition::take()?;
    info!("NVS partition taken");

    // Get node ID from MAC address
    let node_id = get_node_id_from_mac();
    info!("");
    info!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    info!("!!! THIS DEVICE NODE ID: {:08X} !!!", node_id.as_u32());
    info!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    info!("");

    // Initialize I2C for touch controller and power management (AXP192/AXP2101)
    info!("Initializing I2C...");
    let i2c_config = I2cConfig::new().baudrate(400.kHz().into());
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio21, // SDA
        peripherals.pins.gpio22, // SCL
        &i2c_config,
    )?;
    info!("I2C initialized");

    // Initialize AXP (detect hardware version and enable LCD power/backlight)
    info!("Initializing AXP...");
    let hw_version = match axp_init(&mut i2c) {
        Ok(v) => {
            info!("Hardware: {:?}", v);
            v
        }
        Err(e) => {
            warn!("AXP init failed: {:?}", e);
            HardwareVersion::Core2V10 // Assume v1.0 on error
        }
    };

    // Initialize SPI for display
    info!("Initializing SPI...");
    // M5Stack Core2: MOSI=23, SCLK=18, CS=5, DC=15
    let spi_driver = SpiDriver::new(
        peripherals.spi2,
        peripherals.pins.gpio18, // SCLK
        peripherals.pins.gpio23, // MOSI
        None::<esp_idf_hal::gpio::AnyIOPin>, // MISO not used
        &SpiDriverConfig::default(),
    )?;
    info!("SPI driver created");

    let spi_config = SpiConfig::default()
        .baudrate(26.MHz().into()); // Lower speed for stability

    let spi_device = SpiDeviceDriver::new(
        spi_driver,
        Some(peripherals.pins.gpio5), // CS
        &spi_config,
    )?;
    info!("SPI device created");

    let dc = PinDriver::output(peripherals.pins.gpio15)?;
    info!("DC pin configured");

    let spi_iface = SPIInterface::new(spi_device, dc);
    info!("SPI interface created");

    // Initialize display (ILI9341 compatible, 320x240)
    info!("Initializing display...");
    let mut display = Builder::new(ILI9342CRgb565, spi_iface)
        .orientation(Orientation::new())
        .color_order(mipidsi::options::ColorOrder::Bgr)
        .invert_colors(mipidsi::options::ColorInversion::Inverted)
        .init(&mut FreeRtos)
        .map_err(|e| anyhow::anyhow!("Display init failed: {:?}", e))?;

    info!("Display initialized");

    // Clear display to show we're alive
    let _ = display.clear(Rgb565::BLUE);
    info!("Display cleared to blue");

    // Initialize HiveMesh for centralized peer management and document sync
    info!("Initializing HiveMesh...");
    let config = HiveMeshConfig::new(node_id, "ESP32", "DEMO")
        .with_peripheral_type(PeripheralType::SoldierSensor);
    let mesh = HiveMesh::new(config);
    info!("HiveMesh created for node {:08X}", node_id.as_u32());

    // Initialize NVS store for persistence
    let mut store = DocumentStore::new(nvs_partition)?;

    // Load any previously saved document and merge into mesh
    if let Some(saved_data) = store.load_raw() {
        if let Some(result) = mesh.on_ble_data("nvs", &saved_data, now_ms()) {
            info!("Loaded saved state: total_count={}", result.total_count);
        }
    }
    info!("HiveMesh initialized: {} total taps", mesh.total_count());

    // Initialize BLE
    info!("Initializing BLE...");
    if let Err(e) = nimble::init(node_id) {
        error!("Failed to initialize BLE: {}", e);
        // Continue without BLE for testing
    }

    // Update BLE with initial document
    let encoded = mesh.build_document();
    nimble::set_document(&encoded);

    info!("All initialization complete!");

    // Read initial battery status
    let battery_mv = axp_read_battery_voltage(&mut i2c);
    let mut battery_pct = battery_mv.map(battery_percent_from_voltage);
    if let Some(mv) = battery_mv {
        info!("Battery: {}mV ({}%)", mv, battery_pct.unwrap_or(0));
    }

    // Draw static UI once, then update dynamic parts
    draw_static_ui(&mut display, node_id.as_u32());
    update_button_labels(&mut display, false);  // Initial state: ACK greyed out
    let mut display_state = DisplayState::default();
    let mut acked_peers: Vec<u32> = Vec::new();
    display_state = update_display(&mut display, &mesh, false, battery_pct, "BtnC=EMERG  BtnA=ACK", &display_state, &acked_peers);
    print_status(&mesh, false, "BtnC=EMERG  BtnA=ACK");

    // Main loop state
    let mut last_button = Button::None;
    let mut loop_count: u32 = 0;
    let mut needs_redraw = false;

    // Alert state
    let mut alert_active = false;
    let mut vibration_on = false;
    let mut last_vibration_toggle: u32 = 0;
    const VIBRATION_INTERVAL_MS: u32 = 500;  // Buzz on/off every 500ms

    loop {
        let button = read_button(&mut i2c);
        let connected = nimble::is_connected();
        let current_time = unsafe { esp_idf_svc::sys::esp_timer_get_time() as u32 / 1000 };

        // Check for connection state changes
        if nimble::take_connection_changed() {
            if connected {
                info!(">>> PEER CONNECTED!");
            } else {
                info!(">>> PEER DISCONNECTED");
            }
            needs_redraw = true;
        }

        // Handle disconnected peers - mark for display update
        for node_id in nimble::take_disconnected_node_ids() {
            info!(">>> Peer disconnected: {:08X}", node_id);
            needs_redraw = true;
        }

        // Handle vibration buzzing when alert is active
        if alert_active {
            let elapsed = current_time.saturating_sub(last_vibration_toggle);
            if elapsed >= VIBRATION_INTERVAL_MS {
                vibration_on = !vibration_on;
                axp_set_vibration(&mut i2c, vibration_on);
                last_vibration_toggle = current_time;
            }
        } else if vibration_on {
            // Turn off vibration if alert cleared
            vibration_on = false;
            axp_set_vibration(&mut i2c, false);
        }

        // Handle button releases (action on release)
        if button == Button::None && last_button != Button::None {
            let now_ms_u64 = now_ms() as u64;
            match last_button {
                Button::BtnA => {
                    // Button A = ACK (also silences alert)
                    info!(">>> BUTTON A - SENDING ACK");
                    let encoded = mesh.send_ack(now_ms_u64);
                    info!("ACK document: {} bytes", encoded.len());

                    // Silence alert after building document (matches EMERGENCY pattern)
                    alert_active = false;
                    axp_set_vibration(&mut i2c, false);
                    vibration_on = false;

                    // Save before gossip (matches EMERGENCY pattern)
                    if let Err(e) = store.save(&mesh) {
                        error!("Failed to save: {:?}", e);
                    }

                    let sent = nimble::gossip_document(&encoded);
                    info!("Gossiped to {} peers", sent);

                    // Clear alert state after sending ACK
                    alert_active = false;
                    acked_peers.clear();
                    axp_set_vibration(&mut i2c, false);
                    vibration_on = false;
                    display_state = update_display(&mut display, &mesh, false, battery_pct, "ACK sent!", &display_state, &acked_peers);
                }
                Button::BtnB => {
                    // Button B = RESET (clear event, but counter is CRDT - can't truly reset)
                    info!(">>> BUTTON B - CLEARING EVENT");
                    mesh.clear_event();
                    alert_active = false;
                    acked_peers.clear();
                    axp_set_vibration(&mut i2c, false);
                    vibration_on = false;

                    if let Err(e) = store.save(&mesh) {
                        error!("Failed to save reset: {:?}", e);
                    }
                    let encoded = mesh.build_document();
                    nimble::set_document(&encoded);
                    display_state = update_display(&mut display, &mesh, false, battery_pct, "CLEARED!", &display_state, &acked_peers);
                }
                Button::BtnC => {
                    // Button C = EMERGENCY
                    info!(">>> BUTTON C - SENDING EMERGENCY!");
                    let encoded = mesh.send_emergency(now_ms_u64);

                    // Enter alert mode locally too (buzz until ACK'd)
                    alert_active = true;
                    acked_peers.clear();  // Fresh emergency, clear old ACKs
                    last_vibration_toggle = current_time;
                    vibration_on = true;
                    axp_set_vibration(&mut i2c, true);

                    // Save and gossip
                    if let Err(e) = store.save(&mesh) {
                        error!("Failed to save: {:?}", e);
                    }
                    let sent = nimble::gossip_document(&encoded);
                    info!(">>> Gossiped EMERGENCY to {} peers", sent);

                    display_state = update_display(&mut display, &mesh, true, battery_pct, "EMERGENCY!", &display_state, &acked_peers);
                }
                Button::None => {}
            }
        }
        last_button = button;

        // Handle pending document from BLE
        if let Some(data) = nimble::take_pending_document() {
            info!("");
            info!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
            info!("!!! RECEIVED {} BYTES FROM BLE !!!", data.len());
            info!("!!! Raw: {:02X?}", &data[..data.len().min(32)]);
            info!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");

            let now_ms_u64 = now_ms();
            if let Some(result) = mesh.on_ble_data("ble-peer", &data, now_ms_u64) {
                info!(
                    "Received from {:08X}: emergency={}, ack={}, counter_changed={}",
                    result.source_node.as_u32(),
                    result.is_emergency,
                    result.is_ack,
                    result.counter_changed
                );

                // Associate node ID with BLE connection for disconnect tracking
                nimble::set_connection_node_id(result.source_node.as_u32());

                // Check if peer is sending EMERGENCY
                if result.is_emergency && !alert_active {
                    info!(">>> RECEIVED EMERGENCY FROM PEER!");
                    alert_active = true;
                    last_vibration_toggle = current_time;
                    vibration_on = true;
                    axp_set_vibration(&mut i2c, true);
                    needs_redraw = true;
                }

                // Check if peer is sending ACK - track it for display
                if result.is_ack && alert_active {
                    let ack_node = result.source_node.as_u32();
                    if !acked_peers.contains(&ack_node) {
                        info!(">>> RECEIVED ACK FROM {:08X}", ack_node);
                        acked_peers.push(ack_node);
                        needs_redraw = true;
                    }
                }

                if result.counter_changed {
                    info!(">>> MERGED! New total: {}", mesh.total_count());

                    // Save merged state
                    if let Err(e) = store.save(&mesh) {
                        error!("Failed to save merged doc: {:?}", e);
                    }

                    // GOSSIP: Forward merged state to ALL other peers (multi-hop!)
                    let encoded = mesh.build_document();
                    let sent = nimble::gossip_document(&encoded);
                    info!(">>> Forwarded merged doc to {} peers (multi-hop)", sent);

                    needs_redraw = true;
                    print_status(&mesh, connected, "Merged & forwarded!");
                } else {
                    info!("No changes from merge (peer had same or less data)");
                }
            } else {
                warn!("Failed to process peer document ({} bytes)", data.len());
            }
        }

        // Redraw display when needed
        if needs_redraw {
            let status: &'static str = if alert_active {
                "!! ALERT - TAP TO ACK !!"
            } else if connected {
                "Connected"
            } else {
                "Advertising..."
            };
            display_state = update_display(&mut display, &mesh, alert_active, battery_pct, status, &display_state, &acked_peers);
            needs_redraw = false;
        }

        // Check if we should rotate to find other peers (mesh behavior)
        nimble::check_rotation();

        // Periodic status update (every 1 second = 20 * 50ms)
        if loop_count % 20 == 0 {
            // Update battery reading and peripheral health
            if let Some(mv) = axp_read_battery_voltage(&mut i2c) {
                let pct = battery_percent_from_voltage(mv);
                battery_pct = Some(pct);
                mesh.update_health(pct);
            }

            let status: &'static str = if alert_active {
                "!! ALERT - TAP TO ACK !!"
            } else if connected {
                "Connected"
            } else {
                "Advertising..."
            };
            display_state = update_display(&mut display, &mesh, alert_active, battery_pct, status, &display_state, &acked_peers);
            print_status(&mesh, connected, status);
        }

        FreeRtos::delay_ms(50);
        loop_count = loop_count.wrapping_add(1);
    }
}
