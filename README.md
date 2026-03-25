# PEAT — Protocol for Emergent Autonomous Teaming

> A hierarchical capability composition protocol using CRDTs for autonomous systems that scales to 1,000+ nodes with O(n log n) message complexity.

## Overview

PEAT enables scalable coordination of autonomous nodes through:

- **Three-phase protocol**: Discovery → Cell Formation → Hierarchical Operations
- **CRDT-based state**: Eventual consistency via Automerge/Ditto — operates through network partitions
- **Capability composition**: Additive, emergent, redundant, and constraint-based patterns
- **Hierarchical aggregation**: 93–99% bandwidth reduction vs. flat mesh
- **Multi-transport**: QUIC (Iroh), BLE mesh, UDP bypass, HTTP — simultaneous multi-path

## Quick Start

```bash
# Clone and build
git clone https://github.com/defenseunicorns/peat.git
cd peat
cargo build

# Run tests
cargo test --lib

# Run the simulator
cargo run --bin peat-sim

# Development workflow
make check       # format + lint + test
make pre-commit  # full pre-commit checks
```

See [DEVELOPMENT.md](DEVELOPMENT.md) for detailed setup and contributing guidelines.

## Repository Structure

```
peat/
├── Cargo.toml              # Workspace configuration
├── Makefile                # Development commands
├── DEVELOPMENT.md          # Development quickstart
│
├── peat-schema/            # Protobuf message definitions (wire format)
├── peat-protocol/          # Core protocol: cells, hierarchy, sync, security
├── peat-transport/         # HTTP/REST API layer (Axum)
├── peat-persistence/       # Storage backends (Redb, SQLite)
├── peat-discovery/         # Peer discovery (mDNS, static, hybrid)
├── peat-ffi/               # Mobile bindings (Kotlin/Swift via UniFFI)
├── peat-inference/         # Edge AI/ML pipeline (ONNX, YOLOv8)
├── peat-tak-bridge/        # TAK/ATAK CoT interoperability
├── peat-sim/               # Network simulator
├── peat-ble-test/          # BLE integration test harness
│
├── docs/                   # Documentation
│   ├── ARCHITECTURE.md     # Five-layer architecture overview
│   ├── adr/                # Architecture Decision Records
│   ├── guides/             # Developer & operator guides
│   ├── spec/               # Protocol specification (IETF-style)
│   └── whitepaper/         # Technical whitepaper
│
└── examples/               # Embedded examples (ESP32)
```

### External Crate Ecosystem

