# Sprint 1 Issues - Bulk Creation Template

This file contains the initial issues for Sprint 1. Use the GitHub CLI or copy/paste into GitHub.

## GitHub CLI Commands

```bash
# Set repo
REPO="kitplummer/hive"

# Core Team Issues
gh issue create --repo $REPO \
  --title "[SCHEMA] Define CapabilityAdvertisement schema" \
  --label "team/core,type/schema,phase/1-init,priority/p1-critical" \
  --body "## Schema Definition: CapabilityAdvertisement

**Phase**: Phase 1 - Initialization & Capability Advertisement

### Purpose
Define the JSON schema for AI model capability advertisements that flow from edge nodes (Jetson) through the HIVE hierarchy to C2.

### Required Fields
- platform_id
- advertised_at (timestamp)
- models[] (array of model capabilities)
- resources (optional, GPU/memory status)

### Acceptance Criteria
- [ ] JSON Schema validates against draft-07
- [ ] Example instance provided and validated
- [ ] AI team can serialize matching struct
- [ ] Core validation library accepts/rejects correctly

### References
- Vignette Section 4.1: Phase 1 Flow
- ADR-018: AI Model Capability Advertisement
- Contract: docs/contracts/CONTRACT_CORE_AI_CAPABILITY.md"

gh issue create --repo $REPO \
  --title "[SCHEMA] Define TrackUpdate schema" \
  --label "team/core,type/schema,phase/3-tracking,priority/p1-critical" \
  --body "## Schema Definition: TrackUpdate

**Phase**: Phase 3 - Active Tracking

### Purpose
Define the JSON schema for track updates that flow from AI inference (Jetson) through HIVE to C2/TAK.

### Required Fields
- track_id
- classification
- confidence
- position (lat, lon, cep_m)
- velocity (bearing, speed_mps)
- source_platform
- source_model
- model_version
- timestamp

### Acceptance Criteria
- [ ] JSON Schema validates against draft-07
- [ ] Example instance provided
- [ ] AI team can serialize
- [ ] ATAK team can convert to CoT

### References
- Vignette Section 4.3: Phase 3 Flow
- Contract: docs/contracts/CONTRACT_CORE_ATAK_TAK_BRIDGE.md"

gh issue create --repo $REPO \
  --title "[SCHEMA] Define MissionTask schema" \
  --label "team/core,type/schema,phase/2-tasking,priority/p1-critical" \
  --body "## Schema Definition: MissionTask

**Phase**: Phase 2 - Mission Tasking

### Purpose
Define the JSON schema for mission tasks that flow downward from C2 (via TAK) through HIVE to teams.

### Required Fields
- task_id
- task_type (TRACK_TARGET, SEARCH_AREA, etc.)
- issued_at
- issued_by
- expires_at
- target (description, last_known_position)
- boundary (polygon or circle)
- priority

### Acceptance Criteria
- [ ] JSON Schema validates against draft-07
- [ ] ATAK team can convert from CoT
- [ ] Teams can deserialize and display

### References
- Vignette Section 4.2: Phase 2 Flow
- Contract: docs/contracts/CONTRACT_CORE_ATAK_TAK_BRIDGE.md"

gh issue create --repo $REPO \
  --title "[FEATURE] Implement Automerge document sync foundation" \
  --label "team/core,type/enhancement,component/automerge,priority/p1-critical" \
  --body "## Feature: Automerge Document Sync

### Purpose
Implement the foundational Automerge + Iroh sync mechanism for HIVE Protocol documents.

### Requirements
- [ ] Create/open Automerge documents
- [ ] Sync changes between two nodes
- [ ] Handle reconnection after network partition
- [ ] Store documents persistently (RocksDB)

### Acceptance Criteria
- [ ] Two nodes can sync a shared document
- [ ] Changes propagate within 2 seconds
- [ ] Document survives node restart

### References
- ADR-007: Automerge-Based Sync Engine
- ADR-011: Ditto vs Automerge/Iroh comparison"

gh issue create --repo $REPO \
  --title "[FEATURE] Create schema validation library" \
  --label "team/core,type/enhancement,component/schema,priority/p2-normal" \
  --body "## Feature: Schema Validation Library

### Purpose
Provide a validation function/library that all teams can use to validate messages against HIVE schemas.

### Requirements
- [ ] validate_capability(json) -> Result
- [ ] validate_track_update(json) -> Result
- [ ] validate_mission_task(json) -> Result
- [ ] Clear error messages for validation failures

### Deliverable
\`hive-core/src/validate.rs\` or similar

### Acceptance Criteria
- [ ] Valid messages pass
- [ ] Invalid messages rejected with clear error
- [ ] AI and ATAK teams can integrate"

# ATAK Team Issues
gh issue create --repo $REPO \
  --title "[FEATURE] Set up Android development environment" \
  --label "team/atak,type/enhancement,priority/p1-critical" \
  --body "## Feature: Android Development Environment

### Purpose
Set up the Android development environment for ATAK plugin development.

### Requirements
- [ ] Android Studio installed
- [ ] ATAK SDK configured
- [ ] Build system working
- [ ] Emulator or test device available

### Acceptance Criteria
- [ ] Can build empty ATAK plugin
- [ ] Plugin loads in ATAK"

gh issue create --repo $REPO \
  --title "[CONTRACT] Define CoT ↔ HIVE message mapping" \
  --label "team/atak,type/contract,type/integration,priority/p1-critical" \
  --body "## Contract: CoT ↔ HIVE Mapping

### Purpose
Define the mapping between HIVE Protocol messages and Cursor-on-Target (CoT) XML.

### Mappings Needed
1. TrackUpdate (HIVE) → Position Event (CoT)
2. CapabilityAdvertisement (HIVE) → Registration (CoT)
3. Mission Task (CoT) → MissionTask (HIVE)

### Deliverable
Mapping table in contract document

### References
- docs/contracts/CONTRACT_CORE_ATAK_TAK_BRIDGE.md
- MIL-STD-2525 symbology
- CoT specification"

gh issue create --repo $REPO \
  --title "[FEATURE] Scaffold HIVE-TAK Bridge application" \
  --label "team/atak,type/enhancement,component/tak-bridge,priority/p1-critical" \
  --body "## Feature: HIVE-TAK Bridge Scaffold

### Purpose
Create the skeleton application for the HIVE-TAK Bridge that will translate between protocols.

### Requirements
- [ ] Application structure
- [ ] HIVE client connection (placeholder)
- [ ] TAK Server connection (CoT/TCP)
- [ ] Configuration file support

### Acceptance Criteria
- [ ] Application starts without errors
- [ ] Logs connection status"

gh issue create --repo $REPO \
  --title "[INTEGRATION] Connect Bridge to TAK Server" \
  --label "team/atak,type/integration,component/tak-bridge,priority/p2-normal" \
  --body "## Integration: Bridge → TAK Server

### Purpose
Establish connection from HIVE-TAK Bridge to TAK Server and send/receive CoT.

### Requirements
- [ ] Connect to TAK Server (CoT/TCP)
- [ ] Send test CoT event
- [ ] Receive CoT events
- [ ] Handle reconnection

### Dependencies
- Experiments team: TAK Server running in Containerlab

### Acceptance Criteria
- [ ] Bridge connects to TAK Server
- [ ] Test CoT appears in WebTAK"

# Experiments Team Issues
gh issue create --repo $REPO \
  --title "[FEATURE] Create Containerlab topology for demo" \
  --label "team/experiments,type/enhancement,component/containerlab,priority/p1-critical" \
  --body "## Feature: Containerlab Topology

### Purpose
Create the Containerlab topology file that defines the demo network infrastructure.

### Topology
- C2 network (TAK Server, WebTAK, MLOps)
- Coordinator (Bridge node)
- Network A (Alpha team: operator, UGV, Jetson)
- Network B (Bravo team: operator, UAV, Jetson)

### Deliverable
\`hive-demo.clab.yml\`

### Acceptance Criteria
- [ ] \`clab deploy\` succeeds
- [ ] All nodes reachable
- [ ] Network separation enforced"

gh issue create --repo $REPO \
  --title "[FEATURE] Deploy TAK Server container" \
  --label "team/experiments,type/enhancement,priority/p1-critical" \
  --body "## Feature: TAK Server Container

### Purpose
Deploy TAK Server in Containerlab for demo infrastructure.

### Requirements
- [ ] TAK Server container image
- [ ] Configuration for demo
- [ ] WebTAK enabled
- [ ] CoT/TCP port exposed

### Acceptance Criteria
- [ ] TAK Server running
- [ ] WebTAK accessible
- [ ] Can receive CoT events"

gh issue create --repo $REPO \
  --title "[FEATURE] Create network scenario scripts" \
  --label "team/experiments,type/enhancement,component/containerlab,priority/p2-normal" \
  --body "## Feature: Network Scenario Scripts

### Purpose
Create scripts to configure different network conditions for testing.

### Scenarios
- nominal: Baseline network
- contested-light: 30% loss, 200ms latency
- contested-heavy: 50% loss, 500ms latency
- bandwidth-limited: 100 Kbps
- partition-alpha: Network A isolated

### Deliverable
\`scripts/set-network.sh\`

### Acceptance Criteria
- [ ] Scripts change network conditions
- [ ] Changes verifiable with ping/iperf"

gh issue create --repo $REPO \
  --title "[FEATURE] Define validation metrics and collection" \
  --label "team/experiments,type/validation,priority/p1-critical" \
  --body "## Feature: Validation Metrics

### Purpose
Define the metrics that will validate demo success and how to collect them.

### Metrics
- P1: Track update latency < 2s
- P2: Bandwidth < 10 Kbps
- P3: Handoff gap < 10s
- P4: Model distribution < 5 min
- P5: Hot-swap interruption < 5s

### Collection Methods
- Timestamp analysis
- tcpdump/bandwidth measurement
- Gap analysis

### Deliverable
Metrics spec document

### References
- Vignette Section 6: Success Metrics"

# AI Team Issues
gh issue create --repo $REPO \
  --title "[FEATURE] Set up Jetson Orin Nano development environment" \
  --label "team/ai,type/enhancement,component/jetson,priority/p1-critical" \
  --body "## Feature: Jetson Environment Setup

### Purpose
Set up the Jetson Orin Nano Super Dev environment for edge AI development.

### Requirements
- [ ] JetPack SDK installed
- [ ] CUDA/cuDNN configured
- [ ] Python environment
- [ ] Camera/video input working

### Acceptance Criteria
- [ ] Jetson boots and is accessible
- [ ] Can run GPU test"

gh issue create --repo $REPO \
  --title "[FEATURE] Install YOLOv8 + DeepSORT on Jetson" \
  --label "team/ai,type/enhancement,component/jetson,priority/p1-critical" \
  --body "## Feature: YOLOv8 + DeepSORT Installation

### Purpose
Install and configure YOLOv8 object detection and DeepSORT tracking on Jetson.

### Requirements
- [ ] YOLOv8 (ultralytics) installed
- [ ] DeepSORT installed
- [ ] Model weights downloaded
- [ ] Test inference working

### Acceptance Criteria
- [ ] Inference runs on test video
- [ ] Achieves ~15 FPS
- [ ] Detections logged"

gh issue create --repo $REPO \
  --title "[FEATURE] Implement capability struct and serialization" \
  --label "team/ai,type/enhancement,phase/1-init,priority/p1-critical" \
  --body "## Feature: Capability Struct

### Purpose
Implement the Rust struct for CapabilityAdvertisement and JSON serialization.

### Requirements
- [ ] Struct matches Core schema
- [ ] Serialize to JSON
- [ ] Include model performance metrics
- [ ] Include resource status

### Dependencies
- Core team: CapabilityAdvertisement schema

### Acceptance Criteria
- [ ] Serialized JSON validates against Core schema
- [ ] All required fields populated"

echo "✅ Sprint 1 issues created!"
```

