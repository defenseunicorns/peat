# ADR-025: TADIL-J / Link 16 Integration

**Status**: Proposed  
**Date**: 2025-12-10  
**Authors**: Kit Plummer  
**Related ADRs**: 
- [ADR-012](012-schema-definition-protocol-extensibility.md) (Schema Definition & Protocol Extensibility)
- [ADR-020](020-TAK-CoT-Integration.md) (TAK/CoT Integration)
- [ADR-009](009-bidirectional-hierarchical-flows.md) (Bidirectional Hierarchical Flows)
- [ADR-010](010-transport-layer-udp-tcp.md) (Transport Layer)
- [ADR-006](006-security-authentication-authorization.md) (Security, Authentication, Authorization)

## Context

### The Link 16 / TADIL-J Ecosystem

**Link 16** (Tactical Digital Information Link) is NATO's primary tactical data link for command and control, providing jam-resistant, high-capacity communications between military platforms. It operates under the TADIL-J (Tactical Digital Information Link - J series) message standard.

**Key Link 16 Characteristics:**
- **Maturity**: Operational since 1990s, combat-proven across all NATO operations
- **Scale**: Standard across NATO air, land, and sea platforms; 40+ participating nations
- **Architecture**: TDMA (Time Division Multiple Access) with cryptographic COMSEC
- **Bandwidth**: ~115 kbps shared across network (~238,000 bits per 12-second frame)
- **Hardware**: Requires MIDS (Multifunctional Information Distribution System) terminal

**J-Series Message Structure:**
```
┌─────────────────────────────────────────────────────────────┐
│  J-Series Message (TADIL-J)                                 │
├─────────────────────────────────────────────────────────────┤
│  Header: Message Label (J-series type)                      │
│  Word 0: Message identification, time, etc.                 │
│  Word 1-N: Message-specific data fields                     │
│  - Fixed format per message type                            │
│  - 70-bit words (plus 5-bit parity)                         │
└─────────────────────────────────────────────────────────────┘
```

**Critical J-Series Message Types for HIVE Integration:**

| J-Series | Name | Purpose | HIVE Relevance |
|----------|------|---------|----------------|
| J2.2 | Air Track | Air platform position/ID | Air asset SA |
| J3.2 | Surface/Ground Track | Surface/ground position | Squad position aggregation |
| J3.5 | Land Point/Track | Reference points | Control measures |
| J7.0 | Track Management | Track quality/status | Capability status |
| J7.2 | Platform/Track Status | Platform health | Aggregated capability |
| J12.0 | Mission Assignment | Task assignment | Command distribution |
| J12.6 | Control (Vector) | Directional commands | Movement commands |
| J13.2 | Target Designation | Target assignment | Engagement coordination |
| J14.0 | Info Management | Network coordination | Synchronization |

### Strategic Importance for HIVE

**Multi-Domain Integration Requirement:**

Link 16 is the backbone of NATO/coalition tactical networking. Any system coordinating autonomous platforms in joint operations **must** interface with Link 16 to:

1. **Provide SA to Higher Echelons**: Brigade/Division C2 systems (like AFATDS, GCCS-A) consume Link 16 tracks
2. **Receive Tasking**: Mission commands flow down through Link 16 from higher headquarters
3. **Coalition Interoperability**: Allied platforms share SA exclusively via Link 16 in many scenarios
4. **Airspace Integration**: Deconfliction with manned aviation requires Link 16 participation

**The Fundamental Challenge:**

Link 16 is **severely bandwidth-constrained** compared to IP networks:

| Network | Effective Bandwidth | Typical Update Rate |
|---------|--------------------|--------------------|
| Link 16 | ~115 kbps (shared) | 3-12 second cycles |
| TAK/CoT | 1-100 Mbps | 1-5 seconds |
| HIVE internal | 10-1000 Mbps | Sub-second differential |

**Naive bridging would fail catastrophically:**
- 100 platforms × 70-bit position words × 1 Hz = exceeds entire Link 16 capacity
- Each time slot is precious (~7.8125 ms, ~225 bits usable)
- Flat event streaming is physically impossible

**HIVE's hierarchical aggregation is the solution:**
- Squad of 12 platforms → 1 aggregate track
- 95%+ bandwidth reduction through hierarchy
- Natural mapping to military C2 echelons

### Why HIVE Enables This

The question "how would HIVE bridge on-platform messaging to TADIL-J?" has a compelling answer:

