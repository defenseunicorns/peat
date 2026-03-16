# Peat

[![CI](https://github.com/defenseunicorns/peat/actions/workflows/ci.yml/badge.svg)](https://github.com/defenseunicorns/peat/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

> A distributed mesh protocol for human-machine-AI teaming that scales to 1,000+ nodes with O(n log n) message complexity. Designed for tactical edge environments with intermittent connectivity.

## Overview

Peat enables scalable coordination of autonomous nodes through:

- **Three-phase protocol**: Discovery → Cell Formation → Hierarchical Operations
- **CRDT-based state**: Eventual consistency via Automerge — operates through network partitions
- **Capability composition**: Additive, emergent, redundant, and constraint-based patterns
- **Hierarchical aggregation**: 93–99% bandwidth reduction vs. flat mesh
- **Multi-transport**: QUIC (Iroh), BLE mesh, UDP bypass, HTTP — simultaneous multi-path
- **Certificate-based trust**: Ed25519 identity, enrollment protocol, tiered permissions

## Ecosystem

Peat is a workspace of protocol crates backed by standalone libraries published on crates.io:

| Crate | Description | Links |
|-------|-------------|-------|
| **peat-mesh** | P2P topology, Iroh/QUIC transport, Automerge CRDT sync, certificate enrollment | [crates.io](https://crates.io/crates/peat-mesh) · [repo](https://github.com/defenseunicorns/peat-mesh) |
| **peat-btle** | BLE GATT mesh for Android/iOS/Linux/macOS/ESP32 | [crates.io](https://crates.io/crates/peat-btle) · [repo](https://github.com/defenseunicorns/peat-btle) |
| **peat-lite** | Embedded CRDT primitives and wire protocol (`no_std`) | [crates.io](https://crates.io/crates/peat-lite) · [repo](https://github.com/defenseunicorns/peat-lite) |
| **peat-gateway** | Multi-tenant control plane: enrollment, CDC, OIDC, envelope encryption | [repo](https://github.com/defenseunicorns/peat-gateway) |

## Quick Start

```bash
git clone https://github.com/defenseunicorns/peat.git
cd peat
cargo build

# Run tests
make check          # fmt + clippy + test

# Run the simulator
cargo run --bin peat-sim

# 24-node hierarchical validation
make validate
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and [DEVELOPMENT.md](DEVELOPMENT.md) for detailed build instructions.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  APPLICATION       TAK Bridge · Peat Inference · ATAK Plugin    │
├─────────────────────────────────────────────────────────────────┤
│  BINDINGS          peat-ffi (Kotlin/Swift via UniFFI + JNI)     │
├─────────────────────────────────────────────────────────────────┤
│  TRANSPORT         peat-mesh · peat-btle · peat-lite · HTTP     │
├─────────────────────────────────────────────────────────────────┤
│  PROTOCOL          peat-protocol (cells, hierarchy, sync, QoS)  │
├─────────────────────────────────────────────────────────────────┤
│  SCHEMA            peat-schema (Protobuf wire format)           │
├─────────────────────────────────────────────────────────────────┤
│  PERSISTENCE       peat-persistence (Redb, SQLite)              │
└─────────────────────────────────────────────────────────────────┘
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full five-layer breakdown.

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| `peat-schema` | Protobuf wire format definitions (beacon, mission, capability, security, CoT, AI) |
| `peat-protocol` | Core protocol: cells, hierarchy, sync, security, CRDT backends |
| `peat-transport` | HTTP/REST API layer (Axum) |
| `peat-persistence` | Storage backends (Redb, SQLite) |
| `peat-discovery` | Peer discovery (mDNS, static, hybrid) |
| `peat-ffi` | Mobile bindings (Kotlin/Swift via UniFFI + JNI) |
| `peat-inference` | Edge AI/ML pipeline (ONNX Runtime, YOLOv8, GStreamer) |
| `peat-tak-bridge` | TAK/ATAK CoT interoperability bridge |
| `peat-sim` | Network simulator — validates hierarchical protocol at scale |
| `peat-ble-test` | BLE integration test harness (Pi-to-Android) |

## Feature Flags

The `peat-protocol` crate uses feature flags for backend and transport selection:

| Feature | Description |
|---------|-------------|
| `ditto-backend` (default) | Ditto CRDT backend (proprietary, production-grade) |
| `automerge-backend` | Automerge CRDT backend (pure Rust, open-source) |
| `lite-transport` | Embedded node transport via peat-lite |
| `bluetooth` | BLE mesh transport via peat-btle |

The **Automerge backend** is the open-source default for community use. The Ditto backend is available under separate license for production deployments requiring its additional guarantees.

## Three-Phase Protocol

### Phase 1: Discovery
Nodes discover peers via mDNS, static configuration, or geohash-based geographic clustering. O(sqrt(n)) message complexity.

### Phase 2: Cell Formation
Discovered nodes form cells with deterministic leader election based on capability scoring. Intra-cell capability exchange and role assignment.

### Phase 3: Hierarchical Operations
Cells organize into zones for multi-level coordination. Differential state updates propagate through the hierarchy with priority-based routing.

## Deployment

Peat components are packaged for Kubernetes via Helm, Zarf (air-gapped), and UDS (Unicorn Delivery Service):

```bash
# Docker build
make docker-build

# Helm install (peat-mesh node)
helm install peat-mesh deploy/helm/peat-mesh/

# Zarf package (air-gapped)
zarf package create
```

The ATAK plugin provides Android integration for TAK/CoT interoperability:

```bash
make build-atak-plugin    # Build ATAK plugin APK
```

## Technology Stack

| Layer | Technology |
|-------|------------|
| Language | Rust (2021 edition) |
| CRDT Engine | Automerge + Iroh (open-source) / Ditto SDK (proprietary) |
| Transport | QUIC (Iroh), BLE (BlueZ/CoreBluetooth/NimBLE), UDP, HTTP (Axum) |
| Security | Ed25519 identity, X25519 key exchange, ChaCha20-Poly1305, HKDF |
| Serialization | Protobuf (prost) + Serde |
| Async Runtime | Tokio |
| Mobile | UniFFI (Kotlin/Swift) + JNI |
| Edge AI | ONNX Runtime, GStreamer |
| Packaging | Helm, Zarf, UDS |

## Validated Performance

| Metric | Result |
|--------|--------|
| Message complexity | O(n log n) vs. O(n^2) flat mesh |
| Bandwidth reduction | 93-99% via hierarchical aggregation |
| Priority 1 latency | <5 seconds end-to-end propagation |
| Scale | 1,000+ nodes validated in simulation; 24-node lab validated |

## Documentation

| Document | Purpose |
|----------|---------|
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute, PR process |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Development setup and build workflow |
| [Architecture](docs/ARCHITECTURE.md) | Five-layer architecture overview |
| [ADR Index](docs/adr/) | 53 Architecture Decision Records |
| [Protocol Specs](docs/spec/) | IETF-style protocol specifications |
| [Developer Guide](docs/guides/developer/DEVELOPER_GUIDE.md) | API reference, extending Peat |
| [Operator Guide](docs/guides/operator/OPERATOR_GUIDE.md) | Deployment, configuration, monitoring |
| [Whitepaper](docs/whitepaper/) | Technical whitepaper |

## Roadmap

**Completed**
- Three-phase hierarchical protocol with CRDT sync
- Multi-transport (QUIC, BLE, UDP, HTTP) with PACE failover
- Certificate-based enrollment and tactical trust hierarchy
- Network simulator with 1,000+ node validation
- ATAK plugin with CoT interoperability
- Edge inference pipeline (ONNX YOLOv8)

**In Progress**
- QoS enforcement: TTL, sync modes, bandwidth allocation
- Tombstone sync and distributed garbage collection

**Planned**
- MLS group key agreement for forward secrecy
- Protocol conformance test vectors
- Zarf/UDS integration patterns

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reporting and security policy.

## License

[Apache-2.0](LICENSE)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, PR process, and contributor guidelines.