## Manual Issue Creation

If you prefer to create issues manually, use the following information:

---

### Core Team Issues

**Issue 1: [SCHEMA] Define CapabilityAdvertisement schema**
- Labels: `team/core`, `type/schema`, `phase/1-init`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 2: [SCHEMA] Define TrackUpdate schema**
- Labels: `team/core`, `type/schema`, `phase/3-tracking`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 3: [SCHEMA] Define MissionTask schema**
- Labels: `team/core`, `type/schema`, `phase/2-tasking`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 4: [FEATURE] Implement Automerge document sync foundation**
- Labels: `team/core`, `type/enhancement`, `component/automerge`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 5: [FEATURE] Create schema validation library**
- Labels: `team/core`, `type/enhancement`, `component/schema`, `priority/p2-normal`
- Milestone: Sprint 1

---

### ATAK Team Issues

**Issue 6: [FEATURE] Set up Android development environment**
- Labels: `team/atak`, `type/enhancement`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 7: [CONTRACT] Define CoT ↔ HIVE message mapping**
- Labels: `team/atak`, `type/contract`, `type/integration`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 8: [FEATURE] Scaffold HIVE-TAK Bridge application**
- Labels: `team/atak`, `type/enhancement`, `component/tak-bridge`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 9: [INTEGRATION] Connect Bridge to TAK Server**
- Labels: `team/atak`, `type/integration`, `component/tak-bridge`, `priority/p2-normal`
- Milestone: Sprint 1
- Depends on: Experiments team TAK Server

