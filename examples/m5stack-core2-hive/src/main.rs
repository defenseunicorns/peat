//! HIVE-Lite BLE Alert/Ack Demo for M5Stack Core2
//!
//! Tactical alert system over BLE mesh:
//! - Double tap: Send EMERGENCY alert to all peers (they buzz)
//! - Single tap: Acknowledge (silence local buzz, send ACK)
//! - Long press (3s): Reset counter
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

use hive_btle::sync::{EventType, GCounter, HealthStatus, Peripheral, PeripheralType};
use hive_btle::NodeId;

// NVS storage
const NVS_NAMESPACE: &str = "hive";
const NVS_KEY_COUNTER: &str = "counter";

// FT6336U Touch controller on M5Stack Core2
const FT6336U_ADDR: u8 = 0x38;
const FT6336U_REG_STATUS: u8 = 0x02;

/// CRDT Document with persistence and Peripheral state
struct HiveDocument {
    pub counter: GCounter,
    pub peripheral: Peripheral,
    node_id: NodeId,
    version: u32,
}

impl HiveDocument {
    fn new(node_id: NodeId) -> Self {
        let mut peripheral = Peripheral::new(node_id.as_u32(), PeripheralType::SoldierSensor);
        peripheral.health = HealthStatus::new(100); // Will be updated with real battery
        Self {
            counter: GCounter::new(),
            peripheral,
            node_id,
            version: 0,
        }
    }

    /// Send EMERGENCY alert (double-tap)
    fn send_emergency(&mut self) {
        info!(">>> EMERGENCY from node {:08X}", self.node_id.as_u32());
        self.debug_dump("BEFORE EMERGENCY");
        self.counter.increment(&self.node_id, 1);
        self.version += 1;

        let timestamp = unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 / 1000 };
        self.peripheral.set_event(EventType::Emergency, timestamp);
        self.peripheral.timestamp = timestamp;

