//! ESP32 BLE Adapter using NimBLE
//!
//! Provides BLE functionality for ESP32 devices using ESP-IDF NimBLE.
//! Tested on M5Stack Core2 (ESP32-D0WDQ6-V3).
//!
//! ## Prerequisites
//!
//! 1. Install ESP-IDF toolchain and Rust esp fork
//! 2. Enable BLE in ESP-IDF menuconfig:
//!    - Component config → Bluetooth → Enable
//!    - Component config → Bluetooth → NimBLE - BLE only
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::platform::esp32::Esp32Adapter;
//! use hive_btle::{BleConfig, NodeId};
//!
//! // Create adapter
//! let adapter = Esp32Adapter::new(NodeId::new(0x12345678), "HIVE-Device")?;
//!
//! // Initialize
//! adapter.init(&BleConfig::hive_lite(NodeId::new(0x12345678))).await?;
//!
//! // Start operations
//! adapter.start().await?;
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use log::{debug, error, info, warn};

use crate::config::{BleConfig, BlePhy, DiscoveryConfig};
use crate::discovery::HiveBeacon;
use crate::error::{BleError, Result};
use crate::platform::{BleAdapter, ConnectionCallback, DiscoveryCallback};
use crate::transport::BleConnection;
use crate::NodeId;

// ============================================================================
// NimBLE FFI bindings (only available when building for ESP32)
// ============================================================================

#[cfg(all(feature = "esp32", target_os = "espidf"))]
mod nimble {
    use super::*;
    use core::ffi::{c_int, c_void};
    use core::ptr;
    use esp_idf_svc::sys::*;
    use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

    /// BLE_HS_FOREVER constant (INT32_MAX - advertise/scan forever)
    const BLE_HS_FOREVER: i32 = i32::MAX;

    /// HIVE Service UUID (128-bit) - matches Android/iOS
    pub const HIVE_SERVICE_UUID: [u8; 16] = [
        0x79, 0xd4, 0xc3, 0xb2, 0x02, 0x0e, 0x67, 0xa5, 0x72, 0x43, 0xcc, 0x58, 0x0b, 0xc1, 0x7a,
        0xf4,
    ];

    /// Document characteristic UUID (128-bit)
    pub const DOC_CHAR_UUID: [u8; 16] = [
        0x79, 0xd4, 0xc3, 0xb2, 0x02, 0x0e, 0x67, 0xa5, 0x72, 0x43, 0xcc, 0x58, 0x03, 0x00, 0x7a,
        0xf4,
    ];

    /// 16-bit alias for advertising
    pub const HIVE_SERVICE_UUID_16: u16 = 0xF47A;

    const MAX_DOC_SIZE: usize = 256;
    const MAX_CONNECTIONS: usize = 4;

    /// Connection info for each peer
    #[derive(Clone, Copy, Default)]
    struct PeerConnection {
        handle: u16,
        peer_doc_handle: u16,
        active: bool,
        peer_addr: [u8; 6],
        node_id: u32,
    }

