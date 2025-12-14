//! HIVE-Lite CRDT Demo for M5Stack Core2
//!
//! Simple demonstration of CRDT persistence on ESP32:
//! - Tap screen to increment counter
//! - State persists to NVS (survives power off)
//! - Counter displays on serial output
//!
//! BLE sync will be added in a future iteration once we have
//! proper NimBLE bindings working.
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
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use log::{info, warn, error};

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

    fn total_taps(&self) -> u64 {
        self.counter.value()
    }

    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.counter.encode());
        buf
    }

    fn decode(data: &[u8], node_id: NodeId) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let counter = GCounter::decode(&data[4..])?;
        Some(Self { counter, node_id, version })
    }
}

/// Document store with NVS persistence
struct DocumentStore {
    nvs: EspNvs<NvsDefault>,
    node_id: NodeId,
}

impl DocumentStore {
    fn new(nvs_partition: EspDefaultNvsPartition, node_id: NodeId) -> anyhow::Result<Self> {
        let nvs = EspNvs::new(nvs_partition, NVS_NAMESPACE, true)?;
        Ok(Self { nvs, node_id })
    }

    fn load(&self) -> HiveDocument {
        let mut buf = [0u8; 256];
        match self.nvs.get_raw(NVS_KEY_COUNTER, &mut buf) {
            Ok(Some(data)) => {
                if let Some(doc) = HiveDocument::decode(data, self.node_id) {
                    info!("Loaded: {} taps, v{}", doc.total_taps(), doc.version);
                    return doc;
                }
            }
            Ok(None) => info!("No saved document, starting fresh"),
            Err(e) => warn!("NVS load error: {:?}", e),
        }
        HiveDocument::new(self.node_id)
    }

    fn save(&mut self, doc: &HiveDocument) -> anyhow::Result<()> {
        let data = doc.encode();
        self.nvs.set_raw(NVS_KEY_COUNTER, &data)?;
        info!("Saved: {} taps, v{}", doc.total_taps(), doc.version);
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
    if i2c.write_read(FT6336U_ADDR, &[FT6336U_REG_STATUS], &mut buf, 100).is_ok() {
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

fn print_status(doc: &HiveDocument, status: &str) {
    info!("========================================");
    info!("  HIVE-Lite CRDT Demo");
    info!("----------------------------------------");
    info!("  Node ID: {:08X}", doc.node_id.as_u32());
    info!("  Version: {}", doc.version);
    info!("----------------------------------------");
    info!("  My taps:    {:>6}", doc.our_taps());
    info!("  Total taps: {:>6}", doc.total_taps());
    info!("----------------------------------------");
    info!("  {}", status);
    info!("========================================");
}

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("");
    info!("=========================================");
    info!("  HIVE-Lite CRDT Demo - M5Stack Core2");
    info!("=========================================");
    info!("");

    // Take peripherals
    let peripherals = Peripherals::take()?;
    let _sys_loop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;

    // Get node ID from MAC address
    let node_id = get_node_id_from_mac();
    info!("Node ID: {:08X}", node_id.as_u32());

    // Initialize document store and load state
    let mut store = DocumentStore::new(nvs_partition, node_id)?;
    let mut doc = store.load();
    info!("Loaded document: {} total taps", doc.total_taps());

    // Initialize I2C for touch controller
    let i2c_config = I2cConfig::new().baudrate(400.kHz().into());
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio21, // SDA
        peripherals.pins.gpio22, // SCL
        &i2c_config,
    )?;
    info!("Touch controller initialized");

    // Print initial status
    print_status(&doc, "Tap screen to increment!");

    // Main loop
    let mut last_touch = Touch::None;
    let mut loop_count: u32 = 0;

    loop {
        let touch = read_touch(&mut i2c);

        // Handle tap (rising edge only)
        if touch == Touch::Touched && last_touch == Touch::None {
            info!("");
            info!(">>> TAP DETECTED!");
            doc.tap();

            // Save to NVS
            if let Err(e) = store.save(&doc) {
                error!("Failed to save: {:?}", e);
            }

            print_status(&doc, "Tap saved to NVS!");
        }
        last_touch = touch;

        // Periodic status update
        if loop_count % 100 == 0 {
            print_status(&doc, "Waiting for tap...");
        }

        FreeRtos::delay_ms(50);
        loop_count = loop_count.wrapping_add(1);
    }
}
