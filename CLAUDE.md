# Claude Code Project Guide - PEAT Protocol

## Project Overview

PEAT (Protocol for Emergent Autonomous Teaming) is a Rust workspace with 10 crates implementing a hierarchical mesh networking protocol for tactical edge environments. It supports multiple CRDT backends (Ditto, Automerge), transports (QUIC, BLE, UDP), and targets (servers, mobile, embedded).

**GitHub**: `defenseunicorns/peat`

## Workspace Crates

### Libraries (6)

| Crate | Purpose |
|-------|---------|
| `peat-schema` | Protobuf wire format (beacon, mission, capability, security, CoT, AI) |
| `peat-protocol` | Core protocol: cells, hierarchy, sync, security, CRDT backends |
| `peat-transport` | HTTP/REST API layer (Axum-based) |
| `peat-persistence` | Storage abstraction (Redb, SQLite backends) |
| `peat-discovery` | Peer discovery (mDNS, static, hybrid) |
| `peat-ffi` | Mobile bindings (Kotlin/Swift via UniFFI + JNI) |

### Binaries (4)

| Crate | Purpose |
|-------|---------|
| `peat-sim` | Network simulator — validates hierarchical protocol at scale |
| `peat-inference` | Edge AI/ML pipeline (ONNX YOLOv8, GStreamer) |
| `peat-tak-bridge` | TAK/ATAK CoT interoperability bridge |
| `peat-ble-test` | BLE integration test harness (Pi-to-Android) |

## Dependency Graph

```
peat-schema (standalone — protobuf definitions)
  └─► peat-protocol (core hub — all app crates depend on this)
        ├─► peat-transport
        ├─► peat-persistence
        ├─► peat-ffi
        ├─► peat-inference
        ├─► peat-tak-bridge
        ├─► peat-ble-test
        └─► peat-sim

peat-discovery (standalone — no internal deps)
```

### External Crate Dependencies

| Crate | Version | Repo | Role |
|-------|---------|------|------|
| `peat-mesh` | 0.3.1 | `defenseunicorns/peat-mesh` | P2P topology, QUIC/Iroh, Automerge sync |
| `peat-btle` | 0.2.0 | `defenseunicorns/peat-btle` | BLE mesh transport |
| `peat-lite` | 0.2.0 | `defenseunicorns/peat-lite` | Embedded wire protocol (via peat-mesh) |

For local development against these, uncomment `[patch.crates-io]` in root `Cargo.toml`.

## Build Commands

```bash
# Build everything
make build

# Quick development cycle
make test-fast          # Unit tests only (~30s)
make check              # fmt + clippy + test

# Full test suite (tiered)
make test-unit          # Unit tests with nextest (~30s)
make test-integration   # Integration tests (~2 min)
make test-e2e           # E2E tests (~5 min)
make test               # All of the above

# CI checks (what GitHub Actions runs)
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## Simulation

```bash
make validate                   # Quick 24-node hierarchical validation
make compare-architectures      # Hub-spoke vs mesh vs hierarchical
make backend-comparison         # Ditto vs Automerge (24-node)
make build-docker               # Build peat-sim Docker image (run first)
```

## Android / ATAK

```bash
make build-android              # Cross-compile peat-ffi for Android
make build-atak-plugin          # Build ATAK plugin APK
make deploy-atak-plugin         # Deploy to connected device
```

## Functional Tests (Hardware)

```bash
make dual-transport-test        # BLE + QUIC on Pi + Android tablet
make functional-suite           # All hardware tests (BLE + Android + K8s)
```

## CRDT Backends

- **Ditto** (default feature `ditto-backend`): proprietary, production-grade
- **Automerge** (feature `automerge-backend`): pure Rust, works on Android, enables peat-mesh extraction

Toggle in `peat-protocol/Cargo.toml` features.

## Key Documentation

| Path | What |
|------|------|
| `docs/ARCHITECTURE.md` | Five-layer architecture overview |
| `docs/ARCHITECTURE-DECISION-SUMMARY.md` | ADR quick reference |
| `DEVELOPMENT.md` | Development quickstart |
| `docs/spec/` | 5 IETF-style protocol specs |

### Essential ADRs (start here)

| ADR | Topic |
|-----|-------|
| `docs/adr/001-cap-protocol-poc.md` | Protocol design rationale |
| `docs/adr/005-datasync-abstraction-layer.md` | Sync abstraction |
| `docs/adr/011-ditto-vs-automerge-iroh.md` | Backend selection |
| `docs/adr/032-pluggable-transport-abstraction.md` | Transport layer |
| `docs/adr/035-hive-lite-embedded-nodes.md` | Embedded wire protocol |
| `docs/adr/049-hive-mesh-extraction.md` | peat-mesh standalone design |

## CI Workflows

| Workflow | Trigger | What |
|----------|---------|------|
| `ci.yml` | push/PR | fmt, clippy, unit/integration/E2E tests |
| `simulation.yml` | push to `peat-sim/**` | Docker build + 24-node smoke test |
| `android.yml` | manual/PR | Cross-compile peat-ffi, build ATAK APK |
| `functional-test.yml` | manual/release | Dual-transport BLE+QUIC on self-hosted Pi |
| `benchmarks.yml` | manual | Criterion benchmarks |

## Key Files

| Path | Purpose |
|------|---------|
| `Cargo.toml` | Workspace root — members, shared deps, patch overrides |
| `Makefile` | 60+ targets for build, test, sim, android, functional |
| `peat-protocol/src/lib.rs` | Core protocol entry point |
| `peat-schema/proto/` | Protobuf definitions |
| `peat-sim/Dockerfile` | Simulation node Docker image |
| `peat-sim/simple-validate.sh` | Smoke test script |
| `peat-ffi/src/lib.rs` | UniFFI mobile bindings |

## Related Repositories

- **peat-mesh** (`../hive-mesh/`): Standalone mesh networking — topology, sync, K8s, broker
- **peat-btle** (`../hive-btle/`): BLE mesh transport — multi-platform, GATT sync
- **peat-lite** (`../hive-lite/`): Embedded CRDT primitives — no_std, wire protocol, ESP32 firmware
