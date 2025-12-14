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

use hive_btle::sync::GCounter;
use hive_btle::NodeId;

// NVS storage
const NVS_NAMESPACE: &str = "hive";
const NVS_KEY_COUNTER: &str = "counter";

// FT6336U Touch controller on M5Stack Core2
const FT6336U_ADDR: u8 = 0x38;
const FT6336U_REG_STATUS: u8 = 0x02;

/// CRDT Document with persistence
struct HiveDocument {
    pub counter: GCounter,
    node_id: NodeId,
    version: u32,
}

impl HiveDocument {
    fn new(node_id: NodeId) -> Self {
        Self {
            counter: GCounter::new(),
            node_id,
            version: 0,
        }
    }

    fn tap(&mut self) {
        self.counter.increment(&self.node_id, 1);
        self.version += 1;
    }

    fn our_taps(&self) -> u64 {
        self.counter.node_count(&self.node_id)
    }

    fn peer_taps(&self) -> u64 {
        self.counter.value().saturating_sub(self.our_taps())
    }

    fn total_taps(&self) -> u64 {
        self.counter.value()
    }

    /// Merge another document into this one
    fn merge(&mut self, other: &HiveDocument) -> bool {
        let old_value = self.counter.value();
        self.counter.merge(&other.counter);
        let changed = self.counter.value() != old_value;
        if changed {
            self.version += 1;
        }
        changed
    }

    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.node_id.as_u32().to_le_bytes());
        buf.extend_from_slice(&self.counter.encode());
        buf
    }

    fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let node_id = NodeId::new(u32::from_le_bytes([data[4], data[5], data[6], data[7]]));
        let counter = GCounter::decode(&data[8..])?;
        Some(Self {
            counter,
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
                if let Some(mut doc) = HiveDocument::decode(data) {
                    // Update node_id to our own (in case it was from a merge)
                    doc.node_id = node_id;
                    info!("Loaded: {} taps, v{}", doc.total_taps(), doc.version);
                    return doc;
                }
            }
            Ok(None) => info!("No saved document, starting fresh"),
            Err(e) => warn!("NVS load error: {:?}", e),
        }
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
    // Don't modify AXP192 - factory bootloader configures it correctly
    // Modifying registers breaks the display
    let mut buf = [0u8; 1];
    if i2c.write_read(AXP192_ADDR, &[0x03], &mut buf, 100).is_ok() {
        info!("AXP192: OK (status=0x{:02X})", buf[0]);
    }
    Ok(())
}

fn draw_display<D>(display: &mut D, doc: &HiveDocument, connected: bool, status: &str)
where
    D: DrawTarget<Color = Rgb565>,
{
    let _ = display.clear(Rgb565::new(0, 0, 8)); // Dark blue background

    let white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let cyan = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);

    // Title
    let _ = Text::new("HIVE-Lite BLE Sync", Point::new(50, 25), white).draw(display);

    // Connection indicator
    let conn_color = if connected { Rgb565::GREEN } else { Rgb565::RED };
    let _ = Circle::new(Point::new(280, 10), 20)
        .into_styled(PrimitiveStyle::with_fill(conn_color))
        .draw(display);

    // Node ID
    let node_str = format!("Node: {:08X}", doc.node_id.as_u32());
    let _ = Text::new(&node_str, Point::new(20, 60), cyan).draw(display);

    // Separator
    let _ = Rectangle::new(Point::new(10, 75), Size::new(300, 2))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_GRAY))
        .draw(display);

    // Total taps (large, centered)
    let total_str = format!("{}", doc.total_taps());
    let _ = Text::new(&total_str, Point::new(120, 145), white).draw(display);
    let _ = Text::new("taps", Point::new(115, 175), cyan).draw(display);

    // Status
    let _ = Rectangle::new(Point::new(10, 200), Size::new(300, 2))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_GRAY))
        .draw(display);
    let _ = Text::new(status, Point::new(20, 230), cyan).draw(display);
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
    info!("Node ID: {:08X}", node_id.as_u32());

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
        .orientation(Orientation::new().rotate(mipidsi::options::Rotation::Deg180))
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

    // Draw initial status
    draw_display(&mut display, &doc, false, "Tap screen to increment!");
    print_status(&doc, false, "Tap screen to increment!");

    // Main loop
    let mut last_touch = Touch::None;
    let mut loop_count: u32 = 0;
    let mut needs_redraw = false;

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

        // Handle tap (rising edge only)
        if touch == Touch::Touched && last_touch == Touch::None {
            info!("");
            info!(">>> TAP DETECTED!");

            doc.tap();

            // Save to NVS
            if let Err(e) = store.save(&doc) {
                error!("Failed to save: {:?}", e);
            }

            // Update BLE document
            let encoded = doc.encode();
            nimble::set_document(&encoded);

            // Notify peer if connected
            if connected {
                if let Err(e) = nimble::notify_document(&encoded) {
                    warn!("Failed to notify peer: {}", e);
                }
            }

            needs_redraw = true;
            print_status(&doc, connected, "Tap saved!");
        }
        last_touch = touch;

        // Handle pending document from BLE
        if let Some(data) = nimble::take_pending_document() {
            info!(">>> Received {} bytes from peer", data.len());
            if let Some(peer_doc) = HiveDocument::decode(&data) {
                info!("Decoded peer document: {} taps from node {:08X}", peer_doc.total_taps(), peer_doc.node_id.as_u32());
                if doc.merge(&peer_doc) {
                    info!("Merged! New total: {}", doc.total_taps());

                    // Save merged state
                    if let Err(e) = store.save(&doc) {
                        error!("Failed to save merged doc: {:?}", e);
                    }

                    // Update BLE document
                    let encoded = doc.encode();
                    nimble::set_document(&encoded);

                    needs_redraw = true;
                    print_status(&doc, connected, "Merged from peer!");
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
            draw_display(&mut display, &doc, connected, status);
            needs_redraw = false;
        }

        // Periodic status update (every 5 seconds)
        if loop_count % 100 == 0 {
            let status = if connected {
                "Connected - tap to sync!"
            } else {
                "Advertising..."
            };
            draw_display(&mut display, &doc, connected, status);
            print_status(&doc, connected, status);
        }

        FreeRtos::delay_ms(50);
        loop_count = loop_count.wrapping_add(1);
    }
}
