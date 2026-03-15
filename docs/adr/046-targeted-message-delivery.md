# ADR-046: Targeted Message Delivery

**Status**: Proposed
**Date**: 2025-01-09
**Authors**: Kit Plummer, Codex
**Relates to**: ADR-007 (Sync Engine), ADR-016 (TTL/Lifecycle), ADR-042 (UDP Bypass), ADR-045 (Zarf Integration)

## Context

### Current Model: Broadcast Replication

Peat's CRDT synchronization replicates documents to **all nodes** in a cell. When a document is written to a collection, every node eventually receives and persists a copy.

```
Writer → Node A → Node B → Node C → Node D
           ✓        ✓        ✓        ✓
         persist  persist  persist  persist
```

This works well for shared state (positions, capabilities, status) but is inefficient for:

- **Targeted commands**: "Deploy v2.3 to vehicles 1-5 only"
- **Direct messages**: Command to a specific platform
- **Large artifacts**: Binary payloads that only some nodes need
- **Resource-constrained nodes**: Devices that can't store everything

### The Problem

Without targeted delivery:

1. **Storage waste**: Every node persists documents they don't need
2. **Bandwidth waste**: Full replication of targeted content
3. **Privacy leak**: All nodes see all messages (even if encrypted, metadata visible)
4. **No delivery semantics**: Sender can't know if specific target received

### Prior Art: COD (Container Optimized Distribution)

Previous implementations used **collection-per-recipient** (inbox pattern):

```
inbox/node-1/   → documents for node-1
inbox/node-2/   → documents for node-2
```

Nodes subscribe only to their inbox. This works but:
- Requires sender to know collection naming convention
- Doesn't support selector-based targeting
- Collection proliferation at scale

### Requirements

1. **Targeted persistence**: Only recipients persist the document
2. **Relay capability**: Non-targets forward toward targets (mesh routing)
3. **Implicit confirmation**: Leverage existing beacon/capability updates
4. **Configuration**: Per-collection defaults, per-write overrides
5. **Resource efficiency**: Don't burden constrained devices with irrelevant data

### The Naming Problem

Targeting by `PeerId` (cryptographic identity) is stable but opaque. Autonomy algorithms and operators need human-readable names:

```
Algorithm says: "flanker-1 go left, flanker-2 go right"
But which PeerId is flanker-1?
```

**Challenges with static naming (learned from COD):**
- Names hard-coded in autonomy software → tight coupling to specific devices
- Device replacement requires config changes
- No runtime rebinding of roles

**Requirements for naming:**
1. **Alias registry**: Human-readable names → PeerId mapping
2. **APP_ID scoping**: Different apps can use same alias names independently
3. **Dynamic binding**: Roles can be rebound at runtime
4. **Query and subscribe**: Both pull (resolve) and push (watch) access patterns
5. **Beacon integration**: Nodes advertise their aliases

## Decision

### 0. Naming and Identity

#### Identity Layers

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: Aliases (human-readable, app-scoped)               │
│   "zarf.deployer-1", "swarm.flanker-1", "c2.viper-3"       │
└─────────────────────────────────────────────────────────────┘
                            │ resolves to
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 2: Labels (key-value metadata)                        │
│   { platform: "vehicle", role: "flanker", grid: "7" }      │
└─────────────────────────────────────────────────────────────┘
                            │ attached to
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: PeerId (cryptographic, immutable)                  │
│   SHA256(Ed25519 public key) = "abc123..."                  │
└─────────────────────────────────────────────────────────────┘
```

#### Alias Namespace Convention

Aliases are scoped by APP_ID using dot notation:

```
{app_id}.{alias}
```

Examples:
- `zarf.deployer-1` - Zarf app's deployer role
- `swarm.flanker-1` - Swarm autonomy's flanker role
- `swarm.flanker-2` - Same app, different role
- `c2.viper-3` - C2 app's callsign

**Rules:**
- APP_ID: lowercase alphanumeric + hyphens, max 32 chars
- Alias: lowercase alphanumeric + hyphens, max 64 chars
- Full name: `{app_id}.{alias}`, max 97 chars
- Reserved APP_IDs: `peat`, `system`

#### Alias Registry Collection

```protobuf
// Collection: _aliases/{app_id}.{alias}
message AliasBinding {
  string app_id = 1;           // Namespace
  string alias = 2;            // Human-readable name
  string peer_id = 3;          // Resolved identity
  AliasType type = 4;          // How this binding was created
  string bound_by = 5;         // PeerId of binder
  google.protobuf.Timestamp bound_at = 6;
  google.protobuf.Duration ttl = 7;  // Optional expiry
}

