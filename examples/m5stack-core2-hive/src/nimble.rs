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

/// HIVE Service UUID (128-bit) - must match Android/iOS
/// f47ac10b-58cc-4372-a567-0e02b2c3d479
pub const HIVE_SERVICE_UUID: [u8; 16] = [
    0x79, 0xd4, 0xc3, 0xb2, 0x02, 0x0e, 0x67, 0xa5,
    0x72, 0x43, 0xcc, 0x58, 0x0b, 0xc1, 0x7a, 0xf4
];

/// Document characteristic UUID (128-bit) - must match Android/iOS
/// f47a0003-58cc-4372-a567-0e02b2c3d479
pub const DOC_CHAR_UUID: [u8; 16] = [
    0x79, 0xd4, 0xc3, 0xb2, 0x02, 0x0e, 0x67, 0xa5,
    0x72, 0x43, 0xcc, 0x58, 0x03, 0x00, 0x7a, 0xf4
];

/// 16-bit alias for advertising (Android scans for this too)
pub const HIVE_SERVICE_UUID_16: u16 = 0xF47A;

/// Maximum document size
const MAX_DOC_SIZE: usize = 256;

/// Maximum simultaneous connections (for mesh gossip)
const MAX_CONNECTIONS: usize = 4;

/// Connection info for each peer
#[derive(Clone, Copy, Default)]
struct PeerConnection {
    handle: u16,
    peer_doc_handle: u16,  // Their document characteristic (for writing)
    active: bool,
    peer_addr: [u8; 6],    // Peer's BLE address (for disconnect tracking)
    node_id: u32,          // HIVE Node ID (set when first document received)
}

/// Multi-connection state
static CONNECTIONS: Mutex<[PeerConnection; MAX_CONNECTIONS]> = Mutex::new([PeerConnection { handle: 0xFFFF, peer_doc_handle: 0, active: false, peer_addr: [0u8; 6], node_id: 0 }; MAX_CONNECTIONS]);

/// Recently disconnected node IDs (for main loop to update display)
static DISCONNECTED_NODE_IDS: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static NUM_CONNECTIONS: AtomicU16 = AtomicU16::new(0);

/// Legacy single-connection state (for compatibility)
static CONNECTED: AtomicBool = AtomicBool::new(false);
static CONN_HANDLE: AtomicU16 = AtomicU16::new(0xFFFF);

/// Track when we connected (for rotation timeout)
static CONNECT_TIME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Sync completed flag
static SYNC_COMPLETE: AtomicBool = AtomicBool::new(false);

/// Document characteristic value handle (set during registration)
static DOC_CHAR_HANDLE: AtomicU16 = AtomicU16::new(0);

/// Peer's document characteristic handle (discovered via GATT) - for current discovery
static PEER_DOC_HANDLE: AtomicU16 = AtomicU16::new(0);

/// Connection currently being discovered
static DISCOVERING_CONN: AtomicU16 = AtomicU16::new(0xFFFF);

/// Shared document buffer for GATT access
static DOC_BUFFER: Mutex<[u8; MAX_DOC_SIZE]> = Mutex::new([0u8; MAX_DOC_SIZE]);
static DOC_LEN: AtomicU16 = AtomicU16::new(0);

/// Pending received document (set by GATT callback, read by main loop)
static PENDING_DOC: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Debug: Count of GATT writes received (to verify callback is running)
static GATT_WRITE_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Debug: Count of documents successfully stored in PENDING_DOC
static DOC_STORED_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Debug: Count of documents taken from PENDING_DOC
static DOC_TAKEN_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Connection state change flag
static CONNECTION_CHANGED: AtomicBool = AtomicBool::new(false);

/// Flag to track if we're currently connecting (to avoid multiple connection attempts)
static CONNECTING: AtomicBool = AtomicBool::new(false);

/// Our MAC address (for connection arbitration - only connect to higher MACs)
static OUR_MAC: Mutex<[u8; 6]> = Mutex::new([0u8; 6]);

/// Peer sync tracking - MAC address -> last sync timestamp
/// Max 8 peers tracked
const MAX_TRACKED_PEERS: usize = 8;
static PEER_SYNC_TIMES: Mutex<[([u8; 6], u32); MAX_TRACKED_PEERS]> = Mutex::new([([0u8; 6], 0); MAX_TRACKED_PEERS]);

