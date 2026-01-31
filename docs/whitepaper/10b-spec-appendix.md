# HIVE Protocol Specification: Transport Layer

**Spec ID**: HIVE-SPEC-001
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: (r)evolve - Revolve Team LLC

## Abstract

This document specifies the transport layer for the HIVE Protocol. It defines wire formats, connection lifecycle, transport abstractions, and the UDP bypass channel for latency-critical applications.

## 1. Introduction

### 1.1 Purpose

The HIVE transport layer provides reliable and unreliable message delivery across heterogeneous network environments. It abstracts multiple physical transports (QUIC, UDP, BLE) behind a common interface, enabling applications to function regardless of underlying connectivity.

### 1.2 Scope

This specification covers:
- Transport trait abstraction
- QUIC-based primary transport (via Iroh)
- UDP bypass channel for low-latency data
- HIVE-Lite protocol for constrained devices
- BLE mesh transport for mobile/embedded devices
- Connection establishment and teardown
- Wire format encoding

### 1.3 Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Node** | A HIVE-capable device with a unique identity |
| **Peer** | A node with an established transport connection |
| **Endpoint** | A network address (IP:port, BLE address, etc.) |
| **Channel** | A logical stream within a transport connection |
| **Bypass** | Low-latency UDP path that skips CRDT synchronization |
| **Cell** | A group of nodes coordinating together |

---

## 3. Transport Abstraction

### 3.1 Transport Trait

All HIVE transports MUST implement the following interface:

```rust
pub trait Transport: Send + Sync {
    /// Send data to a specific peer
    async fn send(&self, peer: PeerId, data: &[u8]) -> Result<(), TransportError>;

    /// Receive data from any peer
    async fn recv(&self) -> Result<(PeerId, Vec<u8>), TransportError>;

    /// List currently connected peers
    fn peers(&self) -> Vec<PeerId>;

    /// Check if a specific peer is connected
    fn is_connected(&self, peer: &PeerId) -> bool;

    /// Close connection to a peer
    async fn disconnect(&self, peer: &PeerId) -> Result<(), TransportError>;

    /// Shutdown the transport
    async fn shutdown(&self) -> Result<(), TransportError>;
}
```

### 3.2 PeerId

A `PeerId` is a 32-byte identifier derived from a node's Ed25519 public key:

```
PeerId = SHA256(Ed25519PublicKey)
```

Implementations MUST use the first 32 bytes of the SHA-256 hash. The PeerId MUST be represented in hexadecimal when serialized to text.

### 3.3 Transport Error Codes

| Code | Name | Description |
|------|------|-------------|
| 0x01 | `ConnectionRefused` | Peer refused connection |
| 0x02 | `ConnectionClosed` | Connection was closed |
| 0x03 | `Timeout` | Operation timed out |
| 0x04 | `InvalidPeer` | Unknown or invalid peer |
| 0x05 | `SendFailed` | Message delivery failed |
| 0x06 | `ReceiveFailed` | Message receive failed |
| 0x07 | `AuthenticationFailed` | Peer authentication failed |
| 0x08 | `NotConnected` | No connection to peer |

---

## 4. Primary Transport: QUIC/Iroh

### 4.1 Overview

The primary HIVE transport uses QUIC via the Iroh library. QUIC provides:
- Multiplexed streams over a single connection
- Built-in encryption (TLS 1.3)
- Connection migration
- 0-RTT connection establishment (after initial handshake)

### 4.2 Connection Establishment

```
    Initiator                          Responder
        |                                   |
        |-------- QUIC ClientHello -------->|
        |                                   |
        |<------- QUIC ServerHello ---------|
        |                                   |
        |-------- HIVE Handshake ---------->|
        |  (DeviceId, FormationId, Nonce)   |
        |                                   |
        |<------- HIVE HandshakeAck --------|
        |  (DeviceId, Challenge)            |
        |                                   |
        |-------- ChallengeResponse ------->|
        |  (Signature)                      |
        |                                   |
        |<------- ConnectionReady ----------|
        |                                   |
```

### 4.3 HIVE Handshake Message

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Version    |     Type      |            Reserved          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                         Device ID (32 bytes)                  +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                       Formation ID (16 bytes)                 +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                          Nonce (32 bytes)                     +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                       Public Key (32 bytes)                   +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

**Fields**:
- **Version** (1 byte): Protocol version, currently `0x01`
- **Type** (1 byte): Message type (`0x01` = Handshake, `0x02` = HandshakeAck, `0x03` = ChallengeResponse)
- **Device ID** (32 bytes): SHA-256 hash of sender's public key
- **Formation ID** (16 bytes): UUID of the cell formation
- **Nonce** (32 bytes): Random bytes for challenge-response
- **Public Key** (32 bytes): Ed25519 public key

### 4.4 Stream Types

QUIC streams are used for different message categories:

| Stream ID Range | Purpose |
|-----------------|---------|
| 0 | Control messages (handshake, keepalive) |
| 1-15 | Reserved |
| 16-255 | CRDT sync (Automerge/Iroh) |
| 256-511 | Application data |
| 512+ | User-defined |

### 4.5 Keepalive

Nodes SHOULD send keepalive messages every 30 seconds on idle connections. Connections with no activity for 120 seconds MAY be closed.

```
Keepalive Message:
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Version (1)  | Type (0x10)   |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|         Timestamp (8 bytes)   |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

---

## 5. UDP Bypass Channel

### 5.1 Purpose

The UDP bypass channel provides a low-latency path for time-sensitive data that does not require CRDT conflict resolution. Examples include:
- Real-time sensor readings
- Heartbeat/position beacons
- Control commands

### 5.2 Configuration

Bypass is configured per-collection:

```rust
pub struct BypassCollectionConfig {
    /// Collection name (used for routing)
    pub name: String,
    /// Collection hash (4-byte identifier)
    pub collection_hash: u32,
    /// Transport type (unicast, broadcast, multicast)
    pub transport: BypassTransport,
    /// Priority level
    pub priority: MessagePriority,
    /// Time-to-live in milliseconds
    pub ttl_ms: u64,
    /// Maximum message size
    pub max_message_size: usize,
    /// Authentication mode
    pub auth_mode: BypassAuthMode,
}
```

### 5.3 Message Format

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Magic (2)  |  Version (1)  |    Flags (1)  |   Length (2)  |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Collection Hash (4 bytes)                 |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       Timestamp (8 bytes)                     |
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Sequence   |   Priority    |          TTL (2 bytes)        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                      Sender ID (32 bytes)                     +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                         Payload (variable)                    +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                 Optional: Signature (64 bytes)                +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

**Fields**:
- **Magic** (2 bytes): `0x48 0x56` ("HV")
- **Version** (1 byte): `0x01`
- **Flags** (1 byte):
  - Bit 0: Signed (signature present)
  - Bit 1: Encrypted
  - Bit 2-7: Reserved
- **Length** (2 bytes): Total message length including header
- **Collection Hash** (4 bytes): Identifies the collection
- **Timestamp** (8 bytes): Unix timestamp in milliseconds
- **Sequence** (1 byte): Wraparound sequence number
- **Priority** (1 byte): 0=Low, 1=Normal, 2=High, 3=Critical
- **TTL** (2 bytes): Time-to-live in milliseconds
- **Sender ID** (32 bytes): Sender's PeerId
- **Payload**: Variable-length data
- **Signature** (64 bytes, optional): Ed25519 signature over header + payload

### 5.4 Multicast

For broadcast scenarios, HIVE supports IP multicast:

| Multicast Group | Purpose |
|-----------------|---------|
| `239.255.72.86` | Default HIVE multicast group |
| `239.255.72.87` | Sensor data |
| `239.255.72.88` | Control commands |

Nodes MUST join multicast groups using IGMP. The default TTL for multicast packets is 16 hops.

### 5.5 Authentication Modes

| Mode | Overhead | Use Case |
|------|----------|----------|
| `None` | 0 bytes | Trusted LAN, performance critical |
| `Signed` | 64 bytes | Integrity verification |
| `SignedEncrypted` | 80+ bytes | Full confidentiality |

---

## 6. HIVE-Lite Protocol

### 6.1 Purpose

HIVE-Lite is a lightweight UDP protocol for resource-constrained devices (ESP32, ARM Cortex-M) that cannot run the full QUIC stack.

### 6.2 Message Format

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Magic (0xCAFE)              |  Version     |   Msg Type     |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|           Device ID (4 bytes, truncated)                      |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Sequence   |    Flags      |         Payload Length        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                       Payload (variable)                      +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       CRC-16 (2 bytes)                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

**Message Types**:
| Type | Name | Description |
|------|------|-------------|
| 0x01 | `Register` | Device registration |
| 0x02 | `RegisterAck` | Registration acknowledged |
| 0x03 | `Heartbeat` | Periodic presence announcement |
| 0x04 | `Data` | Application data |
| 0x05 | `Ack` | Data acknowledgment |
| 0x06 | `Nack` | Negative acknowledgment |

### 6.3 Reliability

HIVE-Lite uses a simple acknowledgment scheme:
- Sender transmits with sequence number (0-255, wraparound)
- Receiver sends Ack/Nack within timeout (default: 100ms)
- Sender retries up to 3 times before marking peer as failed

---

## 7. BLE Mesh Transport

### 7.1 Overview

The BLE mesh transport (`hive-btle`) enables HIVE communication over Bluetooth Low Energy for mobile and embedded devices.

### 7.2 GATT Service

| UUID | Name | Description |
|------|------|-------------|
| `0x1826` | HIVE Mesh Service | Primary service |
| `0x2A6E` | Data Characteristic | Read/Write/Notify |
| `0x2A6F` | Control Characteristic | Write only |

### 7.3 Advertising

HIVE nodes advertise with:
- Service UUID: `0x1826`
- Manufacturer Data: 4-byte Formation ID + 2-byte Mesh ID

### 7.4 MTU Considerations

BLE has limited MTU (typically 20-512 bytes). Messages larger than MTU MUST be fragmented:

```
Fragment Header (3 bytes):
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Fragment #   |  Total Frags  |   Flags   |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

---

## 8. Connection Lifecycle

### 8.1 States

```
                    ┌───────────────┐
                    │  Disconnected │
                    └───────┬───────┘
                            │ discover peer
                            ▼
                    ┌───────────────┐
                    │  Connecting   │
                    └───────┬───────┘
                            │ handshake complete
                            ▼
                    ┌───────────────┐
              ┌─────│   Connected   │─────┐
              │     └───────────────┘     │
              │             │             │
        error │             │ idle timeout│
              │             ▼             │
              │     ┌───────────────┐     │
              └────>│  Disconnected │<────┘
                    └───────────────┘
```

