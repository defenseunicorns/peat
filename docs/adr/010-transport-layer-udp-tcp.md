# ADR-010: Transport Layer - UDP vs TCP for PEAT Protocol

**Status**: Superseded by ADR-011 (Automerge + Iroh Integration)
**Date**: 2025-11-05
**Superseded Date**: 2025-11-06
**Authors**: Research Team
**Relates To**: ADR-001, ADR-007, ADR-009, ADR-011

---

## Superseded Notice

**This ADR has been superseded by ADR-011 (CRDT + Networking Stack Selection).**

ADR-011 adopts **Iroh** for networking, which provides:
- **QUIC protocol**: Modern transport with built-in multiplexing, eliminating UDP vs TCP debate
- **Multi-path support**: Simultaneous use of multiple network interfaces (Starlink + MANET + 5G)
- **Connection migration**: Sub-second network switching without reconnection
- **Stream prioritization**: Separate streams for commands vs telemetry (solves head-of-line blocking)
- **Loss tolerance**: Tunable per-stream, better than TCP or UDP alone

**Why QUIC via Iroh is superior to this proposal**:
1. TCP vs UDP is a false choice - QUIC provides benefits of both
2. Multi-path networking requirements demand more than UDP multicast
3. Connection migration critical for tactical environments (network transitions)
4. Stream multiplexing eliminates head-of-line blocking without UDP complexity
5. Iroh provides production-ready peer-to-peer mesh with DHT-based discovery

**This ADR remains for historical context showing the evolution of transport thinking.**

---

## Context and Problem Statement (Original)

The PEAT protocol currently assumes TCP as the primary transport layer (via Tokio TcpStream). However, different types of data in the CAP ecosystem have fundamentally different delivery requirements:

### The Transport Mismatch Problem

**TCP Characteristics:**
- Guaranteed delivery with ordering
- Connection-oriented (handshake overhead)
- Automatic retransmission and congestion control
- Higher latency for real-time data (head-of-line blocking)
- Inefficient for one-to-many distribution

**UDP Characteristics:**
- Best-effort delivery (no guarantees)
- Connectionless (minimal overhead)
- No automatic retransmission
- Lower latency for time-sensitive data
- Native support for multicast/broadcast

### CAP Data Types and Their Natural Transports

| Data Type | Update Frequency | Tolerance for Loss | Natural Transport |
|-----------|------------------|-------------------|-------------------|
| Platform position | 1-10 Hz | High (next update coming) | UDP |
| Sensor detections | 1-10 Hz | Medium (fusion tolerates gaps) | UDP |
| Fuel/battery state | 0.1-1 Hz | Medium (next update coming) | UDP or TCP |
| Capability changes | Event-driven | **Zero** (critical for coordination) | TCP |
| Command/tasking | Event-driven | **Zero** (must be delivered) | TCP |
| Software updates | Rare | **Zero** (integrity critical) | TCP |
| Heartbeats | 1 Hz | High (next beat coming) | UDP |
| CRDT sync messages | Event-driven | **Zero** (convergence required) | TCP |

**Key Insight**: Forcing all data through TCP creates unnecessary latency for time-sensitive telemetry while forcing UDP on critical data would require reimplementing TCP's reliability guarantees.

### The Multicast/Broadcast Opportunity

In hierarchical military structures, certain data naturally flows one-to-many:

**Examples:**
- **Commander's Intent**: Platoon leader broadcasts to all squads
- **Software Updates**: Company HQ multicasts to all platoons
- **Formation Commands**: Squad leader broadcasts to fire team
- **Situational Awareness**: Platform broadcasts position to local peers

**Current TCP Limitation**: Broadcasting to N peers requires N separate TCP connections and N transmissions of identical data.

**UDP Multicast Advantage**: Single packet reaches all subscribers on the same network segment.

## Decision Drivers

### Requirements

1. **Minimize Latency**: High-frequency telemetry should not suffer TCP head-of-line blocking
2. **Guarantee Critical Data**: CRDT sync and commands must be reliable
3. **Efficient Broadcasting**: Software/config distribution should use multicast where possible
4. **Bandwidth Optimization**: Avoid duplicate transmissions for shared data
5. **Application Control**: Let application specify transport requirements per data type
6. **Graceful Degradation**: Support networks that don't allow UDP multicast

### Constraints

1. **Network Reality**: Some tactical networks block UDP or multicast traffic
2. **NAT Traversal**: UDP unicast may be blocked by firewalls/NAT
3. **CRDT Requirements**: Automerge/Ditto sync protocols assume reliable delivery
4. **Implementation Complexity**: Supporting both transports increases code surface area