---

### Experiments Team Issues

**Issue 10: [FEATURE] Create Containerlab topology for demo**
- Labels: `team/experiments`, `type/enhancement`, `component/containerlab`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 11: [FEATURE] Deploy TAK Server container**
- Labels: `team/experiments`, `type/enhancement`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 12: [FEATURE] Create network scenario scripts**
- Labels: `team/experiments`, `type/enhancement`, `component/containerlab`, `priority/p2-normal`
- Milestone: Sprint 1

**Issue 13: [FEATURE] Define validation metrics and collection**
- Labels: `team/experiments`, `type/validation`, `priority/p1-critical`
- Milestone: Sprint 1

---

### AI Team Issues

**Issue 14: [FEATURE] Set up Jetson Orin Nano development environment**
- Labels: `team/ai`, `type/enhancement`, `component/jetson`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 15: [FEATURE] Install YOLOv8 + DeepSORT on Jetson**
- Labels: `team/ai`, `type/enhancement`, `component/jetson`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 16: [FEATURE] Implement capability struct and serialization**
- Labels: `team/ai`, `type/enhancement`, `phase/1-init`, `priority/p1-critical`
- Milestone: Sprint 1
- Depends on: Core team schema

---

### PM Team Issues

**Issue 17: [PM] Review and approve interface contracts**
- Labels: `team/pm`, `type/contract`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 18: [PM] Set up GitHub Projects board**
- Labels: `team/pm`, `type/enhancement`, `priority/p1-critical`
- Milestone: Sprint 1

**Issue 19: [PM] Draft demo script outline**
- Labels: `team/pm`, `type/documentation`, `priority/p2-normal`
- Milestone: Sprint 1