### 8.2 Reconnection

Upon disconnection, nodes SHOULD attempt reconnection with exponential backoff:
- Initial delay: 100ms
- Maximum delay: 30 seconds
- Jitter: +/- 10%

---

## 9. Wire Formats

### 9.1 Byte Order

All multi-byte integers are encoded in **big-endian** (network byte order).

### 9.2 Encoding

| Type | Encoding |
|------|----------|
| Timestamps | Unix epoch milliseconds (uint64) |
| UUIDs | Binary 16 bytes |
| Public Keys | Raw Ed25519 (32 bytes) |
| Signatures | Raw Ed25519 (64 bytes) |
| Hashes | SHA-256 (32 bytes) |

---

## 10. Security Considerations

### 10.1 Transport Security

- QUIC connections use TLS 1.3
- All QUIC traffic is encrypted by default
- Certificate validation uses Ed25519 device keys

### 10.2 Bypass Channel Security

- Unsigned bypass messages are vulnerable to spoofing
- Implementations SHOULD use `Signed` or `SignedEncrypted` mode for untrusted networks
- Multicast traffic SHOULD be encrypted with the cell's GroupKey

### 10.3 Denial of Service

- Rate limiting SHOULD be applied to incoming connections
- Malformed messages MUST be discarded silently
- Connection floods SHOULD trigger automatic blacklisting

---

## 11. IANA Considerations

### 11.1 Port Allocations

| Port | Protocol | Purpose |
|------|----------|---------|
| 4433 | UDP | QUIC/Iroh primary transport |
| 4434 | UDP | Bypass channel |
| 4435 | UDP | HIVE-Lite |

### 11.2 Multicast Groups

| Group | Purpose |
|-------|---------|
| 239.255.72.86-88 | HIVE protocol multicast |

---

## Appendix A: References

- RFC 9000: QUIC Transport Protocol
- RFC 2119: Key words for RFCs
- ADR-010: Transport Layer UDP/TCP
- ADR-032: Pluggable Transport Abstraction
- ADR-042/043: Transport Interfaces

## Appendix B: Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-07 | Initial draft |

---

# HIVE Protocol Specification: Synchronization Protocol

**Spec ID**: HIVE-SPEC-002
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: (r)evolve - Revolve Team LLC

## Abstract

This document specifies the synchronization protocol for HIVE. It defines CRDT semantics, conflict resolution, document lifecycle, and the Negentropy-based set reconciliation mechanism.

## 1. Introduction

### 1.1 Purpose

HIVE's synchronization protocol ensures that all nodes in a cell eventually converge to the same state, even when operating offline or with intermittent connectivity. It builds on Conflict-free Replicated Data Types (CRDTs) to achieve automatic conflict resolution.

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

HIVE uses Automerge as its CRDT implementation. Automerge provides:

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

For efficient sync of large collections, HIVE uses Negentropy set reconciliation. This protocol efficiently computes set differences using range fingerprints.

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

When E2E encryption is enabled (see HIVE-SPEC-005), operations SHOULD be signed:

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

Documents at rest SHOULD be encrypted. See HIVE-SPEC-005 for key management.

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

---

# HIVE Protocol Specification: Data Schema Definitions

**Spec ID**: HIVE-SPEC-003
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: (r)evolve - Revolve Team LLC

## Abstract

This document specifies the data schemas for HIVE Protocol. It defines the Protocol Buffer message formats for tactical entities, their relationships, and mapping to external standards (CoT/TAK).

## 1. Introduction

### 1.1 Purpose

HIVE schemas define the structure of all data exchanged between nodes. Using Protocol Buffers ensures:
- Compact binary encoding
- Forward/backward compatibility
- Cross-language support
- Schema validation

### 1.2 Design Principles

- **Standards Alignment**: Optional compatibility with tactical standards (CoT, STANAG 4586)
- **Extensibility**: Unknown fields are preserved
- **Efficiency**: Optimize for constrained networks
- **Interoperability**: Support external system integration

### 1.3 Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

---

## 2. Schema Organization

### 2.1 Package Structure

```
hive-schema/proto/
├── hive/
│   ├── common/
│   │   └── v1/
│   │       └── common.proto       # Common types (Position, Timestamp)
│   ├── beacon/
│   │   └── v1/
│   │       └── beacon.proto       # Track updates, node identity
│   ├── mission/
│   │   └── v1/
│   │       └── mission.proto      # Mission tasking, objectives
│   ├── capability/
│   │   └── v1/
│   │       └── capability.proto   # Capability advertisement
│   ├── security/
│   │   └── v1/
│   │       └── security.proto     # Auth, device identity
│   ├── ai/
│   │   └── v1/
│   │       └── ai.proto           # ML models, inference
│   └── cot/
│       └── v1/
│           └── cot.proto          # CoT/TAK interop
```

### 2.2 Versioning

Schema packages follow semantic versioning:
- **v1**: Initial stable release
- **v2**: Breaking changes (new package)
- Minor additions within a version are backward compatible

### 2.3 Reserved Field Ranges

| Range | Purpose |
|-------|---------|
| 1-99 | Core fields |
| 100-199 | Standard extensions |
| 200-299 | Organization-specific |
| 300-999 | Reserved for future |
| 1000+ | Application-defined |

---

## 3. Core Schemas

### 3.1 Common Types

```protobuf
syntax = "proto3";
package hive.common.v1;

// Geographic position in WGS84
message Position {
    // Latitude in degrees (-90 to 90)
    double latitude = 1;
    // Longitude in degrees (-180 to 180)
    double longitude = 2;
    // Altitude in meters above WGS84 ellipsoid
    optional double altitude = 3;
    // Horizontal accuracy in meters (CEP50)
    optional float horizontal_accuracy = 4;
    // Vertical accuracy in meters
    optional float vertical_accuracy = 5;
    // Heading in degrees (0-360, true north)
    optional float heading = 6;
    // Speed in meters per second
    optional float speed = 7;
}

// Timestamp with nanosecond precision
message Timestamp {
    // Seconds since Unix epoch
    int64 seconds = 1;
    // Nanoseconds (0-999999999)
    int32 nanos = 2;
}

// Universally unique identifier
message UUID {
    // 16-byte UUID value
    bytes value = 1;
}

// Human-readable identifier
message Callsign {
    // Short tactical name (e.g., "ALPHA-1")
    string value = 1;
}

// Geospatial bounding box
message BoundingBox {
    double min_latitude = 1;
    double max_latitude = 2;
    double min_longitude = 3;
    double max_longitude = 4;
}

// Time range
message TimeRange {
    Timestamp start = 1;
    Timestamp end = 2;
}
```

### 3.2 Entity Types

```protobuf
// Entity affiliation (member/external/neutral/unknown)
enum Affiliation {
    AFFILIATION_UNKNOWN = 0;
    AFFILIATION_MEMBER = 1;
    AFFILIATION_EXTERNAL = 2;
    AFFILIATION_NEUTRAL = 3;
    AFFILIATION_PENDING = 4;
}

// Entity dimension (land/air/sea/subsurface/space)
enum Dimension {
    DIMENSION_UNKNOWN = 0;
    DIMENSION_GROUND = 1;
    DIMENSION_AIR = 2;
    DIMENSION_SURFACE = 3;  // Sea surface
    DIMENSION_SUBSURFACE = 4;
    DIMENSION_SPACE = 5;
}

// Platform type
enum PlatformType {
    PLATFORM_UNKNOWN = 0;
    PLATFORM_GROUND_VEHICLE = 1;
    PLATFORM_PORTABLE = 2;
    PLATFORM_FIXED_WING = 3;
    PLATFORM_ROTARY_WING = 4;
    PLATFORM_UAV = 5;
    PLATFORM_UGV = 6;
    PLATFORM_USV = 7;
    PLATFORM_UUV = 8;
    PLATFORM_SENSOR = 9;
    PLATFORM_ACTUATOR = 10;
}
```

---

## 4. Beacon and Tracking

### 4.1 Beacon Message

The primary entity tracking message:

```protobuf
syntax = "proto3";
package hive.beacon.v1;

import "hive/common/v1/common.proto";

// Track update from a node
message Beacon {
    // Unique identifier for this track
    hive.common.v1.UUID track_id = 1;

    // Device that produced this beacon
    bytes device_id = 2;

    // Callsign for display
    hive.common.v1.Callsign callsign = 3;

    // Current position
    hive.common.v1.Position position = 4;

    // Timestamp of position fix
    hive.common.v1.Timestamp timestamp = 5;

    // Entity classification
    Affiliation affiliation = 6;
    Dimension dimension = 7;
    PlatformType platform = 8;

    // Operational status
    OperationalStatus status = 9;

    // Battery/power level (0-100)
    optional uint32 power_level = 10;

    // Time-to-live in seconds (0 = infinite)
    uint32 ttl_seconds = 11;

    // Confidence level (0.0 - 1.0)
    optional float confidence = 12;

    // Free-form remarks
    optional string remarks = 13;

    // Extended data (schema-specific)
    map<string, bytes> extensions = 100;
}

// Operational status
enum OperationalStatus {
    STATUS_UNKNOWN = 0;
    STATUS_OPERATIONAL = 1;
    STATUS_DEGRADED = 2;
    STATUS_INOPERATIVE = 3;
    STATUS_EMERGENCY = 4;
}

// Signed beacon for authenticated networks
message SignedBeacon {
    // The beacon content
    Beacon beacon = 1;
    // Ed25519 signature over beacon bytes
    bytes signature = 2;
    // Public key of signer
    bytes public_key = 3;
}
```

### 4.2 Track Aggregation

For hierarchical reporting:

```protobuf
// Aggregated track summary (sent upward in hierarchy)
message TrackSummary {
    // Cell producing this summary
    hive.common.v1.UUID cell_id = 1;

    // Time range covered
    hive.common.v1.TimeRange time_range = 2;

    // Bounding box containing all tracks
    hive.common.v1.BoundingBox coverage = 3;

    // Count by affiliation
    map<int32, uint32> affiliation_counts = 4;

    // Count by platform type
    map<int32, uint32> platform_counts = 5;

    // Selected high-priority tracks
    repeated Beacon priority_tracks = 6;
}
```

---

## 5. Mission and Tasking

### 5.1 Mission Message