## Considered Options

### Option 1: TCP-Only (Current Approach)
Continue using only TCP for all data flows.

**Pros:**
- Simplest implementation
- Works on all networks
- Single code path to maintain

**Cons:**
- Suboptimal for high-frequency telemetry
- Inefficient for broadcast scenarios
- Unnecessary latency for lossy-tolerant data

### Option 2: UDP-Only with Reliability Layer
Use UDP exclusively, implementing reliability where needed (like QUIC).

**Pros:**
- Single transport to manage
- Maximum control over reliability/latency tradeoffs

**Cons:**
- Reinventing TCP's reliability mechanisms
- Complex implementation
- Poor NAT traversal

### Option 3: Hybrid Transport with Per-Message Selection (SELECTED)
Support both TCP and UDP, allowing application to specify transport per message type.

**Pros:**
- Optimal transport for each data type
- Native multicast/broadcast support
- Backward compatible (TCP fallback)

**Cons:**
- Increased implementation complexity
- Two code paths to test and maintain

### Option 4: Automatic Transport Selection
System automatically chooses transport based on message characteristics.

**Pros:**
- Application doesn't need to think about transport
- Can optimize based on runtime network conditions

**Cons:**
- Magic behavior can be confusing
- Harder to debug
- May make suboptimal choices

## Decision

**Adopt Option 3: Hybrid Transport with Per-Message Selection**

Extend PEAT protocol to support both UDP and TCP transports, with explicit application control over which transport is used for each message type.

## Design Details

### Transport Type Specification

```rust
/// Transport selection for message delivery
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TransportType {
    /// TCP unicast - reliable, ordered delivery
    TcpUnicast,
    
    /// UDP unicast - best effort, unordered
    UdpUnicast,
    
    /// UDP multicast - one-to-many on same network segment
    UdpMulticast { group_addr: Ipv4Addr },
    
    /// UDP broadcast - one-to-all on subnet
    UdpBroadcast,
}

/// Message priority and transport requirements
#[derive(Debug, Clone)]
pub struct MessageMetadata {
    pub priority: Priority,
    pub transport: TransportType,
    pub ttl_seconds: u32,
    pub compression: bool,
}
```

### Message Classification

Define transport requirements for each CAP message type:

```rust
impl MessageMetadata {
    /// Get metadata for platform telemetry
    pub fn for_telemetry() -> Self {
        Self {
            priority: Priority::P3,
            transport: TransportType::UdpUnicast,
            ttl_seconds: 5,  // Stale after 5 seconds
            compression: true,
        }
    }
    
    /// Get metadata for capability changes (CRDT sync)
    pub fn for_capability_update() -> Self {
        Self {
            priority: Priority::P1,
            transport: TransportType::TcpUnicast,
            ttl_seconds: 300,
            compression: true,
        }
    }
    
    /// Get metadata for formation broadcasts
    pub fn for_formation_command(group: Ipv4Addr) -> Self {
        Self {
            priority: Priority::P2,
            transport: TransportType::UdpMulticast { group_addr: group },
            ttl_seconds: 10,
            compression: false,  // Low latency more important
        }
    }
    
    /// Get metadata for software distribution
    pub fn for_software_package(group: Ipv4Addr) -> Self {
        Self {
            priority: Priority::P4,
            transport: TransportType::TcpUnicast,  // Or multicast for efficiency
            ttl_seconds: 0,  // Never expires
            compression: true,
        }
    }
}
```

### Transport Manager Architecture

```rust
/// Manages multiple transports
pub struct TransportManager {
    tcp_transport: TcpTransport,
    udp_transport: UdpTransport,
    multicast_groups: Arc<RwLock<HashMap<Ipv4Addr, MulticastGroup>>>,
}

impl TransportManager {
    /// Send message using specified transport
    pub async fn send(
        &self,
        message: &[u8],
        metadata: &MessageMetadata,
        target: Option<SocketAddr>,
    ) -> Result<()> {
        match metadata.transport {
            TransportType::TcpUnicast => {
                let addr = target.ok_or(Error::NoTargetSpecified)?;
                self.tcp_transport.send(addr, message).await
            }
            
            TransportType::UdpUnicast => {
                let addr = target.ok_or(Error::NoTargetSpecified)?;
                self.udp_transport.send(addr, message).await
            }
            
            TransportType::UdpMulticast { group_addr } => {
                self.multicast_send(group_addr, message).await
            }
            
            TransportType::UdpBroadcast => {
                self.udp_transport.broadcast(message).await
            }
        }
    }
    
    /// Join multicast group
    pub async fn join_multicast(&self, group: Ipv4Addr) -> Result<()> {
        let multicast = MulticastGroup::join(group)?;
        self.multicast_groups.write().await.insert(group, multicast);
        Ok(())
    }
    
    /// Leave multicast group
    pub async fn leave_multicast(&self, group: Ipv4Addr) -> Result<()> {
        self.multicast_groups.write().await.remove(&group);
        Ok(())
    }
}
```

