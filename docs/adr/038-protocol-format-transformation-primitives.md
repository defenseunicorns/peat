# ADR-026: Protocol-Level Format Transformation Primitives

**Status**: Proposed  
**Date**: 2025-12-11  
**Authors**: Kit Plummer, Codex  
**Extends**: ADR-012 (Schema Definition & Protocol Extensibility), ADR-020 (TAK/CoT Integration)  
**Relates To**: ADR-025 (Resource-Constrained Device Optimization)

## Context

### The Problem: Transformation as Afterthought

ADR-020 defines TAK/CoT integration as a transport adapter pattern - CoT encoding/decoding happens in the `cap-schema` crate and bridging logic lives in `cap-protocol`. This works, but treats format transformation as an **application-level concern** rather than a **protocol primitive**.

For the WearTAK use case (ADR-025), this creates friction:

```
Current Architecture Problem:

  Samsung Watch            ATAK Phone              TAK Server
  ┌─────────────┐         ┌─────────────┐         ┌─────────────┐
  │  WearTAK    │   BLE   │    ATAK     │   TCP   │  TAK Server │
  │  (CoT)      │◄───────►│   (CoT)     │◄───────►│   (CoT)     │
  └─────────────┘         └─────────────┘         └─────────────┘
                          │
                          │ ??? Integration Point
                          ▼
                    ┌─────────────┐
                    │ PEAT Node   │
                    │ (CRDT docs) │
                    └─────────────┘

Questions:
1. Where does PEAT ↔ CoT transformation happen?
2. Who owns the transformation logic?
3. How does a WearTAK device join a PEAT hierarchy?
4. Can a node speak CoT natively without full PEAT stack?
```

### The Deeper Issue: Format Interoperability at Scale

PEAT will integrate with multiple format ecosystems:

| Format | Use Case | Characteristics |
|--------|----------|-----------------|
| **CoT/TAK** | ATAK/WinTAK/WearTAK | XML/Protobuf, event-based, position-centric |
| **Link 16** | JTIDS/MIDS, air/naval | Fixed message formats (J-series), TDMA slots |
| **VMF** | Ground forces, fire support | Variable Message Format, rich C2 semantics |
| **JREAP** | Long-haul relay | J-series over IP, NATO standard |
| **DDS/RTPS** | ROS2 robotics | Topic-based pub/sub, QoS-aware |
| **MQTT** | IoT sensors | Lightweight pub/sub, constrained devices |
| **OGC SensorML** | Environmental sensing | Standards-based sensor description |

Each requires bidirectional transformation. If transformation remains an ad-hoc concern, we get:
- **N×M integration matrix** - every format pair needs custom code
- **No composability** - can't chain PEAT → CoT → Link 16
- **No negotiation** - nodes can't discover compatible formats
- **No optimization** - constrained devices can't skip unused transforms

### WearTAK Integration Requirements

For Samsung watches running WearTAK to participate in PEAT hierarchies:

1. **Minimal footprint** - Watch can't run full PEAT stack
2. **CoT native** - WearTAK already speaks CoT to ATAK
3. **Hierarchy participation** - Watch is a leaf node in PEAT hierarchy
4. **Bidirectional** - Receive commands, send position/health/alerts
5. **Battery efficient** - Transform overhead must be minimal

**Key Insight**: The watch shouldn't need to understand PEAT's CRDT internals. It should speak CoT, and PEAT should accept CoT as a valid input format at the protocol level.

## Decision

### Core Principle: Format Adapters as Protocol Primitives

We introduce **Format Adapters** as first-class protocol elements:

