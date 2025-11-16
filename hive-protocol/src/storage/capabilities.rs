//! Storage backend capability traits
//!
//! This module defines **optional capabilities** that complete storage backends may provide.
//!
//! # What is a Backend?
//!
//! A **backend is a complete, integrated solution** for storage, synchronization, and persistence:
//!
//! - **DittoBackend**: Ditto's proprietary CRDT storage + built-in P2P mesh + multi-transport
//! - **AutomergeIrohBackend**: Automerge CRDTs + RocksDB persistence + Iroh QUIC + custom mesh (ADR-017)
//! - **SimpleBackend**: RocksDB only (no CRDT, no sync, just local K/V storage)
//!
//! Backends are **not** individual components like "just Automerge" or "just Iroh". Each backend
//! integrates multiple technologies into a cohesive solution.
//!
//! # Architecture Philosophy
//!
//! Rather than forcing all backends into a one-size-fits-all trait, we use
//! **capability traits** to expose what each complete backend can do:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │ HIVE Protocol Business Logic                             │
//! │ (Uses StorageBackend + optional capabilities)           │
//! └─────────────────┬───────────────────────────────────────┘
//!                   │
//! ┌─────────────────▼───────────────────────────────────────┐
//! │ StorageBackend (required for all backends)              │
//! │ • collection() - Basic CRUD via Vec<u8>                 │
//! │ • flush() - Persistence guarantee                       │
//! └─────────────────┬───────────────────────────────────────┘
//!                   │
//!        ┌──────────┴──────────────────┬─────────────┐
//!        ▼                             ▼             ▼
//! ┌─────────────────┐      ┌─────────────────────┐  ┌──────────┐
//! │ DittoBackend    │      │ AutomergeIrohBackend│  │ Simple   │
//! │ =============== │      │ ===================  │  │ Backend  │
//! │ • Ditto CRDT    │      │ • Automerge CRDTs   │  │ ======== │
//! │ • Ditto P2P     │      │ • RocksDB persist   │  │ • RocksDB│
//! │ • Multi-trans   │      │ • Iroh QUIC         │  │   only   │
//! │                 │      │ • Custom mesh       │  │          │
//! │ + CrdtCapable   │      │   (ADR-017)         │  │          │
//! │ + SyncCapable   │      │                     │  │          │
//! │                 │      │ + CrdtCapable       │  │          │
//! │                 │      │ + SyncCapable       │  │          │
//! └─────────────────┘      └─────────────────────┘  └──────────┘
//!   (one complete           (one complete OSS        (minimal
//!    commercial              stack, not separate      backend)
//!    solution)               pieces)
//! ```
//!
//! # Backend Comparison
//!
//! | Backend                | Components                          | CRDT | Sync | License    | Use Case                 |
//! |------------------------|-------------------------------------|------|------|------------|--------------------------|
//! | **DittoBackend**       | Ditto SDK (all-in-one)              | ✅   | ✅   | Proprietary| Managed service          |
//! | **AutomergeIrohBackend**| Automerge + RocksDB + Iroh + mesh  | ✅   | ✅   | MIT/Apache | OSS, self-hosted         |
//! | **SimpleBackend**      | RocksDB only                        | ❌   | ❌   | Apache 2.0 | Testing, local storage   |
//!
//! # Capability Traits
//!
//! ## CrdtCapable - Field-Level Conflict Resolution
//!
//! Backends that implement `CrdtCapable` can store structured data and provide
//! CRDT-based conflict resolution at the field level, not just document level.
//!
//! **Benefits:**
//! - OR-Set semantics for arrays (concurrent additions merge correctly)
//! - LWW-Register for scalar fields (timestamp-based resolution)
//! - Delta sync (only changed fields transmitted)
//! - 50x+ bandwidth reduction vs. full document sync
//!
//! **Requirements:**
//! - Protobuf messages must have `#[derive(Serialize, Deserialize)]`
//! - Backend must support structured storage (JSON for Ditto, Automerge doc for Automerge)
//!
//! **Example (DittoBackend):**
//! ```ignore
//! use hive_protocol::storage::{DittoBackend, CrdtCapable};
//! use hive_schema::hierarchy::v1::SquadSummary;
//!
//! let backend = DittoBackend::new(store);
//! let squads: Arc<dyn TypedCollection<SquadSummary>> =
//!     backend.typed_collection("squads");
//! squads.upsert("squad-1", &summary)?;
//! // → Ditto stores as JSON, enables CRDT merging
//! ```
//!
//! **Example (AutomergeIrohBackend):**
//! ```ignore
//! use hive_protocol::storage::{AutomergeIrohBackend, CrdtCapable};
//!
//! let backend = AutomergeIrohBackend::new(config);
//! let squads: Arc<dyn TypedCollection<SquadSummary>> =
//!     backend.typed_collection("squads");
//! squads.upsert("squad-1", &summary)?;
//! // → Automerge stores as CRDT document, persists to RocksDB, syncs via Iroh
//! ```
//!
//! ## SyncCapable - Built-in Replication
//!
//! Backends that implement `SyncCapable` have built-in P2P synchronization.
//!
//! **DittoBackend**: Built-in mesh networking with Bluetooth, WiFi-Direct, TCP/IP
//! **AutomergeIrohBackend**: Integrated with Iroh QUIC transport + custom mesh (ADR-017)
//!
//! # Decision Guide
//!
//! ## When to use basic `StorageBackend` interface:
//!
//! - ✅ Backend-agnostic code (must work with any backend)
//! - ✅ Testing with mocks
//! - ✅ CRDT benefits not critical
//! - ✅ Simple binary data storage
//!
//! ## When to use `CrdtCapable` interface:
//!
//! - ✅ Field-level conflict resolution needed
//! - ✅ Delta sync bandwidth optimization critical
//! - ✅ Type safety at compile time desired
//! - ✅ Willing to add serde derives to protobuf messages
//!
//! ## When to use `SyncCapable` interface:
//!
//! - ✅ Need to control sync lifecycle (start/stop)
//! - ✅ Using Ditto's built-in P2P (not Iroh)
//! - ✅ Need sync statistics and monitoring
//!
//! # Open Source Path
//!
//! HIVE Protocol provides a **fully open-source implementation** using:
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │ HIVE Protocol (Apache 2.0)                                │
//! ├──────────────────────────────────────────────────────────┤
//! │ AutomergeIrohBackend (complete OSS backend)              │
//! │   Components:                                            │
//! │   • Automerge (MIT) - CRDT engine                        │
//! │   • RocksDB (Apache 2.0) - Persistence layer             │
//! │   • Iroh (Apache 2.0/MIT) - QUIC transport               │
//! │   • Custom P2P mesh (ADR-017) - Discovery & topology     │
//! │                                                          │
//! │   Capabilities:                                          │
//! │   └─ StorageBackend: Yes (required)                      │
//! │   └─ CrdtCapable: Yes (via Automerge CRDTs)              │
//! │   └─ SyncCapable: Yes (via Iroh + custom mesh)           │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! This ensures:
//! - ✅ No vendor lock-in
//! - ✅ Full auditability
//! - ✅ Military/government deployment sovereignty
//! - ✅ Community contributions and forks
//!
//! # Future Capabilities
//!
//! Additional capability traits may be added:
//! - `QueryCapable` - Advanced query DSLs
//! - `IndexCapable` - Secondary indexes
//! - `TransactionCapable` - Multi-document ACID transactions
//! - `EncryptionCapable` - At-rest encryption

