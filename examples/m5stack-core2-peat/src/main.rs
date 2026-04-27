//! Peat-Lite BLE Alert/Ack Demo for M5Stack Core2
//!
//! Tactical alert system over BLE mesh:
//! - Double tap: Send EMERGENCY alert to all peers (they buzz)
//! - Single tap: Acknowledge (silence local buzz, send ACK)
//! - Long press (3s): Reset counter
//!
//! Uses centralized PeatMesh from peat-btle for peer management and document sync.
//!
//! ## Building
//!
//! ```bash
//! source ~/export-esp.sh
//! cargo build --release
//! espflash flash --monitor target/xtensa-esp32-espidf/release/m5stack-core2-peat
//! ```

mod audio;
mod imu;
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

use peat_btle::canned_message::CannedMessageDocument;
use peat_btle::peat_mesh::{PeatMesh, PeatMeshConfig};
use peat_btle::sync::PeripheralType;
use peat_btle::{MeshGenesis, NodeId};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;

/// Shared mesh genesis for the WEARTAK encrypted mesh. The decoded blob
/// contains `MeshGenesis::encryption_secret()` — i.e. it IS the mesh's
/// long-term root secret. The compile-time fallback below is the demo
/// genesis matching `wearos-tak-civ/.../PeatBtleService.kt`; for any
/// non-demo build, override at compile time with
///   `PEAT_GENESIS_BASE64=<base64> cargo build ...`
/// so the secret lives in the build environment, not in repo history.
///
/// **WARNING:** this fallback genesis is for demo/badge use only. Do not
/// reuse it for any real deployment — the secret is in repo history
/// forever, and key rotation would require both a history rewrite and
/// re-distribution to every node. ADR-044 is the planned path for
/// real (MLS-based) group key distribution.
const SHARED_GENESIS_BASE64: &str = match option_env!("PEAT_GENESIS_BASE64") {
    Some(v) => v,
    None => "BwBXRUFSVEFL4O7thA03dXXBNkT+gG22aTRGICECcX5RHtOgIdLBrb7tU7LTxkFLCLP+De21IALSXAbi6ZR/c3VXW9lKWacbM0YqfK9n5JXqob7/stIM63nBMLzJiFTGl9E6wcF8Gz0gUerY2JsBAAAA",
};

/// True when the build is using the embedded demo-only genesis (no
/// `PEAT_GENESIS_BASE64` env var was set at compile time). Surfaces in a
/// runtime `warn!` at boot so a misconfigured non-demo build is visible
/// in serial — the compile-time comment alone isn't enough; that's
/// exactly the failure mode where a real secret can ship on a fielded
/// node without anyone noticing.
const USING_FALLBACK_GENESIS: bool = option_env!("PEAT_GENESIS_BASE64").is_none();

// NVS storage
const NVS_NAMESPACE: &str = "peat";
const NVS_KEY_COUNTER: &str = "counter";

/// Re-broadcast merged state to every active connection on each received
/// document (multi-hop relay). False here so the M5Stack's small-mesh
/// behavior matches what's been validated under sustained load — the
/// fanout was the worst offender for NimBLE TX queue flooding before the
/// peripheral-only switch. The 5 s periodic gossip handles convergence
/// for 1-hop topologies with no relay needed; for the 48-node demo flip
/// this to true (and re-validate watchdog behavior) so an N-hop chain
/// converges in ~one round-trip per hop instead of N × 5 s.
const MULTIHOP_FORWARD: bool = false;

// FT6336U Touch controller on M5Stack Core2
const FT6336U_ADDR: u8 = 0x38;
const FT6336U_REG_STATUS: u8 = 0x02;

/// Get current timestamp in milliseconds
fn now_ms() -> u64 {
    unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 / 1000 }
}

/// NVS persistence for PeatMesh documents
struct DocumentStore {
    nvs: EspNvs<NvsDefault>,
}

impl DocumentStore {
    fn new(nvs_partition: EspDefaultNvsPartition) -> anyhow::Result<Self> {
        let nvs = EspNvs::new(nvs_partition, NVS_NAMESPACE, true)?;
        Ok(Self { nvs })
    }

    /// Save PeatMesh document to NVS
    fn save(&mut self, mesh: &PeatMesh) -> anyhow::Result<()> {
        let data = mesh.build_document();
        self.nvs.set_raw(NVS_KEY_COUNTER, &data)?;
        info!("NVS: Saved {} bytes", data.len());
        Ok(())
    }

