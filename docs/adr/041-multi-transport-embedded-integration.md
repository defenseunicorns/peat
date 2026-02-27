# ADR-041: Multi-Transport Architecture and Embedded Integration

**Status**: Accepted
**Date**: 2024-12-22
**Authors**: Kit Plummer, Codex
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)
**Relates To**: ADR-035 (PEAT-Lite Embedded Nodes), ADR-039 (PEAT-BTLE Mesh Transport), ADR-032 (Pluggable Transport Abstraction), ADR-007 (Automerge Sync Engine), ADR-011 (Ditto vs AutomergeIroh)

---

## Executive Summary

This ADR defines how PEAT supports multiple simultaneous transports (AutomergeIroh, peat-btle, future transports) and how embedded devices integrate with full PEAT nodes. The key insight is that Automerge is too resource-intensive for embedded targets (ESP32, Pico), requiring a **gateway translation architecture** where full PEAT nodes translate between Automerge documents and peat-btle's lightweight CRDT format.

---

## Context

### The Multi-Transport Reality

PEAT must operate across diverse network conditions:

| Scenario | Transport | Bandwidth | Typical Nodes |
|----------|-----------|-----------|---------------|
| Full connectivity | Iroh (QUIC/UDP) | High (Mbps) | Phones, servers, laptops |
| BLE mesh | peat-btle | Low (Kbps) | Wearables, sensors, embedded |
| Degraded network | Either/both | Variable | All nodes adapt |
| Air-gapped | BLE only | Very low | Field operations |

A phone running PEAT may have both transports active simultaneously:
- WiFi/cellular: Syncing with cloud/HQ via Iroh
- BLE: Syncing with nearby embedded sensors via peat-btle

### The Automerge Problem

Per ADR-007 and ADR-011, PEAT's primary sync engine is AutomergeIroh - Automerge CRDTs synced via Iroh networking. However:

**Automerge Resource Requirements:**
- Binary size: ~2-3MB compiled
- Runtime RAM: 10-100MB+ depending on document size
- Requires `std` library (heap allocation, threading)
- Document history accumulates unbounded

**ESP32/Pico Constraints (per ADR-035):**
- Total RAM: 520KB (ESP32) / 264KB (Pico)
- Available for app: ~256KB after OS/stack
- No `std` - `no_std` embedded Rust only
- Flash: 4-16MB (binary + data)

**Conclusion**: Automerge cannot run on embedded targets. We need an alternative.

### Current Backend Situation

PEAT currently supports two sync backends:
1. **AutomergeIroh** - Open source, Automerge CRDTs + Iroh transport
2. **Ditto** - Commercial, proprietary CRDTs + proprietary transport

peat-btle introduces a third CRDT implementation (GCounter, LWW-Register, etc.) optimized for embedded. This creates a potential fragmentation problem.

---

## Decision

### Architecture: Gateway Translation Model

Full PEAT nodes act as **gateways** that translate between Automerge documents and peat-btle's lightweight format:

```
┌────────────────────────────────────────────────────────────────────────────┐
│                        Full PEAT Node (Phone/Server)                        │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                         AutomergeIroh                                 │  │
│  │              (Full Automerge documents, sync via Iroh)                │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                    ↕                                        │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                      Translation Layer                                │  │
│  │         (Maps peat-btle primitives ↔ Automerge document fields)       │  │
│  │                      OWNED BY PEAT REPO                               │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                    ↕                                        │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                          peat-btle                                    │  │
│  │              (BLE transport + lightweight CRDTs for embedded)         │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────┘
                                     ↕ BLE
┌────────────────────────────────────────────────────────────────────────────┐
│                      Embedded Node (ESP32/Pico)                             │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                          peat-btle                                    │  │
│  │          (BLE transport + lightweight CRDTs, standalone)              │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────┘
```

### Key Principles

#### 1. Schema Ownership Lives in PEAT

The canonical schema definition lives in the PEAT repository, not in peat-btle:

| Component | Owner | Description |
|-----------|-------|-------------|
| **Schema definition** | PEAT repo | Canonical field names, types, semantics |
| **Automerge representation** | PEAT repo | Full documents for std nodes |
| **Lightweight representation** | peat-btle | Embedded-friendly projection |
| **Translation logic** | PEAT repo | Maps between representations |
| **BLE transport** | peat-btle | Document-agnostic byte transport |

#### 2. peat-btle is Transport + Lightweight CRDTs

peat-btle provides two things:
1. **BLE Transport**: Discovery, connections, GATT, chunking - document agnostic
2. **Lightweight CRDTs**: GCounter, LWW-Register, Peripheral - the "embedded projection" of PEAT schema

The lightweight CRDTs exist because embedded devices can't run Automerge. They represent the **same semantic data** as the full schema, just in a format that fits in 256KB RAM.

#### 3. peat-btle Can Stand Alone

peat-btle must be usable independently for:
- Pure embedded deployments (ESP32 mesh without any full PEAT nodes)
- Open source release (standalone BLE mesh library)
- Testing and development

When used standalone, the lightweight CRDTs ARE the schema. When integrated with full PEAT, they become a projection that gets translated.

#### 4. BLE-First Schema Design

PEAT's schema should be designed with BLE constraints in mind:

```
DESIGN PRINCIPLE: If it works well on BLE, it works everywhere.
High-bandwidth transports just make it faster.
```

Benefits:
- Efficient sync across all transports
- No "lossy" translation for embedded
- Better battery life even on high-bandwidth transports
- Simpler mental model

### Integration Points

#### Full PEAT Node Receiving from Embedded

```rust
// In PEAT repo (not peat-btle)
impl TranslationLayer {
    /// Receive lightweight document from BLE, update Automerge
    fn on_ble_document_received(&mut self, data: &[u8]) -> Result<()> {
        // Decode peat-btle format
        let lite_doc = peat_btle::PeatDocument::decode(data)?;

        // Extract fields and map to Automerge document
        let node_id = lite_doc.node_id;
        let counter_value = lite_doc.counter.value();

        if let Some(peripheral) = lite_doc.peripheral {
            // Map peripheral fields to Automerge document
            self.automerge_doc.put(
                &format!("/sensors/{}/counter", node_id),
                counter_value
            )?;

            if let Some(event) = peripheral.last_event {
                match event.event_type {
                    EventType::Emergency => {
                        self.automerge_doc.put(
                            &format!("/alerts/{}/emergency", node_id),
                            event.timestamp
                        )?;
                    }
                    EventType::Ack => {
                        self.automerge_doc.put(
                            &format!("/alerts/{}/ack", node_id),
                            event.timestamp
                        )?;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
```

#### Full PEAT Node Sending to Embedded

```rust
// In PEAT repo (not peat-btle)
impl TranslationLayer {
    /// Build lightweight document from Automerge for BLE broadcast
    fn build_ble_document(&self) -> Vec<u8> {
        let mut doc = peat_btle::PeatDocument::new(self.node_id);

        // Extract relevant fields from Automerge
        // Only include what embedded nodes need
        if let Some(emergency_ts) = self.automerge_doc.get("/alerts/active/emergency") {
            doc.peripheral.as_mut().unwrap().set_event(
                peat_btle::EventType::Emergency,
                emergency_ts
            );
        }

        doc.encode()
    }
}
```

### Transport Abstraction

For true multi-transport support, PEAT should have a transport abstraction:

```rust
// In PEAT repo
trait PeatTransport {
    /// Send data to a peer
    async fn send(&self, peer_id: &NodeId, data: &[u8]) -> Result<()>;

    /// Receive data from peers
    fn receive(&self) -> impl Stream<Item = (NodeId, Vec<u8>)>;

    /// Get connected peers
    fn peers(&self) -> Vec<NodeId>;

    /// Transport capabilities
    fn capabilities(&self) -> TransportCapabilities;
}

// Implementations
struct IrohTransport { /* AutomergeIroh */ }
struct BleTransport { /* peat-btle */ }

// Transport manager selects best transport per peer
struct TransportManager {
    transports: Vec<Box<dyn PeatTransport>>,
}

impl TransportManager {
    fn send(&self, peer_id: &NodeId, data: &[u8]) -> Result<()> {
        // Try transports in priority order
        // Fall back to lower-bandwidth if higher fails
        for transport in &self.transports {
            if transport.can_reach(peer_id) {
                return transport.send(peer_id, data).await;
            }
        }
        Err(Error::NoPeerRoute)
    }
}
```

