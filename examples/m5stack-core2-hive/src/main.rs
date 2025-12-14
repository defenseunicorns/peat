//! HIVE-Lite Counter Demo for M5Stack Core2
//!
//! Demonstrates CRDT document sync between two nodes:
//! - Tap screen to increment your counter
//! - Counter persists to NVS (survives power off)
//! - When peer connects, full state exchange and merge
//! - Eventually consistent: both nodes converge to same state
//!
//! ## Automerge-style Semantics
//!
//! Each node maintains a GCounter CRDT document:
//! - Node tracks its own taps independently
//! - Merge = take max of each node's count
//! - Offline changes are preserved and sync later
//!
//! Example:
//! ```text
//! Node A: {A: 5, B: 3} = 8 total
//! Node B offline, taps 4 times: {A: 2, B: 7}
//! After sync: both have {A: 5, B: 7} = 12 total
//! ```
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
use log::{info, warn, error, debug};

use hive_btle::discovery::HiveBeacon;
use hive_btle::sync::GCounter;
use hive_btle::{HierarchyLevel, NodeId};

// NVS namespace for storing CRDT state
const NVS_NAMESPACE: &str = "hive";
const NVS_KEY_COUNTER: &str = "counter";

// FT6336U Touch controller
const FT6336U_ADDR: u8 = 0x38;
const FT6336U_REG_STATUS: u8 = 0x02;
const FT6336U_REG_TOUCH1_XH: u8 = 0x03;

/// CRDT Document - holds all synced state
///
/// This is our "Automerge document" - a collection of CRDTs
/// that can be synced as a unit.
struct HiveDocument {
    /// The tap counter (G-Counter CRDT)
    pub counter: GCounter,
    /// Our node ID
    node_id: NodeId,
    /// Document version (increments on any local change)
    version: u64,
}

impl HiveDocument {
    /// Create new empty document
    fn new(node_id: NodeId) -> Self {
        Self {
            counter: GCounter::new(),
            node_id,
            version: 0,
        }
    }

    /// Increment our tap counter
    fn tap(&mut self) {
        self.counter.increment(&self.node_id, 1);
        self.version += 1;
    }

    /// Get our tap count
    fn our_taps(&self) -> u64 {
        self.counter.node_count(&self.node_id)
    }

    /// Get peer tap count (everyone else)
    fn peer_taps(&self) -> u64 {
        self.counter.value() - self.our_taps()
    }

    /// Get total taps
    fn total_taps(&self) -> u64 {
        self.counter.value()
    }

    /// Merge with another document (Automerge-style)
    ///
    /// After merge, both documents will have the same state
    /// (max of each node's contribution).
    fn merge(&mut self, other: &HiveDocument) {
        self.counter.merge(&other.counter);
        self.version += 1;
    }

    /// Encode document for sync/storage
    fn encode(&self) -> Vec<u8> {
        // Format: [version: 8 bytes] [counter_data]
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.counter.encode());
        buf
    }

    /// Decode document from sync/storage
    fn decode(data: &[u8], node_id: NodeId) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let version = u64::from_le_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);
        let counter = GCounter::decode(&data[8..])?;
        Some(Self {
            counter,
            node_id,
            version,
        })
    }
}

