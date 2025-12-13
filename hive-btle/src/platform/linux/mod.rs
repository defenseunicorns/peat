//! Linux/BlueZ platform implementation
//!
//! This module provides the BLE adapter implementation for Linux using
//! the `bluer` crate (BlueZ D-Bus bindings).
//!
//! ## Requirements
//!
//! - Linux with BlueZ 5.48+
//! - D-Bus system bus access
//! - Bluetooth adapter (built-in, USB dongle, etc.)
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::platform::linux::BluerAdapter;
//! use hive_btle::{BleConfig, NodeId};
//!
//! let config = BleConfig::new(NodeId::new(0x12345678));
//! let mut adapter = BluerAdapter::new().await?;
//! adapter.init(&config).await?;
//! adapter.start().await?;
//! ```

mod adapter;
mod connection;

pub use adapter::BluerAdapter;
pub use connection::BluerConnection;
