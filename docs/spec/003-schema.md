# Peat Protocol Specification: Data Schema Definitions

**Spec ID**: Peat-SPEC-003
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: Defense Unicorns

## Abstract

This document specifies the data schemas for Peat Protocol. It defines the Protocol Buffer message formats for tactical entities, their relationships, and mapping to external standards (CoT/TAK).

## Table of Contents

1. [Introduction](#1-introduction)
2. [Schema Organization](#2-schema-organization)
3. [Core Schemas](#3-core-schemas)
4. [Beacon and Tracking](#4-beacon-and-tracking)
5. [Mission and Tasking](#5-mission-and-tasking)
6. [Capability Advertisement](#6-capability-advertisement)
7. [Security Schemas](#7-security-schemas)
8. [AI/ML Schemas](#8-aiml-schemas)
9. [CoT/TAK Mapping](#9-cottak-mapping)
10. [Schema Evolution](#10-schema-evolution)
11. [Validation](#11-validation)

---

## 1. Introduction

### 1.1 Purpose

Peat schemas define the structure of all data exchanged between nodes. Using Protocol Buffers ensures:
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
peat-schema/proto/
├── peat/
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
package peat.common.v1;

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
package peat.beacon.v1;

import "peat/common/v1/common.proto";

// Track update from a node
message Beacon {
    // Unique identifier for this track
    peat.common.v1.UUID track_id = 1;

    // Device that produced this beacon
    bytes device_id = 2;

    // Callsign for display
    peat.common.v1.Callsign callsign = 3;

    // Current position
    peat.common.v1.Position position = 4;

    // Timestamp of position fix
    peat.common.v1.Timestamp timestamp = 5;

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
    peat.common.v1.UUID cell_id = 1;

    // Time range covered
    peat.common.v1.TimeRange time_range = 2;

    // Bounding box containing all tracks
    peat.common.v1.BoundingBox coverage = 3;

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
package peat.mission.v1;

import "peat/common/v1/common.proto";

// Mission definition
message Mission {
    // Unique mission identifier
    peat.common.v1.UUID mission_id = 1;

    // Human-readable name
    string name = 2;

    // Mission type
    MissionType type = 3;

    // Issuing authority
    string issuing_authority = 4;

    // Priority level
    Priority priority = 5;

    // Time window
    peat.common.v1.TimeRange time_window = 6;

    // Area of operations
    AreaOfOperations aoo = 7;

    // Assigned cells/units
    repeated peat.common.v1.UUID assigned_cells = 8;

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
    peat.common.v1.Position center = 1;
    double radius_meters = 2;
}

message PolygonArea {
    repeated peat.common.v1.Position vertices = 1;
}

message RouteArea {
    repeated peat.common.v1.Position waypoints = 1;
    double corridor_width_meters = 2;
}
```

### 5.2 Objective

```protobuf
// Individual objective within a mission
message Objective {
    peat.common.v1.UUID objective_id = 1;
    string description = 2;
    ObjectiveType type = 3;
    peat.common.v1.Position location = 4;
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
package peat.capability.v1;

import "peat/common/v1/common.proto";

// Node capability advertisement
message CapabilityAdvertisement {
    // Device advertising capabilities
    bytes device_id = 1;

    // Callsign
    peat.common.v1.Callsign callsign = 2;

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
    peat.common.v1.Timestamp timestamp = 10;
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
    LINK_TYPE_MESH = 1;         // Peat mesh
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
    optional peat.common.v1.Timestamp available_at = 3;
}
```

---

## 7. Security Schemas

### 7.1 Device Identity

```protobuf
syntax = "proto3";
package peat.security.v1;

import "peat/common/v1/common.proto";

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
    peat.common.v1.Timestamp timestamp = 2;
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
package peat.ai.v1;

import "peat/common/v1/common.proto";

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

Peat beacons map to CoT events:

| Peat Field | CoT Field | Notes |
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
Peat Affiliation + Dimension → CoT Type

MEMBER + GROUND     → a-f-G
MEMBER + AIR        → a-f-A
EXTERNAL + GROUND   → a-h-G
NEUTRAL + SURFACE   → a-n-S
UNKNOWN + AIR       → a-u-A
```

### 9.3 CoT Detail Extensions

Peat-specific data is carried in CoT `<detail>` elements:

```xml
<detail>
    <__peat>
        <device_id>0x1234...</device_id>
        <cell_id>uuid</cell_id>
        <power_level>85</power_level>
        <capabilities>sensor,relay</capabilities>
    </__peat>
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