/// Persistent storage for CRDT document
struct DocumentStore<'a> {
    nvs: EspNvs<NvsDefault>,
    node_id: NodeId,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> DocumentStore<'a> {
    /// Open or create the document store
    fn new(nvs_partition: EspDefaultNvsPartition, node_id: NodeId) -> anyhow::Result<Self> {
        let nvs = EspNvs::new(nvs_partition, NVS_NAMESPACE, true)?;
        Ok(Self {
            nvs,
            node_id,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Load document from NVS (or create new if not found)
    fn load(&self) -> HiveDocument {
        let mut buf = [0u8; 256];
        match self.nvs.get_raw(NVS_KEY_COUNTER, &mut buf) {
            Ok(Some(data)) => {
                if let Some(doc) = HiveDocument::decode(data, self.node_id) {
                    info!("Loaded document from NVS: {} taps, version {}",
                          doc.total_taps(), doc.version);
                    return doc;
                }
            }
            Ok(None) => {
                info!("No saved document, starting fresh");
            }
            Err(e) => {
                warn!("Failed to load from NVS: {:?}, starting fresh", e);
            }
        }
        HiveDocument::new(self.node_id)
    }

    /// Save document to NVS
    fn save(&mut self, doc: &HiveDocument) -> anyhow::Result<()> {
        let data = doc.encode();
        self.nvs.set_raw(NVS_KEY_COUNTER, &data)?;
        debug!("Saved document to NVS: {} bytes", data.len());
        Ok(())
    }
}

/// Touch detection
#[derive(Debug, Clone, Copy, PartialEq)]
enum TouchState {
    None,
    Touched,
}

fn read_touch(i2c: &mut I2cDriver) -> TouchState {
    let mut buf = [0u8; 1];
    if i2c.write_read(FT6336U_ADDR, &[FT6336U_REG_STATUS], &mut buf).is_err() {
        return TouchState::None;
    }
    let num_touches = buf[0] & 0x0F;
    if num_touches > 0 {
        TouchState::Touched
    } else {
        TouchState::None
    }
}

/// Get unique Node ID from ESP32 MAC address
fn get_node_id_from_mac() -> NodeId {
    let mut mac = [0u8; 6];
    unsafe {
        esp_idf_svc::sys::esp_efuse_mac_get_default(mac.as_mut_ptr());
    }
    info!("MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
          mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    NodeId::new(u32::from_be_bytes([mac[2], mac[3], mac[4], mac[5]]))
}

/// Display state to serial (LCD driver to come)
fn update_display(doc: &HiveDocument, peer_id: Option<NodeId>, connected: bool, status: &str) {
    info!("┌────────────────────────────────┐");
    info!("│  HIVE-Lite Document Sync       │");
    info!("├────────────────────────────────┤");
    info!("│  My ID: {:08X}  v{}          │", doc.node_id.as_u32(), doc.version);
    if let Some(peer) = peer_id {
        info!("│  Peer:  {:08X} {}           │", peer.as_u32(),
              if connected { "●" } else { "○" });
    } else {
        info!("│  Peer:  (scanning...)          │");
    }
    info!("├────────────────────────────────┤");
    info!("│  My taps:   {:>4}               │", doc.our_taps());
    info!("│  Peer taps: {:>4}               │", doc.peer_taps());
    info!("│  ──────────────                │");
    info!("│  TOTAL:     {:>4}               │", doc.total_taps());
    info!("├────────────────────────────────┤");
    info!("│  TAP SCREEN = +1 (persisted)   │");
    info!("│  Status: {:<20} │", status);
    info!("└────────────────────────────────┘");
}

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("=========================================");
    info!("  HIVE-Lite Document Sync Demo");
    info!("=========================================");

    // Take peripherals
    let peripherals = Peripherals::take()?;
    let _sys_loop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;

    // Get unique Node ID from MAC
    let node_id = get_node_id_from_mac();
    info!("Node ID: {:08X}", node_id.as_u32());

    // Initialize persistent document store
    let mut store = DocumentStore::new(nvs_partition, node_id)?;

    // Load document from NVS (or create new)
    let mut doc = store.load();
    info!("Document loaded: {} total taps", doc.total_taps());

    // Initialize I2C for touch controller
    let i2c_config = I2cConfig::new().baudrate(400.kHz().into());
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio21,
        peripherals.pins.gpio22,
        &i2c_config,
    )?;
    info!("Touch controller initialized");

    // Create HIVE beacon
    let mut beacon = HiveBeacon::new(node_id);
    beacon.set_hierarchy_level(HierarchyLevel::Leaf);
    beacon.set_battery_level(100);
    info!("HIVE beacon ready: {} bytes", beacon.encode_compact().len());

    // TODO: Initialize NimBLE
    // - Advertise with beacon + document hash in service data
    // - Scan for peer HIVE beacons
    // - On connect: exchange full document, merge, save

    // Initial display
    let mut status = "Tap screen!";
    update_display(&doc, None, false, status);

    // Main loop
    let mut last_touch = TouchState::None;
    let mut loop_count: u32 = 0;

    loop {
        // Check for touch
        let touch = read_touch(&mut i2c);

        // Detect rising edge (new touch)
        if touch == TouchState::Touched && last_touch == TouchState::None {
            info!(">>> TAP! Incrementing and saving...");

            // Increment counter
            doc.tap();

            // Persist to NVS immediately
            if let Err(e) = store.save(&doc) {
                error!("Failed to save: {:?}", e);
                status = "Save failed!";
            } else {
                status = "Saved!";
            }

            update_display(&doc, None, false, status);

            // TODO: If connected to peer, trigger sync
        }
        last_touch = touch;

        // Periodic status update
        if loop_count % 50 == 0 {
            status = "Tap screen!";
            update_display(&doc, None, false, status);
        }

        // TODO: BLE sync loop
        // - Check for new peer connections
        // - Exchange document state
        // - Merge and save

        FreeRtos::delay_ms(100);
        loop_count += 1;
    }
}
