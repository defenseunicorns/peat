//! Windows platform implementation (WinRT Bluetooth APIs)
//!
//! This module provides the BLE adapter implementation for Windows using
//! the WinRT Bluetooth APIs via the `windows` crate.
//!
//! ## Requirements
//!
//! - Windows 10 version 1703+ (Creators Update) for scanning/connecting
//! - Windows 10 version 1803+ (April 2018 Update) for GATT server
//! - BLE-capable Bluetooth adapter
//!
//! ## Architecture
//!
//! Windows BLE uses two separate APIs:
//! - **Advertisement API**: For scanning (`BluetoothLEAdvertisementWatcher`) and
//!   broadcasting (`BluetoothLEAdvertisementPublisher`)
//! - **GATT API**: For client/server operations (`GattDeviceService`, `GattServiceProvider`)
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │       WinRtBleAdapter (Rust)            │
//! ├─────────────────────────────────────────┤
//! │     Watcher      │      Publisher       │
//! │   (scanning)     │    (advertising)     │
//! ├─────────────────────────────────────────┤
//! │  GattClient      │    GattServer        │
//! │  (connecting,    │   (hosting HIVE      │
//! │   reading)       │    service)          │
//! ├─────────────────────────────────────────┤
//! │           WinRT Bluetooth APIs          │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::platform::windows::WinRtBleAdapter;
//! use hive_btle::{BleConfig, NodeId};
//!
//! let config = BleConfig::new(NodeId::new(0x12345678));
//! let mut adapter = WinRtBleAdapter::new()?;
//! adapter.init(&config).await?;
//! adapter.start().await?;
//! ```
//!
//! ## Windows Version Notes
//!
//! | Feature | Minimum Version |
//! |---------|-----------------|
//! | BLE Scanning | Windows 10 1703 |
//! | BLE Advertising | Windows 10 1703 |
//! | GATT Client | Windows 10 1703 |
//! | GATT Server | Windows 10 1803 |
//! | Extended Advertising | Windows 10 1903 |

mod adapter;
mod advertiser;
mod connection;
mod gatt_server;
mod watcher;

pub use adapter::WinRtBleAdapter;
pub use connection::WinRtConnection;
