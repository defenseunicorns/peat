# ADR-046: Targeted Message Delivery

**Status**: Proposed
**Date**: 2025-01-09
**Authors**: Kit Plummer, Claude
**Relates to**: ADR-007 (Sync Engine), ADR-016 (TTL/Lifecycle), ADR-042 (UDP Bypass), ADR-045 (Zarf Integration)

## Context

### Current Model: Broadcast Replication

HIVE's CRDT synchronization replicates documents to **all nodes** in a cell. When a document is written to a collection, every node eventually receives and persists a copy.

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

## Decision

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
    /// Specific node IDs that should persist this document
    /// None = broadcast to all (current behavior)
    pub target_nodes: Option<Vec<NodeId>>,

    /// Label selector for targeting (e.g., "platform=vehicle,region=grid7")
    /// Evaluated at each node against its own labels
    pub target_selector: Option<String>,

    /// Behavior for non-target nodes
    pub transit_behavior: TransitBehavior,
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
// Example 1: Deploy to specific nodes
store.write_with_options(
    "deployment_intents",
    &intent,
    WriteOptions {
        target_nodes: Some(vec!["vehicle-1".into(), "vehicle-2".into()]),
        transit_behavior: TransitBehavior::RelayOnly,
        ttl: Some(Duration::from_secs(300)),
        ..Default::default()
    },
).await?;

// Example 2: Deploy to all vehicles in a region
store.write_with_options(
    "deployment_intents",
    &intent,
    WriteOptions {
        target_selector: Some("platform=vehicle,region=grid7".into()),
        transit_behavior: TransitBehavior::RelayOnly,
        ttl: Some(Duration::from_secs(300)),
        ..Default::default()
    },
).await?;

// Example 3: Broadcast (current behavior, explicit)
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

// Example 4: Collection configured for inbox pattern
// Config: DeliveryMode::InboxPattern { node_id_position: 1 }
store.write("commands/vehicle-1", &command).await?;
// Only vehicle-1 persists, others relay
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
- [ ] Add targeting fields to `WriteOptions`
- [ ] Add `TransitBehavior` enum
- [ ] Extend document metadata schema
- [ ] Add `CollectionConfig` delivery mode

### Phase 2: Sync Layer Integration
- [ ] Implement `is_target()` check
- [ ] Implement `SyncDecision` logic
- [ ] Add relay-without-persist behavior
- [ ] Garbage collection for relayed docs

### Phase 3: Selector Support
- [ ] Implement `LabelSelector` parser
- [ ] Add node label configuration
- [ ] Selector matching in sync layer

### Phase 4: Implicit Confirmation
- [ ] Extend beacon schema with version/status fields
- [ ] Document confirmation patterns
- [ ] Add confirmation query helpers

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