/// Current peer MAC (the one we're connected/connecting to)
static CURRENT_PEER_MAC: Mutex<[u8; 6]> = Mutex::new([0u8; 6]);

/// Our document version when we last sent (to avoid redundant sends)
static LAST_SENT_VERSION: AtomicU16 = AtomicU16::new(0);

/// Record when we synced with a peer
fn record_peer_sync(mac: &[u8; 6], timestamp: u32) {
    if let Ok(mut peers) = PEER_SYNC_TIMES.lock() {
        // Find existing entry or oldest slot
        let mut oldest_idx = 0;
        let mut oldest_time = u32::MAX;
        for (i, (peer_mac, sync_time)) in peers.iter().enumerate() {
            if peer_mac == mac {
                // Update existing
                peers[i].1 = timestamp;
                return;
            }
            if *sync_time < oldest_time {
                oldest_time = *sync_time;
                oldest_idx = i;
            }
        }
        // Use oldest slot for new peer
        peers[oldest_idx] = (*mac, timestamp);
    }
}

/// Get last sync time for a peer (0 if never synced)
fn get_peer_last_sync(mac: &[u8; 6]) -> u32 {
    if let Ok(peers) = PEER_SYNC_TIMES.lock() {
        for (peer_mac, sync_time) in peers.iter() {
            if peer_mac == mac {
                return *sync_time;
            }
        }
    }
    0
}

