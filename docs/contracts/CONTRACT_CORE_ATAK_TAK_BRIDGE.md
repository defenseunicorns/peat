# Interface Contract: Core ↔ ATAK

**Document Version**: 1.0  
**Status**: Draft - Awaiting Approval  
**Owner Team**: Core (defines HIVE schema), ATAK (defines CoT mapping)  
**Consumer Team**: Both (bidirectional)  
**Required By**: Phase 1-5 (All phases require TAK integration)

---

## Overview

This contract defines the bidirectional interface between the Core protocol and the ATAK team. The HIVE-TAK Bridge translates between HIVE Protocol messages and Cursor-on-Target (CoT) XML for TAK Server integration.

## Data Flows

### Upward Flow (HIVE → CoT → TAK)
```
┌─────────────┐   HIVE    ┌─────────────┐    CoT/TCP    ┌─────────────┐
│  Team Nodes │ ────────► │ HIVE-TAK    │ ────────────► │ TAK Server  │
│             │           │ Bridge      │               │             │
│ Track, Cap  │           │ (ATAK Team) │               │ WebTAK      │
└─────────────┘           └─────────────┘               └─────────────┘
```

### Downward Flow (TAK → CoT → HIVE)
```
┌─────────────┐   CoT/TCP  ┌─────────────┐    HIVE     ┌─────────────┐
│ TAK Server  │ ─────────► │ HIVE-TAK    │ ──────────► │  Team Nodes │
│ (Commands)  │            │ Bridge      │             │             │
│             │            │ (ATAK Team) │             │ Mission Rx  │
└─────────────┘            └─────────────┘             └─────────────┘
```

---

## Interface 1: Track Update (HIVE → CoT)

### HIVE Schema: TrackUpdate

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://revolveteam.com/hive/schemas/track-update.json",
  "title": "TrackUpdate",
  "type": "object",
  "required": ["track_id", "classification", "confidence", "position", "timestamp"],
  "properties": {
    "track_id": {
      "type": "string",
      "pattern": "^TRACK-[0-9]{3,}$",
      "description": "Unique track identifier"
    },
    "classification": {
      "type": "string",
      "enum": ["person", "vehicle", "aircraft", "vessel", "unknown"],
      "description": "Track classification"
    },
    "confidence": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Classification confidence (0-1)"
    },
    "position": {
      "type": "object",
      "required": ["lat", "lon"],
      "properties": {
        "lat": { "type": "number", "minimum": -90, "maximum": 90 },
        "lon": { "type": "number", "minimum": -180, "maximum": 180 },
        "hae": { "type": "number", "description": "Height above ellipsoid (meters)" },
        "cep_m": { "type": "number", "minimum": 0, "description": "Circular error probable (meters)" }
      }
    },
    "velocity": {
      "type": "object",
      "properties": {
        "bearing": { "type": "number", "minimum": 0, "maximum": 360 },
        "speed_mps": { "type": "number", "minimum": 0 }
      }
    },
    "attributes": {
      "type": "object",
      "description": "Free-form attributes (e.g., jacket_color, has_backpack)"
    },
    "source_platform": {
      "type": "string",
      "description": "Platform that detected this track"
    },
    "source_model": {
      "type": "string",
      "description": "AI model that generated this track"
    },
    "model_version": {
      "type": "string",
      "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$"
    },
    "timestamp": {
      "type": "string",
      "format": "date-time"
    }
  }
}
```

### CoT Mapping

| HIVE Field | CoT Element/Attribute | Notes |
|------------|----------------------|-------|
| `track_id` | `event@uid` | Direct mapping |
| `classification` | `event@type` | See type mapping table below |
| `position.lat` | `point@lat` | Direct mapping |
| `position.lon` | `point@lon` | Direct mapping |
| `position.hae` | `point@hae` | Default 0 if missing |
| `position.cep_m` | `point@ce` | Circular error |
| `velocity.bearing` | `detail/track@course` | Degrees from north |
| `velocity.speed_mps` | `detail/track@speed` | Meters per second |
| `confidence` | `detail/remarks` | Include in remarks text |
| `attributes` | `detail/remarks` | Serialize to human-readable |
| `model_version` | `detail/_hive_@model_version` | HIVE extension element |
| `timestamp` | `event@time`, `event@start` | ISO 8601 |
| N/A | `event@stale` | `timestamp + 5 minutes` |
| N/A | `event@how` | `m-g` (machine-generated) |

### Classification → CoT Type Mapping

| HIVE Classification | CoT Type | Description |
|--------------------|----------|-------------|
| `person` | `a-f-G-E-S` | Friendly Ground Entity (civilian/unknown) |
| `vehicle` | `a-f-G-E-V` | Friendly Ground Vehicle |
| `aircraft` | `a-f-A` | Friendly Aircraft |
| `vessel` | `a-f-S` | Friendly Surface (vessel) |
| `unknown` | `a-u-G` | Unknown Ground |

*Note: Default to friendly (f) for demo. Production would infer from context.*

### Example Transformation

**HIVE Input:**
```json
{
  "track_id": "TRACK-001",
  "classification": "person",
  "confidence": 0.89,
  "position": { "lat": 33.7749, "lon": -84.3958, "cep_m": 2.5 },
  "velocity": { "bearing": 45, "speed_mps": 1.2 },
  "attributes": { "jacket_color": "blue", "has_backpack": true },
  "source_platform": "Alpha-2",
  "source_model": "Alpha-3",
  "model_version": "1.3.0",
  "timestamp": "2025-12-08T14:10:00Z"
}
```

**CoT Output:**
```xml
<event uid="TRACK-001" type="a-f-G-E-S" time="2025-12-08T14:10:00Z" 
       start="2025-12-08T14:10:00Z" stale="2025-12-08T14:15:00Z" how="m-g">
  <point lat="33.7749" lon="-84.3958" hae="0" ce="2.5" le="1"/>
  <detail>
    <track course="45" speed="1.2"/>
    <remarks>person - Blue jacket, backpack (89% confidence) [Alpha-2/Alpha-3]</remarks>
    <_hive_ model_version="1.3.0" source_platform="Alpha-2" source_model="Alpha-3"/>
  </detail>
