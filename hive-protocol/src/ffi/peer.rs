//! FFI peer management API (Issue #258)
//!
//! Provides C-compatible FFI bindings for managing peer connections.
//! Designed for ATAK Android integration via JNI.
//!
//! ## Thread Safety
//!
//! All functions are thread-safe. The global state is protected by RwLock.
//! Callbacks may be invoked from the async runtime thread.
//!
//! ## Error Codes
//!
//! - `HIVE_OK` (0): Operation succeeded
//! - `HIVE_ERR_NOT_INITIALIZED` (-1): HIVE not initialized
//! - `HIVE_ERR_INVALID_PEER` (-2): Invalid peer ID
//! - `HIVE_ERR_CONNECTION_FAILED` (-3): Connection failed
//! - `HIVE_ERR_ALREADY_CONNECTED` (-4): Already connected to peer
//! - `HIVE_ERR_NOT_CONNECTED` (-5): Not connected to peer
//! - `HIVE_ERR_INVALID_ARGUMENT` (-6): Invalid argument (null pointer)
//! - `HIVE_ERR_INTERNAL` (-99): Internal error

use crate::transport::{MeshTransport, NodeId, PeerEvent};
use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

// =============================================================================
// Error Codes
// =============================================================================

/// Operation succeeded
pub const HIVE_OK: c_int = 0;
/// HIVE not initialized
pub const HIVE_ERR_NOT_INITIALIZED: c_int = -1;
/// Invalid peer ID
pub const HIVE_ERR_INVALID_PEER: c_int = -2;
/// Connection failed
pub const HIVE_ERR_CONNECTION_FAILED: c_int = -3;
/// Already connected to peer
pub const HIVE_ERR_ALREADY_CONNECTED: c_int = -4;
/// Not connected to peer
pub const HIVE_ERR_NOT_CONNECTED: c_int = -5;
/// Invalid argument (null pointer)
pub const HIVE_ERR_INVALID_ARGUMENT: c_int = -6;
/// Internal error
pub const HIVE_ERR_INTERNAL: c_int = -99;

// =============================================================================
// Event Types (for callbacks)
// =============================================================================

/// Peer connected event type
pub const HIVE_EVENT_CONNECTED: c_int = 1;
/// Peer disconnected event type
pub const HIVE_EVENT_DISCONNECTED: c_int = 2;
/// Peer connection degraded event type
pub const HIVE_EVENT_DEGRADED: c_int = 3;

// =============================================================================
// Global State
// =============================================================================

/// Global transport instance (set during initialization)
static GLOBAL_TRANSPORT: OnceLock<Arc<dyn MeshTransport>> = OnceLock::new();

/// Last error message
static LAST_ERROR: RwLock<Option<String>> = RwLock::new(None);

/// Registered peer event callback
static PEER_CALLBACK: RwLock<Option<PeerEventCallback>> = RwLock::new(None);

/// Cancellation token for the callback thread
static CALLBACK_CANCEL: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn get_or_init_cancel_token() -> &'static Arc<AtomicBool> {
    CALLBACK_CANCEL.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

/// Type for peer event callbacks
///
/// # Arguments
/// - `event_type`: Event type (HIVE_EVENT_CONNECTED, HIVE_EVENT_DISCONNECTED, HIVE_EVENT_DEGRADED)
/// - `peer_id`: Peer ID as null-terminated string
/// - `reason`: Reason/details as null-terminated string (may be null)
pub type PeerEventCallback =
    extern "C" fn(event_type: c_int, peer_id: *const c_char, reason: *const c_char);

// =============================================================================
// JSON Response Types
// =============================================================================

/// Peer information returned by get_connected_peers
#[derive(Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub connected_since: String, // ISO 8601 format
    pub status: String,          // "healthy", "degraded", "suspect", "dead"
}

/// Response for get_connected_peers
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectedPeersResponse {
    pub peers: Vec<PeerInfo>,
}

/// Response for get_peer_status
#[derive(Debug, Serialize, Deserialize)]
pub struct PeerStatusResponse {
    pub peer_id: String,
    pub connected: bool,
    pub connection_type: String, // "quic", "ditto", "unknown"
}

/// Response for get_peer_health
#[derive(Debug, Serialize, Deserialize)]
pub struct PeerHealthResponse {
    pub peer_id: String,
    pub rtt_ms: u32,
    pub packet_loss_percent: u8,
    pub state: String, // "healthy", "degraded", "suspect", "dead"
}