use anyhow::Result;
use prost::Message;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

/// Typed collection trait for CRDT-optimized storage
///
/// Backends that implement `CrdtCapable` provide this trait to enable
/// field-level conflict resolution via CRDT semantics.
///
/// # Type Parameters
///
/// * `M` - Protobuf message type with serde support
///
/// # CRDT Semantics
///
/// Different field types get different CRDT semantics:
/// - **Arrays**: OR-Set (observed-remove set) - concurrent additions merge
/// - **Scalars**: LWW-Register (last-write-wins) - timestamp-based resolution
/// - **Nested objects**: Recursive application of above rules
///
/// # Example
///
/// ```ignore
/// use hive_protocol::storage::{CrdtCapable, TypedCollection};
/// use hive_schema::hierarchy::v1::SquadSummary;
///
/// let backend = DittoBackend::new(store);
/// let squads: Arc<dyn TypedCollection<SquadSummary>> =
///     backend.typed_collection("squads");
///
/// // Field-level updates
/// let mut summary = squads.get("squad-1")?.unwrap();
/// summary.member_ids.push("node-4".to_string());  // OR-Set addition
/// squads.upsert("squad-1", &summary)?;
/// // → Only member_ids field is transmitted, not entire document
/// ```
pub trait TypedCollection<M>: Send + Sync
where
    M: Message + Serialize + DeserializeOwned + Default + Clone,
{
    /// Insert or update a typed document with CRDT merging
    ///
    /// Backends convert the message to their CRDT format:
    /// - Ditto: `message` → JSON → Ditto CRDT
    /// - Automerge: `message` → Automerge document
    fn upsert(&self, doc_id: &str, message: &M) -> Result<()>;

    /// Get a typed document by ID
    fn get(&self, doc_id: &str) -> Result<Option<M>>;

    /// Delete a typed document by ID
    fn delete(&self, doc_id: &str) -> Result<()>;

    /// Scan all typed documents in the collection
    fn scan(&self) -> Result<Vec<(String, M)>>;

    /// Find typed documents matching a predicate
    fn find(&self, predicate: Box<dyn Fn(&M) -> bool + Send>) -> Result<Vec<(String, M)>>;

    /// Count typed documents in the collection
    fn count(&self) -> Result<usize>;
}

