# PEAT Protocol Architecture

**Status**: Living Document
**Last Updated**: 2025-01-07
**Version**: 1.0

## Overview

PEAT (Hierarchical Intelligence for Virtual Environments) is a decentralized mesh protocol for human-machine teaming in tactical environments. It provides the foundational communication, synchronization, and coordination primitives that enable autonomous and semi-autonomous systems to form dynamic teams ("cells") and operate effectively in contested, denied, or limited communication environments.

### What PEAT Is

PEAT is a **protocol specification** with a reference implementation in Rust. Think of it as:

- **TCP/IP for autonomy**: Just as TCP/IP provides reliable communication primitives, PEAT provides reliable synchronization and coordination primitives
- **HTTP for state**: Just as HTTP provides request/response semantics, PEAT provides eventual consistency semantics via CRDTs
- **Protobuf for tactical data**: Just as Protobuf defines wire formats, PEAT defines tactical entity schemas (tracks, capabilities, missions)

### What PEAT Is Not

- **Not an application**: PEAT is infrastructure that applications build on
- **Not a replacement for tactical systems**: PEAT bridges and extends existing systems like TAK
- **Not AI/ML**: PEAT provides the data fabric that AI systems consume and produce

---

## Architecture Layers

PEAT is organized into five distinct layers, each with clear responsibilities:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         APPLICATION LAYER                                    │
│  ┌─────────────┐  ┌─────────────────┐  ┌────────────┐                      │
│  │ TAK Bridge  │  │ PEAT Inference  │  │  Your App  │                      │
│  │ (CoT ↔ PEAT)│  │  (Edge ML)      │  │            │                      │
│  └─────────────┘  └─────────────────┘  └────────────┘                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                          BINDING LAYER                                       │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                            peat-ffi                                      ││
│  │              (Kotlin/Swift via UniFFI + JNI bindings)                    ││
│  └─────────────────────────────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────────────────────────────┤
│                         TRANSPORT LAYER                                      │
│  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────┐  ┌────────────┐ │
│  │   peat-mesh     │  │  peat-discovery │  │peat-transport│  │ peat-lite  │ │
│  │ (Peer topology) │  │  (mDNS/Static)  │  │(HTTP/Axum)   │  │ (ESP32 UDP)│ │
│  └─────────────────┘  └─────────────────┘  └──────────────┘  └────────────┘ │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                      peat-btle (external)                                ││
│  │            (BLE mesh for Android/iOS/Windows/ESP32)                      ││
│  └─────────────────────────────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────────────────────────────┤
│                         PROTOCOL LAYER                                       │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                          peat-protocol                                   ││
│  │  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  ┌───────────────┐    ││
│  │  │ DocumentStore│  │  Security   │  │Coordination│  │TransportManager│   ││
│  │  │ (CRDT sync) │  │ (PKI+Auth)  │  │(Cell mgmt) │  │(Abstract I/O)  │   ││
│  │  └─────────────┘  └─────────────┘  └────────────┘  └───────────────┘    ││
│  │  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  ┌───────────────┐    ││
│  │  │ QueryEngine │  │ Validation  │  │  Hierarchy │  │ UDP Bypass    │    ││
│  │  │ (DQL/Geo)   │  │ (Schema)    │  │ (Leader)   │  │ (Low latency) │    ││
│  │  └─────────────┘  └─────────────┘  └────────────┘  └───────────────┘    ││
│  └─────────────────────────────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────────────────────────────┤
│                          SCHEMA LAYER                                        │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                          peat-schema                                     ││
│  │   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌────────────┐  ││
│  │   │ beacon.proto │  │ mission.proto│  │security.proto│  │ cot.proto  │  ││
│  │   │ (Tracks)     │  │ (Tasks)      │  │ (Auth)       │  │ (TAK/CoT)  │  ││
│  │   └──────────────┘  └──────────────┘  └──────────────┘  └────────────┘  ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Layer Descriptions

### 1. Schema Layer (`peat-schema`)

**Purpose**: Define the wire format for all PEAT messages