```protobuf
syntax = "proto3";
package hive.mission.v1;

import "hive/common/v1/common.proto";

// Mission definition
message Mission {
    // Unique mission identifier
    hive.common.v1.UUID mission_id = 1;

    // Human-readable name
    string name = 2;

    // Mission type
    MissionType type = 3;

    // Issuing authority
    string issuing_authority = 4;

    // Priority level
    Priority priority = 5;

    // Time window
    hive.common.v1.TimeRange time_window = 6;

    // Area of operations
    AreaOfOperations aoo = 7;

    // Assigned cells/units
    repeated hive.common.v1.UUID assigned_cells = 8;

    // Objectives within this mission
    repeated Objective objectives = 9;

    // Current status
    MissionStatus status = 10;

    // Operational constraints reference
    optional string constraints_reference = 11;

    // Free-form instructions
    optional string instructions = 12;
}

enum MissionType {
    MISSION_TYPE_UNSPECIFIED = 0;
    MISSION_TYPE_OBSERVATION = 1;   // Observe, monitor, survey
    MISSION_TYPE_ACTION = 2;        // Perform coordinated action
    MISSION_TYPE_TRANSPORT = 3;     // Move payload or resources
    MISSION_TYPE_ESCORT = 4;        // Accompany and protect
    MISSION_TYPE_PATROL = 5;        // Monitor area over time
    MISSION_TYPE_SEARCH = 6;        // Search and locate
    MISSION_TYPE_RESUPPLY = 7;      // Deliver resources
}

enum Priority {
    PRIORITY_UNSPECIFIED = 0;
    PRIORITY_ROUTINE = 1;
    PRIORITY_PRIORITY = 2;
    PRIORITY_IMMEDIATE = 3;
    PRIORITY_FLASH = 4;
}

enum MissionStatus {
    MISSION_STATUS_UNSPECIFIED = 0;
    MISSION_STATUS_PLANNED = 1;
    MISSION_STATUS_ASSIGNED = 2;
    MISSION_STATUS_IN_PROGRESS = 3;
    MISSION_STATUS_COMPLETE = 4;
    MISSION_STATUS_ABORTED = 5;
}

// Geographic area of operations
message AreaOfOperations {
    oneof area {
        // Circular area
        CircularArea circle = 1;
        // Polygon area
        PolygonArea polygon = 2;
        // Route/corridor
        RouteArea route = 3;
    }
}

message CircularArea {
    hive.common.v1.Position center = 1;
    double radius_meters = 2;
}

message PolygonArea {
    repeated hive.common.v1.Position vertices = 1;
}

message RouteArea {
    repeated hive.common.v1.Position waypoints = 1;
    double corridor_width_meters = 2;
}
```

### 5.2 Objective

```protobuf
// Individual objective within a mission
message Objective {
    hive.common.v1.UUID objective_id = 1;
    string description = 2;
    ObjectiveType type = 3;
    hive.common.v1.Position location = 4;
    ObjectiveStatus status = 5;
    Priority priority = 6;
}

enum ObjectiveType {
    OBJECTIVE_TYPE_UNSPECIFIED = 0;
    OBJECTIVE_TYPE_OBSERVE = 1;
    OBJECTIVE_TYPE_IDENTIFY = 2;
    OBJECTIVE_TYPE_TRACK = 3;
    OBJECTIVE_TYPE_NEUTRALIZE = 4;
    OBJECTIVE_TYPE_SECURE = 5;
    OBJECTIVE_TYPE_DELIVER = 6;
}

enum ObjectiveStatus {
    OBJECTIVE_STATUS_UNSPECIFIED = 0;
    OBJECTIVE_STATUS_PENDING = 1;
    OBJECTIVE_STATUS_IN_PROGRESS = 2;
    OBJECTIVE_STATUS_COMPLETE = 3;
    OBJECTIVE_STATUS_FAILED = 4;
}
```

---

## 6. Capability Advertisement

### 6.1 Capability Message

```protobuf
syntax = "proto3";
package hive.capability.v1;

import "hive/common/v1/common.proto";

// Node capability advertisement
message CapabilityAdvertisement {
    // Device advertising capabilities
    bytes device_id = 1;

    // Callsign
    hive.common.v1.Callsign callsign = 2;

    // Platform type
    PlatformType platform = 3;

    // Sensor capabilities
    repeated SensorCapability sensors = 4;

    // Actuator capabilities
    repeated ActuatorCapability actuators = 5;

    // Communication capabilities
    CommunicationCapability comms = 6;

    // Compute capabilities
    ComputeCapability compute = 7;

    // Power/endurance
    PowerCapability power = 8;

    // Current availability
    Availability availability = 9;

    // Last update time
    hive.common.v1.Timestamp timestamp = 10;
}

// Sensor capability
message SensorCapability {
    string sensor_id = 1;
    SensorType type = 2;
    SensorSpec spec = 3;
    OperationalStatus status = 4;
}

enum SensorType {
    SENSOR_TYPE_UNSPECIFIED = 0;
    SENSOR_TYPE_EO = 1;         // Electro-optical
    SENSOR_TYPE_IR = 2;         // Infrared
    SENSOR_TYPE_RADAR = 3;
    SENSOR_TYPE_LIDAR = 4;
    SENSOR_TYPE_ACOUSTIC = 5;
    SENSOR_TYPE_RF = 6;         // Radio frequency
    SENSOR_TYPE_CBRN = 7;       // Chemical/Bio/Rad/Nuclear
    SENSOR_TYPE_GPS = 8;
    SENSOR_TYPE_IMU = 9;
}

message SensorSpec {
    // Range in meters
    optional double range_meters = 1;
    // Field of view in degrees
    optional double fov_degrees = 2;
    // Resolution (sensor-specific)
    optional string resolution = 3;
    // Update rate in Hz
    optional double update_rate_hz = 4;
}

// Actuator capability
message ActuatorCapability {
    string actuator_id = 1;
    ActuatorType type = 2;
    ActuatorSpec spec = 3;
    OperationalStatus status = 4;
}

enum ActuatorType {
    ACTUATOR_TYPE_UNSPECIFIED = 0;
    ACTUATOR_TYPE_PHYSICAL = 1;     // Physical actuation
    ACTUATOR_TYPE_SIGNAL = 2;       // Signal/RF emission
    ACTUATOR_TYPE_DIGITAL = 3;      // Digital/cyber action
    ACTUATOR_TYPE_CARGO = 4;        // Payload delivery
    ACTUATOR_TYPE_MANIPULATOR = 5;  // Robotic arm/gripper
}

message ActuatorSpec {
    // Range in meters
    optional double range_meters = 1;
    // Payload capacity in kg
    optional double payload_kg = 2;
    // Uses/resources remaining
    optional uint32 resources_remaining = 3;
}

// Communication capability
message CommunicationCapability {
    // Supported link types
    repeated LinkType links = 1;
    // Maximum data rate (bps)
    uint64 max_data_rate_bps = 2;
    // Current link quality (0-100)
    uint32 link_quality = 3;
}

enum LinkType {
    LINK_TYPE_UNSPECIFIED = 0;
    LINK_TYPE_MESH = 1;         // HIVE mesh
    LINK_TYPE_SATCOM = 2;
    LINK_TYPE_HF = 3;
    LINK_TYPE_VHF = 4;
    LINK_TYPE_UHF = 5;
    LINK_TYPE_LTE = 6;
    LINK_TYPE_WIFI = 7;
    LINK_TYPE_BLE = 8;
}

// Compute capability
message ComputeCapability {
    // TFLOPS available
    optional double compute_tflops = 1;
    // Memory in MB
    optional uint32 memory_mb = 2;
    // Storage in MB
    optional uint32 storage_mb = 3;
    // Supported AI models
    repeated string ai_models = 4;
}

// Power/endurance
message PowerCapability {
    // Battery percentage (0-100)
    uint32 battery_percent = 1;
    // Estimated time remaining (seconds)
    optional uint32 endurance_seconds = 2;
    // Power source type
    PowerSource source = 3;
}

enum PowerSource {
    POWER_SOURCE_UNSPECIFIED = 0;
    POWER_SOURCE_BATTERY = 1;
    POWER_SOURCE_FUEL = 2;
    POWER_SOURCE_SOLAR = 3;
    POWER_SOURCE_TETHERED = 4;
}

// Availability status
message Availability {
    bool available = 1;
    optional string reason = 2;
    optional hive.common.v1.Timestamp available_at = 3;
}
```

---

## 7. Security Schemas

### 7.1 Device Identity

```protobuf
syntax = "proto3";
package hive.security.v1;

import "hive/common/v1/common.proto";

// Device identity information
message DeviceIdentity {
    // Device ID (SHA-256 of public key)
    bytes device_id = 1;
    // Ed25519 public key
    bytes public_key = 2;
    // Device type
    DeviceType device_type = 3;
    // Optional display name
    optional string display_name = 4;
    // Certificate (if using X.509)
    optional bytes certificate = 5;
}

enum DeviceType {
    DEVICE_TYPE_UNSPECIFIED = 0;
    DEVICE_TYPE_SENSOR = 1;
    DEVICE_TYPE_EFFECTOR = 2;
    DEVICE_TYPE_RELAY = 3;
    DEVICE_TYPE_CONTROLLER = 4;
    DEVICE_TYPE_GATEWAY = 5;
}

// Challenge for authentication
message Challenge {
    bytes nonce = 1;
    hive.common.v1.Timestamp timestamp = 2;
    bytes challenger_id = 3;
}

// Response to challenge
message SignedChallengeResponse {
    bytes nonce = 1;
    bytes responder_id = 2;
    bytes public_key = 3;
    bytes signature = 4;
}

// Security error details
message SecurityError {
    SecurityErrorCode code = 1;
    string message = 2;
    optional bytes offending_device = 3;
}

enum SecurityErrorCode {
    SECURITY_ERROR_UNSPECIFIED = 0;
    SECURITY_ERROR_AUTHENTICATION_FAILED = 1;
    SECURITY_ERROR_AUTHORIZATION_DENIED = 2;
    SECURITY_ERROR_INVALID_SIGNATURE = 3;
    SECURITY_ERROR_EXPIRED_CHALLENGE = 4;
    SECURITY_ERROR_REPLAY_DETECTED = 5;
    SECURITY_ERROR_UNKNOWN_DEVICE = 6;
}
```

---

## 8. AI/ML Schemas

### 8.1 Model Metadata