```
┌─────────────────────────────────────────────────────────────────┐
│                    PEAT Protocol Stack                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              Application Layer                            │  │
│  │  • Mission logic, coordination, AI/ML                     │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              Document Layer (CRDT)                        │  │
│  │  • Automerge documents, sync state                        │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │         ★ FORMAT ADAPTER LAYER ★ (NEW)                    │  │
│  │  • Schema-declared transformations                        │  │
│  │  • Bidirectional PEAT ↔ External format mapping          │  │
│  │  • Format capability advertisement                        │  │
│  │  • Automatic format negotiation                           │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              Transport Layer                              │  │
│  │  • Iroh/QUIC, TCP, UDP, BLE, LoRa                        │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Format Adapter Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                  Format Adapter Registry                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
│  │ CoT Adapter │  │ DDS Adapter │  │ MQTT Adapter│  ...        │
│  │             │  │             │  │             │             │
│  │ • encode()  │  │ • encode()  │  │ • encode()  │             │
│  │ • decode()  │  │ • decode()  │  │ • decode()  │             │
│  │ • schema    │  │ • schema    │  │ • schema    │             │
│  │ • validate  │  │ • validate  │  │ • validate  │             │
│  └─────────────┘  └─────────────┘  └─────────────┘             │
│         │                │                │                     │
│         └────────────────┴────────────────┘                     │
│                          │                                      │
│                          ▼                                      │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              Canonical PEAT Document Model                │  │
│  │  • Position, Health, Capability, Command, Alert           │  │
│  │  • Hierarchical relationships (parent, children)          │  │
│  │  • CRDT-native (Automerge compatible)                     │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Schema-Declared Transformations

Transformations are **declared in schema**, not just implemented in code:

```protobuf
// peat_format_adapters.proto

// Format adapter capability declaration
message FormatAdapterCapability {
  string format_id = 1;           // "cot-xml", "cot-protobuf", "dds-rtps"
  string format_version = 2;       // "2.0", "3.0"
  repeated string supported_types = 3;  // ["position", "track", "alert"]
  TransformDirection direction = 4;
  uint32 priority = 5;            // Preference order
}

enum TransformDirection {
  BIDIRECTIONAL = 0;
  ENCODE_ONLY = 1;   // PEAT → External
  DECODE_ONLY = 2;   // External → PEAT
}

// Schema-level type mapping declaration
message TypeMapping {
  string peat_type = 1;           // "peat.Position"
  string external_type = 2;        // "cot.event[a-f-G]"
  string transform_id = 3;         // Reference to transform implementation
  FieldMappingSet field_mappings = 4;
  LossInfo loss_info = 5;          // What's lost in translation
}

message FieldMappingSet {
  repeated FieldMapping mappings = 1;
}

message FieldMapping {
  string peat_field = 1;           // "latitude"
  string external_field = 2;       // "point/@lat"
  FieldTransform transform = 3;    // Optional field-level transform
}

message FieldTransform {
  oneof transform_type {
    IdentityTransform identity = 1;
    ScaleTransform scale = 2;       // Unit conversion
    EnumMapping enum_map = 3;       // Enum value mapping
    ExpressionTransform expr = 4;   // Simple expression
    CustomTransform custom = 5;     // Reference to code
  }
}

// Declare what information is lost in transformation
message LossInfo {
  repeated string peat_fields_not_mapped = 1;   // PEAT fields with no external equivalent
  repeated string external_fields_ignored = 2;  // External fields we don't import
  string semantic_notes = 3;                    // Human-readable loss description
}
```

### CoT Format Adapter (Reference Implementation)

```rust
/// CoT Format Adapter - Reference implementation for TAK ecosystem
pub struct CotFormatAdapter {
    /// Supported CoT protocol versions
    supported_versions: Vec<CotVersion>,
    
    /// Type mappings (PEAT type → CoT type code)
    type_mappings: HashMap<PeatType, CotTypeMapping>,
    
    /// Field-level transformations
    field_transforms: FieldTransformRegistry,
    
    /// Schema validation
    validator: CotSchemaValidator,
    
    /// Encoding options
    encoding: CotEncoding,
}

#[derive(Clone, Copy)]
pub enum CotVersion {
    Xml2_0,        // Standard XML
    TakProtobuf,   // TAK Protocol Version 1 (mesh SA)
    StreamingV1,   // TAK streaming format
}

#[derive(Clone, Copy)]
pub enum CotEncoding {
    Xml,
    Protobuf,
    ProtobufCompressed,  // zlib compressed
}

