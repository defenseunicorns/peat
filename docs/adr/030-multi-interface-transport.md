# ADR-030: Multi-Interface Transport for Network Bridging

**Status**: Accepted
**Date**: 2025-12-02
**Authors**: Kit Plummer, Codex
**Relates To**: ADR-011 (Automerge + Iroh), ADR-017 (P2P Mesh Management)

---

## Investigation Results (2025-12-02)

**Critical Finding: Iroh already handles multi-interface automatically!**

When calling `Endpoint::builder().bind().await`, Iroh:
1. Discovers ALL local network interfaces
2. Binds to all of them simultaneously
3. Advertises all addresses in `EndpointAddr`

Test output showing automatic multi-interface discovery:
```
EndpointAddr {
    id: PublicKey(...),
    addrs: {
        Ip(100.85.90.54:56575),   // External/Tailscale IP
        Ip(192.168.1.95:56575),    // LAN IPv4
        Ip([2600:1700:...]:56576), // IPv6 (multiple)
    },
}
```

**Implication**: Our current `IrohTransport::bind(bind_addr)` that forces a single address is actually **limiting** Iroh's native capabilities. The default `IrohTransport::new()` (which binds to all interfaces) is the correct approach for multi-network scenarios.

---

## Context and Problem Statement

### The Multi-Network Reality

Tactical platforms operate across multiple disparate networks simultaneously:

- **Tactical Radio (MANET)**: Low bandwidth (300bps-2Mbps), high latency, mesh topology
- **WiFi/Ethernet**: Medium bandwidth, low latency, infrastructure-based
- **Starlink (Satellite)**: High bandwidth, high latency, requires gateway
- **5G (Private Cellular)**: Variable bandwidth, medium latency, coverage-dependent

**Critical Use Case: Network Bridging**

A platform with access to multiple networks must act as a **bridge** to sync data across network boundaries:

```
┌─────────────────┐         ┌─────────────────┐         ┌─────────────────┐
│  Radio Network  │◄───────►│  Bridge Node    │◄───────►│  WiFi Network   │
│  (MANET peers)  │         │  (Multi-NIC)    │         │  (Base station) │
└─────────────────┘         └─────────────────┘         └─────────────────┘
      eth0                        eth0 + eth1                  eth1
   192.168.1.x                                              10.0.0.x
```

### Current Implementation Gap

**ADR-011 claims Iroh provides**:
> Multi-path support: Simultaneous use of multiple network interfaces (Starlink + MANET + 5G)

**Current implementation reality** (`iroh_transport.rs`):
```rust
pub async fn bind(bind_addr: SocketAddr) -> Result<Self> {
    // Only binds to ONE address
    let endpoint = Endpoint::builder()
        .bind_addr_v4(bind_addr_v4)  // Single interface
        .bind()
        .await?;
}
```

The current `IrohTransport` only supports binding to a **single socket address**, making network bridging impossible.

---

## Decision Drivers

### Requirements

1. **Multi-Interface Binding**: Bind to multiple network interfaces simultaneously
2. **Cross-Network Sync**: Sync CRDT data between nodes on different physical networks
3. **Interface-Aware Routing**: Route messages based on destination network
4. **Graceful Degradation**: Continue operating if one interface fails
5. **Let Iroh Manage**: Prefer Iroh's native capabilities over custom implementation

### Constraints

1. **Iroh API Limitations**: Current Iroh API may not expose multi-interface directly
2. **IPv4 Only**: Current implementation only supports IPv4 (`bind_addr_v4`)
3. **Single Endpoint Model**: Each `IrohTransport` wraps one `Endpoint`

---

## Considered Options

### Option 1: Multiple Transport Instances (Application-Level)

Create separate `IrohTransport` instances per interface, route at application layer.

```rust
pub struct MultiBridgeTransport {
    transports: HashMap<NetworkInterface, Arc<IrohTransport>>,
    router: MessageRouter,
}

impl MultiBridgeTransport {
    pub async fn new(interfaces: Vec<NetworkInterface>) -> Result<Self> {
        let mut transports = HashMap::new();
        for iface in interfaces {
            let transport = IrohTransport::bind(iface.bind_addr).await?;
            transports.insert(iface, Arc::new(transport));
        }
        Ok(Self { transports, router: MessageRouter::new() })
    }

    pub async fn broadcast(&self, message: &[u8]) -> Result<()> {
        for transport in self.transports.values() {
            transport.broadcast(message).await?;
        }
        Ok(())
    }
}
```

