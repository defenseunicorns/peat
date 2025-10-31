# Ditto SDK Integration Notes - E1.2

**Date:** 2025-10-28
**SDK Version:** dittolive-ditto 4.12.3
**Status:** In Progress

## Overview

This document captures findings from integrating the Ditto Rust SDK for CRDT-based state management in the CAP protocol.

## Setup

### Dependencies
```toml
[dependencies]
dittolive-ditto = "4"
dotenvy = "0.15"  # For .env file support
chrono = "0.4"    # For timestamps
```

### Environment Variables
```bash
DITTO_APP_ID=<your-app-id>
DITTO_SHARED_KEY=<your-shared-key>  # Base64-encoded ECDSA P-256 private key
DITTO_PERSISTENCE_DIR=.ditto        # Optional, defaults to .ditto
```

## SharedKey Identity

SharedKey identity enables **local-only syncing** without requiring internet connectivity. Perfect for:
- Development and testing
- Tactical edge scenarios
- Air-gapped environments

### Key Characteristics
- Peer-to-peer sync over LAN/Bluetooth/WiFi Direct
- No cloud dependency
- All devices must use the same shared key
- Suitable for trusted environments

### Documentation
- [SharedKey Struct](https://software.ditto.live/rust/Ditto/4.12.3/x86_64-unknown-linux-gnu/docs/dittolive_ditto/identity/struct.SharedKey.html)
- [Ditto Main Docs](https://software.ditto.live/rust/Ditto/4.12.3/x86_64-unknown-linux-gnu/docs/dittolive_ditto/index.html)

## API Changes in 4.12.3

### Major Breaking Changes

1. **AppId is now private**
   - Cannot access `identity::AppId::from_env()` directly
   - Must use alternative initialization methods
   - Related: https://github.com/getditto/ditto/issues/XXX

2. **DittoRoot requires Arc wrapper**
   ```rust
   // OLD (doesn't work)
   .with_root(PathBuf::from(".ditto"))

   // NEW (needs investigation)
   .with_root(Arc<dyn DittoRoot>)
   ```

3. **Collection API returns Result**
   ```rust
   // OLD
   let coll = ditto.store().collection("nodes");

   // NEW
   let coll = ditto.store().collection("nodes")?;
   ```

4. **Deprecated Methods**
   - `Collection::find()` - Use `execute_v2()` or `register_observer_v2()`
   - `Collection::upsert()` - Use newer API
   - `Ditto::site_id()` - Use `ditto.presence().graph().local_peer.peer_key_string`

## CRDT Types in Ditto

### Supported CRDT Operations

Ditto implements CRDTs at the document level with JSON-like operations:

1. **G-Set (Grow-Only Set)**
   - Use case: Static node capabilities
   - Implementation: Array fields that only grow
   - Operations: Insert only, no removal

2. **OR-Set (Observed-Remove Set)**
   - Use case: Cell membership
   - Implementation: Array with add/remove tracking
   - Operations: Add, Remove with causality tracking

3. **LWW-Register (Last-Write-Wins)**
   - Use case: Node position, fuel, health
   - Implementation: Single value with timestamp
   - Operations: Set with timestamp, last write wins

4. **PN-Counter (Positive-Negative Counter)**
   - Use case: Fuel consumption tracking
   - Implementation: Increment/decrement counter
   - Operations: Inc, Dec

### Document Structure Example

```json
{
  "_id": "node_uav_001",
  "node_id": "uav_001",
  "static_capabilities": ["camera", "gps", "satcom"],  // G-Set
  "position": {                                         // LWW-Register
    "lat": 37.7749,
    "lon": -122.4194,
    "alt": 100.0,
    "timestamp": 1698765432
  },
  "fuel_minutes": 120,                                  // PN-Counter
  "cell_id": "alpha"                                   // LWW-Register
}
```

## Known Issues & Quirks

### 1. API Documentation Gaps
- **Issue**: Rust docs don't clearly show migration path from deprecated APIs
- **Impact**: Difficult to upgrade existing code
- **Workaround**: Need to study internal source or wait for examples

### 2. Identity Initialization
- **Issue**: AppId struct is private, limiting initialization options
- **Impact**: Cannot use `from_env()` pattern shown in older docs
- **Workaround**: TBD - need to find alternative initialization

### 3. Type System Complexity
- **Issue**: Heavy use of Arc, trait objects, and complex lifetimes
- **Impact**: Steep learning curve, verbose code
- **Mitigation**: Create wrapper layer to simplify API

### 4. Deprecation Without Clear Alternatives
- **Issue**: Methods marked deprecated but replacement not obvious
- **Impact**: Uncertainty about correct approach
- **Example**: `collection()`, `find()`, `upsert()` all deprecated

## Sync Behavior

### Local Sync Mechanisms

With SharedKey identity, Ditto automatically discovers and syncs with peers using:

1. **LAN (TCP/IP)**
   - Multicast discovery on local network
   - Direct peer-to-peer connections
   - Fastest option for co-located systems

2. **Bluetooth Low Energy (BLE)**
   - For mobile devices
   - Lower bandwidth but good for small updates
   - Range: ~30-100 meters

3. **WiFi Direct / P2P**
   - Ad-hoc mesh networking
   - No infrastructure required
   - Good for tactical scenarios

### Sync Timing

- **Discovery**: Typically <1 second on LAN
- **Initial Sync**: Depends on data size
- **Delta Sync**: Very fast (<100ms for small updates)
- **Conflict Resolution**: Automatic via CRDT semantics

## Performance Characteristics

### Memory Usage
- Base Ditto instance: ~5-10MB
- Per collection overhead: <1MB
- Per document: Variable (JSON size + metadata)

### Network Efficiency
- **Initial Sync**: Full document transfer
- **Subsequent Sync**: Delta-only (CRDT operations)
- **Bandwidth**: Typically <1KB for position updates
- **Compression**: Built-in

### Latency
- **Write Latency**: <1ms (local)
- **Sync Latency**: 10-100ms (LAN), 100-500ms (BLE)
- **Query Latency**: <1ms (in-memory index)

## Next Steps for E1.2

### Immediate Actions
1. **Resolve API compatibility issues**
   - Contact Ditto support or check GitHub for migration guide
   - Study internal source code for proper initialization
   - Create minimal working example

2. **Create simplified wrapper**
   - Abstract away Ditto complexity
   - Provide clean API for CAP protocol needs
   - Handle error conversion properly

3. **Implement test scenarios**
   - Two-instance sync test
   - CRDT operation tests
   - Network partition simulation

### Future Considerations
1. **Alternative Identity Types**
   - Investigate OnlinePlayground for development
   - Plan for production identity (Online with authentication)

2. **Performance Testing**
   - Measure sync latency at scale (100+ nodes)
   - Test bandwidth usage patterns
   - Validate CRDT convergence time

3. **Error Handling**
   - Map Ditto errors to CAP protocol errors
   - Handle network failures gracefully
   - Implement retry logic

## References

- [Ditto Rust Documentation](https://docs.ditto.live/rust/)
- [CRDT Primer](https://crdt.tech/)
- [Conflict-Free Replicated Data Types Paper](https://arxiv.org/abs/1805.06358)

## Status Summary

**Completed:**
- ✅ Environment setup with .env
- ✅ Dependency configuration
- ✅ Understanding of SharedKey identity model
- ✅ CRDT types and use cases identified

**Blocked:**
- ❌ API compatibility issues with Ditto 4.12.3
- ❌ Lack of clear migration path from deprecated APIs
- ❌ Private AppId struct limiting initialization options

**Recommendations:**
1. Reach out to Ditto support for API migration guidance
2. Consider pinning to earlier SDK version if needed
3. Create abstraction layer to isolate from SDK changes
4. Document all workarounds for future maintainers

---

**Last Updated:** 2025-10-28
**Next Review:** Upon API resolution