> **HIVE's hierarchical tiers create natural aggregation points that map directly to Link 16 network participation models. The tier leader—which already maintains aggregated subordinate state via CRDT synchronization—serves as the bridge point, presenting consolidated SA to the tactical data link rather than raw platform telemetry.**

This is not a workaround; it's the architecture working as intended.

## Decision

We will implement **TADIL-J/Link 16 integration as a transport adapter** within the HIVE architecture, following these principles:

### Principle 1: Hierarchy as Bridge Architecture

HIVE tier boundaries serve as natural Link 16 integration points:

```
┌─────────────────────────────────────────────────────────────┐
│              Link 16 Network (TADIL-J)                      │
│         (J-series messages, TDMA time slots)                │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          │ J3.2 (Aggregated Squad Track)
                          │ J7.2 (Squad Capability Status)
                          │ J12.x (Commands ↓)
                          │
              ┌───────────▼───────────┐
              │   HIVE Tier Leader    │
              │   (Link 16 Bridge)    │
              │                       │
              │  • MIDS Terminal      │
              │  • Aggregation Logic  │
              │  • J-Series Codec     │
              └───────────┬───────────┘
                          │
          ┌───────────────┼───────────────┐
          │               │               │
    ┌─────▼─────┐   ┌─────▼─────┐   ┌─────▼─────┐
    │ Platform 1│   │ Platform 2│   │ Platform 3│
    │           │   │           │   │           │
    │ ROS2/DDS  │   │ ROS2/DDS  │   │ ROS2/DDS  │
    │ CAN Bus   │   │ CAN Bus   │   │ CAN Bus   │
    │ Sensors   │   │ Sensors   │   │ Sensors   │
    └───────────┘   └───────────┘   └───────────┘
         │               │               │
         └───────────────┴───────────────┘
                         │
              HIVE CRDT Sync (Rich State)
              - Full telemetry
              - Detailed capabilities  
              - Sub-second updates
```

**Key Insight**: The tier leader already has the aggregated view. Link 16 bridging is an *output format*, not a separate aggregation step.

### Principle 2: Schema Layer Handles J-Series Encoding

Following ADR-012's separation of concerns, J-series message encoding/decoding belongs in `hive-schema`:

```
hive-schema/
├── proto/
│   ├── hive_core.proto           # Core HIVE messages
│   ├── cot_bridge.proto          # CoT mappings (ADR-020)
│   └── tadil_j_bridge.proto      # J-series mappings (this ADR)
├── src/
│   ├── tadil_j/
│   │   ├── mod.rs
│   │   ├── encoder.rs            # HIVE → J-series
│   │   ├── decoder.rs            # J-series → HIVE
│   │   ├── j_series_types.rs     # J2, J3, J7, J12, J13, J14
│   │   ├── track_number.rs       # JU Track Number management
│   │   └── validation.rs         # MIL-STD-6016 compliance
```

**Bidirectional Message Mapping:**

| HIVE Concept | J-Series Message | Direction | Notes |
|--------------|------------------|-----------|-------|
| Platform position (individual) | J3.2 (surface track) | HIVE → Link 16 | Only if full fidelity needed |
| **Squad aggregate position** | **J3.2 (surface track)** | **HIVE → Link 16** | **Primary use case** |
| Squad capability summary | J7.2 (platform status) | HIVE → Link 16 | Aggregated readiness |
| Air contact detection | J2.2 (air track) | HIVE → Link 16 | Sensor fusion result |
| Target designation | J13.2 (target) | HIVE → Link 16 | Engagement handoff |
| Control measure / ROZ | J3.5 (land point) | Bidirectional | Operational boundaries |
| Mission tasking | J12.0 (mission assignment) | Link 16 → HIVE | Higher HQ commands |
| Movement vector | J12.6 (control) | Link 16 → HIVE | Directional commands |
| Network time sync | J14.0 (info management) | Link 16 → HIVE | Time reference |

### Principle 3: Transport Adapter in hive-transport

The Link 16 interface is a transport adapter, not core protocol logic:

```rust
// hive-transport/src/link16_transport.rs

use hive_schema::tadil_j::{JSeriesEncoder, JSeriesDecoder, JMessage};

pub struct Link16Transport {
    config: Link16Config,
    mids_interface: MidsInterface,
    encoder: JSeriesEncoder,
    decoder: JSeriesDecoder,
    track_manager: TrackNumberManager,
}

pub struct Link16Config {
    /// MIDS terminal configuration
    terminal_id: u16,
    
    /// Assigned time slots (NPG membership)
    time_slot_assignments: Vec<TimeSlot>,
    
    /// Network Participating Group assignments
    npg_memberships: Vec<NpgMembership>,
    
    /// Track number block allocation
    track_number_block: TrackNumberBlock,
    
    /// Aggregation policy for outbound messages
    aggregation_policy: Link16AggregationPolicy,
    
    /// COMSEC key material reference (not stored here)
    comsec_key_id: KeyId,
}

pub enum Link16AggregationPolicy {
    /// Send one track per HIVE tier (recommended)
    TierAggregation,
    
    /// Send tracks for designated platforms only
    DesignatedPlatforms(Vec<PlatformId>),
    
    /// Full fidelity (use sparingly - bandwidth intensive)
    FullFidelity,
}

#[async_trait]
impl TacticalDataLink for Link16Transport {
    /// Publish aggregated HIVE state to Link 16 network
    async fn publish(&self, state: &AggregatedState) -> Result<(), Link16Error> {
        // 1. Convert aggregated state to J-series messages
        let j_messages = self.encoder.encode_aggregated(state, &self.config)?;
        
        // 2. Assign track numbers from allocated block
        let numbered_messages = self.track_manager.assign_numbers(j_messages)?;
        
        // 3. Queue for transmission in assigned time slots
        for msg in numbered_messages {
            self.mids_interface.queue_for_transmission(msg).await?;
        }
        
        Ok(())
    }
    
    /// Receive and process J-series messages from Link 16
    async fn receive(&self) -> Result<Link16Message, Link16Error> {
        let j_message = self.mids_interface.receive().await?;
        
        match j_message.label() {
            JLabel::J12_0 | JLabel::J12_6 => {
                // Command message - decode and return for HIVE processing
                let command = self.decoder.decode_command(&j_message)?;
                Ok(Link16Message::Command(command))
            }
            JLabel::J3_2 | JLabel::J2_2 => {
                // External track - add to SA picture
                let track = self.decoder.decode_track(&j_message)?;
                Ok(Link16Message::ExternalTrack(track))
            }
            JLabel::J3_5 => {
                // Control measure - decode as operational boundary
                let control_measure = self.decoder.decode_control_measure(&j_message)?;
                Ok(Link16Message::ControlMeasure(control_measure))
            }
            _ => Ok(Link16Message::Other(j_message)),
        }
    }
}
```

### Principle 4: Track Number Management

Link 16 requires globally unique track numbers (JU numbers) within a network. HIVE bridge nodes must manage track number allocation:

```rust
// hive-transport/src/link16/track_manager.rs

pub struct TrackNumberManager {
    /// Allocated block of track numbers for this unit
    allocated_block: TrackNumberBlock,
    
    /// Currently assigned track numbers
    assignments: HashMap<HiveEntityId, JuTrackNumber>,
    
    /// Track number recycling (stale tracks)
    stale_threshold: Duration,
}

pub struct TrackNumberBlock {
    /// Starting track number in allocated block
    start: u16,
    
    /// Number of track numbers in block
    count: u16,
    
    /// Unit identifier for this block
    source_unit: SourceTrackId,
}

impl TrackNumberManager {
    /// Assign track number to HIVE entity (squad, platform, etc.)
    pub fn assign(&mut self, entity_id: HiveEntityId) -> Result<JuTrackNumber, TrackError> {
        // Check if already assigned
        if let Some(existing) = self.assignments.get(&entity_id) {
            return Ok(*existing);
        }
        
        // Find available track number in block
        let track_num = self.find_available()?;
        self.assignments.insert(entity_id, track_num);
        
        Ok(track_num)
    }
    
    /// Release track number when entity no longer needs representation
    pub fn release(&mut self, entity_id: &HiveEntityId) {
        self.assignments.remove(entity_id);
    }
}
```

### Principle 5: Aggregation Semantics

The critical architectural decision is **what gets aggregated and how**:

```rust
// hive-core/src/aggregation/link16.rs

/// Aggregation rules for Link 16 representation
pub struct Link16Aggregator {
    echelon: Echelon,
    rules: AggregationRules,
}

pub struct AggregationRules {
    /// Position: centroid, leader position, or weighted center
    position_method: PositionAggregation,
    
    /// Capability: sum, min, max, or custom
    capability_method: CapabilityAggregation,
    
    /// Status: worst-case, average, or leader
    status_method: StatusAggregation,
    
    /// Which subordinate events trigger track updates
    update_triggers: Vec<UpdateTrigger>,
}

pub enum PositionAggregation {
    /// Geometric center of all subordinate positions
    Centroid,
    
    /// Position of designated leader platform
    LeaderPosition,
    
    /// Weighted by capability or role
    WeightedCenter { weights: HashMap<PlatformRole, f32> },
    
    /// Bounding box center with uncertainty ellipse
    BoundingBoxCenter,
}

pub enum CapabilityAggregation {
    /// Sum of subordinate capabilities (e.g., total ammunition)
    Sum,
    
    /// Minimum across subordinates (e.g., fuel = worst case)
    Minimum,
    
    /// Custom aggregation function per capability type
    Custom(Box<dyn Fn(&[Capability]) -> Capability>),
}

impl Link16Aggregator {
    /// Generate J3.2 track representing squad
    pub fn aggregate_to_track(&self, squad: &SquadState) -> J3_2_SurfaceTrack {
        let position = match self.rules.position_method {
            PositionAggregation::Centroid => {
                compute_centroid(&squad.platform_positions)
            }
            PositionAggregation::LeaderPosition => {
                squad.leader_position()
            }
            // ... other methods
        };
        
        let capability_summary = match self.rules.capability_method {
            CapabilityAggregation::Sum => {
                squad.capabilities.iter().fold(Capability::zero(), |a, b| a + b)
            }
            // ... other methods
        };
        
        J3_2_SurfaceTrack {
            position,
            track_quality: compute_track_quality(squad),
            platform_type: PlatformType::GroundUnit,
            specific_type: squad.unit_type.to_link16_code(),
            iff_mode: squad.iff_status,
            strength: squad.platform_count as u8,
            // ... additional fields
        }
    }
}
```

## Schema Impact (Open Standard)

### hive-schema Additions

The HIVE open standard (`hive-schema`) gains J-series message definitions:

```protobuf
// hive-schema/proto/tadil_j_bridge.proto

syntax = "proto3";
package hive.schema.tadil_j;

// J3.2 Surface Track representation
message J3_2_SurfaceTrack {
    // Track identification
    JuTrackNumber track_number = 1;
    SourceTrackId source_id = 2;
    
    // Position (WGS-84)
    Position position = 3;
    Velocity velocity = 4;
    
    // Track quality
    TrackQuality quality = 5;
    
    // Platform identification
    PlatformType platform_type = 6;
    SpecificType specific_type = 7;
    IffStatus iff = 8;
    
    // Force/strength for unit tracks
    uint32 strength = 9;
    
    // Timestamp
    Timestamp time = 10;
}

// J7.2 Platform/Track Status
message J7_2_PlatformStatus {
    JuTrackNumber track_number = 1;
    
    OperationalStatus status = 2;
    FuelState fuel = 3;
    repeated WeaponState weapons = 4;
    SensorState sensors = 5;
    
    Timestamp time = 6;
}

// J12.0 Mission Assignment
message J12_0_MissionAssignment {
    JuTrackNumber assigned_track = 1;
    MissionType mission_type = 2;
    
    // Mission parameters
    oneof mission_params {
        PatrolMission patrol = 10;
        StrikeMission strike = 11;
        ReconMission recon = 12;
        EscortMission escort = 13;
    }
    
    // Constraints
    repeated ControlMeasure constraints = 20;
    Timestamp not_before = 21;
    Timestamp not_after = 22;
}

// Mapping from HIVE entities to J-series representations
message HiveToLink16Mapping {
    string hive_entity_id = 1;
    JuTrackNumber link16_track = 2;
    AggregationLevel level = 3;
    
    enum AggregationLevel {
        PLATFORM = 0;      // Individual platform
        SQUAD = 1;         // Squad aggregate
        PLATOON = 2;       // Platoon aggregate
        COMPANY = 3;       // Company aggregate
    }
}
```

### Canonical Aggregation Definitions

The schema also defines **standard aggregation semantics** that implementations must follow:

```protobuf
// hive-schema/proto/aggregation.proto

// Standard aggregation rules for tactical data link representation
message Link16AggregationProfile {
    string profile_id = 1;
    
    // Position aggregation method
    PositionAggregationMethod position_method = 2;
    
    // How to aggregate capabilities
    map<string, CapabilityAggregationMethod> capability_methods = 3;
    
    // Track update rate limits
    Duration min_update_interval = 4;
    Duration max_stale_time = 5;
}

enum PositionAggregationMethod {
    CENTROID = 0;
    LEADER_POSITION = 1;
    WEIGHTED_CENTER = 2;
    BOUNDING_BOX_CENTER = 3;
}

enum CapabilityAggregationMethod {
    SUM = 0;
    MINIMUM = 1;
    MAXIMUM = 2;
    AVERAGE = 3;
    LEADER_VALUE = 4;
}
```

## Reference Implementation

### hive-transport Link 16 Adapter

The reference implementation provides a complete Link 16 transport adapter:

```
hive-transport/
├── src/
│   ├── lib.rs
│   ├── link16/
│   │   ├── mod.rs
│   │   ├── transport.rs          # Link16Transport implementation
│   │   ├── mids_interface.rs     # MIDS terminal abstraction
│   │   ├── track_manager.rs      # JU track number management
│   │   ├── time_slot.rs          # TDMA time slot handling
│   │   ├── aggregator.rs         # Aggregation logic
│   │   └── simulator.rs          # Simulation mode for testing
│   ├── tak/                      # TAK/CoT adapter (ADR-020)
│   └── ...
```

### MIDS Terminal Abstraction

To support both real hardware and simulation:

```rust
// hive-transport/src/link16/mids_interface.rs

/// Abstraction over MIDS terminal hardware
#[async_trait]
pub trait MidsInterface: Send + Sync {
    /// Queue message for transmission in next available slot
    async fn queue_for_transmission(&self, msg: JSeriesMessage) -> Result<(), MidsError>;
    
    /// Receive next J-series message from network
    async fn receive(&self) -> Result<JSeriesMessage, MidsError>;
    
    /// Get current network time reference
    fn network_time(&self) -> Link16Time;
    
    /// Check terminal status
    fn terminal_status(&self) -> TerminalStatus;
}

/// Real MIDS terminal implementation (requires hardware SDK)
pub struct MidsTerminal {
    // Hardware-specific implementation
    // Would use vendor SDK (e.g., Collins, BAE, L3Harris)
}

/// Simulated MIDS for testing and PoC
pub struct SimulatedMids {
    network: Arc<SimulatedLink16Network>,
    terminal_id: u16,
    rx_queue: mpsc::Receiver<JSeriesMessage>,
    tx_queue: mpsc::Sender<JSeriesMessage>,
}

impl SimulatedMids {
    pub fn new(network: Arc<SimulatedLink16Network>, terminal_id: u16) -> Self {
        let (tx, rx) = network.register_terminal(terminal_id);
        Self {
            network,
            terminal_id,
            rx_queue: rx,
            tx_queue: tx,
        }
    }
}

#[async_trait]
impl MidsInterface for SimulatedMids {
    async fn queue_for_transmission(&self, msg: JSeriesMessage) -> Result<(), MidsError> {
        self.tx_queue.send(msg).await.map_err(|_| MidsError::TransmitFailed)
    }
    
    async fn receive(&self) -> Result<JSeriesMessage, MidsError> {
        self.rx_queue.recv().await.ok_or(MidsError::NetworkDisconnected)
    }
    
    fn network_time(&self) -> Link16Time {
        self.network.current_time()
    }
    
    fn terminal_status(&self) -> TerminalStatus {
        TerminalStatus::Operational
    }
}
```

### Link 16 Network Simulator

For PoC validation without hardware:

```rust
// hive-transport/src/link16/simulator.rs

/// Simulated Link 16 network for testing
pub struct SimulatedLink16Network {
    /// All registered terminals
    terminals: RwLock<HashMap<u16, TerminalConnection>>,
    
    /// Network time reference
    time_base: Instant,
    
    /// Simulated network conditions
    conditions: NetworkConditions,
    
    /// Message log for analysis
    message_log: mpsc::Sender<LoggedMessage>,
}

pub struct NetworkConditions {
    /// Simulated propagation delay
    latency: Duration,
    
    /// Message loss probability (0.0 - 1.0)
    loss_rate: f32,
    
    /// Jamming simulation
    jamming: Option<JammingProfile>,
}

impl SimulatedLink16Network {
    /// Create network with N simulated terminals
    pub fn new(terminal_count: usize) -> Self {
        // Initialize simulated network
    }
    
    /// Register a terminal and get tx/rx channels
    pub fn register_terminal(&self, id: u16) -> (mpsc::Sender<JSeriesMessage>, mpsc::Receiver<JSeriesMessage>) {
        // Create channels for this terminal
    }
    
    /// Inject external traffic (simulated other network participants)
    pub fn inject_external_traffic(&self, messages: Vec<JSeriesMessage>) {
        // Add messages from "other" network participants
    }
    
    /// Get current network time (simulated)
    pub fn current_time(&self) -> Link16Time {
        Link16Time::from_epoch(self.time_base.elapsed())
    }
}
```