### UDP Transport Implementation

```rust
pub struct UdpTransport {
    socket: Arc<UdpSocket>,
    recv_buffer: Arc<Mutex<BytesMut>>,
}

impl UdpTransport {
    pub async fn new(bind_addr: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await?;
        Ok(Self {
            socket: Arc::new(socket),
            recv_buffer: Arc::new(Mutex::new(BytesMut::with_capacity(65536))),
        })
    }
    
    /// Send UDP unicast
    pub async fn send(&self, target: SocketAddr, data: &[u8]) -> Result<()> {
        self.socket.send_to(data, target).await?;
        Ok(())
    }
    
    /// Receive UDP packet
    pub async fn recv(&self) -> Result<(Vec<u8>, SocketAddr)> {
        let mut buf = vec![0u8; 65536];
        let (len, addr) = self.socket.recv_from(&mut buf).await?;
        buf.truncate(len);
        Ok((buf, addr))
    }
    
    /// Broadcast to subnet
    pub async fn broadcast(&self, data: &[u8]) -> Result<()> {
        self.socket.set_broadcast(true)?;
        let broadcast_addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
            self.socket.local_addr()?.port(),
        );
        self.socket.send_to(data, broadcast_addr).await?;
        Ok(())
    }
}

/// Multicast group management
pub struct MulticastGroup {
    socket: UdpSocket,
    group_addr: Ipv4Addr,
}

impl MulticastGroup {
    pub fn join(group: Ipv4Addr) -> Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        socket.join_multicast_v4(group, Ipv4Addr::UNSPECIFIED)?;
        Ok(Self {
            socket,
            group_addr: group,
        })
    }
    
    pub async fn send(&self, data: &[u8]) -> Result<()> {
        let addr = SocketAddr::new(IpAddr::V4(self.group_addr), 5000);
        self.socket.send_to(data, addr).await?;
        Ok(())
    }
    
    pub async fn recv(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; 65536];
        let (len, _) = self.socket.recv_from(&mut buf).await?;
        buf.truncate(len);
        Ok(buf)
    }
}
```

### CRDT Sync Integration

Critical: CRDT sync messages **must** use TCP for reliability.

```rust
impl SyncEngine {
    /// Sync document with peer over TCP
    pub async fn sync_document(
        &self,
        doc: &mut Document,
        peer_id: PeerId,
    ) -> Result<()> {
        // Always use TCP for CRDT sync
        let metadata = MessageMetadata {
            priority: Priority::P1,
            transport: TransportType::TcpUnicast,
            ttl_seconds: 300,
            compression: true,
        };
        
        let conn = self.transport_manager
            .get_tcp_connection(peer_id)
            .await?;
        
        // Generate sync message
        let sync_msg = doc.generate_sync_message(&mut self.sync_state)?;
        
        // Send over TCP
        conn.send(&sync_msg).await?;
        
        // Receive response over TCP
        let response = conn.recv().await?;
        doc.receive_sync_message(&response)?;
        
        Ok(())
    }
}
```

### Hierarchical Multicast Groups

Map organizational hierarchy to multicast groups:

```rust
/// Multicast groups for hierarchical organization
pub struct HierarchicalMulticast {
    company_group: Ipv4Addr,      // 239.1.1.1 - All company platforms
    platoon_groups: Vec<Ipv4Addr>, // 239.1.2.X - Per-platoon
    squad_groups: Vec<Ipv4Addr>,   // 239.1.3.X - Per-squad
}

impl HierarchicalMulticast {
    /// Create multicast address scheme
    pub fn new() -> Self {
        Self {
            company_group: Ipv4Addr::new(239, 1, 1, 1),
            platoon_groups: (0..20)
                .map(|i| Ipv4Addr::new(239, 1, 2, i))
                .collect(),
            squad_groups: (0..100)
                .map(|i| Ipv4Addr::new(239, 1, 3, i))
                .collect(),
        }
    }
    
    /// Get multicast group for organizational unit
    pub fn get_group(&self, unit: OrganizationalUnit) -> Ipv4Addr {
        match unit {
            OrganizationalUnit::Company => self.company_group,
            OrganizationalUnit::Platoon(id) => self.platoon_groups[id as usize],
            OrganizationalUnit::Squad(id) => self.squad_groups[id as usize],
        }
    }
}
```