        self.debug_dump("AFTER EMERGENCY");
    }

    /// Send ACK (single-tap when alert is active)
    fn send_ack(&mut self) {
        info!(">>> ACK from node {:08X}", self.node_id.as_u32());
        self.counter.increment(&self.node_id, 1);
        self.version += 1;

        let timestamp = unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 / 1000 };
        self.peripheral.set_event(EventType::Ack, timestamp);
        self.peripheral.timestamp = timestamp;
    }

    /// Clear event (after timeout or processing)
    fn clear_event(&mut self) {
        self.peripheral.clear_event();
    }

    fn current_event(&self) -> EventType {
        self.peripheral.last_event.as_ref()
            .map(|e| e.event_type)
            .unwrap_or(EventType::None)
    }

    fn update_health(&mut self, battery_pct: u8) {
        self.peripheral.health.battery_percent = battery_pct;
        if battery_pct < 20 {
            self.peripheral.health.set_alert(HealthStatus::ALERT_LOW_BATTERY);
        } else {
            self.peripheral.health.clear_alert(HealthStatus::ALERT_LOW_BATTERY);
        }
    }

    fn total_taps(&self) -> u64 {
        self.counter.value()
    }

    fn num_nodes(&self) -> usize {
        self.counter.node_count_total()
    }

    /// Debug dump the GCounter state
    fn debug_dump(&self, prefix: &str) {
        let encoded = self.counter.encode();
        if encoded.len() >= 4 {
            let num_entries = u32::from_le_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]);
            info!("{}: {} entries, total={}, v{}", prefix, num_entries, self.total_taps(), self.version);
            let mut offset = 4;
            for i in 0..num_entries as usize {
                if offset + 12 <= encoded.len() {
                    let node = u32::from_le_bytes([
                        encoded[offset], encoded[offset+1], encoded[offset+2], encoded[offset+3]
                    ]);
                    let count = u64::from_le_bytes([
                        encoded[offset+4], encoded[offset+5], encoded[offset+6], encoded[offset+7],
                        encoded[offset+8], encoded[offset+9], encoded[offset+10], encoded[offset+11]
                    ]);
                    info!("{}:   [{}] node={:08X} count={}", prefix, i, node, count);
                    offset += 12;
                }
            }
        }
    }

    /// Merge another document into this one
    fn merge(&mut self, other: &HiveDocument) -> bool {
        info!("=== MERGE START ===");
        self.debug_dump("OURS BEFORE");
        other.debug_dump("THEIRS");

        let old_value = self.counter.value();
        self.counter.merge(&other.counter);
        let changed = self.counter.value() != old_value;
        if changed {
            self.version += 1;
        }

        self.debug_dump("OURS AFTER");
        info!("=== MERGE END (changed={}) ===", changed);
        changed
    }

    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Header: version (4) + node_id (4) = 8 bytes (same as Build 11)
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.node_id.as_u32().to_le_bytes());

        // Counter data FIRST (for backwards compatibility with Build 11)
        let counter_data = self.counter.encode();
        buf.extend_from_slice(&counter_data);

        // Peripheral data after counter (Build 12+)
        // Marker byte 0xAB to indicate extended format
        buf.push(0xAB);
        buf.push(0); // reserved byte
        let peripheral_data = self.peripheral.encode();
        buf.extend_from_slice(&(peripheral_data.len() as u16).to_le_bytes());
        buf.extend_from_slice(&peripheral_data);

        info!("Encoded doc: {} bytes (header=8, counter={}, peripheral={})",
              buf.len(), counter_data.len(), peripheral_data.len());
        buf
    }

    fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let node_id = NodeId::new(u32::from_le_bytes([data[4], data[5], data[6], data[7]]));

        // Counter data starts at offset 8 (same as Build 11)
        let counter = GCounter::decode(&data[8..])?;

        // Calculate where counter data ends
        let num_entries = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let counter_end = 8 + 4 + num_entries * 12;

        // Check for extended format (Build 12+)
        let peripheral = if data.len() > counter_end && data[counter_end] == 0xAB {
            // data[counter_end + 1] is reserved byte, skip it
            let peripheral_len =
                u16::from_le_bytes([data[counter_end + 2], data[counter_end + 3]]) as usize;
            let peripheral_start = counter_end + 4;
            if data.len() >= peripheral_start + peripheral_len {
                let p = Peripheral::decode(&data[peripheral_start..peripheral_start + peripheral_len]);
                if p.is_some() {
                    info!("Decoded Peripheral OK ({} bytes), event={:?}",
                          peripheral_len, p.as_ref().map(|x| x.last_event.as_ref().map(|e| e.event_type)));
                } else {
                    warn!("Peripheral decode FAILED ({} bytes available)", peripheral_len);
                }
                p
            } else {
                warn!("Peripheral data truncated! need {} bytes at offset {}, have {}",
                      peripheral_len, peripheral_start, data.len());
                None
            }
        } else {
            // Old format (Build 11) - no peripheral data
            info!("No peripheral marker (counter_end={}, data.len={})", counter_end, data.len());
            None
        };

        // Create peripheral if not decoded from message
        let peripheral = peripheral.unwrap_or_else(|| {
            warn!("Creating default Peripheral (no event data from peer)");
            Peripheral::new(node_id.as_u32(), PeripheralType::SoldierSensor)
        });

        Some(Self {
            counter,
            peripheral,
            node_id,
            version,
        })
    }
}

/// Document store with NVS persistence
struct DocumentStore {
    nvs: EspNvs<NvsDefault>,
}

impl DocumentStore {
    fn new(nvs_partition: EspDefaultNvsPartition) -> anyhow::Result<Self> {
        let nvs = EspNvs::new(nvs_partition, NVS_NAMESPACE, true)?;
        Ok(Self { nvs })
    }

