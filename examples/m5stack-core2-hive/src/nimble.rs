//! NimBLE BLE wrapper for ESP32
//!
//! Provides safe Rust wrappers around ESP-IDF NimBLE APIs for:
//! - GAP advertising and scanning
//! - GATT server with custom service
//! - Document exchange between peers

use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Mutex;

use esp_idf_svc::sys::*;
use ::log::{debug, error, info, warn};

/// BLE_HS_FOREVER constant (INT32_MAX - advertise/scan forever)
const BLE_HS_FOREVER: i32 = i32::MAX;

use hive_btle::NodeId;

/// HIVE Service UUID (16-bit)
pub const HIVE_SERVICE_UUID: u16 = 0xF47A;

/// Document characteristic UUID (16-bit)
pub const DOC_CHAR_UUID: u16 = 0xF47B;

/// Maximum document size
const MAX_DOC_SIZE: usize = 256;

/// BLE connection state
static CONNECTED: AtomicBool = AtomicBool::new(false);
static CONN_HANDLE: AtomicU16 = AtomicU16::new(0xFFFF);

/// Document characteristic value handle (set during registration)
static DOC_CHAR_HANDLE: AtomicU16 = AtomicU16::new(0);

/// Peer's document characteristic handle (discovered via GATT)
static PEER_DOC_HANDLE: AtomicU16 = AtomicU16::new(0);

/// Shared document buffer for GATT access
static DOC_BUFFER: Mutex<[u8; MAX_DOC_SIZE]> = Mutex::new([0u8; MAX_DOC_SIZE]);
static DOC_LEN: AtomicU16 = AtomicU16::new(0);

/// Pending received document (set by GATT callback, read by main loop)
static PENDING_DOC: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Connection state change flag
static CONNECTION_CHANGED: AtomicBool = AtomicBool::new(false);

/// Flag to track if we're currently connecting (to avoid multiple connection attempts)
static CONNECTING: AtomicBool = AtomicBool::new(false);

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

        // Check for Complete or Incomplete 16-bit Service UUIDs (0x03 or 0x02)
        if (field_type == 0x02 || field_type == 0x03) && field_len >= 3 {
            // Parse 16-bit UUIDs
            let mut j = 2usize;
            while j + 1 < field_len + 1 {
                let uuid = u16::from_le_bytes([*data.add(i + j), *data.add(i + j + 1)]);
                if uuid == HIVE_SERVICE_UUID {
                    return true;
                }
                j += 2;
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
                CONNECTED.store(true, Ordering::SeqCst);
                CONN_HANDLE.store(connect.conn_handle, Ordering::SeqCst);
                CONNECTION_CHANGED.store(true, Ordering::SeqCst);

                // Request MTU exchange for larger payloads (default is 23, need at least 27 for our docs)
                let ret = ble_gattc_exchange_mtu(connect.conn_handle, Some(mtu_exchange_cb), ptr::null_mut());
                if ret != 0 {
                    warn!("BLE: MTU exchange failed to start: {}", ret);
                    // Fall back to service discovery anyway
                    let ret = ble_gattc_disc_all_svcs(connect.conn_handle, Some(gatt_disc_svc_cb), ptr::null_mut());
                    if ret != 0 {
                        warn!("BLE: Failed to start service discovery: {}", ret);
                    }
                }
            } else {
                warn!("BLE: Connection failed, status={}", connect.status);
                // Restart scanning
                let _ = start_scanning();
            }
        }
        BLE_GAP_EVENT_DISCONNECT => {
            info!("BLE: Disconnected");
            CONNECTED.store(false, Ordering::SeqCst);
            CONN_HANDLE.store(0xFFFF, Ordering::SeqCst);
            CONNECTION_CHANGED.store(true, Ordering::SeqCst);
            // Restart advertising and scanning
            let _ = start_advertising();
            let _ = start_scanning();
        }
        BLE_GAP_EVENT_ADV_COMPLETE => {
            debug!("BLE: Advertising complete");
            // Restart advertising if not connected
            if !CONNECTED.load(Ordering::SeqCst) {
                let _ = start_advertising();
            }
        }
        BLE_GAP_EVENT_DISC => {
            let disc = &event.__bindgen_anon_1.disc;

            // Check if this device advertises HIVE service
            if has_hive_service(disc.data, disc.length_data) {
                info!("BLE: Found HIVE peer!");

                // Only connect if not already connected/connecting
                if !CONNECTED.load(Ordering::SeqCst) && !CONNECTING.load(Ordering::SeqCst) {
                    CONNECTING.store(true, Ordering::SeqCst);

                    // Stop scanning before connecting
                    ble_gap_disc_cancel();

                    // Connect to this peer
                    info!("BLE: Connecting to peer...");
                    let ret = ble_gap_connect(
                        BLE_OWN_ADDR_PUBLIC as u8,
                        &disc.addr,
                        30000, // 30 second timeout
                        ptr::null(),
                        Some(gap_event_handler),
                        ptr::null_mut(),
                    );
                    if ret != 0 {
                        error!("BLE: ble_gap_connect failed: {}", ret);
                        CONNECTING.store(false, Ordering::SeqCst);
                        // Restart scanning
                        let _ = start_scanning();
                    }
                }
            }
        }
        BLE_GAP_EVENT_DISC_COMPLETE => {
            debug!("BLE: Discovery complete");
            // Restart scanning if not connected
            if !CONNECTED.load(Ordering::SeqCst) && !CONNECTING.load(Ordering::SeqCst) {
                let _ = start_scanning();
            }
        }
        BLE_GAP_EVENT_NOTIFY_RX => {
            // Received notification from peer
            let notify = &event.__bindgen_anon_1.notify_rx;
            info!("BLE: Received notification, attr_handle={}", notify.attr_handle);
            let om = notify.om;
            if !om.is_null() {
                let len = os_mbuf_len(om) as usize;
                if len > 0 && len <= MAX_DOC_SIZE {
                    let mut buf = vec![0u8; len];
                    let ret = os_mbuf_copydata(om, 0, len as i32, buf.as_mut_ptr() as *mut c_void);
                    if ret == 0 {
                        info!("BLE: Notification data, {} bytes", len);
                        if let Ok(mut pending) = PENDING_DOC.lock() {
                            *pending = Some(buf);
                        }
                    }
                }
            }
        }
        _ => {
            debug!("BLE: GAP event {}", event.type_);
        }
    }
    0
}

