# Contributing to Peat

Thank you for your interest in contributing to Peat. This document covers development setup, testing, and the pull request process.

## Getting Started

1. Fork the repository and clone your fork
2. Create a feature branch from `main`
3. Make your changes
4. Run pre-commit checks
5. Submit a pull request

## Development Setup

### Prerequisites

- Rust stable toolchain (install via [rustup](https://rustup.rs))
- Protobuf compiler (`protoc`)
- [mold](https://github.com/rui314/mold) linker (configured in `.cargo/config.toml`)
- System dependencies: `clang`, `libdbus-1-dev` (Linux)

### Workspace Crates

| Crate | Purpose |
|-------|---------|
| `peat-schema` | Protobuf wire format definitions |
| `peat-protocol` | Core protocol: cells, hierarchy, sync, security, CRDT backends |
| `peat-transport` | HTTP/REST API (Axum) |
| `peat-persistence` | Storage abstraction (Redb, SQLite) |
| `peat-discovery` | Peer discovery (mDNS, static, hybrid) |
| `peat-ffi` | Mobile bindings (Kotlin/Swift via UniFFI + JNI) |
| `peat-tak-bridge` | TAK/ATAK CoT interoperability bridge |
| `peat-ble-test` | BLE integration test harness |

### Feature Flags

The `peat-protocol` crate uses feature flags for optional transports and bindings:

| Feature | Description |
|---------|-------------|
| `automerge-backend` (default) | Automerge CRDT backend with Iroh QUIC transport |
| `lite-transport` | Embedded node transport via peat-lite |
| `bluetooth` | BLE mesh transport via peat-btle |

### Building

```bash
make build                  # full workspace
cargo build                 # default features (Automerge + Iroh)
```

## Testing

```bash
make test-fast              # unit tests (~30s)
make check                  # fmt + clippy + test
make test                   # unit + integration + e2e
make validate               # 24-node hierarchical simulation
```

## Pre-Commit Checks

Before submitting a PR, ensure all of the following pass locally:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace --exclude peat-ffi -- -D warnings
cargo test --workspace --exclude peat-ffi
```

The CI pipeline runs these same checks on every PR.

## Branching Strategy

We use **trunk-based development** on `main` with short-lived feature branches:

- Branch from `main` for all changes
- Keep branches small and focused (prefer multiple small PRs over one large one)
- Squash-and-merge to `main`

## Commit Requirements

- **GPG-signed commits are required.** Configure commit signing per [GitHub's documentation](https://docs.github.com/en/authentication/managing-commit-signature-verification).
- Write clear, descriptive commit messages

## Pull Request Access

Submitting pull requests requires contributor access to the repository. If you're interested in contributing, please open an issue to introduce yourself and discuss the change you'd like to make. A maintainer will grant PR access to active contributors.

## Pull Request Process

1. Open a PR against `main` with a clear description of the change
2. Focus each PR on a single concern
3. Ensure CI passes (fmt, clippy, tests, feature builds)
4. PRs require at least one approving review from a CODEOWNERS member
5. PRs are squash-merged to maintain a clean history

## Architectural Changes

For significant architectural changes, open an issue first to discuss the approach. Reference the relevant ADR in `docs/adr/` if one exists, or propose a new one. See the [Architecture Decision Summary](docs/ARCHITECTURE-DECISION-SUMMARY.md) for context.

## Reporting Issues

Use GitHub Issues to report bugs or request features. Include steps to reproduce, expected vs. actual behavior, and relevant log output.

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
