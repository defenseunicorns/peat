# Interface Contract: Core ↔ AI (MLOps)

**Document Version**: 1.0  
**Status**: Draft - Awaiting Approval  
**Owner Team**: Core (defines schema, implements distribution)  
**Consumer Team**: AI (implements model hot-swap)  
**Required By**: Phase 5 - MLOps Model Distribution

---

## Overview

This contract defines the interface for distributing AI model updates through the HIVE hierarchy. The MLOps server pushes model packages downward through the coordinator to edge Jetson nodes, which perform hot-swap deployment without interrupting tracking.

## Data Flow

```
┌─────────────────┐                                      
│  MLOps Server   │  Model Training Complete             
│  (C2 Element)   │                                      
└────────┬────────┘                                      
         │ Model Package                                 
         ▼                                               
┌─────────────────┐                                      
│   Coordinator   │  Caches model, initiates distribution
│   (Bridge)      │                                      
└────────┬────────┘                                      
         │ HIVE Sync (chunked blob)                      
    ┌────┴────┐                                          
    ▼         ▼                                          
┌───────┐ ┌───────┐                                      
│Alpha-3│ │Bravo-3│  Edge nodes receive, verify, hot-swap
│Jetson │ │Jetson │                                      
└───────┘ └───────┘                                      
```

---

## Schema: ModelUpdatePackage

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://revolveteam.com/hive/schemas/model-update-package.json",
  "title": "ModelUpdatePackage",
  "type": "object",
  "required": ["package_type", "model_id", "model_version", "model_hash", "model_size_bytes", "blob_reference", "target_platforms", "deployment_policy"],
  "properties": {
    "package_type": {
      "type": "string",
      "const": "AI_MODEL_UPDATE"
    },
    "model_id": {
      "type": "string",
      "description": "Model identifier (e.g., 'object_tracker')"
    },
    "model_version": {
      "type": "string",
      "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$",
      "description": "Semantic version"
    },
    "model_hash": {
      "type": "string",
      "pattern": "^sha256:[a-f0-9]{64}$",
      "description": "SHA-256 hash of model file"
    },
    "model_size_bytes": {
      "type": "integer",
      "minimum": 1,
      "description": "Size of model file in bytes"
    },
    "blob_reference": {
      "type": "string",
      "pattern": "^hive://blobs/sha256:[a-f0-9]{64}$",
      "description": "HIVE blob store reference"
    },
    "target_platforms": {
      "type": "array",
      "items": { "type": "string" },
      "minItems": 1,
      "description": "Platform IDs to receive update"
    },
    "deployment_policy": {
      "type": "string",
      "enum": ["IMMEDIATE", "ROLLING", "SCHEDULED", "MANUAL"],
      "description": "How to deploy the update"
    },
    "rollback_version": {
      "type": "string",
      "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$",
      "description": "Version to rollback to if deployment fails"
    },
    "metadata": {
      "type": "object",
      "properties": {
        "changelog": { "type": "string" },
        "training_date": { "type": "string", "format": "date" },
        "validation_accuracy": { "type": "number", "minimum": 0, "maximum": 1 },
        "training_dataset": { "type": "string" },
        "compatible_hardware": {
          "type": "array",
          "items": { "type": "string" }
        }
      }
    },
    "issued_at": {
      "type": "string",
      "format": "date-time"
    },
    "issued_by": {
      "type": "string",
      "description": "Identity of issuer"
    }
  }
}
```

### Example Instance

```json
{
  "package_type": "AI_MODEL_UPDATE",
  "model_id": "object_tracker",
  "model_version": "1.3.0",
  "model_hash": "sha256:b8d9c4e2f1a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9",
  "model_size_bytes": 45000000,
  "blob_reference": "hive://blobs/sha256:b8d9c4e2f1a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9",
  "target_platforms": ["Alpha-3", "Bravo-3"],
  "deployment_policy": "ROLLING",
  "rollback_version": "1.2.0",
  "metadata": {
    "changelog": "Improved low-light detection, reduced false positives by 15%",
    "training_date": "2025-12-07",
    "validation_accuracy": 0.94,
    "training_dataset": "combined-field-20251207",
    "compatible_hardware": ["jetson-orin-nano", "jetson-xavier-nx"]
  },
  "issued_at": "2025-12-08T14:20:00Z",
  "issued_by": "mlops-server-c2"
}
```

---

## Schema: ModelDeploymentStatus

Edge nodes report deployment status back up the hierarchy.

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://revolveteam.com/hive/schemas/model-deployment-status.json",
  "title": "ModelDeploymentStatus",
  "type": "object",
  "required": ["platform_id", "model_id", "model_version", "status", "reported_at"],
  "properties": {
    "platform_id": { "type": "string" },
    "model_id": { "type": "string" },
    "model_version": { "type": "string" },
    "status": {
      "type": "string",
      "enum": ["PENDING", "DOWNLOADING", "VERIFYING", "DEPLOYING", "ACTIVE", "FAILED", "ROLLED_BACK"]
    },
    "progress_percent": {
      "type": "integer",
      "minimum": 0,
      "maximum": 100
    },
    "error_message": {
      "type": "string",
      "description": "Error details if status is FAILED"
    },
    "previous_version": {
      "type": "string",
      "description": "Version that was replaced"
    },
    "reported_at": {
      "type": "string",
      "format": "date-time"
    }
  }
}
```