    fn load(&self, node_id: NodeId) -> HiveDocument {
        let mut buf = [0u8; 256];
        match self.nvs.get_raw(NVS_KEY_COUNTER, &mut buf) {
            Ok(Some(data)) => {
                info!("NVS: loaded {} raw bytes", data.len());
                if let Some(mut doc) = HiveDocument::decode(data) {
                    // Update node_id to our own (in case it was from a merge)
                    doc.node_id = node_id;
                    info!("Loaded document:");
                    doc.debug_dump("  LOADED");
                    return doc;
                } else {
                    warn!("Failed to decode NVS data!");
                }
            }
            Ok(None) => info!("No saved document, starting fresh"),
            Err(e) => warn!("NVS load error: {:?}", e),
        }
        info!("Creating fresh document for node {:08X}", node_id.as_u32());
        HiveDocument::new(node_id)
    }

    fn save(&mut self, doc: &HiveDocument) -> anyhow::Result<()> {
        let data = doc.encode();
        self.nvs.set_raw(NVS_KEY_COUNTER, &data)?;
        Ok(())
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

fn print_status(doc: &HiveDocument, connected: bool, status: &str) {
    let conn_sym = if connected { "●" } else { "○" };
    info!("========================================");
    info!("  HIVE-Lite BLE Sync Demo");
    info!("----------------------------------------");
    info!(
        "  Node: {:08X}  v{}  BLE:{}",
        doc.node_id.as_u32(),
        doc.version,
        conn_sym
    );
    info!("----------------------------------------");
    info!("  Taps: {}", doc.total_taps());
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
const BUILD_NUM: u32 = 17;

/// Draw initial static UI elements (call once at startup)
fn draw_static_ui<D>(display: &mut D, node_id: u32)
where
    D: DrawTarget<Color = Rgb565>,
{
    let _ = display.clear(Rgb565::BLACK);

    let white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let cyan = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);

    // Title with build number
    let title = format!("HIVE Alert/Ack  b{}", BUILD_NUM);
    let _ = Text::new(&title, Point::new(45, 25), white).draw(display);

    // Node ID (static)
    let node_str = format!("Node: {:08X}", node_id);
    let _ = Text::new(&node_str, Point::new(20, 60), cyan).draw(display);

    // Separators
    let _ = Rectangle::new(Point::new(10, 75), Size::new(300, 2))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_GRAY))
        .draw(display);
    let _ = Rectangle::new(Point::new(10, 200), Size::new(300, 2))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_GRAY))
        .draw(display);

    // Button labels at bottom (for the three touch buttons)
    let green = MonoTextStyle::new(&FONT_10X20, Rgb565::GREEN);
    let red = MonoTextStyle::new(&FONT_10X20, Rgb565::RED);
    let _ = Text::new("[ACK]", Point::new(25, 230), green).draw(display);
    let _ = Text::new("[RST]", Point::new(130, 230), cyan).draw(display);
    let _ = Text::new("[EMERG]", Point::new(225, 230), red).draw(display);
}