/// GATT service discovery callback
unsafe extern "C" fn gatt_disc_svc_cb(
    conn_handle: u16,
    error: *const ble_gatt_error,
    service: *const ble_gatt_svc,
    _arg: *mut c_void,
) -> c_int {
    if error.is_null() {
        return 0;
    }

    let err = &*error;
    if err.status == BLE_HS_EDONE as u16 {
        // Service discovery complete - now discover characteristics
        info!("BLE: Service discovery complete, discovering characteristics...");
        // Discover all characteristics
        let ret = ble_gattc_disc_all_chrs(
            conn_handle,
            1, 0xFFFF, // All handles
            Some(gatt_disc_chr_cb),
            ptr::null_mut(),
        );
        if ret != 0 {
            warn!("BLE: Failed to start characteristic discovery: {}", ret);
        }
        return 0;
    }

    if err.status != 0 {
        warn!("BLE: Service discovery error: {}", err.status);
        return 0;
    }

    if !service.is_null() {
        let svc = &*service;
        debug!("BLE: Found service, handles {}-{}", svc.start_handle, svc.end_handle);
    }

    0
}

/// GATT characteristic discovery callback
unsafe extern "C" fn gatt_disc_chr_cb(
    conn_handle: u16,
    error: *const ble_gatt_error,
    chr: *const ble_gatt_chr,
    _arg: *mut c_void,
) -> c_int {
    if error.is_null() {
        return 0;
    }

    let err = &*error;
    if err.status == BLE_HS_EDONE as u16 {
        info!("BLE: Characteristic discovery complete");
        // Subscribe to notifications first
        let peer_handle = PEER_DOC_HANDLE.load(Ordering::SeqCst);
        if peer_handle != 0 {
            // Subscribe to notifications (CCCD is handle + 1 typically)
            let cccd_handle = peer_handle + 1;
            let notify_enable: [u8; 2] = [0x01, 0x00]; // Enable notifications
            info!("BLE: Subscribing to notifications on handle {}", cccd_handle);
            let om = ble_hs_mbuf_from_flat(notify_enable.as_ptr() as *const c_void, 2);
            if !om.is_null() {
                let ret = ble_gattc_write_flat(conn_handle, cccd_handle, notify_enable.as_ptr() as *const c_void, 2, Some(gatt_write_cb), ptr::null_mut());
                if ret != 0 {
                    warn!("BLE: Failed to subscribe to notifications: {}", ret);
                    os_mbuf_free_chain(om);
                }
            }

            // Read the peer's document
            info!("BLE: Reading peer document from handle {}", peer_handle);
            let ret = ble_gattc_read(conn_handle, peer_handle, Some(gatt_read_cb), ptr::null_mut());
            if ret != 0 {
                warn!("BLE: Failed to read peer document: {}", ret);
            }
        }
        return 0;
    }

    if err.status != 0 {
        warn!("BLE: Characteristic discovery error: {}", err.status);
        return 0;
    }

    if !chr.is_null() {
        let c = &*chr;
        // Check if this is the HIVE document characteristic
        if c.uuid.u.type_ == BLE_UUID_TYPE_16 as u8 {
            let uuid16 = &*((&c.uuid) as *const ble_uuid_any_t as *const ble_uuid16_t);
            if uuid16.value == DOC_CHAR_UUID {
                info!("BLE: Found HIVE document characteristic, handle={}", c.val_handle);
                PEER_DOC_HANDLE.store(c.val_handle, Ordering::SeqCst);
            }
        }
    }

    0
}