impl FormatAdapter for CotFormatAdapter {
    fn format_id(&self) -> &str {
        "cot"
    }
    
    fn supported_versions(&self) -> &[&str] {
        &["2.0", "tak-proto-1"]
    }
    
    fn capabilities(&self) -> FormatAdapterCapability {
        FormatAdapterCapability {
            format_id: "cot".into(),
            format_version: "2.0".into(),
            supported_types: vec![
                "position", "track", "alert", "chat", 
                "mission", "geofence", "sensor"
            ],
            direction: TransformDirection::Bidirectional,
            priority: 100,
        }
    }
    
    /// Encode PEAT document to CoT event
    fn encode(&self, doc: &PeatDocument) -> Result<EncodedMessage, TransformError> {
        let cot_event = match doc.doc_type() {
            PeatDocType::Position => self.encode_position(doc)?,
            PeatDocType::Track => self.encode_track(doc)?,
            PeatDocType::Alert => self.encode_alert(doc)?,
            PeatDocType::Capability => self.encode_capability(doc)?,
            PeatDocType::Command => self.encode_command(doc)?,
            _ => return Err(TransformError::UnsupportedType(doc.doc_type())),
        };
        
        // Serialize based on encoding preference
        let bytes = match self.encoding {
            CotEncoding::Xml => cot_event.to_xml()?,
            CotEncoding::Protobuf => cot_event.to_protobuf()?,
            CotEncoding::ProtobufCompressed => {
                let pb = cot_event.to_protobuf()?;
                compress_zlib(&pb)?
            }
        };
        
        Ok(EncodedMessage {
            format: self.format_id().into(),
            encoding: self.encoding.to_string(),
            payload: bytes,
            metadata: self.build_metadata(&cot_event),
        })
    }
    
    /// Decode CoT event to PEAT document
    fn decode(&self, msg: &EncodedMessage) -> Result<PeatDocument, TransformError> {
        // Parse CoT event
        let cot_event = self.parse_cot(&msg.payload, &msg.encoding)?;
        
        // Validate against CoT schema
        self.validator.validate(&cot_event)?;
        
        // Map to PEAT document based on CoT type
        let peat_doc = match self.classify_cot_type(&cot_event.event_type) {
            CotClassification::Position => self.decode_position(&cot_event)?,
            CotClassification::Track => self.decode_track(&cot_event)?,
            CotClassification::Alert => self.decode_alert(&cot_event)?,
            CotClassification::Chat => self.decode_chat(&cot_event)?,
            CotClassification::Mission => self.decode_mission(&cot_event)?,
            CotClassification::Geofence => self.decode_geofence(&cot_event)?,
            CotClassification::Unknown => {
                // Preserve as opaque CoT event for passthrough
                self.decode_opaque(&cot_event)?
            }
        };
        
        Ok(peat_doc)
    }
    
