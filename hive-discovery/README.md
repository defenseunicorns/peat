# hive-discovery

Peer discovery layer for the HIVE Protocol mesh intelligence system.

## Overview

`hive-discovery` provides pluggable discovery strategies for finding and tracking peers in a distributed mesh network. This is **Layer 1** of the P2P Mesh Intelligence architecture (ADR-017).

## Features

- **mDNS Discovery**: Zero-configuration local network peer discovery using multicast DNS
- **Static Configuration**: Pre-configured peer lists for EMCON (Emission Control) operations
- **Hybrid Discovery**: Combine multiple discovery strategies simultaneously
- **Backend Agnostic**: Works with any CRDT/networking backend (Ditto, Automerge+Iroh, etc.)
- **Event-Driven**: Subscribe to peer found/lost/updated events

## Usage

### mDNS Discovery (Local Networks)

```rust
use hive_discovery::{MdnsDiscovery, DiscoveryStrategy};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut discovery = MdnsDiscovery::new()?;

    // Advertise this node
    discovery.advertise("my-node-id", 5000, None)?;

    // Subscribe to discovery events
    let mut events = discovery.event_stream()?;

    // Start discovering peers
    discovery.start().await?;

    // Listen for discovered peers
    while let Some(event) = events.recv().await {
        match event {
            DiscoveryEvent::PeerFound(peer) => {
                println!("Discovered peer: {} at {:?}", peer.node_id, peer.addresses);
            }
            DiscoveryEvent::PeerLost(node_id) => {
                println!("Lost peer: {}", node_id);
            }
            _ => {}
        }
    }

    Ok(())
}
```

### Static Configuration

Create a TOML configuration file (`peers.toml`):

```toml
[[peers]]
node_id = "company-hq"
addresses = ["10.0.0.100:5000", "192.168.1.100:5000"]
relay_url = "https://relay.example.com:3479"
priority = 255

[[peers]]
node_id = "platoon-1-leader"
addresses = ["10.0.1.50:5000"]
priority = 200
```

Then use it:

```rust
use hive_discovery::{StaticDiscovery, DiscoveryStrategy};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut discovery = StaticDiscovery::from_file(Path::new("peers.toml"))?;
    discovery.start().await?;

    let peers = discovery.discovered_peers().await;
    for peer in peers {
        println!("Configured peer: {}", peer.node_id);
    }

    Ok(())
}
```

### Hybrid Discovery (Combine Multiple Strategies)

```rust
use hive_discovery::{HybridDiscovery, MdnsDiscovery, StaticDiscovery};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut discovery = HybridDiscovery::new();

    // Add mDNS for local network
    let mdns = MdnsDiscovery::new()?;
    discovery.add_strategy("mdns", Box::new(mdns));

    // Add static config for pre-planned peers
    let static_disc = StaticDiscovery::from_file(Path::new("peers.toml"))?;
    discovery.add_strategy("static", Box::new(static_disc));

    // Start all strategies
    discovery.start_all().await?;

    // Get all discovered peers from all sources
    let peers = discovery.all_discovered_peers().await;
    println!("Discovered {} peers total", peers.len());

    Ok(())
}
```

## Architecture

This crate implements Layer 1 of the mesh intelligence architecture defined in ADR-017:

```
┌─────────────────────────────────────────┐
│     Layer 3: Data Flow Control          │  (hive-mesh - future)
│   Routing, Aggregation, Priority        │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│  Layer 2: Mesh Topology Management      │  (hive-mesh - future)
│   Beacons, Hierarchy, Connections       │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│     Layer 1: Discovery Strategies       │  ← THIS CRATE
│   mDNS, Static Config, Relay Discovery  │
└─────────────────────────────────────────┘
```

## Design Goals

1. **Simplicity**: Clean, minimal API that's easy to understand and use
2. **Flexibility**: Pluggable strategies that can be combined as needed
3. **Testability**: Comprehensive unit tests with no external dependencies
4. **Backend Agnostic**: Works with any CRDT/networking layer
5. **Production Ready**: Proper error handling, logging, and documentation

## Testing

Run all tests:

```bash
cargo test -p hive-discovery
```

Run with logging:

```bash
RUST_LOG=debug cargo test -p hive-discovery -- --nocapture
```

## Future Work

- Relay-based discovery for NAT traversal (Phase 1, Week 2)
- Integration tests with Containerlab (Phase 1, Week 2)
- Performance benchmarks
- Additional discovery strategies (DHT, gossip, etc.)

## References

- [ADR-017: P2P Mesh Management and Discovery Architecture](../docs/adr/017-p2p-mesh-management-discovery.md)
- [EPIC 2: P2P Mesh Intelligence Layer](https://github.com/kitplummer/hive/issues/105)
