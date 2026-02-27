# PEAT ↔ CoT Schema Mapping Specification

**Document Type**: Technical Specification
**Date**: 2025-11-26
**Version**: 1.0
**Source**: peat-m1-poc message definitions

## Overview

This document provides field-by-field mapping between PEAT M1 message types and Cursor-on-Target (CoT) XML schema. These mappings enable bidirectional translation for TAK integration.

---

## 1. TrackUpdate → CoT Event (PEAT → TAK)

### PEAT Source Structure

```rust
pub struct TrackUpdate {
    pub track_id: String,           // "TRACK-001"
    pub classification: String,      // "person", "vehicle"
    pub confidence: f64,            // 0.89
    pub position: Position,
    pub velocity: Option<Velocity>,
    pub attributes: HashMap<String, Value>,
    pub source_platform: String,    // "Alpha-2"
    pub source_model: String,       // "Alpha-3"
    pub model_version: String,      // "1.3.0"
    pub timestamp: DateTime<Utc>,
}

pub struct Position {
    pub lat: f64,
    pub lon: f64,
    pub cep_m: Option<f64>,
    pub hae: Option<f64>,
}

pub struct Velocity {
    pub bearing: f64,       // degrees, 0 = North
    pub speed_mps: f64,     // meters per second
}
```

### CoT Target Structure

```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0"
       uid="{track_id}"
       type="{cot_type}"
       time="{timestamp}"
       start="{timestamp}"
       stale="{timestamp + stale_duration}"
       how="m-g">

  <point lat="{position.lat}"
         lon="{position.lon}"
         hae="{position.hae | 0.0}"
         ce="{position.cep_m | 9999999.0}"
         le="9999999.0"/>

  <detail>
    <track course="{velocity.bearing}"
           speed="{velocity.speed_mps}"/>

    <remarks>{classification}: {formatted_attributes} ({confidence}% confidence)</remarks>

    <_peat_ version="1.0">
      <source platform="{source_platform}"
              model="{source_model}"
              model_version="{model_version}"/>
      <confidence value="{confidence}"/>
      <attributes>
        {for (key, value) in attributes}
        <attr key="{key}">{value}</attr>
        {end}
      </attributes>
    </_peat_>

    <link uid="{source_platform}" type="a-f-G-U-C" relation="o-o"/>
  </detail>
</event>
```

### Field Mapping Table

| PEAT Field | CoT Field | Transformation | Notes |
|------------|-----------|----------------|-------|
| `track_id` | `event@uid` | Direct | Unique track identifier |
| `classification` | `event@type` | Lookup table | See classification mapping |
| `timestamp` | `event@time` | RFC 3339 format | ISO 8601 timestamp |
| `timestamp` | `event@start` | Same as time | Event validity start |
| `timestamp` | `event@stale` | time + 30s | Configurable stale duration |
| - | `event@how` | Constant `m-g` | Machine-generated |
| `position.lat` | `point@lat` | Direct | WGS84 latitude |
| `position.lon` | `point@lon` | Direct | WGS84 longitude |
| `position.hae` | `point@hae` | Default 0.0 | Height above ellipsoid (m) |
| `position.cep_m` | `point@ce` | Default 9999999.0 | Circular error (m) |
| - | `point@le` | Constant 9999999.0 | Linear error (unknown) |
| `velocity.bearing` | `track@course` | Direct | Degrees from north |
| `velocity.speed_mps` | `track@speed` | Direct | Meters per second |
| `classification` + `attributes` + `confidence` | `remarks` | Formatted string | Human-readable summary |
| `source_platform` | `_peat_/source@platform` | Direct | PEAT extension |
| `source_model` | `_peat_/source@model` | Direct | PEAT extension |
| `model_version` | `_peat_/source@model_version` | Direct | PEAT extension |
| `confidence` | `_peat_/confidence@value` | Direct | PEAT extension |
| `attributes` | `_peat_/attributes/attr` | Key-value pairs | PEAT extension |
| `source_platform` | `link@uid` | Direct | Links to sensor platform |

### Classification → CoT Type Mapping

| Classification | CoT Type | Description |
|---------------|----------|-------------|
| `person` | `a-f-G-E-S` | Friendly Ground Equipment - Sensor (tracked entity) |
| `vehicle` | `a-f-G-E-V` | Friendly Ground Equipment - Vehicle |
| `aircraft` | `a-f-A` | Friendly Air |
| `vessel` | `a-f-S` | Friendly Surface (maritime) |
| `unknown` | `a-u-G` | Unknown Ground |
| `hostile_person` | `a-h-G-U-C-I` | Hostile Ground - Infantry |
| `hostile_vehicle` | `a-h-G-E-V` | Hostile Ground Equipment - Vehicle |