/// MTU exchange callback
unsafe extern "C" fn mtu_exchange_cb(
    conn_handle: u16,
    error: *const ble_gatt_error,
    mtu: u16,
    _arg: *mut c_void,
) -> c_int {
    if !error.is_null() {
        let err = &*error;
        if err.status == 0 {
            info!("BLE: MTU exchange complete, MTU={}", mtu);
        } else {
            warn!("BLE: MTU exchange failed: {}", err.status);
        }
    }

    // Now start service discovery regardless of MTU result
    let ret = ble_gattc_disc_all_svcs(conn_handle, Some(gatt_disc_svc_cb), ptr::null_mut());
    if ret != 0 {
        warn!("BLE: Failed to start service discovery after MTU exchange: {}", ret);
    }
    0
}

/// GATT write callback (for subscription confirmation)
unsafe extern "C" fn gatt_write_cb(
    _conn_handle: u16,
    error: *const ble_gatt_error,
    _attr: *mut ble_gatt_attr,
    _arg: *mut c_void,
) -> c_int {
    if !error.is_null() {
        let err = &*error;
        if err.status == 0 {
            info!("BLE: Subscribed to notifications successfully");
        } else {
            warn!("BLE: Subscription failed: {}", err.status);
        }
    }
    0
}

/// GATT read callback
unsafe extern "C" fn gatt_read_cb(
    conn_handle: u16,
    error: *const ble_gatt_error,
    attr: *mut ble_gatt_attr,
    _arg: *mut c_void,
) -> c_int {
    if error.is_null() {
        return 0;
    }

    let err = &*error;
    if err.status != 0 {
        warn!("BLE: Read error: {}", err.status);
        return 0;
    }

    if !attr.is_null() {
        let a = &*attr;
        let om = a.om;
        if !om.is_null() {
            let len = os_mbuf_len(om) as usize;
            if len > 0 && len <= MAX_DOC_SIZE {
                let mut buf = vec![0u8; len];
                let ret = os_mbuf_copydata(om, 0, len as i32, buf.as_mut_ptr() as *mut c_void);
                if ret == 0 {
                    info!("BLE: Read peer document, {} bytes", len);
                    if let Ok(mut pending) = PENDING_DOC.lock() {
                        *pending = Some(buf);
                    }
                }
            }
        }
    }

    // After reading peer's document, write our document to peer
    let peer_handle = PEER_DOC_HANDLE.load(Ordering::SeqCst);
    if peer_handle != 0 {
        if let Ok(doc) = DOC_BUFFER.lock() {
            let len = DOC_LEN.load(Ordering::SeqCst) as usize;
            if len > 0 {
                let om = ble_hs_mbuf_from_flat(doc.as_ptr() as *const c_void, len as u16);
                if !om.is_null() {
                    info!("BLE: Writing our document to peer, {} bytes", len);
                    let ret = ble_gattc_write_no_rsp(conn_handle, peer_handle, om);
                    if ret != 0 {
                        warn!("BLE: Write failed: {}", ret);
                        os_mbuf_free_chain(om);
                    }
                }
            }
        }
    }

    0
}