</event>
```

---

## Interface 2: Mission Tasking (CoT → HIVE)

### CoT Input: Mission Task

```xml
<event uid="MISSION-001" type="t-x-m-c-c" time="2025-12-08T14:05:00Z"
       start="2025-12-08T14:05:00Z" stale="2025-12-08T15:05:00Z" how="h-g-i-g-o">
  <point lat="33.7756" lon="-84.3963" hae="0" ce="100" le="100"/>
  <detail>
    <mission type="TRACK_TARGET">
      <target description="Adult male, blue jacket, backpack"/>
      <boundary>
        <polygon>33.7760,-84.3970 33.7760,-84.3950 33.7740,-84.3950 33.7740,-84.3970</polygon>
      </boundary>
    </mission>
    <remarks>Track POI within designated area</remarks>
  </detail>
</event>
```

### HIVE Output: MissionTask

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://revolveteam.com/hive/schemas/mission-task.json",
  "title": "MissionTask",
  "type": "object",
  "required": ["task_id", "task_type", "issued_at", "issued_by"],
  "properties": {
    "task_id": { "type": "string" },
    "task_type": {
      "type": "string",
      "enum": ["TRACK_TARGET", "SEARCH_AREA", "MONITOR_ZONE", "ABORT"]
    },
    "issued_at": { "type": "string", "format": "date-time" },
    "issued_by": { "type": "string", "description": "CoT UID of issuer" },
    "expires_at": { "type": "string", "format": "date-time" },
    "target": {
      "type": "object",
      "properties": {
        "description": { "type": "string" },
        "last_known_position": {
          "type": "object",
          "properties": {
            "lat": { "type": "number" },
            "lon": { "type": "number" }
          }
        }
      }
    },
    "boundary": {
      "type": "object",
      "properties": {
        "type": { "type": "string", "enum": ["polygon", "circle"] },
        "coordinates": { "type": "array", "items": { "type": "array" } },
        "radius_m": { "type": "number" }
      }
    },
    "priority": {
      "type": "string",
      "enum": ["CRITICAL", "HIGH", "NORMAL", "LOW"]
    }
  }
}
```