    /// Load document bytes from NVS (for initial merge into PeatMesh)
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

fn print_status(mesh: &PeatMesh, connected: bool, status: &str) {
    let conn_sym = if connected { "●" } else { "○" };
    info!("========================================");
    info!("  Peat-Lite BLE Sync Demo (PeatMesh)");
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

            // Configure LDO3 voltage for vibration motor (register 0x28)
            // Upper nibble = LDO2, Lower nibble = LDO3 (per M5Stack library)
            // Voltage formula: V = 1.8V + (val * 0.1V)
            // For 2.0V vibration: val = 2 = 0x02
            // Read current LDO2/3 config to preserve LDO2
            if i2c.write_read(AXP_ADDR, &[0x28], &mut buf, 100).is_ok() {
                info!("AXP192: LDO2/3 voltage config=0x{:02X}", buf[0]);
                // Set LDO3 to 2.0V (0x02 in lower nibble), preserve LDO2 (upper nibble)
                let new_val = (buf[0] & 0xF0) | 0x02;
                let _ = i2c.write(AXP_ADDR, &[0x28, new_val], 100);
                info!("AXP192: LDO3 set to 2.0V for vibration (0x{:02X})", new_val);
            }

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

            // Set DLDO1 voltage for vibration (3.0V)
            // Per M5Unified: Formula: V = 500 + val*100 mV, so for 3000mV: val = (3000-500)/100 = 25 = 0x19
            let _ = i2c.write(AXP_ADDR, &[0x99, 0x19], 100);
            info!("AXP2101: DLDO1 voltage set to 3.0V for vibration motor");

            // Read LDO control register
            if i2c.write_read(AXP_ADDR, &[0x90], &mut buf, 100).is_ok() {
                info!("AXP2101: LDO control (0x90) = 0x{:02X}", buf[0]);
                // Enable ALDO2 (bit 1), BLDO1 (bit 4), but NOT DLDO1 yet
                // Per M5Unified: DLDO1 is bit 7 (0x80), DLDO2 is bit 6 (0x40)
                let new_val = (buf[0] | 0x12) & !0x80; // Enable ALDO2 + BLDO1, disable DLDO1
                let _ = i2c.write(AXP_ADDR, &[0x90, new_val], 100);
                info!("AXP2101: LDO control: 0x{:02X} (DLDO1 off)", new_val);
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
            // AXP2101: DLDO1 uses bit 0x80 (bit 7) in register 0x90, NOT bit 6!
            // Per M5Unified AXP2101_Class.cpp implementation
            if i2c.write_read(AXP_ADDR, &[0x90], &mut buf, 100).is_ok() {
                let new_val = if enable {
                    buf[0] | 0x80  // Set bit 7 (enable DLDO1)
                } else {
                    buf[0] & !0x80  // Clear bit 7 (disable DLDO1)
                };
                let _ = i2c.write(AXP_ADDR, &[0x90, new_val], 100);
                info!("Vibration (AXP2101): {} (0x90: 0x{:02X} -> 0x{:02X})",
                      if enable { "ON" } else { "OFF" }, buf[0], new_val);
            }
        }
    }
}

/// Build number for tracking firmware versions
const BUILD_NUM: u32 = 62;

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
    activity: u8,                 // Activity state (0=Stationary, 1=Walking, 2=Running, 3=Fall)
    last_canned_msg: Option<(u8, u32, u64)>, // (code, source_node, timestamp)
}

/// Draw initial static UI elements (call once at startup)
fn draw_static_ui<D>(display: &mut D, _node_id: u32)
where
    D: DrawTarget<Color = Rgb565>,
{
    let _ = display.clear(Rgb565::BLACK);

    let _white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let gray = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_GRAY);
    let cyan = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);

    // Top bar: battery | Peat:MESH | build + hw version
    let title = format!("Peat:{}", MESH_ID);
    let _ = Text::new(&title, Point::new(110, 25), cyan).draw(display);
    // Show build number and detected hardware version
    let hw_str = unsafe {
        match HARDWARE_VERSION {
            HardwareVersion::Core2V10 => "192",
            HardwareVersion::Core2V11 => "2101",
        }
    };
    let build_str = format!("b{}/{}", BUILD_NUM, hw_str);
    let _ = Text::new(&build_str, Point::new(220, 25), gray).draw(display);

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


/// Draw canned message ticker at bottom of content area
fn draw_canned_ticker<D>(display: &mut D, msg_name: &str, source_node: u32)
where
    D: DrawTarget<Color = Rgb565>,
{
    // Ticker bar: y=185-205, just above the button separator
    let _ = Rectangle::new(Point::new(0, 185), Size::new(320, 20))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::new(4, 8, 4))) // dark green bg
        .draw(display);
    let style = MonoTextStyle::new(&FONT_10X20, Rgb565::YELLOW);
    let text = format!("{:08X}: {}", source_node, &msg_name[..msg_name.len().min(22)]);
    let _ = Text::new(&text, Point::new(5, 202), style).draw(display);
}