/// CRDT capability trait - Backend supports field-level conflict resolution
///
/// Backends that implement this trait can store structured data and provide
/// CRDT-based merging at the field level, enabling:
/// - Delta sync (only changed fields transmitted)
/// - Automatic conflict resolution
/// - Optimistic replication
///
/// # Implementations
///
/// - ✅ `DittoBackend` - JSON expansion with Ditto CRDTs
/// - ✅ `AutomergeIrohBackend` - Native Automerge documents with RocksDB persistence
/// - ❌ `SimpleBackend` - Blob storage, no CRDT support
pub trait CrdtCapable: Send + Sync {
    /// Create a typed collection for CRDT-optimized storage
    ///
    /// # Type Parameters
    ///
    /// * `M` - Protobuf message type with serde support
    ///
    /// # Returns
    ///
    /// Thread-safe typed collection handle with CRDT semantics
    fn typed_collection<M>(&self, name: &str) -> Arc<dyn TypedCollection<M>>
    where
        M: Message + Serialize + DeserializeOwned + Default + Clone + 'static;
}

/// Sync capability trait - Backend has built-in replication
///
/// Complete backends that provide integrated P2P synchronization implement this trait.
///
/// # Implementations
///
/// - ✅ `DittoBackend` - Built-in Bluetooth/WiFi/TCP mesh
/// - ✅ `AutomergeIrohBackend` - Integrated Iroh QUIC transport with custom mesh (ADR-017)
/// - ❌ `SimpleBackend` - Local storage only, no synchronization
pub trait SyncCapable: Send + Sync {
    /// Start background synchronization
    ///
    /// For Ditto: Activates mesh networking (Bluetooth, WiFi-Direct, TCP)
    fn start_sync(&self) -> Result<()>;

    /// Stop background synchronization
    ///
    /// For Ditto: Disconnects from all peers, stops listening
    fn stop_sync(&self) -> Result<()>;

    /// Get current sync statistics
    ///
    /// Returns metrics like peer count, bytes sent/received, etc.
    fn sync_stats(&self) -> Result<SyncStats>;
}

/// Synchronization statistics
#[derive(Debug, Clone)]
pub struct SyncStats {
    /// Number of connected peers
    pub peer_count: usize,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Last sync timestamp (if applicable)
    pub last_sync: Option<std::time::SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify TypedCollection is object-safe (can be used as trait object)
    #[test]
    fn test_typed_collection_is_object_safe() {
        use hive_schema::hierarchy::v1::SquadSummary;
        fn _assert_object_safe(_: &dyn TypedCollection<SquadSummary>) {}
    }

    // Note: CrdtCapable is intentionally NOT object-safe due to generic method.
    // This is correct - use concrete backend types:
    //   let backend = DittoBackend::new(store);
    //   let collection = backend.typed_collection::<SquadSummary>("squads");
}