    /// Validate external message without full decode
    fn validate(&self, msg: &EncodedMessage) -> Result<ValidationResult, TransformError> {
        let cot_event = self.parse_cot(&msg.payload, &msg.encoding)?;
        self.validator.validate(&cot_event)
    }
}
```

### Type Mapping: PEAT ↔ CoT

```rust
/// PEAT to CoT type mapping definitions
impl CotFormatAdapter {
    fn init_type_mappings() -> HashMap<PeatType, CotTypeMapping> {
        let mut mappings = HashMap::new();
        
        // Position → CoT Event (friendly ground unit)
        mappings.insert(PeatType::Position, CotTypeMapping {
            cot_type_template: "a-{affiliation}-G-{dimension}-{function}",
            default_type: "a-f-G-U-C",  // friendly-ground-unit-combat
            field_mappings: vec![
                FieldMapping::direct("latitude", "point/@lat"),
                FieldMapping::direct("longitude", "point/@lon"),
                FieldMapping::direct("altitude", "point/@hae"),
                FieldMapping::direct("accuracy_h", "point/@ce"),
                FieldMapping::direct("accuracy_v", "point/@le"),
                FieldMapping::direct("timestamp", "@time"),
                FieldMapping::direct("node_id", "@uid"),
                FieldMapping::expression("stale_time", "@time + ttl_seconds"),
                FieldMapping::custom("how", encode_position_source),
            ],
            detail_mappings: vec![
                DetailMapping::nested("health", "detail/__group", encode_health_detail),
                DetailMapping::nested("track", "detail/track", encode_track_detail),
            ],
        });
        
        // Alert → CoT Event (emergency/alert type)
        mappings.insert(PeatType::Alert, CotTypeMapping {
            cot_type_template: "b-{alert_class}-{severity}",
            default_type: "b-a",  // alert atom
            field_mappings: vec![
                FieldMapping::direct("latitude", "point/@lat"),
                FieldMapping::direct("longitude", "point/@lon"),
                FieldMapping::direct("alert_id", "@uid"),
                FieldMapping::direct("timestamp", "@time"),
                FieldMapping::custom("alert_type", encode_alert_type),
            ],
            detail_mappings: vec![
                DetailMapping::nested("message", "detail/remarks", encode_remarks),
                DetailMapping::nested("priority", "detail/priority", encode_priority),
            ],
        });
        
        // Capability → CoT Event with custom detail schema
        mappings.insert(PeatType::Capability, CotTypeMapping {
            cot_type_template: "a-f-G-U-C",  // Unit with capability detail
            default_type: "a-f-G-U-C",
            field_mappings: vec![
                FieldMapping::direct("node_id", "@uid"),
                FieldMapping::direct("timestamp", "@time"),
            ],
            detail_mappings: vec![
                DetailMapping::nested("capabilities", "detail/peat_capabilities", 
                    encode_capability_detail),
                DetailMapping::nested("hierarchy", "detail/peat_hierarchy",
                    encode_hierarchy_detail),
            ],
        });
        
        // Track (aggregated positions) → CoT track message
        mappings.insert(PeatType::Track, CotTypeMapping {
            cot_type_template: "a-{affiliation}-{dimension}-{function}",
            default_type: "a-u-G",  // unknown ground track
            field_mappings: vec![
                FieldMapping::direct("track_id", "@uid"),
                FieldMapping::direct("last_position.latitude", "point/@lat"),
                FieldMapping::direct("last_position.longitude", "point/@lon"),
                FieldMapping::direct("last_position.timestamp", "@time"),
                FieldMapping::custom("affiliation", infer_affiliation),
            ],
            detail_mappings: vec![
                DetailMapping::nested("track_history", "detail/track", encode_track_history),
                DetailMapping::nested("velocity", "detail/track/@speed", encode_velocity),
                DetailMapping::nested("course", "detail/track/@course", encode_course),
            ],
        });
        
        mappings
    }
}
```

### Format Capability Advertisement

Nodes advertise their format capabilities as part of PEAT discovery:

```protobuf
// Extend beacon to include format capabilities
message PeatBeacon {
  string node_id = 1;
  Position position = 2;
  NodeHealth health = 3;
  repeated Capability capabilities = 4;
  
  // NEW: Format adapter capabilities
  repeated FormatAdapterCapability format_adapters = 10;
}

// Example beacon with CoT capability
// {
//   "node_id": "watch-001",
//   "format_adapters": [
//     {
//       "format_id": "cot",
//       "format_version": "2.0",
//       "supported_types": ["position", "alert"],
//       "direction": "BIDIRECTIONAL",
//       "priority": 100
//     }
//   ]
// }
```

### Format Negotiation Protocol

When nodes establish connections, they negotiate compatible formats:

```rust
/// Format negotiation during connection establishment
pub struct FormatNegotiator {
    local_adapters: Vec<FormatAdapterCapability>,
}