### Graceful Degradation

When network conditions change:

| Scenario | Behavior |
|----------|----------|
| Full connectivity | Use Iroh for full Automerge sync |
| WiFi fails, BLE available | Fall back to BLE, sync lightweight format |
| Only BLE available | Full PEAT nodes act as BLE mesh participants |
| BLE to embedded | Translate and sync lightweight format |

The translation layer ensures data flows correctly regardless of which transport is active.

---

## Consequences

### Positive

1. **Embedded devices can participate** - No Automerge requirement
2. **Clean separation** - peat-btle is standalone and useful independently
3. **Single source of truth** - Schema owned by PEAT, not duplicated
4. **Graceful degradation** - BLE works when WiFi fails
5. **OSS-friendly** - peat-btle can be released standalone

### Negative

1. **Translation complexity** - Must maintain mapping between formats
2. **Potential data loss** - Lightweight format is a subset of full schema
3. **Two CRDT implementations** - Automerge + peat-btle lightweight
4. **Testing surface** - Must test translation correctness

### Neutral

1. **Schema design constraint** - Must consider BLE limits when designing schema
2. **Documentation burden** - Must document what maps where

---

## Schema Mapping Guidelines

When designing PEAT schema, consider BLE representation:

| Full Schema Field | BLE Representation | Notes |
|-------------------|-------------------|-------|
| Node status | GCounter + Peripheral | Activity tracking |
| Emergency events | EventType::Emergency | Immediate priority |
| ACK responses | EventType::Ack | Confirmation |
| Position | Future: Position struct | GPS-equipped devices |
| Sensor readings | LWW-Register values | In Peripheral extension |
| Complex documents | NOT synced to embedded | Stay on full nodes |
| History/audit | NOT synced to embedded | Stay on full nodes |

**Rule of thumb**: If it needs to reach a wearable or sensor, it must fit in the lightweight format.

---

## Implementation Plan

### Phase 1: Document Architecture (This ADR)
- [x] Define gateway translation model
- [x] Clarify ownership (schema in PEAT, transport in peat-btle)
- [x] Document integration points

### Phase 2: peat-btle Standalone (Current Work)
- [x] Lightweight CRDTs (GCounter, Peripheral)
- [x] PeatDocument wire format
- [x] PeerManager, DocumentSync
- [ ] PeatMesh facade
- [ ] Platform bindings (UniFFI, JNI)

### Phase 3: PEAT Translation Layer (Future)
- [ ] Define schema-to-lightweight mapping
- [ ] Implement TranslationLayer in PEAT repo
- [ ] Integrate with AutomergeIroh documents

### Phase 4: Transport Abstraction (Future)
- [ ] Define PeatTransport trait
- [ ] Implement for Iroh
- [ ] Implement for peat-btle
- [ ] TransportManager with fallback logic

---

## References

- ADR-007: Automerge-Based Sync Engine
- ADR-011: Ditto vs AutomergeIroh Analysis
- ADR-032: Pluggable Transport Abstraction
- ADR-035: PEAT-Lite Embedded Nodes
- ADR-039: PEAT-BTLE Mesh Transport Crate
- [Automerge](https://automerge.org/) - CRDT library
- [Iroh](https://iroh.computer/) - P2P networking

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2024-12-22 | Gateway translation model | Automerge too heavy for ESP32, need lightweight alternative |
| 2024-12-22 | Schema ownership in PEAT | Single source of truth, peat-btle is projection |
| 2024-12-22 | peat-btle standalone capability | OSS release, pure embedded deployments |
| 2024-12-22 | BLE-first schema design | If it works on BLE, it works everywhere |