### Software Distribution via Multicast

Example: Distribute AI model update to platoon

```rust
pub async fn distribute_software_package(
    &self,
    package: SoftwarePackage,
    target_unit: OrganizationalUnit,
) -> Result<()> {
    // Get multicast group for target
    let group = self.hierarchy.get_group(target_unit);
    
    // Join multicast group
    self.transport_manager.join_multicast(group).await?;
    
    // Split package into chunks
    let chunks = package.split_into_chunks(1_000_000); // 1MB chunks
    
    for (i, chunk) in chunks.enumerate() {
        let message = PackageChunk {
            package_id: package.id,
            chunk_index: i,
            total_chunks: chunks.len(),
            data: chunk,
        };
        
        let serialized = bincode::serialize(&message)?;
        
        // Multicast chunk to all group members
        let metadata = MessageMetadata {
            priority: Priority::P4,
            transport: TransportType::UdpMulticast { group_addr: group },
            ttl_seconds: 0,
            compression: true,
        };
        
        self.transport_manager.send(&serialized, &metadata, None).await?;
        
        // Rate limit to avoid flooding
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    Ok(())
}
```

### Fallback Mechanism

Support networks that don't allow UDP multicast:

```rust
impl TransportManager {
    /// Send with automatic fallback
    pub async fn send_with_fallback(
        &self,
        message: &[u8],
        metadata: &MessageMetadata,
        targets: &[SocketAddr],
    ) -> Result<()> {
        match metadata.transport {
            TransportType::UdpMulticast { .. } | TransportType::UdpBroadcast => {
                // Try multicast/broadcast first
                match self.send(message, metadata, None).await {
                    Ok(_) => return Ok(()),
                    Err(_) => {
                        // Fall back to TCP unicast to each target
                        warn!("Multicast failed, falling back to TCP unicast");
                        for target in targets {
                            let tcp_metadata = MessageMetadata {
                                transport: TransportType::TcpUnicast,
                                ..metadata.clone()
                            };
                            self.send(message, &tcp_metadata, Some(*target)).await?;
                        }
                    }
                }
            }
            _ => {
                self.send(message, metadata, targets.first().copied()).await?;
            }
        }
        Ok(())
    }
}
```

## Use Cases and Transport Selection

### Use Case 1: Platform Position Updates

**Scenario**: Quadcopter sends position at 10 Hz to squad members

**Transport**: UDP Multicast
- Updates are ephemeral (next one coming in 100ms)
- Loss tolerance is high (sensor fusion handles gaps)
- Low latency critical for real-time SA
- One-to-many distribution (all squad members need it)

```rust
let position = PlatformPosition {
    lat: 37.7749,
    lon: -122.4194,
    alt: 100.0,
    timestamp: Utc::now(),
};

let metadata = MessageMetadata {
    priority: Priority::P3,
    transport: TransportType::UdpMulticast { 
        group_addr: squad_multicast_addr 
    },
    ttl_seconds: 1,
    compression: false,
};

transport.send(&serialize(&position)?, &metadata, None).await?;
```

### Use Case 2: Capability Change (CRDT Update)

**Scenario**: Platform's fuel drops below threshold, capability change must propagate

**Transport**: TCP Unicast
- Zero loss tolerance (CRDT convergence required)
- Ordering matters (Automerge sync protocol)
- Not time-critical (eventual consistency acceptable)

```rust
let capability_delta = CapabilityDelta {
    node_id: self.id,
    changes: vec![
        Change::FuelUpdate { old: 50, new: 20 }
    ],
    timestamp: Utc::now(),
};

let metadata = MessageMetadata::for_capability_update();

// Send to squad leader over TCP
transport.send(
    &serialize(&capability_delta)?, 
    &metadata, 
    Some(squad_leader_addr)
).await?;
```

### Use Case 3: Commander's Intent Broadcast

**Scenario**: Platoon leader issues formation change to all squads