impl FormatNegotiator {
    /// Negotiate best format for communication with peer
    pub fn negotiate(&self, peer_capabilities: &[FormatAdapterCapability]) 
        -> Result<NegotiatedFormat, NegotiationError> 
    {
        // Find common formats
        let mut candidates: Vec<FormatMatch> = vec![];
        
        for local in &self.local_adapters {
            for peer in peer_capabilities {
                if local.format_id == peer.format_id {
                    // Check version compatibility
                    if self.versions_compatible(&local.format_version, &peer.format_version) {
                        // Check type coverage
                        let common_types: Vec<_> = local.supported_types.iter()
                            .filter(|t| peer.supported_types.contains(t))
                            .cloned()
                            .collect();
                        
                        if !common_types.is_empty() {
                            candidates.push(FormatMatch {
                                format_id: local.format_id.clone(),
                                version: self.negotiate_version(
                                    &local.format_version, 
                                    &peer.format_version
                                ),
                                types: common_types,
                                priority: local.priority + peer.priority,
                            });
                        }
                    }
                }
            }
        }
        
        // Select highest priority match
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority));
        
        candidates.into_iter().next()
            .map(|m| NegotiatedFormat {
                format_id: m.format_id,
                version: m.version,
                supported_types: m.types,
            })
            .ok_or(NegotiationError::NoCommonFormat)
    }
    
    /// Handle PEAT-native peer (no format adapter needed)
    pub fn is_peat_native(peer_capabilities: &[FormatAdapterCapability]) -> bool {
        peer_capabilities.iter().any(|c| c.format_id == "peat-native")
    }
}

/// Result of format negotiation
pub struct NegotiatedFormat {
    pub format_id: String,
    pub version: String,
    pub supported_types: Vec<String>,
}
```

### Transform Pipeline (Chained Transformations)

For multi-hop scenarios (e.g., PEAT → CoT → Link 16 gateway):

```rust
/// Transform pipeline for chained format conversion
pub struct TransformPipeline {
    stages: Vec<Box<dyn FormatAdapter>>,
}

impl TransformPipeline {
    /// Build pipeline from source to destination format
    pub fn build(
        source_format: &str, 
        dest_format: &str,
        registry: &FormatAdapterRegistry
    ) -> Result<Self, PipelineError> {
        // Find path through registered adapters
        // All adapters convert to/from canonical PEAT format
        
        let mut stages: Vec<Box<dyn FormatAdapter>> = vec![];
        
        // If source isn't PEAT-native, add decode stage
        if source_format != "peat-native" {
            let decoder = registry.get(source_format)
                .ok_or(PipelineError::AdapterNotFound(source_format.into()))?;
            stages.push(decoder);
        }
        
        // If dest isn't PEAT-native, add encode stage
        if dest_format != "peat-native" {
            let encoder = registry.get(dest_format)
                .ok_or(PipelineError::AdapterNotFound(dest_format.into()))?;
            stages.push(encoder);
        }
        
        Ok(Self { stages })
    }
    
    /// Execute pipeline transformation
    pub fn transform(&self, input: &EncodedMessage) -> Result<EncodedMessage, TransformError> {
        let mut current = input.clone();
        
        for stage in &self.stages {
            // Decode to PEAT doc
            let doc = stage.decode(&current)?;
            
            // If more stages, encode to next format
            if let Some(next_stage) = self.stages.get(1) {
                current = next_stage.encode(&doc)?;
            } else {
                // Final stage - return as PEAT doc or re-encode
                return stage.encode(&doc);
            }
        }
        
        Ok(current)
    }
}

// Example: CoT → PEAT → Link 16 (via gateway)
// let pipeline = TransformPipeline::build("cot", "link16", &registry)?;
// let link16_msg = pipeline.transform(&cot_message)?;
```

### WearTAK Integration Mode

For WearTAK devices, PEAT Lite can operate in **CoT-Native Mode**:

```rust
/// PEAT Lite operating modes for format handling
pub enum LiteFormatMode {
    /// Full PEAT protocol with optional format adapters
    PeatNative {
        adapters: Vec<FormatAdapterCapability>,
    },
    
    /// Speak external format natively, parent handles conversion
    ExternalNative {
        format: String,          // "cot", "mqtt", etc.
        parent_converts: bool,   // Parent handles PEAT ↔ format conversion
    },
    
    /// Minimal: Only send raw data, parent handles everything
    RawRelay {
        payload_type: String,
    },
}

