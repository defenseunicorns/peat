# HIVE-BTLE

**Tactical BLE Mesh Networking for Disconnected Operations**

---

## What It Is

HIVE-BTLE is a Bluetooth Low Energy mesh networking library that enables secure, resilient communication between devices when traditional infrastructure is unavailable. Built for tactical and field operations where connectivity cannot be guaranteed.

---

## Key Capabilities

| Feature | Description |
|---------|-------------|
| **Mesh Networking** | Automatic peer discovery, multi-hop relay, self-healing topology |
| **CRDT Sync** | Conflict-free state synchronization across all nodes |
| **End-to-End Encryption** | ChaCha20-Poly1305 with mesh-wide or per-peer keys |
| **Cryptographic Identity** | Ed25519 device identity with X25519 key exchange |
| **Location Tracking** | GPS position sharing with configurable privacy |
| **Emergency Broadcast** | Priority SOS propagation with mesh-wide ACK |
| **Chat Messaging** | Encrypted group messaging across the mesh |

---

## Platform Support

| Platform | Status | Stack |
|----------|--------|-------|
| Android | Production | Kotlin + Native JNI |
| Linux | Production | BlueZ/D-Bus |
| iOS | Beta | CoreBluetooth |
| macOS | Beta | CoreBluetooth |
| Windows | Beta | WinRT |
| ESP32 | Alpha | ESP-IDF |

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Application Layer                     │
├─────────────────────────────────────────────────────────┤
│  HiveMesh API  │  Chat CRDT  │  Location  │  Emergency  │
├─────────────────────────────────────────────────────────┤
│         Document Sync (CRDT)  │  Delta Encoding         │
├─────────────────────────────────────────────────────────┤
│    ChaCha20-Poly1305    │   Ed25519   │   X25519       │
├─────────────────────────────────────────────────────────┤
│              BLE Transport (GATT/Advertising)           │
└─────────────────────────────────────────────────────────┘
```

---

## Performance

| Metric | Value |
|--------|-------|
| Sync latency | 100-300 µs per document |
| Encryption overhead | 30 bytes + ~100 µs |
| Throughput | 38 Mbps encrypted (exceeds BLE bandwidth) |
| Document size | 50-600 bytes typical |
| Battery impact | Optimized for wearables/sensors |

---

## Use Cases

- **Tactical Teams** — Squad-level mesh for dismounted operations
- **First Responders** — Interoperable comms in disaster zones
- **Industrial IoT** — Sensor networks in RF-denied environments
- **Outdoor Recreation** — Group tracking without cell coverage
- **Asset Tracking** — Equipment location in warehouses/facilities

---

## Integration

**Android (Gradle)**
```kotlin
implementation("com.revolveteam:hive:0.1.0-rc7")
```

**Rust**
```toml
[dependencies]
hive-btle = { version = "0.1.0", features = ["linux"] }
```

---

## Quick Start

```kotlin
// Initialize mesh with encryption
val mesh = HiveMesh.createWithIdentity(nodeId, "ALPHA-1", "SQUAD", secret)

// Set location and broadcast
mesh.updateLocation(lat, lon, alt)
mesh.sendEmergency(timestamp)  // SOS with location

// Receive from peers
mesh.onBleData(peerAddress, data, timestamp)
```

---

## Security Model

- **Mesh Genesis** — Cryptographic mesh identity with founder key
- **Device Binding** — Hardware-bound Ed25519 keypairs
- **TOFU Registry** — Trust-on-first-use with attestation verification
- **Replay Protection** — Timestamped messages with nonce rotation
- **Forward Secrecy** — Session key derivation via X25519

---

## Open Source

Apache 2.0 License

**Repository:** [Radicle](https://app.radicle.xyz/nodes/rosa.radicle.xyz/rad:z458mp9Um3AYNQQFMdHaNEUtmiohq)

**Contact:** Revolve Team LLC

---

*HIVE-BTLE: Mesh when it matters.*