/// Check if advertising data contains HIVE service UUID (16-bit or 128-bit)
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
                if uuid == HIVE_SERVICE_UUID_16 {
                    return true;
                }
                j += 2;
            }
        }

        // Check for Complete or Incomplete 128-bit Service UUIDs (0x07 or 0x06)
        if (field_type == 0x06 || field_type == 0x07) && field_len >= 17 {
            // Parse 128-bit UUIDs
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

                // Get peer address from CURRENT_PEER_MAC (stored during discovery)
                let peer_addr = if let Ok(mac) = CURRENT_PEER_MAC.lock() {
                    *mac
                } else {
                    [0u8; 6]
                };

                // Add to multi-connection list
                if let Ok(mut conns) = CONNECTIONS.lock() {
                    for conn in conns.iter_mut() {
                        if !conn.active {
                            conn.handle = connect.conn_handle;
                            conn.peer_doc_handle = 0;
                            conn.active = true;
                            conn.peer_addr = peer_addr;
                            NUM_CONNECTIONS.fetch_add(1, Ordering::SeqCst);
                            break;
                        }
                    }
                }

                // Legacy single-connection tracking
                CONNECTED.store(true, Ordering::SeqCst);
                CONN_HANDLE.store(connect.conn_handle, Ordering::SeqCst);
                CONNECTION_CHANGED.store(true, Ordering::SeqCst);
                SYNC_COMPLETE.store(false, Ordering::SeqCst);
                CONNECT_TIME.store(esp_idf_svc::sys::esp_timer_get_time() as u32 / 1_000_000, Ordering::SeqCst);

                // Track which connection we're discovering
                DISCOVERING_CONN.store(connect.conn_handle, Ordering::SeqCst);

                // Request MTU exchange for larger payloads
                let ret = ble_gattc_exchange_mtu(connect.conn_handle, Some(mtu_exchange_cb), ptr::null_mut());
                if ret != 0 {
                    warn!("BLE: MTU exchange failed to start: {}", ret);
                    let ret = ble_gattc_disc_all_svcs(connect.conn_handle, Some(gatt_disc_svc_cb), ptr::null_mut());
                    if ret != 0 {
                        warn!("BLE: Failed to start service discovery: {}", ret);
                    }
                }

                // Keep scanning for more peers (mesh!)
                let _ = start_scanning();
            } else {
                warn!("BLE: Connection failed, status={}", connect.status);
                let _ = start_scanning();
            }
        }
        BLE_GAP_EVENT_DISCONNECT => {
            let disconnect = &event.__bindgen_anon_1.disconnect;
            let disc_handle = disconnect.conn.conn_handle;
            info!("BLE: Disconnected, handle={}", disc_handle);

            // Remove from multi-connection list and track disconnected node ID
            if let Ok(mut conns) = CONNECTIONS.lock() {
                for conn in conns.iter_mut() {
                    if conn.active && conn.handle == disc_handle {
                        // Add to disconnected node IDs for main loop to update display
                        if conn.node_id != 0 {
                            if let Ok(mut disconnected) = DISCONNECTED_NODE_IDS.lock() {
                                disconnected.push(conn.node_id);
                                info!("BLE: Tracking disconnect of node {:08X}", conn.node_id);
                            }
                        }
                        conn.active = false;
                        conn.handle = 0xFFFF;
                        conn.peer_doc_handle = 0;
                        conn.peer_addr = [0u8; 6];
                        conn.node_id = 0;
                        NUM_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
                        break;
                    }
                }
            }

            // Update legacy tracking
            let remaining = NUM_CONNECTIONS.load(Ordering::SeqCst);
            CONNECTED.store(remaining > 0, Ordering::SeqCst);
            if remaining == 0 {
                CONN_HANDLE.store(0xFFFF, Ordering::SeqCst);
            }
            CONNECTION_CHANGED.store(true, Ordering::SeqCst);
            LAST_SENT_VERSION.store(0, Ordering::SeqCst);

            // Keep advertising and scanning
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
                // Get peer MAC address
                let peer_mac = disc.addr.val;

                // Check cooldown - prefer peers we haven't synced with recently
                let now = esp_idf_svc::sys::esp_timer_get_time() as u32 / 1_000_000;
                let last_sync = get_peer_last_sync(&peer_mac);
                let since_sync = now.saturating_sub(last_sync);

                // 30 second cooldown per peer - prevents thrashing
                let in_cooldown = last_sync > 0 && since_sync < 30;

                if in_cooldown {
                    debug!("BLE: Peer in cooldown (synced {}s ago)", since_sync);
                } else if !CONNECTED.load(Ordering::SeqCst) && !CONNECTING.load(Ordering::SeqCst) {
                    info!("BLE: Found HIVE peer (last sync {}s ago), connecting...", since_sync);
                    CONNECTING.store(true, Ordering::SeqCst);

                    // Store current peer MAC
                    if let Ok(mut current) = CURRENT_PEER_MAC.lock() {
                        *current = peer_mac;
                    }

                    // Stop scanning before connecting
                    ble_gap_disc_cancel();

                    let ret = ble_gap_connect(
                        BLE_OWN_ADDR_PUBLIC as u8,
                        &disc.addr,
                        10000, // 10 second timeout
                        ptr::null(),
                        Some(gap_event_handler),
                        ptr::null_mut(),
                    );
                    if ret != 0 {
                        // Error 14 = BLE_HS_EBUSY (already connecting/connected) - not critical
                        if ret != 14 {
                            warn!("BLE: ble_gap_connect failed: {}", ret);
                        }
                        CONNECTING.store(false, Ordering::SeqCst);
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
        let peer_handle = PEER_DOC_HANDLE.load(Ordering::SeqCst);
        if peer_handle != 0 {
            // Discover descriptors to find the actual CCCD handle
            info!("BLE: Discovering descriptors for characteristic handle {}", peer_handle);
            let ret = ble_gattc_disc_all_dscs(
                conn_handle,
                peer_handle,
                peer_handle + 10,  // Search up to 10 handles ahead
                Some(gatt_disc_dsc_cb),
                ptr::null_mut(),
            );
            if ret != 0 {
                warn!("BLE: Failed to start descriptor discovery: {}, falling back to handle+1", ret);
                // Fallback: try handle+1 as the CCCD
                subscribe_to_notifications(conn_handle, peer_handle + 1);
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
        // Check if this is the HIVE document characteristic (128-bit or 16-bit)
        let is_hive_char = if c.uuid.u.type_ == BLE_UUID_TYPE_128 as u8 {
            let uuid128 = &*((&c.uuid) as *const ble_uuid_any_t as *const ble_uuid128_t);
            uuid128.value == DOC_CHAR_UUID
        } else if c.uuid.u.type_ == BLE_UUID_TYPE_16 as u8 {
            // Also check for 16-bit alias (0xF47B) for legacy devices
            let uuid16 = &*((&c.uuid) as *const ble_uuid_any_t as *const ble_uuid16_t);
            uuid16.value == 0xF47B
        } else {
            false
        };

        if is_hive_char {
            info!("BLE: Found HIVE document characteristic, handle={}", c.val_handle);
            PEER_DOC_HANDLE.store(c.val_handle, Ordering::SeqCst);

            // Store in the connection struct
            let disc_conn = DISCOVERING_CONN.load(Ordering::SeqCst);
            if let Ok(mut conns) = CONNECTIONS.lock() {
                for conn in conns.iter_mut() {
                    if conn.active && conn.handle == disc_conn {
                        conn.peer_doc_handle = c.val_handle;
                        info!("BLE: Stored peer doc handle {} for conn {}", c.val_handle, disc_conn);
                        break;
                    }
                }
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

/// CCCD UUID (Client Characteristic Configuration Descriptor)
const BLE_GATT_DSC_CLT_CFG_UUID16: u16 = 0x2902;

/// Helper to subscribe to notifications on a given CCCD handle
unsafe fn subscribe_to_notifications(conn_handle: u16, cccd_handle: u16) {
    let notify_enable: [u8; 2] = [0x01, 0x00]; // Enable notifications
    info!("BLE: Subscribing to notifications on CCCD handle {}", cccd_handle);
    let ret = ble_gattc_write_flat(
        conn_handle,
        cccd_handle,
        notify_enable.as_ptr() as *const c_void,
        2,
        Some(gatt_write_cb),
        ptr::null_mut(),
    );
    if ret != 0 {
        warn!("BLE: Failed to subscribe to notifications: {}", ret);
    }

    // Also read the peer's document
    let peer_handle = PEER_DOC_HANDLE.load(Ordering::SeqCst);
    if peer_handle != 0 {
        info!("BLE: Reading peer document from handle {}", peer_handle);
        let ret = ble_gattc_read(conn_handle, peer_handle, Some(gatt_read_cb), ptr::null_mut());
        if ret != 0 {
            warn!("BLE: Failed to read peer document: {}", ret);
        }
    }
}

/// GATT descriptor discovery callback - finds CCCD for notification subscription
unsafe extern "C" fn gatt_disc_dsc_cb(
    conn_handle: u16,
    error: *const ble_gatt_error,
    chr_val_handle: u16,
    dsc: *const ble_gatt_dsc,
    _arg: *mut c_void,
) -> c_int {
    if error.is_null() {
        return 0;
    }

    let err = &*error;
    if err.status == BLE_HS_EDONE as u16 {
        info!("BLE: Descriptor discovery complete");
        return 0;
    }

    if err.status != 0 {
        warn!("BLE: Descriptor discovery error: {}", err.status);
        return 0;
    }

    if !dsc.is_null() {
        let d = &*dsc;
        // Check if this is the CCCD (UUID 0x2902)
        if d.uuid.u.type_ == BLE_UUID_TYPE_16 as u8 {
            let uuid16 = &*((&d.uuid) as *const ble_uuid_any_t as *const ble_uuid16_t);
            if uuid16.value == BLE_GATT_DSC_CLT_CFG_UUID16 {
                info!("BLE: Found CCCD at handle {} (chr_val={})", d.handle, chr_val_handle);
                subscribe_to_notifications(conn_handle, d.handle);
            }
        }
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

    // After reading peer's document, write our document to peer (only if we have newer data)
    let peer_handle = PEER_DOC_HANDLE.load(Ordering::SeqCst);
    if peer_handle != 0 {
        if let Ok(doc) = DOC_BUFFER.lock() {
            let len = DOC_LEN.load(Ordering::SeqCst) as usize;
            // Get version from our doc (first 4 bytes are version)
            let our_version = if len >= 4 {
                u32::from_le_bytes([doc[0], doc[1], doc[2], doc[3]])
            } else {
                0
            };
            let last_sent = LAST_SENT_VERSION.load(Ordering::SeqCst) as u32;

            if len > 0 && our_version > last_sent {
                let om = ble_hs_mbuf_from_flat(doc.as_ptr() as *const c_void, len as u16);
                if !om.is_null() {
                    info!("BLE: Sending doc v{} to peer ({} bytes)", our_version, len);
                    let ret = ble_gattc_write_no_rsp(conn_handle, peer_handle, om);
                    if ret != 0 {
                        warn!("BLE: Write failed: {}", ret);
                        os_mbuf_free_chain(om);
                    } else {
                        LAST_SENT_VERSION.store(our_version as u16, Ordering::SeqCst);
                        info!("BLE: Sync complete");
                        SYNC_COMPLETE.store(true, Ordering::SeqCst);
                        if let Ok(current) = CURRENT_PEER_MAC.lock() {
                            let now = esp_idf_svc::sys::esp_timer_get_time() as u32 / 1_000_000;
                            record_peer_sync(&*current, now);
                        }
                    }
                }
            } else {
                // Nothing new to send, but still mark sync complete
                debug!("BLE: No new data to send (v{} <= last sent v{})", our_version, last_sent);
                SYNC_COMPLETE.store(true, Ordering::SeqCst);
                if let Ok(current) = CURRENT_PEER_MAC.lock() {
                    let now = esp_idf_svc::sys::esp_timer_get_time() as u32 / 1_000_000;
                    record_peer_sync(&*current, now);
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
            GATT_WRITE_COUNT.fetch_add(1, Ordering::SeqCst);
            info!("BLE: GATT write #{}", GATT_WRITE_COUNT.load(Ordering::SeqCst));
            let om = ctxt.om;
            if !om.is_null() {
                // Get total length across all mbuf segments using OS_MBUF_PKTLEN
                let len = os_mbuf_len(om) as usize;
                info!("BLE: Write len={}", len);
                if len > 0 && len <= MAX_DOC_SIZE {
                    let mut buf = vec![0u8; len];
                    // Copy all data from mbuf chain
                    let ret = os_mbuf_copydata(om, 0, len as i32, buf.as_mut_ptr() as *mut c_void);
                    if ret == 0 {
                        // Store in pending queue
                        if let Ok(mut pending) = PENDING_DOC.lock() {
                            *pending = Some(buf);
                            DOC_STORED_COUNT.fetch_add(1, Ordering::SeqCst);
                            info!("BLE: Stored {} bytes (total stored: {})", len, DOC_STORED_COUNT.load(Ordering::SeqCst));
                        } else {
                            warn!("BLE: Failed to lock PENDING_DOC!");
                        }
                    } else {
                        warn!("BLE: Failed to copy mbuf data: {}", ret);
                    }
                } else {
                    warn!("BLE: Invalid write len={}", len);
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
static mut SVC_UUID: ble_uuid128_t = unsafe { core::mem::zeroed() };
static mut CHR_UUID: ble_uuid128_t = unsafe { core::mem::zeroed() };
/// Device name for advertising (e.g., "HIVE_DEMO-12345678")
static mut DEVICE_NAME: [u8; 20] = [0; 20];
static mut DEVICE_NAME_LEN: u8 = 0;

/// Initialize NimBLE stack
pub fn init(node_id: NodeId) -> Result<(), i32> {
    unsafe {
        info!("BLE: Initializing NimBLE");

        // Build device name from node ID (e.g., "HIVE_DEMO-12345678")
        let name = format!("HIVE_DEMO-{:08X}", node_id.as_u32());
        let name_bytes = name.as_bytes();
        let len = name_bytes.len().min(DEVICE_NAME.len());
        DEVICE_NAME[..len].copy_from_slice(&name_bytes[..len]);
        DEVICE_NAME_LEN = len as u8;
        info!("BLE: Device name: {}", name);

        // Store our MAC for connection arbitration
        let mut mac = [0u8; 6];
        esp_idf_svc::sys::esp_efuse_mac_get_default(mac.as_mut_ptr());
        if let Ok(mut our_mac) = OUR_MAC.lock() {
            *our_mac = mac;
        }
        info!("BLE: Our MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
              mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

        // Initialize NimBLE
        let ret = nimble_port_init();
        if ret != ESP_OK {
            error!("BLE: nimble_port_init failed: {}", ret);
            return Err(ret);
        }

        // Configure host
        ble_hs_cfg.sync_cb = Some(on_sync);
        ble_hs_cfg.reset_cb = Some(on_reset);

        // Set preferred ATT MTU to 128 bytes
        // Our documents can be ~70-100 bytes with Peripheral event data
        let ret = ble_att_set_preferred_mtu(128);
        if ret != 0 {
            warn!("BLE: Failed to set preferred MTU: {}", ret);
        } else {
            info!("BLE: Preferred MTU set to 128");
        }

        // Set up service UUID (128-bit)
        SVC_UUID.u.type_ = BLE_UUID_TYPE_128 as u8;
        SVC_UUID.value = HIVE_SERVICE_UUID;

        // Set up characteristic UUID (128-bit)
        CHR_UUID.u.type_ = BLE_UUID_TYPE_128 as u8;
        CHR_UUID.value = DOC_CHAR_UUID;

        // Configure document characteristic
        GATT_CHARS[0].uuid = &raw const CHR_UUID.u as *const _;
        GATT_CHARS[0].access_cb = Some(gatt_access_cb);
        GATT_CHARS[0].flags =
            (BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_WRITE | BLE_GATT_CHR_F_WRITE_NO_RSP | BLE_GATT_CHR_F_NOTIFY) as ble_gatt_chr_flags;
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

        // Build advertising data with 128-bit UUID (standard across all platforms)
        let mut fields: ble_hs_adv_fields = core::mem::zeroed();
        fields.flags = (BLE_HS_ADV_F_DISC_GEN | BLE_HS_ADV_F_BREDR_UNSUP) as u8;
        fields.uuids128 = &raw const SVC_UUID as *const _ as *mut ble_uuid128_t;
        fields.num_uuids128 = 1;
        fields.set_uuids128_is_complete(1);

        let ret = ble_gap_adv_set_fields(&fields);
        if ret != 0 {
            error!("BLE: ble_gap_adv_set_fields failed: {}", ret);
            return Err(ret);
        }

        // Set scan response with device name
        let mut rsp_fields: ble_hs_adv_fields = core::mem::zeroed();
        rsp_fields.name = DEVICE_NAME.as_ptr();
        rsp_fields.name_len = DEVICE_NAME_LEN;
        rsp_fields.set_name_is_complete(1);

        let ret = ble_gap_adv_rsp_set_fields(&rsp_fields);
        if ret != 0 {
            warn!("BLE: ble_gap_adv_rsp_set_fields failed: {}", ret);
            // Continue anyway - name is optional
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
        if ret != 0 && ret != 2 {
            // Ignore error 2 (BLE_HS_EALREADY - already scanning)
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
        info!("BLE: notify_document - not connected");
        return Err(-1); // Not connected
    }

    // Update local buffer first
    set_document(data);

    let our_handle = DOC_CHAR_HANDLE.load(Ordering::SeqCst);
    let peer_handle = PEER_DOC_HANDLE.load(Ordering::SeqCst);
    info!("BLE: notify_document - conn={}, our_handle={}, peer_handle={}, len={}",
          conn, our_handle, peer_handle, data.len());

    // Try to notify (works if we're the peripheral)
    if our_handle != 0 {
        unsafe {
            let ret = ble_gatts_notify(conn, our_handle);
            if ret == 0 {
                info!("BLE: Notified via GATT");
            } else {
                warn!("BLE: Notify failed: {}", ret);
            }
        }
    }

    // Also write to peer (works if we're the central and discovered their characteristic)
    if peer_handle != 0 {
        unsafe {
            let om = ble_hs_mbuf_from_flat(data.as_ptr() as *const c_void, data.len() as u16);
            if !om.is_null() {
                let ret = ble_gattc_write_no_rsp(conn, peer_handle, om);
                if ret == 0 {
                    info!("BLE: Wrote to peer via GATT");
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
        if let Some(doc) = pending.take() {
            DOC_TAKEN_COUNT.fetch_add(1, Ordering::SeqCst);
            Some(doc)
        } else {
            None
        }
    } else {
        None
    }
}

/// Get count of GATT writes received (debug)
pub fn gatt_write_count() -> u32 {
    GATT_WRITE_COUNT.load(Ordering::SeqCst)
}

/// Get count of documents stored (debug)
pub fn doc_stored_count() -> u32 {
    DOC_STORED_COUNT.load(Ordering::SeqCst)
}

/// Get count of documents taken (debug)
pub fn doc_taken_count() -> u32 {
    DOC_TAKEN_COUNT.load(Ordering::SeqCst)
}

/// Check if connection state changed (and clear the flag)
pub fn take_connection_changed() -> bool {
    CONNECTION_CHANGED.swap(false, Ordering::SeqCst)
}

/// Check if connected to a peer
pub fn is_connected() -> bool {
    CONNECTED.load(Ordering::SeqCst)
}

/// Check if we should rotate to find other peers (call this periodically)
pub fn check_rotation() -> bool {
    false // No rotation needed - using multi-connection gossip instead
}

/// Gossip document to ALL connected peers (multi-hop mesh sync)
pub fn gossip_document(data: &[u8]) -> usize {
    let mut sent_count = 0;

    // Update local buffer (for GATT reads)
    set_document(data);

    // Send via notification to all peers (we're peripheral)
    // Use ble_gatts_notify_custom to send actual data, not just trigger a read
    let our_handle = DOC_CHAR_HANDLE.load(Ordering::SeqCst);
    if our_handle != 0 {
        if let Ok(conns) = CONNECTIONS.lock() {
            for conn in conns.iter() {
                if conn.active {
                    unsafe {
                        // Create mbuf with the data to send
                        let om = ble_hs_mbuf_from_flat(data.as_ptr() as *const c_void, data.len() as u16);
                        if !om.is_null() {
                            let ret = ble_gatts_notify_custom(conn.handle, our_handle, om);
                            if ret == 0 {
                                info!("BLE: Gossiped via notify to conn {} ({} bytes)", conn.handle, data.len());
                                sent_count += 1;
                            } else {
                                warn!("BLE: Notify failed: {} (conn={}, handle={})", ret, conn.handle, our_handle);
                                os_mbuf_free_chain(om);
                            }
                        } else {
                            warn!("BLE: Failed to create mbuf for notify");
                        }
                    }
                }
            }
        }
    } else {
        warn!("BLE: our_handle is 0, cannot send notifications");
    }

    // Also write to peers where we're the central
    if let Ok(conns) = CONNECTIONS.lock() {
        for conn in conns.iter() {
            if conn.active && conn.peer_doc_handle != 0 {
                unsafe {
                    let om = ble_hs_mbuf_from_flat(data.as_ptr() as *const c_void, data.len() as u16);
                    if !om.is_null() {
                        let ret = ble_gattc_write_no_rsp(conn.handle, conn.peer_doc_handle, om);
                        if ret == 0 {
                            info!("BLE: Gossiped via write to conn {} handle {} ({} bytes)", conn.handle, conn.peer_doc_handle, data.len());
                            sent_count += 1;
                        } else {
                            warn!("BLE: Write failed: {}", ret);
                            os_mbuf_free_chain(om);
                        }
                    }
                }
            }
        }
    }

    info!("BLE: Gossiped to {} peers total", sent_count);
    sent_count
}

/// Get number of active connections
pub fn connection_count() -> usize {
    NUM_CONNECTIONS.load(Ordering::SeqCst) as usize
}

/// Get and clear list of recently disconnected node IDs
pub fn take_disconnected_node_ids() -> Vec<u32> {
    if let Ok(mut disconnected) = DISCONNECTED_NODE_IDS.lock() {
        let ids = disconnected.clone();
        disconnected.clear();
        ids
    } else {
        Vec::new()
    }
}

/// Associate a node ID with a connection (called when first document received)
pub fn set_connection_node_id(node_id: u32) {
    // Find the first active connection without a node_id set
    if let Ok(mut conns) = CONNECTIONS.lock() {
        for conn in conns.iter_mut() {
            if conn.active && conn.node_id == 0 {
                conn.node_id = node_id;
                info!("BLE: Associated connection with node {:08X}", node_id);
                break;
            }
        }
    }
}

/// Get list of currently connected node IDs
pub fn get_connected_node_ids() -> Vec<u32> {
    if let Ok(conns) = CONNECTIONS.lock() {
        conns.iter()
            .filter(|c| c.active && c.node_id != 0)
            .map(|c| c.node_id)
            .collect()
    } else {
        Vec::new()
    }
}