/// WearTAK configuration: CoT-native leaf node
pub fn weartk_config() -> LiteNodeConfig {
    LiteNodeConfig {
        format_mode: LiteFormatMode::ExternalNative {
            format: "cot".into(),
            parent_converts: true,
        },
        sync_config: wearable_config(),
        
        // Minimal CoT types this device produces
        produces: vec!["position", "health", "alert"],
        
        // CoT types this device consumes
        consumes: vec!["command", "geofence", "chat"],
    }
}
```

### Parent-Side CoT Bridge for PEAT Lite

The ATAK phone (PEAT Edge node) handles CoT ↔ PEAT transformation for its leaf children:

```rust
/// ATAK/PEAT Edge node handling WearTAK children
pub struct AtakPeatBridge {
    /// Local PEAT node
    peat_node: PeatEdgeNode,
    
    /// CoT format adapter
    cot_adapter: CotFormatAdapter,
    
    /// Connected WearTAK devices (children)
    weartk_children: HashMap<NodeId, WearTakChild>,
    
    /// TAK server connection
    tak_connection: TakServerConnection,
}

impl AtakPeatBridge {
    /// Handle incoming CoT from WearTAK device
    pub async fn handle_weartk_cot(&mut self, 
        child_id: &NodeId, 
        cot_bytes: &[u8]
    ) -> Result<(), BridgeError> {
        // Decode CoT to PEAT document
        let encoded = EncodedMessage {
            format: "cot".into(),
            encoding: "protobuf".into(),
            payload: cot_bytes.to_vec(),
            metadata: Default::default(),
        };
        
        let peat_doc = self.cot_adapter.decode(&encoded)?;
        
        // Inject into PEAT as child's document
        let doc_with_hierarchy = peat_doc.with_parent(self.peat_node.node_id());
        self.peat_node.merge_child_document(child_id, doc_with_hierarchy).await?;
        
        // Optionally forward to TAK server (as CoT)
        if self.should_forward_to_tak(&peat_doc) {
            self.tak_connection.send(cot_bytes).await?;
        }
        
        Ok(())
    }
    
    /// Send command to WearTAK device
    pub async fn send_to_weartk(&mut self,
        child_id: &NodeId,
        command: PeatCommand
    ) -> Result<(), BridgeError> {
        // Encode PEAT command to CoT
        let peat_doc = command.to_document();
        let cot_msg = self.cot_adapter.encode(&peat_doc)?;
        
        // Send via BLE to watch
        let child = self.weartk_children.get_mut(child_id)
            .ok_or(BridgeError::ChildNotFound)?;
        
        child.ble_connection.send(&cot_msg.payload).await?;
        
        Ok(())
    }
}
```

### Minimal CoT Schema for Wearables

Constrained devices only need a subset of CoT:

```protobuf
// Minimal CoT for PEAT Lite wearables
// Subset of full CoT schema

message MinimalCotEvent {
  // Required CoT attributes
  string uid = 1;           // Unique identifier
  string type = 2;          // CoT type code (e.g., "a-f-G-U-C")
  int64 time = 3;           // Event time (Unix millis)
  int64 start = 4;          // Valid start time
  int64 stale = 5;          // Stale time (TTL)
  
  // Position (required for most types)
  MinimalPoint point = 6;
  
  // Minimal detail for wearable-relevant data
  MinimalDetail detail = 7;
}

message MinimalPoint {
  double lat = 1;
  double lon = 2;
  double hae = 3;          // Height above ellipsoid
  double ce = 4;           // Circular error (accuracy)
  double le = 5;           // Linear error (vertical accuracy)
}

message MinimalDetail {
  // Wearable-specific detail elements
  oneof detail_type {
    HealthDetail health = 1;
    AlertDetail alert = 2;
    AckDetail ack = 3;
  }
}

message HealthDetail {
  uint32 battery = 1;       // Battery percentage
  float heart_rate = 2;     // Heart rate (wearable)
  bool sos_active = 3;      // Emergency state
}