**Responsibilities**:
- Protobuf message definitions for all tactical entities
- Schema versioning and compatibility
- Code generation for Rust (prost) and other languages

**Key Schemas**:
| Schema | Purpose |
|--------|---------|
| `beacon.proto` | Track updates, position reports, node identity |
| `mission.proto` | Mission tasking, objectives, phases |
| `capability.proto` | Device capabilities, sensor/actuator advertisements |
| `security.proto` | Authentication challenges, device identity, signatures |
| `cot.proto` | Cursor-on-Target (TAK interoperability) |
| `ai.proto` | ML model metadata, inference requests/responses |

**Dependencies**: None (foundation layer)

---

### 2. Protocol Layer (`peat-protocol`)

**Purpose**: Core synchronization, coordination, and security primitives

**Responsibilities**:
- CRDT-based document synchronization (Automerge or Ditto backend)
- Device authentication and authorization (PKI, RBAC)
- Cell formation, leader election, and hierarchy management
- Query engine for spatial and semantic queries
- Transport abstraction for backend-agnostic networking

**Submodules**:

| Module | Purpose | ADRs |
|--------|---------|------|
| `document_store` | CRDT sync, collection management | ADR-007, ADR-011 |
| `security` | Device PKI, user auth, encryption | ADR-006, ADR-044 |
| `coordination` | Cell lifecycle, leader election | ADR-014, ADR-024 |
| `query` | DQL parser, geohash queries | ADR-021 |
| `transport` | Backend abstraction, bypass channel | ADR-032, ADR-042 |
| `validation` | Schema validation, type checking | ADR-012 |
| `hierarchy` | Hierarchical aggregation, flow control | ADR-009, ADR-027 |

**Dependencies**: `peat-schema`

**Feature Flags**:
- `ditto-backend`: Use Ditto for CRDT sync (default, requires SDK)
- `automerge-backend`: Use Automerge+Iroh (pure Rust, Android-compatible)
- `lite-transport`: UDP protocol for embedded devices

---

### 3. Transport Layer

**Purpose**: Network connectivity across diverse physical layers

**Crates**:

| Crate | Purpose | Platforms |
|-------|---------|-----------|
| `peat-mesh` | Peer topology management, routing | All |
| `peat-discovery` | mDNS/static peer discovery | Desktop, Mobile |
| `peat-transport` | HTTP/REST API for external systems | Servers |
| `peat-lite` | UDP-based protocol for constrained devices | ESP32, no_std |
| `peat-btle` (external) | BLE mesh for mobile/embedded | Android, iOS, Windows, ESP32 |

**Transport Abstractions** (ADR-032):
```rust
pub trait Transport: Send + Sync {
    async fn send(&self, peer: PeerId, data: &[u8]) -> Result<()>;
    async fn recv(&self) -> Result<(PeerId, Vec<u8>)>;
    fn peers(&self) -> Vec<PeerId>;
}
```

**Dependencies**: `peat-protocol`, `peat-schema`

---

### 4. Binding Layer (`peat-ffi`)

**Purpose**: Cross-platform mobile and native bindings

**Responsibilities**:
- UniFFI-generated Kotlin bindings (Android)
- UniFFI-generated Swift bindings (iOS)
- JNI direct bindings for Android performance-critical paths

**Dependencies**: `peat-protocol` (without ditto-backend for cross-compilation)

---

### 5. Application Layer

**Purpose**: End-user applications and system integrations

**Crates**:

| Crate | Purpose |
|-------|---------|
| `peat-tak-bridge` | Bidirectional TAK Server <-> PEAT bridge |
| `peat-inference` | Edge ML inference (YOLOv8, object tracking) |
| `peat-sim` | Network simulation and validation |

**Dependencies**: `peat-protocol`, `peat-transport`, `peat-schema`

---

## Crate Dependency Graph