// =============================================================================
// Initialization
// =============================================================================

/// Initialize the FFI layer with a MeshTransport implementation.
///
/// This must be called before any other FFI functions.
/// Typically called during application startup after creating the transport.
///
/// # Safety
///
/// The transport must remain valid for the lifetime of the FFI usage.
pub fn initialize_ffi(transport: Arc<dyn MeshTransport>) -> Result<(), &'static str> {
    GLOBAL_TRANSPORT
        .set(transport)
        .map_err(|_| "FFI already initialized")
}

/// Check if FFI is initialized
pub fn is_initialized() -> bool {
    GLOBAL_TRANSPORT.get().is_some()
}

fn get_transport() -> Option<&'static Arc<dyn MeshTransport>> {
    GLOBAL_TRANSPORT.get()
}

fn set_last_error(error: String) {
    if let Ok(mut last) = LAST_ERROR.write() {
        *last = Some(error);
    }
}

// =============================================================================
// FFI Functions
// =============================================================================

/// Get the last error message.
///
/// Returns a null-terminated string that must be freed with `hive_free_string`.
/// Returns NULL if no error has occurred.
///
/// # Safety
///
/// The returned string must be freed with `hive_free_string`.
#[no_mangle]
pub extern "C" fn hive_get_last_error() -> *mut c_char {
    let error = LAST_ERROR.read().ok().and_then(|e| e.clone());
    match error {
        Some(msg) => CString::new(msg)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// Free a string returned by HIVE FFI functions.
///
/// # Safety
///
/// The pointer must have been returned by a HIVE FFI function.
/// Do not call this on the same pointer twice.
#[no_mangle]
pub unsafe extern "C" fn hive_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// Get list of connected peers as JSON.
///
/// Returns a JSON string with the following format:
/// ```json
/// {
///   "peers": [
///     {
///       "peer_id": "node-alpha",
///       "connected_since": "2025-11-21T10:30:00Z",
///       "status": "healthy"
///     }
///   ]
/// }
/// ```
///
/// # Safety
///
/// The returned string must be freed with `hive_free_string`.
#[no_mangle]
pub extern "C" fn hive_get_connected_peers() -> *mut c_char {
    let transport = match get_transport() {
        Some(t) => t,
        None => {
            set_last_error("HIVE not initialized".to_string());
            return std::ptr::null_mut();
        }
    };

    let peer_ids = transport.connected_peers();

    let peers: Vec<PeerInfo> = peer_ids
        .into_iter()
        .map(|peer_id| {
            let (status, connected_since) = transport
                .get_connection(&peer_id)
                .map(|c| {
                    let status = if c.is_alive() { "healthy" } else { "dead" };
                    // Calculate connected_since from Instant
                    let duration_ago = c.connected_at().elapsed();
                    let connected_at = chrono::Utc::now()
                        - chrono::Duration::from_std(duration_ago).unwrap_or_default();
                    (status, connected_at.to_rfc3339())
                })
                .unwrap_or(("unknown", chrono::Utc::now().to_rfc3339()));

            PeerInfo {
                peer_id: peer_id.to_string(),
                connected_since,
                status: status.to_string(),
            }
        })
        .collect();

    let response = ConnectedPeersResponse { peers };
    match serde_json::to_string(&response) {
        Ok(json) => CString::new(json)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            set_last_error(format!("JSON serialization failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Get connection status for a specific peer.
///
/// # Arguments
///
/// * `peer_id` - Null-terminated peer ID string
///
/// # Returns
///
/// JSON string with status, or NULL on error.
///
/// # Safety
///
/// - `peer_id` must be a valid null-terminated string
/// - The returned string must be freed with `hive_free_string`
#[no_mangle]
pub unsafe extern "C" fn hive_get_peer_status(peer_id: *const c_char) -> *mut c_char {
    if peer_id.is_null() {
        set_last_error("peer_id is null".to_string());
        return std::ptr::null_mut();
    }

    let peer_id_str = match CStr::from_ptr(peer_id).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("Invalid UTF-8 in peer_id".to_string());
            return std::ptr::null_mut();
        }
    };

    let transport = match get_transport() {
        Some(t) => t,
        None => {
            set_last_error("HIVE not initialized".to_string());
            return std::ptr::null_mut();
        }
    };

    let node_id = NodeId::new(peer_id_str.to_string());
    let connected = transport.is_connected(&node_id);

    let response = PeerStatusResponse {
        peer_id: peer_id_str.to_string(),
        connected,
        connection_type: if connected { "quic" } else { "unknown" }.to_string(),
    };

    match serde_json::to_string(&response) {
        Ok(json) => CString::new(json)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            set_last_error(format!("JSON serialization failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Get health metrics for a peer connection.
///
/// # Arguments
///
/// * `peer_id` - Null-terminated peer ID string
///
/// # Returns
///
/// JSON string with health metrics, or NULL on error.
///
/// # Safety
///
/// - `peer_id` must be a valid null-terminated string
/// - The returned string must be freed with `hive_free_string`
#[no_mangle]
pub unsafe extern "C" fn hive_get_peer_health(peer_id: *const c_char) -> *mut c_char {
    if peer_id.is_null() {
        set_last_error("peer_id is null".to_string());
        return std::ptr::null_mut();
    }

    let peer_id_str = match CStr::from_ptr(peer_id).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("Invalid UTF-8 in peer_id".to_string());
            return std::ptr::null_mut();
        }
    };

    let transport = match get_transport() {
        Some(t) => t,
        None => {
            set_last_error("HIVE not initialized".to_string());
            return std::ptr::null_mut();
        }
    };

    let node_id = NodeId::new(peer_id_str.to_string());

    // Check if peer is connected first
    if !transport.is_connected(&node_id) {
        set_last_error(format!("Peer not found: {}", peer_id_str));
        return std::ptr::null_mut();
    }

    let health = transport.get_peer_health(&node_id);

    let response = match health {
        Some(h) => PeerHealthResponse {
            peer_id: peer_id_str.to_string(),
            rtt_ms: h.rtt_ms,
            packet_loss_percent: h.packet_loss_percent,
            state: format!("{}", h.state),
        },
        None => {
            // Peer is connected but no health data available - return defaults
            PeerHealthResponse {
                peer_id: peer_id_str.to_string(),
                rtt_ms: 0,
                packet_loss_percent: 0,
                state: "unknown".to_string(),
            }
        }
    };

    match serde_json::to_string(&response) {
        Ok(json) => CString::new(json)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            set_last_error(format!("JSON serialization failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Register a callback for peer events.
///
/// The callback will be invoked when peers connect, disconnect, or become degraded.
///
/// # Arguments
///
/// * `callback` - Function pointer for handling events (may be NULL to unregister)
///
/// # Returns
///
/// - `HIVE_OK` on success
/// - `HIVE_ERR_NOT_INITIALIZED` if HIVE not initialized
///
/// # Callback Signature
///
/// ```c
/// void callback(int event_type, const char* peer_id, const char* reason);
/// ```
///
/// Event types:
/// - `HIVE_EVENT_CONNECTED` (1): Peer connected
/// - `HIVE_EVENT_DISCONNECTED` (2): Peer disconnected
/// - `HIVE_EVENT_DEGRADED` (3): Connection quality degraded
///
/// # Safety
///
/// The callback must be thread-safe as it may be called from any thread.
#[no_mangle]
pub extern "C" fn hive_register_peer_callback(callback: Option<PeerEventCallback>) -> c_int {
    let transport = match get_transport() {
        Some(t) => t,
        None => {
            set_last_error("HIVE not initialized".to_string());
            return HIVE_ERR_NOT_INITIALIZED;
        }
    };

    // Reset cancellation token for new callback
    let cancel_token = get_or_init_cancel_token();
    cancel_token.store(false, Ordering::SeqCst);

    // Store the callback
    if let Ok(mut cb) = PEER_CALLBACK.write() {
        *cb = callback;
    }

    // If callback is set, spawn a task to forward events
    if callback.is_some() {
        let mut rx = transport.subscribe_peer_events();
        let cancel = Arc::clone(cancel_token);

        // Spawn background task to handle events
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime");

            rt.block_on(async move {
                loop {
                    // Check cancellation
                    if cancel.load(Ordering::SeqCst) {
                        break;
                    }

                    // Use timeout to periodically check cancellation
                    match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
                        .await
                    {
                        Ok(Some(event)) => {
                            if let Ok(cb_guard) = PEER_CALLBACK.read() {
                                if let Some(cb) = *cb_guard {
                                    invoke_callback(cb, &event);
                                } else {
                                    // Callback was unregistered, stop listening
                                    break;
                                }
                            }
                        }
                        Ok(None) => {
                            // Channel closed
                            break;
                        }
                        Err(_) => {
                            // Timeout - continue loop to check cancellation
                        }
                    }
                }
            });
        });
    }

    HIVE_OK
}

/// Unregister the peer event callback.
///
/// This will stop the callback thread within 100ms.
///
/// # Returns
///
/// - `HIVE_OK` on success
#[no_mangle]
pub extern "C" fn hive_unregister_peer_callback() -> c_int {
    // Signal the callback thread to stop
    get_or_init_cancel_token().store(true, Ordering::SeqCst);

    // Clear the callback
    if let Ok(mut cb) = PEER_CALLBACK.write() {
        *cb = None;
    }
    HIVE_OK
}

fn invoke_callback(callback: PeerEventCallback, event: &PeerEvent) {
    match event {
        PeerEvent::Connected { peer_id, .. } => {
            let peer_id_cstr = CString::new(peer_id.to_string()).unwrap_or_default();
            callback(
                HIVE_EVENT_CONNECTED,
                peer_id_cstr.as_ptr(),
                std::ptr::null(),
            );
        }
        PeerEvent::Disconnected {
            peer_id, reason, ..
        } => {
            let peer_id_cstr = CString::new(peer_id.to_string()).unwrap_or_default();
            let reason_cstr = CString::new(format!("{}", reason)).unwrap_or_default();
            callback(
                HIVE_EVENT_DISCONNECTED,
                peer_id_cstr.as_ptr(),
                reason_cstr.as_ptr(),
            );
        }
        PeerEvent::Degraded { peer_id, health } => {
            let peer_id_cstr = CString::new(peer_id.to_string()).unwrap_or_default();
            let health_cstr = CString::new(format!(
                "rtt={}ms, loss={}%",
                health.rtt_ms, health.packet_loss_percent
            ))
            .unwrap_or_default();
            callback(
                HIVE_EVENT_DEGRADED,
                peer_id_cstr.as_ptr(),
                health_cstr.as_ptr(),
            );
        }
    }
}

// =============================================================================
// Async Operations (require runtime)
// =============================================================================

/// Context for async FFI operations
pub struct HiveAsyncContext {
    runtime: tokio::runtime::Runtime,
}

impl HiveAsyncContext {
    /// Create a new async context with a tokio runtime
    pub fn new() -> Result<Self, std::io::Error> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        Ok(Self { runtime })
    }

    /// Connect to a peer (async operation)
    pub fn connect_peer(&self, peer_id: &str) -> c_int {
        let transport = match get_transport() {
            Some(t) => t.clone(),
            None => {
                set_last_error("HIVE not initialized".to_string());
                return HIVE_ERR_NOT_INITIALIZED;
            }
        };

        let node_id = NodeId::new(peer_id.to_string());

        match self.runtime.block_on(transport.connect(&node_id)) {
            Ok(_) => HIVE_OK,
            Err(e) => {
                set_last_error(format!("Connection failed: {}", e));
                HIVE_ERR_CONNECTION_FAILED
            }
        }
    }

    /// Disconnect from a peer (async operation)
    pub fn disconnect_peer(&self, peer_id: &str) -> c_int {
        let transport = match get_transport() {
            Some(t) => t.clone(),
            None => {
                set_last_error("HIVE not initialized".to_string());
                return HIVE_ERR_NOT_INITIALIZED;
            }
        };

        let node_id = NodeId::new(peer_id.to_string());

        match self.runtime.block_on(transport.disconnect(&node_id)) {
            Ok(_) => HIVE_OK,
            Err(e) => {
                set_last_error(format!("Disconnect failed: {}", e));
                HIVE_ERR_INTERNAL
            }
        }
    }
}

// Global async context for FFI operations
static ASYNC_CONTEXT: OnceLock<HiveAsyncContext> = OnceLock::new();

fn get_async_context() -> Option<&'static HiveAsyncContext> {
    ASYNC_CONTEXT.get()
}

/// Initialize the async context for FFI operations.
///
/// Must be called before `hive_connect_peer` or `hive_disconnect_peer`.
///
/// # Returns
///
/// - `HIVE_OK` on success
/// - `HIVE_ERR_INTERNAL` on failure
#[no_mangle]
pub extern "C" fn hive_init_async() -> c_int {
    match HiveAsyncContext::new() {
        Ok(ctx) => match ASYNC_CONTEXT.set(ctx) {
            Ok(_) => HIVE_OK,
            Err(_) => {
                set_last_error("Async context already initialized".to_string());
                HIVE_OK // Not an error, just already done
            }
        },
        Err(e) => {
            set_last_error(format!("Failed to create async runtime: {}", e));
            HIVE_ERR_INTERNAL
        }
    }
}

/// Connect to a peer by ID.
///
/// # Arguments
///
/// * `peer_id` - Null-terminated peer ID string
///
/// # Returns
///
/// - `HIVE_OK` on success
/// - `HIVE_ERR_NOT_INITIALIZED` if HIVE not initialized
/// - `HIVE_ERR_INVALID_ARGUMENT` if peer_id is null
/// - `HIVE_ERR_CONNECTION_FAILED` if connection failed
///
/// # Safety
///
/// `peer_id` must be a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn hive_connect_peer(peer_id: *const c_char) -> c_int {
    if peer_id.is_null() {
        set_last_error("peer_id is null".to_string());
        return HIVE_ERR_INVALID_ARGUMENT;
    }

    let peer_id_str = match CStr::from_ptr(peer_id).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("Invalid UTF-8 in peer_id".to_string());
            return HIVE_ERR_INVALID_ARGUMENT;
        }
    };

    let ctx = match get_async_context() {
        Some(c) => c,
        None => {
            set_last_error("Async context not initialized, call hive_init_async first".to_string());
            return HIVE_ERR_NOT_INITIALIZED;
        }
    };

    ctx.connect_peer(peer_id_str)
}

/// Disconnect from a peer by ID.
///
/// # Arguments
///
/// * `peer_id` - Null-terminated peer ID string
///
/// # Returns
///
/// - `HIVE_OK` on success
/// - `HIVE_ERR_NOT_INITIALIZED` if HIVE not initialized
/// - `HIVE_ERR_INVALID_ARGUMENT` if peer_id is null
///
/// # Safety
///
/// `peer_id` must be a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn hive_disconnect_peer(peer_id: *const c_char) -> c_int {
    if peer_id.is_null() {
        set_last_error("peer_id is null".to_string());
        return HIVE_ERR_INVALID_ARGUMENT;
    }

    let peer_id_str = match CStr::from_ptr(peer_id).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("Invalid UTF-8 in peer_id".to_string());
            return HIVE_ERR_INVALID_ARGUMENT;
        }
    };

    let ctx = match get_async_context() {
        Some(c) => c,
        None => {
            set_last_error("Async context not initialized, call hive_init_async first".to_string());
            return HIVE_ERR_NOT_INITIALIZED;
        }
    };

    ctx.disconnect_peer(peer_id_str)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(HIVE_OK, 0);
        assert_eq!(HIVE_ERR_NOT_INITIALIZED, -1);
        assert_eq!(HIVE_ERR_INVALID_PEER, -2);
    }

    #[test]
    fn test_event_types() {
        assert_eq!(HIVE_EVENT_CONNECTED, 1);
        assert_eq!(HIVE_EVENT_DISCONNECTED, 2);
        assert_eq!(HIVE_EVENT_DEGRADED, 3);
    }

    #[test]
    fn test_not_initialized() {
        // Before initialization, functions should return appropriate errors
        let result = hive_get_connected_peers();
        assert!(result.is_null());
    }

    #[test]
    fn test_peer_info_serialization() {
        let info = PeerInfo {
            peer_id: "test-peer".to_string(),
            connected_since: "2025-01-01T00:00:00Z".to_string(),
            status: "healthy".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test-peer"));
        assert!(json.contains("healthy"));
    }

    #[test]
    fn test_connected_peers_response_serialization() {
        let response = ConnectedPeersResponse {
            peers: vec![PeerInfo {
                peer_id: "node-1".to_string(),
                connected_since: "2025-01-01T00:00:00Z".to_string(),
                status: "healthy".to_string(),
            }],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("peers"));
        assert!(json.contains("node-1"));
    }
}
