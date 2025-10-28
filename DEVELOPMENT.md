# CAP Protocol - Development Guide

## Overview

This repository contains the Capabilities Aggregation Protocol (CAP) proof-of-concept implementation in Rust. The CAP protocol enables scalable coordination of autonomous platforms through hierarchical capability composition using CRDTs.

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
│   │   ├── storage/       # Ditto CRDT integration
│   │   └── traits.rs      # Core trait definitions
│   └── Cargo.toml
├── cap-sim/               # Reference application & simulator
│   ├── src/
│   │   └── main.rs        # Simulation harness
│   └── Cargo.toml
├── docs/                  # Architecture & project docs
├── .github/workflows/     # CI/CD pipelines
└── Cargo.toml            # Workspace configuration
```

## Prerequisites

### Required

- **Rust** 1.70 or later (2021 edition)
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- **Ditto SDK** - The Ditto Rust SDK will be installed via Cargo

### Optional

- **cargo-make** - Task automation
  ```bash
  cargo install cargo-make
  ```

- **cargo-watch** - Auto-rebuild on changes
  ```bash
  cargo install cargo-watch
  ```

- **cargo-nextest** - Faster test runner
  ```bash
  cargo install cargo-nextest
  ```

## Getting Started

### 1. Clone and Setup

```bash
git clone <repository-url>
cd cap
```

### 2. Build the Project

```bash
# Build all crates in the workspace
cargo build

# Build in release mode
cargo build --release
```

### 3. Run Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_platform_state
```

### 4. Run the Simulator

```bash
# Run the reference simulator
cargo run --bin cap-sim

# Run with debug logging
RUST_LOG=debug cargo run --bin cap-sim
```

