//! FFI (Foreign Function Interface) for Peat Protocol
//!
//! This module provides C-compatible FFI bindings for use with Android (JNI/Kotlin),
//! iOS (Swift/Objective-C), and other languages that need to interop with Peat.
//!
//! ## Overview
//!
//! The FFI is designed for the ATAK integration team and other consumers who need
//! to manage peer connections through native code bindings.
//!
//! ## Peer Management API (Issue #258)
//!
//! The peer management FFI provides:
//! - `peat_get_connected_peers()` - Get list of connected peers as JSON
//! - `peat_get_peer_status()` - Get connection status for a specific peer
//! - `peat_get_peer_health()` - Get health metrics for a peer
//! - `peat_connect_peer()` - Initiate connection to a peer
//! - `peat_disconnect_peer()` - Close connection to a peer
//! - `peat_register_peer_callback()` - Register callback for peer events
//!
//! ## Memory Management
//!
//! All strings returned by FFI functions are heap-allocated and must be freed
//! using `peat_free_string()`. Failure to do so will result in memory leaks.
//!
//! ## Thread Safety
//!
//! All FFI functions are thread-safe. Callbacks may be invoked from any thread.
//!
//! ## Example (Kotlin)
//!
//! ```kotlin
//! object PeatPeerManager {
//!     external fun getConnectedPeers(): String
//!     external fun getPeerStatus(peerId: String): String
//!     external fun connectPeer(peerId: String): Int
//!     external fun disconnectPeer(peerId: String): Int
//! }
//! ```

pub mod peer;

pub use peer::*;