/// Update only changed parts of the display (minimizes flicker)
/// Returns the new display state for comparison on next update
fn update_display<D>(
    display: &mut D,
    mesh: &PeatMesh,
    alert_active: bool,
    battery_pct: Option<u8>,
    _status: &'static str,
    prev: &DisplayState,
    acked_peers: &[u32],
    activity: imu::Activity,
) -> DisplayState
where
    D: DrawTarget<Color = Rgb565>,
{
    // Get peer IDs and connection status from mesh (simpler, more reliable)
    let peers = mesh.get_peers();
    let peer_ids: Vec<u32> = peers.iter().map(|p| p.node_id.as_u32()).collect();
    // Use mesh's is_connected status (based on recent sync activity)
    let connected_peers: Vec<u32> = peers.iter()
        .filter(|p| p.is_connected)
        .map(|p| p.node_id.as_u32())
        .collect();

    // Convert activity to u8 for diff comparison only — the same mapping
    // is sent to peers via mesh.update_health_full() so it must avoid
    // colliding with the legacy enum (0=Stationary, 1=Walking, 2=Running,
    // 3=PossibleFall) on un-updated decoders. Prone uses 4 — un-updated
    // peers see "unknown" and skip rendering rather than mis-rendering as
    // Walking. Standing keeps 0 (semantically aligned with Stationary).
    let activity_u8 = match activity {
        imu::Activity::Standing => 0,
        imu::Activity::Prone => 4,
        imu::Activity::PossibleFall => 3,
    };

    // Build current state
    let current = DisplayState {
        num_connections: nimble::connection_count(),
        battery_pct: battery_pct.unwrap_or(0),
        alert_active,
        peer_count: peers.len(),
        peer_ids: peer_ids.clone(),
        connected_peers: connected_peers.clone(),
        acked_peers: acked_peers.to_vec(),
        activity: activity_u8,
        last_canned_msg: {
            let msgs = mesh.get_all_app_documents_of_type::<CannedMessageDocument>();
            msgs.iter()
                .max_by_key(|m| m.timestamp())
                .map(|m| (m.message_code(), m.source_node(), m.timestamp()))
        },
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

    // Main content area: split into zones so each one only redraws when its
    // own state changes. Full-area clears only happen on alert-mode transition.
    let mode_changed = current.alert_active != prev.alert_active;

    if current.alert_active {
        // ALERT MODE — full red-box scene on entry, peer list refresh on ack
        if mode_changed {
            let _ = Rectangle::new(Point::new(0, 40), Size::new(320, 160))
                .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
                .draw(display);
            let _ = Rectangle::new(Point::new(20, 50), Size::new(280, 150))
                .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
                .draw(display);
            let white_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
            let _ = Text::new("EMERGENCY", Point::new(90, 80), white_style).draw(display);
            let _ = Text::new("Tap ACK to clear", Point::new(70, 185), white_style).draw(display);
        }

        let peers_changed = current.peer_ids != prev.peer_ids
            || current.connected_peers != prev.connected_peers
            || current.acked_peers != prev.acked_peers;
        if mode_changed || peers_changed {
            // Redraw only the peer-list band inside the red box.
            let _ = Rectangle::new(Point::new(25, 100), Size::new(270, 80))
                .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
                .draw(display);
            let white_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
            let green_style = MonoTextStyle::new(&FONT_10X20, Rgb565::GREEN);
            let gray_style = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_GRAY);
            if !peer_ids.is_empty() {
                let mut y = 110;
                for id in peer_ids.iter().take(3) {
                    let is_connected = current.connected_peers.contains(id);
                    let acked = current.acked_peers.contains(id);
                    let (status, style) = if !is_connected {
                        (format!("{:08X} [--]", id), gray_style)
                    } else if acked {
                        (format!("{:08X} [ACK]", id), green_style)
                    } else {
                        (format!("{:08X} ...", id), white_style)
                    };
                    let _ = Text::new(&status, Point::new(70, y), style).draw(display);
                    y += 25;
                }
            } else {
                let _ = Text::new("No peers known", Point::new(80, 120), white_style).draw(display);
            }
        }
    } else {
        // READY MODE — zoned invalidation. Only the zones whose inputs changed
        // clear+redraw, so STAND↔PRONE flips don't wipe the peer list etc.
        let green = MonoTextStyle::new(&FONT_10X20, Rgb565::GREEN);
        let white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let gray = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_GRAY);
        let yellow = MonoTextStyle::new(&FONT_10X20, Rgb565::YELLOW);

        if mode_changed {
            // Coming out of alert: wipe once so the zones below paint on a
            // black canvas instead of on top of the old red box.
            let _ = Rectangle::new(Point::new(0, 40), Size::new(320, 160))
                .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
                .draw(display);
        }

        // Zone A — callsign + node id (static for the life of the boot).
        if mode_changed {
            let node_id_str = format!("{}  {:08X}", mesh.callsign(), mesh.node_id().as_u32());
            let x = ((320 - (node_id_str.len() as i32) * 10) / 2).max(0);
            let _ = Text::new(&node_id_str, Point::new(x, 55), white).draw(display);
        }

        // Zone B — READY + activity badge (y ≈ 80). Flips with orientation.
        if mode_changed || current.activity != prev.activity {
            let _ = Rectangle::new(Point::new(0, 62), Size::new(320, 24))
                .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
                .draw(display);
            let _ = Text::new("READY", Point::new(125, 80), green).draw(display);
            let (activity_str, activity_style) = match activity {
                imu::Activity::Standing => ("STAND", green),
                imu::Activity::Prone => ("PRONE", yellow),
                imu::Activity::PossibleFall => ("FALL!", MonoTextStyle::new(&FONT_10X20, Rgb565::RED)),
            };
            let _ = Text::new(activity_str, Point::new(230, 80), activity_style).draw(display);
        }

        // Zone C+D — mesh count + peer list (y ≈ 105 to 200). Only peer
        // membership or connection state drive this; activity flips skip it.
        let peers_changed = mode_changed
            || current.peer_ids != prev.peer_ids
            || current.connected_peers != prev.connected_peers;
        if peers_changed {
            let _ = Rectangle::new(Point::new(0, 105), Size::new(320, 95))
                .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
                .draw(display);
            if !peer_ids.is_empty() {
                let connected_count = current.connected_peers.len();
                let total_count = peer_ids.len();
                let count_str = format!("Mesh: {}/{} peers", connected_count, total_count);
                let _ = Text::new(&count_str, Point::new(80, 120), white).draw(display);

                let mut y = 145;
                for id in peer_ids.iter().take(3) {
                    let is_connected = current.connected_peers.contains(id);
                    let id_str = format!("{:08X}", id);
                    let style = if is_connected { green } else { gray };
                    let x = if peer_ids.len() == 1 { 115 } else { 60 };
                    let _ = Text::new(&id_str, Point::new(x, y), style).draw(display);
                    let status = if is_connected { " [OK]" } else { " [--]" };
                    let _ = Text::new(status, Point::new(x + 85, y), style).draw(display);
                    y += 25;
                }
                if peer_ids.len() > 3 {
                    let _ = Text::new(&format!("+{} more", peer_ids.len() - 3), Point::new(100, y), gray).draw(display);
                }
            } else {
                let _ = Text::new("Mesh: no peers", Point::new(85, 130), gray).draw(display);
            }
        }
    }

    // Canned message ticker
    if current.last_canned_msg != prev.last_canned_msg {
        if let Some((_code, source, _ts)) = &current.last_canned_msg {
            let msgs = mesh.get_all_app_documents_of_type::<CannedMessageDocument>();
            if let Some(latest) = msgs.iter().max_by_key(|m| m.timestamp()) {
                draw_canned_ticker(display, latest.message_name(), *source);
            }
        }
    }

    // Update button labels when alert state changes
    if current.alert_active != prev.alert_active {
        update_button_labels(display, current.alert_active);
    }

    current
}