## Integration with HIVE Core

### Bridge Node Configuration

A HIVE node configured as a Link 16 bridge:

```rust
// Example: Squad leader with Link 16 bridge capability

let config = HiveNodeConfig {
    role: NodeRole::TierLeader {
        echelon: Echelon::Squad,
        subordinate_count: 12,
    },
    
    transports: vec![
        // Internal HIVE sync
        TransportConfig::Quic(QuicConfig::default()),
        
        // Link 16 bridge (if hardware available)
        TransportConfig::Link16(Link16Config {
            terminal_id: 0x1234,
            time_slot_assignments: vec![/* assigned slots */],
            npg_memberships: vec![
                NpgMembership::SurveillanceNpg,
                NpgMembership::MissionManagementNpg,
            ],
            track_number_block: TrackNumberBlock {
                start: 1000,
                count: 50,
                source_unit: SourceTrackId::from_unit("ALPHA-1"),
            },
            aggregation_policy: Link16AggregationPolicy::TierAggregation,
            comsec_key_id: KeyId::from_slot(1),
        }),
    ],
    
    // Enable bidirectional flow (ADR-009)
    bidirectional_flows: BidirectionalConfig {
        command_acceptance: CommandAcceptance::FromLink16 {
            authorized_sources: vec![/* higher HQ track numbers */],
        },
        ..Default::default()
    },
};

let node = HiveNode::new(config).await?;

// Bridge automatically publishes aggregated state to Link 16
// and injects received commands into HIVE hierarchy
node.run().await?;
```

### Hierarchical Bridge Topology

For larger formations, bridges at multiple echelons:

```
┌─────────────────────────────────────────────────────────────┐
│                    Link 16 Network                          │
└───────┬─────────────────────────────────┬───────────────────┘
        │                                 │
        │ J3.2 (Company aggregate)        │ J3.2 (Platoon aggregates)
        │                                 │
┌───────▼───────┐                 ┌───────▼───────┐
│  Company HQ   │                 │  Platoon HQ   │
│  HIVE Bridge  │                 │  HIVE Bridge  │
│  (Optional)   │                 │  (Primary)    │
└───────┬───────┘                 └───────┬───────┘
        │                                 │
        │ HIVE Protocol                   │ HIVE Protocol
        │ (Platoon aggregates)            │ (Squad details)
        │                                 │
┌───────▼───────┐                 ┌───────▼───────┐
│   Platoons    │                 │    Squads     │
│   (No L16)    │                 │   (No L16)    │
└───────────────┘                 └───────────────┘
```

**Design Rationale:**
- Not every node needs Link 16 hardware (expensive, power-hungry)
- Tier leaders at appropriate echelons serve as bridges
- Higher bridges provide more aggregated views
- Lower bridges provide more detailed views
- Configuration determines which tracks go to Link 16

## Comparison: Link 16 vs TAK/CoT Integration

| Aspect | Link 16 (ADR-025) | TAK/CoT (ADR-020) |
|--------|-------------------|-------------------|
| **Bandwidth** | ~115 kbps shared | 1-100 Mbps |
| **Message Format** | Fixed J-series words | Flexible XML/Protobuf |
| **Aggregation Requirement** | Mandatory | Recommended |
| **Hardware** | MIDS terminal required | Any IP device |
| **Security** | Hardware COMSEC | PKI/TLS |
| **Primary Use** | Joint/coalition SA | Tactical team SA |
| **Latency** | 3-12 second cycles | Sub-second |
| **Ecosystem** | NATO/military only | Military + civilian |

**Complementary Integration:**
- TAK for rich, low-latency team awareness
- Link 16 for joint force integration and higher echelon SA
- Both can operate simultaneously from same HIVE network

## Implementation Phases

### Phase 0: Requirements & Design (Weeks 1-2)

**Goal**: Validate integration requirements with stakeholders

**Tasks**:
1. Survey Link 16 usage patterns in target operational contexts
2. Identify critical J-series message types for HIVE integration
3. Define track number allocation strategy
4. Document MIDS integration requirements (vendor coordination)

