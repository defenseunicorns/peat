# Peat Protocol Specification: Transport Layer

**Spec ID**: Peat-SPEC-001
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: Defense Unicorns

## Abstract

This document specifies the transport layer for the Peat Protocol. It defines wire formats, connection lifecycle, transport abstractions, and the UDP bypass channel for latency-critical applications.

## Table of Contents

1. [Introduction](#1-introduction)
2. [Terminology](#2-terminology)
3. [Transport Abstraction](#3-transport-abstraction)
4. [Primary Transport: QUIC/Iroh](#4-primary-transport-quiciroh)
5. [UDP Bypass Channel](#5-udp-bypass-channel)
6. [Peat-Lite Protocol](#6-peat-lite-protocol)
7. [BLE Mesh Transport](#7-ble-mesh-transport)
8. [Connection Lifecycle](#8-connection-lifecycle)
9. [Wire Formats](#9-wire-formats)
10. [Security Considerations](#10-security-considerations)
11. [IANA Considerations](#11-iana-considerations)

---

## 1. Introduction

### 1.1 Purpose

The Peat transport layer provides reliable and unreliable message delivery across heterogeneous network environments. It abstracts multiple physical transports (QUIC, UDP, BLE) behind a common interface, enabling applications to function regardless of underlying connectivity.

### 1.2 Scope

This specification covers:
- Transport trait abstraction
- QUIC-based primary transport (via Iroh)
- UDP bypass channel for low-latency data
- Peat-Lite protocol for constrained devices
- BLE mesh transport for mobile/embedded devices
- Connection establishment and teardown
- Wire format encoding

### 1.3 Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Node** | A Peat-capable device with a unique identity |
| **Peer** | A node with an established transport connection |
| **Endpoint** | A network address (IP:port, BLE address, etc.) |
| **Channel** | A logical stream within a transport connection |
| **Bypass** | Low-latency UDP path that skips CRDT synchronization |
| **Cell** | A group of nodes coordinating together |

---

## 3. Transport Abstraction

### 3.1 Transport Trait

All Peat transports MUST implement the following interface:

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

The primary Peat transport uses QUIC via the Iroh library. QUIC provides:
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
        |-------- Peat Handshake ---------->|
        |  (DeviceId, FormationId, Nonce)   |
        |                                   |
        |<------- Peat HandshakeAck --------|
        |  (DeviceId, Challenge)            |
        |                                   |
        |-------- ChallengeResponse ------->|
        |  (Signature)                      |
        |                                   |
        |<------- ConnectionReady ----------|
        |                                   |
```

### 4.3 Peat Handshake Message

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

For broadcast scenarios, Peat supports IP multicast:

| Multicast Group | Purpose |
|-----------------|---------|
| `239.255.72.86` | Default Peat multicast group |
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

## 6. Peat-Lite Protocol

### 6.1 Purpose

Peat-Lite is a lightweight UDP protocol for resource-constrained devices (ESP32, ARM Cortex-M) that cannot run the full QUIC stack.

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

Peat-Lite uses a simple acknowledgment scheme:
- Sender transmits with sequence number (0-255, wraparound)
- Receiver sends Ack/Nack within timeout (default: 100ms)
- Sender retries up to 3 times before marking peer as failed

---

## 7. BLE Mesh Transport

### 7.1 Overview

The BLE mesh transport (`peat-btle`) enables Peat communication over Bluetooth Low Energy for mobile and embedded devices.

### 7.2 GATT Service

| UUID | Name | Description |
|------|------|-------------|
| `0x1826` | Peat Mesh Service | Primary service |
| `0x2A6E` | Data Characteristic | Read/Write/Notify |
| `0x2A6F` | Control Characteristic | Write only |

### 7.3 Advertising

Peat nodes advertise with:
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
| 4435 | UDP | Peat-Lite |

### 11.2 Multicast Groups

| Group | Purpose |
|-------|---------|
| 239.255.72.86-88 | Peat protocol multicast |

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
