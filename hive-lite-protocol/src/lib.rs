//! HIVE-Lite Wire Protocol
//!
//! Single source of truth for the HIVE-Lite binary protocol (ADR-035).
//! This crate is `#![no_std]` and has zero dependencies, so it can be
//! consumed by both embedded (`hive-lite`) and hosted (`hive-mesh`) code.

#![no_std]

pub mod capabilities;
pub mod constants;
pub mod crdt_type;
pub mod error;
pub mod header;
pub mod message_type;
pub mod ota;

pub use capabilities::NodeCapabilities;
pub use constants::*;
pub use crdt_type::CrdtType;
pub use error::MessageError;
pub use header::{decode_header, encode_header, Header};
pub use message_type::MessageType;
