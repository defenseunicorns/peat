//! HIVE-Lite Sync Protocol
//!
//! Efficient CRDT synchronization over BLE GATT characteristics.
//!
//! ## Overview
//!
//! This module provides the sync layer for HIVE-Lite nodes, enabling
//! efficient state synchronization over bandwidth-constrained BLE links.
//!
//! ## Key Components
//!
//! - **CRDTs**: Conflict-free replicated data types (LWW-Register, G-Counter)
//! - **Batching**: Accumulates changes to reduce radio activity
//! - **Delta Encoding**: Only sends changes since last sync
//! - **Chunking**: Splits large messages across MTU boundaries
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │                  Application                        │
//! │    (position updates, health status, alerts)       │
//! └─────────────────────┬──────────────────────────────┘
//!                       │
//!                       ▼
//! ┌────────────────────────────────────────────────────┐
//! │              GattSyncProtocol                       │
//! │  ┌──────────────┐  ┌────────────┐  ┌────────────┐  │
//! │  │    Batch     │  │   Delta    │  │  Chunked   │  │
//! │  │ Accumulator  │─▶│  Encoder   │─▶│  Transfer  │  │
//! │  └──────────────┘  └────────────┘  └────────────┘  │
//! └─────────────────────┬──────────────────────────────┘
//!                       │
//!                       ▼
//! ┌────────────────────────────────────────────────────┐
//! │              GATT Characteristics                   │
//! │           (Sync Data, Sync State)                  │
//! └────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::sync::{GattSyncProtocol, SyncConfig, CrdtOperation, Position};
//! use hive_btle::NodeId;
//!
//! // Create sync protocol
//! let mut sync = GattSyncProtocol::new(
//!     NodeId::new(0x12345678),
//!     SyncConfig::default(),
//! );
//!
//! // Add a peer
//! sync.add_peer(&peer_id);
//!
//! // Queue position update
//! sync.queue_operation(CrdtOperation::UpdatePosition {
//!     node_id: my_node_id,
//!     position: Position::new(37.7749, -122.4194),
//!     timestamp: current_time_ms,
//! });
//!
//! // Check if time to sync
//! if sync.should_sync() {
//!     let chunks = sync.prepare_sync(&peer_id);
//!     for chunk in chunks {
//!         // Write chunk to GATT characteristic
//!         gatt.write_sync_data(&chunk.encode());
//!     }
//! }
//!
//! // Process received data
//! if let Some(ops) = sync.process_received(chunk, &peer_id) {
//!     for op in ops {
//!         // Apply CRDT operation to local state
//!         apply_operation(op);
//!     }
//! }
//! ```
//!
//! ## Power Efficiency
//!
//! The sync protocol is designed for constrained devices:
//!
//! | Feature | Benefit |
//! |---------|---------|
//! | Batching | Reduces sync frequency (less radio time) |
//! | Delta Encoding | Sends only changes (less bytes) |
//! | Configurable Intervals | Trade freshness for battery |
//! | Compact CRDT Encoding | Minimal overhead |
//!
//! ## Sync Profiles
//!
//! ```ignore
//! // For smartwatch (battery critical)
//! let config = SyncConfig::low_power();
//!
//! // For tablet (responsiveness preferred)
//! let config = SyncConfig::responsive();
//! ```

pub mod batch;
pub mod crdt;
pub mod delta;
pub mod protocol;

pub use batch::{BatchAccumulator, BatchConfig, OperationBatch};
pub use crdt::{CrdtOperation, GCounter, HealthStatus, LwwRegister, Position, Timestamp};
pub use delta::{DeltaEncoder, DeltaStats, PeerSyncState, VectorClock};
pub use protocol::{
    chunk_data, ChunkHeader, ChunkReassembler, GattSyncProtocol, SyncChunk, SyncConfig, SyncState,
    SyncStats, CHUNK_HEADER_SIZE, DEFAULT_MTU, MAX_MTU,
};
