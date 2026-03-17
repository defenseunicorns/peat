# Peat

[![CI](https://github.com/defenseunicorns/peat/actions/workflows/ci.yml/badge.svg)](https://github.com/defenseunicorns/peat/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

> A mesh protocol that connects heterogeneous systems — phones, servers, sensors, embedded devices, AI models — into a coordinated whole, across any transport, even when the network is degraded or denied.

## Overview

Tactical environments are heterogeneous. TAK operators carry phones. Sensors run on microcontrollers. AI inference runs on edge servers. Robots carry embedded computers. These systems speak different protocols, use different transports, and often can't reach each other directly.

Peat gives them a common coordination layer:

- **Any device joins**: Servers, phones, ESP32 sensors, Raspberry Pis, AI platforms — each contributes what it can
- **Any transport works**: QUIC, BLE mesh, UDP, HTTP — simultaneously, with automatic failover
- **Interoperability built in**: TAK/CoT bridge, Android bindings (ATAK plugin), embedded wire protocol, edge AI pipeline
- **Works disconnected**: CRDT-based state via Automerge — no central server, operates through network partitions
- **Scales when you need it**: Hierarchical aggregation means the protocol that works for 5 nodes also works for 1,000+

## How It Works

Peat organizes diverse systems through three phases:

**Discovery** — Nodes find each other via mDNS, BLE advertisements, static config, or geographic clustering. A phone discovers a nearby sensor. A server discovers edge nodes.

**Cell Formation** — Discovered nodes form cells based on capabilities. A cell might be a squad leader's phone, two sensors, and a UGV. Each node advertises what it can do; the cell composes those capabilities.

**Coordination** — Cells self-organize into a hierarchy for efficient state sharing. A sensor's reading flows up to its cell leader, aggregates with other cells at the zone level, and reaches the command post — without flooding the network.

## Ecosystem

| Crate | What it connects | Links |
|-------|-----------------|-------|
| **peat** | Protocol workspace: cells, hierarchy, sync, security, TAK bridge, edge AI, Android FFI | [Maven Central](https://central.sonatype.com/artifact/com.defenseunicorns/peat-ffi) · [repo](https://github.com/defenseunicorns/peat) |
| **peat-mesh** | P2P topology, QUIC/Iroh transport, Automerge CRDT sync, certificate enrollment | [crates.io](https://crates.io/crates/peat-mesh) · [repo](https://github.com/defenseunicorns/peat-mesh) |
| **peat-btle** | BLE mesh for Android, iOS, Linux, macOS, ESP32 — short-range device-to-device | [crates.io](https://crates.io/crates/peat-btle) · [Maven Central](https://central.sonatype.com/artifact/com.defenseunicorns/peat-btle) · [repo](https://github.com/defenseunicorns/peat-btle) |
| **peat-lite** | Embedded wire protocol for microcontrollers (256KB RAM, `no_std`) | [crates.io](https://crates.io/crates/peat-lite) · [Maven Central](https://central.sonatype.com/artifact/com.defenseunicorns/peat-lite) · [repo](https://github.com/defenseunicorns/peat-lite) |
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
│  APPLICATIONS    TAK Bridge · ATAK Plugin · Edge Inference      │
│                  Your app — anything that produces or consumes   │
│                  tactical data                                   │
├─────────────────────────────────────────────────────────────────┤
│  BINDINGS        peat-ffi (Kotlin/Swift via UniFFI + JNI)       │
│                  Android AAR on Maven Central                    │
├─────────────────────────────────────────────────────────────────┤
│  TRANSPORT       peat-mesh (QUIC) · peat-btle (BLE)             │
│                  peat-lite (embedded UDP) · HTTP                 │
│                  Multiple transports active simultaneously       │
├─────────────────────────────────────────────────────────────────┤
│  PROTOCOL        peat-protocol                                   │
│                  Cells, hierarchy, sync, capabilities, QoS       │
├─────────────────────────────────────────────────────────────────┤
│  SCHEMA          peat-schema (Protobuf wire format)              │
│                  Beacons, missions, capabilities, CoT, AI        │
├─────────────────────────────────────────────────────────────────┤
│  PERSISTENCE     peat-persistence (Redb, SQLite)                 │
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
| `peat-sim` | Network simulator |
| `peat-ble-test` | BLE integration test harness (Pi-to-Android) |

## Feature Flags

| Feature | Description |
|---------|-------------|
| `automerge-backend` | Automerge CRDT backend (pure Rust, open-source) |
| `ditto-backend` (default) | Ditto CRDT backend (proprietary, production-grade) |
| `lite-transport` | Embedded node transport via peat-lite |
| `bluetooth` | BLE mesh transport via peat-btle |

The **Automerge backend** is the open-source default for community use.

## Deployment

Peat components are packaged for Kubernetes via Helm, Zarf (air-gapped), and UDS:

```bash
make docker-build                         # Container images
helm install peat-mesh deploy/helm/peat-mesh/  # Helm
zarf package create                       # Air-gapped
make build-atak-plugin                    # ATAK plugin APK
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
| Mobile | UniFFI (Kotlin/Swift) + JNI — AAR on Maven Central |
| Edge AI | ONNX Runtime, GStreamer |
| Packaging | Helm, Zarf, UDS |

## Performance

The protocol that works for a 5-node squad also works for a 1,000-node operation:

| Metric | Result |
|--------|--------|
| Bandwidth reduction | 93-99% via hierarchical aggregation |
| Priority 1 latency | <5 seconds end-to-end propagation |
| Simulation validated | 1,000+ nodes; 24-node lab validated |
| Message complexity | O(n log n) vs. O(n^2) flat mesh |

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
- Multi-transport coordination (QUIC, BLE, UDP, HTTP) with PACE failover
- TAK/CoT interoperability bridge and ATAK plugin
- Certificate-based enrollment and tactical trust hierarchy
- Edge inference pipeline (ONNX YOLOv8)
- Three-phase hierarchical protocol with CRDT sync
- Simulation validated to 1,000+ nodes

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
