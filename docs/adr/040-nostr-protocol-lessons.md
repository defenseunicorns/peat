# ADR-040: Nostr Protocol Lessons for PEAT Architecture

**Status**: Proposed  
**Date**: 2025-12-15  
**Authors**: Kit Plummer, Claude  
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)  
**Relates To**: ADR-007 (Automerge Sync Engine), ADR-012 (Schema Definition), ADR-017 (P2P Mesh Management), ADR-021 (Document-Oriented Architecture), ADR-027 (Event Routing), ADR-035 (PEAT-Lite), ADR-039 (BTLE Mesh)

---

## Executive Summary

This ADR captures architectural lessons from the Nostr protocol and its NIPs (Nostr Implementation Possibilities) that inform PEAT Protocol design. Nostr represents a novel "Relay Architecture" that accepts network centralization as inevitable while preserving user key ownership. PEAT can adapt several Nostr patterns—particularly NIP-29 (Relay-Based Groups), NIP-77 (Negentropy Syncing), and the event-centric data model—while maintaining its distinct "Hierarchical Aggregation Architecture" optimized for contested tactical environments without cloud infrastructure.

---

## Context

### The Nostr Discovery

Gordon Brander's analysis "[Nature's Many Attempts to Evolve a Nostr](https://newsletter.squishy.computer/p/natures-many-attempts-to-evolve-a)" identifies a critical insight: all network architectures eventually centralize due to fundamental network science principles:

1. **Preferential attachment**: More connections → more network effect → more connections
2. **N² scaling**: Mesh networks scale O(n²) connections, creating pressure for hub emergence
3. **Fitness pressure**: Reliable, high-bandwidth nodes survive; unreliable nodes fail
4. **Efficiency**: Exponentially-distributed networks are "ultra-small worlds"

Rather than fighting this physics, Nostr accepts it while preserving user sovereignty through cryptographic key ownership. The protocol treats servers as "dumb, untrusted pipes" (relays) while users own their keys and can migrate freely.

### The Four Network Architectures

| Architecture | Scale Strategy | Key Assumption | Best For |
|--------------|----------------|----------------|----------|
| **Centralized** | Bigger servers | Unlimited resources | Enterprise SaaS |
| **Federated** | More servers talk | Servers cooperate | Social platforms (Mastodon) |
| **Relay (Nostr)** | Dumb pipes + smart clients | Cloud available | Censorship resistance |
| **Hierarchical (PEAT)** | Decompose by scope | No cloud, hierarchy exists | Tactical coordination |

PEAT represents a fourth architecture: **Hierarchical Aggregation**—designed for environments where relays don't exist, peers are bandwidth-constrained, and command hierarchy naturally dictates information flow.

### Why Nostr Matters for PEAT

Despite different target environments, Nostr and PEAT share fundamental challenges:

1. **Eventual consistency** across disconnected nodes
2. **Cryptographic identity** independent of any server
3. **Efficient synchronization** when bandwidth is constrained
4. **Extensible schema** for diverse message types
5. **Decentralized authority** (no single point of control)

---

## Nostr Protocol Analysis

### Core Primitive: The Event

Nostr's entire protocol centers on a single object type—the **Event**:

```json
{
  "id": "<32-bytes sha256 hash of serialized event>",
  "pubkey": "<32-bytes public key of creator>",
  "created_at": "<unix timestamp in seconds>",
  "kind": "<integer 0-65535>",
  "tags": [["key", "value", "..."], ...],
  "content": "<arbitrary string>",
  "sig": "<64-bytes Schnorr signature>"
}
```

**Design Principles**:
- **Single object type**: Everything is an Event—metadata, posts, reactions, DMs
- **Cryptographic identity**: Schnorr signatures on secp256k1 (Bitcoin-compatible)
- **Kind-based extensibility**: `kind` field determines event semantics
- **Tag-based indexing**: Single-letter tags (`e`, `p`, `a`) are indexed by relays
- **Content flexibility**: Arbitrary string payload interpreted per-kind

### NIP-29: Relay-Based Groups

NIP-29 defines closed groups managed by relay authority—directly analogous to PEAT's hierarchical echelons:

**Key Patterns**:

1. **Relay as Group Authority**: Groups exist within a relay's jurisdiction. The relay signs metadata events (kind 39000-39003) establishing group identity, membership, and roles.

2. **The `h` Tag**: Every event in a group includes `["h", "<group-id>"]`, enabling scope filtering:
   ```json
   {
     "kind": 9,
     "tags": [
       ["h", "pizza-lovers"],
       ["previous", "<event-id-prefix>", "..."]
     ],
     "content": "Hello everyone!"
   }
   ```

3. **Role-Based Access**: Roles are arbitrary labels with relay-defined permissions:
   ```json
   {
     "kind": 39001,
     "tags": [
       ["d", "<group-id>"],
       ["p", "<pubkey1>", "admin", "add-user", "delete-event"],
       ["p", "<pubkey2>", "moderator", "delete-event"]
     ]
   }
   ```

4. **The `previous` Tag**: Events reference prior events seen from the same relay, creating lightweight causal ordering without full CRDT overhead.

5. **Anti-Replay Protection**: "Relays should prevent late publication (messages published now with a timestamp from days or even hours ago)."

**PEAT Parallel**: The squad leader's node is the "relay" for that squad. The `h` tag maps to hierarchical scope (`squad/alpha-1`). Roles map to military positions (squad leader, team leader).

### NIP-77: Negentropy Syncing

NIP-77 defines an efficient set reconciliation protocol for syncing events between nodes:

**The Problem**: When two nodes have overlapping event sets, how do they efficiently discover:
- Which events does A have that B doesn't?
- Which events does B have that A doesn't?

**The Solution**: Range-Based Set Reconciliation (RBSR) using the Negentropy protocol:

1. **Sort events** by (timestamp, id) tuples
2. **Compute fingerprints** for ranges using additive hashing
3. **Binary search** by exchanging range fingerprints
4. **Converge** in O(log n) round trips for small differences

**Wire Protocol**:
```
NEG-OPEN: [subscription_id, filter, initial_message_hex]
NEG-MSG:  [subscription_id, message_hex]
NEG-CLOSE: [subscription_id]
NEG-ERR:  [subscription_id, reason]
```

**Fingerprint Algorithm**:
1. Compute addition mod 2^256 of all element IDs (32-byte little-endian)
2. Concatenate with element count as varint
3. SHA-256 hash
4. Take first 16 bytes

**Key Properties**:
- Works for client-relay and relay-relay sync
- Bandwidth-efficient when sets overlap significantly
- Decoupled from storage structure (tree, array, etc.)
- Supports frame size limits for constrained transports

**PEAT Application**: Use Negentropy for **event discovery** (what capability reports exist?) before CRDT state sync (what's the current aggregated state?).

### Other Relevant NIPs

| NIP | Purpose | PEAT Relevance |
|-----|---------|----------------|
| NIP-01 | Basic protocol flow | Event structure, filter syntax |
| NIP-09 | Event deletion | Tombstone handling (ADR-034) |
| NIP-11 | Relay information | Node capability advertisement |
| NIP-40 | Expiration timestamp | TTL management (ADR-016) |
| NIP-42 | Client authentication | Node authentication (ADR-006) |
| NIP-44 | Encrypted payloads | End-to-end encryption |
| NIP-59 | Gift wrap | Message privacy/metadata protection |
| NIP-65 | Relay list metadata | "Outbox model" for capability discovery |
| NIP-70 | Protected events | Preventing event leakage across contexts |

---

## Decision

### Adopt Nostr Patterns for PEAT

#### 1. Event Structure Alignment

Adopt a Nostr-compatible event structure as PEAT's base message format:

```rust
/// PEAT Protocol Event (Nostr-compatible base)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeatEvent {
    /// SHA-256 hash of serialized event data
    pub id: EventId,  // [u8; 32]
    
    /// Public key of event creator (Ed25519 or secp256k1)
    pub pubkey: PublicKey,  // [u8; 32]
    
    /// Unix timestamp in seconds
    pub created_at: u64,
    
    /// Event kind (determines semantics)
    pub kind: u16,
    
    /// Indexed metadata tags
    pub tags: Vec<Vec<String>>,
    
    /// Event payload (interpretation per-kind)
    pub content: String,
    
    /// Cryptographic signature
    pub sig: Signature,  // [u8; 64]
}

impl PeatEvent {
    /// Compute event ID per Nostr spec
    pub fn compute_id(&self) -> EventId {
        let serialized = json!([
            0,  // Reserved
            hex::encode(&self.pubkey),
            self.created_at,
            self.kind,
            self.tags,
            self.content
        ]);
        sha256(serialized.to_string().as_bytes())
    }
    
    /// Verify signature
    pub fn verify(&self) -> Result<(), SignatureError> {
        verify_schnorr(&self.pubkey, &self.id, &self.sig)
    }
}
```

#### 2. PEAT Kind Registry

Define PEAT-specific event kinds in reserved ranges:

```rust
/// PEAT Protocol Kind Registry
/// 
/// Ranges:
/// - 0-9999: Nostr standard kinds (reserved)
/// - 10000-19999: PEAT core protocol
/// - 20000-29999: PEAT integrations (TAK, Link 16, etc.)
/// - 30000-39999: Replaceable events (Nostr standard)
/// - 40000-49999: Application-specific (user-defined)

pub mod kinds {
    // PEAT Core Protocol (10000-10999)
    pub const CAPABILITY_REPORT: u16 = 10001;
    pub const CAPABILITY_AGGREGATE: u16 = 10002;
    pub const COMMAND_INTENT: u16 = 10003;
    pub const COMMAND_ACK: u16 = 10004;
    pub const HEALTH_BEACON: u16 = 10005;
    pub const HIERARCHY_ANNOUNCEMENT: u16 = 10006;
    pub const SYNC_REQUEST: u16 = 10007;
    pub const SYNC_RESPONSE: u16 = 10008;
    
    // PEAT Coordination (10100-10199)
    pub const TASK_ASSIGNMENT: u16 = 10100;
    pub const TASK_STATUS: u16 = 10101;
    pub const RESOURCE_REQUEST: u16 = 10102;
    pub const RESOURCE_OFFER: u16 = 10103;
    
    // PEAT AI/ML (10200-10299)
    pub const MODEL_CAPABILITY: u16 = 10200;
    pub const INFERENCE_REQUEST: u16 = 10201;
    pub const INFERENCE_RESULT: u16 = 10202;
    
    // PEAT Security (10300-10399)
    pub const KEY_ANNOUNCEMENT: u16 = 10300;
    pub const KEY_ROTATION: u16 = 10301;
    pub const ACCESS_GRANT: u16 = 10302;
    pub const ACCESS_REVOKE: u16 = 10303;
    
    // TAK Integration (20000-20099)
    pub const COT_EVENT: u16 = 20000;
    pub const COT_AGGREGATE: u16 = 20001;
    
    // Link 16 Integration (20100-20199)
    pub const LINK16_J_MESSAGE: u16 = 20100;
    pub const LINK16_PPLI: u16 = 20101;
}
```

#### 3. Hierarchical Scope Tags

Adopt the `h` tag pattern for hierarchical scoping:

```rust
/// PEAT Hierarchical Scope Tags
/// 
/// Tag structure: ["h", "<echelon>/<unit-id>"]
/// Examples:
///   ["h", "squad/alpha-1"]
///   ["h", "platoon/1st"]  
///   ["h", "company/bravo"]
///   ["h", "battalion/2-506"]

pub struct HierarchyTag {
    pub echelon: Echelon,
    pub unit_id: String,
}

impl HierarchyTag {
    pub fn to_tag(&self) -> Vec<String> {
        vec!["h".to_string(), format!("{}/{}", self.echelon, self.unit_id)]
    }
    
    pub fn from_tag(tag: &[String]) -> Option<Self> {
        if tag.len() >= 2 && tag[0] == "h" {
            let parts: Vec<&str> = tag[1].splitn(2, '/').collect();
            if parts.len() == 2 {
                return Some(Self {
                    echelon: parts[0].parse().ok()?,
                    unit_id: parts[1].to_string(),
                });
            }
        }
        None
    }
}

/// Filter events by hierarchical scope
pub fn filter_by_scope(events: &[PeatEvent], scope: &HierarchyTag) -> Vec<&PeatEvent> {
    events.iter()
        .filter(|e| e.tags.iter().any(|t| {
            HierarchyTag::from_tag(t)
                .map(|h| h.is_within_scope(scope))
                .unwrap_or(false)
        }))
        .collect()
}
```

#### 4. Causal Ordering via `previous` Tag

Adopt lightweight causal ordering without full vector clocks:

```rust
/// Previous Event Reference
/// 
/// Tag structure: ["previous", "<event-id-prefix>", "<event-id-prefix>", ...]
/// 
/// Events reference the most recent events seen in the same scope,
/// creating implicit causal ordering for conflict detection.

pub struct PreviousTag {
    /// Event ID prefixes (first 8 bytes hex = 16 chars)
    pub event_prefixes: Vec<String>,
}

impl PreviousTag {
    pub const PREFIX_LENGTH: usize = 16;  // 8 bytes hex-encoded
    
    pub fn from_events(events: &[EventId]) -> Self {
        Self {
            event_prefixes: events.iter()
                .map(|id| hex::encode(&id[..8]))
                .collect()
        }
    }
    
    pub fn to_tag(&self) -> Vec<String> {
        let mut tag = vec!["previous".to_string()];
        tag.extend(self.event_prefixes.clone());
        tag
    }
}

/// Builder for events with causal references
impl PeatEvent {
    pub fn with_previous(mut self, seen_events: &[EventId]) -> Self {
        let prev = PreviousTag::from_events(seen_events);
        self.tags.push(prev.to_tag());
        self
    }
}
```

#### 5. Negentropy Integration for Sync

Integrate Negentropy for efficient event discovery:

```rust
/// Negentropy-based Event Discovery
/// 
/// Use for:
/// - Initial sync after connection
/// - Reconnection after network partition
/// - Cross-echelon state reconciliation
/// 
/// Complements (does not replace) Automerge CRDT sync.

pub struct NegentropySync {
    /// Local event store reference
    storage: Arc<dyn EventStorage>,
    
    /// Negentropy state per peer
    sessions: RwLock<HashMap<NodeId, NegentropySession>>,
}

impl NegentropySync {
    /// Initiate sync with peer
    pub async fn initiate(
        &self,
        peer: NodeId,
        filter: EventFilter,
    ) -> Result<SyncSession, SyncError> {
        // Get local events matching filter
        let local_events = self.storage.query(&filter).await?;
        
        // Build negentropy state
        let mut neg = Negentropy::new(16, None)?;  // 16-byte fingerprints
        for event in &local_events {
            neg.add_item(event.created_at, &event.id)?;
        }
        neg.seal()?;
        
        // Create initial message
        let initial_msg = neg.initiate()?;
        
        Ok(SyncSession {
            peer,
            filter,
            negentropy: neg,
            have_ids: Vec::new(),
            need_ids: Vec::new(),
        })
    }
    
    /// Process peer response
    pub async fn reconcile(
        &self,
        session: &mut SyncSession,
        peer_msg: &[u8],
    ) -> Result<ReconcileResult, SyncError> {
        let (response, have, need) = session.negentropy.reconcile(peer_msg)?;
        
        session.have_ids.extend(have);
        session.need_ids.extend(need);
        
        if response.is_empty() {
            Ok(ReconcileResult::Complete {
                have: session.have_ids.clone(),
                need: session.need_ids.clone(),
            })
        } else {
            Ok(ReconcileResult::Continue(response))
        }
    }
}

/// Combined sync strategy
pub struct PeatSyncEngine {
    /// Negentropy for event discovery
    negentropy: NegentropySync,
    
    /// Automerge for CRDT state sync
    automerge: AutomergeSync,
}

impl PeatSyncEngine {
    /// Full sync sequence after partition
    pub async fn full_sync(&self, peer: NodeId) -> Result<SyncReport, SyncError> {
        // Phase 1: Event discovery via Negentropy
        let filter = EventFilter::for_scope(self.local_scope());
        let mut session = self.negentropy.initiate(peer, filter).await?;
        
        loop {
            let msg = self.send_and_receive(peer, session.current_message()).await?;
            match self.negentropy.reconcile(&mut session, &msg).await? {
                ReconcileResult::Complete { have, need } => {
                    // Phase 2: Fetch missing events
                    let missing = self.fetch_events(peer, &need).await?;
                    self.storage.insert_batch(missing).await?;
                    
                    // Phase 3: Push events peer needs
                    let to_send = self.storage.get_batch(&have).await?;
                    self.push_events(peer, &to_send).await?;
                    
                    break;
                }
                ReconcileResult::Continue(response) => {
                    session.set_message(response);
                }
            }
        }
        
        // Phase 4: CRDT state sync for current aggregations
        self.automerge.sync(peer).await?;
        
        Ok(SyncReport { /* ... */ })
    }
}
```

#### 6. Filter Syntax Compatibility

Adopt Nostr's filter syntax for event queries:

```rust
/// Event Filter (Nostr-compatible)
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EventFilter {
    /// Event IDs to match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<EventId>>,
    
    /// Author public keys to match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<PublicKey>>,
    
    /// Event kinds to match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<u16>>,
    
    /// Events referencing these event IDs
    #[serde(rename = "#e", skip_serializing_if = "Option::is_none")]
    pub e_tags: Option<Vec<EventId>>,
    
    /// Events referencing these pubkeys
    #[serde(rename = "#p", skip_serializing_if = "Option::is_none")]
    pub p_tags: Option<Vec<PublicKey>>,
    
    /// Events in these hierarchical scopes
    #[serde(rename = "#h", skip_serializing_if = "Option::is_none")]
    pub h_tags: Option<Vec<String>>,
    
    /// Events created after this timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    
    /// Events created before this timestamp  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
    
    /// Maximum number of events to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl EventFilter {
    /// Filter for a hierarchical scope
    pub fn for_scope(scope: &str) -> Self {
        Self {
            h_tags: Some(vec![scope.to_string()]),
            ..Default::default()
        }
    }
    
    /// Filter for capability reports in scope
    pub fn capability_reports(scope: &str) -> Self {
        Self {
            kinds: Some(vec![kinds::CAPABILITY_REPORT]),
            h_tags: Some(vec![scope.to_string()]),
            ..Default::default()
        }
    }
    
    /// Check if event matches filter
    pub fn matches(&self, event: &PeatEvent) -> bool {
        // IDs filter
        if let Some(ids) = &self.ids {
            if !ids.contains(&event.id) {
                return false;
            }
        }
        
        // Authors filter
        if let Some(authors) = &self.authors {
            if !authors.contains(&event.pubkey) {
                return false;
            }
        }
        
        // Kinds filter
        if let Some(kinds) = &self.kinds {
            if !kinds.contains(&event.kind) {
                return false;
            }
        }
        
        // Time range
        if let Some(since) = self.since {
            if event.created_at < since {
                return false;
            }
        }
        if let Some(until) = self.until {
            if event.created_at > until {
                return false;
            }
        }
        
        // Tag filters (#e, #p, #h)
        if let Some(e_tags) = &self.e_tags {
            if !event.has_tag_value("e", e_tags) {
                return false;
            }
        }
        if let Some(p_tags) = &self.p_tags {
            if !event.has_tag_value("p", p_tags) {
                return false;
            }
        }
        if let Some(h_tags) = &self.h_tags {
            if !event.has_tag_value("h", h_tags) {
                return false;
            }
        }
        
        true
    }
}
```

---

## Architectural Comparison

### Nostr vs PEAT: Where They Diverge

| Aspect | Nostr | PEAT |
|--------|-------|------|
| **Problem Domain** | Censorship-resistant social | Real-time tactical coordination |
| **Network Assumption** | Cloud relays always available | No cloud, contested comms |
| **Consistency Model** | Eventually consistent, last-write-wins | CRDT causal consistency |
| **Data Flow** | Flat (any client → any relay) | Hierarchical (squad→platoon→company) |
| **Aggregation** | None (raw events) | Capability aggregation per echelon |
| **Authority Model** | Relay decides what to store | Echelon node aggregates and forwards |
| **Transport** | WebSocket over TCP | Multi-transport (BLE, LoRa, TAK, QUIC) |
| **Scale Strategy** | Add more relays | Hierarchical decomposition |
| **Sync Protocol** | REQ/EVENT + Negentropy | Automerge + Negentropy hybrid |

### Complementary, Not Competing

The architectures address different constraints:

```
                    Cloud Available
                          │
            ┌─────────────┴─────────────┐
            │                           │
      High Bandwidth              Low Bandwidth
            │                           │
      ┌─────┴─────┐               ┌─────┴─────┐
      │           │               │           │
   Nostr      Federation      PEAT-Full   PEAT-Lite
   Relays     (Mastodon)      (Platoon+)  (Wearables)
```

PEAT's hierarchical aggregation is what Nostr would need if relays disappeared and nodes had to form their own coordination structure.

---

## Implementation Strategy

### Phase 1: Event Format Adoption (Immediate)

1. Update `peat-core` event structures to Nostr compatibility
2. Implement kind registry with PEAT-specific ranges
3. Add `h` tag parsing and scope filtering
4. Validate events with Schnorr signatures

### Phase 2: Negentropy Integration (Q1 2026)

1. Integrate `rust-negentropy` crate
2. Implement `NegentropySync` layer
3. Add to `PeatSyncEngine` as discovery phase
4. Benchmark vs pure Automerge sync

### Phase 3: Filter Optimization (Q2 2026)

1. Implement indexed tag storage
2. Optimize filter matching for common patterns
3. Add query planning for complex filters
4. Profile and tune for resource-constrained devices

### Phase 4: Interoperability Testing (Q3 2026)

1. Test Nostr client connectivity (read-only)
2. Evaluate hybrid Nostr/PEAT scenarios
3. Document interop limitations
4. Consider Nostr relay mode for PEAT nodes

---

## Consequences

### Positive

1. **Proven Primitives**: Event structure, signing, and filtering are battle-tested
2. **Ecosystem Leverage**: Nostr tooling (libraries, debugging) partially applicable
3. **Efficient Sync**: Negentropy provides O(log n) reconciliation for sparse changes
4. **Extensibility**: Kind-based schema evolution without protocol changes
5. **Interoperability Path**: Future Nostr client compatibility possible

### Negative

1. **Signature Algorithm**: Schnorr on secp256k1 differs from some military PKI (Ed25519)
2. **Timestamp Dependency**: Negentropy requires ordered timestamps (GPS/NTP assumptions)
3. **Dual Sync Complexity**: Running both Negentropy and Automerge adds implementation burden
4. **Not Fully CRDT**: Event discovery doesn't provide CRDT merge semantics

### Risks

1. **Nostr Protocol Drift**: NIPs may evolve incompatibly
2. **Fingerprint Collisions**: 16-byte fingerprints have theoretical collision risk at scale
3. **Scope Explosion**: Deep hierarchies may create tag proliferation

---

## References

### Nostr Resources

- [Nostr Protocol](https://nostr.com/the-protocol)
- [NIPs Repository](https://github.com/nostr-protocol/nips)
- [NIP-01: Basic Protocol](https://github.com/nostr-protocol/nips/blob/master/01.md)
- [NIP-29: Relay-Based Groups](https://github.com/nostr-protocol/nips/blob/master/29.md)
- [NIP-77: Negentropy Syncing](https://nips.nostr.com/77)

### Analysis

- [Nature's Many Attempts to Evolve a Nostr](https://newsletter.squishy.computer/p/natures-many-attempts-to-evolve-a) - Gordon Brander
- [Range-Based Set Reconciliation](https://logperiodic.com/rbsr.html) - Log Periodic

### Implementations

- [Negentropy Reference](https://github.com/hoytech/negentropy) - C++, JS, Rust
- [rust-negentropy](https://github.com/yukibtc/rust-negentropy) - Rust port
- [strfry](https://github.com/hoytech/strfry) - High-performance Nostr relay with Negentropy

### Related ADRs

- ADR-007: Automerge-Based Sync Engine
- ADR-012: Schema Definition and Protocol Extensibility
- ADR-021: Document-Oriented Architecture
- ADR-027: Event Routing and Aggregation Protocol
- ADR-034: Record Deletion and Tombstone Management
- ADR-035: PEAT-Lite Embedded Nodes

---

## Appendix A: Event Serialization

Per Nostr specification, event ID is computed from UTF-8 JSON serialization:

```rust
/// Canonical event serialization for ID computation
fn serialize_for_id(event: &PeatEvent) -> String {
    // Array format: [0, pubkey, created_at, kind, tags, content]
    let arr = json!([
        0,
        hex::encode(&event.pubkey),
        event.created_at,
        event.kind,
        event.tags,
        event.content
    ]);
    
    // Minified JSON with specific ordering
    serde_json::to_string(&arr).unwrap()
}

/// Compute event ID
fn compute_event_id(event: &PeatEvent) -> [u8; 32] {
    let serialized = serialize_for_id(event);
    sha256(serialized.as_bytes())
}
```

---

## Appendix B: Negentropy Message Format

```
Message := <protocolVersion (0x61)> <Range>*

Range := <upperBound (Bound)> <mode (Varint)> <payload>

Bound := <encodedTimestamp (Varint)> <length (Varint)> <idPrefix (Byte)>*

Payload modes:
  0 = Skip (empty)
  1 = Fingerprint (16 bytes)
  2 = IdList (varint count + ids)
```

Fingerprint computation:
1. Sum all 32-byte IDs mod 2^256 (little-endian)
2. Append element count as varint
3. SHA-256 hash
4. Take first 16 bytes

---

## Appendix C: PEAT vs Nostr Tag Mapping

| Nostr Tag | PEAT Usage | Example |
|-----------|------------|---------|
| `e` | Reference to event | `["e", "<event-id>"]` |
| `p` | Reference to pubkey/node | `["p", "<node-pubkey>"]` |
| `h` | Hierarchical scope | `["h", "squad/alpha-1"]` |
| `a` | Addressable event ref | `["a", "10002:<pubkey>:scope"]` |
| `d` | Unique identifier (replaceable) | `["d", "capability-summary"]` |
| `previous` | Causal reference | `["previous", "<id-prefix>", ...]` |
| `expiration` | TTL timestamp | `["expiration", "1702656000"]` |
| `echelon` | PEAT-specific: echelon level | `["echelon", "squad"]` |
| `capability` | PEAT-specific: capability type | `["capability", "isr", "sigint"]` |

---

## Appendix D: Community Reception Analysis

The [Hacker News discussion](https://news.ycombinator.com/item?id=46225803) (December 2025) of Brander's article surfaced several critiques that validate PEAT's architectural choices.

### Key Critiques and Responses

| Critique | Nostr Response | PEAT Response |
|----------|----------------|---------------|
| Centralization inevitable | Accept it, users choose relays | Structure it via designed hierarchy |
| "Relays don't actually relay" | Clients push to multiple relays | Echelon nodes actively aggregate and forward |
| Key management too hard for users | User's problem | Operational provisioning with HSM support |
| No economic incentives for operators | Zaps/Lightning payments | Mission requirement, not economic choice |
| Content moderation unsolved | User-side filtering | Hierarchical scope boundaries (OPSEC) |
| Discovery unreliable | Outbox model | Capability advertisement (ADR-018) |
| DHT would be better | Not applicable to relay model | Iroh uses DHT for discovery + QUIC streams |

### Notable Comments

**On relays not relaying** (treyd):
> "If you and another party aren't using the same relay, there is 0 way for you to interact... The protocol explicitly forbids relays from forwarding to each other."

*PEAT insight*: This validates active coordination. Nostr punts message flow to clients; PEAT explicitly defines information flow through echelon nodes.

**On inevitable centralization** (treyd):
> "Email is currently more decentralized than Nostr is in practice."

*PEAT insight*: Networks centralize due to physics. The question is whether centralization is emergent/unplanned (Nostr) or designed/accountable (PEAT hierarchy).

**On protocol simplicity vs robustness** (treyd):
> "Nostr is a very simple protocol that could have been invented in essence in 1995. There's a reason it wasn't invented until recently, because it's difficult to build *robust* protocols with good guarantees about discoverability and reliability with a foundation that is as limited as it is."

*PEAT insight*: This captures the fundamental tradeoff. Nostr optimizes for simplicity in connected environments. PEAT accepts complexity cost to provide robustness guarantees in contested environments.

**On key management** (rglullis):
> "Nostr will always be a fringe network. The normies do not want to manage their own keys... What happens if you lose the cryptographic key to your nostr account? Who do you call for help?"

*PEAT insight*: Less relevant for military/industrial use cases where devices have HSMs, key provisioning is part of deployment, and recovery involves issuing new credentials through existing C2. Trained operators, not "normies."

**On moderation as sewage filtering** (bflesch):
> "Their statement underlines the fact that nostr is a stream of dirty sewage and they want users to submit their valuable user-created content into this sewage."

*PEAT insight*: Military hierarchies *are* moderation systems. Classification levels, need-to-know, commander's authority—content moderation by another name. Hierarchical scoping provides natural boundaries.

**On incentives** (wmf):
> "I'm also not aware of any incentives for the relay operators either."

*PEAT insight*: Military systems don't need economic incentives—they have mission requirements. The squad leader runs the squad node because that's their job, not because they get paid per message.

### The Outbox Model Defense

Nostr advocates (shark_laser) cite the "outbox model" as solving discovery:
> "You post to your own preferred relays, as well as to the preferred relays of others who are involved in the conversation, as well as to a couple of global relays for easy discoverability."

This parallels PEAT's capability advertisement (ADR-018)—nodes announce authoritative data sources. The difference: PEAT aggregates at each echelon, so consumers query the squad node rather than every squad member.

### Strategic Positioning

| Dimension | Nostr | PEAT |
|-----------|-------|------|
| Optimization target | Simplicity | Robustness |
| Environment assumption | Cloud available | Contested, disconnected |
| Centralization stance | Emergent, user-chosen | Designed, command-aligned |
| Target user | Consumer (key management is user problem) | Trained operator (provisioned credentials) |
| Moderation model | User filtering | Hierarchical scope (OPSEC) |
| Incentive model | Economic (Zaps) | Mission requirement |

**Summary**: Nostr and PEAT solve different problems. Nostr provides censorship resistance for social media in connected environments. PEAT provides coordination robustness for human-machine teams in contested environments. The HN critiques of Nostr largely don't apply to PEAT's domain, while validating PEAT's choice to accept architectural complexity for operational guarantees.

---

## Revision History

| Date | Version | Author | Changes |
|------|---------|--------|---------|
| 2025-12-15 | 0.1 | Kit Plummer, Claude | Initial draft |
| 2025-12-15 | 0.2 | Kit Plummer, Claude | Added Appendix D: HN community analysis |
