//! HIVE-Lite BLE Document Sync Demo for M5Stack Core2
//!
//! Two-device CRDT sync over BLE:
//! - Tap screen to increment counter
//! - State persists to NVS (survives power off)
//! - Devices discover each other via BLE
//! - Documents merge automatically (eventually consistent)
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

/// Event types in order for cycling through with taps
const EVENT_CYCLE: [EventType; 7] = [
    EventType::None,
    EventType::Ping,
    EventType::Moving,
    EventType::InPosition,
    EventType::Ack,
    EventType::NeedAssist,
    EventType::Emergency,
];

/// CRDT Document with persistence and Peripheral state
struct HiveDocument {
    pub counter: GCounter,
    pub peripheral: Peripheral,
    node_id: NodeId,
    version: u32,
    event_index: usize,
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
            event_index: 0,
        }
    }

    fn tap(&mut self) {
        info!(">>> TAP: incrementing node {:08X}", self.node_id.as_u32());
        self.debug_dump("BEFORE TAP");
        self.counter.increment(&self.node_id, 1);
        self.version += 1;

        // Cycle to next event type
        self.event_index = (self.event_index + 1) % EVENT_CYCLE.len();
        let event_type = EVENT_CYCLE[self.event_index];
        let timestamp = unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 / 1000 };

        if event_type == EventType::None {
            self.peripheral.clear_event();
            info!(">>> Event: CLEARED");
        } else {
            self.peripheral.set_event(event_type, timestamp);
            info!(">>> Event: {}", event_type.label());
        }
        self.peripheral.timestamp = timestamp;

        self.debug_dump("AFTER TAP");
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

        // NEW: Peripheral data after counter (Build 12+)
        // Marker byte 0xAB to indicate extended format
        buf.push(0xAB);
        buf.push(self.event_index as u8);
        let peripheral_data = self.peripheral.encode();
        buf.extend_from_slice(&(peripheral_data.len() as u16).to_le_bytes());
        buf.extend_from_slice(&peripheral_data);

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
        let (event_index, peripheral) = if data.len() > counter_end && data[counter_end] == 0xAB {
            let event_index = data[counter_end + 1] as usize % EVENT_CYCLE.len();
            let peripheral_len =
                u16::from_le_bytes([data[counter_end + 2], data[counter_end + 3]]) as usize;
            let peripheral_start = counter_end + 4;
            if data.len() >= peripheral_start + peripheral_len {
                let peripheral = Peripheral::decode(&data[peripheral_start..peripheral_start + peripheral_len]);
                (event_index, peripheral)
            } else {
                (0, None)
            }
        } else {
            // Old format (Build 11) - no peripheral data
            (0, None)
        };

        // Create peripheral if not decoded from message
        let peripheral = peripheral.unwrap_or_else(|| {
            Peripheral::new(node_id.as_u32(), PeripheralType::SoldierSensor)
        });

        Some(Self {
            counter,
            peripheral,
            node_id,
            version,
            event_index,
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

/// Touch state
#[derive(PartialEq, Clone, Copy)]
enum Touch {
    None,
    Touched,
}

fn read_touch(i2c: &mut I2cDriver) -> Touch {
    let mut buf = [0u8; 1];
    if i2c
        .write_read(FT6336U_ADDR, &[FT6336U_REG_STATUS], &mut buf, 100)
        .is_ok()
    {
        if buf[0] & 0x0F > 0 {
            return Touch::Touched;
        }
    }
    Touch::None
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

/// AXP192 power management IC (controls backlight and power)
const AXP192_ADDR: u8 = 0x34;

fn axp192_init(i2c: &mut I2cDriver) -> anyhow::Result<()> {
    let mut buf = [0u8; 1];

    // Read current ADC config
    if i2c.write_read(AXP192_ADDR, &[0x82], &mut buf, 100).is_ok() {
        info!("AXP192: ADC config=0x{:02X} (need bit7 set for battery)", buf[0]);

        // Enable battery voltage ADC (bit 7) if not already set
        if (buf[0] & 0x80) == 0 {
            let new_val = buf[0] | 0x80; // Set only bit 7, preserve others
            info!("AXP192: Enabling battery ADC: 0x{:02X} -> 0x{:02X}", buf[0], new_val);
            // Write register address and value
            let write_result = i2c.write(AXP192_ADDR, &[0x82, new_val], 1000);
            match write_result {
                Ok(_) => {
                    info!("AXP192: Write succeeded");
                    FreeRtos::delay_ms(10); // Small delay after write
                }
                Err(e) => {
                    warn!("AXP192: Write failed: {:?}", e);
                }
            }
            // Wait for ADC to stabilize
            FreeRtos::delay_ms(100);
            // Verify it took effect
            if i2c.write_read(AXP192_ADDR, &[0x82], &mut buf, 100).is_ok() {
                info!("AXP192: ADC config after enable=0x{:02X}", buf[0]);
                if (buf[0] & 0x80) == 0 {
                    warn!("AXP192: Battery ADC enable did NOT take effect!");
                }
            }
        }
    }

    if i2c.write_read(AXP192_ADDR, &[0x00], &mut buf, 100).is_ok() {
        info!("AXP192: Power status=0x{:02X}", buf[0]);
    }
    Ok(())
}

/// Read battery voltage from AXP192 (returns millivolts)
fn axp192_read_battery_voltage(i2c: &mut I2cDriver) -> Option<u16> {
    let mut buf = [0u8; 2];
    // Register 0x78-0x79: Battery voltage ADC (12-bit, 1.1mV/step)
    // High 8 bits in 0x78, low 4 bits in upper nibble of 0x79
    if i2c.write_read(AXP192_ADDR, &[0x78], &mut buf, 100).is_ok() {
        let raw = ((buf[0] as u16) << 4) | ((buf[1] as u16) >> 4);
        let mv = (raw as u32 * 1100 / 1000) as u16; // 1.1mV per step
        info!("Battery ADC: raw=0x{:03X} ({}) => {}mV [0x78=0x{:02X}, 0x79=0x{:02X}]",
              raw, raw, mv, buf[0], buf[1]);
        Some(mv)
    } else {
        None
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
fn axp192_on_battery(i2c: &mut I2cDriver) -> bool {
    let mut buf = [0u8; 1];
    if i2c.write_read(AXP192_ADDR, &[0x00], &mut buf, 100).is_ok() {
        // Bit 7: ACIN exists, Bit 5: VBUS exists
        let acin = (buf[0] & 0x80) != 0;
        let vbus = (buf[0] & 0x20) != 0;
        !acin && !vbus
    } else {
        false
    }
}

/// Build number for tracking firmware versions
const BUILD_NUM: u32 = 13;

/// Draw initial static UI elements (call once at startup)
fn draw_static_ui<D>(display: &mut D, node_id: u32)
where
    D: DrawTarget<Color = Rgb565>,
{
    let _ = display.clear(Rgb565::BLACK);

    let white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let cyan = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);

    // Title with build number
    let title = format!("HIVE-Lite BLE Sync  b{}", BUILD_NUM);
    let _ = Text::new(&title, Point::new(30, 25), white).draw(display);

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

    // Static label
    let _ = Text::new("taps / event", Point::new(95, 85), cyan).draw(display);
}

/// Compute a simple hash for visual state comparison
fn state_hash(doc: &HiveDocument) -> u16 {
    let encoded = doc.encode();
    let mut hash: u16 = 0;
    for byte in &encoded {
        hash = hash.wrapping_add(*byte as u16);
        hash = hash.wrapping_mul(31);
    }
    hash
}

/// Get color for event type
fn event_color(event: EventType) -> Rgb565 {
    match event {
        EventType::None => Rgb565::CSS_GRAY,
        EventType::Ping => Rgb565::GREEN,
        EventType::Moving => Rgb565::CYAN,
        EventType::InPosition => Rgb565::BLUE,
        EventType::Ack => Rgb565::WHITE,
        EventType::NeedAssist => Rgb565::YELLOW,
        EventType::Emergency => Rgb565::RED,
    }
}

/// Update only the dynamic parts of the display (no flicker)
fn update_display<D>(
    display: &mut D,
    doc: &HiveDocument,
    connected: bool,
    battery_pct: Option<u8>,
    status: &str,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let black = PrimitiveStyle::with_fill(Rgb565::BLACK);
    let white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let green = MonoTextStyle::new(&FONT_10X20, Rgb565::GREEN);
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

    // Battery indicator (top left) - clear area then draw
    // Hide if 0% (ADC not enabled on some modules)
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

    // Tap count - clear area then draw large total
    let _ = Rectangle::new(Point::new(30, 90), Size::new(260, 55))
        .into_styled(black)
        .draw(display);

    // Large total count - this should be identical on all devices after sync!
    let total_str = format!("{}", doc.total_taps());
    let _ = Text::new(&total_str, Point::new(130, 130), green).draw(display);

    // Event type indicator - show current event with appropriate color
    let _ = Rectangle::new(Point::new(30, 145), Size::new(260, 30))
        .into_styled(black)
        .draw(display);

    let current_event = doc.current_event();
    let event_label = if current_event == EventType::None {
        "tap to send".to_string()
    } else {
        current_event.label().to_string()
    };
    let event_style = MonoTextStyle::new(&FONT_10X20, event_color(current_event));
    // Center the event label
    let x_offset = 160 - (event_label.len() as i32 * 5);
    let _ = Text::new(&event_label, Point::new(x_offset, 165), event_style).draw(display);

    // Show state hash and node count for visual convergence check
    let _ = Rectangle::new(Point::new(60, 175), Size::new(200, 25))
        .into_styled(black)
        .draw(display);
    let hash = state_hash(doc);
    let hash_str = format!("{:04X} v{} n{}", hash, doc.version, doc.num_nodes());
    let _ = Text::new(&hash_str, Point::new(80, 190), white).draw(display);

    // Status - clear area then draw
    let _ = Rectangle::new(Point::new(10, 210), Size::new(300, 25))
        .into_styled(black)
        .draw(display);
    let _ = Text::new(status, Point::new(20, 230), yellow).draw(display);
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

    // Initialize I2C for touch controller and AXP192
    info!("Initializing I2C...");
    let i2c_config = I2cConfig::new().baudrate(400.kHz().into());
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio21, // SDA
        peripherals.pins.gpio22, // SCL
        &i2c_config,
    )?;
    info!("I2C initialized");

    // Initialize AXP192 (enable LCD power/backlight)
    info!("Initializing AXP192...");
    if let Err(e) = axp192_init(&mut i2c) {
        warn!("AXP192 init failed: {:?}", e);
    }

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
    let battery_mv = axp192_read_battery_voltage(&mut i2c);
    let mut battery_pct = battery_mv.map(battery_percent_from_voltage);
    if let Some(mv) = battery_mv {
        info!("Battery: {}mV ({}%)", mv, battery_pct.unwrap_or(0));
    }

    // Draw static UI once, then update dynamic parts
    draw_static_ui(&mut display, node_id.as_u32());
    update_display(&mut display, &doc, false, battery_pct, "Tap screen to increment!");
    print_status(&doc, false, "Tap screen to increment!");

    // Main loop
    let mut last_touch = Touch::None;
    let mut loop_count: u32 = 0;
    let mut needs_redraw = false;
    let mut touch_start_time: u32 = 0;  // For long-press detection
    const LONG_PRESS_MS: u32 = 3000;  // 3 seconds to reset

    loop {
        let touch = read_touch(&mut i2c);
        let connected = nimble::is_connected();

        // Check for connection state changes
        if nimble::take_connection_changed() {
            if connected {
                info!(">>> PEER CONNECTED!");
            } else {
                info!(">>> PEER DISCONNECTED");
            }
            needs_redraw = true;
        }

        // Handle touch - detect tap vs long-press
        let now_ms = unsafe { esp_idf_svc::sys::esp_timer_get_time() as u32 / 1000 };

        if touch == Touch::Touched && last_touch == Touch::None {
            // Touch started
            touch_start_time = now_ms;
        } else if touch == Touch::Touched && last_touch == Touch::Touched {
            // Still touching - check for long press
            let held_ms = now_ms.saturating_sub(touch_start_time);
            if held_ms >= LONG_PRESS_MS && touch_start_time != 0 {
                // Long press detected - RESET counter
                info!(">>> LONG PRESS - RESETTING COUNTER!");
                doc = HiveDocument::new(node_id);
                if let Err(e) = store.save(&doc) {
                    error!("Failed to save reset: {:?}", e);
                }
                let encoded = doc.encode();
                nimble::set_document(&encoded);
                touch_start_time = 0; // Prevent repeated resets
                needs_redraw = true;
                update_display(&mut display, &doc, connected, battery_pct, "RESET!");
            }
        } else if touch == Touch::None && last_touch == Touch::Touched {
            // Touch released - check if it was a tap (short press)
            let held_ms = now_ms.saturating_sub(touch_start_time);
            if held_ms < LONG_PRESS_MS && touch_start_time != 0 {
                // Short tap - increment
                doc.tap();

                // Save to NVS
                if let Err(e) = store.save(&doc) {
                    error!("Failed to save: {:?}", e);
                }

                // Gossip to ALL connected peers (multi-hop mesh)
                let encoded = doc.encode();
                let sent = nimble::gossip_document(&encoded);
                info!(">>> Gossiped tap to {} peers", sent);

                needs_redraw = true;
                print_status(&doc, connected, "Tap saved!");
            }
        }
        last_touch = touch;

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
            let status = if connected { "Connected!" } else { "Advertising..." };
            update_display(&mut display, &doc, connected, battery_pct, status);
            needs_redraw = false;
        }

        // Check if we should rotate to find other peers (mesh behavior)
        nimble::check_rotation();

        // Periodic status update (every 5 seconds = 100 * 50ms)
        if loop_count % 100 == 0 {
            // Update battery reading and peripheral health
            if let Some(mv) = axp192_read_battery_voltage(&mut i2c) {
                let pct = battery_percent_from_voltage(mv);
                battery_pct = Some(pct);
                doc.update_health(pct);
            }

            let status = if connected {
                "Connected - tap to sync!"
            } else {
                "Advertising..."
            };
            update_display(&mut display, &doc, connected, battery_pct, status);
            print_status(&doc, connected, status);
        }

        FreeRtos::delay_ms(50);
        loop_count = loop_count.wrapping_add(1);
    }
}
