# HIVE BLE Demo App (iOS/macOS)

A SwiftUI application demonstrating HIVE BLE mesh connectivity with M5Stack Core2 devices.

## Features

- **Discover** HIVE BLE nodes advertising the HIVE service UUID (`0xF47A`)
- **Connect** to discovered M5Stack Core2 nodes
- **Advertise** as a HIVE node for other devices to discover
- **Sync** CRDT data over BLE GATT characteristics
- **Alert/Ack** emergency alert system with haptic feedback

## Requirements

- iOS 16.0 or later
- macOS 13.0 or later
- Bluetooth Low Energy support
- For BLE 5.0 features: devices with LE 2M PHY support

## Building

### Prerequisites

1. Install Xcode 15 or later
2. Install Rust and the iOS/macOS targets:
   ```bash
   rustup target add aarch64-apple-ios aarch64-apple-darwin
   ```

### Build with Mock Data (SwiftUI only)

For development and testing without native Rust bindings:

```bash
cd examples/ios-hive-test
swift build
swift run
```

### Build with Native Library

To build the full app with Rust BLE bindings:

```bash
cd examples/ios-hive-test
./build-ios.sh
```

This will:
1. Build the Rust `hive-apple-ffi` library for iOS and macOS
2. Generate UniFFI Swift bindings
3. Create an xcframework for Xcode integration

Then open in Xcode:
- Add `build/HiveFFI.xcframework` to your Xcode project
- Replace the mock `HiveFFI.swift` with the generated `hive_apple_ffi.swift`

## Usage

1. Launch the app on an iOS device or Mac
2. Grant Bluetooth permissions when prompted
3. The mesh automatically starts and discovers nearby HIVE nodes
4. Tap **EMERGENCY** to send an alert to all connected peers
5. Tap **ACK** to acknowledge received alerts
6. Tap **RESET** to clear the current alert state

## HIVE BLE Protocol

This demo uses the same BLE protocol as the M5Stack Core2 firmware and Android demo for full interoperability.

### Service & Characteristics

| UUID | Name | Description |
|------|------|-------------|
| `0xF47A` | Service | HIVE BLE Service |
| `0xF47B` | Document | CRDT document exchange (read/write/notify) |

### Document Format

The HIVE document format is:

```
[version: 4 bytes] [node_id: 4 bytes] [counter_data: N bytes] [0xAB marker] [reserved: 1 byte] [peripheral_len: 2 bytes] [peripheral_data: M bytes]
```

- **version**: Document version (u32 little-endian)
- **node_id**: Source node ID (u32 little-endian)
- **counter_data**: GCounter CRDT data
- **peripheral_data**: Event type, health status, etc.

### Event Types

| Event | Description |
|-------|-------------|
| `None` | No active event |
| `Emergency` | Emergency alert (triggers haptic) |
| `Ack` | Acknowledgment (clears alert) |
| `Heartbeat` | Periodic health update |

## Testing with M5Stack Core2

1. Flash the M5Stack Core2 with the `m5stack-core2-hive` firmware
2. Power on the M5Stack - it will advertise as `HIVE-XXXXXXXX`
3. Use this demo app to discover and connect
4. Tap the M5Stack's right button (C) to send EMERGENCY
5. Tap ACK on the iOS app to acknowledge
6. Observe CRDT sync and haptic alerts between devices

## Architecture

```
┌──────────────────┐         BLE          ┌──────────────────┐
│   iOS/macOS      │◄────────────────────►│  M5Stack Core2   │
│   (this app)     │                      │  (ESP32 + NimBLE)│
│                  │   GATT read/write    │                  │
│   HiveViewModel  │   notifications      │  nimble.rs       │
│   CoreBluetooth  │◄────────────────────►│  gap_event_handler│
└──────────────────┘                      └──────────────────┘
        │                                          │
        ▼                                          ▼
┌──────────────────┐                      ┌──────────────────┐
│  HiveDocument    │                      │  HiveDocument    │
│  - GCounter      │     CRDT merge       │  - GCounter      │
│  - Peripheral    │◄────────────────────►│  - Peripheral    │
│  - version       │                      │  - version       │
└──────────────────┘                      └──────────────────┘
```

## Project Structure

```
examples/ios-hive-test/
├── Package.swift           # Swift Package Manager config
├── build-ios.sh           # Build script for native library
├── HiveTest/
│   ├── HiveTestApp.swift  # App entry point
│   ├── ContentView.swift  # Main UI (peer list + buttons)
│   ├── Models/
│   │   └── HivePeer.swift # Peer and ACK status models
│   ├── ViewModels/
│   │   └── HiveViewModel.swift  # Mesh state management
│   ├── Extensions/
│   │   └── Color+Platform.swift # Cross-platform colors
│   ├── HiveBridge/
│   │   └── HiveFFI.swift  # UniFFI bindings placeholder
│   └── Info.plist         # iOS permissions
└── hive-apple-ffi/        # Rust FFI crate
    ├── Cargo.toml
    ├── src/lib.rs         # UniFFI exports
    └── uniffi-bindgen.rs  # Binding generator
```

## License

Apache-2.0