**Pros**:
- Works with current Iroh API
- Simple to implement
- Clear separation of concerns

**Cons**:
- Multiple QUIC endpoints (higher resource usage)
- Application must handle cross-transport routing
- No unified connection migration between interfaces
- Doesn't leverage Iroh's multipath capabilities

### Option 2: Iroh Multipath QUIC (Let Iroh Manage)

Use Iroh's native multipath QUIC support (if available).

Per [QUIC Multipath Extension (RFC draft)](https://datatracker.ietf.org/doc/draft-ietf-quic-multipath/), QUIC supports:
- Multiple paths within single connection
- Path-aware packet scheduling
- Connection migration between paths

**Iroh's approach** (from iroh.computer docs):
- Endpoint discovers all local addresses automatically
- Peers exchange address info via relay or direct connection
- Path probing selects best path dynamically

```rust
// Iroh may already bind to all interfaces by default
let endpoint = Endpoint::builder()
    .bind()  // Binds to 0.0.0.0 - all interfaces
    .await?;

// Iroh should advertise all addresses
let addr = endpoint.addr();  // Contains all interface addresses
```

**Pros**:
- Leverages Iroh's production-tested multipath
- Single endpoint, multiple paths
- Automatic path selection and failover
- Connection migration built-in

**Cons**:
- Requires Iroh to support this (need to verify)
- Less control over interface selection
- May not work for network bridging (different subnets)

### Option 3: Hybrid Approach (RECOMMENDED)

1. **Use Iroh's multipath** for same-subnet multi-interface
2. **Use multiple endpoints** for cross-subnet bridging

```rust
pub struct BridgeTransport {
    /// Primary endpoint (Iroh manages multi-interface on same subnet)
    primary: Arc<IrohTransport>,

    /// Bridge endpoints for isolated networks
    bridges: HashMap<NetworkId, Arc<IrohTransport>>,

    /// Cross-network message relay
    relay: BridgeRelay,
}

impl BridgeTransport {
    pub async fn new(config: BridgeConfig) -> Result<Self> {
        // Primary endpoint for main network (Iroh multi-interface)
        let primary = IrohTransport::new().await?;  // Binds to all interfaces

        // Additional endpoints for isolated networks (if needed)
        let mut bridges = HashMap::new();
        for isolated_net in config.isolated_networks {
            let bridge = IrohTransport::bind(isolated_net.bind_addr).await?;
            bridges.insert(isolated_net.id, Arc::new(bridge));
        }

        Ok(Self {
            primary: Arc::new(primary),
            bridges,
            relay: BridgeRelay::new(),
        })
    }
}
```

**Pros**:
- Best of both worlds
- Iroh manages multi-interface within networks
- Custom logic for cross-network bridging
- Flexible for different deployment scenarios

**Cons**:
- More complex implementation
- Need to understand when to use which approach

---

## Decision

**Adopt Option 2: Let Iroh Manage (Simplified)**

Based on investigation results, Iroh natively handles multi-interface. The decision is:

1. **Default**: Use `IrohTransport::new()` (not `bind(addr)`) for all standard deployments
   - Iroh automatically discovers and binds to ALL interfaces
   - Peers receive all addresses and can connect via any one

2. **Restrict Only When Needed**: Use `IrohTransport::bind(addr)` only when explicitly limiting to one interface (e.g., security isolation)

3. **Bridge Mode (Future)**: Only needed for truly air-gapped networks where Iroh's automatic discovery cannot work. Defer implementation until required.

**Implementation is simpler than anticipated** - we primarily need to ensure our code uses the default `new()` constructor rather than forcing single-interface binding.

---

## Implementation Plan

### Phase 1: Verify Iroh Multipath (Investigation) - COMPLETE ✅

**Test**: `peat-protocol/tests/iroh_multipath_investigation.rs`

**Result**: Iroh automatically advertises all interfaces:
- IPv4 LAN addresses
- IPv6 addresses
- External/VPN addresses (e.g., Tailscale)

```rust
// This already works - Iroh discovers all interfaces
let endpoint = Endpoint::builder()
    .alpns(vec![b"peat/1".to_vec()])
    .bind()  // Binds to ALL interfaces automatically
    .await?;

let addr = endpoint.addr();
// addr.addrs contains ALL interface addresses
```

### Phase 2: Audit IrohTransport Usage - TODO

Review codebase to ensure we're using the multi-interface-friendly constructor:

**Current code** (`iroh_transport.rs`):
```rust
// GOOD - uses all interfaces
pub async fn new() -> Result<Self> {
    let endpoint = Endpoint::builder()
        .alpns(...)
        .bind()  // Binds to all interfaces
        .await?;
}

// LIMITING - forces single interface (use only when explicitly needed)
pub async fn bind(bind_addr: SocketAddr) -> Result<Self> {
    let endpoint = Endpoint::builder()
        .bind_addr_v4(bind_addr_v4)  // Single interface only
        .bind()
        .await?;
}
```

**Action**: Ensure production code paths use `new()` by default. The `bind()` method should be reserved for:
- Testing (deterministic ports)
- Security isolation (restrict to specific interface)
- Legacy compatibility

### Phase 3: Bridge Transport (DEFERRED - Future)

**Not needed for most deployments** since Iroh handles multi-interface natively.

Only implement if a specific requirement arises for truly air-gapped/isolated networks where:
- Network A has no IP route to Network B
- A node physically connected to both must relay traffic

```rust
// Future: Only if needed
pub struct BridgeTransport {
    main: Arc<IrohMeshTransport>,
    bridges: HashMap<String, Arc<IrohMeshTransport>>,
}
```

### Phase 4: Documentation Update - TODO

Update documentation to clarify multi-interface support:

1. **README**: Note that Iroh automatically uses all available interfaces
2. **PeerConfig docs**: Clarify when to use `bind_address` vs letting Iroh choose
3. **Deployment guide**: Explain that bridge nodes just need multiple NICs - Iroh handles the rest

---

## Success Criteria

### Functional Requirements

- [ ] Single node can bind to multiple network interfaces
- [ ] Peers on different networks can sync via bridge node
- [ ] Interface failure doesn't break sync on other interfaces
- [ ] Configuration supports per-interface peer assignment

### Performance Requirements

- [ ] Message relay latency < 50ms additional overhead
- [ ] No message loss during interface transitions
- [ ] Memory overhead < 10MB per additional interface

### Testing

- [ ] Unit tests for multi-transport routing
- [ ] Integration tests with simulated multi-network topology
- [ ] Containerlab test with isolated network namespaces

---

## Open Questions

1. **Iroh Multipath Status**: Does current Iroh version (0.35+) support multi-interface out of box?
   - Need to test with actual multi-NIC setup

2. **Same Endpoint ID?**: Should bridge transports share same EndpointId or have separate identities?
   - Separate: Cleaner isolation, but more complex peer management
   - Shared: Unified identity, but may confuse Iroh's path selection

3. **Relay Protocol**: How should cross-network messages be relayed?
   - Option A: Re-wrap in new QUIC stream on destination network
   - Option B: Use Automerge sync protocol (document-level bridging)
   - Option C: Use Iroh Gossip for multi-network broadcast

---

## References

1. [Iroh Documentation](https://iroh.computer/docs)
2. [QUIC Multipath RFC Draft](https://datatracker.ietf.org/doc/draft-ietf-quic-multipath/)
3. ADR-011: CRDT + Networking Stack Selection
4. ADR-017: P2P Mesh Management and Discovery

---

**Last Updated**: 2025-12-02
**Investigation Complete**: Iroh natively supports multi-interface
**Decision Status**: ✅ **ACCEPTED** - Let Iroh manage interfaces; defer BridgeTransport until needed

## Summary

**Question**: Does AutomergeIroh backend support multiple network interfaces?

**Answer**: **Yes** - Iroh automatically discovers and binds to all available interfaces when using the default `bind()` method. Peers receive all addresses and can connect via any one.

**Action Items**:
1. ✅ Investigation complete - Iroh handles multi-interface natively
2. 🔲 Audit codebase to ensure `IrohTransport::new()` is used in production paths
3. 🔲 Update documentation
4. 🔲 (Future) Implement BridgeTransport only if air-gapped network support is required