**Transport**: UDP Broadcast (then TCP confirmation)
- Initial broadcast for speed
- TCP confirmation for reliability
- All squads need the command simultaneously

```rust
let command = FormationCommand {
    formation_type: FormationType::Wedge,
    timestamp: Utc::now(),
    command_id: Uuid::new_v4(),
};

// Phase 1: UDP broadcast for immediate awareness
let broadcast_metadata = MessageMetadata {
    priority: Priority::P2,
    transport: TransportType::UdpBroadcast,
    ttl_seconds: 5,
    compression: false,
};

transport.send(&serialize(&command)?, &broadcast_metadata, None).await?;

// Phase 2: TCP unicast to each squad for guaranteed delivery
let tcp_metadata = MessageMetadata {
    priority: Priority::P2,
    transport: TransportType::TcpUnicast,
    ttl_seconds: 60,
    compression: true,
};

for squad_addr in squad_addresses {
    transport.send(&serialize(&command)?, &tcp_metadata, Some(squad_addr)).await?;
}
```

### Use Case 4: Software Distribution

**Scenario**: Distribute 45MB AI model to all platoon platforms

**Transport**: TCP Multicast (if available) or TCP Unicast

```rust
let package = SoftwarePackage::load("model_v4.2.tar.gz")?;

// Try multicast first for efficiency
let multicast_metadata = MessageMetadata {
    priority: Priority::P4,
    transport: TransportType::UdpMulticast { 
        group_addr: platoon_multicast_addr 
    },
    ttl_seconds: 0,
    compression: true,
};

match distribute_package_multicast(&package, &multicast_metadata).await {
    Ok(_) => info!("Package distributed via multicast"),
    Err(_) => {
        // Fallback: Hierarchical TCP distribution
        distribute_package_hierarchical(&package).await?;
    }
}
```

## Network Simulation Updates

Update network simulator to support both transports:

```rust
pub struct NetworkSimulator {
    tcp_simulator: TcpSimulator,
    udp_simulator: UdpSimulator,
    bandwidth_limiter: Arc<Mutex<BandwidthLimiter>>,
}

impl NetworkSimulator {
    pub async fn route_message(
        &self,
        message: &[u8],
        metadata: &MessageMetadata,
        from: NodeId,
        to: Option<NodeId>,
    ) -> Result<()> {
        // Apply bandwidth limiting
        self.bandwidth_limiter.lock().await.consume(message.len())?;
        
        match metadata.transport {
            TransportType::TcpUnicast => {
                self.tcp_simulator.route(message, from, to.unwrap()).await
            }
            TransportType::UdpUnicast => {
                // Simulate packet loss
                if self.should_drop_packet() {
                    return Ok(()); // Silently drop
                }
                self.udp_simulator.route(message, from, to.unwrap()).await
            }
            TransportType::UdpMulticast { group_addr } => {
                // Simulate to all group members
                let members = self.get_multicast_members(group_addr);
                for member in members {
                    if !self.should_drop_packet() {
                        self.udp_simulator.route(message, from, member).await?;
                    }
                }
                Ok(())
            }
            TransportType::UdpBroadcast => {
                // Simulate to all nodes in subnet
                let subnet_members = self.get_subnet_members(from);
                for member in subnet_members {
                    if !self.should_drop_packet() {
                        self.udp_simulator.route(message, from, member).await?;
                    }
                }
                Ok(())
            }
        }
    }
}
```

## Performance Implications

### Bandwidth Savings (Example Scenario: 100 Platforms)

**Telemetry Distribution (10 Hz position updates):**

TCP Unicast (all-to-all):
```
100 platforms × 99 connections × 500B × 10 Hz = 49.5 MB/s
```

UDP Multicast (squad-based):
```
100 platforms / 5 platforms per squad = 20 squads
20 multicast groups × 500B × 10 Hz = 100 KB/s
495x bandwidth reduction
```

**Software Distribution (45MB package to 100 platforms):**

TCP Unicast Sequential:
```
45 MB × 100 platforms = 4.5 GB
On 1 Mbps link: 600 minutes (10 hours)
```

UDP Multicast (single transmission):
```
45 MB × 1 transmission = 45 MB
On 1 Mbps link: 6 minutes
100x time reduction
```

### Latency Improvements

**Position Update Propagation:**

TCP (with head-of-line blocking):
```
Baseline latency: 100ms
+ Queue delay (congested): 50-200ms
+ Retransmission (3% loss): 100-500ms
Total: 250-800ms
```

