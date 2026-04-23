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

| Crate | What it provides | Links |
|-------|-----------------|-------|
| **peat-protocol** | **The SDK entry point.** Protocol facade — cells, hierarchy, sync, security, capabilities. Re-exports `peat-mesh` and `peat-schema` so one dep covers the stack. | [crates.io](https://crates.io/crates/peat-protocol) · [repo](https://github.com/defenseunicorns/peat) |
| **peat-schema** | Wire format (Protobuf): beacons, missions, capabilities, CoT, AI. Pulled in transitively by `peat-protocol`; depend on directly only if you need the types without the protocol. | [crates.io](https://crates.io/crates/peat-schema) · [repo](https://github.com/defenseunicorns/peat) |
| **peat-mesh** | P2P topology, QUIC/Iroh transport, Automerge CRDT sync, certificate enrollment. Pulled in transitively by `peat-protocol`. | [crates.io](https://crates.io/crates/peat-mesh) · [repo](https://github.com/defenseunicorns/peat-mesh) |
| **peat-btle** | BLE mesh for Android, iOS, Linux, macOS, ESP32 — short-range device-to-device. Enable via the `bluetooth` feature. | [crates.io](https://crates.io/crates/peat-btle) · [Maven Central](https://central.sonatype.com/artifact/com.defenseunicorns/peat-btle) · [repo](https://github.com/defenseunicorns/peat-btle) |
| **peat-lite** | Embedded wire protocol for microcontrollers (256KB RAM, `no_std`). Enable via the `lite-transport` feature. | [crates.io](https://crates.io/crates/peat-lite) · [Maven Central](https://central.sonatype.com/artifact/com.defenseunicorns/peat-lite) · [repo](https://github.com/defenseunicorns/peat-lite) |
| **peat-ffi** | Mobile bindings (Kotlin / Swift via UniFFI + JNI). Use when you need to consume Peat from Android / iOS rather than Rust. | [Maven Central](https://central.sonatype.com/artifact/com.defenseunicorns/peat-ffi) · [repo](https://github.com/defenseunicorns/peat) |
| **peat-gateway** | Multi-tenant control plane: enrollment, CDC, OIDC, envelope encryption. Server-side, not part of the embedded SDK surface. | [repo](https://github.com/defenseunicorns/peat-gateway) |

## Integrate Peat

You depend on **one crate** — `peat-protocol`. It re-exports `peat-mesh` and `peat-schema` so the rest of the stack comes along automatically.

### Add the dep

```toml
# Cargo.toml
[dependencies]
# During the rc window, use the exact-version pin — Cargo does not
# select pre-release versions by default with the caret form.
peat-protocol = "=0.9.0-rc.1"

# Once 0.9.0 stable is published, the normal selector works:
# peat-protocol = "0.9"
```

### Pick the features you need

| Building this | Enable |
|---------------|--------|
| Default mesh (Automerge CRDT + Iroh QUIC) | nothing — `automerge-backend` is on by default |
| BLE mesh (Android/iOS/Linux/ESP32) | `bluetooth` |
| Embedded / size-constrained UDP gateway to microcontrollers | `lite-transport` |
| Lite/embedded only (no Automerge backend) | `default-features = false, features = ["lite-transport"]` |

```toml
# Example: add BLE on top of the default mesh stack
peat-protocol = { version = "=0.9.0-rc.1", features = ["bluetooth"] }
```

### Minimum working example

The example also requires `anyhow = "1"`, `tempfile = "3"`, and a Tokio runtime (`tokio = { version = "1", features = ["full"] }`).

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use peat_protocol::network::IrohTransport;
use peat_protocol::storage::{AutomergeBackend, AutomergeStore};
use peat_protocol::storage::capabilities::SyncCapable;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let bind: SocketAddr = "0.0.0.0:0".parse()?;

    // Open a CRDT store and a P2P transport in parallel.
    let (store, transport) = tokio::join!(
        tokio::task::spawn_blocking({
            let path = dir.path().to_path_buf();
            move || AutomergeStore::open(&path)
        }),
        IrohTransport::from_seed_at_addr("my-node-seed", bind),
    );

    // `store??` unwraps two layers: the outer Result is from
    // spawn_blocking (JoinError), the inner one is from AutomergeStore::open.
    let store = Arc::new(store??);
    let transport = Arc::new(transport?);

    // Wire them together and start syncing with peers.
    let backend = AutomergeBackend::with_transport(store, transport);
    backend.start_sync()?;

    // ...your app code: subscribe to documents, advertise capabilities,
    //    join cells, etc. See the Developer Guide for the full surface.
    Ok(())
}
```

### Three integration depths

Most adopters fit one of three patterns:

- **Shallow (REST/HTTP, ~500–1000 LOC).** Use Peat's HTTP transport layer as a bridge. Encode/decode via CoT protocol adapters. Works from any language with an HTTP client. Latency ~500ms+. Example: TAK Server sending track updates to Peat's REST endpoint.
- **Medium (library, ~2000–5000 LOC).** What this section is about — link `peat-protocol` directly, or `peat-ffi` from Kotlin/Swift/C. Subscribe to CRDT collections directly, participate in cell formation. Latency <50ms.
- **Deep (transport layer, ~5000+ LOC).** Implement a custom transport or embed Peat's cell formation logic. Direct access to CRDT document operations. Latency <5ms. Example: a ROS2 DDS bridge with Peat as the coordination transport.

For deeper guidance see the [Developer Guide](docs/guides/developer/DEVELOPER_GUIDE.md).

## Develop on Peat

If you're working **on** the protocol itself rather than integrating it, this is the workflow:

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
| `examples/peat-tak-bridge` | TAK/ATAK CoT interoperability bridge |
| `examples/peat-ble-test` | BLE integration test harness (Pi-to-Android) |

## Feature Flags

| Feature | Description |
|---------|-------------|
| `automerge-backend` (default) | Automerge CRDT backend with Iroh QUIC transport (pure Rust, Apache-2.0/MIT) |
| `lite-transport` | Embedded node transport via peat-lite |
| `bluetooth` | BLE mesh transport via peat-btle |

Peat uses **Automerge + Iroh** as its CRDT and transport stack — pure Rust, Apache-2.0 / MIT, with native multi-path and connection migration. See [ADR-011](docs/adr/011-ditto-vs-automerge-iroh.md) for the design rationale.

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
| CRDT Engine | Automerge + Iroh (open-source) |
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

## Security

Peat's security model starts from a fundamental design principle: **autonomy under human authority**. Within the human-machine-AI teaming framework, Peat enables machines and AI to coordinate at machine speed while humans retain command authority at every level of the hierarchy. The objective is decision superiority — giving human commanders better information faster and executing their intent more effectively — not replacing humans in the decision loop.

This means the security architecture must enforce not just confidentiality and integrity, but also **authority boundaries**. Every autonomous action traces back to a human-delegated mandate. Humans set the rules of engagement, define formation policies, and retain the ability to override, revoke, or constrain autonomous behavior at any time. The hierarchy's role-based access control and downward authority propagation exist specifically to preserve this chain of command.

With that context, Peat implements a five-layer security model designed for contested environments where nodes may be captured, networks may be compromised, and connectivity is intermittent.

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

Authority propagates as CRDT data — delegation flows downward through the hierarchy, and revocation is immediate. Humans at any level can tighten constraints, revoke delegations, or assume direct control. Even when cells operate autonomously through network partitions, they do so within the authority boundaries last set by their human commanders.

See [ADR-006](docs/adr/006-security-authentication-authorization.md) for the full specification.

## FAQs

### What CRDT and transport stack does Peat use?

Peat uses **Automerge + Iroh**: Automerge CRDTs (MIT, ~90% columnar compression) for conflict-free state replication plus Iroh QUIC transport (Apache 2.0) with native multi-path, connection migration, and stream multiplexing. This pure-OSS stack can be tuned for contested networks with 20-30% packet loss. See [ADR-011](docs/adr/011-ditto-vs-automerge-iroh.md) for the design rationale.

### How much effort does it take to integrate?

See [Three integration depths](#three-integration-depths) above for the shallow/medium/deep breakdown. The library-level path (medium) is what most adopters land on; it is the workflow [Integrate Peat](#integrate-peat) walks through end to end.

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
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute, PR process |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Development setup and build workflow |
| [Architecture](docs/ARCHITECTURE.md) | Five-layer architecture overview |
| [ADR Index](docs/adr/) | 53 Architecture Decision Records |
| [Protocol Specs](spec/) | Protocol specification drafts and Protobuf schemas |
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