/// GATT access callback for document characteristic
unsafe extern "C" fn gatt_access_cb(
    _conn_handle: u16,
    _attr_handle: u16,
    ctxt: *mut ble_gatt_access_ctxt,
    _arg: *mut c_void,
) -> c_int {
    let ctxt = &*ctxt;

    match ctxt.op as u32 {
        BLE_GATT_ACCESS_OP_READ_CHR => {
            // Peer is reading our document
            info!("BLE: GATT read request");
            if let Ok(doc) = DOC_BUFFER.lock() {
                let len = DOC_LEN.load(Ordering::SeqCst) as usize;
                if len > 0 {
                    os_mbuf_append(ctxt.om, doc.as_ptr() as *const c_void, len as u16);
                }
            }
        }
        BLE_GATT_ACCESS_OP_WRITE_CHR => {
            // Peer is sending their document (write from central)
            info!("BLE: GATT write request received");
            let om = ctxt.om;
            if !om.is_null() {
                // Get total length across all mbuf segments using OS_MBUF_PKTLEN
                let len = os_mbuf_len(om) as usize;
                info!("BLE: Write total data length: {}", len);
                if len > 0 && len <= MAX_DOC_SIZE {
                    let mut buf = vec![0u8; len];
                    // Copy all data from mbuf chain
                    let ret = os_mbuf_copydata(om, 0, len as i32, buf.as_mut_ptr() as *mut c_void);
                    if ret == 0 {
                        info!("BLE: Received document via write, {} bytes", len);
                        // Store in pending queue
                        if let Ok(mut pending) = PENDING_DOC.lock() {
                            *pending = Some(buf);
                        }
                    } else {
                        warn!("BLE: Failed to copy mbuf data: {}", ret);
                    }
                }
            } else {
                warn!("BLE: Write request with null om");
            }
        }
        _ => {}
    }
    0
}

/// GATT service definition (must be static)
static mut GATT_SVCS: [ble_gatt_svc_def; 2] = unsafe { core::mem::zeroed() };
static mut GATT_CHARS: [ble_gatt_chr_def; 2] = unsafe { core::mem::zeroed() };
static mut SVC_UUID: ble_uuid16_t = unsafe { core::mem::zeroed() };
static mut CHR_UUID: ble_uuid16_t = unsafe { core::mem::zeroed() };

/// Initialize NimBLE stack
pub fn init(_node_id: NodeId) -> Result<(), i32> {
    unsafe {
        info!("BLE: Initializing NimBLE");

        // Initialize NimBLE
        let ret = nimble_port_init();
        if ret != ESP_OK {
            error!("BLE: nimble_port_init failed: {}", ret);
            return Err(ret);
        }

        // Configure host
        ble_hs_cfg.sync_cb = Some(on_sync);
        ble_hs_cfg.reset_cb = Some(on_reset);

        // Set preferred ATT MTU to 64 bytes (enough for our 24-byte documents + overhead)
        let ret = ble_att_set_preferred_mtu(64);
        if ret != 0 {
            warn!("BLE: Failed to set preferred MTU: {}", ret);
        } else {
            info!("BLE: Preferred MTU set to 64");
        }

        // Set up service UUID
        SVC_UUID.u.type_ = BLE_UUID_TYPE_16 as u8;
        SVC_UUID.value = HIVE_SERVICE_UUID;

        // Set up characteristic UUID
        CHR_UUID.u.type_ = BLE_UUID_TYPE_16 as u8;
        CHR_UUID.value = DOC_CHAR_UUID;

        // Configure document characteristic
        GATT_CHARS[0].uuid = &raw const CHR_UUID.u as *const _;
        GATT_CHARS[0].access_cb = Some(gatt_access_cb);
        GATT_CHARS[0].flags =
            (BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_WRITE | BLE_GATT_CHR_F_NOTIFY) as ble_gatt_chr_flags;
        GATT_CHARS[0].val_handle = &DOC_CHAR_HANDLE as *const _ as *mut u16;
        // Null terminator
        GATT_CHARS[1] = core::mem::zeroed();

        // Configure service
        GATT_SVCS[0].type_ = BLE_GATT_SVC_TYPE_PRIMARY as u8;
        GATT_SVCS[0].uuid = &raw const SVC_UUID.u as *const _;
        GATT_SVCS[0].characteristics = &raw const GATT_CHARS as *const _ as *mut _;
        // Null terminator
        GATT_SVCS[1] = core::mem::zeroed();

        // Register services
        let ret = ble_gatts_count_cfg(&raw const GATT_SVCS as *const _);
        if ret != 0 {
            error!("BLE: ble_gatts_count_cfg failed: {}", ret);
            return Err(ret);
        }

        let ret = ble_gatts_add_svcs(&raw const GATT_SVCS as *const _);
        if ret != 0 {
            error!("BLE: ble_gatts_add_svcs failed: {}", ret);
            return Err(ret);
        }

        // Start NimBLE task
        nimble_port_freertos_init(Some(nimble_host_task));

        info!("BLE: NimBLE initialized");
        Ok(())
    }
}

/// NimBLE host task (runs in FreeRTOS)
unsafe extern "C" fn nimble_host_task(_param: *mut c_void) {
    info!("BLE: Host task started");
    nimble_port_run();
}