```protobuf
syntax = "proto3";
package hive.ai.v1;

import "hive/common/v1/common.proto";

// AI model metadata
message ModelMetadata {
    // Unique model identifier
    string model_id = 1;
    // Human-readable name
    string name = 2;
    // Model version (semver)
    string version = 3;
    // Model type
    ModelType type = 4;
    // Input specification
    ModelInput input = 5;
    // Output specification
    ModelOutput output = 6;
    // Hardware requirements
    HardwareRequirements requirements = 7;
    // Model hash for verification
    bytes hash = 8;
    // Size in bytes
    uint64 size_bytes = 9;
}

enum ModelType {
    MODEL_TYPE_UNSPECIFIED = 0;
    MODEL_TYPE_DETECTION = 1;
    MODEL_TYPE_CLASSIFICATION = 2;
    MODEL_TYPE_SEGMENTATION = 3;
    MODEL_TYPE_TRACKING = 4;
    MODEL_TYPE_NLP = 5;
    MODEL_TYPE_ANOMALY = 6;
}

message ModelInput {
    string format = 1;        // e.g., "image/rgb", "tensor"
    repeated uint32 shape = 2; // e.g., [640, 640, 3]
    string dtype = 3;         // e.g., "float32"
}

message ModelOutput {
    string format = 1;
    repeated uint32 shape = 2;
    string dtype = 3;
    repeated string labels = 4;  // For classification
}

message HardwareRequirements {
    optional double min_tflops = 1;
    optional uint32 min_memory_mb = 2;
    repeated string accelerators = 3;  // e.g., ["cuda", "tensorrt"]
}

// Inference request
message InferenceRequest {
    string model_id = 1;
    bytes input_data = 2;
    optional string request_id = 3;
    optional uint32 timeout_ms = 4;
}

// Inference response
message InferenceResponse {
    string request_id = 1;
    bytes output_data = 2;
    float inference_time_ms = 3;
    optional string error = 4;
}

// Detection result (for object detection models)
message Detection {
    // Bounding box (normalized 0-1)
    float x_min = 1;
    float y_min = 2;
    float x_max = 3;
    float y_max = 4;
    // Class label
    string label = 5;
    // Confidence (0-1)
    float confidence = 6;
    // Track ID if tracking
    optional string track_id = 7;
}
```

---

## 9. CoT/TAK Mapping

### 9.1 CoT Event Mapping

HIVE beacons map to CoT events:

| HIVE Field | CoT Field | Notes |
|------------|-----------|-------|
| `track_id` | `uid` | UUID format |
| `position.latitude` | `point/@lat` | |
| `position.longitude` | `point/@lon` | |
| `position.altitude` | `point/@hae` | Height above ellipsoid |
| `timestamp` | `@time` | ISO 8601 |
| `callsign` | `contact/@callsign` | |
| `affiliation` | `@type` (prefix) | a-f, a-h, a-n, a-u |
| `dimension` | `@type` (atom) | G, A, S, U |
| `platform` | `detail/platform` | Extended |

### 9.2 CoT Type Mapping

```
HIVE Affiliation + Dimension → CoT Type

MEMBER + GROUND     → a-f-G
MEMBER + AIR        → a-f-A
EXTERNAL + GROUND   → a-h-G
NEUTRAL + SURFACE   → a-n-S
UNKNOWN + AIR       → a-u-A
```

### 9.3 CoT Detail Extensions

HIVE-specific data is carried in CoT `<detail>` elements:

```xml
<detail>
    <__hive>
        <device_id>0x1234...</device_id>
        <cell_id>uuid</cell_id>
        <power_level>85</power_level>
        <capabilities>sensor,relay</capabilities>
    </__hive>
</detail>
```

---

## 10. Schema Evolution

### 10.1 Backward Compatibility Rules

When evolving schemas:
1. New fields MUST use new field numbers
2. Existing field semantics MUST NOT change
3. Required fields MUST NOT be removed
4. Field types MUST NOT change

### 10.2 Deprecation Process

1. Mark field as deprecated in proto
2. Add to deprecation list in documentation
3. Support deprecated field for 2 major versions
4. Remove in subsequent version

### 10.3 Extension Points

All major messages include extension points:
- `map<string, bytes> extensions = 100;`
- Reserved field range 200-299 for organization-specific fields

---

## 11. Validation

### 11.1 Required Field Validation

Implementations MUST validate:
- UUIDs are valid 16-byte values
- Timestamps are within reasonable range
- Positions have valid lat/lon ranges
- Enum values are known

### 11.2 Semantic Validation

Implementations SHOULD validate:
- Callsigns follow naming conventions
- TTL values are reasonable
- Timestamps are not in the future

### 11.3 Schema Registry

Production deployments SHOULD maintain a schema registry for:
- Version compatibility checking
- Dynamic schema discovery
- Schema documentation

---

## Appendix A: References

- Protocol Buffers Language Guide: https://protobuf.dev
- MIL-STD-6016: Link 16 Standard
- CoT Specification: Cursor-on-Target
- ADR-012: Schema Definition Protocol Extensibility
- ADR-020: TAK-CoT Integration
- ADR-028: CoT Detail Extension Schema

## Appendix B: Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-07 | Initial draft |

---

# HIVE Protocol Specification: Coordination Protocol

**Spec ID**: HIVE-SPEC-004
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: (r)evolve - Revolve Team LLC

## Abstract

This document specifies the coordination protocol for HIVE. It defines cell formation, leader election, hierarchical organization, and inter-cell coordination mechanisms.

## 1. Introduction

### 1.1 Purpose

The HIVE coordination protocol enables autonomous and semi-autonomous systems to form dynamic teams ("cells") that operate effectively without centralized control. It provides mechanisms for:
- Discovering and joining cells
- Electing leaders based on capabilities and authority
- Organizing hierarchically (team → group → formation)
- Handling node failures and network partitions

### 1.2 Design Goals

- **Decentralized**: No single point of failure
- **Adaptive**: Responds to changing conditions
- **Hybrid Human-Machine**: Integrates human authority
- **Resilient**: Continues operating during partitions

### 1.3 Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Cell** | A group of nodes coordinating together |
| **Formation** | The process of establishing a cell |
| **Leader** | Node responsible for cell coordination |
| **Member** | Any node in a cell (including leader) |
| **Parent Cell** | Higher echelon cell (e.g., group to team) |
| **Child Cell** | Lower echelon cell |
| **Hierarchy Level** | Position in command structure (0=root) |
| **Capability Score** | Numeric rating of node capabilities |
| **Authority Score** | Numeric rating of human authority |

---

## 3. Cell Fundamentals

### 3.1 Cell Identity

Each cell has:
- **Cell ID**: UUID v4 (16 bytes)
- **Formation ID**: Shared secret for admission (32 bytes)
- **Callsign**: Human-readable name (e.g., "ALPHA")
- **Hierarchy Level**: Position in structure (0-7)

### 3.2 Cell Configuration

```rust
pub struct CellConfig {
    /// Minimum nodes required for quorum
    pub min_members: usize,
    /// Maximum nodes allowed
    pub max_members: usize,
    /// Leader election interval
    pub election_interval: Duration,
    /// Heartbeat timeout
    pub heartbeat_timeout: Duration,
    /// Leadership policy
    pub leadership_policy: LeadershipPolicy,
    /// Whether humans are required
    pub requires_human: bool,
}

pub enum LeadershipPolicy {
    /// Highest rank always wins
    RankDominant,
    /// Best technical capabilities wins
    TechnicalDominant,
    /// Weighted combination
    Hybrid { authority_weight: f32, technical_weight: f32 },
    /// Adapts to mission phase
    Contextual,
}
```

### 3.3 Cell States

```
                    ┌──────────────┐
                    │   Forming    │
                    └──────┬───────┘
                           │ quorum reached
                           ▼
                    ┌──────────────┐
              ┌─────│   Active     │─────┐
              │     └──────────────┘     │
              │             │             │
        partition│         │ quorum lost  │ merge
              │             ▼             │
              │     ┌──────────────┐     │
              └────>│  Degraded    │<────┘
                    └──────┬───────┘
                           │ dissolved
                           ▼
                    ┌──────────────┐
                    │  Dissolved   │
                    └──────────────┘
```

---

## 4. Cell Formation

### 4.1 Discovery

Nodes discover potential cells through:
1. **mDNS broadcast**: Local network discovery
2. **Static configuration**: Pre-configured peer list
3. **BLE advertising**: Bluetooth discovery
4. **Parent assignment**: Directed by higher echelon

### 4.2 Formation Protocol

```
    Initiator                          Responder
        │                                   │
        │-------- FormationRequest -------->│
        │  (formation_id, capabilities)     │
        │                                   │
        │<------- FormationChallenge -------|
        │  (nonce)                          │
        │                                   │
        │-------- FormationResponse ------->│
        │  (signature over nonce)           │
        │                                   │
        │<------- FormationAccept ----------|
        │  (cell_id, members, leader)       │
        │                                   │
```

### 4.3 Formation Messages

```protobuf
message FormationRequest {
    // Pre-shared formation key hash
    bytes formation_id = 1;
    // Requester's device ID
    bytes device_id = 2;
    // Requester's public key
    bytes public_key = 3;
    // Capability advertisement
    CapabilityAdvertisement capabilities = 4;
    // Requested role (optional)
    optional Role requested_role = 5;
}

message FormationChallenge {
    // Random nonce (32 bytes)
    bytes nonce = 1;
    // Challenge timestamp
    Timestamp timestamp = 2;
    // Challenger's device ID
    bytes challenger_id = 3;
}

message FormationResponse {
    // Original nonce
    bytes nonce = 1;
    // Ed25519 signature over (nonce || formation_id)
    bytes signature = 2;
    // Responder's device ID
    bytes device_id = 3;
}

message FormationAccept {
    // Assigned cell ID
    bytes cell_id = 1;
    // Current cell members
    repeated CellMember members = 2;
    // Current leader
    bytes leader_id = 3;
    // Cell configuration
    CellConfig config = 4;
}

message CellMember {
    bytes device_id = 1;
    bytes public_key = 2;
    Role role = 3;
    Timestamp joined_at = 4;
}
```

### 4.4 Admission Control

Nodes MUST be rejected if:
- Formation key challenge fails
- Cell is at max capacity
- Device is on blocklist
- Required capabilities not present

---

## 5. Leader Election

### 5.1 Election Trigger

Leader election occurs when:
1. Cell is newly formed
2. Current leader fails (heartbeat timeout)
3. Current leader resigns
4. Periodic re-election interval expires
5. Higher authority overrides

### 5.2 Scoring Algorithm

