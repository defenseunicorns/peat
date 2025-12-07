//! Network layer for HIVE Protocol
//!
//! This module provides both:
//! - Network simulation (bandwidth, latency, loss constraints)
//! - Real P2P transport via Iroh (for AutomergeIrohBackend)

// Network simulation modules
pub mod constraints;
pub mod metrics;
pub mod partition;
pub mod transport;

// Real P2P transport (Phase 3: Iroh integration)
#[cfg(feature = "automerge-backend")]
pub mod formation_handshake;
#[cfg(feature = "automerge-backend")]
pub mod iroh_transport;
#[cfg(feature = "automerge-backend")]
pub mod peer_config;

// Re-exports
#[cfg(feature = "automerge-backend")]
pub use formation_handshake::{perform_initiator_handshake, perform_responder_handshake};
#[cfg(feature = "automerge-backend")]
pub use iroh_transport::{
    IrohTransport, TransportEventReceiver, TransportEventSender, TransportPeerEvent,
    TRANSPORT_EVENT_CHANNEL_CAPACITY,
};
#[cfg(feature = "automerge-backend")]
pub use peer_config::{FormationConfig, LocalConfig, PeerConfig, PeerInfo};