---

## Interface 3: Capability Advertisement (HIVE → CoT)

### HIVE Schema: CapabilityAdvertisement (from capability.proto)

```protobuf
message CapabilityAdvertisement {
  string platform_id = 1;                    // "Alpha-3"
  Timestamp advertised_at = 2;               // Unix timestamp
  repeated Capability capabilities = 3;      // List of capabilities
  ResourceStatus resources = 4;              // Optional resource utilization
  OperationalStatus operational_status = 5;  // READY, ACTIVE, DEGRADED, etc.
}

message Capability {
  string id = 1;                    // "object_tracker"
  string name = 2;                  // "Object Tracker Model"
  CapabilityType capability_type = 3;  // COMPUTE, SENSOR, etc.
  float confidence = 4;             // 0.0-1.0
  string metadata_json = 5;         // AI model details
}
```

### CoT Mapping: Platform Registration

| HIVE Field | CoT Element/Attribute | Notes |
|------------|----------------------|-------|
| `platform_id` | `event@uid` | Direct mapping |
| N/A | `event@type` | `a-f-G-U-C` (friendly ground unit, combat) |
| `advertised_at` | `event@time`, `event@start` | ISO 8601 |
| N/A | `event@stale` | `advertised_at + 60 seconds` (heartbeat) |
| N/A | `event@how` | `m-g` (machine-generated) |
| `operational_status` | `detail/status@readiness` | See status mapping |
| `capabilities[].name` | `detail/remarks` | Comma-separated list |
| `resources.compute_utilization` | `detail/_hive_@cpu` | Percentage (×100) |
| `resources.memory_utilization` | `detail/_hive_@mem` | Percentage (×100) |
| `resources.power_level` | `detail/_hive_@battery` | Percentage (×100) |
| `capabilities[].metadata_json` | `detail/_hive_@models` | JSON array of model info |

### Operational Status → CoT Mapping

| HIVE OperationalStatus | CoT status@readiness | Description |
|------------------------|---------------------|-------------|
| `READY` | `true` | Platform ready for tasking |
| `ACTIVE` | `true` | Platform executing task |
| `DEGRADED` | `partial` | Reduced capability |
| `OFFLINE` | `false` | Not available |
| `MAINTENANCE` | `false` | Under maintenance |

### Example Transformation

**HIVE Input (CapabilityAdvertisement):**
```json
{
  "platform_id": "Alpha-3",
  "advertised_at": { "seconds": 1733670600, "nanos": 0 },
  "capabilities": [
    {
      "id": "object_tracker",
      "name": "Object Tracker v1.2",
      "capability_type": "COMPUTE",
      "confidence": 0.95,
      "metadata_json": "{\"model_type\":\"detector_tracker\",\"fps\":15}"
    }
  ],
  "resources": {
    "compute_utilization": 0.65,
    "memory_utilization": 0.50,
    "power_level": 0.90
  },
  "operational_status": "READY"
}
```

**CoT Output (Platform Registration):**
```xml
<event uid="Alpha-3" type="a-f-G-U-C" time="2025-12-08T14:30:00Z"
       start="2025-12-08T14:30:00Z" stale="2025-12-08T14:31:00Z" how="m-g">
  <point lat="0" lon="0" hae="0" ce="9999999" le="9999999"/>
  <detail>
    <status readiness="true"/>
    <remarks>Capabilities: Object Tracker v1.2 (95%)</remarks>
    <_hive_ cpu="65" mem="50" battery="90"
            models="[{\"id\":\"object_tracker\",\"type\":\"detector_tracker\",\"fps\":15}]"/>
  </detail>
</event>
```

*Note: Position is placeholder (0,0) until actual GPS fix. Platform position comes from separate TrackUpdate or SA message.*

---

## Interface 4: Protobuf ↔ JSON Field Reference

For implementation, here are the exact protobuf field mappings:

### TrackUpdate (track.proto) → JSON