enum AliasType {
  ROLE = 0;       // Assigned role (flanker-1)
  CALLSIGN = 1;   // Operator-assigned (viper-3)
  SLOT = 2;       // Formation position
  CLAIMED = 3;    // Self-claimed by node
}
```

#### Binding Authority (per APP_ID)

```protobuf
// Collection: _alias_config/{app_id}
message AliasAppConfig {
  string app_id = 1;
  BindAuthority authority = 2;
  ConflictPolicy conflict_policy = 3;
  google.protobuf.Duration default_ttl = 4;
}

enum BindAuthority {
  LEADER_ONLY = 0;    // Only cell leader can bind
  OPEN = 1;           // Any node can bind/claim
  ROLES = 2;          // Specific roles can bind (see allowed_roles)
  CONSENSUS = 3;      // Requires threshold agreement
}

enum ConflictPolicy {
  REJECT_DUPLICATE = 0;  // First binding wins
  LAST_WRITE_WINS = 1;   // Latest binding wins
  LEADER_RESOLVES = 2;   // Leader adjudicates conflicts
}
```

#### Name Resolution API

```rust
/// Resolve human-readable names to PeerIds
pub trait NameResolver: Send + Sync {
    /// Resolve fully-qualified alias to PeerId
    /// e.g., "swarm.flanker-1" → PeerId
    async fn resolve(&self, alias: &str) -> Result<Option<PeerId>>;

    /// Resolve selector to matching PeerIds
    /// e.g., "role=flanker" → [PeerId, PeerId, ...]
    async fn resolve_selector(&self, selector: &str) -> Result<Vec<PeerId>>;

    /// Watch for alias binding changes
    async fn watch_alias(&self, alias: &str) -> Result<impl Stream<Item = AliasChange>>;

    /// Watch for nodes matching selector
    async fn watch_selector(&self, selector: &str) -> Result<impl Stream<Item = SelectorMatch>>;
}

/// Bind aliases to PeerIds
pub trait NameBinder: Send + Sync {
    /// Bind alias to peer (requires authority)
    async fn bind(
        &self,
        app_id: &str,
        alias: &str,
        peer_id: &PeerId,
        alias_type: AliasType,
    ) -> Result<()>;

    /// Unbind alias
    async fn unbind(&self, app_id: &str, alias: &str) -> Result<()>;

    /// Claim alias for self (if policy allows)
    async fn claim(&self, app_id: &str, alias: &str) -> Result<()>;

    /// Release own claim
    async fn release(&self, app_id: &str, alias: &str) -> Result<()>;
}
```

#### Beacon Integration

Nodes advertise their aliases in beacon:

```protobuf
message Beacon {
  // ... existing fields ...

  // Aliases this node is bound to (for discovery)
  repeated string aliases = 25;  // ["swarm.flanker-1", "c2.viper-3"]

  // Labels for selector matching
  map<string, string> labels = 26;
}
```

This enables:
- Alias discovery without querying registry
- Selector matching against beacon labels
- Implicit confirmation (alias appears in beacon = binding active)

### 1. Extend WriteOptions with Targeting

```rust
/// Options for write operations
pub struct WriteOptions {
    // === Existing fields ===
    /// Skip CRDT sync, use UDP bypass
    pub bypass_sync: bool,
    /// Time-to-live for the document
    pub ttl: Option<Duration>,
    /// Message priority for QoS
    pub priority: MessagePriority,

    // === New: Targeted delivery ===
    /// Target by alias (resolved to PeerId at write time)
    /// e.g., "swarm.flanker-1" or ["swarm.flanker-1", "swarm.flanker-2"]
    pub target_aliases: Option<Vec<String>>,

    /// Target by PeerId directly (for low-level use)
    /// None = broadcast to all (current behavior)
    pub target_nodes: Option<Vec<NodeId>>,

    /// Label selector for targeting (e.g., "platform=vehicle,region=grid7")
    /// Evaluated at each node against its own labels
    pub target_selector: Option<String>,

    /// Behavior for non-target nodes
    pub transit_behavior: TransitBehavior,
}

impl WriteOptions {
    /// Target a single alias
    pub fn to_alias(alias: impl Into<String>) -> Self {
        Self {
            target_aliases: Some(vec![alias.into()]),
            transit_behavior: TransitBehavior::RelayOnly,
            ..Default::default()
        }
    }

    /// Target multiple aliases
    pub fn to_aliases(aliases: Vec<String>) -> Self {
        Self {
            target_aliases: Some(aliases),
            transit_behavior: TransitBehavior::RelayOnly,
            ..Default::default()
        }
    }