```
                              ┌──────────────┐
                              │ Applications │
                              │              │
              ┌───────────────┼──────────────┼───────────────┐
              │               │              │               │
              ▼               ▼              ▼               ▼
      ┌─────────────┐ ┌───────────┐ ┌─────────────┐
      │tak-bridge   │ │inference  │ │  peat-sim   │
      └─────────────┘ └───────────┘ └─────────────┘
              │              │               │
              └──────────────┼───────────────┘
                              │              │
                              ▼              ▼
                    ┌─────────────────────────────┐
                    │         peat-ffi            │
                    │  (UniFFI + JNI bindings)    │
                    └─────────────────────────────┘
                              │
                              ▼
      ┌───────────────────────┼───────────────────────┐
      │                       │                       │
      ▼                       ▼                       ▼
┌──────────────┐    ┌─────────────────┐    ┌─────────────────┐
│ peat-mesh    │    │ peat-discovery  │    │ peat-transport  │
│              │────│                 │    │                 │
└──────────────┘    └─────────────────┘    └─────────────────┘
      │                       │                       │
      └───────────────────────┼───────────────────────┘
                              │
                              ▼
                    ┌─────────────────────────────┐
                    │       peat-protocol         │
                    │  (Core sync + security)     │
                    └─────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────────────────┐
                    │        peat-schema          │
                    │    (Protobuf definitions)   │
                    └─────────────────────────────┘

                    ┌─────────────────────────────┐
                    │        peat-lite            │
                    │   (Standalone, no_std)      │
                    └─────────────────────────────┘
```

---

## Key Concepts

### Cells

A **cell** is a dynamic group of nodes that coordinate together. Cells are the fundamental unit of organization in PEAT:

- **Formation**: Nodes discover each other and negotiate cell membership
- **Leadership**: Cells elect leaders based on capabilities and authority
- **Hierarchy**: Cells can form parent-child relationships (team → group → formation)
- **Autonomy**: Cells operate independently when disconnected from higher echelons

### Capability Aggregation and Emergent Behavior

A core principle of PEAT is that **cells exhibit emergent capabilities** greater than the sum of their individual members:

```
┌─────────────────────────────────────────────────────────────┐
│                    CLUSTER COORDINATOR                       │
│   Sees: "Full-spectrum Sensing + Action + Signal package"   │
│   Can task based on COMBINED capabilities                    │
└─────────────────────────────────────────────────────────────┘
                              ▲
              Aggregated + Emergent capabilities
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
  ┌───────────┐         ┌───────────┐         ┌───────────┐
  │FORMATION 1│         │FORMATION 2│         │FORMATION 3│
  │Sense+Relay│         │Action+Sens│         │Action+Sig │
  │ Emergent: │         │ Emergent: │         │ Emergent: │
  │ Wide-area │         │ Sense-and-│         │ Coordinated│
  │ coverage  │         │ act loop  │         │ response  │
  └───────────┘         └───────────┘         └───────────┘
        ▲                     ▲                     ▲
   Group caps            Group caps            Group caps
```

**How it works**:
1. **Platforms** advertise individual capabilities (sensors, actuators, compute)
2. **Cells** aggregate member capabilities and detect **emergent patterns**
3. **Parents** receive aggregated summaries, enabling capability-based tasking

**Emergent capability examples**:
- **Sensing + Action** in same cell → **Sense-and-act loop**
- **Signal + Action** → **Coordinated response**
- **Multiple sensors** → **Wide-area observation**
- **Compute + sensors** → **Edge AI processing**

**Bidirectional flow**:
- **Upward**: Capabilities, tracks, status → aggregate at each level
- **Downward**: Commands, missions, constraints, AI models → disseminate to leaves
- **Horizontal**: Handoffs, deconfliction, mutual support between peers

### Documents and CRDTs

PEAT uses **Conflict-free Replicated Data Types (CRDTs)** for state synchronization:

- **Documents**: JSON-like structures that merge automatically
- **Collections**: Named groups of documents (e.g., "tracks", "missions")
- **Eventual Consistency**: All nodes converge to the same state
- **Offline-First**: Operations succeed locally, sync when connected

### Bypass Channel

