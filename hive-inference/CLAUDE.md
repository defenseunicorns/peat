# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

HIVE Inference implements the M1 vignette: object tracking across distributed human-machine-AI teams. This crate is part of the HIVE workspace and uses the `hive-protocol` crate for mesh networking.

### Core Capabilities

- **Capability-based operations**: Platforms advertise capabilities upward; C2 sees aggregated formation capabilities before tasking
- **Bidirectional hierarchical flows**: Decisions/tracks flow upward (not raw data), capabilities/models flow downward
- **Cross-network coordination**: Bridge nodes connect isolated network segments
- **TAK integration**: HIVE-TAK Bridge translates between HIVE messages and CoT (Cursor on Target) XML

## Build Commands

```bash
cargo build                    # Build the project
cargo run                      # Run the application
cargo test                     # Run all tests
cargo test <name>              # Run a specific test
cargo clippy                   # Run linter
cargo fmt                      # Format code
```

## Related Workspace Crates

The core protocol implementation lives in sibling crates:

| Crate | Purpose |
|-------|---------|
| `hive-protocol` | Core protocol: discovery, cell formation, hierarchy, composition |
| `hive-schema` | Data models and validation |
| `hive-transport` | HTTP transport layer |
| `hive-persistence` | Storage backends |
| `hive-sim` | Network simulation |

### Key Modules in `hive-protocol`

- `discovery/` - Phase 1: Geographic, directed, and capability-based discovery
- `cell/` - Phase 2: Cell formation, leader election, capability aggregation
- `hierarchy/` - Phase 3: Routing, zone coordination, state aggregation
- `composition/` - Capability composition engine (additive, emergent, redundant, constraint)
- `models/` - Node, Cell, Zone, Capability, Operator data structures
- `storage/` - Ditto CRDT and Automerge backends
- `command/` - Bidirectional command coordination

### Dependency Configuration

This crate uses the AutomergeIroh backend (not Ditto):

```toml
# In Cargo.toml
[dependencies]
hive-protocol = { path = "../hive-protocol", default-features = false, features = ["automerge-backend"] }
```

The `automerge-backend` feature enables:
- Automerge CRDT library for document sync
- Iroh for QUIC-based P2P networking
- iroh-blobs for content-addressed blob storage (model distribution)
- mDNS service discovery

## Architecture

### Key Concepts

- **Platforms**: Individual entities (operators, UGVs/UAVs, AI models) that advertise capabilities
- **Teams/Cells**: Human-machine-AI cells (e.g., 1 operator + 1 vehicle + 1 AI model)
- **Coordinator/Bridge**: Aggregates team capabilities, bridges isolated networks, translates HIVE↔CoT
- **C2 Element**: Command element (TAK Server + WebTAK) that receives aggregated capabilities and issues tasking

### Message Types

- **Capability Advertisement**: Platforms advertise model versions, performance metrics, operational status
- **Track Updates**: POI position, confidence, velocity (~500 bytes vs 5 Mbps video)
- **Model Updates**: AI model packages distributed downward with rolling deployment
- **Commands**: Track target tasking, handoff commands, configuration

### Data Flow

1. Platforms advertise capabilities → aggregated at team → aggregated at coordinator → C2 sees full picture
2. C2 issues tasking → flows down through hierarchy to capable teams
3. AI models process locally, send track updates (not raw video) upward
4. Model updates flow downward with hash verification and rollback support
- We are going to use the AutomergeIroh backend and avoid Ditto for this PoC