### Example Conversion

**PEAT TrackUpdate**:
```json
{
  "track_id": "TRACK-001",
  "classification": "person",
  "confidence": 0.89,
  "position": {
    "lat": 33.7749,
    "lon": -84.3958,
    "cep_m": 2.5,
    "hae": 0.0
  },
  "velocity": {
    "bearing": 45.0,
    "speed_mps": 1.2
  },
  "attributes": {
    "jacket_color": "blue",
    "has_backpack": true
  },
  "source_platform": "Alpha-2",
  "source_model": "Alpha-3",
  "model_version": "1.3.0",
  "timestamp": "2025-11-26T14:10:00Z"
}
```

**CoT Event**:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0"
       uid="TRACK-001"
       type="a-f-G-E-S"
       time="2025-11-26T14:10:00Z"
       start="2025-11-26T14:10:00Z"
       stale="2025-11-26T14:10:30Z"
       how="m-g">

  <point lat="33.7749"
         lon="-84.3958"
         hae="0.0"
         ce="2.5"
         le="9999999.0"/>

  <detail>
    <track course="45.0" speed="1.2"/>

    <remarks>person: blue jacket, has backpack (89% confidence)</remarks>

    <_peat_ version="1.0">
      <source platform="Alpha-2"
              model="Alpha-3"
              model_version="1.3.0"/>
      <confidence value="0.89"/>
      <attributes>
        <attr key="jacket_color">blue</attr>
        <attr key="has_backpack">true</attr>
      </attributes>
    </_peat_>

    <link uid="Alpha-2" type="a-f-G-U-C" relation="o-o"/>
  </detail>
</event>
```

---

## 2. CapabilityAdvertisement → CoT Event (PEAT → TAK)

### PEAT Source Structure

```rust
pub struct CapabilityAdvertisement {
    pub platform_id: String,
    pub advertised_at: DateTime<Utc>,
    pub models: Vec<ModelCapability>,
    pub resources: Option<ResourceMetrics>,
}

pub struct ModelCapability {
    pub model_id: String,
    pub model_version: String,
    pub model_hash: String,
    pub model_type: String,
    pub performance: ModelPerformance,
    pub operational_status: OperationalStatus,
}

pub struct ModelPerformance {
    pub precision: f64,
    pub recall: f64,
    pub fps: f64,
    pub latency_ms: Option<f64>,
}

pub enum OperationalStatus {
    Ready, Active, Degraded, Offline, Loading,
}
```

### CoT Target Structure

```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0"
       uid="{platform_id}"
       type="a-f-G-U-C"
       time="{advertised_at}"
       start="{advertised_at}"
       stale="{advertised_at + 60s}"
       how="m-g">

  <point lat="{platform_position.lat}"
         lon="{platform_position.lon}"
         hae="0.0"
         ce="9999999.0"
         le="9999999.0"/>

  <detail>
    <contact callsign="{platform_id}"/>

    <remarks>AI Platform: {model_summary}</remarks>

    <_peat_ version="1.0">
      <status operational="{operational_status}" readiness="{readiness_score}"/>
      {for model in models}
      <capability type="{model.model_type}"
                  model_id="{model.model_id}"
                  model_version="{model.model_version}"
                  precision="{model.performance.precision}"
                  recall="{model.performance.recall}"
                  fps="{model.performance.fps}"
                  status="{model.operational_status}"/>
      {end}
      {if resources}
      <resources gpu="{resources.gpu_utilization}"
                 memory_used_mb="{resources.memory_used_mb}"
                 memory_total_mb="{resources.memory_total_mb}"/>
      {end}
    </_peat_>

    <__group name="{cell_id}" role="AI Platform"/>
  </detail>
