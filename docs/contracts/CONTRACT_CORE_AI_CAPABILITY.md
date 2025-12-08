# Interface Contract: Core ↔ AI

**Document Version**: 1.0  
**Status**: Draft - Awaiting Approval  
**Owner Team**: Core (defines schema)  
**Consumer Team**: AI (implements)  
**Required By**: Phase 1 - Initialization & Capability Advertisement

---

## Overview

This contract defines the interface between the Core protocol team and the AI team for capability advertisement. The AI team's Jetson nodes emit capability advertisements that the Core protocol synchronizes through the HIVE hierarchy.

## Data Flow

```
┌─────────────────┐    HIVE Sync    ┌─────────────────┐    HIVE Sync    ┌─────────────────┐
│   Jetson Node   │ ──────────────► │   Team Leader   │ ──────────────► │   Coordinator   │
│   (Alpha-3)     │                 │   (Alpha-1)     │                 │   (Bridge)      │
│                 │                 │                 │                 │                 │
│ AI Team Impl    │                 │ Core Protocol   │                 │ Core Protocol   │
└─────────────────┘                 └─────────────────┘                 └─────────────────┘
```

## Schema: CapabilityAdvertisement

### JSON Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://revolveteam.com/hive/schemas/capability-advertisement.json",
  "title": "CapabilityAdvertisement",
  "description": "AI model capability advertisement for HIVE Protocol",
  "type": "object",
  "required": ["platform_id", "advertised_at", "models"],
  "properties": {
    "platform_id": {
      "type": "string",
      "description": "Unique identifier for the platform (e.g., 'Alpha-3')",
      "pattern": "^[A-Za-z]+-[0-9]+$"
    },
    "advertised_at": {
      "type": "string",
      "format": "date-time",
      "description": "ISO 8601 timestamp of advertisement"
    },
    "models": {
      "type": "array",
      "description": "List of AI models available on this platform",
      "items": {
        "$ref": "#/definitions/ModelCapability"
      },
      "minItems": 0
    },
    "resources": {
      "$ref": "#/definitions/ResourceStatus"
    }
  },
  "definitions": {
    "ModelCapability": {
      "type": "object",
      "required": ["model_id", "model_version", "model_hash", "model_type", "performance", "operational_status"],
      "properties": {
        "model_id": {
          "type": "string",
          "description": "Identifier for the model (e.g., 'object_tracker')"
        },
        "model_version": {
          "type": "string",
          "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$",
          "description": "Semantic version of the model"
        },
        "model_hash": {
          "type": "string",
          "pattern": "^sha256:[a-f0-9]{64}$",
          "description": "SHA-256 hash of the model weights"
        },
        "model_type": {
          "type": "string",
          "enum": ["detector", "tracker", "detector_tracker", "classifier", "segmenter"],
          "description": "Type of AI model"
        },
        "performance": {
          "$ref": "#/definitions/PerformanceMetrics"
        },
        "operational_status": {
          "type": "string",
          "enum": ["READY", "LOADING", "DEGRADED", "OFFLINE", "UPDATING"],
          "description": "Current operational status"
        },
        "input_signature": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Expected input format (e.g., ['video_stream', '640x480'])"
        },
        "output_signature": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Output format (e.g., ['bounding_boxes', 'track_ids'])"
        }
      }
    },
    "PerformanceMetrics": {
      "type": "object",
      "required": ["precision", "recall", "fps"],
      "properties": {
        "precision": {
          "type": "number",
          "minimum": 0,
          "maximum": 1,
          "description": "Model precision (0-1)"
        },
        "recall": {
          "type": "number",
          "minimum": 0,
          "maximum": 1,
          "description": "Model recall (0-1)"
        },
        "fps": {
          "type": "integer",
          "minimum": 0,
          "description": "Inference frames per second"
        },
        "latency_ms": {
          "type": "integer",
          "minimum": 0,
          "description": "Average inference latency in milliseconds"
        }
      }
    },
    "ResourceStatus": {
      "type": "object",
      "properties": {
        "gpu_utilization": {
          "type": "number",
          "minimum": 0,
          "maximum": 1,
          "description": "GPU utilization (0-1)"
        },
        "memory_used_mb": {
          "type": "integer",
          "minimum": 0,
          "description": "Memory used in MB"
        },
        "memory_total_mb": {
          "type": "integer",
          "minimum": 0,
          "description": "Total memory in MB"
        },
        "temperature_c": {
          "type": "number",
          "description": "GPU/CPU temperature in Celsius"
        }
      }
    }
  }
}
```

### Example Instance

```json
{
  "platform_id": "Alpha-3",
  "advertised_at": "2025-12-08T14:25:00Z",
  "models": [{
    "model_id": "object_tracker",
    "model_version": "1.2.0",
    "model_hash": "sha256:a7f8b3c1d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1",
    "model_type": "detector_tracker",
    "performance": {
      "precision": 0.91,
      "recall": 0.87,
      "fps": 15,
      "latency_ms": 67
    },
    "operational_status": "READY",
    "input_signature": ["video_stream", "640x480", "RGB"],
    "output_signature": ["bounding_boxes", "track_ids", "classifications", "confidence"]
  }],
  "resources": {
    "gpu_utilization": 0.65,
    "memory_used_mb": 2048,
    "memory_total_mb": 4096,
    "temperature_c": 52.5
  }
}
```

## Protocol / Transport

| Property | Value |
|----------|-------|
| Transport | HIVE Automerge document sync |
| Collection | `capabilities` |
| Document ID | `{platform_id}` (e.g., `Alpha-3`) |
| Sync Frequency | On change + heartbeat every 30 seconds |
| Conflict Resolution | Last-writer-wins on `advertised_at` |

## Core Team Responsibilities

- [ ] Define and publish JSON Schema (this document)
- [ ] Implement schema validation in Core library
- [ ] Provide validation function: `validate_capability(json) -> Result<(), ValidationError>`
- [ ] Create mock capability data for AI team testing
- [ ] Document Automerge collection structure
- [ ] Implement capability aggregation at team/formation levels

## AI Team Responsibilities

- [ ] Implement Rust struct matching schema
- [ ] Serialize to JSON matching schema exactly
- [ ] Emit advertisement on:
  - Node startup (initial advertisement)
  - Model load/unload
  - Status change (READY → DEGRADED, etc.)
  - Performance metric update (>5% change)
  - Heartbeat (every 30 seconds if no other update)
- [ ] Handle serialization errors gracefully (log, retry)
- [ ] Pass validation against Core's validator

## Acceptance Criteria

### Schema Validation
- [ ] AI team's output validates against JSON Schema
- [ ] Core team can deserialize AI team's output without errors
- [ ] All required fields present
- [ ] All field types match specification

### Integration Test
- [ ] Jetson node emits capability on startup
- [ ] Capability syncs to team leader within 2 seconds
- [ ] Capability syncs to coordinator within 5 seconds
- [ ] Status change propagates within 2 seconds
- [ ] Heartbeat maintains presence (no stale detection false positives)

### Performance
- [ ] Serialization: < 1ms
- [ ] Message size: < 2KB typical, < 5KB max
- [ ] Sync latency: < 2 seconds edge-to-coordinator

## Error Handling

| Error | Producer (AI) Action | Consumer (Core) Action |
|-------|---------------------|------------------------|
| Serialization failure | Log error, retry once, skip if still fails | N/A |
| Invalid schema | N/A | Log error, reject message, continue |
| Missing required field | N/A | Log error, reject message |
| Network failure | Buffer locally, retry with exponential backoff | Detect stale (>60s), mark UNKNOWN |
| Model crash | Emit OFFLINE status | Propagate status up hierarchy |

## Change Management

1. Schema changes require both teams to approve PR
2. Breaking changes require version bump in schema `$id`
3. New optional fields can be added without version bump
4. Deprecation: Mark field as deprecated, remove after 2 sprints

## Approval

| Team | Approver | Date | Signature |
|------|----------|------|-----------|
| Core | | | ☐ Approved |
| AI | | | ☐ Approved |

---

*Document maintained by (r)evolve - Revolve Team LLC*  
*https://revolveteam.com*