    // Static state for NimBLE callbacks
    static CONNECTIONS: Mutex<[PeerConnection; MAX_CONNECTIONS]> = Mutex::new(
        [PeerConnection {
            handle: 0xFFFF,
            peer_doc_handle: 0,
            active: false,
            peer_addr: [0u8; 6],
            node_id: 0,
        }; MAX_CONNECTIONS],
    );
    static NUM_CONNECTIONS: AtomicU16 = AtomicU16::new(0);
    static CONNECTED: AtomicBool = AtomicBool::new(false);
    static ADVERTISING: AtomicBool = AtomicBool::new(false);
    static SCANNING: AtomicBool = AtomicBool::new(false);
    static POWERED: AtomicBool = AtomicBool::new(false);
    static DOC_CHAR_HANDLE: AtomicU16 = AtomicU16::new(0);
    static DOC_BUFFER: Mutex<[u8; MAX_DOC_SIZE]> = Mutex::new([0u8; MAX_DOC_SIZE]);
    static DOC_LEN: AtomicU16 = AtomicU16::new(0);
    static PENDING_DOCS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());
    static CONNECTING: AtomicBool = AtomicBool::new(false);
    static OUR_MAC: Mutex<[u8; 6]> = Mutex::new([0u8; 6]);

    // GATT service definitions (must be static)
    static mut GATT_SVCS: [ble_gatt_svc_def; 2] = unsafe { core::mem::zeroed() };
    static mut GATT_CHARS: [ble_gatt_chr_def; 2] = unsafe { core::mem::zeroed() };
    static mut SVC_UUID: ble_uuid128_t = unsafe { core::mem::zeroed() };
    static mut CHR_UUID: ble_uuid128_t = unsafe { core::mem::zeroed() };
    static mut DEVICE_NAME: [u8; 20] = [0; 20];
    static mut DEVICE_NAME_LEN: u8 = 0;

    /// Check if advertising data contains HIVE service UUID
    unsafe fn has_hive_service(data: *const u8, len: u8) -> bool {
        if data.is_null() || len < 4 {
            return false;
        }

        let mut i = 0usize;
        while i < len as usize {
            let field_len = *data.add(i) as usize;
            if field_len == 0 || i + field_len >= len as usize {
                break;
            }
            let field_type = *data.add(i + 1);

            // Check for 16-bit Service UUIDs
            if (field_type == 0x02 || field_type == 0x03) && field_len >= 3 {
                let mut j = 2usize;
                while j + 1 < field_len + 1 {
                    let uuid = u16::from_le_bytes([*data.add(i + j), *data.add(i + j + 1)]);
                    if uuid == HIVE_SERVICE_UUID_16 {
                        return true;
                    }
                    j += 2;
                }
            }

            // Check for 128-bit Service UUIDs
            if (field_type == 0x06 || field_type == 0x07) && field_len >= 17 {
                let mut j = 2usize;
                while j + 15 < field_len + 1 {
                    let mut uuid_bytes = [0u8; 16];
                    for k in 0..16 {
                        uuid_bytes[k] = *data.add(i + j + k);
                    }
                    if uuid_bytes == HIVE_SERVICE_UUID {
                        return true;
                    }
                    j += 16;
                }
            }

            i += field_len + 1;
        }
        false
    }

    /// GAP event callback
    unsafe extern "C" fn gap_event_handler(event: *mut ble_gap_event, _arg: *mut c_void) -> c_int {
        let event = &*event;

        match event.type_ as u32 {
            BLE_GAP_EVENT_CONNECT => {
                let connect = &event.__bindgen_anon_1.connect;
                CONNECTING.store(false, Ordering::SeqCst);
                if connect.status == 0 {
                    info!("BLE: Connected, handle={}", connect.conn_handle);

                    if let Ok(mut conns) = CONNECTIONS.lock() {
                        for conn in conns.iter_mut() {
                            if !conn.active {
                                conn.handle = connect.conn_handle;
                                conn.active = true;
                                NUM_CONNECTIONS.fetch_add(1, Ordering::SeqCst);
                                break;
                            }
                        }
                    }

                    CONNECTED.store(true, Ordering::SeqCst);

                    // Request MTU exchange
                    let _ = ble_gattc_exchange_mtu(connect.conn_handle, None, ptr::null_mut());

                    // Restart advertising and scanning
                    let _ = start_advertising_internal();
                    let _ = start_scanning_internal();
                } else {
                    warn!("BLE: Connection failed, status={}", connect.status);
                    let _ = start_advertising_internal();
                    let _ = start_scanning_internal();
                }
            }
            BLE_GAP_EVENT_DISCONNECT => {
                let disconnect = &event.__bindgen_anon_1.disconnect;
                let disc_handle = disconnect.conn.conn_handle;
                info!("BLE: Disconnected, handle={}", disc_handle);

                if let Ok(mut conns) = CONNECTIONS.lock() {
                    for conn in conns.iter_mut() {
                        if conn.active && conn.handle == disc_handle {
                            conn.active = false;
                            conn.handle = 0xFFFF;
                            NUM_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
                            break;
                        }
                    }
                }

                let remaining = NUM_CONNECTIONS.load(Ordering::SeqCst);
                CONNECTED.store(remaining > 0, Ordering::SeqCst);

                let _ = start_advertising_internal();
                let _ = start_scanning_internal();
            }
            BLE_GAP_EVENT_ADV_COMPLETE => {
                debug!("BLE: Advertising complete");
                let _ = start_advertising_internal();
            }
            BLE_GAP_EVENT_DISC => {
                let disc = &event.__bindgen_anon_1.disc;

                if has_hive_service(disc.data, disc.length_data) {
                    let current = NUM_CONNECTIONS.load(Ordering::SeqCst) as usize;
                    if current < MAX_CONNECTIONS && !CONNECTING.load(Ordering::SeqCst) {
                        info!(
                            "BLE: Found HIVE peer, connecting... ({}/{} conns)",
                            current, MAX_CONNECTIONS
                        );
                        CONNECTING.store(true, Ordering::SeqCst);

                        ble_gap_disc_cancel();

                        let ret = ble_gap_connect(
                            BLE_OWN_ADDR_PUBLIC as u8,
                            &disc.addr,
                            10000, // 10 second timeout
                            ptr::null(),
                            Some(gap_event_handler),
                            ptr::null_mut(),
                        );
                        if ret != 0 && ret != 14 {
                            // 14 = BLE_HS_EBUSY
                            warn!("BLE: ble_gap_connect failed: {}", ret);
                            CONNECTING.store(false, Ordering::SeqCst);
                            let _ = start_scanning_internal();
                        }
                    }
                }
            }
            BLE_GAP_EVENT_DISC_COMPLETE => {
                debug!("BLE: Discovery complete");
                let _ = start_advertising_internal();
                if !CONNECTING.load(Ordering::SeqCst) {
                    let _ = start_scanning_internal();
                }
            }
            _ => {
                debug!("BLE: GAP event {}", event.type_);
            }
        }
        0
    }

    /// GATT access callback
    unsafe extern "C" fn gatt_access_cb(
        _conn_handle: u16,
        _attr_handle: u16,
        ctxt: *mut ble_gatt_access_ctxt,
        _arg: *mut c_void,
    ) -> c_int {
        let ctxt = &*ctxt;

        match ctxt.op as u32 {
            BLE_GATT_ACCESS_OP_READ_CHR => {
                info!("BLE: GATT read request");
                if let Ok(doc) = DOC_BUFFER.lock() {
                    let len = DOC_LEN.load(Ordering::SeqCst) as usize;
                    if len > 0 {
                        os_mbuf_append(ctxt.om, doc.as_ptr() as *const c_void, len as u16);
                    }
                }
            }
            BLE_GATT_ACCESS_OP_WRITE_CHR => {
                info!("BLE: GATT write");
                let om = ctxt.om;
                if !om.is_null() {
                    let len = os_mbuf_len(om) as usize;
                    if len > 0 && len <= MAX_DOC_SIZE {
                        let mut buf = vec![0u8; len];
                        let ret =
                            os_mbuf_copydata(om, 0, len as i32, buf.as_mut_ptr() as *mut c_void);
                        if ret == 0 {
                            if let Ok(mut pending) = PENDING_DOCS.lock() {
                                pending.push(buf);
                                info!("BLE: Queued {} bytes", len);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        0
    }

    /// Called when BLE stack syncs
    unsafe extern "C" fn on_sync() {
        info!("BLE: Stack synced");
        POWERED.store(true, Ordering::SeqCst);

        if let Err(e) = start_advertising_internal() {
            error!("BLE: Failed to start advertising: {}", e);
        }

        if let Err(e) = start_scanning_internal() {
            error!("BLE: Failed to start scanning: {}", e);
        }
    }

    /// Called when BLE stack resets
    unsafe extern "C" fn on_reset(reason: c_int) {
        warn!("BLE: Stack reset, reason={}", reason);
        POWERED.store(false, Ordering::SeqCst);
    }

    /// NimBLE host task
    unsafe extern "C" fn nimble_host_task(_param: *mut c_void) {
        info!("BLE: Host task started");
        nimble_port_run();
    }

    /// Initialize NimBLE stack
    pub fn init(node_id: NodeId) -> Result<()> {
        unsafe {
            info!("BLE: Initializing NimBLE for node {:08X}", node_id.as_u32());

            // Build device name
            let name = format!("HIVE-{:08X}", node_id.as_u32());
            let name_bytes = name.as_bytes();
            let len = name_bytes.len().min(DEVICE_NAME.len());
            DEVICE_NAME[..len].copy_from_slice(&name_bytes[..len]);
            DEVICE_NAME_LEN = len as u8;

            // Store our MAC
            let mut mac = [0u8; 6];
            esp_idf_svc::sys::esp_efuse_mac_get_default(mac.as_mut_ptr());
            if let Ok(mut our_mac) = OUR_MAC.lock() {
                *our_mac = mac;
            }
            info!(
                "BLE: Our MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            );

            // Initialize NimBLE
            let ret = nimble_port_init();
            if ret != ESP_OK {
                return Err(BleError::PlatformError(format!(
                    "nimble_port_init failed: {}",
                    ret
                )));
            }

            // Configure host callbacks
            ble_hs_cfg.sync_cb = Some(on_sync);
            ble_hs_cfg.reset_cb = Some(on_reset);

            // Set preferred MTU
            let _ = ble_att_set_preferred_mtu(128);

            // Set up service UUID
            SVC_UUID.u.type_ = BLE_UUID_TYPE_128 as u8;
            SVC_UUID.value = HIVE_SERVICE_UUID;

            // Set up characteristic UUID
            CHR_UUID.u.type_ = BLE_UUID_TYPE_128 as u8;
            CHR_UUID.value = DOC_CHAR_UUID;

            // Configure document characteristic
            GATT_CHARS[0].uuid = &raw const CHR_UUID.u as *const _;
            GATT_CHARS[0].access_cb = Some(gatt_access_cb);
            GATT_CHARS[0].flags = (BLE_GATT_CHR_F_READ
                | BLE_GATT_CHR_F_WRITE
                | BLE_GATT_CHR_F_WRITE_NO_RSP
                | BLE_GATT_CHR_F_NOTIFY) as ble_gatt_chr_flags;
            GATT_CHARS[0].val_handle = &DOC_CHAR_HANDLE as *const _ as *mut u16;
            GATT_CHARS[1] = core::mem::zeroed();

            // Configure service
            GATT_SVCS[0].type_ = BLE_GATT_SVC_TYPE_PRIMARY as u8;
            GATT_SVCS[0].uuid = &raw const SVC_UUID.u as *const _;
            GATT_SVCS[0].characteristics = &raw const GATT_CHARS as *const _ as *mut _;
            GATT_SVCS[1] = core::mem::zeroed();

            // Register services
            let ret = ble_gatts_count_cfg(&raw const GATT_SVCS as *const _);
            if ret != 0 {
                return Err(BleError::GattError(format!(
                    "ble_gatts_count_cfg failed: {}",
                    ret
                )));
            }

            let ret = ble_gatts_add_svcs(&raw const GATT_SVCS as *const _);
            if ret != 0 {
                return Err(BleError::GattError(format!(
                    "ble_gatts_add_svcs failed: {}",
                    ret
                )));
            }

            // Start NimBLE task
            nimble_port_freertos_init(Some(nimble_host_task));

            info!("BLE: NimBLE initialized");
            Ok(())
        }
    }

    fn start_advertising_internal() -> Result<()> {
        unsafe {
            let mut adv_params: ble_gap_adv_params = core::mem::zeroed();
            adv_params.conn_mode = BLE_GAP_CONN_MODE_UND as u8;
            adv_params.disc_mode = BLE_GAP_DISC_MODE_GEN as u8;
            adv_params.itvl_min = 160; // 100ms
            adv_params.itvl_max = 320; // 200ms

            let mut fields: ble_hs_adv_fields = core::mem::zeroed();
            fields.flags = (BLE_HS_ADV_F_DISC_GEN | BLE_HS_ADV_F_BREDR_UNSUP) as u8;
            fields.uuids128 = &raw const SVC_UUID as *const _ as *mut ble_uuid128_t;
            fields.num_uuids128 = 1;
            fields.set_uuids128_is_complete(1);

            let ret = ble_gap_adv_set_fields(&fields);
            if ret != 0 {
                return Err(BleError::PlatformError(format!(
                    "ble_gap_adv_set_fields failed: {}",
                    ret
                )));
            }

            // Set scan response with device name
            let mut rsp_fields: ble_hs_adv_fields = core::mem::zeroed();
            rsp_fields.name = DEVICE_NAME.as_ptr();
            rsp_fields.name_len = DEVICE_NAME_LEN;
            rsp_fields.set_name_is_complete(1);

            let _ = ble_gap_adv_rsp_set_fields(&rsp_fields);

            let ret = ble_gap_adv_start(
                BLE_OWN_ADDR_PUBLIC as u8,
                ptr::null(),
                BLE_HS_FOREVER,
                &adv_params,
                Some(gap_event_handler),
                ptr::null_mut(),
            );
            if ret != 0 && ret != BLE_HS_EALREADY as i32 {
                return Err(BleError::PlatformError(format!(
                    "ble_gap_adv_start failed: {}",
                    ret
                )));
            }

            ADVERTISING.store(true, Ordering::SeqCst);
            info!("BLE: Advertising started");
            Ok(())
        }
    }

    fn start_scanning_internal() -> Result<()> {
        unsafe {
            let mut params: ble_gap_disc_params = core::mem::zeroed();
            params.itvl = 160; // 100ms
            params.window = 80; // 50ms
            params.filter_policy = BLE_HCI_SCAN_FILT_NO_WL as u8;
            params.set_limited(0);
            params.set_passive(0);
            params.set_filter_duplicates(1);

            let ret = ble_gap_disc(
                BLE_OWN_ADDR_PUBLIC as u8,
                10000, // 10 seconds
                &params,
                Some(gap_event_handler),
                ptr::null_mut(),
            );
            if ret != 0 && ret != 2 {
                // 2 = BLE_HS_EALREADY
                return Err(BleError::DiscoveryFailed(format!(
                    "ble_gap_disc failed: {}",
                    ret
                )));
            }

            SCANNING.store(true, Ordering::SeqCst);
            info!("BLE: Scanning started");
            Ok(())
        }
    }

    pub fn start_advertising() -> Result<()> {
        start_advertising_internal()
    }

    pub fn stop_advertising() -> Result<()> {
        unsafe {
            ble_gap_adv_stop();
        }
        ADVERTISING.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn start_scanning() -> Result<()> {
        start_scanning_internal()
    }

    pub fn stop_scanning() -> Result<()> {
        unsafe {
            ble_gap_disc_cancel();
        }
        SCANNING.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_powered() -> bool {
        POWERED.load(Ordering::SeqCst)
    }

    pub fn is_advertising() -> bool {
        ADVERTISING.load(Ordering::SeqCst)
    }

    pub fn is_scanning() -> bool {
        SCANNING.load(Ordering::SeqCst)
    }

    pub fn connection_count() -> usize {
        NUM_CONNECTIONS.load(Ordering::SeqCst) as usize
    }

    pub fn get_mac_address() -> Option<String> {
        if let Ok(mac) = OUR_MAC.lock() {
            Some(format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            ))
        } else {
            None
        }
    }

    pub fn set_document(data: &[u8]) {
        if data.len() <= MAX_DOC_SIZE {
            if let Ok(mut doc) = DOC_BUFFER.lock() {
                doc[..data.len()].copy_from_slice(data);
                DOC_LEN.store(data.len() as u16, Ordering::SeqCst);
            }
        }
    }

    pub fn take_pending_document() -> Option<Vec<u8>> {
        if let Ok(mut pending) = PENDING_DOCS.lock() {
            if !pending.is_empty() {
                Some(pending.remove(0))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Send document to all connected peers via notifications
    pub fn gossip_document(data: &[u8]) -> usize {
        set_document(data);

        let mut sent_count = 0;
        let our_handle = DOC_CHAR_HANDLE.load(Ordering::SeqCst);

        if our_handle != 0 {
            if let Ok(conns) = CONNECTIONS.lock() {
                for conn in conns.iter() {
                    if conn.active {
                        unsafe {
                            let om = ble_hs_mbuf_from_flat(
                                data.as_ptr() as *const c_void,
                                data.len() as u16,
                            );
                            if !om.is_null() {
                                let ret = ble_gatts_notify_custom(conn.handle, our_handle, om);
                                if ret == 0 {
                                    sent_count += 1;
                                } else {
                                    os_mbuf_free_chain(om);
                                }
                            }
                        }
                    }
                }
            }
        }

        sent_count
    }
}

// ============================================================================
// ESP32 BLE Adapter Implementation
// ============================================================================

/// ESP32 BLE connection handle
pub struct Esp32Connection {
    peer_id: NodeId,
    conn_handle: u16,
    address: String,
    mtu: u16,
    connected_at_ms: u64,
    current_time_ms: u64,
    alive: bool,
}

impl Esp32Connection {
    pub fn new(peer_id: NodeId, conn_handle: u16, address: String) -> Self {
        Self {
            peer_id,
            conn_handle,
            address,
            mtu: 23,
            connected_at_ms: 0,
            current_time_ms: 0,
            alive: true,
        }
    }

    pub fn set_time_ms(&mut self, time_ms: u64) {
        if self.connected_at_ms == 0 {
            self.connected_at_ms = time_ms;
        }
        self.current_time_ms = time_ms;
    }
}

impl BleConnection for Esp32Connection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        self.alive
    }

    fn mtu(&self) -> u16 {
        self.mtu
    }

    fn phy(&self) -> BlePhy {
        BlePhy::Le1M
    }

    fn rssi(&self) -> Option<i8> {
        None
    }

    fn connected_duration(&self) -> core::time::Duration {
        let ms = self.current_time_ms.saturating_sub(self.connected_at_ms);
        core::time::Duration::from_millis(ms)
    }
}

/// ESP32 adapter state
struct Esp32AdapterState {
    connections: HashMap<NodeId, Esp32Connection>,
    handle_map: HashMap<u16, NodeId>,
    discovery_callback: Option<DiscoveryCallback>,
    connection_callback: Option<ConnectionCallback>,
    advertising: bool,
    scanning: bool,
    powered: bool,
}

impl Default for Esp32AdapterState {
    fn default() -> Self {
        Self {
            connections: HashMap::new(),
            handle_map: HashMap::new(),
            discovery_callback: None,
            connection_callback: None,
            advertising: false,
            scanning: false,
            powered: false,
        }
    }
}

/// ESP32 BLE Adapter using NimBLE
pub struct Esp32Adapter {
    state: Arc<Mutex<Esp32AdapterState>>,
    node_id: NodeId,
    device_name: String,
    beacon: Option<HiveBeacon>,
    #[cfg(all(feature = "esp32", target_os = "espidf"))]
    initialized: std::sync::atomic::AtomicBool,
}

impl Esp32Adapter {
    pub fn new(node_id: NodeId, device_name: &str) -> Result<Self> {
        info!(
            "ESP32: Creating BLE adapter for node {:08X}",
            node_id.as_u32()
        );

        Ok(Self {
            state: Arc::new(Mutex::new(Esp32AdapterState::default())),
            node_id,
            device_name: device_name.to_string(),
            beacon: None,
            #[cfg(all(feature = "esp32", target_os = "espidf"))]
            initialized: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub fn hive_lite(node_id: NodeId) -> Result<Self> {
        Self::new(node_id, &format!("HIVE-{:08X}", node_id.as_u32()))
    }

    fn build_adv_data(&self, beacon: &HiveBeacon) -> Vec<u8> {
        let mut data = Vec::with_capacity(31);

        // Flags
        data.push(0x02);
        data.push(0x01);
        data.push(0x06);

        // 16-bit Service UUIDs
        data.push(0x03);
        data.push(0x03);
        data.extend_from_slice(&crate::HIVE_SERVICE_UUID_16BIT.to_le_bytes());

        // Service Data
        let beacon_data = beacon.encode_compact();
        data.push((beacon_data.len() + 3) as u8);
        data.push(0x16);
        data.extend_from_slice(&crate::HIVE_SERVICE_UUID_16BIT.to_le_bytes());
        data.extend_from_slice(&beacon_data);

        data
    }

    /// Take pending received document (call from main loop)
    #[cfg(all(feature = "esp32", target_os = "espidf"))]
    pub fn take_pending_document(&self) -> Option<Vec<u8>> {
        nimble::take_pending_document()
    }

    /// Send document to all connected peers
    #[cfg(all(feature = "esp32", target_os = "espidf"))]
    pub fn gossip_document(&self, data: &[u8]) -> usize {
        nimble::gossip_document(data)
    }

    /// Update local document for GATT reads
    #[cfg(all(feature = "esp32", target_os = "espidf"))]
    pub fn set_document(&self, data: &[u8]) {
        nimble::set_document(data)
    }
}

#[async_trait]
impl BleAdapter for Esp32Adapter {
    async fn init(&mut self, config: &BleConfig) -> Result<()> {
        info!("ESP32: Initializing with config {:?}", config);

        self.beacon = Some(HiveBeacon::new(config.node_id));

        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            if !self.initialized.load(std::sync::atomic::Ordering::SeqCst) {
                nimble::init(self.node_id)?;
                self.initialized
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }

        let mut state = self.state.lock().unwrap();
        state.powered = true;

        Ok(())
    }

    async fn start(&self) -> Result<()> {
        info!("ESP32: Starting adapter");

        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            nimble::start_advertising()?;
            nimble::start_scanning()?;
        }

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("ESP32: Stopping adapter");

        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            let _ = nimble::stop_advertising();
            let _ = nimble::stop_scanning();
        }

        let mut state = self.state.lock().unwrap();
        state.advertising = false;
        state.scanning = false;
        Ok(())
    }

    fn is_powered(&self) -> bool {
        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            nimble::is_powered()
        }
        #[cfg(not(all(feature = "esp32", target_os = "espidf")))]
        {
            self.state.lock().unwrap().powered
        }
    }

    fn address(&self) -> Option<String> {
        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            nimble::get_mac_address()
        }
        #[cfg(not(all(feature = "esp32", target_os = "espidf")))]
        {
            None
        }
    }

    async fn start_scan(&self, _config: &DiscoveryConfig) -> Result<()> {
        info!("ESP32: Starting scan");

        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            nimble::start_scanning()?;
        }

        let mut state = self.state.lock().unwrap();
        state.scanning = true;
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        info!("ESP32: Stopping scan");

        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            nimble::stop_scanning()?;
        }

        let mut state = self.state.lock().unwrap();
        state.scanning = false;
        Ok(())
    }

    async fn start_advertising(&self, _config: &DiscoveryConfig) -> Result<()> {
        info!("ESP32: Starting advertising");

        if let Some(ref beacon) = self.beacon {
            let adv_data = self.build_adv_data(beacon);
            debug!(
                "ESP32: Advertising data ({} bytes): {:02X?}",
                adv_data.len(),
                adv_data
            );
        }

        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            nimble::start_advertising()?;
        }

        let mut state = self.state.lock().unwrap();
        state.advertising = true;
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<()> {
        info!("ESP32: Stopping advertising");

        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            nimble::stop_advertising()?;
        }

        let mut state = self.state.lock().unwrap();
        state.advertising = false;
        Ok(())
    }

    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>) {
        let mut state = self.state.lock().unwrap();
        state.discovery_callback = callback;
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        info!("ESP32: Connecting to {:08X}", peer_id.as_u32());
        // Connection is handled automatically by NimBLE GAP callbacks
        Err(BleError::NotSupported(
            "ESP32 uses automatic connection via GAP discovery".into(),
        ))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        info!("ESP32: Disconnecting from {:08X}", peer_id.as_u32());
        let mut state = self.state.lock().unwrap();
        if let Some(conn) = state.connections.remove(peer_id) {
            state.handle_map.remove(&conn.conn_handle);
        }
        Ok(())
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>> {
        let state = self.state.lock().unwrap();
        state.connections.get(peer_id).map(|conn| {
            Box::new(Esp32Connection::new(
                conn.peer_id,
                conn.conn_handle,
                conn.address.clone(),
            )) as Box<dyn BleConnection>
        })
    }

    fn peer_count(&self) -> usize {
        #[cfg(all(feature = "esp32", target_os = "espidf"))]
        {
            nimble::connection_count()
        }
        #[cfg(not(all(feature = "esp32", target_os = "espidf")))]
        {
            self.state.lock().unwrap().connections.len()
        }
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.state
            .lock()
            .unwrap()
            .connections
            .keys()
            .copied()
            .collect()
    }

    fn set_connection_callback(&mut self, callback: Option<ConnectionCallback>) {
        let mut state = self.state.lock().unwrap();
        state.connection_callback = callback;
    }

    async fn register_gatt_service(&self) -> Result<()> {
        info!("ESP32: GATT service registered during init");
        Ok(())
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        info!("ESP32: Unregistering HIVE GATT service");
        Ok(())
    }

    fn supports_coded_phy(&self) -> bool {
        // Original ESP32 does not support Coded PHY
        false
    }

    fn supports_extended_advertising(&self) -> bool {
        // Original ESP32 does not support extended advertising
        false
    }

    fn max_mtu(&self) -> u16 {
        512
    }

    fn max_connections(&self) -> u8 {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adv_data_size() {
        let beacon = HiveBeacon::new(NodeId::new(0x12345678));
        let expected_size = 3 + 4 + 3 + crate::discovery::BEACON_COMPACT_SIZE;
        assert!(
            expected_size <= 31,
            "Adv data ({}) exceeds 31-byte limit",
            expected_size
        );
    }

    #[test]
    fn test_esp32_connection() {
        let conn =
            Esp32Connection::new(NodeId::new(0x12345678), 1, "00:11:22:33:44:55".to_string());
        assert_eq!(conn.peer_id().as_u32(), 0x12345678);
        assert!(conn.is_alive());
        assert_eq!(conn.mtu(), 23);
        assert_eq!(conn.phy(), BlePhy::Le1M);
    }
}
