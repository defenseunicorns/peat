# HIVE-Lite

Resource-constrained HIVE protocol implementation for embedded sensors.

## Overview

HIVE-Lite enables ESP32-based devices (M5Stack Core2, Waveshare UGV Beast ESP32 controller, etc.) to participate as **first-class mesh members** in a HIVE network. Unlike bridge-based approaches, Lite nodes speak the same protocol as Full nodes with capability negotiation.

See [ADR-035: HIVE-Lite Embedded Sensor Nodes](../docs/adr/035-hive-lite-embedded-nodes.md) for architecture details.

## Features

- **First-class mesh participation** - Same protocol as HIVE-Full, no bridging required
- **Primitive CRDTs** - LWW-Register, G-Counter, PN-Counter for sensor data
- **Ephemeral operation** - No persistent storage required
- **Capability negotiation** - Announces what it supports, Full nodes adapt
- **UDP gossip** - Lightweight mesh communication

## Target Hardware

### Primary: M5Stack Core2
- ESP32-D0WDQ6-V3 (dual-core 240MHz)
- 520KB SRAM + 8MB PSRAM
- WiFi 802.11 b/g/n
- Sensors: 6-axis IMU (MPU6886), microphone (SPM1423), touch (FT6336U)

### Secondary: ESP32 Dev Boards
- Any ESP32 with WiFi capability
- Minimum 256KB RAM available for HIVE-Lite

## Building

### Prerequisites

```bash
# Install Rust ESP toolchain
cargo install espup
espup install

# Install flash tool
cargo install espflash

# Source the environment (add to .bashrc/.zshrc)
. ~/export-esp.sh
```

### Build

```bash
# Build for ESP32
cargo build --release

# Flash to device
espflash flash --release --monitor
```

## Architecture

```
┌─────────────────────────────────────────┐
│            HIVE-Lite Node               │
├─────────────────────────────────────────┤
│  ┌─────────────────────────────────┐    │
│  │      Application Layer          │    │
│  │  - Sensor reading               │    │
│  │  - Local decisions              │    │
│  │  - Display/alerts               │    │
│  └─────────────┬───────────────────┘    │
│                │                        │
│  ┌─────────────▼───────────────────┐    │
│  │       CRDT Layer                │    │
│  │  - LWW-Register (sensor data)   │    │
│  │  - G-Counter (event counts)     │    │
│  │  - OR-Set (active alerts)       │    │
│  └─────────────┬───────────────────┘    │
│                │                        │
│  ┌─────────────▼───────────────────┐    │
│  │      Gossip Protocol            │    │
│  │  - Peer discovery               │    │
│  │  - State sync                   │    │
│  │  - Capability advertisement     │    │
│  └─────────────┬───────────────────┘    │
│                │                        │
│  ┌─────────────▼───────────────────┐    │
│  │      UDP Transport              │    │
│  │  - Multicast discovery          │    │
│  │  - Unicast sync                 │    │
│  └─────────────────────────────────┘    │
└─────────────────────────────────────────┘
```

## Memory Budget (256KB target)

| Component | Budget | Notes |
|-----------|--------|-------|
| Network stack | 64KB | esp-wifi + smoltcp |
| CRDT state | 64KB | ~100 LWW registers |
| Gossip buffers | 32KB | 64 x 512-byte packets |
| Protocol state | 16KB | Peer table, routing |
| Application | 80KB | Sensor logic, display |

## Protocol Compatibility

HIVE-Lite speaks the same wire protocol as HIVE-Full. Differences are handled via capability flags:

```rust
bitflags! {
    pub struct NodeCapabilities: u16 {
        const PERSISTENT_STORAGE = 0b0000_0001;
        const RELAY_CAPABLE      = 0b0000_0010;
        const DOCUMENT_CRDT      = 0b0000_0100;
        const PRIMITIVE_CRDT     = 0b0000_1000;  // Lite nodes have this
        const BLOB_STORAGE       = 0b0001_0000;
        const HISTORY_QUERY      = 0b0010_0000;
        const AGGREGATION        = 0b0100_0000;
    }
}
```

## License

Apache-2.0