---

## Protocol / Transport

### Model Package Metadata
| Property | Value |
|----------|-------|
| Transport | HIVE Automerge document sync |
| Collection | `model_updates` |
| Document ID | `{model_id}-{model_version}` |
| Direction | Downward (C2 → Edge) |

### Model Blob Transfer
| Property | Value |
|----------|-------|
| Transport | HIVE blob store (chunked) |
| Chunk Size | 64 KB |
| Resume | Yes (content-addressable) |
| Verification | SHA-256 hash check |
| Caching | Coordinator caches for team distribution |

### Deployment Status
| Property | Value |
|----------|-------|
| Transport | HIVE Automerge document sync |
| Collection | `deployment_status` |
| Document ID | `{platform_id}-{model_id}` |
| Direction | Upward (Edge → C2) |

---

## Deployment Policies

### IMMEDIATE
- All target platforms begin download simultaneously
- No coordination between platforms
- Use for critical security patches

### ROLLING (Default for Demo)
- Deploy to one platform at a time
- Wait for ACTIVE status before next platform
- Maintains at least one operational tracker during deployment
- Automatic rollback if any platform fails

### SCHEDULED
- Deploy at specified time
- All platforms deploy simultaneously at trigger time

### MANUAL
- Package distributed but not deployed
- Operator must approve deployment per-platform

---

## Hot-Swap Procedure (AI Team)

```
1. RECEIVE package metadata via HIVE sync
2. VERIFY platform_id in target_platforms
3. UPDATE status: PENDING → DOWNLOADING
4. DOWNLOAD blob chunks, resume if interrupted
5. UPDATE status: DOWNLOADING → VERIFYING (report progress %)
6. VERIFY hash matches model_hash
   - If mismatch: status = FAILED, error_message = "Hash mismatch"
7. UPDATE status: VERIFYING → DEPLOYING
8. STOP current inference (brief pause acceptable, <5s)
9. LOAD new model into memory
10. WARM UP model (run inference on test frame)
11. RESUME inference with new model
12. UPDATE status: DEPLOYING → ACTIVE
13. EMIT new CapabilityAdvertisement with updated version

ROLLBACK TRIGGER:
- Hash verification fails
- Model load fails
- Warmup inference fails
- Performance degrades >20% after deployment

ROLLBACK PROCEDURE:
1. STOP new model
2. LOAD rollback_version model
3. RESUME inference
4. UPDATE status: ROLLED_BACK
5. EMIT CapabilityAdvertisement with rollback version
```

---

## Core Team Responsibilities

- [ ] Define ModelUpdatePackage and ModelDeploymentStatus schemas
- [ ] Implement blob store with chunked transfer
- [ ] Implement content-addressable caching at coordinator
- [ ] Route model packages to target platforms only
- [ ] Aggregate deployment status for C2 visibility
- [ ] Implement rolling deployment coordinator logic

## AI Team Responsibilities

- [ ] Implement model download with resume capability
- [ ] Implement hash verification
- [ ] Implement hot-swap without tracking interruption >5s
- [ ] Report deployment status at each stage
- [ ] Implement automatic rollback on failure
- [ ] Re-advertise capability after successful deployment
- [ ] Support multiple model versions cached locally

---

## Acceptance Criteria

### Distribution
- [ ] 45 MB model distributes in <5 minutes over 500 Kbps link
- [ ] Resume works after network interruption
- [ ] Coordinator caches model (doesn't re-download for Bravo after Alpha)

### Verification
- [ ] Hash verified before deployment
- [ ] Corrupted model detected and rejected
- [ ] Status reports hash verification failure

### Hot-Swap
- [ ] Tracking interruption <5 seconds
- [ ] New model active within 30 seconds of download complete
- [ ] Capability re-advertised within 10 seconds of model swap

### Rollback
- [ ] Failed deployment triggers automatic rollback
- [ ] Rollback completes within 30 seconds
- [ ] Tracking resumes on previous model

### Status Reporting
- [ ] Status updates propagate to C2 within 5 seconds
- [ ] C2 sees aggregated view of all platform deployment status
- [ ] Progress percentage accurate to ±5%

---

## Error Handling

| Error | AI Team Action | Core Team Action |
|-------|---------------|-----------------|
| Download timeout | Retry 3x, then FAILED | Re-queue for retry |
| Hash mismatch | FAILED status, don't deploy | Alert MLOps server |
| Model load failure | Rollback, ROLLED_BACK status | Log for diagnostics |
| Inference failure | Rollback, ROLLED_BACK status | Halt rolling deployment |
| Storage full | FAILED status with error | Alert, manual intervention |

---

## Performance Requirements

| Metric | Target | Validation |
|--------|--------|------------|
| Distribution latency (45 MB @ 500 Kbps) | < 5 minutes | Timing measurement |
| Hot-swap interruption | < 5 seconds | Gap analysis in tracks |
| Capability re-advertisement | < 10 seconds | Log timing |
| Status propagation | < 5 seconds | Timestamp comparison |
| Rollback time | < 30 seconds | Fault injection test |

---

## Approval

| Team | Approver | Date | Signature |
|------|----------|------|-----------|
| Core | | | ☐ Approved |
| AI | | | ☐ Approved |

---

*Document maintained by (r)evolve - Revolve Team LLC*  
*https://revolveteam.com*
