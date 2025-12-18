//! Android platform implementation
//!
//! This module provides the BLE adapter implementation for Android using
//! JNI bindings to the Android Bluetooth API.
//!
//! ## Requirements
//!
//! - Android 6.0 (API 23) or later
//! - `BLUETOOTH`, `BLUETOOTH_ADMIN`, `ACCESS_FINE_LOCATION` permissions
//! - For BLE 5.0 features: Android 8.0 (API 26) or later
//!
//! ## Architecture
//!
//! The Android implementation uses JNI to call Android Bluetooth APIs:
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │           AndroidAdapter (Rust)          │
//! ├─────────────────────────────────────────┤
//! │                JNI Bridge                │
//! ├─────────────────────────────────────────┤
//! │  BluetoothAdapter / BluetoothLeScanner  │
//! │  BluetoothLeAdvertiser / BluetoothGatt  │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::platform::android::AndroidAdapter;
//! use hive_btle::{BleConfig, NodeId};
//!
//! // Must be called from Android app with JNI environment
//! let config = BleConfig::new(NodeId::new(0x12345678));
//! let mut adapter = AndroidAdapter::new(jni_env, context)?;
//! adapter.init(&config).await?;
//! adapter.start().await?;
//! ```
//!
//! ## JNI Callbacks
//!
//! The implementation registers JNI callbacks for:
//! - Scan results (`onScanResult`)
//! - Connection state changes (`onConnectionStateChange`)
//! - GATT service discovery (`onServicesDiscovered`)
//! - Characteristic reads/writes (`onCharacteristicRead`, `onCharacteristicWrite`)
//! - Notifications (`onCharacteristicChanged`)

mod adapter;
mod connection;
mod jni_bridge;

pub use adapter::AndroidAdapter;
pub use connection::AndroidConnection;