message AlertDetail {
  string alert_type = 1;    // "sos", "low_battery", "geofence_breach"
  string message = 2;       // Alert text
  uint32 priority = 3;      // 1-5 priority
}

message AckDetail {
  string ack_uid = 1;       // UID of message being acknowledged
  string status = 2;        // "received", "executed", "rejected"
}
```

## Consequences

### Positive

1. **Native format support**: Devices can speak CoT (or other formats) without full PEAT stack
2. **Negotiated interoperability**: Peers automatically discover compatible formats
3. **Composable transforms**: Multi-hop bridging becomes straightforward
4. **Explicit semantics**: Schema-declared mappings document what's preserved/lost
5. **Optimized for constraints**: Lite nodes can skip transformation overhead
6. **Ecosystem integration**: TAK, ROS2, MQTT devices join PEAT naturally

### Negative

1. **Complexity**: Format adapter layer adds architectural complexity
2. **Maintenance**: Each format adapter requires ongoing maintenance
3. **Lossy transforms**: Some transformations lose information (documented in LossInfo)
4. **Version matrix**: Format version compatibility adds combinatorial complexity

### Risks

1. **Semantic drift**: PEAT concepts may not map cleanly to all formats
2. **Performance**: Transform overhead in hot paths (mitigated by caching)
3. **Security**: Malformed external messages could exploit transform bugs

## Implementation Plan

### Phase 1: Core Format Adapter Framework (Q1 2026)
1. FormatAdapter trait and registry
2. Capability advertisement in beacons
3. Format negotiation protocol
4. Basic pipeline infrastructure

### Phase 2: CoT Reference Adapter (Q1 2026)
1. Full PEAT ↔ CoT type mappings
2. XML and Protobuf encoding
3. Schema validation
4. Integration tests with TAK

### Phase 3: WearTAK Integration (Q1 2026)
1. CoT-native mode for PEAT Lite
2. ATAK bridge implementation
3. Minimal CoT schema for wearables
4. Field testing with Ascent

### Phase 4: Additional Adapters (Q2 2026)
1. DDS/RTPS for ROS2
2. MQTT for IoT sensors
3. Link 16 (gateway pattern)
4. Custom format SDK

## References

- ADR-012: Schema Definition and Protocol Extensibility
- ADR-020: TAK/CoT Integration
- ADR-025: Resource-Constrained Device Optimization
- CoT Schema: MIL-PRF-XXXX (ATAK/TAK documentation)
- TAK Protocol Spec: TAK Product Center
- Ascent WearTAK feedback (Alex Gorsuch, December 2025)

## Appendix A: CoT Type Hierarchy Reference

```
CoT Type Code Structure: a-{affiliation}-{battle_dimension}-{function}

Affiliation:
  f = friendly
  h = hostile  
  u = unknown
  n = neutral
  p = pending

Battle Dimension:
  A = air
  G = ground
  S = sea surface
  U = subsurface
  P = space
  F = SOF

Function (examples):
  U-C = unit-combat
  U-R = unit-recon
  E-V = equipment-vehicle
  I-R = installation-radar

Examples:
  a-f-G-U-C = friendly ground unit combat
  a-h-A-M-F = hostile air military fixed-wing
  a-u-G-E-V = unknown ground equipment vehicle
```

## Appendix B: Transform Loss Documentation

| PEAT → CoT | Lost Information | Mitigation |
|------------|------------------|------------|
| Capability | Rich capability taxonomy | Encode in detail/peat_capabilities extension |
| Hierarchy | Multi-parent relationships | Use CoT link elements for primary parent |
| CRDT metadata | Vector clocks, merge history | Not needed for TAK display |
| TTL semantics | PEAT's flexible TTL tiers | Map to CoT stale time |

| CoT → PEAT | Lost Information | Mitigation |
|------------|------------------|------------|
| Symbology detail | MIL-STD-2525 specifics | Preserve in opaque detail field |
| Contact chains | CoT contact references | Parse and map to PEAT relationships |
| Custom detail schemas | Domain-specific extensions | Preserve as opaque JSON |