/// Update only the dynamic parts of the display (no flicker)
fn update_display<D>(
    display: &mut D,
    doc: &HiveDocument,
    alert_active: bool,
    battery_pct: Option<u8>,
    status: &str,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let black = PrimitiveStyle::with_fill(Rgb565::BLACK);
    let white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let yellow = MonoTextStyle::new(&FONT_10X20, Rgb565::YELLOW);

    // Connection indicator (top right) - show number of connections
    let num_conns = nimble::connection_count();
    let conn_color = if num_conns > 0 { Rgb565::GREEN } else { Rgb565::RED };
    let _ = Circle::new(Point::new(280, 10), 20)
        .into_styled(PrimitiveStyle::with_fill(conn_color))
        .draw(display);
    // Show connection count inside circle
    if num_conns > 0 {
        let conn_str = format!("{}", num_conns);
        let black_text = MonoTextStyle::new(&FONT_10X20, Rgb565::BLACK);
        let _ = Text::new(&conn_str, Point::new(285, 25), black_text).draw(display);
    }

    // Battery indicator (top left)
    let _ = Rectangle::new(Point::new(5, 5), Size::new(45, 25))
        .into_styled(black)
        .draw(display);
    if let Some(pct) = battery_pct {
        if pct > 0 {
            let batt_color = if pct > 20 { Rgb565::GREEN } else { Rgb565::RED };
            let batt_style = MonoTextStyle::new(&FONT_10X20, batt_color);
            let batt_str = format!("{}%", pct);
            let _ = Text::new(&batt_str, Point::new(10, 25), batt_style).draw(display);
        }
    }

    // Main alert area - large central display
    let _ = Rectangle::new(Point::new(20, 85), Size::new(280, 100))
        .into_styled(black)
        .draw(display);

    if alert_active {
        // ALERT MODE - big red background with EMERGENCY text
        let _ = Rectangle::new(Point::new(30, 90), Size::new(260, 80))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
            .draw(display);
        let white_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let _ = Text::new("!! EMERGENCY !!", Point::new(65, 125), white_style).draw(display);
        let _ = Text::new("TAP TO ACK", Point::new(90, 155), white_style).draw(display);
    } else {
        // NORMAL MODE - show tap count and ready status
        let green = MonoTextStyle::new(&FONT_10X20, Rgb565::GREEN);
        let _ = Text::new("READY", Point::new(120, 110), green).draw(display);

        // Show total taps (smaller, informational)
        let total_str = format!("taps: {}", doc.total_taps());
        let _ = Text::new(&total_str, Point::new(105, 140), white).draw(display);

        // Show node count
        let node_str = format!("peers: {}", doc.num_nodes().saturating_sub(1));
        let _ = Text::new(&node_str, Point::new(105, 165), white).draw(display);
    }

    // Status line (above button labels)
    let _ = Rectangle::new(Point::new(10, 185), Size::new(300, 20))
        .into_styled(black)
        .draw(display);
    let _ = Text::new(status, Point::new(20, 198), yellow).draw(display);
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

    // Initialize document store and load state
    info!("Initializing document store...");
    let mut store = DocumentStore::new(nvs_partition)?;
    let mut doc = store.load(node_id);
    info!("Loaded document: {} total taps", doc.total_taps());

    // Initialize BLE
    info!("Initializing BLE...");
    if let Err(e) = nimble::init(node_id) {
        error!("Failed to initialize BLE: {}", e);
        // Continue without BLE for testing
    }

    // Update BLE with initial document
    let encoded = doc.encode();
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
    update_display(&mut display, &doc, false, battery_pct, "BtnC=EMERG  BtnA=ACK");
    print_status(&doc, false, "BtnC=EMERG  BtnA=ACK");

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
        let now_ms = unsafe { esp_idf_svc::sys::esp_timer_get_time() as u32 / 1000 };

        // Check for connection state changes
        if nimble::take_connection_changed() {
            if connected {
                info!(">>> PEER CONNECTED!");
            } else {
                info!(">>> PEER DISCONNECTED");
            }
            needs_redraw = true;
        }

        // Handle vibration buzzing when alert is active
        if alert_active {
            let elapsed = now_ms.saturating_sub(last_vibration_toggle);
            if elapsed >= VIBRATION_INTERVAL_MS {
                vibration_on = !vibration_on;
                axp_set_vibration(&mut i2c, vibration_on);
                last_vibration_toggle = now_ms;
            }
        } else if vibration_on {
            // Turn off vibration if alert cleared
            vibration_on = false;
            axp_set_vibration(&mut i2c, false);
        }

        // Handle button releases (action on release)
        if button == Button::None && last_button != Button::None {
            match last_button {
                Button::BtnA => {
                    // Button A = ACK (also silences alert)
                    info!(">>> BUTTON A - SENDING ACK");
                    doc.send_ack();
                    alert_active = false;
                    axp_set_vibration(&mut i2c, false);
                    vibration_on = false;

                    // Save and gossip
                    if let Err(e) = store.save(&doc) {
                        error!("Failed to save: {:?}", e);
                    }
                    let encoded = doc.encode();
                    let sent = nimble::gossip_document(&encoded);
                    info!(">>> Gossiped ACK to {} peers", sent);

                    needs_redraw = true;
                    update_display(&mut display, &doc, false, battery_pct, "ACK sent!");
                }
                Button::BtnB => {
                    // Button B = RESET
                    info!(">>> BUTTON B - RESETTING!");
                    doc = HiveDocument::new(node_id);
                    alert_active = false;
                    axp_set_vibration(&mut i2c, false);
                    vibration_on = false;

                    if let Err(e) = store.save(&doc) {
                        error!("Failed to save reset: {:?}", e);
                    }
                    let encoded = doc.encode();
                    nimble::set_document(&encoded);
                    needs_redraw = true;
                    update_display(&mut display, &doc, false, battery_pct, "RESET!");
                }
                Button::BtnC => {
                    // Button C = EMERGENCY
                    info!(">>> BUTTON C - SENDING EMERGENCY!");
                    doc.send_emergency();

                    // Enter alert mode locally too (buzz until ACK'd)
                    alert_active = true;
                    last_vibration_toggle = now_ms;
                    vibration_on = true;
                    axp_set_vibration(&mut i2c, true);

                    // Save and gossip
                    if let Err(e) = store.save(&doc) {
                        error!("Failed to save: {:?}", e);
                    }
                    let encoded = doc.encode();
                    let sent = nimble::gossip_document(&encoded);
                    info!(">>> Gossiped EMERGENCY to {} peers", sent);

                    needs_redraw = true;
                    update_display(&mut display, &doc, true, battery_pct, "EMERGENCY!");
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
            if let Some(peer_doc) = HiveDocument::decode(&data) {
                info!("Decoded peer doc from node {:08X}:", peer_doc.node_id.as_u32());
                peer_doc.debug_dump("  PEER");

                // Check if peer is sending EMERGENCY
                let peer_event = peer_doc.current_event();
                if peer_event == EventType::Emergency && !alert_active {
                    info!(">>> RECEIVED EMERGENCY FROM PEER!");
                    alert_active = true;
                    last_vibration_toggle = now_ms;
                    vibration_on = true;
                    axp_set_vibration(&mut i2c, true);
                    needs_redraw = true;
                }

                if doc.merge(&peer_doc) {
                    info!(">>> MERGED! New total: {}", doc.total_taps());

                    // Save merged state
                    if let Err(e) = store.save(&doc) {
                        error!("Failed to save merged doc: {:?}", e);
                    }

                    // GOSSIP: Forward merged state to ALL other peers (multi-hop!)
                    let encoded = doc.encode();
                    let sent = nimble::gossip_document(&encoded);
                    info!(">>> Forwarded merged doc to {} peers (multi-hop)", sent);

                    needs_redraw = true;
                    print_status(&doc, connected, "Merged & forwarded!");
                } else {
                    info!("No changes from merge (peer had same or less data)");
                }
            } else {
                warn!("Failed to decode peer document ({} bytes)", data.len());
            }
        }

        // Redraw display when needed
        if needs_redraw {
            let status = if alert_active {
                "!! ALERT - TAP TO ACK !!"
            } else if connected {
                "Connected"
            } else {
                "Advertising..."
            };
            update_display(&mut display, &doc, alert_active, battery_pct, status);
            needs_redraw = false;
        }

        // Check if we should rotate to find other peers (mesh behavior)
        nimble::check_rotation();

        // Periodic status update (every 5 seconds = 100 * 50ms)
        if loop_count % 100 == 0 {
            // Update battery reading and peripheral health
            if let Some(mv) = axp_read_battery_voltage(&mut i2c) {
                let pct = battery_percent_from_voltage(mv);
                battery_pct = Some(pct);
                doc.update_health(pct);
            }

            let status = if alert_active {
                "!! ALERT - TAP TO ACK !!"
            } else if connected {
                "Connected"
            } else {
                "Advertising..."
            };
            update_display(&mut display, &doc, alert_active, battery_pct, status);
            print_status(&doc, connected, status);
        }

        FreeRtos::delay_ms(50);
        loop_count = loop_count.wrapping_add(1);
    }
}