**Deliverables**:
- [ ] Link 16 integration requirements document
- [ ] J-series message mapping specification
- [ ] Track number management design
- [ ] MIDS abstraction interface definition

### Phase 1: Schema Layer (Weeks 3-4)

**Goal**: Implement J-series message encoding/decoding in `hive-schema`

**Tasks**:
1. Define J-series protobuf messages
2. Implement J-series encoder (HIVE → Link 16)
3. Implement J-series decoder (Link 16 → HIVE)
4. Define aggregation profile schemas
5. Unit tests for all conversions

**Success Criteria**:
- [ ] J3.2, J7.2, J12.0, J12.6 messages encode/decode correctly
- [ ] Round-trip conversion preserves semantic meaning
- [ ] Validation catches malformed messages
- [ ] MIL-STD-6016 compliance for supported message types

### Phase 2: Transport Adapter (Weeks 5-7)

**Goal**: Implement `Link16Transport` in `hive-transport`

**Tasks**:
1. Implement MIDS interface abstraction
2. Create simulated MIDS for testing
3. Implement track number manager
4. Build Link16Transport with aggregation policies
5. Integration tests with simulated network

**Success Criteria**:
- [ ] SimulatedMids enables full testing without hardware
- [ ] Track numbers assigned and managed correctly
- [ ] Aggregation policies produce expected track representations
- [ ] Bidirectional message flow works (HIVE ↔ Link 16)

### Phase 3: Aggregation Logic (Weeks 8-10)

**Goal**: Implement hierarchical aggregation for Link 16 representation

**Tasks**:
1. Implement position aggregation methods (centroid, leader, weighted)
2. Implement capability aggregation (sum, min, custom)
3. Create configurable aggregation profiles
4. Integrate with HIVE tier leader logic
5. End-to-end testing with multi-tier hierarchy

**Success Criteria**:
- [ ] Squad of 12 platforms produces single J3.2 track
- [ ] Aggregated capability reflects subordinate states
- [ ] Track updates only on meaningful state changes
- [ ] Bandwidth usage within Link 16 constraints

### Phase 4: Network Simulator (Weeks 11-13)

**Goal**: Create comprehensive Link 16 network simulator for validation

**Tasks**:
1. Implement multi-terminal simulated network
2. Add network condition simulation (latency, loss, jamming)
3. Create external traffic injection (simulate other network participants)
4. Build analysis tools for message flow visualization
5. Integration with existing HIVE simulation framework (ADR-008)

**Success Criteria**:
- [ ] Simulate 10+ HIVE nodes with Link 16 bridges
- [ ] Inject realistic external Link 16 traffic
- [ ] Measure and validate bandwidth utilization
- [ ] Demonstrate graceful degradation under jamming

### Phase 5: PoC Integration (Weeks 14-16)

**Goal**: Integrate Link 16 bridging into HIVE PoC demonstration

**Tasks**:
1. Add Link 16 bridge to POI tracking vignette
2. Demonstrate aggregated track generation
3. Demonstrate command reception and distribution
4. Create visualization showing Link 16 representation
5. Document integration patterns

**Success Criteria**:
- [ ] POI tracking vignette shows squad tracks on simulated Link 16
- [ ] Commands injected via Link 16 execute correctly in HIVE
- [ ] Visualization clearly shows aggregation in action
- [ ] Documentation enables third-party integration

### Phase 6: Hardware Integration (Future)

**Goal**: Validate with actual MIDS hardware (requires partnership/facility access)

**Tasks**:
1. Obtain access to MIDS terminal (JITC, vendor lab, or partner)
2. Implement vendor-specific MIDS interface
3. Validate J-series message compliance
4. Test in Link 16 network environment
5. Security accreditation activities

**Success Criteria**:
- [ ] Real MIDS terminal successfully transmits HIVE-generated tracks
- [ ] Received commands correctly processed
- [ ] Interoperability with other Link 16 participants validated
- [ ] Security architecture passes review

## Consequences

### Positive

1. **Joint Force Integration**: HIVE-coordinated platforms visible to entire coalition via Link 16
2. **Higher Echelon SA**: Brigade/Division C2 systems see aggregated HIVE formations
3. **Command Reception**: Higher HQ can task HIVE teams through existing C2 infrastructure
4. **Coalition Interoperability**: Allied platforms share SA with HIVE formations
5. **Airspace Integration**: HIVE air assets properly represented for deconfliction
6. **Validates Core Architecture**: Link 16's constraints prove HIVE's aggregation value
7. **Standards Alignment**: Uses established NATO tactical data link standards
8. **Bandwidth Efficiency**: Hierarchical aggregation enables feasible Link 16 participation

