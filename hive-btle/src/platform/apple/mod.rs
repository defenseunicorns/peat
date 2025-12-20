//! Apple platform implementation (iOS/macOS)
//!
//! This module provides the BLE adapter implementation for Apple platforms using
//! CoreBluetooth framework bindings via the `objc2` crate.
//!
//! ## Requirements
//!
//! ### iOS
//! - iOS 13.0 or later
//! - `NSBluetoothAlwaysUsageDescription` in Info.plist
//! - Background modes: `bluetooth-central`, `bluetooth-peripheral`
//!
//! ### macOS
//! - macOS 10.15 (Catalina) or later
//! - Bluetooth entitlement in app sandbox
//!
//! ## Architecture
//!
//! CoreBluetooth uses a delegate-based pattern where callbacks are delivered
//! to Objective-C delegate objects. This module bridges those delegates to
//! Rust async channels.
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │       CoreBluetoothAdapter (Rust)        │
//! ├─────────────────────────────────────────┤
//! │  CentralManager    │  PeripheralManager │
//! │   (scanning,       │   (advertising,    │
//! │    connecting)     │    GATT server)    │
//! ├─────────────────────────────────────────┤
//! │           Objective-C Delegates          │
//! │  (CentralDelegate, PeripheralDelegate)  │
//! ├─────────────────────────────────────────┤
//! │            CoreBluetooth Framework       │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::platform::apple::CoreBluetoothAdapter;
//! use hive_btle::{BleConfig, NodeId};
//!
//! let config = BleConfig::new(NodeId::new(0x12345678));
//! let mut adapter = CoreBluetoothAdapter::new()?;
//! adapter.init(&config).await?;
//! adapter.start().await?;
//! ```
//!
//! ## CoreBluetooth Concepts
//!
//! - **CBCentralManager**: Scans for and connects to peripherals (GATT client)
//! - **CBPeripheralManager**: Advertises and hosts GATT services (GATT server)
//! - **CBPeripheral**: Represents a remote BLE device
//! - **CBService/CBCharacteristic**: GATT service and characteristic objects
//!
//! ## iOS Background Execution
//!
//! For iOS apps to use BLE in the background, add to Info.plist:
//!
//! ```xml
//! <key>UIBackgroundModes</key>
//! <array>
//!     <string>bluetooth-central</string>
//!     <string>bluetooth-peripheral</string>
//! </array>
//! <key>NSBluetoothAlwaysUsageDescription</key>
//! <string>HIVE uses Bluetooth to sync data with nearby devices</string>
//! ```

mod adapter;
mod central;
mod connection;
mod delegates;
mod peripheral;

pub use adapter::CoreBluetoothAdapter;
pub use connection::CoreBluetoothConnection;

// These are used internally by adapter.rs
#[allow(unused_imports)]
pub(crate) use central::CentralManager;
#[allow(unused_imports)]
pub(crate) use peripheral::PeripheralManager;