UDP (no blocking):
```
Baseline latency: 100ms
No retransmission
Total: 100ms (but 3% loss)
```

For position updates where loss is acceptable, UDP provides 2.5-8x latency reduction.

## Migration Path

### Phase 1: UDP Unicast Support (Week 1-2)
- Implement `UdpTransport` struct
- Add transport selection to `MessageMetadata`
- Update telemetry to use UDP
- Validate packet loss tolerance

### Phase 2: Multicast Support (Week 3-4)
- Implement `MulticastGroup` management
- Create hierarchical multicast addressing scheme
- Convert position broadcasts to multicast
- Measure bandwidth savings

### Phase 3: TCP Reliability Layer (Week 5-6)
- Ensure CRDT sync stays on TCP
- Add fallback mechanisms (multicast → TCP)
- Test network partition scenarios

### Phase 4: Broadcast Commands (Week 7-8)
- Implement formation command broadcasting
- Add UDP + TCP confirmation pattern
- Validate in network simulator

## Testing Strategy

### Unit Tests
- UDP send/receive correctness
- Multicast group join/leave
- Transport selection logic
- Fallback mechanism

### Integration Tests
- Mixed UDP/TCP message flows
- Multicast distribution trees
- Packet loss simulation
- Bandwidth measurement

### Performance Tests
- Latency comparison (UDP vs TCP)
- Bandwidth usage (multicast vs unicast)
- Scale test (100+ platforms with mixed traffic)

## Risks and Mitigations

### Risk 1: Network Environments Block UDP
**Impact**: High (protocol won't work in some networks)
**Mitigation**: 
- Implement automatic TCP fallback
- Make UDP optional in configuration
- Document network requirements clearly

### Risk 2: Multicast Not Supported
**Impact**: Medium (lose bandwidth optimization)
**Mitigation**:
- Fallback to repeated TCP unicast
- Hierarchical distribution reduces impact
- Document multicast as optional optimization

### Risk 3: UDP Packet Loss Exceeds Tolerance
**Impact**: Medium (stale telemetry)
**Mitigation**:
- Configurable loss tolerance per message type
- Monitor packet loss rates
- Adaptive transport selection based on measured loss

### Risk 4: Debugging Complexity
**Impact**: Medium (harder to trace message flows)
**Mitigation**:
- Comprehensive logging with transport type tags
- Network visualization tool
- Packet capture integration

## Success Metrics

1. **Latency Reduction**: Position updates propagate in <150ms (vs 300ms+ with TCP)
2. **Bandwidth Savings**: 10x reduction for telemetry via multicast
3. **Software Distribution**: 50x faster distribution of large packages
4. **Reliability**: CRDT sync maintains 100% delivery over TCP
5. **Graceful Degradation**: Automatic fallback works in 100% of test cases

## References

1. ADR-001: PEAT Protocol POC Architecture
2. ADR-007: Automerge-Based Sync Engine
3. ADR-009: Bidirectional Hierarchical Flows
4. RFC 768: User Datagram Protocol
5. RFC 1112: Host Extensions for IP Multicasting
6. RFC 3376: Internet Group Management Protocol (IGMP)
7. "Reliable Multicast Transport Protocol" - IETF RFC 3940

## Appendix: Transport Decision Matrix

| Message Type | Frequency | Loss Tolerance | Latency Requirement | Transport |
|--------------|-----------|----------------|---------------------|-----------|
| Position | 10 Hz | High (90%) | <200ms | UDP Multicast |
| Velocity | 10 Hz | High (90%) | <200ms | UDP Multicast |
| Orientation | 10 Hz | High (90%) | <200ms | UDP Multicast |
| Sensor Detection | 1-10 Hz | Medium (50%) | <500ms | UDP Unicast |
| Fuel/Battery | 0.1 Hz | Medium (10%) | <5s | UDP Unicast |
| Health Status | 0.1 Hz | Low (1%) | <10s | TCP Unicast |
| Capability Change | Event | Zero | <5s | TCP Unicast |
| CRDT Sync | Event | Zero | <5s | TCP Unicast |
| Command/Tasking | Event | Zero | <1s | UDP Broadcast + TCP Confirm |
| Software Package | Rare | Zero | <30min | TCP Multicast (fallback TCP) |
| Heartbeat | 1 Hz | High (95%) | <2s | UDP Unicast |
| Formation Command | Event | Low (5%) | <1s | UDP Broadcast + TCP Confirm |

---

**Last Updated**: 2025-11-05  
**Next Review**: After Phase 2 implementation