```rust
pub fn compute_leadership_score(
    node: &Node,
    policy: &LeadershipPolicy,
) -> f64 {
    let technical = compute_technical_score(node);
    let authority = compute_authority_score(node);

    match policy {
        LeadershipPolicy::TechnicalDominant => technical,
        LeadershipPolicy::RankDominant => authority,
        LeadershipPolicy::Hybrid { authority_weight, technical_weight } => {
            technical * technical_weight + authority * authority_weight
        }
        LeadershipPolicy::Contextual => {
            // Adapts based on mission phase
            context_adaptive_score(node)
        }
    }
}

fn compute_technical_score(node: &Node) -> f64 {
    // Weighted components (sum to 1.0)
    const COMPUTE_WEIGHT: f64 = 0.30;
    const COMMS_WEIGHT: f64 = 0.25;
    const SENSORS_WEIGHT: f64 = 0.20;
    const POWER_WEIGHT: f64 = 0.15;
    const RELIABILITY_WEIGHT: f64 = 0.10;

    normalize(node.compute) * COMPUTE_WEIGHT
        + normalize(node.comms) * COMMS_WEIGHT
        + normalize(node.sensors) * SENSORS_WEIGHT
        + normalize(node.power) * POWER_WEIGHT
        + normalize(node.reliability) * RELIABILITY_WEIGHT
}

fn compute_authority_score(node: &Node) -> f64 {
    if let Some(operator) = &node.operator_binding {
        rank_to_score(operator.rank) * 0.6
            + authority_level_to_score(operator.authority) * 0.3
            + (1.0 - operator.cognitive_load) * 0.1
    } else {
        0.0 // No human operator
    }
}
```

### 5.3 Election Protocol

```
    Node A (candidate)          Node B (candidate)          Node C (voter)
         │                           │                           │
         │<────────── RequestVote ───┼───────────────────────────│
         │   (score: 0.85)           │                           │
         │                           │                           │
         │───────── RequestVote ─────┼──────────────────────────>│
         │   (score: 0.72)           │                           │
         │                           │                           │
         │                           │<───── VoteGrant ──────────│
         │                           │   (for: A)                │
         │<───────── VoteGrant ──────┼───────────────────────────│
         │   (for: A)                │                           │
         │                           │                           │
         │────────── Elected ────────┼──────────────────────────>│
         │                           │                           │
```

### 5.4 Tie-Breaking

If scores are equal (within 0.01), ties are broken by:
1. Higher human authority rank
2. Longer cell membership duration
3. Lexicographically higher device ID

### 5.5 Election Timeout

Elections MUST complete within:
- Normal: 5 seconds
- Emergency (leader failed): 2 seconds

If no consensus in timeout, the node with highest score self-declares.

---

## 6. Hierarchical Organization

### 6.1 Hierarchy Levels

| Level | Name | Typical Size | Parent |
|-------|------|--------------|--------|
| 0 | Root | 1 | None |
| 1 | Cluster | 100-200 | Root |
| 2 | Formation | 30-50 | Cluster |
| 3 | Group | 8-12 | Formation |
| 4 | Team | 2-4 | Group |
| 5 | Node | 1 | Team |

### 6.2 Parent-Child Relationship

```protobuf
message HierarchyBinding {
    // Child cell ID
    bytes child_cell_id = 1;
    // Parent cell ID
    bytes parent_cell_id = 2;
    // Parent leader's device ID
    bytes parent_leader_id = 3;
    // Binding timestamp
    Timestamp bound_at = 4;
    // Binding status
    BindingStatus status = 5;
}

enum BindingStatus {
    BINDING_STATUS_UNSPECIFIED = 0;
    BINDING_STATUS_PENDING = 1;
    BINDING_STATUS_ACTIVE = 2;
    BINDING_STATUS_SUSPENDED = 3;
    BINDING_STATUS_DISSOLVED = 4;
}
```

### 6.3 Capability Aggregation and Emergent Behavior

A core principle of HIVE is that **cells exhibit emergent capabilities** greater than the sum of their individual members. Capability aggregation flows upward through the hierarchy, enabling higher echelons to understand and task based on collective capabilities.

#### 6.3.1 Capability Flow Model

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CLUSTER COORDINATOR                                │
│  Sees: "3 formations with combined Sensing, Action, and Relay capabilities" │
│  Emergent: Full-spectrum observation and coordinated action package          │
└─────────────────────────────────────────────────────────────────────────────┘
                                    ▲
                    Aggregated capability summaries
                                    │
        ┌───────────────────────────┼───────────────────────────┐
        │                           │                           │
        ▼                           ▼                           ▼
┌───────────────────┐     ┌───────────────────┐     ┌───────────────────┐
│   FORMATION 1     │     │   FORMATION 2     │     │   FORMATION 3     │
│  Sensing + Relay  │     │  Action + Sensing │     │  Action + Signal  │
│  Emergent: Wide   │     │  Emergent: Sense- │     │  Emergent: Coord- │
│  area coverage    │     │  and-act          │     │  inated response  │
└─────────┬─────────┘     └─────────┬─────────┘     └─────────┬─────────┘
          │                         │                         │
    ┌─────┴─────┐             ┌─────┴─────┐             ┌─────┴─────┐
    ▼           ▼             ▼           ▼             ▼           ▼
┌───────┐   ┌───────┐     ┌───────┐   ┌───────┐     ┌───────┐   ┌───────┐
│Group 1│   │Group 2│     │Group 3│   │Group 4│     │Group 5│   │Group 6│
│EO+IR  │   │COMMS  │     │Action │   │EO     │     │Action │   │Signal │
└───┬───┘   └───┬───┘     └───┬───┘   └───┬───┘     └───┬───┘   └───┬───┘
    │           │             │           │             │           │
┌───┴───┐   ┌───┴───┐     ┌───┴───┐   ┌───┴───┐     ┌───┴───┐   ┌───┴───┐
│4 UAV  │   │2 Relay│     │2 Auton│   │4 UAV  │     │2 Auton│   │2 Signal│
│sensors│   │nodes  │     │actors │   │sensors│     │actors │   │ nodes  │
└───────┘   └───────┘     └───────┘   └───────┘     └───────┘   └───────┘

Individual platforms → Group capabilities → Formation emergent → Cluster emergent
```

#### 6.3.2 Emergent Capability Discovery

Emergent capabilities arise from the **composition** of individual platform capabilities:

```rust
/// Emergent capability patterns recognized by HIVE
pub enum EmergentCapability {
    /// Multiple sensors with overlapping coverage → Wide-area observation
    WideAreaObservation {
        sensor_count: usize,
        coverage_area_km2: f64,
    },

    /// Sensing + Actuation in same cell → Sense-and-act loop
    SenseAndAct {
        sensing_platforms: Vec<DeviceId>,
        actuation_platforms: Vec<DeviceId>,
    },

    /// Signal + Actuation → Coordinated response
    CoordinatedResponse {
        signal_nodes: Vec<DeviceId>,
        actuators: Vec<DeviceId>,
    },

    /// Multiple relay nodes → Extended mesh range
    ExtendedRange {
        relay_chain: Vec<DeviceId>,
        range_extension_km: f64,
    },

    /// Heterogeneous sensors → Multi-spectral fusion
    MultiSpectralFusion {
        eo_sensors: Vec<DeviceId>,
        ir_sensors: Vec<DeviceId>,
        radar_sensors: Vec<DeviceId>,
    },

    /// Compute + sensors → Edge AI processing
    EdgeIntelligence {
        compute_nodes: Vec<DeviceId>,
        sensor_feeds: Vec<DeviceId>,
        models_available: Vec<String>,
    },
}

/// Detect emergent capabilities from member capabilities
pub fn discover_emergent_capabilities(
    members: &[CapabilityAdvertisement],
) -> Vec<EmergentCapability> {
    let mut emergent = Vec::new();

    // Collect capability types across all members
    let all_sensors: Vec<_> = members.iter()
        .flat_map(|m| &m.sensors)
        .collect();
    let all_actuators: Vec<_> = members.iter()
        .flat_map(|m| &m.actuators)
        .collect();
    let total_compute: f64 = members.iter()
        .filter_map(|m| m.compute.as_ref())
        .map(|c| c.compute_tflops.unwrap_or(0.0))
        .sum();

    // Pattern: Sensing + Actuation = Sense-and-Act Loop
    let sensing_capable: Vec<_> = members.iter()
        .filter(|m| has_sensing_capability(m))
        .map(|m| m.device_id)
        .collect();
    let actuation_capable: Vec<_> = members.iter()
        .filter(|m| has_actuation_capability(m))
        .map(|m| m.device_id)
        .collect();

    if !sensing_capable.is_empty() && !actuation_capable.is_empty() {
        emergent.push(EmergentCapability::SenseAndAct {
            sensing_platforms: sensing_capable,
            actuation_platforms: actuation_capable,
        });
    }

    // Pattern: Multiple overlapping sensors = Wide Area Observation
    if all_sensors.len() >= 3 {
        let coverage = calculate_combined_coverage(&all_sensors);
        emergent.push(EmergentCapability::WideAreaObservation {
            sensor_count: all_sensors.len(),
            coverage_area_km2: coverage,
        });
    }

    // Pattern: Compute + Sensors = Edge Intelligence
    if total_compute > 1.0 && !all_sensors.is_empty() {
        emergent.push(EmergentCapability::EdgeIntelligence {
            compute_nodes: members.iter()
                .filter(|m| m.compute.as_ref()
                    .map(|c| c.compute_tflops.unwrap_or(0.0) > 0.5)
                    .unwrap_or(false))
                .map(|m| m.device_id)
                .collect(),
            sensor_feeds: members.iter()
                .filter(|m| !m.sensors.is_empty())
                .map(|m| m.device_id)
                .collect(),
            models_available: collect_available_models(members),
        });
    }

    emergent
}
```

#### 6.3.3 Capability Aggregation Protocol

Each cell leader periodically computes and advertises aggregated capabilities:

```protobuf
message CellCapabilitySummary {
    // Cell identifier
    bytes cell_id = 1;
    // Hierarchy level
    uint32 level = 2;
    // Timestamp of this summary
    Timestamp timestamp = 3;

    // Member count by type
    map<int32, uint32> member_counts = 4;  // PlatformType -> count

    // Aggregated sensor capabilities
    AggregatedSensors sensors = 5;

    // Aggregated actuator capabilities
    AggregatedActuators actuators = 6;

    // Total compute available
    double total_compute_tflops = 7;

    // Communication reach
    CommunicationSummary comms = 8;

    // Average power/endurance
    PowerSummary power = 9;

    // Discovered emergent capabilities
    repeated EmergentCapability emergent = 10;

    // Geographic coverage
    BoundingBox coverage_area = 11;

    // Operational readiness (0.0 - 1.0)
    float readiness = 12;
}

