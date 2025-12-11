//! Primitive CRDTs for HIVE-Lite
//!
//! These are lightweight, no_std compatible CRDTs suitable for
//! resource-constrained embedded devices.

pub mod lww_register;
pub mod g_counter;
pub mod pn_counter;

pub use lww_register::LwwRegister;
pub use g_counter::GCounter;
pub use pn_counter::PnCounter;

/// Trait for all HIVE-Lite CRDTs
pub trait LiteCrdt: Sized {
    /// The operation type for this CRDT
    type Op;
    /// The value type this CRDT produces
    type Value;

    /// Apply a local operation
    fn apply(&mut self, op: &Self::Op);

    /// Merge with another instance of this CRDT
    fn merge(&mut self, other: &Self);

    /// Get the current value
    fn value(&self) -> Self::Value;

    /// Encode to bytes for network transmission
    /// Returns number of bytes written
    fn encode(&self, buf: &mut [u8]) -> Result<usize, CrdtError>;

    /// Decode from bytes
    fn decode(buf: &[u8]) -> Result<Self, CrdtError>;
}

/// Errors that can occur during CRDT operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrdtError {
    /// Buffer too small for encoding
    BufferTooSmall,
    /// Invalid data during decoding
    InvalidData,
    /// Node ID not found (for counters)
    NodeNotFound,
}