    /// Target by selector
    pub fn to_selector(selector: impl Into<String>) -> Self {
        Self {
            target_selector: Some(selector.into()),
            transit_behavior: TransitBehavior::RelayOnly,
            ..Default::default()
        }
    }
}

/// How non-target nodes handle targeted documents
#[derive(Debug, Clone, Copy, Default)]
pub enum TransitBehavior {
    /// Relay and persist (standard CRDT behavior, default for broadcast)
    #[default]
    Persist,

    /// Relay toward targets, but don't persist locally
    /// Document held briefly for sync, then dropped
    RelayOnly,

    /// Don't relay - only source node has document
    /// Use for local-only writes
    SourceOnly,
}
```

### 2. Collection-Level Configuration

```rust
/// Collection configuration with delivery defaults
pub struct CollectionConfig {
    /// Collection name
    pub name: String,

    /// Default delivery mode for this collection
    pub delivery_mode: DeliveryMode,

    /// Default transit behavior for targeted documents
    pub default_transit_behavior: TransitBehavior,

    /// Default TTL for documents in this collection
    pub default_ttl: Option<Duration>,

    /// Sync mode (full, hierarchical, etc.)
    pub sync_mode: SyncMode,
}

/// How documents in this collection are addressed
#[derive(Debug, Clone, Default)]
pub enum DeliveryMode {
    /// Replicate to all nodes (current behavior)
    #[default]
    Broadcast,

    /// Require explicit target_nodes or target_selector in WriteOptions
    Targeted,

    /// Use a document field as the target address
    /// e.g., FieldAddressed("recipient_id") looks at doc.recipient_id
    FieldAddressed {
        field: String,
    },

    /// Inbox pattern: collection name includes recipient
    /// e.g., "commands/{node_id}"
    InboxPattern {
        node_id_position: usize,  // Path segment index
    },
}
```

### 3. Sync Layer Behavior

The sync layer checks targeting before persistence:

```rust
impl SyncEngine {
    /// Called when a document is received via sync
    async fn on_document_received(
        &self,
        doc: &Document,
        collection: &str,
        source_peer: &PeerId,
    ) -> SyncDecision {
        let config = self.get_collection_config(collection);
        let dominated = self.is_target(doc, config);

        // Determine persistence
        let should_persist = match config.delivery_mode {
            DeliveryMode::Broadcast => true,
            DeliveryMode::Targeted => self.is_target(doc, config),
            DeliveryMode::FieldAddressed { ref field } => {
                doc.get_field(field) == Some(&self.local_node_id)
            }
            DeliveryMode::InboxPattern { node_id_position } => {
                self.matches_inbox_pattern(collection, node_id_position)
            }
        };

        // Determine relay behavior
        let should_relay = match doc.transit_behavior() {
            TransitBehavior::Persist => true,
            TransitBehavior::RelayOnly => true,
            TransitBehavior::SourceOnly => false,
        };

        SyncDecision {
            persist: should_persist,
            relay: should_relay,
            notify_subscribers: should_persist,
        }
    }

    /// Check if local node is a target for this document
    fn is_target(&self, doc: &Document, config: &CollectionConfig) -> bool {
        // Check explicit node list
        if let Some(ref targets) = doc.target_nodes() {
            if targets.contains(&self.local_node_id) {
                return true;
            }
        }

        // Check selector against local labels
        if let Some(ref selector) = doc.target_selector() {
            if self.local_labels.matches(selector) {
                return true;
            }
        }

        // No targeting specified = broadcast (depends on collection config)
        doc.target_nodes().is_none() && doc.target_selector().is_none()
    }
}

/// Result of sync decision
struct SyncDecision {
    /// Store document in local database
    persist: bool,
    /// Forward to other peers
    relay: bool,
    /// Notify local subscribers
    notify_subscribers: bool,
}
```

### 4. Document Metadata

Targeting information stored in document metadata:

```rust
/// Document wrapper with targeting metadata
pub struct TargetedDocument<T> {
    /// The actual document content
    pub content: T,

    /// Document ID
    pub id: DocumentId,

    /// Target node IDs (None = broadcast)
    pub target_nodes: Option<Vec<NodeId>>,

    /// Target selector expression
    pub target_selector: Option<String>,

    /// Transit behavior for non-targets
    pub transit_behavior: TransitBehavior,

    /// TTL from creation time
    pub ttl: Option<Duration>,

    /// Creation timestamp
    pub created_at: Timestamp,