message AggregatedSensors {
    // Count by sensor type
    map<int32, uint32> type_counts = 1;
    // Combined detection range (best case)
    double max_detection_range_m = 2;
    // Combined coverage area
    double coverage_area_km2 = 3;
    // Available sensor modalities
    repeated SensorType modalities = 4;
}

message AggregatedActuators {
    // Count by actuator type
    map<int32, uint32> type_counts = 1;
    // Total available resources/uses
    uint32 total_resources = 2;
    // Combined action range
    double max_action_range_m = 3;
    // Total payload capacity
    double total_payload_kg = 4;
}
```

#### 6.3.4 Aggregation Policies

Different data types require different aggregation strategies:

```rust
pub struct AggregationPolicy {
    /// How to aggregate track data
    pub track_aggregation: TrackAggregation,
    /// How to summarize capabilities
    pub capability_mode: CapabilitySummaryMode,
    /// Status report interval to parent
    pub status_interval: Duration,
    /// Priority threshold for immediate escalation
    pub escalation_priority: Priority,
    /// Whether to include individual member details
    pub include_member_details: bool,
    /// Emergent capability detection enabled
    pub detect_emergent: bool,
}

pub enum TrackAggregation {
    /// Send all tracks (high bandwidth)
    Full,
    /// Send summary counts by type (minimal bandwidth)
    CountOnly,
    /// Send priority tracks + counts (balanced)
    PriorityPlusCounts { max_tracks: usize },
    /// Spatial clustering (geographic compression)
    Clustered { cluster_radius_m: f64 },
}

pub enum CapabilitySummaryMode {
    /// Full capability details per member
    Detailed,
    /// Aggregated totals only
    Totals,
    /// Totals + emergent capabilities
    TotalsWithEmergent,
    /// Only report capability changes
    DeltaOnly,
}
```

### 6.4 Bidirectional Flow Model

HIVE operates as a **full-duplex hierarchical synchronization system**:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            UPWARD FLOW                                       │
│  (Edge → Coordinator)                                                        │
│                                                                              │
│  • Capability advertisements (what can I do?)                                │
│  • Track/detection reports (what do I see?)                                  │
│  • Status updates (how am I doing?)                                          │
│  • Emergent capability discovery (what can WE do together?)                  │
│                                                                              │
│  Characteristics: High volume, compressible, tolerates staleness             │
├─────────────────────────────────────────────────────────────────────────────┤
│                            DOWNWARD FLOW                                     │
│  (Coordinator → Edge)                                                        │
│                                                                              │
│  • Mission tasking (what should I do?)                                       │
│  • Operational constraints (what am I allowed to do?)                        │
│  • AI model distribution (how should I process data?)                        │
│  • Configuration changes (how should I operate?)                             │
│  • Coordinator intent (what is the goal?)                                    │
│                                                                              │
│  Characteristics: Low volume, high priority, cannot tolerate loss            │
├─────────────────────────────────────────────────────────────────────────────┤
│                           HORIZONTAL FLOW                                    │
│  (Peer → Peer)                                                               │
│                                                                              │
│  • Track handoffs (transferring responsibility)                              │
│  • Deconfliction (avoiding collisions/interference)                          │
│  • Mutual support requests (need sensor/actuator coverage)                   │
│  • Boundary coordination (adjacent cell awareness)                           │
│                                                                              │
│  Characteristics: Time-critical, requires peer authentication                │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 6.4.1 Policy-Based Routing

Events carry routing policies that HIVE enforces:

```rust
pub struct EventRoutingPolicy {
    /// How far up the hierarchy to propagate
    pub propagation: PropagationMode,
    /// Priority for bandwidth allocation
    pub priority: Priority,
    /// What to do on network partition
    pub partition_handling: PartitionPolicy,
    /// Time-to-live before expiration
    pub ttl: Duration,
}

pub enum PropagationMode {
    /// Store locally, respond to queries only
    Local,
    /// Propagate to immediate parent only
    Parent,
    /// Propagate to all ancestors
    AllAncestors,
    /// Immediate propagation, preempt other traffic
    Critical,
}

pub enum PartitionPolicy {
    /// Buffer and retry when connection restored
    BufferAndRetry,
    /// Drop if cannot deliver immediately
    DropOnPartition,
    /// Require immediate delivery or fail
    RequireImmediate,
}
```

### 6.5 Downward Command Flow

Commands flow from parent to child:

```protobuf
message CommandMessage {
    // Source cell ID
    bytes source_cell = 1;
    // Target cell ID (or broadcast)
    optional bytes target_cell = 2;
    // Command type
    CommandType type = 3;
    // Command payload
    bytes payload = 4;
    // Priority
    Priority priority = 5;
    // Acknowledgment required
    bool ack_required = 6;
}

enum CommandType {
    COMMAND_TYPE_UNSPECIFIED = 0;
    COMMAND_TYPE_MISSION_ASSIGN = 1;
    COMMAND_TYPE_POSITION_UPDATE = 2;
    COMMAND_TYPE_FORMATION_CHANGE = 3;
    COMMAND_TYPE_ABORT = 4;
    COMMAND_TYPE_RALLY = 5;
}
```

---

## 7. Role Assignment

### 7.1 Standard Roles

```protobuf
enum Role {
    ROLE_UNSPECIFIED = 0;
    ROLE_LEADER = 1;      // Cell leader
    ROLE_DEPUTY = 2;      // Backup leader
    ROLE_SCOUT = 3;       // Forward observer
    ROLE_RELAY = 4;       // Communications relay
    ROLE_SENSOR = 5;      // Primary sensor platform
    ROLE_ACTUATOR = 6;    // Primary actuator
    ROLE_SUPPORT = 7;     // Supply/support
    ROLE_OBSERVER = 8;    // Passive observer
}
```

### 7.2 Role Assignment Algorithm

```rust
pub fn assign_roles(cell: &Cell) -> HashMap<DeviceId, Role> {
    let mut assignments = HashMap::new();

    // Leader is already elected
    assignments.insert(cell.leader_id, Role::Leader);

    // Deputy = second-highest leadership score
    let deputy = cell.members
        .iter()
        .filter(|m| m.device_id != cell.leader_id)
        .max_by_key(|m| m.leadership_score);
    if let Some(d) = deputy {
        assignments.insert(d.device_id, Role::Deputy);
    }

    // Assign remaining roles by capability match
    for member in &cell.members {
        if assignments.contains_key(&member.device_id) {
            continue;
        }

        let role = match_best_role(member, &cell.mission);
        assignments.insert(member.device_id, role);
    }

    assignments
}
```

### 7.3 Role Handoff

When roles change (e.g., leader failure):

```
    Old Leader                New Leader               Members
         │                        │                        │
         │ (fails)                │                        │
         │                        │                        │
         │          ┌─────────────┼────────────────────────│
         │          │ election    │                        │
         │          ▼             │                        │
         │     ┌─────────┐        │                        │
         │     │ ELECTED │        │                        │
         │     └────┬────┘        │                        │
         │          │             │                        │
         │          │────────── RoleChange ───────────────>│
         │          │  (new_leader, new_deputy)            │
         │          │                                      │
         │          │<──────── RoleAck ────────────────────│
         │          │                                      │
```

---

## 8. State Synchronization

### 8.1 Cell State Document

Cell state is maintained as a CRDT document:

```rust
pub struct CellState {
    /// Cell identifier
    pub cell_id: CellId,
    /// Current members
    pub members: HashMap<DeviceId, MemberState>,
    /// Current leader
    pub leader_id: DeviceId,
    /// Role assignments
    pub roles: HashMap<DeviceId, Role>,
    /// Active missions
    pub missions: Vec<MissionId>,
    /// Parent binding
    pub parent: Option<HierarchyBinding>,
    /// Children
    pub children: Vec<CellId>,
    /// Last election epoch
    pub election_epoch: u64,
    /// Configuration
    pub config: CellConfig,
}

pub struct MemberState {
    pub device_id: DeviceId,
    pub last_heartbeat: Timestamp,
    pub position: Option<Position>,
    pub status: OperationalStatus,
    pub capabilities: CapabilityAdvertisement,
}
```

### 8.2 Heartbeat Protocol

Members MUST send heartbeats to maintain membership:

```protobuf
message Heartbeat {
    bytes device_id = 1;
    bytes cell_id = 2;
    Timestamp timestamp = 3;
    Position position = 4;
    OperationalStatus status = 5;
    uint32 power_level = 6;
}
```

**Timing**:
- Heartbeat interval: 5 seconds (configurable)
- Failure threshold: 3 missed heartbeats
- Grace period after rejoin: 10 seconds

---

## 9. Failure Handling

### 9.1 Member Failure Detection

```
    Member A                  Leader                   Member B
        │                        │                        │
        │──── Heartbeat ────────>│                        │
        │                        │                        │
        │     (fails)            │                        │
        │                        │                        │
        │                        │<─── Heartbeat ─────────│
        │                        │                        │
        │                   ┌────┴────┐                   │
        │                   │ Timeout │                   │
        │                   └────┬────┘                   │
        │                        │                        │
        │                        │──── MemberFailed ─────>│
        │                        │     (device_id: A)     │
        │                        │                        │
```

### 9.2 Leader Failure

1. Deputy detects leader heartbeat timeout
2. Deputy initiates emergency election
3. Election completes within 2 seconds
4. New leader announces to all members
5. New leader notifies parent cell

### 9.3 Network Partition

```
          Pre-Partition                     Post-Partition
    ┌───────────────────────┐         ┌──────────┐  ┌──────────┐
    │  Cell A               │         │ Cell A-1 │  │ Cell A-2 │
    │  Leader: L            │   ──>   │ Leader:L │  │Leader:D  │
    │  Members: L,D,M1,M2   │         │ M: L,M1  │  │ M: D,M2  │
    └───────────────────────┘         └──────────┘  └──────────┘
```

**Partition rules**:
1. Each partition independently elects leader
2. Partition with original leader retains Cell ID
3. Other partition generates new Cell ID (same Formation ID)
4. On heal, merge negotiation occurs

### 9.4 Partition Healing

```
    Cell A-1 (original)           Cell A-2 (split)
         │                             │
         │<────── PartitionHealing ────│
         │   (members, state_hash)     │
         │                             │
         │───── MergeProposal ────────>│
         │   (merged_state)            │
         │                             │
         │<────── MergeAccept ─────────│
         │                             │
         │ (re-election with all)      │
         │                             │