| Protobuf Field | JSON Path | Type |
|----------------|-----------|------|
| `track.track_id` | `track_id` | string |
| `track.classification` | `classification` | string |
| `track.confidence` | `confidence` | float |
| `track.position.latitude` | `position.lat` | double |
| `track.position.longitude` | `position.lon` | double |
| `track.position.altitude` | `position.hae` | float |
| `track.position.cep_m` | `position.cep_m` | float |
| `track.velocity.bearing` | `velocity.bearing` | float |
| `track.velocity.speed_mps` | `velocity.speed_mps` | float |
| `track.source.platform_id` | `source_platform` | string |
| `track.source.sensor_id` | `source_model` | string |
| `track.source.model_version` | `model_version` | string |
| `track.attributes_json` | `attributes` | object (parsed) |
| `timestamp.seconds` | `timestamp` | ISO 8601 |
| `update_type` | N/A | Internal use |

### HierarchicalCommand (command.proto) ← CoT Mission

| CoT Element | Protobuf Field | Notes |
|-------------|----------------|-------|
| `event@uid` | `command_id` | Direct mapping |
| `event@time` | `issued_at.seconds` | Parse ISO 8601 |
| `event@stale` | `expires_at.seconds` | Parse ISO 8601 |
| `detail/mission@type` | `mission_order.mission_type` | See enum mapping |
| `detail/target@description` | `mission_order.description` | Target description |
| `point@lat` | `mission_order.objective_location.latitude` | Target position |
| `point@lon` | `mission_order.objective_location.longitude` | Target position |
| N/A | `originator_id` | Set to TAK Server UID |
| N/A | `target.scope` | Default to SQUAD |

### Mission Type Mapping

| CoT mission@type | HierarchicalCommand MissionType |
|------------------|--------------------------------|
| `TRACK_TARGET` | `ISR` (1) |
| `SEARCH_AREA` | `ISR` (1) |
| `MONITOR_ZONE` | `DEFENSIVE` (5) |
| `ABORT` | N/A (cancel command) |

---

## Core Team Responsibilities

- [ ] Define HIVE schemas (TrackUpdate, MissionTask, etc.)
- [ ] Provide schema validation functions
- [ ] Implement Automerge collection sync for tracks and tasks
- [ ] Document sync protocol for bridge integration
- [ ] Provide mock HIVE data for ATAK testing

## ATAK Team Responsibilities

- [ ] Implement HIVE-TAK Bridge application
- [ ] Translate TrackUpdate → CoT events
- [ ] Translate CoT mission tasks → HIVE MissionTask
- [ ] Connect to TAK Server via CoT/TCP
- [ ] Display tracks on ATAK plugin map
- [ ] Show capability status in ATAK UI
- [ ] Handle bidirectional sync errors gracefully

## Acceptance Criteria

### Track Flow (HIVE → TAK)
- [ ] TrackUpdate syncs from Jetson to bridge within 2s
- [ ] Bridge converts to CoT within 100ms
- [ ] CoT appears on WebTAK within 1s
- [ ] Track position updates at 2 Hz minimum
- [ ] Track stale detection works (disappears after 5 min no update)

### Command Flow (TAK → HIVE)
- [ ] Mission task created in WebTAK
- [ ] CoT reaches bridge within 1s
- [ ] MissionTask syncs to team nodes within 2s
- [ ] ATAK plugin displays mission on operator device

### Integration Test
- [ ] End-to-end track: Jetson → Bridge → TAK Server → WebTAK
- [ ] End-to-end command: WebTAK → TAK Server → Bridge → Team Nodes
- [ ] Latency < 5s total for both directions
- [ ] No data loss under normal network conditions

## Error Handling

| Error | Bridge Action |
|-------|--------------|
| Invalid HIVE message | Log warning, skip message |
| CoT serialization error | Log error, retry with defaults |
| TAK Server connection lost | Buffer messages, reconnect with backoff |
| Invalid CoT from TAK | Log warning, skip message |
| HIVE sync failure | Buffer task, retry |

## Approval

| Team | Approver | Date | Signature |
|------|----------|------|-----------|
| Core | | | ☐ Approved |
| ATAK | | | ☐ Approved |

---

*Document maintained by (r)evolve - Revolve Team LLC*  
*https://revolveteam.com*
