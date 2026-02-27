# Multi-Hop Sync Investigation (Issue #159)

**Date**: 2025-11-24
**Status**: Decision Made
**Blocks**: #106 (Security & Authorization)

## Decision (2025-11-24)

**Trust Model A: Trust All Mesh Members** selected for MVP.

**Rationale:**
1. Matches Ditto's apparent model - simpler parity
2. Peers responsible for aggregation/dissemination must read documents for CRDT sync
3. E2E encryption deferred to Phase 2 due to complexity
4. Practical: hierarchical relay nodes (e.g., squad leaders aggregating to platoon) need document visibility

## Executive Summary

Both Ditto and Iroh-gossip provide multi-hop data propagation by design. However, neither provides built-in end-to-end encryption for untrusted relay scenarios. This has significant implications for our security architecture (ADR-006).

## Ditto Multi-Hop Behavior

### Protocol: Flood-Fill

Ditto uses a **flood-fill** pattern for mesh synchronization:

> "When all peers connected in the mesh need to share the same view of the data, Ditto initiates the flood-fill process to multihop data from one connected peer to another connected peer by way of intermediate 'hops' along a given path."

**Key Characteristics:**
- **Automatic**: Multi-hop is built-in, no configuration needed
- **CRDT-based**: Only deltas (changes) are synced, not full documents
- **Presence Graph**: Maintains routing information for adaptive path selection
- **Resilient**: If intermediate peer becomes unavailable, presence graph facilitates route updates

### Transport Layer
- Supports BLE, P2P Wi-Fi, LAN, WebSocket, Cellular
- Automatic transport selection based on availability
- Random connection churn for network adaptation

### Security Model (Gap Analysis)
- **APP ID / License Token**: Proprietary authentication at mesh level
- **Encryption**: TLS for transport, but intermediate nodes likely see decrypted data
- **Access Control**: Not documented for multi-hop scenarios
- **Question**: Can intermediate nodes read documents they relay?

**Assessment**: Ditto appears to use a "trust all mesh members" model. Each node in the mesh has the same APP ID and can read all synced data.

## Iroh Gossip Protocol

### Protocol: HyParView + PlumTree

Iroh-gossip is based on two academic papers:
- **HyParView**: Peer sampling and overlay management
- **PlumTree**: Epidemic broadcast trees for efficient dissemination

**Key Characteristics:**
- **Topic-based**: Peers subscribe to 32-byte topic IDs
- **Epidemic Broadcast**: Messages propagate through tree structure
- **Multi-hop by Design**: Messages travel through intermediate peers
- **Probabilistic**: Not all nodes are directly connected

### Architecture
```
┌─────────────────────────────────────────────┐
│                 iroh-gossip                  │
├─────────────────┬───────────────────────────┤
│   proto module  │       net module          │
│  (state machine)│  (networking on iroh)     │
└─────────────────┴───────────────────────────┘
```

### Integration with iroh-docs
- iroh-docs uses iroh-gossip for sync coordination
- Replica synchronization over gossip topics
- Combined with iroh-blobs for content transfer

### Security Model (Gap Analysis)
- **Transport**: QUIC with TLS 1.3 (via Iroh)
- **Message Content**: Not explicitly encrypted at application layer
- **Question**: Do intermediate gossip nodes see message content?

**Assessment**: Iroh provides secure transport but doesn't appear to encrypt gossip message content beyond TLS. Intermediate nodes in the gossip tree likely see message payloads.

## Comparison Matrix

| Feature | Ditto | Iroh-Gossip |
|---------|-------|-------------|
| Multi-hop | Flood-fill | Epidemic broadcast tree |
| Automatic | Yes | Yes (with topic subscription) |
| Transport encryption | TLS | QUIC/TLS 1.3 |
| End-to-end encryption | Not documented | Not built-in |
| Access control | APP ID level | Topic level |
| Intermediate visibility | Likely yes | Likely yes |
| CRDT integration | Built-in | Via iroh-docs |

## Security Implications for PEAT

### Critical Question
**Can we trust all mesh members?**

In tactical scenarios:
- **Trusted mesh**: All nodes have same clearance, multi-hop just works
- **Mixed trust**: Some nodes are untrusted relays (e.g., captured device)
- **Adversarial**: Enemy node joins mesh to exfiltrate data

### Proposed Trust Models

#### Model A: Trust All Mesh Members (Current)
```
A ←→ B ←→ C
     ↓
  B sees data
```
- Simplest implementation
- Matches Ditto's apparent model
- **Risk**: Compromised node sees everything

#### Model B: End-to-End Encryption with Untrusted Relay
```
A ←────encrypted────→ C
     ↓
  B forwards opaque blob
```
- Application-layer encryption before sync
- Relay nodes cannot read content
- **Challenge**: Key distribution, CRDT merge on encrypted data

#### Model C: Selective Document Encryption
```
A ←→ B ←→ C

Doc X: Encrypted for A,C only (B cannot read)
Doc Y: Plaintext (all can read)
```
- Per-document encryption policies
- Group keys for authorized readers
- **Challenge**: Key rotation when members change

## Recommendations for ADR-006 Update

### 1. Document Trust Model Decision
ADR-006 should explicitly state which trust model we're implementing:
- Phase 1: Trust all mesh members (matches Ditto)
- Phase 2: Selective document encryption for sensitive data

### 2. Layer Security Appropriately
```
Layer 1: Transport (TLS/QUIC) - Both backends provide this
Layer 2: Mesh Authentication (PKI) - Who can join the mesh
Layer 3: Document Encryption (optional) - E2E for sensitive data
Layer 4: Authorization (RBAC) - What operations are allowed
```

### 3. Multi-Hop Authentication Chain
For Model B/C, need to address:
- How does C verify data originated from A (not forged by B)?
- Options: Signed documents, certificate chains, hash chains

### 4. Key Management for E2E
If implementing Model B/C:
- Cell-level group keys (members of cell X share key)
- Key rotation when members leave
- Offline key distribution (pre-mission provisioning)

## Next Steps

1. [ ] Confirm Ditto behavior with test (3-node chain sync)
2. [ ] Confirm Iroh-gossip behavior with test
3. [ ] Decide on trust model for Phase 1
4. [ ] Update ADR-006 with multi-hop section
5. [ ] Design E2E encryption layer (if Model B/C chosen)

## Sources

- [Ditto Mesh Networking](https://docs.ditto.live/key-concepts/mesh-networking)
- [Ditto Mesh Networking 101](https://docs.ditto.live/sdk/latest/sync/concepts/mesh-networking-101)
- [iroh-gossip Documentation](https://docs.rs/iroh-gossip/latest/iroh_gossip/)
- [HyParView Paper](https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf)
- [PlumTree Paper](https://asc.di.fct.unl.pt/~jleitao/pdf/srds07-leitao.pdf)