For latency-critical data (sensor readings, control commands), PEAT provides a **UDP bypass channel**:

- Skips CRDT synchronization overhead
- Configurable per-collection
- Optional encryption and authentication
- Multicast support for broadcast scenarios

---

## Data Flow Example

### Track Update Flow

```
┌───────────────────────────────────────────────────────────────────────────┐
│                           TRACK UPDATE FLOW                                │
└───────────────────────────────────────────────────────────────────────────┘

  Sensor (ESP32)              Group Node                 Formation Node
       │                          │                           │
       │  ┌──────────────────┐    │                           │
       │  │ 1. Position data │    │                           │
       │  │    via peat-lite │    │                           │
       │  │    UDP protocol  │    │                           │
       │  └────────┬─────────┘    │                           │
       │           │              │                           │
       │           ▼              │                           │
       │     ┌──────────────┐     │                           │
       │     │ 2. Ingest to │     │                           │
       │     │ DocumentStore│     │                           │
       │     └──────┬───────┘     │                           │
       │            │             │                           │
       │            ▼             │                           │
       │     ┌──────────────┐     │     ┌──────────────┐      │
       │     │ 3. CRDT sync │────────── │ 4. Replicate │      │
       │     │ via Iroh/QUIC│     │     │  to parent   │      │
       │     └──────────────┘     │     └──────┬───────┘      │
       │                          │            │              │
       │                          │            ▼              │
       │                          │     ┌──────────────┐      │
       │                          │     │ 5. Aggregate │      │
       │                          │     │  & forward   │      │
       │                          │     └──────┬───────┘      │
       │                          │            │              │
       │                          │            ▼              │
       │                          │     ┌──────────────┐      │
       │                          │     │ 6. TAK Bridge│      │
       │                          │     │  → WebTAK    │      │
       │                          │     └──────────────┘      │
```

---

## Security Architecture

See [ADR-006](adr/006-security-authentication-authorization.md) and [ADR-044](adr/044-e2e-encryption-key-management.md) for complete details.

### Layers

1. **Device Identity**: Ed25519 keypairs, challenge-response authentication
2. **User Authentication**: RBAC, authority level integration
3. **Encryption**: ChaCha20-Poly1305 AEAD, X25519 key exchange
4. **Cell Key Management**: MLS-based group key agreement (planned)
5. **Hardware Root of Trust**: PUF/TPM integration (future)

---

## Protocol Specifications

For IETF RFC-style specifications, see the [Protocol Specification](spec/) directory:

| Document | Status | Description |
|----------|--------|-------------|
| [001-transport.md](spec/001-transport.md) | Draft | Wire formats, connection lifecycle |
| [002-sync.md](spec/002-sync.md) | Draft | CRDT semantics, conflict resolution |
| [003-schema.md](spec/003-schema.md) | Draft | Data type definitions, CoT mapping |
| [004-coordination.md](spec/004-coordination.md) | Draft | Cell formation, membership, hierarchy |
| [005-security.md](spec/005-security.md) | Draft | Auth, encryption, key management |

---

## Getting Started

### For Application Developers

```rust
use peat_protocol::prelude::*;

// Create a document store with Automerge backend
let store = DocumentStore::new(Config::default()).await?;

// Subscribe to track updates
let mut tracks = store.subscribe("tracks").await?;
while let Some(track) = tracks.next().await {
    println!("Track update: {:?}", track);
}
```

### For Protocol Contributors

See [CONTRIBUTING.md](../CONTRIBUTING.md) for development setup.

Key ADRs to read first:
1. [ADR-007: Automerge-based Sync Engine](adr/007-automerge-based-sync-engine-updated.md)
2. [ADR-011: Ditto vs Automerge/Iroh](adr/011-ditto-vs-automerge-iroh.md)
3. [ADR-006: Security Architecture](adr/006-security-authentication-authorization.md)

---

## References

- [Architecture Decision Records](adr/)
- [Protocol Specifications](spec/)
- [Interface Contracts](contracts/)
- [Whitepaper](whitepaper/)