</event>
```

### Field Mapping Table

| PEAT Field | CoT Field | Transformation | Notes |
|------------|-----------|----------------|-------|
| `platform_id` | `event@uid` | Direct | Platform identifier |
| `platform_id` | `contact@callsign` | Direct | TAK callsign |
| - | `event@type` | Constant `a-f-G-U-C` | Friendly ground unit |
| `advertised_at` | `event@time` | RFC 3339 | Advertisement timestamp |
| `advertised_at` | `event@stale` | time + 60s | Longer stale for capabilities |
| `models[*].operational_status` | `_peat_/status@operational` | Enum to string | Overall platform status |
| `models[*]` | `_peat_/capability` | One element per model | Model capabilities |
| `models[*].model_id` | `capability@model_id` | Direct | |
| `models[*].model_version` | `capability@model_version` | Direct | |
| `models[*].performance.precision` | `capability@precision` | Direct | |
| `models[*].performance.recall` | `capability@recall` | Direct | |
| `models[*].performance.fps` | `capability@fps` | Direct | |
| `resources.gpu_utilization` | `resources@gpu` | Direct | |
| `resources.memory_used_mb` | `resources@memory_used_mb` | Direct | |

### Operational Status Mapping

| PEAT Status | CoT Value | TAK Display Suggestion |
|-------------|-----------|----------------------|
| `Ready` | `READY` | Green indicator |
| `Active` | `ACTIVE` | Blue/pulsing indicator |
| `Degraded` | `DEGRADED` | Yellow/warning indicator |
| `Offline` | `OFFLINE` | Red/grayed out |
| `Loading` | `LOADING` | Gray/spinner |

---

## 3. HandoffMessage → CoT Event (PEAT → TAK)

### PEAT Source Structure

```rust
pub struct HandoffMessage {
    pub handoff_id: Uuid,
    pub handoff_type: HandoffType,
    pub track_id: String,
    pub source_team: String,
    pub target_team: String,
    pub track_state: TrackUpdate,
    pub track_history: Vec<TrackUpdate>,
    pub poi_description: Option<String>,
    pub predicted_position: Option<Position>,
    pub timestamp: DateTime<Utc>,
}

pub enum HandoffType {
    PrepareHandoff,
    ConfirmAcquisition,
    ReleaseTrack,
    HandoffFailed,
}
```

### CoT Target Structure

```xml
<?xml version="2.0" encoding="UTF-8"?>
<event version="2.0"
       uid="{handoff_id}"
       type="a-x-h-h"
       time="{timestamp}"
       start="{timestamp}"
       stale="{timestamp + 60s}"
       how="m-g">

  <point lat="{track_state.position.lat}"
         lon="{track_state.position.lon}"
         hae="{track_state.position.hae | 0.0}"
         ce="{track_state.position.cep_m | 9999999.0}"
         le="9999999.0"/>

  <detail>
    <remarks>HANDOFF {handoff_type}: {track_id} from {source_team} to {target_team}</remarks>

    <_peat_ version="1.0">
      <handoff type="{handoff_type}"
               track_id="{track_id}"
               source="{source_team}"
               target="{target_team}"/>
      {if poi_description}
      <poi_description>{poi_description}</poi_description>
      {end}
      {if predicted_position}
      <predicted lat="{predicted_position.lat}"
                 lon="{predicted_position.lon}"/>
      {end}
    </_peat_>

    <!-- Link to source team -->
    <link uid="{source_team}" type="a-f-G-U-C" relation="h-h" remarks="handoff-source"/>

    <!-- Link to target team -->
    <link uid="{target_team}" type="a-f-G-U-C" relation="h-h" remarks="handoff-target"/>

    <!-- Link to track being handed off -->
    <link uid="{track_id}" type="a-f-G-E-S" relation="p-p" remarks="track"/>
  </detail>
</event>
```

### Handoff Type Mapping

| PEAT HandoffType | CoT Remarks Prefix | Semantics |
|------------------|-------------------|-----------|
| `PrepareHandoff` | `HANDOFF PREPARE` | Source initiating handoff |
| `ConfirmAcquisition` | `HANDOFF CONFIRM` | Target confirms acquisition |
| `ReleaseTrack` | `HANDOFF RELEASE` | Source releases responsibility |
| `HandoffFailed` | `HANDOFF FAILED` | Handoff unsuccessful |

---

## 4. CoT Event → TrackCommand (TAK → PEAT)

### CoT Source Structure (Mission Tasking)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0"
       uid="{command_uid}"
       type="t-x-m-c"
       time="{timestamp}"
       start="{timestamp}"
       stale="{timestamp + 3600s}"
       how="h-g-i-g-o">

  <point lat="{target_lat}"
         lon="{target_lon}"
         hae="0.0"
         ce="9999999.0"
         le="9999999.0"/>

  <detail>
    <remarks>{target_description}</remarks>

    <mission type="track"
             name="Track POI"
             priority="{priority}"/>

    <_flow-tags_ priority="{priority}"/>

    {if has_geofence}
    <shape>
      <polyline closed="true">
        {for coord in boundary_coords}
        <vertex lat="{coord.lat}" lon="{coord.lon}"/>
        {end}
      </polyline>
    </shape>
    {end}

    <contact callsign="{source_authority}"/>
  </detail>
</event>
```