```

---

## 10. Inter-Cell Coordination

### 10.1 Peer Cell Discovery

Cells at the same hierarchy level discover each other for:
- Handoff coordination
- Mutual support
- De-confliction

### 10.2 Handoff Protocol

When a tracked entity moves between cell coverage areas:

```
    Cell A (tracking)            Cell B (receiving)
         │                             │
         │                        (detects target entering AOI)
         │                             │
         │<───── HandoffRequest ───────│
         │   (track_id, my_coverage)   │
         │                             │
         │────── HandoffOffer ────────>│
         │   (track_history, sensor)   │
         │                             │
         │<───── HandoffAccept ────────│
         │                             │
         │   (A stops tracking)        │
         │                             │
```

### 10.3 Mutual Support

Cells can request support from peers:

```protobuf
message SupportRequest {
    bytes requesting_cell = 1;
    SupportType type = 2;
    Position location = 3;
    Priority priority = 4;
    Duration duration = 5;
}

enum SupportType {
    SUPPORT_TYPE_UNSPECIFIED = 0;
    SUPPORT_TYPE_SENSOR = 1;      // Need sensor coverage
    SUPPORT_TYPE_RELAY = 2;       // Need comm relay
    SUPPORT_TYPE_ACTUATOR = 3;    // Need actuation capability
    SUPPORT_TYPE_LOGISTICS = 4;   // Need resupply
}
```

---

## 11. Security Considerations

### 11.1 Formation Key

The Formation ID MUST be:
- Pre-shared out-of-band
- At least 256 bits of entropy
- Rotated periodically or after compromise

### 11.2 Leader Authority

Leaders can:
- Assign roles
- Accept/reject members
- Dissolve cell

Leaders MUST NOT:
- Forge member messages
- Bypass formation authentication
- Override human authority (unless autonomous mode)

### 11.3 Hierarchy Trust

- Child cells trust parent commands (verified by signature)
- Parent cells trust child reports (verified by signature)
- Sibling cells verify each other before coordination

### 11.4 Replay Protection

All coordination messages include:
- Timestamp (reject if > 30 seconds old)
- Nonce (track in replay cache)
- Sequence number (per sender)

---

## Appendix A: References

- Raft Consensus Algorithm (leader election inspiration)
- STANAG 4586 (UAV interoperability)
- ADR-004: Human-Machine Cell Composition
- ADR-014: Distributed Coordination Primitives
- ADR-024: Flexible Hierarchy Strategies
- ADR-027: Event Routing Aggregation Protocol

## Appendix B: Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-07 | Initial draft |

---

# HIVE Protocol Specification: Security Framework

**Spec ID**: HIVE-SPEC-005
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: (r)evolve - Revolve Team LLC

## Abstract

This document specifies the security framework for HIVE Protocol. It defines device authentication, user authorization, encryption, key management, and audit logging requirements.

## 1. Introduction

### 1.1 Purpose

The HIVE security framework ensures that tactical mesh networks operate securely in contested environments. It provides:
- Device identity verification
- Cell membership authentication
- Role-based access control
- End-to-end encryption
- Comprehensive audit logging

### 1.2 Security Objectives

| Objective | Mechanism |
|-----------|-----------|
| Authenticity | Ed25519 signatures |
| Confidentiality | ChaCha20-Poly1305 AEAD |
| Integrity | Cryptographic hashes + signatures |
| Authorization | RBAC + hierarchy verification |
| Non-repudiation | Signed audit logs |

### 1.3 Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Device Keypair** | Ed25519 signing key for device identity |
| **Device ID** | SHA-256 hash of public key (32 bytes) |
| **Formation Key** | Pre-shared secret for cell admission |
| **Group Key** | Symmetric key for cell broadcast encryption |
| **Secure Channel** | Encrypted peer-to-peer connection |
| **Principal** | Entity (device or user) with permissions |
| **Access Level** | Data sensitivity classification |

---

## 3. Security Architecture

### 3.1 Layer Model

```
┌─────────────────────────────────────────────────────────────────┐
│                    Application Security                          │
│  (Input validation, business logic authorization)                │
├─────────────────────────────────────────────────────────────────┤
│                    Protocol Security                             │
│  (Message signing, CRDT authentication, replay protection)       │
├─────────────────────────────────────────────────────────────────┤
│                    Transport Security                            │
│  (TLS 1.3 via QUIC, secure channels, bypass encryption)         │
├─────────────────────────────────────────────────────────────────┤
│                    Identity Security                             │
│  (Device PKI, user auth, formation key verification)            │
├─────────────────────────────────────────────────────────────────┤
│                    Hardware Security (Optional)                  │
│  (TPM, Secure Enclave, PUF-derived keys)                        │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 Trust Boundaries

```
┌─────────────────────────────────────────────────────────────────┐
│                      Untrusted Network                           │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    Cell Boundary                           │  │
│  │  ┌─────────────────────────────────────────────────────┐  │  │
│  │  │              Authenticated Peers                     │  │  │
│  │  │  ┌─────────────────────────────────────────────┐    │  │  │
│  │  │  │         Authorized Principals               │    │  │  │
│  │  │  │  ┌─────────────────────────────────────┐   │    │  │  │
│  │  │  │  │    Local Process (Trusted)          │   │    │  │  │
│  │  │  │  └─────────────────────────────────────┘   │    │  │  │
│  │  │  └─────────────────────────────────────────────┘    │  │  │
│  │  └─────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 4. Device Identity

### 4.1 Key Generation

Devices MUST generate an Ed25519 keypair at initialization:

```rust
pub struct DeviceKeypair {
    /// Ed25519 signing key (32 bytes secret)
    signing_key: SigningKey,
    /// Ed25519 verification key (32 bytes public)
    verifying_key: VerifyingKey,
}

impl DeviceKeypair {
    pub fn generate() -> Self {
        let mut rng = OsRng;
        let signing_key = SigningKey::generate(&mut rng);
        let verifying_key = signing_key.verifying_key();
        Self { signing_key, verifying_key }
    }

    pub fn device_id(&self) -> DeviceId {
        let hash = Sha256::digest(self.verifying_key.as_bytes());
        DeviceId(hash.into())
    }
}
```

### 4.2 Key Storage

Device keys MUST be stored securely:

| Platform | Storage Mechanism |
|----------|-------------------|
| Linux | File with mode 0600, encrypted at rest |
| Android | Android Keystore (hardware-backed) |
| iOS | Secure Enclave |
| Windows | DPAPI or TPM 2.0 |
| ESP32 | eFuse or NVS with encryption |

### 4.3 Device Identity Binding

```protobuf
message DeviceIdentity {
    // Device ID (SHA-256 of public key)
    bytes device_id = 1;
    // Ed25519 public key (32 bytes)
    bytes public_key = 2;
    // Device type
    DeviceType device_type = 3;
    // Hardware attestation (if available)
    optional bytes attestation = 4;
    // Display name
    optional string display_name = 5;
    // Creation timestamp
    Timestamp created_at = 6;
}
```

---

## 5. Authentication

### 5.1 Challenge-Response Protocol

```
    Prover (joining node)           Verifier (cell member)
           │                              │
           │───── AuthRequest ───────────>│
           │  (device_id, public_key)     │
           │                              │
           │<──── Challenge ──────────────│
           │  (nonce, timestamp)          │
           │                              │
           │───── ChallengeResponse ─────>│
           │  (signature over nonce)      │
           │                              │
           │<──── AuthResult ─────────────│
           │  (success/failure, session)  │
           │                              │
```

### 5.2 Challenge Generation

```rust
pub fn generate_challenge() -> Challenge {
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);

    Challenge {
        nonce: nonce.to_vec(),
        timestamp: current_timestamp(),
        challenger_id: self.device_id.clone(),
    }
}
```

### 5.3 Challenge Response

The prover signs:
```
message = nonce || challenger_id || timestamp
signature = Ed25519_Sign(signing_key, message)
```

### 5.4 Verification

```rust
pub fn verify_response(
    response: &SignedChallengeResponse,
    original_challenge: &Challenge,
) -> Result<DeviceId, SecurityError> {
    // Check timestamp freshness (max 30 seconds)
    if is_expired(&original_challenge.timestamp, 30) {
        return Err(SecurityError::ExpiredChallenge);
    }

    // Reconstruct signed message
    let message = [
        &original_challenge.nonce[..],
        &original_challenge.challenger_id[..],
        &timestamp_bytes(&original_challenge.timestamp)[..],
    ].concat();

    // Verify signature
    let public_key = VerifyingKey::from_bytes(&response.public_key)?;
    let signature = Signature::from_bytes(&response.signature)?;

    public_key.verify(&message, &signature)?;

    // Derive and return device ID
    Ok(DeviceId::from_public_key(&response.public_key))
}
```

### 5.5 Formation Key Authentication

For cell admission, nodes must also prove knowledge of the formation key:

```rust
pub fn verify_formation_key(
    response: &FormationResponse,
    formation_key: &FormationKey,
    challenge: &FormationChallenge,
) -> Result<(), SecurityError> {
    // Compute expected response
    let expected = Hmac::<Sha256>::new_from_slice(&formation_key.0)?
        .chain_update(&challenge.nonce)
        .finalize()
        .into_bytes();

    // Constant-time comparison
    if !constant_time_eq(&response.proof, &expected) {
        return Err(SecurityError::FormationKeyMismatch);
    }

    Ok(())
}
```

---

## 6. Authorization

### 6.1 Role-Based Access Control

```rust
pub enum Role {
    /// Observer with read-only access
    Observer,
    /// Standard member with read/write
    Member,
    /// Operator with human authority
    Operator,
    /// Cell leader with admin rights
    Leader,
    /// Parent cell supervisor
    Supervisor,
}