| Crate | Description | Repo |
|-------|-------------|------|
| [peat-mesh](https://crates.io/crates/peat-mesh) | P2P topology, Iroh/QUIC transport, Automerge sync | [defenseunicorns/peat-mesh](https://github.com/defenseunicorns/peat-mesh) |
| [peat-btle](https://crates.io/crates/peat-btle) | BLE GATT mesh for Android/iOS/Linux/ESP32 | [defenseunicorns/peat-btle](https://github.com/defenseunicorns/peat-btle) |
| [peat-lite](https://crates.io/crates/peat-lite) | Embedded UDP protocol + wire format (no_std) | [defenseunicorns/peat-lite](https://github.com/defenseunicorns/peat-lite) |

## Three-Phase Protocol

### Phase 1: Discovery
Nodes discover peers via mDNS, static configuration, or geohash-based geographic clustering. O(√n) message complexity.

### Phase 2: Cell Formation
Discovered nodes form cells with deterministic leader election based on capability scoring. Intra-cell capability exchange and role assignment.

### Phase 3: Hierarchical Operations
Cells organize into zones for multi-level coordination. Differential state updates propagate through the hierarchy with priority-based routing.

## Technology Stack

| Layer | Technology |
|-------|------------|
| Language | Rust (2021 edition) |
| CRDT Engine | Automerge + Iroh (pure OSS) / Ditto SDK (production) |
| Transport | QUIC (Iroh), BLE (BlueZ/CoreBluetooth), UDP, HTTP (Axum) |
| Serialization | Protobuf (prost) + Serde |
| Async Runtime | Tokio |
| Mobile | UniFFI (Kotlin/Swift) + JNI |
| Edge AI | ONNX Runtime, GStreamer |

## Success Metrics

- **Scalability**: O(n log n) message complexity (vs. O(n²) flat mesh)
- **Efficiency**: 93–99% bandwidth reduction via hierarchical aggregation
- **Latency**: Priority 1 updates propagate in <5 seconds
- **Scale**: 1,000+ nodes validated in simulation; 24-node lab validated

## Security

Peat implements a five-layer security model designed for contested environments where nodes may be captured, networks may be compromised, and connectivity is intermittent.

### Cryptographic Primitives

| Function | Algorithm | Key Size |
|----------|-----------|----------|
| Device Identity | Ed25519 | 256-bit |
| Symmetric Encryption | ChaCha20-Poly1305 AEAD | 256-bit |
| Key Exchange | X25519 Diffie-Hellman | 256-bit |
| Hashing | SHA-256 | 256-bit |
| Key Derivation | HKDF-SHA256 | — |

### Security Layers

**Layer 1 — Device Identity.** Each device generates an Ed25519 keypair. The device ID is the SHA-256 hash of its public key. Peers authenticate via challenge-response: the challenger sends a nonce, the device signs `nonce || challenger_id || timestamp`, and the challenger verifies the signature.

**Layer 2 — Transport Encryption.** All peer connections use TLS 1.3 via QUIC (built into Iroh), providing authenticated encrypted streams with ephemeral key exchange. There is no unencrypted mode.

**Layer 3 — Protocol Security.** Cell admission requires proof of a pre-shared formation key via HMAC-SHA256 — the key itself never crosses the wire. All CRDT operations are cryptographically signed by the originating device to prevent injection of forged state. Role-based access control (RBAC) enforces five privilege levels: Observer, Member, Operator, Leader, and Supervisor.

**Layer 4 — Group Key Agreement.** Cells use MLS (Messaging Layer Security, RFC 9420) for group key management, providing forward secrecy (removed members cannot read future messages) and post-compromise security. Symmetric encryption uses ChaCha20-Poly1305 AEAD.

**Layer 5 — Audit.** All security-relevant events (authentication attempts, privilege changes, data access) are logged to a tamper-evident audit trail for forensics and compliance.

### Authorization Model

| Role | Read | Write | Delete | Modify Membership | Issue Commands | Classified Data |
|------|:----:|:-----:|:------:|:-----------------:|:--------------:|:---------------:|
| Observer | ✓ | | | | | Own level only |
| Member | ✓ | ✓ | Own | | | Own level |
| Operator | ✓ | ✓ | ✓ | | ✓ | Own level |
| Leader | ✓ | ✓ | ✓ | ✓ | ✓ | Cell level |
| Supervisor | ✓ | ✓ | ✓ | ✓ | ✓ | Parent level |

Authority propagates as CRDT data — delegation flows downward through the hierarchy, and revocation is immediate. See [ADR-006](docs/adr/006-security-authentication-authorization.md) for the full specification.

## FAQs

### What's the difference between the Ditto and Automerge+Iroh backends?

Peat supports two complete backend strategies — these are full stacks for storage, sync, and transport, not interchangeable components.

**Ditto (Commercial, All-In-One):** Proprietary CRDT engine with built-in RocksDB persistence, integrated Bluetooth/WiFi/TCP transport, and automatic mesh discovery. Zero networking code to write — plug and play. Tradeoffs: vendor licensing, TCP-only (no QUIC multi-path), ~60% wire compression, no source access.

**Automerge + Iroh (Pure Open Source):** Automerge CRDTs (MIT, ~90% columnar compression) plus Iroh QUIC transport (Apache 2.0) with native multi-path, connection migration, and stream multiplexing. Full source control — can optimize for contested networks with 20-30% packet loss. Tradeoffs: requires implementing discovery plugins, repository wrappers, and a query engine (~16-20 weeks of integration work).

| Scenario | Recommendation |
|----------|---------------|
| Government/DoD with licensing approval | Ditto — zero networking complexity |
| Open-source requirement | Automerge+Iroh — full Apache 2.0 |
| Tactical multi-path networks (radio + satellite) | Automerge+Iroh — QUIC multi-path vs TCP single-path |
| Commercial product with licensing budget | Ditto — faster time-to-market |

### How much effort does it take to integrate?

Three integration depths, depending on how tightly you want to couple:

**Shallow (REST/HTTP, ~500-1000 LOC).** Use Peat's HTTP transport layer as a bridge. Encode/decode via CoT protocol adapters. Works from any language with an HTTP client. Latency ~500ms+. Example: TAK Server sending track updates to Peat's REST endpoint.

**Medium (Protobuf + Library, ~2000-5000 LOC).** Link against `peat-protocol` as a Rust library or use `peat-ffi` bindings (Kotlin, Swift, C). Subscribe to CRDT collections directly. Latency <50ms. Example: Android app using UniFFI bindings to participate in cell formation.

**Deep (Transport Layer, ~5000+ LOC).** Implement a custom transport or embed Peat's cell formation logic. Direct access to CRDT document operations. Latency <5ms. Example: ROS2 DDS bridge with Peat as the coordination transport.

### How would adopting Peat change my current work?

Peat augments existing systems — it does not replace them.

- **TAK stays.** Peat's TAK bridge makes Peat nodes appear as native TAK endpoints. Operators keep using ATAK as their common operating picture. Peat handles machine-to-machine coordination underneath.
- **Radios stay.** Peat uses your existing network links (tactical radios, Starlink, MANET, 5G) as transports. It reduces bandwidth demand by 93-99% through hierarchical aggregation, making constrained links viable.
- **ROS2 stays.** A DDS adapter bridges ROS2 topics into Peat's CRDT mesh. Robots keep their existing autonomy stacks.

The migration path is gradual: start with a TAK bridge on one server (Phase 1), scale autonomous node count as Peat handles the coordination load (Phase 2), then deploy disconnected mesh operations where cells operate autonomously and reconcile on reconnect (Phase 3). At no point do operators need retraining or existing C2 interfaces need replacement.

### What does everyone need to agree on to use Peat?

Four things:

1. **Formation key.** A pre-shared secret distributed out-of-band (pre-deployment briefing, key management system). All nodes in a cell must have the same key. Rotated per-mission.

2. **Protocol version.** Nodes must run compatible schema versions (major version must match). Checked automatically — incompatible nodes silently ignore each other rather than corrupt state.

3. **Discovery mode.** All nodes in a deployment must agree on how to find each other: mDNS (local subnet auto-discovery), static peer list (pre-configured IPs), or hybrid. Mixed modes within a cell won't work.

4. **Network configuration.** Matching bind ports, firewall rules allowing P2P traffic, and UTC time synchronized within ±5 seconds (for challenge freshness). If using static discovery, all nodes need the seed peer addresses.

That's it. Capability advertisement, cell formation, leader election, hierarchical organization, and CRDT sync all happen automatically once nodes can discover and authenticate each other.

## Documentation

| Document | Purpose |
|----------|---------|
| [DEVELOPMENT.md](DEVELOPMENT.md) | Development setup and workflow |
| [Architecture](docs/ARCHITECTURE.md) | Five-layer architecture overview |
| [Developer Guide](docs/guides/developer/DEVELOPER_GUIDE.md) | API reference, extending PEAT |
| [Operator Guide](docs/guides/operator/OPERATOR_GUIDE.md) | Deployment, configuration, monitoring |
| [ADRs](docs/adr/) | Architecture Decision Records |
| [Whitepaper](docs/whitepaper/) | Technical whitepaper |

## License

Apache-2.0

## Contributing

Contributions are welcome! See [DEVELOPMENT.md](DEVELOPMENT.md) for guidelines.