### PEAT Target Structure

```rust
pub struct TrackCommand {
    pub command_id: Uuid,          // from event@uid
    pub command_type: CommandType, // from mission@type
    pub target_description: String, // from remarks
    pub operational_boundary: Option<OperationalBoundary>, // from shape
    pub priority: Priority,        // from _flow-tags_@priority
    pub source_authority: String,  // from contact@callsign
    pub timestamp: DateTime<Utc>,  // from event@time
}
```

### Field Mapping Table (CoT → PEAT)

| CoT Field | PEAT Field | Transformation | Notes |
|-----------|------------|----------------|-------|
| `event@uid` | `command_id` | Parse as UUID | May need to generate if not UUID |
| `event@type` | `command_type` | Type mapping | See below |
| `event@time` | `timestamp` | Parse RFC 3339 | |
| `remarks` | `target_description` | Direct | |
| `mission@priority` or `_flow-tags_@priority` | `priority` | Priority mapping | See below |
| `contact@callsign` | `source_authority` | Direct | |
| `shape/polyline/vertex` | `operational_boundary.coordinates` | Extract lat/lon pairs | |
| - | `operational_boundary.boundary_type` | Infer from shape | Polygon if closed="true" |

### CoT Type → CommandType Mapping

| CoT Type | PEAT CommandType | Description |
|----------|-----------------|-------------|
| `t-x-m-c` | `TrackTarget` | Mission tasking - track |
| `t-x-m-c-c` | `CancelTrack` | Mission tasking - cancel |
| `t-x-m-c-u` | `UpdateParameters` | Mission tasking - update |
| `t-x-m-c-a` | `AcknowledgeHandoff` | Mission tasking - acknowledge |

### Priority Mapping (CoT → PEAT)

| CoT Priority | PEAT Priority |
|--------------|---------------|
| `flash` | `Critical` |
| `immediate` | `High` |
| `priority` | `High` |
| `routine` | `Normal` |
| `deferred` | `Low` |
| `bulk` | `Bulk` |

---

## 5. CoT Event → OperationalBoundary (TAK → PEAT)

### CoT Source Structure (Drawing/Geofence)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0"
       uid="{geofence_uid}"
       type="u-d-r"
       time="{timestamp}"
       start="{timestamp}"
       stale="{timestamp + 86400s}"
       how="h-e">

  <point lat="{centroid_lat}"
         lon="{centroid_lon}"
         hae="0.0"
         ce="9999999.0"
         le="9999999.0"/>

  <detail>
    <remarks>Operational Area: {area_name}</remarks>

    <shape>
      <polyline closed="true">
        <vertex lat="33.77" lon="-84.40"/>
        <vertex lat="33.77" lon="-84.39"/>
        <vertex lat="33.78" lon="-84.39"/>
        <vertex lat="33.78" lon="-84.40"/>
      </polyline>
    </shape>

    <link uid="{parent_mission}" type="t-x-m" relation="p-p"/>
  </detail>
</event>
```

### PEAT Target Structure

```rust
pub struct OperationalBoundary {
    pub boundary_type: BoundaryType,
    pub coordinates: Vec<Vec<f64>>,  // [[lon, lat], ...]
}