/// Called when BLE stack syncs
unsafe extern "C" fn on_sync() {
    info!("BLE: Stack synced");

    // Start advertising
    if let Err(e) = start_advertising() {
        error!("BLE: Failed to start advertising: {}", e);
    }

    // Start scanning for peers
    if let Err(e) = start_scanning() {
        error!("BLE: Failed to start scanning: {}", e);
    }
}

/// Called when BLE stack resets
unsafe extern "C" fn on_reset(reason: c_int) {
    warn!("BLE: Stack reset, reason={}", reason);
}

/// Start BLE advertising
pub fn start_advertising() -> Result<(), i32> {
    unsafe {
        let mut adv_params: ble_gap_adv_params = core::mem::zeroed();
        adv_params.conn_mode = BLE_GAP_CONN_MODE_UND as u8;
        adv_params.disc_mode = BLE_GAP_DISC_MODE_GEN as u8;
        adv_params.itvl_min = 160; // 100ms
        adv_params.itvl_max = 320; // 200ms

        // Build advertising data
        let mut fields: ble_hs_adv_fields = core::mem::zeroed();
        fields.flags = (BLE_HS_ADV_F_DISC_GEN | BLE_HS_ADV_F_BREDR_UNSUP) as u8;
        fields.uuids16 = &raw const SVC_UUID as *const _ as *mut ble_uuid16_t;
        fields.num_uuids16 = 1;
        fields.set_uuids16_is_complete(1);

        let ret = ble_gap_adv_set_fields(&fields);
        if ret != 0 {
            error!("BLE: ble_gap_adv_set_fields failed: {}", ret);
            return Err(ret);
        }

        let ret = ble_gap_adv_start(
            BLE_OWN_ADDR_PUBLIC as u8,
            ptr::null(),
            BLE_HS_FOREVER,
            &adv_params,
            Some(gap_event_handler),
            ptr::null_mut(),
        );
        if ret != 0 && ret != BLE_HS_EALREADY as i32 {
            error!("BLE: ble_gap_adv_start failed: {}", ret);
            return Err(ret);
        }

        info!("BLE: Advertising started");
        Ok(())
    }
}

/// Start BLE scanning for peers
pub fn start_scanning() -> Result<(), i32> {
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
        if ret != 0 {
            error!("BLE: ble_gap_disc failed: {}", ret);
            return Err(ret);
        }

        info!("BLE: Scanning started");
        Ok(())
    }
}

/// Update local document (for GATT reads)
pub fn set_document(data: &[u8]) {
    if data.len() <= MAX_DOC_SIZE {
        if let Ok(mut doc) = DOC_BUFFER.lock() {
            doc[..data.len()].copy_from_slice(data);
            DOC_LEN.store(data.len() as u16, Ordering::SeqCst);
        }
    }
}

/// Send document to connected peer via notification (as peripheral) and write (as central)
pub fn notify_document(data: &[u8]) -> Result<(), i32> {
    let conn = CONN_HANDLE.load(Ordering::SeqCst);
    if conn == 0xFFFF {
        return Err(-1); // Not connected
    }

    // Update local buffer first
    set_document(data);

    // Try to notify (works if we're the peripheral)
    let our_handle = DOC_CHAR_HANDLE.load(Ordering::SeqCst);
    if our_handle != 0 {
        unsafe {
            let ret = ble_gatts_notify(conn, our_handle);
            if ret == 0 {
                info!("BLE: Notified document, {} bytes", data.len());
            }
        }
    }

    // Also write to peer (works if we're the central and discovered their characteristic)
    let peer_handle = PEER_DOC_HANDLE.load(Ordering::SeqCst);
    if peer_handle != 0 {
        unsafe {
            let om = ble_hs_mbuf_from_flat(data.as_ptr() as *const c_void, data.len() as u16);
            if !om.is_null() {
                let ret = ble_gattc_write_no_rsp(conn, peer_handle, om);
                if ret == 0 {
                    info!("BLE: Wrote document to peer, {} bytes", data.len());
                } else {
                    warn!("BLE: Write to peer failed: {}", ret);
                    os_mbuf_free_chain(om);
                }
            }
        }
    }

    Ok(())
}

/// Take pending received document (returns None if no document waiting)
pub fn take_pending_document() -> Option<Vec<u8>> {
    if let Ok(mut pending) = PENDING_DOC.lock() {
        pending.take()
    } else {
        None
    }
}

/// Check if connection state changed (and clear the flag)
pub fn take_connection_changed() -> bool {
    CONNECTION_CHANGED.swap(false, Ordering::SeqCst)
}

/// Check if connected to a peer
pub fn is_connected() -> bool {
    CONNECTED.load(Ordering::SeqCst)
}