/// Get list of peer IDs that have ACKed the current emergency from the document
/// Excludes the emergency source (they initiated, not ACKed)
fn get_acked_peers_from_mesh(mesh: &PeatMesh) -> Vec<u32> {
    // Get the source node so we can exclude it from the "acked" list
    let source_node = mesh.get_emergency_status().map(|(src, _, _, _)| src);

    let peers = mesh.get_peers();
    peers
        .iter()
        .filter(|p| {
            let peer_id = p.node_id.as_u32();
            // Exclude the source - they initiated, not ACKed
            source_node != Some(peer_id) && mesh.has_peer_acked(peer_id)
        })
        .map(|p| p.node_id.as_u32())
        .collect()
}

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("");
    info!("=========================================");
    info!("  Peat-Lite BLE Sync - M5Stack Core2");
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

    // Initialize MPU6886 IMU for activity/fall detection.
    //
    // `imu_ok` is mutable because boot-time init can fail transiently
    // (I2C bus glitch, slow MPU6886 power-up). If that happens we'd be
    // stuck with activity detection disabled forever, so the main loop
    // periodically retries `imu::init` (see IMU_BOOT_RETRY_MS) until
    // it succeeds.
    info!("Initializing IMU...");
    let mut imu_ok = imu::init(&mut i2c);
    let mut imu_state = imu::ImuState::default();
    if imu_ok {
        info!("IMU initialized successfully");
    } else {
        warn!("IMU initialization failed - activity detection disabled (will retry)");
    }

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

    // Initialize PeatMesh for centralized peer management and document sync.
    // Uses the same shared WEARTAK genesis as the WearOS watch and ATAK plugin
    // so encrypted sync documents decode on this node.
    info!("Initializing PeatMesh...");
    if USING_FALLBACK_GENESIS {
        warn!(
            "PEAT GENESIS: using embedded DEMO/badge fallback (PEAT_GENESIS_BASE64 \
             was not set at build time). Do NOT ship this build to a non-demo \
             deployment — the secret is in repo history forever. See ADR-044."
        );
    }
    let genesis_bytes = BASE64_STANDARD
        .decode(SHARED_GENESIS_BASE64)
        .expect("SHARED_GENESIS_BASE64 must be valid base64");
    let genesis = MeshGenesis::decode(&genesis_bytes)
        .expect("SHARED_GENESIS_BASE64 must decode to a MeshGenesis");
    info!(
        "Using SHARED WEARTAK genesis: mesh_id={}",
        genesis.mesh_id()
    );
    // Callsign derives from the low 16 bits of the node id so multiple M5Stacks
    // on the same mesh still stay distinguishable on a display at a glance.
    let callsign = format!("SCOUT-{:04X}", node_id.as_u32() as u16);
    let config = PeatMeshConfig::new(node_id, &callsign, &genesis.mesh_id())
        .with_encryption(genesis.encryption_secret())
        .with_peripheral_type(PeripheralType::SoldierSensor);
    let mesh = PeatMesh::new(config);
    mesh.document_registry().try_register::<CannedMessageDocument>();
    info!("PeatMesh created for node {:08X}", node_id.as_u32());

    // Initialize NVS store for persistence
    let mut store = DocumentStore::new(nvs_partition)?;

    // Load any previously saved document and merge into mesh
    if let Some(saved_data) = store.load_raw() {
        if let Some(result) = mesh.on_ble_data("nvs", &saved_data, now_ms()) {
            info!("Loaded saved state: total_count={}", result.total_count);
        }
    }
    // Clear any stale emergency/ACK state from previous session
    // We want to start fresh on boot without old alerts
    mesh.clear_event();       // Clear peripheral event
    mesh.clear_emergency();   // Clear document emergency + ACK state
    info!("PeatMesh initialized: {} total taps (events cleared)", mesh.total_count());

    // Initialize BLE. The advertised name encodes the mesh_id so WEARTAK peers
    // recognize this device as part of their mesh during scan/filtering.
    info!("Initializing BLE...");
    let ble_device_name = format!("PEAT_{}-{:08X}", genesis.mesh_id(), node_id.as_u32());
    if let Err(e) = nimble::init(node_id, &ble_device_name) {
        error!("Failed to initialize BLE: {}", e);
        // Continue without BLE for testing
    }

    // Update BLE with initial document
    let encoded = mesh.build_document();
    nimble::set_document(&encoded);

    info!("All initialization complete!");

    // Test vibration motor at startup (AXP2101: voltage-based control)
    info!("Testing vibration motor...");
    axp_set_vibration(&mut i2c, true);
    FreeRtos::delay_ms(300);
    axp_set_vibration(&mut i2c, false);
    info!("Vibration test complete");

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
    let acked = get_acked_peers_from_mesh(&mesh);
    display_state = update_display(&mut display, &mesh, false, battery_pct, "BtnC=EMERG  BtnA=ACK", &display_state, &acked, imu::Activity::Standing);
    print_status(&mesh, false, "BtnC=EMERG  BtnA=ACK");

    // Main loop state
    let mut last_button = Button::None;
    let mut loop_count: u32 = 0;
    let mut needs_redraw = false;

    // IMU state
    let mut last_imu_read: u32 = 0;
    const IMU_READ_INTERVAL_MS: u32 = 100;  // Read IMU at 10Hz
    let mut current_activity = imu::Activity::Standing;
    let mut fall_detected = false;

    // IMU read-failure tracking. On the M5Stack Core2 the AXP2101 power
    // mgmt and the MPU6886 share an I2C bus; an interrupted transaction
    // can leave reads silently returning None for an indefinite stretch,
    // which presents as the activity badge "stuck" on its last value.
    // Track consecutive failures and re-init the chip after a short
    // streak so we self-recover instead of staying frozen.
    let mut imu_consecutive_fails: u32 = 0;
    const IMU_REINIT_THRESHOLD: u32 = 10; // ~1 s of failures at 10 Hz poll

    // Counter-of-counters: how many re-init attempts in a row have failed
    // OR succeeded but left the bus broken (next read also fails). After
    // IMU_REINIT_GIVEUP_STREAK consecutive cycles we escalate to error!
    // once so the operator sees that the bus is permanently broken,
    // rather than spamming a warn! every ~1 s forever.
    let mut imu_reinit_failure_streak: u32 = 0;
    let mut imu_reinit_giveup_logged = false;
    const IMU_REINIT_GIVEUP_STREAK: u32 = 5;

    // If boot-time `imu::init` failed, retry it from the main loop so a
    // transient I2C/power glitch at startup doesn't permanently disable
    // activity detection. Retry every 30 s — fast enough to recover
    // before the operator notices a frozen STAND/PRONE, slow enough not
    // to spam the bus.
    let mut last_imu_boot_retry: u32 = 0;
    const IMU_BOOT_RETRY_MS: u32 = 30_000;

    // Alert state
    let mut alert_active = false;
    let mut vibration_on = false;
    let mut last_vibration_toggle: u32 = 0;
    let mut we_are_emergency_source = false;  // Don't buzz if we sent the emergency
    const VIBRATION_INTERVAL_MS: u32 = 500;  // Buzz on/off every 500ms

    // Track last processed emergency to avoid re-triggers from same emergency
    let mut last_emergency: Option<(u32, u64)> = None;  // (node_id, timestamp)

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

        // If IMU init failed at boot, try again periodically. Without
        // this, a transient I2C glitch at startup leaves activity
        // detection permanently off — the badge displays the last
        // known activity (often STAND) forever, which looks "frozen".
        if !imu_ok && current_time.saturating_sub(last_imu_boot_retry) >= IMU_BOOT_RETRY_MS {
            last_imu_boot_retry = current_time;
            if imu::init(&mut i2c) {
                info!("IMU: post-boot init retry succeeded — activity detection enabled");
                imu_ok = true;
            }
        }

        // Read IMU and detect activity/falls (every 100ms)
        if imu_ok && current_time.saturating_sub(last_imu_read) >= IMU_READ_INTERVAL_MS {
            last_imu_read = current_time;

            if let Some(accel) = imu::read_accel(&mut i2c) {
                if imu_consecutive_fails > 0 {
                    info!(
                        "IMU: read recovered after {} failures",
                        imu_consecutive_fails
                    );
                    imu_consecutive_fails = 0;
                    imu_reinit_failure_streak = 0;
                    imu_reinit_giveup_logged = false;
                }
                let activity = imu_state.update(&accel, now_ms());

                // Periodic accel snapshot (~every 2 s) regardless of activity
                // change. Lets us validate IMU reads + axis mapping when the
                // badge looks "stuck" — without this, a steady classification
                // produces no log line and we can't tell if reads are even
                // happening or what gravity is on which axis.
                if loop_count % 40 == 0 {
                    info!(
                        "IMU: accel x={:.2} y={:.2} z={:.2} mag={:.2} -> {:?}",
                        accel.x, accel.y, accel.z, accel.magnitude(), activity
                    );
                }

                // Check for activity change
                if activity != current_activity {
                    info!("Activity: {:?} -> {:?} (accel: x={} y={} z={} mag={})",
                          current_activity, activity,
                          accel.x, accel.y, accel.z, accel.magnitude());
                    current_activity = activity;
                    needs_redraw = true;

                    // Fall detection - trigger emergency automatically!
                    if activity == imu::Activity::PossibleFall && !fall_detected && !alert_active {
                        fall_detected = true;
                        info!("!!! FALL DETECTED - AUTO-TRIGGERING EMERGENCY !!!");

                        let now_ms_u64 = now_ms();
                        let encoded = mesh.start_emergency_with_known_peers(now_ms_u64);
                        info!("Fall emergency document: {} bytes", encoded.len());

                        // Enter alert mode (no buzz - we triggered it)
                        alert_active = true;
                        we_are_emergency_source = true;

                        // Save and gossip
                        if let Err(e) = store.save(&mesh) {
                            error!("Failed to save fall emergency: {:?}", e);
                        }
                        let sent = nimble::gossip_document(&encoded);
                        info!("Gossiped FALL EMERGENCY to {} peers", sent);

                        let acked = get_acked_peers_from_mesh(&mesh);
                        display_state = update_display(&mut display, &mesh, true, battery_pct, "FALL DETECTED!", &display_state, &acked, current_activity);
                    }
                }

                // Reset fall detection when activity returns to normal
                if fall_detected && activity != imu::Activity::PossibleFall {
                    fall_detected = false;
                }
            } else {
                // IMU read returned None (likely an I2C bus glitch from
                // contention with AXP2101 battery reads). Without the
                // recovery below we'd silently fall through every IMU
                // tick from now on — the badge would stay on its last
                // classification ("stuck on STAND/PRONE") with no log
                // trail showing why.
                imu_consecutive_fails += 1;
                if imu_consecutive_fails == 1 {
                    warn!("IMU: read returned None (first failure)");
                } else if imu_consecutive_fails == IMU_REINIT_THRESHOLD {
                    if !imu_reinit_giveup_logged {
                        warn!(
                            "IMU: {} consecutive failed reads, attempting re-init (streak {})",
                            imu_consecutive_fails, imu_reinit_failure_streak
                        );
                    }
                    let reinit_ok = imu::init(&mut i2c);
                    if reinit_ok {
                        info!("IMU: re-initialized successfully");
                        imu_reinit_failure_streak = 0;
                        imu_reinit_giveup_logged = false;
                    } else {
                        imu_reinit_failure_streak += 1;
                        if imu_reinit_failure_streak >= IMU_REINIT_GIVEUP_STREAK
                            && !imu_reinit_giveup_logged
                        {
                            error!(
                                "IMU: {} consecutive re-init attempts have failed — \
                                 bus is likely permanently broken until next reboot. \
                                 Subsequent attempts will be silent.",
                                imu_reinit_failure_streak
                            );
                            imu_reinit_giveup_logged = true;
                        } else if !imu_reinit_giveup_logged {
                            warn!("IMU: re-init failed; will retry on next failure streak");
                        }
                    }
                    // Reset the read-failure counter either way: on success
                    // the next read should succeed; on failure we want the
                    // next IMU_REINIT_THRESHOLD reads to trigger another
                    // attempt rather than logging every single tick.
                    imu_consecutive_fails = 0;
                }
            }
        }

        // Handle vibration buzzing when alert is active (but not if we sent it)
        if alert_active && !we_are_emergency_source {
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
                    // Button A = ACK (using document-based tracking)
                    info!(">>> BUTTON A - SENDING ACK");

                    // Use document-based ACK - this updates the emergency document's ACK map
                    if let Some(encoded) = mesh.ack_emergency(now_ms_u64) {
                        info!("ACK document: {} bytes", encoded.len());

                        // Silence alert
                        alert_active = false;
                        axp_set_vibration(&mut i2c, false);
                        vibration_on = false;

                        // Save and gossip
                        if let Err(e) = store.save(&mesh) {
                            error!("Failed to save: {:?}", e);
                        }

                        let sent = nimble::gossip_document(&encoded);
                        info!("Gossiped ACK to {} peers", sent);

                        let acked = get_acked_peers_from_mesh(&mesh);
                        display_state = update_display(&mut display, &mesh, false, battery_pct, "ACK sent!", &display_state, &acked, current_activity);
                    } else {
                        info!("No active emergency to ACK");
                        // Still silence any local alert state
                        alert_active = false;
                        axp_set_vibration(&mut i2c, false);
                        vibration_on = false;
                        let acked = get_acked_peers_from_mesh(&mesh);
                        display_state = update_display(&mut display, &mesh, false, battery_pct, "No emergency", &display_state, &acked, current_activity);
                    }
                }
                Button::BtnB => {
                    // Button B = RESET (clear emergency and event state)
                    info!(">>> BUTTON B - CLEARING EVENT");
                    mesh.clear_emergency();  // Clears document-based emergency
                    mesh.clear_event();      // Clears peripheral event
                    alert_active = false;
                    last_emergency = None;
                    axp_set_vibration(&mut i2c, false);
                    vibration_on = false;

                    if let Err(e) = store.save(&mesh) {
                        error!("Failed to save reset: {:?}", e);
                    }
                    let encoded = mesh.build_document();
                    nimble::set_document(&encoded);
                    let acked = get_acked_peers_from_mesh(&mesh);
                    display_state = update_display(&mut display, &mesh, false, battery_pct, "CLEARED!", &display_state, &acked, current_activity);
                }
                Button::BtnC => {
                    // Button C = EMERGENCY (using document-based tracking)
                    info!(">>> BUTTON C - SENDING EMERGENCY! (ts={})", now_ms_u64);

                    // Log known peers before creating emergency
                    let peers = mesh.get_peers();
                    info!("Known peers: {} total", peers.len());
                    for peer in &peers {
                        info!("  Peer {:08X}: connected={}", peer.node_id.as_u32(), peer.is_connected);
                    }

                    // Use document-based emergency with built-in ACK tracking
                    let encoded = mesh.start_emergency_with_known_peers(now_ms_u64);
                    info!("Created emergency document: {} bytes", encoded.len());

                    // Log emergency status after creation
                    if let Some((src, ts, acked, pending)) = mesh.get_emergency_status() {
                        info!("Emergency status: source={:08X} ts={} acked={} pending={}", src, ts, acked, pending);
                    }

                    // Debug: log ACK status of each peer RIGHT AFTER creation
                    info!(">>> Initial ACK status (should all be false except source):");
                    for peer in mesh.get_peers() {
                        let has_acked = mesh.has_peer_acked(peer.node_id.as_u32());
                        info!(">>>   peer {:08X}: has_peer_acked={}", peer.node_id.as_u32(), has_acked);
                    }

                    // Track for gossip deduplication
                    last_emergency = Some((node_id.as_u32(), now_ms_u64));

                    // Enter alert mode locally (no buzz - we sent it)
                    alert_active = true;
                    we_are_emergency_source = true;  // Don't buzz for our own emergency

                    // Save and gossip
                    if let Err(e) = store.save(&mesh) {
                        error!("Failed to save: {:?}", e);
                    }
                    let sent = nimble::gossip_document(&encoded);
                    info!(">>> Gossiped EMERGENCY to {} peers", sent);

                    // Get ACK status from document for display
                    let acked = get_acked_peers_from_mesh(&mesh);
                    display_state = update_display(&mut display, &mesh, true, battery_pct, "EMERGENCY!", &display_state, &acked, current_activity);
                }
                Button::None => {}
            }
        }
        last_button = button;

        // Drain at most DOC_DRAIN_PER_TICK pending documents per main-loop
        // tick. Each iteration does an NVS save plus (under MULTIHOP_FORWARD)
        // a gossip_document fanout, both of which spend time deep in
        // ESP-IDF/NimBLE; processing the whole queue in one tick can starve
        // IDLE long enough to trip the task watchdog. Two-per-tick keeps the
        // bound well under the 60 s WDT budget while halving the worst-case
        // ack-burst feedback latency for emergency-broadcast scenarios. Any
        // remaining queue gets picked up on the next tick (50 ms later).
        const DOC_DRAIN_PER_TICK: u8 = 2;
        let mut drained = 0u8;
        while drained < DOC_DRAIN_PER_TICK {
            let Some(data) = nimble::take_pending_document() else {
                break;
            };
            drained += 1;
            info!("");
            info!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
            info!("!!! RECEIVED {} BYTES FROM BLE !!!", data.len());
            info!("!!! Raw: {:02X?}", &data[..data.len().min(32)]);
            info!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");

            let now_ms_u64 = now_ms();
            // Try to decode document for debugging before processing
            if let Some(decoded_doc) = peat_btle::document::PeatDocument::decode(&data) {
                info!("Decoded doc: version={} node_id={:08X} counter_value={}",
                      decoded_doc.version, decoded_doc.node_id.as_u32(), decoded_doc.total_count());
            } else {
                warn!("!!! Failed to decode document ({} bytes)", data.len());
            }

            if let Some(result) = mesh.on_ble_data("ble-peer", &data, now_ms_u64) {
                info!(
                    "Received from {:08X}: emergency={}, ack={}, counter_changed={}",
                    result.source_node.as_u32(),
                    result.is_emergency,
                    result.is_ack,
                    result.counter_changed
                );
                // Log peer count after receiving
                info!("Mesh now has {} peers, {} connected",
                      mesh.get_peers().len(), mesh.get_connected_peers().len());

                // Associate node ID with BLE connection for disconnect tracking
                nimble::set_connection_node_id(result.source_node.as_u32());

                // Check document's emergency state (CRDT merge already updated it)
                // Use document state instead of peripheral event for proper tracking
                if let Some((source, ts, acked_count, pending_count)) = mesh.get_emergency_status() {
                    let emergency_key = (source, ts);
                    let is_new = last_emergency.map_or(true, |prev| prev != emergency_key);

                    if is_new && !alert_active {
                        info!(">>> RECEIVED EMERGENCY FROM {:08X} (ts={}, {}/{} acked)",
                              source, ts, acked_count, acked_count + pending_count);
                        last_emergency = Some(emergency_key);
                        alert_active = true;
                        we_are_emergency_source = false;  // We received it, so buzz
                        last_vibration_toggle = current_time;
                        vibration_on = true;
                        axp_set_vibration(&mut i2c, true);
                        needs_redraw = true;
                    } else if !is_new {
                        // Same emergency - check for ACK updates
                        info!(">>> ACK update: source={:08X} {}/{} acked",
                              source, acked_count, acked_count + pending_count);
                        // Log which peers have acked according to mesh
                        for peer in mesh.get_peers() {
                            let has_acked = mesh.has_peer_acked(peer.node_id.as_u32());
                            info!(">>>   peer {:08X}: has_peer_acked={}", peer.node_id.as_u32(), has_acked);
                        }
                        needs_redraw = true;
                    }
                }

                // Also check peripheral event for backward compatibility
                if result.is_emergency && !alert_active {
                    let emergency_key = (result.source_node.as_u32(), result.event_timestamp);
                    let is_new = last_emergency.map_or(true, |prev| prev != emergency_key);

                    if is_new {
                        info!(">>> RECEIVED EMERGENCY (peripheral event) FROM {:08X} (ts={})",
                              result.source_node.as_u32(), result.event_timestamp);
                        last_emergency = Some(emergency_key);
                        alert_active = true;
                        we_are_emergency_source = false;  // We received it, so buzz
                        last_vibration_toggle = current_time;
                        vibration_on = true;
                        axp_set_vibration(&mut i2c, true);
                        needs_redraw = true;
                    }
                }

                if result.is_ack && alert_active {
                    info!(">>> RECEIVED ACK (peripheral event) FROM {:08X}",
                          result.source_node.as_u32());
                    needs_redraw = true;
                }

                // Gossip when counter OR emergency state changes (for ACK propagation)
                if result.counter_changed || result.emergency_changed {
                    if result.counter_changed {
                        info!(">>> MERGED! New total: {}", mesh.total_count());
                    }
                    if result.emergency_changed {
                        info!(">>> EMERGENCY STATE CHANGED (ACK update)");
                    }

                    // Save merged state
                    if let Err(e) = store.save(&mesh) {
                        error!("Failed to save merged doc: {:?}", e);
                    }

                    // Post-merge multi-hop forward, gated on MULTIHOP_FORWARD.
                    // Disabled for the small-mesh demo: the per-doc fanout
                    // floods NimBLE's TX queue and previously starved IDLE1
                    // on CPU 1 enough to trip the task watchdog. The 5 s
                    // periodic gossip is sufficient for 1-hop topologies.
                    // Re-enable for the 48-node demo so an N-hop chain
                    // converges in roughly one round-trip per hop instead
                    // of N × 5 s.
                    if MULTIHOP_FORWARD {
                        let encoded = mesh.build_document();
                        let sent = nimble::gossip_document(&encoded);
                        info!(">>> Forwarded merged doc to {} peers (multi-hop)", sent);
                    }
                    needs_redraw = true;
                    print_status(&mesh, connected, "Merged");
                } else {
                    info!("No changes from merge (peer had same or less data)");
                }
            } else {
                // on_ble_data returned None - could be:
                // 1. Document decode failed
                // 2. Document was from ourselves (filtered out)
                let our_node_id = mesh.node_id().as_u32();
                if let Some(decoded) = peat_btle::document::PeatDocument::decode(&data) {
                    if decoded.node_id.as_u32() == our_node_id {
                        info!("Ignored own document (node {:08X})", our_node_id);
                    } else {
                        warn!("on_ble_data returned None for document from {:08X} (our node: {:08X})",
                              decoded.node_id.as_u32(), our_node_id);
                    }
                } else {
                    warn!("Failed to decode document ({} bytes)", data.len());
                }
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
            let acked = get_acked_peers_from_mesh(&mesh);
            display_state = update_display(&mut display, &mesh, alert_active, battery_pct, status, &display_state, &acked, current_activity);
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
                // Wire-format activity code sent to peers via
                // mesh.update_health_full(). Avoid colliding with the
                // legacy enum (0=Stationary, 1=Walking, 2=Running,
                // 3=PossibleFall) so un-updated peers don't mis-render
                // Prone as Walking. Prone uses 4; old peers fall through
                // to "unknown" instead of a wrong label.
                //   0 = Standing       (legacy: Stationary — semantic match)
                //   1 = (legacy Walking — no longer emitted)
                //   2 = (legacy Running — no longer emitted)
                //   3 = PossibleFall   (unchanged)
                //   4 = Prone          (new — no legacy collision)
                let activity_u8 = match current_activity {
                    imu::Activity::Standing => 0,
                    imu::Activity::Prone => 4,
                    imu::Activity::PossibleFall => 3,
                };
                mesh.update_health_full(pct, activity_u8);
            }

            let status: &'static str = if alert_active {
                "!! ALERT - TAP TO ACK !!"
            } else if connected {
                "Connected"
            } else {
                "Advertising..."
            };
            let acked = get_acked_peers_from_mesh(&mesh);
            display_state = update_display(&mut display, &mesh, alert_active, battery_pct, status, &display_state, &acked, current_activity);
            print_status(&mesh, connected, status);
        }

        // Periodic gossip (every 5 seconds = 100 * 50ms) to ensure peer discovery
        // This ensures both sides exchange documents even if initial sync was incomplete
        if loop_count % 100 == 0 && connected {
            let conn_count = nimble::connection_count();
            if conn_count > 0 {
                let encoded = mesh.build_document();
                let sent = nimble::gossip_document(&encoded);
                info!("Periodic gossip: sent to {} of {} connections", sent, conn_count);
            }
        }

        FreeRtos::delay_ms(50);
        loop_count = loop_count.wrapping_add(1);
    }
}