pub enum BoundaryType {
    Polygon,
    Circle,
    Rectangle,
}
```

### Field Mapping Table

| CoT Field | PEAT Field | Transformation |
|-----------|------------|----------------|
| `shape/polyline@closed="true"` | `boundary_type` | `Polygon` |
| `shape/ellipse` | `boundary_type` | `Circle` |
| `shape/polyline/vertex` | `coordinates` | `[[lon, lat], ...]` |
| `shape/ellipse@major` | `coordinates` | `[[center_lon, center_lat], [radius]]` |

---

## 6. FormationCapabilitySummary → CoT Event (PEAT → TAK)

### PEAT Source Structure

```rust
// From coordinator.rs (conceptual - aggregated at formation level)
pub struct FormationCapabilitySummary {
    pub formation_id: String,
    pub team_count: usize,
    pub platform_count: usize,
    pub camera_count: usize,
    pub tracker_versions: Vec<String>,
    pub coverage_sectors: Vec<String>,
    pub capability_confidence: HashMap<String, f64>,
    pub readiness_score: f64,
}
```

### CoT Target Structure

```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0"
       uid="{formation_id}"
       type="a-f-G-U-C"
       time="{timestamp}"
       start="{timestamp}"
       stale="{timestamp + 60s}"
       how="m-g">

  <point lat="{formation_centroid.lat}"
         lon="{formation_centroid.lon}"
         hae="0.0"
         ce="9999999.0"
         le="9999999.0"/>

  <detail>
    <contact callsign="{formation_id}"/>

    <remarks>Formation: {team_count} teams, {platform_count} platforms, {readiness_pct}% ready</remarks>

    <_peat_ version="1.0">
      <formation teams="{team_count}"
                 platforms="{platform_count}"
                 cameras="{camera_count}"
                 readiness="{readiness_score}"/>
      <trackers>
        {for version in tracker_versions}
        <version>{version}</version>
        {end}
      </trackers>
      <coverage>
        {for sector in coverage_sectors}
        <sector>{sector}</sector>
        {end}
      </coverage>
      <capabilities>
        {for (cap, confidence) in capability_confidence}
        <capability type="{cap}" confidence="{confidence}"/>
        {end}
      </capabilities>
    </_peat_>

    <!-- Links to subordinate teams -->
    {for team in teams}
    <link uid="{team.id}" type="a-f-G-U-C" relation="p-p"/>
    {end}
  </detail>
</event>
```

---

## 7. Encoding/Decoding Implementation Notes

### Timestamp Handling

```rust
// PEAT → CoT
fn encode_timestamp(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

// CoT → PEAT
fn decode_timestamp(s: &str) -> Result<DateTime<Utc>, ParseError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
}
```

### Stale Time Calculation

```rust
const DEFAULT_TRACK_STALE_SECS: i64 = 30;
const DEFAULT_CAPABILITY_STALE_SECS: i64 = 60;
const DEFAULT_HANDOFF_STALE_SECS: i64 = 60;
const DEFAULT_COMMAND_STALE_SECS: i64 = 3600;

fn calculate_stale(timestamp: DateTime<Utc>, message_type: MessageType) -> DateTime<Utc> {
    let duration = match message_type {
        MessageType::TrackUpdate => DEFAULT_TRACK_STALE_SECS,
        MessageType::CapabilityAdvertisement => DEFAULT_CAPABILITY_STALE_SECS,
        MessageType::HandoffMessage => DEFAULT_HANDOFF_STALE_SECS,
        MessageType::TrackCommand => DEFAULT_COMMAND_STALE_SECS,
    };
    timestamp + chrono::Duration::seconds(duration)
}
```

### XML Encoding

Recommend using `quick-xml` crate for XML encoding/decoding:

```rust
use quick_xml::{Writer, events::{Event, BytesStart, BytesEnd, BytesText}};

fn encode_track_to_cot(track: &TrackUpdate) -> Result<String, EncodingError> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    // Event element
    let mut event = BytesStart::borrowed(b"event", "event".len());
    event.push_attribute(("version", "2.0"));
    event.push_attribute(("uid", track.track_id.as_str()));
    event.push_attribute(("type", classify_to_cot_type(&track.classification)));
    // ... etc
}
```

### Validation Rules

1. **UID uniqueness**: CoT UIDs must be unique per message type
2. **Stale time**: Must be > time, recommend minimum 5 seconds
3. **Position bounds**: lat ∈ [-90, 90], lon ∈ [-180, 180]
4. **CE/LE values**: Use 9999999.0 for unknown (CoT convention)
5. **Type format**: Must follow MIL-STD-2525 atom format

---

## 8. Error Handling

### Encoding Errors

| Error | Handling |
|-------|----------|
| Missing required field | Return `EncodingError::MissingField(field_name)` |
| Invalid position | Return `EncodingError::InvalidPosition` |
| Unknown classification | Use `a-u-G` (unknown ground) and log warning |

### Decoding Errors

| Error | Handling |
|-------|----------|
| Malformed XML | Return `DecodingError::MalformedXml(details)` |
| Unknown CoT type | Log and skip, or map to generic PEAT message |
| Missing required element | Return `DecodingError::MissingElement(name)` |
| Invalid timestamp | Return `DecodingError::InvalidTimestamp` |

---

## References

- [CoT XML Schema](http://cot.mitre.org)
- [MIL-STD-2525D](https://www.jcs.mil/Portals/36/Documents/Doctrine/Other_Pubs/ms_2525d.pdf)
- [TAK Protocol Documentation](https://tak.gov)
- [peat-m1-poc/src/messages.rs](../src/messages.rs)
- [TAK Integration Requirements](./TAK_INTEGRATION_REQUIREMENTS.md)