### 5. Code Quality

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Run all checks
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings
```

## Development Workflow

### Branch Strategy

- `main` - Stable releases
- `develop` - Integration branch for features
- `feature/*` - Feature branches
- `fix/*` - Bug fix branches

### Commit Conventions

Follow Conventional Commits:

```
feat: Add geographic bootstrap strategy
fix: Correct leader election tie-breaking
docs: Update README with setup instructions
test: Add property tests for CRDT operations
refactor: Simplify capability composition engine
```

### Pull Request Process

1. Create a feature branch from `develop`
2. Implement changes with tests
3. Ensure all tests pass: `cargo test`
4. Check formatting: `cargo fmt --check`
5. Run clippy: `cargo clippy`
6. Create PR with description linking to issue
7. Wait for CI to pass
8. Request review from maintainers

## Testing Strategy

### Unit Tests

Located alongside code in module files:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_initialization() {
        let config = PlatformConfig::new("UAV".to_string());
        assert_eq!(config.platform_type, "UAV");
    }
}
```

### Integration Tests

Located in `tests/` directory:

```rust
// tests/bootstrap_test.rs
#[tokio::test]
async fn test_bootstrap_phase() {
    // Test end-to-end bootstrap
}
```

### Property Tests

Using `proptest` for property-based testing:

```rust
proptest! {
    #[test]
    fn test_capability_composition_associative(a: f32, b: f32, c: f32) {
        // Verify composition is associative
    }
}
```

### Benchmarks

Located in `benches/` directory:

```bash
cargo bench
```

## Logging and Debugging

### Log Levels

Use `RUST_LOG` environment variable:

```bash
# All debug logs
RUST_LOG=debug cargo run

# Specific module
RUST_LOG=cap_protocol::bootstrap=trace cargo run

# Multiple modules
RUST_LOG=cap_protocol=debug,cap_sim=info cargo run
```

### Tracing

The project uses `tracing` for structured logging:

```rust
use tracing::{info, debug, warn, error, instrument};

#[instrument]
async fn bootstrap_platform(id: &str) -> Result<()> {
    info!("Starting bootstrap for platform {}", id);
    debug!("Platform details: {:?}", details);
    // ...
}
```

## Architecture Overview

### Three-Phase Protocol

1. **Bootstrap Phase** - Constrained discovery and initial group formation
   - Geographic self-organization
   - C2-directed assignment
   - Capability-based queries

2. **Squad Formation Phase** - Intra-squad cohesion and leader election
   - Capability exchange
   - Leader election
   - Role assignment
   - Capability aggregation

3. **Hierarchical Operations Phase** - Hierarchical routing and operations
   - Constrained messaging
   - Multi-level aggregation
   - Priority-based routing
   - Flow control

### Data Flow

```
Platform State (CRDT)
    ↓
Change Detection
    ↓
Delta Generation
    ↓
Priority Assignment
    ↓
Hierarchical Router
    ↓
Network Transport
```

### CRDT Integration

The protocol uses Ditto SDK for CRDT synchronization:

- **G-Set** - Grow-only sets (static capabilities)
- **OR-Set** - Observed-remove sets (squad membership)
- **LWW-Register** - Last-write-wins registers (leader, position)
- **PN-Counter** - Positive-negative counters (fuel)

## Performance Guidelines

### Targets

- Platform state update: <10ms p99
- Delta generation: <5ms p99
- Capability composition: <20ms p99
- Leader election: <5 seconds
- Bootstrap (100 platforms): <60 seconds

### Profiling

```bash
# Profile with flamegraph
cargo install flamegraph
sudo flamegraph --bin cap-sim

# Profile with perf
cargo build --release
perf record --call-graph=dwarf target/release/cap-sim
perf report
```

## Common Tasks

### Adding a New Capability Type

1. Update `models/capability.rs`:
   ```rust
   pub enum CapabilityType {
       Sensor,
       // ... existing types
       NewType,  // Add your type
   }
   ```

2. Implement composition rules in `composition/rules/`

3. Add tests

### Adding a Bootstrap Strategy

1. Create module in `bootstrap/`
2. Implement bootstrap logic
3. Register in `bootstrap/coordinator.rs`
4. Add integration tests

### Extending the Hierarchy

1. Update `hierarchy/router.rs` for new levels
2. Update `hierarchy/platoon.rs` for aggregation
3. Adjust metrics collection
4. Test with simulation

## Troubleshooting

### Build Issues

**Problem**: Ditto SDK not found

**Solution**: Ensure Cargo can access the Ditto crate. Check network connectivity and cargo registry.

### Test Failures

**Problem**: Timing-sensitive tests fail intermittently

**Solution**: Use `tokio::time::pause()` for deterministic time in tests.

### Performance Issues

**Problem**: Simulation runs slowly with many platforms

**Solution**:
- Profile with `cargo flamegraph`
- Check for O(n²) algorithms
- Reduce logging verbosity
- Use release builds

## Documentation

### Generating Docs

```bash
# Generate and open documentation
cargo doc --open --no-deps

# Include private items
cargo doc --document-private-items
```

### Documentation Guidelines

- All public APIs must have doc comments
- Include examples in doc comments
- Document safety requirements
- Link related types with `[Type]` syntax

## Release Process

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md
3. Create release branch: `release/v0.1.0`
4. Run full test suite
5. Merge to `main`
6. Tag release: `git tag v0.1.0`
7. Push tags: `git push --tags`

## Contributing

See the project plan in `docs/CAP-POC-Project-Plan.md` for current priorities and roadmap.

### Areas for Contribution

- Phase implementations (bootstrap, squad, hierarchical)
- Composition rule patterns
- Network simulation realism
- Visualization improvements
- Performance optimization
- Documentation

## Resources

- [Project Plan](docs/CAP-POC-Project-Plan.md)
- [Architecture Decision Record](docs/ADR-001-CAP-Protocol-POC.md)
- [Ditto Documentation](https://docs.ditto.live/rust/)
- [Rust Book](https://doc.rust-lang.org/book/)

## Getting Help

- Review documentation in `docs/`
- Check GitHub Issues
- Ask questions in pull requests
- Contact project maintainers

## License

MIT OR Apache-2.0 - see LICENSE files
