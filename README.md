# Peat

> A hierarchical capability composition protocol using CRDTs for autonomous systems that scales to 1,000+ nodes with O(n log n) message complexity.

## Overview

Peat enables scalable coordination of autonomous nodes through:

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

## Documentation

| Document | Purpose |
|----------|---------|
| [DEVELOPMENT.md](DEVELOPMENT.md) | Development setup and workflow |
| [Architecture](docs/ARCHITECTURE.md) | Five-layer architecture overview |
| [Developer Guide](docs/guides/developer/DEVELOPER_GUIDE.md) | API reference, extending Peat |
| [Operator Guide](docs/guides/operator/OPERATOR_GUIDE.md) | Deployment, configuration, monitoring |
| [ADRs](docs/adr/) | Architecture Decision Records |
| [Whitepaper](docs/whitepaper/) | Technical whitepaper |

## License

Apache-2.0

## Contributing

Contributions are welcome! See [DEVELOPMENT.md](DEVELOPMENT.md) for guidelines.
