# Peat Protocol Specification: Synchronization Protocol

**Spec ID**: Peat-SPEC-002
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: Defense Unicorns

## Abstract

This document specifies the synchronization protocol for Peat. It defines CRDT semantics, conflict resolution, document lifecycle, and the Negentropy-based set reconciliation mechanism.

## Table of Contents

1. [Introduction](#1-introduction)
2. [Terminology](#2-terminology)
3. [CRDT Foundation](#3-crdt-foundation)
4. [Document Model](#4-document-model)
5. [Synchronization Protocol](#5-synchronization-protocol)
6. [Conflict Resolution](#6-conflict-resolution)
7. [Document Lifecycle](#7-document-lifecycle)
8. [Negentropy Set Reconciliation](#8-negentropy-set-reconciliation)
9. [Subscription Model](#9-subscription-model)
10. [Performance Considerations](#10-performance-considerations)
11. [Security Considerations](#11-security-considerations)

---

## 1. Introduction

### 1.1 Purpose

Peat's synchronization protocol ensures that all nodes in a cell eventually converge to the same state, even when operating offline or with intermittent connectivity. It builds on Conflict-free Replicated Data Types (CRDTs) to achieve automatic conflict resolution.

### 1.2 Design Goals

- **Eventual Consistency**: All nodes converge to identical state
- **Offline-First**: Operations succeed locally, sync when possible
- **Automatic Merge**: No manual conflict resolution required
- **Causality Preservation**: Operations respect happened-before ordering

### 1.3 Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Document** | A JSON-like structure with automatic merge semantics |
| **Collection** | A named set of documents with shared configuration |
| **Operation** | An atomic change to a document |
| **Actor** | A unique identifier for an editing session |
| **Head** | The current state of a document (set of operation hashes) |
| **Change** | A sequence of operations bundled together |
| **Sync State** | Metadata tracking what changes have been exchanged |

---

## 3. CRDT Foundation

### 3.1 Automerge CRDT

Peat uses Automerge as its CRDT implementation. Automerge provides:

- **JSON-like documents**: Nested maps, lists, and primitives
- **Causal ordering**: Operations include dependency information
- **Efficient sync**: Only new changes are transmitted
- **Deterministic merge**: Same inputs produce same output

### 3.2 Supported Data Types

| Type | CRDT Type | Semantics |
|------|-----------|-----------|
| `Map` | LWW-Map | Last-writer-wins per key |
| `List` | RGA | Maintains insertion order |
| `Text` | Peritext | Character-level granularity |
| `Counter` | PN-Counter | Increment/decrement without conflict |
| `Primitive` | Register | Boolean, number, string, null |

### 3.3 Operation Types

```rust
pub enum Operation {
    /// Set a value in a map
    Put { key: String, value: Value },
    /// Delete a key from a map
    Delete { key: String },
    /// Insert into a list at index
    Insert { index: usize, value: Value },
    /// Remove from list at index
    Remove { index: usize },
    /// Increment a counter
    Increment { amount: i64 },
    /// Set the root of a document
    SetRoot { value: Value },
}
```

---

## 4. Document Model

### 4.1 Document Structure

Each document has:
- **Document ID**: UUID v4 (16 bytes)
- **Collection**: String name (e.g., "tracks", "missions")
- **Schema Type**: Protocol buffer message type
- **Content**: Automerge document bytes
- **Metadata**: Timestamps, author, version

### 4.2 Document ID Generation

```
DocumentId = UUID v4 (random)
```

Document IDs MUST be globally unique. Implementations SHOULD use a cryptographically secure random number generator.

### 4.3 Document Metadata

```rust
pub struct DocumentMetadata {
    /// Document identifier
    pub id: DocumentId,
    /// Collection name
    pub collection: String,
    /// Schema type (protobuf message name)
    pub schema_type: String,
    /// Creation timestamp (milliseconds since epoch)
    pub created_at: u64,
    /// Last modification timestamp
    pub modified_at: u64,
    /// Actor ID of last modifier
    pub modified_by: ActorId,
    /// Current heads (change hashes)
    pub heads: Vec<ChangeHash>,
}
```

### 4.4 Actor Identity

An `ActorId` is a 128-bit identifier combining:
- Device ID (64 bits, truncated from full PeerId)
- Session counter (64 bits, monotonically increasing)

```
ActorId = DeviceId[0:8] || SessionCounter
```

---

## 5. Synchronization Protocol

### 5.1 Sync Flow

```
    Node A                              Node B
       │                                   │
       │-------- SyncRequest ------------->│
       │  (have: [heads], want: bloom)     │
       │                                   │
       │<------- SyncResponse -------------|
       │  (changes: [...])                 │
       │                                   │
       │-------- SyncRequest ------------->│
       │  (have: [new_heads], want: bloom) │
       │                                   │
       │<------- SyncComplete -------------|
       │  (synced: true)                   │
       │                                   │
```

### 5.2 SyncRequest Message

```protobuf
message SyncRequest {
    // Document ID
    bytes document_id = 1;
    // Our current heads
    repeated bytes have = 2;
    // Bloom filter of changes we have
    bytes bloom_filter = 3;
    // Maximum response size
    uint32 max_response_bytes = 4;
}
```

### 5.3 SyncResponse Message

```protobuf
message SyncResponse {
    // Document ID
    bytes document_id = 1;
    // Changes we think the requester needs
    repeated Change changes = 2;
    // Our current heads
    repeated bytes heads = 3;
    // Whether sync is complete
    bool synced = 4;
}

message Change {
    // Change hash (SHA-256)
    bytes hash = 1;
    // Parent change hashes (dependencies)
    repeated bytes deps = 2;
    // Actor who made this change
    bytes actor = 3;
    // Sequence number for this actor
    uint64 seq = 4;
    // Timestamp
    uint64 timestamp = 5;
    // Compressed operations
    bytes operations = 6;
}
```

### 5.4 Sync States

```
                    ┌──────────────┐
                    │   Unknown    │
                    └──────┬───────┘
                           │ receive heads
                           ▼
                    ┌──────────────┐
              ┌─────│   InSync     │─────┐
              │     └──────────────┘     │
              │             │             │
        detect │            │ receive     │
        change │            │ change      │
              │             ▼             │
              │     ┌──────────────┐     │
              └────>│  OutOfSync   │─────┘
                    └──────────────┘
                           │
                           │ sync complete
                           ▼
                    ┌──────────────┐
                    │   InSync     │
                    └──────────────┘
```

### 5.5 Sync Triggers

Synchronization SHOULD be triggered when:
1. A local change is made
2. A new peer connects
3. A peer announces new heads
4. A periodic sync interval expires (default: 5 seconds)

---

## 6. Conflict Resolution

### 6.1 Last-Writer-Wins (LWW)

For map properties, concurrent writes to the same key are resolved by:
1. Compare Lamport timestamps (logical clock)
2. If equal, compare actor IDs lexicographically

```rust
fn resolve_conflict(a: &Operation, b: &Operation) -> &Operation {
    if a.timestamp > b.timestamp {
        a
    } else if a.timestamp < b.timestamp {
        b
    } else {
        // Tie-break by actor ID
        if a.actor > b.actor { a } else { b }
    }
}
```

### 6.2 List Merge (RGA)

Concurrent list insertions at the same position are ordered by:
1. Insertion timestamp (Lamport)
2. Actor ID (lexicographic)

This ensures all nodes converge to the same list order.

### 6.3 Counter Semantics

Counters use positive-negative (PN) semantics:
- Increments and decrements are both preserved
- Final value = sum of all increments - sum of all decrements

### 6.4 Delete Semantics

Deleted map keys are tombstoned and MAY be garbage collected after:
- All peers have acknowledged the deletion
- A configurable retention period (default: 7 days)

---

## 7. Document Lifecycle

### 7.1 Creation

```rust
pub async fn create_document(
    collection: &str,
    schema_type: &str,
    initial_value: Value,
) -> Result<DocumentId, Error>;
```

1. Generate random UUID for document ID
2. Create actor ID from device ID + session counter
3. Initialize Automerge document with SetRoot operation
4. Store locally
5. Announce to connected peers

### 7.2 Updates

```rust
pub async fn update_document(
    doc_id: &DocumentId,
    operations: Vec<Operation>,
) -> Result<(), Error>;
```

1. Load current document state
2. Apply operations to create a new change
3. Assign Lamport timestamp
4. Store change
5. Trigger sync with peers

### 7.3 Deletion (Tombstone)

```rust
pub async fn delete_document(doc_id: &DocumentId) -> Result<(), Error>;
```

1. Mark document as deleted (tombstone)
2. Set deletion timestamp
3. Retain tombstone for garbage collection period
4. Sync tombstone to peers
5. Remove from active indexes

### 7.4 Garbage Collection

Tombstones and old changes MAY be collected when:
- All known peers have synced past the deletion
- Retention period has expired
- Storage pressure requires cleanup

---

## 8. Negentropy Set Reconciliation

### 8.1 Overview

For efficient sync of large collections, Peat uses Negentropy set reconciliation. This protocol efficiently computes set differences using range fingerprints.

### 8.2 Fingerprint Computation

```
Fingerprint = XOR(SHA256(item)[0:16] for item in range)
```

### 8.3 Reconciliation Protocol

```
    Node A                              Node B
       │                                   │
       │-------- Initiate ----------------->│
       │  (root fingerprint)               │
       │                                   │
       │<------- Diff Response ------------|
       │  (matching ranges, diff ranges)   │
       │                                   │
       │-------- Resolve ----------------->│
       │  (items in diff ranges)           │
       │                                   │
       │<------- Items --------------------|
       │  (missing items)                  │
       │                                   │
```

### 8.4 Message Format

```protobuf
message NegentropyMessage {
    oneof msg {
        NegentropyInit init = 1;
        NegentropyResponse response = 2;
        NegentropyFinalize finalize = 3;
    }
}

message NegentropyInit {
    // Collection name
    string collection = 1;
    // Root fingerprint
    bytes fingerprint = 2;
    // Item count
    uint64 count = 3;
}

message NegentropyResponse {
    // Ranges that match (skip)
    repeated Range matching = 1;
    // Ranges that differ (reconcile)
    repeated Range differing = 2;
}

message Range {
    bytes lower_bound = 1;
    bytes upper_bound = 2;
    bytes fingerprint = 3;
    uint64 count = 4;
}
```

---

## 9. Subscription Model

### 9.1 Collection Subscriptions

```rust
pub async fn subscribe(
    collection: &str,
    query: Option<Query>,
) -> Result<Subscription, Error>;
```

Subscriptions provide:
- Real-time updates for matching documents
- Initial snapshot of current state
- Automatic reconnection on network failure

### 9.2 Query-Based Subscriptions

```rust
pub struct Query {
    /// Filter expression (DQL)
    pub filter: Option<String>,
    /// Geospatial bounding box
    pub bbox: Option<BoundingBox>,
    /// Maximum results
    pub limit: Option<usize>,
    /// Include deleted documents
    pub include_deleted: bool,
}
```

### 9.3 Subscription Events

```rust
pub enum SubscriptionEvent {
    /// Initial sync complete, snapshot provided
    Snapshot(Vec<Document>),
    /// Document created or updated
    Update(Document),
    /// Document deleted
    Delete(DocumentId),
    /// Subscription error
    Error(Error),
}
```

---

## 10. Performance Considerations

### 10.1 Change Bundling

Multiple operations SHOULD be bundled into a single change to reduce sync overhead. Recommended bundling strategies:
- Time-based: Flush every 100ms
- Size-based: Flush every 1KB of operations
- Event-based: Flush on user action boundary

### 10.2 Compression

Changes SHOULD be compressed using zstd before transmission. Compression level 3 is RECOMMENDED for a balance of speed and ratio.

### 10.3 Incremental Loading

Large documents SHOULD support incremental loading:
- Load metadata first (< 1KB)
- Load content on demand
- Cache frequently accessed documents

### 10.4 Sync Priority

Sync priority SHOULD be assigned based on:
1. User-visible documents (highest)
2. Documents in active queries
3. Recently modified documents
4. Background prefetch (lowest)

---

## 11. Security Considerations

### 11.1 Operation Signing

When E2E encryption is enabled (see Peat-SPEC-005), operations SHOULD be signed:

```rust
pub struct SignedChange {
    /// The change content
    pub change: Change,
    /// Author's device ID
    pub author: DeviceId,
    /// Ed25519 signature over change hash
    pub signature: [u8; 64],
    /// Nonce for replay protection
    pub nonce: [u8; 16],
}
```

### 11.2 Malicious Operations

Implementations MUST validate:
- Operation signatures match claimed author
- Lamport timestamps are monotonically increasing per actor
- Document IDs exist before accepting updates
- Operations conform to schema constraints

### 11.3 Storage Encryption

Documents at rest SHOULD be encrypted. See Peat-SPEC-005 for key management.

---

## Appendix A: References

- Automerge Paper: "A Conflict-Free Replicated JSON Datatype"
- Negentropy: "Efficient Set Reconciliation Protocol"
- ADR-005: DataSync Abstraction Layer
- ADR-007: Automerge-based Sync Engine
- ADR-011: Ditto vs Automerge/Iroh

## Appendix B: Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-07 | Initial draft |
