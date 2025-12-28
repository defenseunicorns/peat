# hive-btle

Bluetooth Low Energy mesh transport for tactical edge networking.

[![Crate](https://img.shields.io/crates/v/hive-btle.svg)](https://crates.io/crates/hive-btle)
[![Documentation](https://docs.rs/hive-btle/badge.svg)](https://docs.rs/hive-btle)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## Overview

`hive-btle` provides a cross-platform Bluetooth Low Energy mesh networking stack optimized for resource-constrained tactical devices. It enables peer-to-peer discovery, advertisement, connectivity, and efficient CRDT-based data synchronization over BLE.

### Key Features

- **Cross-Platform**: Linux, Android, iOS, macOS, Windows, ESP32
- **Power Efficient**: Designed for 18+ hour battery life on smartwatches
- **Long Range**: Coded PHY support for 300m+ range (BLE 5.0+)
- **Mesh Topology**: Hierarchical mesh with automatic peer discovery
- **Efficient Sync**: Delta-based CRDT synchronization over GATT
- **Embedded Ready**: `no_std` support for bare-metal targets

### Why hive-btle?

Traditional BLE mesh implementations (like those in commercial sync SDKs) often suffer from:

| Problem | Impact | hive-btle Solution |
|---------|--------|-------------------|
| Continuous scanning | 20%+ radio duty cycle | Batched sync windows (<5%) |
| Gossip-based discovery | All devices advertise constantly | Hierarchical discovery (leaf nodes don't scan) |
| Full mesh participation | Every device relays everything | Lite profile (minimal state, single parent) |
| **Result** | **3-4 hour watch battery** | **18-24 hour battery life** |

## Status

> **Pre-release**: This crate is under active development. APIs may change.

| Platform | Status | Notes |
|----------|--------|-------|
| Linux (BlueZ) | ✅ Complete | BlueZ 5.48+ required |
| macOS | ✅ Complete | CoreBluetooth, tested with ESP32 devices |
| iOS | ✅ Complete | CoreBluetooth (shared with macOS) |
| ESP32 | ✅ Complete | ESP-IDF NimBLE integration |
| Android | 🔄 In Progress | JNI bindings to Android Bluetooth API |
| Windows | 📋 Planned | WinRT Bluetooth APIs |

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hive-btle = { version = "0.1", features = ["linux"] }
```

### Feature Flags

| Feature | Description |
|---------|-------------|
| `std` (default) | Standard library support |
| `linux` | Linux/BlueZ support via `bluer` |
| `android` | Android support via JNI |
| `ios` | iOS support via CoreBluetooth |
| `macos` | macOS support via CoreBluetooth |
| `windows` | Windows support via WinRT |
| `embedded` | Embedded/no_std support |
| `esp32` | ESP32 support via ESP-IDF |
| `coded-phy` | Enable Coded PHY for extended range |
| `extended-adv` | Enable extended advertising |

## Quick Start

```rust
use hive_btle::{BleConfig, BluetoothLETransport, NodeId, PowerProfile};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create power-efficient configuration
    let config = BleConfig::hive_lite(NodeId::new(0x12345678))
        .with_power_profile(PowerProfile::LowPower);

    // Create platform adapter (Linux example)
    #[cfg(feature = "linux")]
    let adapter = hive_btle::platform::linux::BluerAdapter::new().await?;

    // Create and start transport
    let transport = BluetoothLETransport::new(config, adapter);
    transport.start().await?;

    // Transport is now advertising and ready for connections
    println!("Node {} is running", transport.node_id());

    // Connect to a discovered peer
    // let conn = transport.connect(&peer_id).await?;

    Ok(())
}
```

## Architecture

### Standalone vs HIVE Integration

hive-btle is designed for **dual use**:

1. **Standalone**: Pure embedded mesh (ESP32/Pico devices) without any full HIVE nodes
2. **HIVE Integration**: BLE transport for full HIVE nodes, with gateway translation to Automerge

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         HIVE Integration Mode                            │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                    Full HIVE Node (Phone)                          │  │
│  │  ┌─────────────────────────────────────────────────────────────┐  │  │
│  │  │                   AutomergeIroh                              │  │  │
│  │  │             (Full CRDT documents)                            │  │  │
│  │  └─────────────────────────────────────────────────────────────┘  │  │
│  │                            ↕                                       │  │
│  │  ┌─────────────────────────────────────────────────────────────┐  │  │
│  │  │              Translation Layer (HIVE repo)                   │  │  │
│  │  │        Maps: Automerge ↔ hive-btle lightweight               │  │  │
│  │  └─────────────────────────────────────────────────────────────┘  │  │
│  │                            ↕                                       │  │
│  │  ┌─────────────────────────────────────────────────────────────┐  │  │
│  │  │                      hive-btle                               │  │  │
│  │  │            (BLE transport + lightweight CRDTs)               │  │  │
│  │  └─────────────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                ↕ BLE                                     │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │               Embedded Node (ESP32/Pico)                           │  │
│  │                        hive-btle                                   │  │
│  │              (standalone, lightweight CRDTs)                       │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

**Why two modes?** Automerge is too resource-intensive for embedded targets (requires ~10MB+ RAM, `std` library). hive-btle's lightweight CRDTs (GCounter, Peripheral) provide the same semantics in <256KB RAM. Full HIVE nodes translate between formats.

See [ADR-041](docs/adr/041-multi-transport-embedded-integration.md) for the full architectural rationale.

### Component Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Application                             │
├─────────────────────────────────────────────────────────────┤
│                  BluetoothLETransport                        │
│         (MeshTransport trait implementation)                 │
├──────────────┬──────────────┬──────────────┬────────────────┤
│   Discovery  │     GATT     │     Mesh     │     Power      │
│  (Beacon,    │  (Service,   │  (Topology,  │  (Scheduler,   │
│   Scanner)   │   Protocol)  │   Routing)   │   Profiles)    │
├──────────────┴──────────────┴──────────────┴────────────────┤
│                     BleAdapter Trait                         │
├──────────┬──────────┬──────────┬──────────┬─────────────────┤
│  Linux   │ Android  │   iOS    │ Windows  │     ESP32       │
│ (BlueZ)  │  (JNI)   │(CoreBT)  │ (WinRT)  │   (NimBLE)      │
└──────────┴──────────┴──────────┴──────────┴─────────────────┘
```

### Core Components

| Component | Description |
|-----------|-------------|
| **Discovery** | HIVE beacon format, scanning, and advertising |
| **GATT** | BLE service definition and sync protocol |
| **Mesh** | Topology management and message routing |
| **PHY** | Physical layer configuration (1M, 2M, Coded) |
| **Power** | Radio scheduling and battery optimization |
| **Sync** | Delta-based CRDT synchronization |

## Power Profiles

| Profile | Radio Duty | Sync Interval | Watch Battery* |
|---------|------------|---------------|----------------|
| Aggressive | 20% | 1 second | ~6 hours |
| Balanced | 10% | 5 seconds | ~12 hours |
| **LowPower** | **2%** | **30 seconds** | **~20 hours** |
| UltraLow | 0.5% | 2 minutes | ~36 hours |

*Estimated for typical smartwatch (300mAh battery)

## GATT Service

hive-btle defines a custom GATT service for mesh communication:

| UUID | Characteristic | Description |
|------|----------------|-------------|
| `0xF47A` | Service | HIVE BLE Service (16-bit short form) |
| `0x0001` | Node Info | Node ID, capabilities, hierarchy level |
| `0x0002` | Sync State | Vector clock and sync metadata |
| `0x0003` | Sync Data | CRDT delta payloads (chunked) |
| `0x0004` | Command | Control commands (connect, disconnect, etc.) |
| `0x0005` | Status | Connection status and errors |

## Mesh-Wide Encryption

hive-btle supports optional mesh-wide encryption using ChaCha20-Poly1305 AEAD. When enabled, all documents are encrypted before transmission, providing confidentiality across multi-hop BLE relay.

### Enabling Encryption

```rust
use hive_btle::{HiveMesh, HiveMeshConfig, NodeId};

// Create a 32-byte shared secret (distribute securely to all mesh nodes)
let secret: [u8; 32] = [0x42; 32]; // Use a real secret in production!

// Configure mesh with encryption
let config = HiveMeshConfig::new(NodeId::new(0x12345678), "ALPHA-1", "DEMO")
    .with_encryption(secret);

let mesh = HiveMesh::new(config);

// All documents sent through this mesh are now encrypted
let encrypted_doc = mesh.build_document();
```

### How It Works

- **Algorithm**: ChaCha20-Poly1305 authenticated encryption
- **Key Derivation**: HKDF-SHA256 from shared secret + mesh ID
- **Overhead**: 30 bytes per document (2 marker + 12 nonce + 16 auth tag)
- **Wire Format**: `ENCRYPTED_MARKER (0xAE) | reserved | nonce | ciphertext`

### Multi-Hop Security

```
Node A ──encrypted──> Node B (relay) ──encrypted──> Node C
                           │
                    Can forward but
                    only decrypts if
                    has formation key
```

Nodes without the shared secret can relay encrypted documents but cannot read their contents. This provides end-to-end confidentiality even across untrusted relay nodes.

### Backward Compatibility

- Encrypted nodes can receive unencrypted documents (for gradual rollout)
- Unencrypted nodes will reject encrypted documents they can't decrypt

## Platform Requirements

### Linux

- BlueZ 5.48 or later
- D-Bus system bus access
- Bluetooth adapter with BLE support

```bash
# Check BlueZ version
bluetoothctl --version

# Ensure bluetooth service is running
sudo systemctl start bluetooth
```

### Android

- Android 6.0 (API 23) or later
- `BLUETOOTH`, `BLUETOOTH_ADMIN`, `ACCESS_FINE_LOCATION` permissions
- For BLE 5.0 features: Android 8.0 (API 26) or later

### iOS

- iOS 13.0 or later
- `NSBluetoothAlwaysUsageDescription` in Info.plist
- CoreBluetooth framework

## Examples

See the [`examples/`](examples/) directory:

- `linux_scanner.rs` - Scan for HIVE nodes on Linux
- `linux_advertiser.rs` - Advertise as a HIVE node
- `mesh_demo.rs` - Two-node mesh demonstration

Run examples with:

```bash
cargo run --example linux_scanner --features linux
```

## Testing

```bash
# Run unit tests (no hardware required)
cargo test

# Run with Linux platform tests (requires Bluetooth adapter)
cargo test --features linux

# Run specific test module
cargo test sync::
```

## Related Documentation

- [ADR-039: HIVE-BTLE Mesh Transport](https://github.com/revolveteam/hive/blob/main/docs/adr/039-hive-btle-mesh-transport.md) - Full architecture design
- [ADR-041: Multi-Transport Integration](https://github.com/revolveteam/hive/blob/main/docs/adr/041-multi-transport-embedded-integration.md) - HIVE integration architecture
- [ADR-035: HIVE-Lite Embedded Nodes](https://github.com/revolveteam/hive/blob/main/docs/adr/035-hive-lite-embedded-nodes.md) - Embedded node design

## Contributing

Contributions are welcome! Priority areas:

1. **Android Implementation** (#410) - JNI bindings to Android Bluetooth API
2. **Windows Implementation** (#412) - WinRT Bluetooth APIs
3. **Per-Peer E2EE** (#413) - Phase 2 of security: per-peer encryption for sensitive formations
4. **Hardware Testing** - Real-world validation on various devices

Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Acknowledgments

Developed by [(r)evolve - Revolve Team LLC](https://revolveteam.com) as part of the HIVE Protocol project.
