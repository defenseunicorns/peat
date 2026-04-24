<p align="center">
  <img src="https://raw.githubusercontent.com/defenseunicorns/peat/main/assets/peat-wordmark.png" alt="PEAT" width="420">
</p>

# peat-protocol

[![crates.io](https://img.shields.io/crates/v/peat-protocol.svg)](https://crates.io/crates/peat-protocol)
[![docs.rs](https://img.shields.io/docsrs/peat-protocol)](https://docs.rs/peat-protocol)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://github.com/defenseunicorns/peat/blob/main/LICENSE)

The SDK entry point for the [Peat](https://github.com/defenseunicorns/peat) coordination protocol — hierarchical capability composition over CRDTs for heterogeneous mesh networks (phones, servers, sensors, embedded devices, AI models).

## Facade

`peat-protocol` is the single public crate. It re-exports [`peat-mesh`](https://crates.io/crates/peat-mesh) (P2P topology, QUIC/Iroh transport, Automerge CRDT sync) and [`peat-schema`](https://crates.io/crates/peat-schema) (Protobuf wire types), so downstream consumers depend on `peat-protocol` alone:

```rust
use peat_protocol::{peat_mesh, peat_schema};
```

## Add the dep

```toml
[dependencies]
# During the rc window, use the exact-version pin — Cargo does not
# select pre-release versions by default with the caret form.
peat-protocol = "=0.9.0-rc.1"

# Once 0.9.0 stable is published, the normal selector works:
# peat-protocol = "0.9"
```

## Features

| Feature | Default | Purpose |
|---------|---------|---------|
| `automerge-backend` | yes | Automerge CRDT over Iroh QUIC — the standard backend |
| `lite-transport` | no | UDP-based [peat-lite](https://crates.io/crates/peat-lite) bridge for microcontrollers |
| `bluetooth` | no | [peat-btle](https://crates.io/crates/peat-btle) BLE mesh transport |

## How it works

Peat organizes diverse systems through three phases:

1. **Discovery** — nodes find each other via mDNS, BLE advertisements, static config, or geographic clustering.
2. **Cell formation** — discovered nodes compose capabilities into cells (additive, emergent, redundant, or constraint-based).
3. **Hierarchical operations** — cells self-organize into zones for efficient state sharing across the mesh.

State is CRDT-based (Automerge), so the protocol operates through network partitions without a central server.

## Documentation

- API reference: <https://docs.rs/peat-protocol>
- Ecosystem overview, integration guide, and ADRs: <https://github.com/defenseunicorns/peat>

## License

Apache-2.0
