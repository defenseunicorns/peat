# Capabilities Aggregation Protocol (CAP)

> "Let me give you a threshold that's easy to understand: when we can fly drones by command, not by pilot. When your drones can understand commander's intent—that, ladies and gentlemen, is the threshold for AI autonomy to help us."
> — Brig. Gen. Travis McIntosh, on the Army's goal for autonomous drones

A hierarchical capability composition protocol using CRDTs for autonomous systems that scales to 100+ platforms with O(n log n) message complexity.

## Overview

The CAP protocol enables scalable coordination of autonomous platforms through:

- **Three-phase protocol**: Bootstrap → Squad Formation → Hierarchical Operations
- **CRDT-based state**: Eventual consistency via Ditto SDK
- **Capability composition**: Additive, emergent, redundant, and constraint-based patterns
- **Differential updates**: Bandwidth-efficient delta propagation (95%+ reduction)
- **Network efficiency**: Designed for constrained networks (9.6Kbps - 1Mbps)

## Quick Start

### Prerequisites

- Rust 1.70+ (2021 edition)
- Cargo

### Build

```bash
# Clone the repository
git clone https://github.com/kitplummer/cap.git
cd cap

# Build all crates
cargo build

# Run tests
cargo test

# Run the simulator
cargo run --bin cap-sim
```

### Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for detailed setup instructions, architecture overview, and contributing guidelines.

## Repository Structure

```
cap/
├── cap-protocol/          # Core protocol library
│   ├── src/
│   │   ├── bootstrap/     # Phase 1: Bootstrap
│   │   ├── squad/         # Phase 2: Squad Formation
│   │   ├── hierarchy/     # Phase 3: Hierarchical Operations
│   │   ├── composition/   # Capability composition engine
│   │   ├── delta/         # Differential update system
│   │   ├── network/       # Network simulation layer
│   │   ├── models/        # Data structures
│   │   └── storage/       # Ditto CRDT integration
│   └── Cargo.toml
├── cap-sim/               # Reference application & simulator
│   └── src/main.rs
├── docs/                  # Architecture & project docs
│   ├── ADR-001-CAP-Protocol-POC.md
│   └── CAP-POC-Project-Plan.md
└── DEVELOPMENT.md         # Development guide
```

## Project Status

**Current Phase**: Foundation & Setup (Week 1)

This is a proof-of-concept implementation following a 12-week development plan. See the [project plan](docs/CAP-POC-Project-Plan.md) for detailed roadmap.

### Recent Progress

✅ Repository initialized with Rust workspace
✅ Core trait definitions established
✅ CI/CD pipeline configured
✅ Development environment documented
✅ GitHub issues created for all 10 epics

### Next Steps

- Epic 1: Complete Ditto SDK integration spike
- Epic 2: Implement CRDT-based data models
- Epic 3: Begin bootstrap phase implementation

See [GitHub Issues](https://github.com/kitplummer/cap/issues) for current work items.

## Documentation

- [Architecture Decision Record](docs/ADR-001-CAP-Protocol-POC.md) - Technical architecture and decisions
- [Project Plan](docs/CAP-POC-Project-Plan.md) - 12-week implementation roadmap
- [Development Guide](DEVELOPMENT.md) - Setup, workflow, and contribution guidelines

## Key Features (Planned)

### Phase 1: Bootstrap
- Geographic self-organization (geohash-based)
- C2-directed assignment
- Capability-based queries
- O(√n) message complexity

### Phase 2: Squad Formation
- Deterministic leader election
- Intra-squad capability exchange
- Emergent capability detection
- Role assignment

### Phase 3: Hierarchical Operations
- Hierarchical message routing
- Multi-level capability aggregation
- Priority-based message queuing
- Differential state updates

## Success Metrics

- **Scalability**: O(n log n) message complexity (vs. O(n²) baseline)
- **Efficiency**: 95%+ bandwidth reduction via differential updates
- **Latency**: Priority 1 updates propagate in <5 seconds
- **Scale**: Support 100+ platforms in simulation

## Technology Stack

- **Language**: Rust 1.70+ (2021 edition)
- **CRDT Engine**: Ditto Rust SDK
- **Async Runtime**: Tokio 1.x
- **Serialization**: Serde + serde_json
- **Logging**: Tracing

## License

MIT OR Apache-2.0

## Contributing

Contributions are welcome! Please see [DEVELOPMENT.md](DEVELOPMENT.md) for guidelines.

## Contact

For questions or discussions, please open an issue on GitHub.

