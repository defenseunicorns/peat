# Peat Developer Guide

> **Version**: 1.0
> **Last Updated**: 2025-12-08
> **Audience**: Software Engineers, Protocol Contributors, Integration Developers

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Getting Started](#2-getting-started)
3. [Architecture](#3-architecture)
4. [Core Concepts](#4-core-concepts)
5. [Crate Reference](#5-crate-reference)
6. [API Reference](#6-api-reference)
7. [Extending Peat](#7-extending-peat)
8. [Testing](#8-testing)
9. [Backend Abstraction](#9-backend-abstraction)
10. [Mobile Development](#10-mobile-development)
11. [Edge AI Integration](#11-edge-ai-integration)
12. [Contributing](#12-contributing)
13. [Reference](#13-reference)

---

## 1. Introduction

### 1.1 About This Guide

This guide is for software engineers who want to:
- **Build applications** using Peat as a coordination protocol
- **Contribute** to the Peat core protocol
- **Integrate** Peat with existing systems
- **Extend** Peat with custom capabilities and behaviors

### 1.2 Prerequisites

- **Rust proficiency**: Familiarity with Rust 2021 edition
- **Async programming**: Understanding of Tokio and async/await
- **Distributed systems**: Basic knowledge of CRDTs, eventual consistency
- **Development tools**: Git, cargo, IDE of choice

### 1.3 What is Peat?

Peat (Hierarchical Intelligence for Versatile Entities) is a protocol enabling scalable coordination of autonomous nodes through:

- **CRDT-based state synchronization** - Conflict-free data structures for eventual consistency
- **Three-phase protocol** - Discovery → Cell Formation → Hierarchical Operations
- **Capability composition** - Dynamic aggregation of node capabilities
- **O(n log n) message complexity** - Scales to 100+ nodes efficiently

---

## 2. Getting Started

### 2.1 Development Environment Setup

#### Required Tools

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Verify Rust version (1.70+ required)
rustc --version

# Clone the repository
git clone https://github.com/defenseunicorns/peat.git
cd peat
```

#### Recommended Tools

```bash
# Fast test runner (highly recommended)
cargo install cargo-nextest

# File watcher for auto-rebuild
cargo install cargo-watch

# Code coverage
cargo install cargo-tarpaulin

# Dependency analysis
cargo install cargo-udeps
```

#### IDE Setup

**VS Code** (recommended):
```json
// .vscode/settings.json
{
    "rust-analyzer.cargo.features": "all",
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.inlayHints.parameterHints.enable": true,
    "editor.formatOnSave": true
}
```

**Recommended Extensions**:
- rust-analyzer
- crates
- Even Better TOML
- Error Lens

### 2.2 First Build

```bash
# Build all crates (first build takes 5-10 minutes)
cargo build

# Build in release mode
cargo build --release

# Check without building (faster)
cargo check
```

### 2.3 Running Tests

```bash
# Quick unit tests only (30 seconds)
make test-fast

# All tests with nextest (faster parallel execution)
make test-unit

# Integration tests (2 minutes)
make test-integration

# End-to-end tests (5 minutes, requires real Ditto sync)
make test-e2e

# Full test suite (10 minutes)
make test
```

### 2.4 Running the Simulator

```bash
# Run with default configuration
cargo run --bin peat-sim

# Run with debug logging
RUST_LOG=debug cargo run --bin peat-sim

# Run with specific module tracing
RUST_LOG=peat_protocol::discovery=trace cargo run --bin peat-sim
```

### 2.5 Project Layout

```
peat/
├── Cargo.toml                 # Workspace configuration
├── Makefile                   # Development commands
├── DEVELOPMENT.md             # Development quickstart
├── Codex.md                  # AI assistant context
│
├── peat-protocol/             # Core protocol library
│   ├── src/
│   │   ├── lib.rs            # Crate root, public exports
│   │   ├── cell/             # Phase 2: Cell formation
│   │   ├── command/          # Bidirectional commands
│   │   ├── composition/      # Capability composition
│   │   ├── cot/              # CoT XML translation
│   │   ├── credentials/      # Credential management
│   │   ├── discovery/        # Phase 1: Discovery
│   │   ├── distribution/     # AI model distribution
│   │   ├── hierarchy/        # Phase 3: Hierarchical ops
│   │   ├── models/           # Core data structures
│   │   ├── network/          # Network constraints
│   │   ├── policy/           # Policy engine
│   │   ├── qos/              # Quality of service
│   │   ├── security/         # Auth, encryption
│   │   ├── storage/          # CRDT backends
│   │   ├── sync/             # Sync abstraction
│   │   ├── testing/          # Test harness
│   │   ├── traits/           # Core traits
│   │   └── transport/        # Mesh transport
│   ├── tests/                # Integration & E2E tests
│   └── examples/             # Usage examples
│
├── peat-schema/               # Protobuf definitions
│   ├── proto/                # .proto files
│   └── src/                  # Generated Rust code
│
├── peat-mesh/                 # Mesh topology management
├── peat-transport/            # HTTP/REST API layer
├── peat-discovery/            # Peer discovery strategies
├── peat-persistence/          # Storage backends
├── peat-ffi/                  # Mobile bindings (Kotlin/Swift)
├── peat-inference/            # Edge AI/ML pipeline
├── peat-sim/                  # Network simulator
├── docs/                      # Documentation
│   ├── adr/                  # Architecture Decision Records
│   ├── guides/               # User & developer guides
│   └── spec/                 # Protocol specification
│
└── spec/                      # Normative specification
    └── proto/                # Canonical protobuf schemas
```

---

## 3. Architecture

### 3.1 System Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Peat Architecture                               │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                        Application Layer                             │    │
│  │   peat-sim    peat-transport    peat-inference                      │    │
│  └────────────────────────────┬────────────────────────────────────────┘    │
│                               │                                              │
│  ┌────────────────────────────▼────────────────────────────────────────┐    │
│  │                        Protocol Layer                                │    │
│  │                         peat-protocol                                │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐    │    │
│  │  │Discovery │  │   Cell   │  │Hierarchy │  │   Composition    │    │    │
│  │  │ Phase 1  │──│ Phase 2  │──│ Phase 3  │  │     Engine       │    │    │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘    │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐    │    │
│  │  │ Security │  │  Policy  │  │   QoS    │  │     Command      │    │    │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘    │    │
│  └────────────────────────────┬────────────────────────────────────────┘    │
│                               │                                              │
│  ┌────────────────────────────▼────────────────────────────────────────┐    │
│  │                       Storage Abstraction                            │    │
│  │  ┌─────────────────────┐      ┌─────────────────────────────┐       │    │
│  │  │    Ditto Backend    │      │    Automerge/Iroh Backend   │       │    │
│  │  │    (Production)     │      │       (Pure OSS)            │       │    │
│  │  └─────────────────────┘      └─────────────────────────────┘       │    │
│  └────────────────────────────┬────────────────────────────────────────┘    │
│                               │                                              │
│  ┌────────────────────────────▼────────────────────────────────────────┐    │
│  │                        Network Layer                                 │    │
│  │            P2P Mesh (Ditto SDK / Iroh QUIC)                         │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                        Data Flow                                 │
│                                                                  │
│  Local Change                                                    │
│       │                                                          │
│       ▼                                                          │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐        │
│  │   Node      │     │   Delta     │     │   Priority  │        │
│  │   State     │ ──► │  Generator  │ ──► │  Assigner   │        │
│  │   (CRDT)    │     │             │     │   (QoS)     │        │
│  └─────────────┘     └─────────────┘     └──────┬──────┘        │
│                                                  │               │
│                                                  ▼               │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐        │
│  │   Peer      │     │ Hierarchical│     │   Sync      │        │
│  │   State     │ ◄── │   Router    │ ◄── │   Engine    │        │
│  │   Merged    │     │             │     │   (CRDT)    │        │
│  └─────────────┘     └─────────────┘     └─────────────┘        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 3.3 Crate Dependency Graph

```
peat-sim ────────────────────────────────────┐
    │                                        │
    ▼                                        │
peat-mesh ◄──────────────────────────────────┤
    │                                        │
    ▼                                        │
peat-protocol (core) ◄───────────────────────┤
    │                                        │
    ├── peat-schema (protobuf)               │
    │                                        │
    ├──► Ditto Backend (default)             │
    │    OR                                  │
    └──► Automerge + Iroh (feature flag)     │
                                             │
peat-transport ──────────────────────────────┤
    │                                        │
    └──► peat-protocol                       │
                                             │
peat-persistence ────────────────────────────┤
    │                                        │
    └──► peat-protocol                       │
                                             │
peat-ffi (mobile) ───────────────────────────┤
    │                                        │
    └──► peat-protocol (automerge only)      │
                                             │
peat-inference ──────────────────────────────┘
    │
    └──► peat-protocol (automerge)
```

### 3.4 CRDT Usage

Peat uses specific CRDT types for different data:

| Data Type | CRDT | Rationale |
|-----------|------|-----------|
| Node capabilities | G-Set | Capabilities only grow, never removed |
| Cell membership | OR-Set | Members can join and leave |
| Leader identity | LWW-Register | Latest leader wins |
| Node position | LWW-Register | Latest position is truth |
| Fuel/resources | PN-Counter | Can increase and decrease |
| Message history | G-Set | Messages append-only |
| Configuration | LWW-Map | Latest config wins per key |

---

## 4. Core Concepts

### 4.1 Nodes

A **Node** represents a single entity in the Peat network:

```rust
use peat_protocol::models::{Node, NodeId, PlatformType};
use peat_protocol::models::capability::Capability;

// Create a node
let node = Node::new(
    NodeId::new(),
    PlatformType::UAV,
    vec![
        Capability::sensor(SensorType::EoIr, 10_000.0),
        Capability::compute(ComputeType::EdgeMl, 5.2),
    ],
);

// Node has unique ID, platform type, and capabilities
println!("Node ID: {}", node.id);
println!("Platform: {:?}", node.platform_type);
println!("Capabilities: {:?}", node.capabilities);
```

### 4.2 Capabilities

**Capabilities** describe what a node can do:

```rust
use peat_protocol::models::capability::{Capability, CapabilityType};

// Sensor capability
let eo_ir = Capability::new(CapabilityType::Sensor)
    .with_sensor_type(SensorType::EoIr)
    .with_range_meters(10_000.0)
    .with_resolution("4K");

// Compute capability
let compute = Capability::new(CapabilityType::Compute)
    .with_compute_type(ComputeType::EdgeMl)
    .with_tflops(5.2)
    .with_models(vec!["yolov8", "detection"]);

// Communication capability
let comms = Capability::new(CapabilityType::Communication)
    .with_radio_type(RadioType::TacticalRadio)
    .with_bandwidth_kbps(256);
```

#### Capability Types

| Type | Description | Composition |
|------|-------------|-------------|
| `Sensor` | Detection, imaging, tracking | Additive (ranges combine) |
| `Compute` | Processing, ML inference | Additive (compute sums) |
| `Communication` | Radio, network relay | Aggregated (best wins) |
| `Weapon` | Kinetic effects | Not composed |
| `Payload` | Cargo, delivery | Additive |
| `Mobility` | Movement, speed | Constraint (slowest) |

### 4.3 Cells

A **Cell** is a group of nodes that coordinate together:

```rust
use peat_protocol::cell::{Cell, CellId, CellConfig};

// Cell configuration
let config = CellConfig::new()
    .with_target_size(5)
    .with_leader_election_timeout(Duration::from_secs(10))
    .with_heartbeat_interval(Duration::from_secs(5));

// Cells form during Phase 2
// Leader election is deterministic based on node capabilities
let cell = Cell::new(CellId::new(), config);

// Add members
cell.add_member(node1.id)?;
cell.add_member(node2.id)?;

// Leader is elected automatically
let leader = cell.leader();
```

#### Cell Roles

| Role | Description | Count per Cell |
|------|-------------|----------------|
| `Leader` | Coordinates cell, aggregates to zone | 1 |
| `Sensor` | Primary sensing capability | 0-N |
| `Compute` | Primary compute capability | 0-N |
| `Relay` | Network relay/gateway | 0-N |
| `Strike` | Kinetic capability | 0-N |
| `Support` | Logistics, support | 0-N |
| `Follower` | General member | 0-N |

### 4.4 Zones

A **Zone** aggregates multiple cells for hierarchical coordination:

```rust
use peat_protocol::hierarchy::{Zone, ZoneId, ZoneConfig};

// Zone configuration
let config = ZoneConfig::new()
    .with_target_cells(5)
    .with_aggregation_interval(Duration::from_secs(10));

// Zones form during Phase 3
let zone = Zone::new(ZoneId::new(), config);

// Zone coordinator elected from cell leaders
let coordinator = zone.coordinator();

// Aggregated capabilities from all cells
let zone_capabilities = zone.aggregated_capabilities();
```

### 4.5 Three-Phase Protocol

#### Phase 1: Discovery

Nodes discover each other and form initial groups:

```rust
use peat_protocol::discovery::{DiscoveryConfig, DiscoveryStrategy};

let config = DiscoveryConfig::new()
    .with_strategy(DiscoveryStrategy::Hybrid)
    .with_timeout(Duration::from_secs(30))
    .with_geohash_precision(6);

// Discovery finds peers within geohash region
let peers = discovery.discover_peers(&config).await?;
```

**Discovery Strategies**:
- `mDNS`: Multicast DNS for local network
- `Static`: Pre-configured peer list
- `Hybrid`: mDNS + static fallback
- `Geographic`: Geohash-based clustering

#### Phase 2: Cell Formation

Discovered nodes form cells with elected leaders:

```rust
use peat_protocol::cell::formation::{FormationConfig, FormationStrategy};

let config = FormationConfig::new()
    .with_target_cell_size(5)
    .with_formation_strategy(FormationStrategy::CapabilityBased);

// Formation groups nodes by capability diversity
let cell = formation.form_cell(&config, &peers).await?;

// Leader election uses deterministic scoring
// Score = capability_score * uptime_weight * stability_bonus
let leader = cell.elect_leader()?;
```

#### Phase 3: Hierarchical Operations

Cells organize into zones for multi-level coordination:

```rust
use peat_protocol::hierarchy::{HierarchyConfig, AggregationPolicy};

let config = HierarchyConfig::new()
    .with_zone_size(25)
    .with_aggregation_policy(AggregationPolicy::Differential);

// Cells aggregate into zones
let zone = hierarchy.form_zone(&config, &cells).await?;

// Differential updates propagate efficiently
hierarchy.propagate_update(&update).await?;
```

### 4.6 Capability Composition

Peat composes capabilities from multiple nodes:

```rust
use peat_protocol::composition::{Composer, CompositionRules};

let composer = Composer::with_rules(CompositionRules::default());

// Compose cell capabilities from members
let cell_capabilities = composer.compose(&cell.members())?;

// Composition follows rules:
// - Additive: sum ranges, compute
// - Emergent: detect new capabilities from combinations
// - Redundant: count duplicates for reliability
// - Constraint: apply limits (slowest speed wins)
```

#### Composition Patterns

| Pattern | Example | Rule |
|---------|---------|------|
| **Additive** | Sensor ranges combine | `a + b` |
| **Emergent** | Compute + Sensor = ML | `f(a, b) = c` |
| **Redundant** | Multiple sensors | `count(type)` |
| **Constraint** | Movement speed | `min(a, b)` |

---

## 5. Crate Reference

### 5.1 peat-protocol

The core protocol implementation.

```toml
# Cargo.toml
[dependencies]
peat-protocol = { path = "../peat-protocol" }
```

**Key Modules**:

| Module | Purpose |
|--------|---------|
| `cell` | Cell formation, leader election |
| `command` | Bidirectional command flow |
| `composition` | Capability composition engine |
| `discovery` | Peer discovery strategies |
| `hierarchy` | Zone coordination, aggregation |
| `models` | Core data structures |
| `policy` | Policy engine, conflict resolution |
| `qos` | Quality of service framework |
| `security` | Authentication, encryption |
| `storage` | CRDT backend abstraction |
| `sync` | Synchronization primitives |

### 5.2 peat-schema

Protocol buffer definitions and generated code.

```toml
[dependencies]
peat-schema = { path = "../peat-schema" }
```

**Protobuf Messages**:

| Proto File | Messages |
|------------|----------|
| `node.proto` | Node, NodeState, Position |
| `capability.proto` | Capability, CapabilityType |
| `cell.proto` | Cell, CellState, CellRole |
| `zone.proto` | Zone, ZoneState |
| `message.proto` | PeatMessage, Priority |
| `command.proto` | Command, CommandResponse |
| `sync.proto` | SyncRequest, SyncResponse |
| `discovery.proto` | DiscoveryRequest, Peer |

### 5.3 peat-mesh

Mesh topology and beacon management.

```toml
[dependencies]
peat-mesh = { path = "../peat-mesh" }
```

**Key Types**:
- `MeshTopology`: Peer connection graph
- `Beacon`: Node advertisement
- `MeshRouter`: Message routing

### 5.4 peat-transport

HTTP/REST API layer.

```toml
[dependencies]
peat-transport = { path = "../peat-transport" }
```

**Endpoints**:
- `GET /api/v1/status` - Node status
- `GET /api/v1/peers` - Connected peers
- `GET /api/v1/cell` - Cell information
- `POST /api/v1/command` - Send command

### 5.5 peat-discovery

Peer discovery implementations.

```toml
[dependencies]
peat-discovery = { path = "../peat-discovery" }
```

**Strategies**:
- `MdnsDiscovery`: mDNS-based discovery
- `StaticDiscovery`: Pre-configured peers
- `HybridDiscovery`: Combined strategy

### 5.6 peat-persistence

Storage backend abstraction.

```toml
[dependencies]
peat-persistence = { path = "../peat-persistence" }
```

**Backends**:
- `RedbBackend`: Embedded key-value store
- `SqliteBackend`: SQLite database

### 5.7 peat-ffi

Foreign function interface for mobile.

```toml
[dependencies]
peat-ffi = { path = "../peat-ffi", features = ["automerge-backend"] }
```

**Bindings**:
- UniFFI for Kotlin/Swift
- JNI for direct Android

### 5.8 peat-inference

Edge AI/ML inference pipeline.

```toml
[dependencies]
peat-inference = { path = "../peat-inference", features = ["onnx-inference"] }
```

**Features**:
- ONNX runtime integration
- Object detection pipeline
- Model distribution
- Video capture (GStreamer)

---

## 6. API Reference

### 6.1 Node API

```rust
use peat_protocol::models::{Node, NodeId, NodeState};

impl Node {
    /// Create a new node with capabilities
    pub fn new(id: NodeId, platform: PlatformType, capabilities: Vec<Capability>) -> Self;

    /// Get node identifier
    pub fn id(&self) -> &NodeId;

    /// Get current state
    pub fn state(&self) -> &NodeState;

    /// Update position
    pub fn update_position(&mut self, position: Position);

    /// Add capability
    pub fn add_capability(&mut self, capability: Capability);

    /// Get all capabilities
    pub fn capabilities(&self) -> &[Capability];
}
```

### 6.2 Cell API

```rust
use peat_protocol::cell::{Cell, CellId, CellConfig};

impl Cell {
    /// Create a new cell
    pub fn new(id: CellId, config: CellConfig) -> Self;

    /// Add a member to the cell
    pub fn add_member(&mut self, node_id: NodeId) -> Result<(), CellError>;

    /// Remove a member from the cell
    pub fn remove_member(&mut self, node_id: &NodeId) -> Result<(), CellError>;

    /// Get current leader
    pub fn leader(&self) -> Option<&NodeId>;

    /// Trigger leader election
    pub fn elect_leader(&mut self) -> Result<NodeId, CellError>;

    /// Get all members
    pub fn members(&self) -> &[NodeId];

    /// Get aggregated capabilities
    pub fn capabilities(&self) -> Vec<Capability>;
}
```

### 6.3 Discovery API

```rust
use peat_protocol::discovery::{Discovery, DiscoveryConfig, Peer};

impl Discovery {
    /// Create discovery service
    pub fn new(config: DiscoveryConfig) -> Self;

    /// Start discovery process
    pub async fn start(&self) -> Result<(), DiscoveryError>;

    /// Discover peers
    pub async fn discover_peers(&self) -> Result<Vec<Peer>, DiscoveryError>;

    /// Subscribe to peer events
    pub fn subscribe(&self) -> broadcast::Receiver<PeerEvent>;
}
```

### 6.4 Sync API

```rust
use peat_protocol::sync::{SyncEngine, SyncConfig};

impl SyncEngine {
    /// Create sync engine with backend
    pub fn new(backend: impl SyncBackend, config: SyncConfig) -> Self;

    /// Start synchronization
    pub async fn start(&self) -> Result<(), SyncError>;

    /// Apply local change
    pub async fn apply_change(&self, change: Change) -> Result<(), SyncError>;

    /// Subscribe to remote changes
    pub fn subscribe(&self) -> broadcast::Receiver<Change>;

    /// Get current state
    pub fn state(&self) -> State;
}
```

### 6.5 Storage API

```rust
use peat_protocol::storage::{Storage, StorageBackend};

/// Backend-agnostic storage trait
pub trait StorageBackend: Send + Sync {
    /// Get value by key
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;

    /// Set value
    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError>;

    /// Delete value
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// List keys with prefix
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError>;

    /// Subscribe to changes
    fn subscribe(&self) -> broadcast::Receiver<StorageEvent>;
}
```

### 6.6 Error Handling

Peat uses typed errors for each domain:

```rust
use peat_protocol::error::{PeatError, CellError, DiscoveryError, SyncError};

// Errors are enums with variants
pub enum CellError {
    MemberNotFound(NodeId),
    CellFull { max_size: usize },
    LeaderElectionFailed { reason: String },
    InvalidConfiguration(String),
}

// Use Result throughout
fn add_to_cell(cell: &mut Cell, node: NodeId) -> Result<(), CellError> {
    if cell.members().len() >= cell.max_size() {
        return Err(CellError::CellFull { max_size: cell.max_size() });
    }
    cell.add_member(node)?;
    Ok(())
}

// Propagate with ?
async fn join_network(config: &Config) -> Result<(), PeatError> {
    let peers = discovery.discover_peers().await?;
    let cell = formation.join_cell(&peers).await?;
    Ok(())
}
```

---

## 7. Extending Peat

### 7.1 Adding Custom Capabilities

Create a new capability type:

```rust
// src/models/capability.rs extension

/// Add new capability type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CapabilityType {
    // Existing types...
    Sensor,
    Compute,
    Communication,

    // Your new type
    CustomLidar {
        range_meters: f64,
        points_per_second: u64,
        fov_degrees: f64,
    },
}

// Add composition rule
impl CompositionRule for CustomLidar {
    fn compose(&self, other: &Self) -> Option<Self> {
        // Custom composition logic
        Some(CustomLidar {
            range_meters: self.range_meters.max(other.range_meters),
            points_per_second: self.points_per_second + other.points_per_second,
            fov_degrees: self.fov_degrees.max(other.fov_degrees),
        })
    }
}
```

### 7.2 Custom Discovery Strategy

Implement a custom discovery strategy:

```rust
use peat_protocol::discovery::{DiscoveryStrategy, Peer, DiscoveryError};
use async_trait::async_trait;

pub struct CustomDiscovery {
    config: CustomDiscoveryConfig,
}

#[async_trait]
impl DiscoveryStrategy for CustomDiscovery {
    async fn discover(&self) -> Result<Vec<Peer>, DiscoveryError> {
        // Your discovery logic
        let peers = self.query_custom_service().await?;
        Ok(peers)
    }

    async fn announce(&self, node: &Node) -> Result<(), DiscoveryError> {
        // Announce this node
        self.register_with_service(node).await?;
        Ok(())
    }

    fn strategy_name(&self) -> &str {
        "custom"
    }
}

// Register with discovery coordinator
let discovery = Discovery::new(config)
    .with_strategy(Box::new(CustomDiscovery::new(custom_config)));
```

### 7.3 Custom Composition Rules

Add custom composition rules:

```rust
use peat_protocol::composition::{CompositionRule, CompositionResult};

/// Custom rule for detecting emergent capabilities
pub struct DetectionEmergence;

impl CompositionRule for DetectionEmergence {
    fn name(&self) -> &str {
        "detection_emergence"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        // Check if we have both sensor and compute
        let has_sensor = capabilities.iter().any(|c| c.is_sensor());
        let has_compute = capabilities.iter().any(|c| c.is_compute());
        has_sensor && has_compute
    }

    fn compose(&self, capabilities: &[Capability]) -> CompositionResult {
        // Combine sensor + compute = detection capability
        let sensor = capabilities.iter().find(|c| c.is_sensor())?;
        let compute = capabilities.iter().find(|c| c.is_compute())?;

        CompositionResult::Emergent(Capability::detection(
            sensor.range(),
            compute.tflops(),
        ))
    }
}

// Register the rule
composer.add_rule(Box::new(DetectionEmergence));
```

### 7.4 Custom Policy Rules

Implement policy rules for conflict resolution:

```rust
use peat_protocol::policy::{PolicyRule, PolicyContext, PolicyDecision};

pub struct CustomAuthorizationPolicy;

impl PolicyRule for CustomAuthorizationPolicy {
    fn name(&self) -> &str {
        "custom_authorization"
    }

    fn evaluate(&self, context: &PolicyContext) -> PolicyDecision {
        // Check authorization
        if context.requester_authority() < context.required_authority() {
            return PolicyDecision::Deny {
                reason: "Insufficient authority".to_string(),
            };
        }

        // Check time-based restrictions
        if !context.within_operation_window() {
            return PolicyDecision::Deny {
                reason: "Outside operation window".to_string(),
            };
        }

        PolicyDecision::Allow
    }

    fn priority(&self) -> u32 {
        100 // Higher priority evaluated first
    }
}

// Register policy
policy_engine.add_rule(Box::new(CustomAuthorizationPolicy));
```

### 7.5 Custom QoS Configuration

Configure quality of service:

```rust
use peat_protocol::qos::{QosConfig, Priority, QosRule};

let qos_config = QosConfig::new()
    // Position updates are high priority
    .with_rule(QosRule::new()
        .message_type("position_update")
        .priority(Priority::P1)
        .max_latency(Duration::from_millis(100)))

    // Telemetry is lower priority
    .with_rule(QosRule::new()
        .message_type("telemetry")
        .priority(Priority::P3)
        .max_latency(Duration::from_secs(5)))

    // Custom message type
    .with_rule(QosRule::new()
        .message_type("custom_alert")
        .priority(Priority::P0)
        .max_latency(Duration::from_millis(50)));
```

---

## 8. Testing

### 8.1 Test Philosophy

Peat follows a test pyramid approach:

```
         /\
        /E2E\         10% effort, 100% mission assurance
       /------\
      /Integra-\      20% effort, component validation
     /  tion    \
    /------------\
   /    Unit      \   70% effort, business logic
  /----------------\
```

**Key Principle**: E2E tests validate real CRDT sync behavior - this is critical for autonomous systems.

### 8.2 Unit Tests

Unit tests are inline with code:

```rust
// src/composition/rules.rs

pub fn compose_additive(a: f64, b: f64) -> f64 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose_additive() {
        assert_eq!(compose_additive(1.0, 2.0), 3.0);
        assert_eq!(compose_additive(0.0, 5.0), 5.0);
    }

    #[test]
    fn test_compose_additive_negative() {
        assert_eq!(compose_additive(-1.0, 1.0), 0.0);
    }
}
```

Run unit tests:
```bash
make test-fast
# or
cargo test --lib
```

### 8.3 Integration Tests

Integration tests are in `tests/` directory:

```rust
// tests/cell_integration.rs

use peat_protocol::cell::{Cell, CellConfig};
use peat_protocol::models::Node;

#[tokio::test]
async fn test_cell_formation_integration() {
    // Create multiple nodes
    let nodes = create_test_nodes(5);

    // Form a cell
    let config = CellConfig::default();
    let mut cell = Cell::new(CellId::new(), config);

    for node in &nodes {
        cell.add_member(node.id.clone()).unwrap();
    }

    // Verify cell state
    assert_eq!(cell.members().len(), 5);
    assert!(cell.leader().is_some());
}

#[tokio::test]
async fn test_leader_election_integration() {
    let mut cell = create_test_cell(5);

    // Trigger leader election
    let leader = cell.elect_leader().unwrap();

    // Leader should be the highest-scoring node
    assert!(cell.members().contains(&leader));
}
```

Run integration tests:
```bash
make test-integration
# or
cargo test --test '*_integration'
```

### 8.4 E2E Tests

E2E tests validate real CRDT synchronization:

```rust
// tests/sync_e2e.rs

use peat_protocol::testing::{E2eHarness, TestObserver};

#[tokio::test]
async fn test_state_sync_e2e() {
    // Create E2E harness with real Ditto
    let harness = E2eHarness::new()
        .with_node_count(3)
        .with_observer(TestObserver::new())
        .build()
        .await;

    // Start all nodes
    harness.start_all().await;

    // Make change on node 0
    let node0 = harness.node(0);
    node0.update_position(Position::new(38.8977, -77.0365)).await;

    // Wait for sync (with timeout)
    harness.wait_for_sync(Duration::from_secs(5)).await?;

    // Verify all nodes have the update
    for i in 0..3 {
        let node = harness.node(i);
        let position = node.position();
        assert_eq!(position.lat, 38.8977);
        assert_eq!(position.lon, -77.0365);
    }
}

#[tokio::test]
async fn test_partition_recovery_e2e() {
    let harness = E2eHarness::new()
        .with_node_count(4)
        .build()
        .await;

    harness.start_all().await;

    // Create partition between nodes 0,1 and 2,3
    harness.partition(vec![0, 1], vec![2, 3]).await;

    // Make changes on both sides
    harness.node(0).update_value("key", "value_a").await;
    harness.node(2).update_value("key", "value_b").await;

    // Heal partition
    harness.heal_partition().await;

    // Wait for reconciliation
    harness.wait_for_sync(Duration::from_secs(10)).await?;

    // CRDT ensures consistent state (LWW - last write wins)
    let final_value = harness.node(0).get_value("key");
    for i in 0..4 {
        assert_eq!(harness.node(i).get_value("key"), final_value);
    }
}
```

Run E2E tests:
```bash
make test-e2e
# or
cargo test --test '*_e2e' -- --test-threads=1
```

### 8.5 Test Fixtures

Use fixtures for consistent test data:

```rust
// src/testing/fixtures.rs

pub fn create_test_node() -> Node {
    Node::new(
        NodeId::new(),
        PlatformType::UAV,
        vec![
            Capability::sensor(SensorType::EoIr, 10_000.0),
        ],
    )
}

pub fn create_test_nodes(count: usize) -> Vec<Node> {
    (0..count).map(|_| create_test_node()).collect()
}

pub fn create_test_cell(size: usize) -> Cell {
    let mut cell = Cell::new(CellId::new(), CellConfig::default());
    for node in create_test_nodes(size) {
        cell.add_member(node.id).unwrap();
    }
    cell
}
```

### 8.6 Mocking

Use mock implementations for unit testing:

```rust
use mockall::automock;

#[automock]
pub trait StorageBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError>;
}

#[tokio::test]
async fn test_with_mock_storage() {
    let mut mock = MockStorageBackend::new();

    mock.expect_get()
        .with(eq("test_key"))
        .returning(|_| Ok(Some(b"test_value".to_vec())));

    let service = Service::new(mock);
    let result = service.read("test_key").await;

    assert_eq!(result, "test_value");
}
```

### 8.7 Test Commands

```bash
# Quick development cycle
make test-fast              # Unit tests only (~30s)

# Pre-commit (recommended)
make pre-commit             # Format + lint + all tests

# Full test suite
make test                   # Unit + integration + E2E (~10 min)

# Specific test patterns
cargo test discovery        # Tests matching "discovery"
cargo test --test sync_e2e  # Specific E2E test file

# With output
cargo test -- --nocapture   # Show println! output

# Coverage
cargo tarpaulin --out Html
```

---

## 9. Backend Abstraction

### 9.1 Backend Architecture

Peat abstracts the CRDT backend to support multiple implementations:

```rust
/// Backend-agnostic sync trait
pub trait SyncBackend: Send + Sync {
    /// Initialize the backend
    async fn initialize(&self) -> Result<(), BackendError>;

    /// Apply a local change
    async fn apply_local(&self, change: Change) -> Result<(), BackendError>;

    /// Subscribe to remote changes
    fn subscribe(&self) -> broadcast::Receiver<Change>;

    /// Get current state
    async fn state(&self) -> State;

    /// Sync with peers
    async fn sync(&self) -> Result<(), BackendError>;
}
```

### 9.2 Ditto Backend (Default)

Production-ready CRDT backend using Ditto SDK:

```rust
use peat_protocol::storage::ditto::DittoBackend;

// Configure Ditto
let ditto_config = DittoConfig::new()
    .with_app_id(env::var("DITTO_APP_ID")?)
    .with_offline_token(env::var("DITTO_OFFLINE_TOKEN")?)
    .with_persistence_dir("/var/lib/peat/ditto");

let backend = DittoBackend::new(ditto_config).await?;
let sync_engine = SyncEngine::new(backend);
```

**Advantages**:
- Production-tested at scale
- Built-in P2P mesh
- Automatic conflict resolution
- Mobile SDK support

**Requirements**:
- Ditto license (free for evaluation)
- `DITTO_APP_ID` and `DITTO_OFFLINE_TOKEN`

### 9.3 Automerge Backend (Pure OSS)

Pure Rust implementation using Automerge + Iroh:

```rust
use peat_protocol::storage::automerge::AutomergeBackend;

// Configure Automerge + Iroh
let automerge_config = AutomergeConfig::new()
    .with_persistence_dir("/var/lib/peat/automerge");

let backend = AutomergeBackend::new(automerge_config).await?;
let sync_engine = SyncEngine::new(backend);
```

**Advantages**:
- 100% open source (MIT/Apache-2.0)
- No external dependencies
- Iroh QUIC transport
- Works on all platforms including Android

**Current Status**: ~70% feature parity with Ditto

### 9.4 Switching Backends

Build with specific backend:

```bash
# Ditto backend (default)
cargo build --release

# Automerge backend
cargo build --release --no-default-features --features automerge-backend

# Both backends (for testing)
cargo build --release --features automerge-backend
```

Runtime selection (if both compiled):

```rust
use peat_protocol::storage::{DittoBackend, AutomergeBackend, BackendSelector};

let backend: Box<dyn SyncBackend> = match config.backend {
    BackendType::Ditto => Box::new(DittoBackend::new(ditto_config).await?),
    BackendType::Automerge => Box::new(AutomergeBackend::new(am_config).await?),
};
```

---

## 10. Mobile Development

### 10.1 Architecture

Mobile support via peat-ffi using UniFFI:

```
┌─────────────────┐     ┌─────────────────┐
│  Kotlin/Swift   │     │     Android     │
│   Application   │     │      ATAK       │
└────────┬────────┘     └────────┬────────┘
         │                       │
         ▼                       ▼
┌─────────────────────────────────────────┐
│              peat-ffi                   │
│         (UniFFI bindings)               │
└─────────────────┬───────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────┐
│           peat-protocol                  │
│      (Automerge backend only)           │
└─────────────────────────────────────────┘
```

### 10.2 Building for Android

```bash
# Install Android NDK
# Set ANDROID_NDK_HOME environment variable

# Add Android targets
rustup target add aarch64-linux-android
rustup target add armv7-linux-androideabi
rustup target add x86_64-linux-android

# Build FFI library
cd peat-ffi
cargo build --release --target aarch64-linux-android

# Generate Kotlin bindings
cargo run --bin uniffi-bindgen generate \
    --library target/aarch64-linux-android/release/libpeat_ffi.so \
    --language kotlin \
    --out-dir kotlin-bindings
```

### 10.3 Kotlin Usage

```kotlin
import com.peat.protocol.*

class PeatService {
    private val peat: PeatClient

    init {
        // Initialize Peat client
        val config = PeatConfig(
            nodeId = UUID.randomUUID().toString(),
            persistenceDir = context.filesDir.absolutePath
        )
        peat = PeatClient(config)
    }

    suspend fun start() {
        peat.start()
    }

    suspend fun updatePosition(lat: Double, lon: Double) {
        peat.updatePosition(Position(lat, lon, 0.0))
    }

    fun observePeers(): Flow<List<Peer>> {
        return peat.peers().asFlow()
    }
}
```

### 10.4 ATAK Plugin Development

The ATAK plugin provides TAK integration:

```bash
# Build ATAK plugin
cd atak-plugin
./gradlew assembleRelease

# Install on device
adb install -r app/build/outputs/apk/release/peat-atak-plugin.apk
```

Plugin architecture:
- `PeatDropDownReceiver`: UI integration
- `PeatCotTranslator`: CoT message translation
- `PeatPeerManager`: Peer discovery and management

---

## 11. Edge AI Integration

### 11.1 peat-inference Overview

The `peat-inference` crate provides edge AI/ML capabilities:

```rust
use peat_inference::{InferencePipeline, DetectionModel, VideoSource};

// Create inference pipeline
let pipeline = InferencePipeline::new()
    .with_model(DetectionModel::YoloV8("models/yolov8n.onnx"))
    .with_source(VideoSource::Gstreamer("v4l2src device=/dev/video0"))
    .with_tracker(ObjectTracker::ByteTrack)
    .build()?;

// Run inference
pipeline.start().await?;

// Subscribe to detections
let mut detections = pipeline.subscribe();
while let Some(detection) = detections.recv().await {
    println!("Detected: {:?} at {:?}", detection.class, detection.bbox);
}
```

### 11.2 Model Distribution

Peat distributes AI models across the network:

```rust
use peat_protocol::distribution::{ModelDistributor, ModelManifest};

// Create model manifest
let manifest = ModelManifest::new()
    .with_model_id("yolov8n-v1.0")
    .with_hash("sha256:abc123...")
    .with_size_bytes(6_000_000)
    .with_compatible_platforms(vec![PlatformType::Jetson, PlatformType::EdgeML]);

// Distribute to cell
let distributor = ModelDistributor::new(cell.clone());
distributor.distribute(&manifest, &model_bytes).await?;

// Verify distribution
let status = distributor.distribution_status(&manifest.model_id).await?;
println!("Distributed to {}/{} nodes", status.completed, status.total);
```

### 11.3 Runtime Adapters

Support multiple inference runtimes:

```rust
use peat_inference::runtime::{RuntimeAdapter, OnnxRuntime, TensorRtRuntime};

// ONNX Runtime (cross-platform)
let onnx = OnnxRuntime::new(OnnxConfig::default())?;

// TensorRT (NVIDIA GPU acceleration)
let tensorrt = TensorRtRuntime::new(TensorRtConfig {
    fp16: true,
    max_batch_size: 8,
})?;

// Select based on platform
let runtime: Box<dyn RuntimeAdapter> = if has_nvidia_gpu() {
    Box::new(tensorrt)
} else {
    Box::new(onnx)
};
```

---

## 12. Contributing

### 12.1 Code Style

Follow Rust conventions and project style:

```rust
// Use rustfmt defaults
cargo fmt

// Fix clippy warnings
cargo clippy --all-targets --all-features -- -D warnings

// Naming conventions
pub struct NodeState { }        // PascalCase for types
pub fn discover_peers() { }     // snake_case for functions
const MAX_CELL_SIZE: usize = 10; // SCREAMING_SNAKE for constants
```

### 12.2 Commit Messages

Follow Conventional Commits:

```
feat(cell): Add dynamic cell resizing

Add ability for cells to grow/shrink based on operational needs.
Includes new configuration option `dynamic_sizing` and tests.

Closes #123
```

**Prefixes**:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `test`: Tests
- `refactor`: Code refactoring
- `perf`: Performance improvement
- `chore`: Maintenance

### 12.3 Pull Request Process

1. **Fork and branch**:
   ```bash
   git checkout -b feat/your-feature
   ```

2. **Make changes** with tests

3. **Run pre-commit checks**:
   ```bash
   make pre-commit
   ```

4. **Push and create PR**:
   ```bash
   git push -u origin feat/your-feature
   ```

5. **PR template** includes:
   - Description of changes
   - Related issues
   - Test plan
   - Documentation updates

### 12.4 Review Guidelines

PRs require:
- All CI checks passing
- At least one approval
- No unresolved comments
- Documentation for public APIs
- Tests for new functionality

### 12.5 Documentation Requirements

All public APIs must have doc comments:

```rust
/// Discovers peers in the local network.
///
/// Uses the configured discovery strategy to find other Peat nodes.
/// Returns a list of discovered peers within the timeout period.
///
/// # Arguments
///
/// * `timeout` - Maximum time to wait for discovery
///
/// # Returns
///
/// A vector of discovered peers, or an error if discovery fails.
///
/// # Examples
///
/// ```rust
/// let discovery = Discovery::new(config);
/// let peers = discovery.discover(Duration::from_secs(30)).await?;
/// println!("Found {} peers", peers.len());
/// ```
///
/// # Errors
///
/// Returns `DiscoveryError::Timeout` if no peers found within timeout.
/// Returns `DiscoveryError::NetworkError` if network is unavailable.
pub async fn discover(&self, timeout: Duration) -> Result<Vec<Peer>, DiscoveryError> {
    // implementation
}
```

---

## 13. Reference

### 13.1 Glossary

| Term | Definition |
|------|------------|
| **Cell** | Group of nodes coordinating together |
| **Zone** | Group of cells in hierarchical organization |
| **Capability** | What a node can do (sense, compute, communicate) |
| **Composition** | Combining capabilities from multiple nodes |
| **CRDT** | Conflict-free Replicated Data Type |
| **Differential Update** | Sending only changed data |
| **Formation Key** | Shared secret for cell formation |
| **Leader** | Node coordinating a cell |
| **Coordinator** | Node coordinating a zone |

### 13.2 ADR Index

Key Architecture Decision Records:

| ADR | Title |
|-----|-------|
| [001](../../adr/001-cap-protocol-poc.md) | Peat Protocol POC |
| [004](../../adr/004-human-machine-cell-composition.md) | Human-Machine Cell Composition |
| [011](../../adr/011-ditto-vs-automerge-iroh.md) | Ditto vs Automerge/Iroh |
| [012](../../adr/012-schema-definition-protocol-extensibility.md) | Schema Definition |
| [017](../../adr/017-p2p-mesh-management-discovery.md) | P2P Mesh Discovery |
| [018](../../adr/018-ai-model-capability-advertisement.md) | AI Model Advertisement |
| [019](../../adr/019-qos-and-data-prioritization.md) | QoS and Prioritization |
| [020](../../adr/020-TAK-CoT-Integration.md) | TAK/CoT Integration |

### 13.3 External Resources

- [Ditto Documentation](https://docs.ditto.live/rust/)
- [Automerge Documentation](https://automerge.org/docs/)
- [Iroh Documentation](https://iroh.computer/docs/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Rust Book](https://doc.rust-lang.org/book/)

### 13.4 Getting Help

- **GitHub Issues**: [github.com/defenseunicorns/peat/issues](https://github.com/defenseunicorns/peat/issues)
- **Documentation**: [docs/INDEX.md](../../INDEX.md)
- **ADRs**: [docs/adr/](../../adr/)

---

**Document Version**: 1.0
**Last Updated**: 2025-12-08
**Maintainer**: Peat Development Team