pub enum Permission {
    /// Read documents
    Read,
    /// Write documents
    Write,
    /// Delete documents
    Delete,
    /// Modify cell membership
    ModifyMembership,
    /// Issue commands
    IssueCommands,
    /// Access sensitive data
    AccessSensitive { level: AccessLevel },
}
```

### 6.2 Permission Matrix

| Role | Read | Write | Delete | Membership | Commands | Classified |
|------|------|-------|--------|------------|----------|------------|
| Observer | Yes | No | No | No | No | Own level |
| Member | Yes | Yes | Own | No | No | Own level |
| Operator | Yes | Yes | Yes | No | Yes | Own level |
| Leader | Yes | Yes | Yes | Yes | Yes | Cell level |
| Supervisor | Yes | Yes | Yes | Yes | Yes | Parent level |

### 6.3 Access Levels

```rust
pub enum AccessLevel {
    /// Public - no restrictions
    Public = 0,
    /// Internal - cell members only
    Internal = 1,
    /// Restricted - requires role
    Restricted = 2,
    /// Sensitive - leader approval
    Sensitive = 3,
    /// Critical - root approval
    Critical = 4,
}
```

### 6.4 Authorization Check

```rust
pub fn check_authorization(
    principal: &Principal,
    action: &Action,
    resource: &Resource,
    context: &AuthorizationContext,
) -> Result<(), AuthorizationError> {
    // Check role permission
    if !principal.role.has_permission(&action.permission) {
        return Err(AuthorizationError::InsufficientRole);
    }

    // Check access level
    if principal.access_level < resource.required_level {
        return Err(AuthorizationError::InsufficientAccess);
    }

    // Check cell membership
    if !context.cell.contains(&principal.device_id) {
        return Err(AuthorizationError::NotCellMember);
    }

    // Check hierarchy (for parent/child access)
    if let Some(required_level) = action.required_hierarchy_level {
        if context.hierarchy_level > required_level {
            return Err(AuthorizationError::HierarchyViolation);
        }
    }

    Ok(())
}
```

---

## 7. Encryption

### 7.1 Algorithms

| Purpose | Algorithm | Key Size |
|---------|-----------|----------|
| Symmetric encryption | ChaCha20-Poly1305 | 256 bits |
| Key exchange | X25519 | 256 bits |
| Signing | Ed25519 | 256 bits |
| Hashing | SHA-256 | 256 bits |
| Key derivation | HKDF-SHA256 | Variable |

### 7.2 Secure Channel Establishment

```
    Initiator                         Responder
        │                                 │
        │──── Ephemeral Public Key ──────>│
        │  (X25519 public key)            │
        │                                 │
        │<─── Ephemeral Public Key ───────│
        │                                 │
        │ (Both compute shared secret)    │
        │                                 │
        │ shared = X25519(my_secret, their_public)
        │                                 │
        │ keys = HKDF(shared, salt, info) │
        │   - initiator_to_responder_key  │
        │   - responder_to_initiator_key  │
        │                                 │
```

### 7.3 Message Encryption

```rust
pub fn encrypt_message(
    plaintext: &[u8],
    key: &SymmetricKey,
) -> Result<EncryptedData, EncryptionError> {
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);

    let cipher = ChaCha20Poly1305::new(key.as_ref().into());
    let ciphertext = cipher.encrypt(&nonce.into(), plaintext)?;

    Ok(EncryptedData {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}

pub fn decrypt_message(
    encrypted: &EncryptedData,
    key: &SymmetricKey,
) -> Result<Vec<u8>, EncryptionError> {
    let cipher = ChaCha20Poly1305::new(key.as_ref().into());
    let nonce = GenericArray::from_slice(&encrypted.nonce);

    cipher.decrypt(nonce, encrypted.ciphertext.as_ref())
        .map_err(|_| EncryptionError::DecryptionFailed)
}
```

### 7.4 Group Encryption

For cell-wide broadcasts, a shared GroupKey is used:

```rust
pub struct GroupKey {
    /// The symmetric key material
    key: [u8; 32],
    /// Key generation/epoch
    generation: u64,
    /// Expiration timestamp
    expires_at: Timestamp,
}
```

---

## 8. Key Management

### 8.1 Key Hierarchy

```
                    ┌───────────────────┐
                    │  Device Master Key │
                    │  (Ed25519 keypair) │
                    └─────────┬─────────┘
                              │
              ┌───────────────┼───────────────┐
              │               │               │
              ▼               ▼               ▼
       ┌──────────┐    ┌──────────┐    ┌──────────┐
       │ Signing  │    │ Identity │    │ Derivation│
       │   Key    │    │   Key    │    │   Key    │
       └──────────┘    └──────────┘    └─────┬────┘
                                             │
                              ┌──────────────┼──────────────┐
                              │              │              │
                              ▼              ▼              ▼
                       ┌──────────┐   ┌──────────┐   ┌──────────┐
                       │ Channel  │   │  Group   │   │ Storage  │
                       │   Keys   │   │   Keys   │   │   Keys   │
                       └──────────┘   └──────────┘   └──────────┘
```

### 8.2 Key Rotation

#### Formation Key Rotation
- **Interval**: SHOULD rotate after any member departure
- **Method**: Leader generates new key, distributes via secure channels
- **Grace period**: 5 minutes for late-arriving updates

#### Group Key Rotation
- **Interval**: Configurable (default: 24 hours or on member change)
- **Method**: MLS tree ratcheting (recommended) or leader distribution
- **Retained epochs**: Keep last 5 keys for late messages

#### Device Key Rotation
- **Interval**: Annually or on suspected compromise
- **Method**: Generate new keypair, re-authenticate to cells
- **Impact**: Requires manual re-provisioning

### 8.3 Key Distribution

```protobuf
message KeyDistribution {
    // Key type being distributed
    KeyType type = 1;
    // Encrypted key material (per recipient)
    repeated EncryptedKeyShare shares = 2;
    // Key generation/epoch
    uint64 generation = 3;
    // Expiration
    Timestamp expires_at = 4;
    // Signature from distributor
    bytes signature = 5;
}

message EncryptedKeyShare {
    bytes recipient_id = 1;
    bytes encrypted_key = 2;  // Encrypted with recipient's public key
    bytes nonce = 3;
}
```

### 8.4 Forward Secrecy

HIVE provides forward secrecy through:
1. **Ephemeral keys**: New X25519 keypair per session
2. **Key ratcheting**: Group keys advance after member removal
3. **MLS integration** (recommended): Full forward secrecy via tree-based key agreement

---

## 9. Audit Logging

### 9.1 Audit Events

```rust
pub enum AuditEventType {
    // Authentication events
    AuthenticationAttempt { device_id: DeviceId, success: bool },
    FormationJoin { device_id: DeviceId, cell_id: CellId },
    FormationLeave { device_id: DeviceId, cell_id: CellId },

    // Authorization events
    AuthorizationCheck { principal: Principal, action: Action, allowed: bool },
    PermissionChange { target: DeviceId, old_role: Role, new_role: Role },

    // Key management events
    KeyRotation { key_type: KeyType, generation: u64 },
    KeyDistribution { recipients: Vec<DeviceId> },

    // Security violations
    SecurityViolation { violation: SecurityViolation, source: DeviceId },
}

pub enum SecurityViolation {
    InvalidSignature,
    ExpiredChallenge,
    ReplayDetected,
    UnauthorizedAccess,
    MalformedMessage,
    RateLimitExceeded,
}
```

### 9.2 Audit Log Entry

```protobuf
message AuditLogEntry {
    // Unique entry ID
    bytes entry_id = 1;
    // Timestamp
    Timestamp timestamp = 2;
    // Event type
    AuditEventType event = 3;
    // Device that logged this entry
    bytes logger_id = 4;
    // Device that triggered the event
    optional bytes actor_id = 5;
    // Human-readable description
    string description = 6;
    // Additional context (JSON)
    optional bytes context = 7;
    // Hash of previous entry (chain integrity)
    bytes previous_hash = 8;
    // Signature over entry
    bytes signature = 9;
}
```

### 9.3 Log Integrity

Audit logs form a hash chain for tamper detection:

```
Entry[n].previous_hash = SHA256(Entry[n-1])
Entry[n].signature = Sign(Entry[n] - signature field)
```

### 9.4 Log Retention

| Log Type | Minimum Retention |
|----------|-------------------|
| Authentication | 90 days |
| Authorization | 30 days |
| Security violations | 1 year |
| Key management | 2 years |

---

## 10. Threat Model

### 10.1 Adversary Capabilities

| Adversary | Capabilities |
|-----------|--------------|
| Passive Eavesdropper | Monitor network traffic |
| Active Attacker | Inject, modify, replay messages |
| Compromised Node | Full control of one cell member |
| Insider Threat | Valid credentials, malicious intent |

### 10.2 Threats and Mitigations

| Threat | Mitigation |
|--------|------------|
| Eavesdropping | TLS 1.3, ChaCha20-Poly1305 encryption |
| Impersonation | Ed25519 device authentication |
| Replay attacks | Timestamp + nonce + sequence numbers |
| Man-in-the-middle | Public key verification, challenge-response |
| Unauthorized access | RBAC, access levels |
| Data tampering | Cryptographic signatures |
| Key compromise | Key rotation, forward secrecy |
| Denial of service | Rate limiting, connection limits |

### 10.3 Out of Scope

- Physical access to device
- Side-channel attacks (timing, power analysis)
- Quantum computing attacks (future consideration)

---

## 11. Implementation Requirements

### 11.1 MUST Implement

- Ed25519 device keypair generation and storage
- Challenge-response authentication
- Formation key verification
- ChaCha20-Poly1305 encryption for group messages
- Basic audit logging (auth, security violations)

### 11.2 SHOULD Implement

- X25519 secure channel establishment
- Role-based access control
- Key rotation mechanisms
- Hardware-backed key storage
- Comprehensive audit logging

### 11.3 MAY Implement

- MLS-based group key agreement
- Hardware attestation (TPM, Secure Enclave)
- PUF-derived device identity
- Zero-knowledge membership proofs
- Access level enforcement

### 11.4 Cryptographic Library Requirements

Implementations MUST use:
- Constant-time comparison for secrets
- Secure random number generation (OS-provided)
- Approved algorithm implementations (audited libraries)

RECOMMENDED libraries:
- Rust: `ed25519-dalek`, `x25519-dalek`, `chacha20poly1305`
- C: libsodium
- Android: Android Keystore + Tink
- iOS: CryptoKit

---

## Appendix A: References

- RFC 8032: Edwards-Curve Digital Signature Algorithm (Ed25519)
- RFC 7748: Elliptic Curves for Security (X25519)
- RFC 8439: ChaCha20 and Poly1305 for IETF Protocols
- RFC 9420: The Messaging Layer Security (MLS) Protocol
- NIST SP 800-57: Key Management Guidelines
- ADR-006: Security Authentication Authorization
- ADR-044: E2E Encryption and Key Management

## Appendix B: Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-07 | Initial draft |

---