### Negative

1. **Hardware Dependency**: Real-world use requires expensive MIDS terminals
2. **Complexity**: J-series message formats are intricate and rigid
3. **Security Surface**: Link 16 integration adds attack vectors
4. **Testing Difficulty**: Full validation requires specialized facilities
5. **Latency**: Link 16's TDMA cycles add inherent delay (3-12 seconds)
6. **Track Management**: JU track number coordination requires careful design
7. **Limited Bandwidth**: Even with aggregation, Link 16 constrains what's representable

### Risks & Mitigations

**Risk 1**: J-series encoding errors cause interoperability failures
- **Mitigation**: Comprehensive test suite against MIL-STD-6016
- **Mitigation**: Validation mode that checks all outbound messages
- **Mitigation**: Partnership with JITC for certification testing

**Risk 2**: Track number conflicts with other network participants
- **Mitigation**: Track number block allocation coordinated with network authority
- **Mitigation**: Source Track ID uniquely identifies HIVE formations
- **Mitigation**: Track recycling with appropriate stale timers

**Risk 3**: Aggregation loses operationally critical information
- **Mitigation**: Configurable aggregation policies
- **Mitigation**: Ability to expose individual platforms when needed
- **Mitigation**: TAK/CoT integration provides higher-fidelity parallel path

**Risk 4**: Hardware access delays PoC validation
- **Mitigation**: Comprehensive simulator enables all testing without hardware
- **Mitigation**: Schema validation ensures message correctness
- **Mitigation**: Hardware integration treated as separate phase

**Risk 5**: COMSEC integration complexity
- **Mitigation**: MIDS abstraction isolates crypto handling
- **Mitigation**: Simulated mode bypasses COMSEC for development
- **Mitigation**: Security architecture designed for eventual accreditation

## Success Metrics

1. **Aggregation Efficiency**:
   - [ ] 12-platform squad represented as single track
   - [ ] Bandwidth usage ≤ 5% of naive (per-platform) approach
   - [ ] Track updates only on meaningful state changes

2. **Message Compliance**:
   - [ ] J3.2, J7.2, J12.x messages pass MIL-STD-6016 validation
   - [ ] Round-trip encoding preserves all required fields
   - [ ] Interoperability with JITC reference implementation

3. **Operational Integration**:
   - [ ] Commands from Link 16 correctly execute in HIVE (< 5s latency)
   - [ ] HIVE state changes reflected in Link 16 tracks (< 15s latency)
   - [ ] Simulated external participants see correct HIVE representation

4. **PoC Demonstration**:
   - [ ] POI tracking vignette includes Link 16 bridge
   - [ ] Visualization shows aggregation in real-time
   - [ ] Documentation enables reproduction

## Related Standards & Technologies

1. **MIL-STD-6016**: TADIL-J Message Standard
2. **STANAG 5516**: NATO Link 16 Standard
3. **MIL-STD-3011**: Joint Range Extension Applications Protocol (JREAP)
4. **MIDS LVT**: Low Volume Terminal specifications
5. **JITC**: Joint Interoperability Test Command certification

## References

1. MIL-STD-6016E: Tactical Data Link (TDL) 16 Message Standard
2. STANAG 5516: Tactical Data Exchange - Link 16
3. [ADR-020](020-TAK-CoT-Integration.md): TAK/CoT Integration (complementary)
4. [ADR-012](012-schema-definition-protocol-extensibility.md): Schema Definition & Protocol Extensibility
5. [ADR-009](009-bidirectional-hierarchical-flows.md): Bidirectional Hierarchical Flows
6. JITC Link 16 Test Procedures

---

**Author's Note**: This ADR represents a strategic integration that validates HIVE's core architectural thesis: hierarchical aggregation isn't just an optimization—it's the **only** way to participate in bandwidth-constrained tactical networks at scale. While TAK/CoT integration (ADR-020) proves HIVE works with modern IP-based systems, Link 16 integration proves HIVE can bridge to the legacy tactical data link infrastructure that remains the backbone of NATO joint operations. The same hierarchical design that enables 1000+ platform coordination also enables participation in networks designed for dozens of tracks. This is "stop moving data, start moving decisions" made tangible.