    /// Source node ID
    pub source_node: NodeId,
}
```

### 5. Implicit Delivery Confirmation

Rather than explicit ACKs, use existing beacon/capability updates:

```protobuf
message Beacon {
  // ... existing fields ...

  // Implicit delivery confirmation via state advertisement
  // When a node receives and processes a deployment, it updates its beacon

  // Installed software versions (confirms deployment receipt)
  map<string, string> installed_versions = 20;

  // Pending operations (shows in-progress state)
  repeated PendingOperation pending_operations = 21;

  // Last processed intent per collection (watermark)
  map<string, string> last_processed_intent = 22;
}

message PendingOperation {
  string intent_id = 1;
  string operation_type = 2;  // "deploy", "configure", etc.
  string state = 3;           // "downloading", "applying", "verifying"
  google.protobuf.Timestamp started_at = 4;
}
```

**Confirmation flow:**

```
1. Sender writes DeploymentIntent targeting Node-X
2. Intent syncs through mesh (RelayOnly for non-targets)
3. Node-X receives, persists, begins deployment
4. Node-X updates beacon: installed_versions["app"] = "2.3"
5. Beacon syncs back through hierarchy
6. Sender observes Node-X capability change = confirmation
```

**Benefits:**
- Zero additional message overhead
- Works with existing hierarchical aggregation
- Provides rich status, not just ACK/NAK
- Natural "who has what" queries

### 6. Selector Syntax

Simple label selector syntax (Kubernetes-inspired):

```
// Equality
platform=vehicle
region=grid7

// Set membership
platform in (vehicle, drone)
region notin (grid9)

// Existence
has(thermal_camera)
!has(deprecated_model)

// Compound (AND)
platform=vehicle,region=grid7,has(thermal_camera)
```

```rust
impl LabelSelector {
    pub fn matches(&self, labels: &HashMap<String, String>) -> bool;
    pub fn parse(selector: &str) -> Result<Self, SelectorError>;
}
```

### 7. API Usage Examples

```rust
// Example 1: Target by alias (recommended for human-readable addressing)
store.write_with_options(
    "deployment_intents",
    &intent,
    WriteOptions::to_alias("zarf.deployer-1"),
).await?;

// Example 2: Target multiple aliases
store.write_with_options(
    "swarm_commands",
    &formation_command,
    WriteOptions::to_aliases(vec![
        "swarm.flanker-1".into(),
        "swarm.flanker-2".into(),
    ]),
).await?;

// Example 3: Deploy to specific PeerIds (low-level)
store.write_with_options(
    "deployment_intents",
    &intent,
    WriteOptions {
        target_nodes: Some(vec!["abc123...".into(), "def456...".into()]),
        transit_behavior: TransitBehavior::RelayOnly,
        ttl: Some(Duration::from_secs(300)),
        ..Default::default()
    },
).await?;

// Example 4: Deploy to all vehicles in a region (selector)
store.write_with_options(
    "deployment_intents",
    &intent,
    WriteOptions::to_selector("platform=vehicle,region=grid7"),
).await?;

// Example 5: Broadcast (current behavior, explicit)
store.write_with_options(
    "positions",
    &position,
    WriteOptions {
        // No targeting = broadcast
        transit_behavior: TransitBehavior::Persist,
        ttl: Some(Duration::from_secs(30)),
        ..Default::default()
    },
).await?;

// Example 6: Collection configured for inbox pattern
// Config: DeliveryMode::InboxPattern { node_id_position: 1 }
store.write("commands/vehicle-1", &command).await?;
// Only vehicle-1 persists, others relay

// Example 7: Alias binding (leader binding flanker role)
let name_binder = peat.name_binder();
name_binder.bind("swarm", "flanker-1", &vehicle_peer_id, AliasType::ROLE).await?;

// Example 8: Self-claiming an alias
let name_binder = peat.name_binder();
name_binder.claim("c2", "viper-3").await?;  // Claims for local node

// Example 9: Resolving alias before send (manual)
let name_resolver = peat.name_resolver();
if let Some(peer_id) = name_resolver.resolve("swarm.flanker-1").await? {
    // Use peer_id directly
}

