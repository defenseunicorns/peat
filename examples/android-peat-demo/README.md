# Peat BLE Demo App

A simple Android application demonstrating Peat BLE mesh connectivity with M5Stack Core2 devices.

## Features

- **Scan** for Peat BLE nodes (devices advertising the Peat service UUID `0xF47A`)
- **Connect** to discovered M5Stack Core2 nodes
- **Advertise** as a Peat node for other devices to discover
- **Sync** CRDT data over BLE GATT characteristics
- **Alert/Ack** emergency alert system with vibration feedback

## Requirements

- Android 6.0 (API 23) or later
- Bluetooth Low Energy support
- For BLE 5.0 features: Android 8.0 (API 26) or later

## Building

### Prerequisites

1. Install the Android NDK and set `ANDROID_NDK_HOME`
2. Install Rust Android targets:
   ```bash
   rustup target add aarch64-linux-android armv7-linux-androideabi
   ```
3. Install `cargo-ndk`:
   ```bash
   cargo install cargo-ndk
   ```

### Build Native Library

First, build the peat-btle native library for Android:

```bash
cd ../../peat-btle
cargo ndk -t arm64-v8a -t armeabi-v7a build --release --features android
```

Copy the built libraries to the jniLibs directory:

```bash
mkdir -p app/src/main/jniLibs/arm64-v8a app/src/main/jniLibs/armeabi-v7a
cp ../../target/aarch64-linux-android/release/libpeat_btle.so app/src/main/jniLibs/arm64-v8a/
cp ../../target/armv7-linux-androideabi/release/libpeat_btle.so app/src/main/jniLibs/armeabi-v7a/
```

### Build the App

```bash
./gradlew assembleDebug
```

## Usage

1. Launch the app on an Android device
2. Grant Bluetooth permissions when prompted
3. Tap "Start Scan" to discover nearby Peat nodes
4. Tap a discovered device to connect
5. Tap "Start Advertise" to make this device discoverable

## Peat BLE Protocol

This demo uses the same BLE protocol as the M5Stack Core2 firmware for full interoperability.

### Service & Characteristics

| UUID | Name | Description |
|------|------|-------------|
| `0xF47A` | Service | Peat BLE Service |
| `0xF47B` | Document | CRDT document exchange (read/write/notify) |

### Document Format

The Peat document format is:

```
[version: 4 bytes] [node_id: 4 bytes] [counter_data: N bytes] [0xAB marker] [reserved: 1 byte] [peripheral_len: 2 bytes] [peripheral_data: M bytes]
```

- **version**: Document version (u32 little-endian)
- **node_id**: Source node ID (u32 little-endian)
- **counter_data**: GCounter CRDT data
- **peripheral_data**: Event type, health status, etc.

### Event Types

The Peripheral data includes event information:

| Event | Description |
|-------|-------------|
| `None` | No active event |
| `Emergency` | Emergency alert (triggers vibration) |
| `Ack` | Acknowledgment (silences alert) |
| `Heartbeat` | Periodic health update |

## Testing with M5Stack Core2

1. Flash the M5Stack Core2 with the `m5stack-core2-peat` firmware
2. Power on the M5Stack - it will advertise as `Peat-XXXXXXXX`
3. Use this demo app to scan and connect
4. Tap the M5Stack's right button (C) to send EMERGENCY
5. Tap the left button (A) on M5Stack or ACK button on Android to acknowledge
6. Observe CRDT sync and vibration alerts between devices

## Architecture

```
┌──────────────────┐         BLE          ┌──────────────────┐
│  Android Phone   │◄────────────────────►│  M5Stack Core2   │
│  (this app)      │                      │  (ESP32 + NimBLE)│
│                  │   GATT read/write    │                  │
│  PeatBtle.kt     │   notifications      │  nimble.rs       │
│  GattCallback    │◄────────────────────►│  gap_event_handler│
└──────────────────┘                      └──────────────────┘
        │                                          │
        ▼                                          ▼
┌──────────────────┐                      ┌──────────────────┐
│  PeatDocument    │                      │  PeatDocument    │
│  - GCounter      │     CRDT merge       │  - GCounter      │
│  - Peripheral    │◄────────────────────►│  - Peripheral    │
│  - version       │                      │  - version       │
└──────────────────┘                      └──────────────────┘
```

## License

Apache-2.0