// Example 10: Watch for alias changes (reactive)
let mut alias_stream = name_resolver.watch_alias("swarm.flanker-1").await?;
while let Some(change) = alias_stream.next().await {
    match change {
        AliasChange::Bound { peer_id, .. } => println!("flanker-1 is now {}", peer_id),
        AliasChange::Unbound { .. } => println!("flanker-1 unbound"),
    }
}
```

### 8. Configuration Examples

```yaml
# Collection configurations
collections:
  # Broadcast collection (default behavior)
  positions:
    delivery_mode: broadcast
    default_ttl: 30s
    sync_mode: full

  # Targeted delivery collection
  deployment_intents:
    delivery_mode: targeted
    default_transit_behavior: relay_only
    default_ttl: 5m
    sync_mode: hierarchical

  # Field-addressed collection
  direct_commands:
    delivery_mode:
      field_addressed:
        field: recipient_node_id
    default_transit_behavior: relay_only
    default_ttl: 1m

  # Inbox pattern
  inbox:
    delivery_mode:
      inbox_pattern:
        node_id_position: 1  # inbox/{node_id}/...
    default_transit_behavior: relay_only
    default_ttl: 10m
```

## Consequences

### Positive

- **Storage efficiency**: Non-targets don't persist unnecessary documents
- **Bandwidth preservation**: Relay-only reduces redundant storage writes
- **Flexible addressing**: Node IDs, selectors, field-based, inbox patterns
- **No extra ACK overhead**: Implicit confirmation via beacon
- **Configurable**: Per-collection defaults with per-write overrides
- **Backward compatible**: Default behavior unchanged (broadcast + persist)

### Negative

- **Sync layer complexity**: Additional checks in hot path
- **Potential message loss**: If all paths to target fail, no retry (mitigated by TTL + resend)
- **Selector evaluation cost**: Each node evaluates selector locally
- **Debugging harder**: Documents not everywhere, need to trace routing

### Neutral

- **Not end-to-end encryption**: Targeting is routing, not privacy (use payload encryption for that)
- **Best-effort delivery**: CRDT sync is eventually consistent, not guaranteed delivery

## Implementation Plan

### Phase 1: Core Infrastructure
- [ ] Add targeting fields to `WriteOptions` (target_nodes, target_selector, target_aliases)
- [ ] Add `TransitBehavior` enum
- [ ] Extend document metadata schema
- [ ] Add `CollectionConfig` delivery mode

### Phase 2: Naming & Identity
- [ ] Define `AliasBinding` and `AliasAppConfig` protobuf schemas
- [ ] Create `_aliases/` and `_alias_config/` system collections
- [ ] Implement `NameResolver` trait and default implementation
- [ ] Implement `NameBinder` trait and default implementation
- [ ] Add alias fields to Beacon schema
- [ ] Alias resolution in WriteOptions (target_aliases → target_nodes)

### Phase 3: Sync Layer Integration
- [ ] Implement `is_target()` check
- [ ] Implement `SyncDecision` logic
- [ ] Add relay-without-persist behavior
- [ ] Garbage collection for relayed docs

### Phase 4: Selector Support
- [ ] Implement `LabelSelector` parser
- [ ] Add node label configuration
- [ ] Selector matching in sync layer
- [ ] Beacon labels for selector matching

### Phase 5: Implicit Confirmation
- [ ] Extend beacon schema with version/status fields
- [ ] Document confirmation patterns
- [ ] Add confirmation query helpers

### Phase 6: Integration & Testing
- [ ] End-to-end tests for alias-based targeting
- [ ] Integration tests with Zarf deployment flow (ADR-045)
- [ ] Performance benchmarks for selector evaluation
- [ ] Documentation and examples

## Alternatives Considered

### 1. Explicit ACK Messages

Dedicated acknowledgment messages from target to sender.

**Rejected**: Adds message overhead, doesn't leverage existing sync, requires additional reliability handling.

### 2. Collection-per-Recipient Only (Inbox Pattern)

Only support inbox pattern, no inline targeting.

**Rejected**: Works but inflexible. Sender must know naming convention. Doesn't support selectors.

### 3. Encryption-Only Privacy

Encrypt for target, let everyone replicate.

**Rejected**: Still wastes storage/bandwidth. Metadata visible. Doesn't address resource constraints.

### 4. Separate Routing Layer

Build routing on top of sync, not integrated.

**Rejected**: Duplicates effort. Better to extend existing configuration patterns.

## Security Considerations

- **Targeting is not access control**: Determines persistence, not visibility during transit
- **Payload encryption**: Use for sensitive content (separate concern)
- **Selector injection**: Validate selector syntax to prevent malformed queries
- **Spoofed targets**: Signed documents prevent target list tampering

## References

- ADR-007: Automerge-based Sync Engine
- ADR-016: TTL and Data Lifecycle Abstraction
- ADR-042: Direct-to-UDP Bypass Pathway
- ADR-045: Zarf/UDS Integration
- Kubernetes Label Selectors: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/
