# ADR-018: AI Model Capability Advertisement and Metadata Standards

**Status**: Proposed  
**Date**: 2025-11-16  
**Authors**: Claude, Kit Plummer  
**Relates to**: ADR-009 (Bidirectional Flows), ADR-012 (Schema Definition), ADR-013 (Distributed Software/AI Operations), ADR-001 (CAP Protocol)

## Context

### The AI Model Discovery Problem

Modern military autonomous systems increasingly depend on AI/ML models deployed at the edge for critical capabilities:
- **Target recognition and classification** (ISR platforms)
- **Autonomous navigation and obstacle avoidance** (ground/air vehicles)
- **Communications intelligence and signal processing** (EW systems)
- **Predictive maintenance and diagnostics** (all platforms)
- **Decision support and mission planning** (C2 systems)

However, distributed autonomous operations face a fundamental **model discovery and capability matching problem**:

**Scenario: Company-Level ISR Coordination**
- Platoon Alpha has 8 platforms with target_recognition_v4.2.1 (precision: 0.94, recall: 0.89)
- Platoon Bravo has 12 platforms with target_recognition_v3.8.0 (precision: 0.87, recall: 0.85)  
- Platoon Charlie has 6 platforms with target_recognition_v4.2.1 BUT running on different hardware (lower inference latency)
- Company C2 needs to task ISR assets based on capability, not just platform count

**Current Approaches Fail:**
- **Platform-centric tasking**: "Send 3 drones to Grid 7" ignores model versions and performance differences
- **Manual tracking**: Spreadsheets of "which platform has which model" become stale immediately
- **Implicit assumptions**: Operators assume all "ISR drones" have equivalent capability
- **No runtime visibility**: Cannot assess actual operational capability state across the formation

This creates **dangerous capability mismatches** where:
1. Critical missions are assigned to platforms with degraded/outdated models
2. High-performing models are underutilized due to lack of visibility
3. Model updates create capability fragmentation that C2 cannot see
4. Performance degradation (hardware failures, environmental factors) goes undetected

### Standards Landscape Analysis

We conducted a comprehensive survey of existing AI/ML metadata standards and military information exchange standards:

#### Industry AI/ML Standards (De Facto)

**Model Cards (Google, 2019)**
- Originated from Mitchell, Gebru et al. paper on responsible AI documentation
- Provides human-readable documentation framework
- Implementations: Google Model Card Toolkit (JSON schema), TensorFlow (proto/JSON), AWS SageMaker (formal JSON schema), HuggingFace (YAML metadata)
- **Coverage**: Model details, intended use, training data, performance metrics, ethical considerations, limitations
- **Strengths**: Well-adopted, comprehensive documentation approach, supports responsible AI practices
- **Gaps**: Not designed for runtime capability advertisement, lacks operational status, no hierarchical aggregation, human-focused rather than machine-readable

**ONNX (Open Neural Network Exchange)**
- Open standard format for ML model interoperability across frameworks
- Built-in metadata support: `metadata_props` (key-value pairs), versioning, producer information
- Custom metadata maps accessible at runtime via InferenceSession
- **Coverage**: Model format, input/output schemas, basic metadata, cross-platform deployment
- **Strengths**: Runtime-accessible metadata, wide tool support, framework-agnostic
- **Gaps**: Minimal schema enforcement, no performance tracking, no operational status, limited security/provenance

**MLflow Model Registry**
- Centralized model lifecycle management for ML operations
- Metadata includes: model signatures (I/O schemas), version tracking, stage transitions (staging/production), tags, custom metadata dictionaries, lineage to training runs
- **Coverage**: Version control, deployment stages, basic metadata, experiment lineage
- **Strengths**: Operational focus, version management, deployment workflow support
- **Gaps**: Centralized architecture (single point of failure), no distributed aggregation, limited security model, not designed for edge/tactical networks

#### Military/NATO Standards

**STANAG 4774 - Confidentiality Labels**
- XML-based metadata format for classified information
- Focus: Document classification and access control
- **Gap**: Not designed for AI model metadata

**STANAG 4778 - Metadata Binding**  
- Cryptographic binding of metadata to data objects
- Focus: Tamper-proof metadata attachment
- **Relevance**: Applicable to model provenance and integrity verification

**STANAG 5636 - NATO Core Metadata Specification (NCMS)**
- Structured metadata for searchability across NATO systems
- Focus: Information discovery and cataloging
- **Gap**: Generic information exchange, not AI/ML specific

**STANAG 4559 NSILI - NATO ISR Interoperability**
- Defines ISR product metadata and query/publish services
- Focus: Intelligence product exchange (imagery, reports, signals)
- **Gap**: Static products, not runtime model capabilities

**STANAG 4586 - UAV Interoperability**
- Standard interface for UAV control systems
- Focus: Command and control data link protocols
- **Partial Relevance**: Defines payload capabilities but not AI model specifics

**DoD Metadata Guidance (2024)**
- Enterprise metadata baseline for data assets
- Fields: Description, Format, Custodian, Security Classification, Disclosure & Releasability
- Focus: Data governance and FOIA compliance
- **Gap**: Document-centric, not capability-centric

### Key Findings

**No existing standard addresses the PEAT use case:**

1. **Industry standards** (Model Cards, ONNX, MLflow) provide good **static documentation** but lack:
   - Operational status tracking (is the model actually running?)
   - Performance monitoring (runtime metrics vs. design specs)
   - Resource constraints (compute/memory/bandwidth limitations)
   - Hierarchical capability aggregation (team-level emergent capabilities)
   - Security provenance for contested environments
   - Distributed architecture assumptions

2. **Military standards** (STANAGs) provide **information exchange frameworks** but lack:
   - AI/ML model-specific metadata schemas
   - Runtime capability advertisement
   - Performance and quality metrics
   - Model versioning and lifecycle management

3. **Critical gaps for distributed autonomous operations:**
   - No standard for **runtime AI capability advertisement** in bandwidth-constrained networks
   - No framework for **hierarchical capability aggregation** (platform → squad → platoon → company)
   - No approach for **differential capability updates** (only advertise changes)
   - No integration of **operational status + model performance + resource constraints**
   - No support for **contested environment security requirements** (provenance, integrity verification)

### Business and Operational Drivers

**NWIC PAC Proposal Requirements:**
- Demonstrate novel approach to distributed AI coordination
- Show technical feasibility for 1000+ node autonomous systems
- Address DoD concerns about AI model governance and transparency
- Provide standards-compatible solution for NATO interoperability

**Operational Needs:**
- **Capability-based mission planning**: "I need 50 platforms with target recognition precision ≥0.90" not "send 50 drones"
- **Runtime capability assessment**: Know actual operational capability state, not assumptions
- **Intelligent task allocation**: Route ISR tasks to platforms with best-suited models
- **Performance monitoring**: Detect model degradation in real-time (hardware failures, adversarial conditions)
- **Software logistics**: Coordinate model updates based on operational priorities
- **Risk management**: Understand capability gaps and mission-critical dependencies

**Technical Drivers:**
- Support ADR-009 downward distribution flows (models flow from C2 to edge)
- Enable ADR-013 capability-focused software operations
- Integrate with ADR-012 schema extensibility framework
- Maintain ADR-007 Automerge CRDT-based synchronization efficiency
- Satisfy ADR-006 security requirements for contested environments

## Decision

### Core Principle: Capability-Centric AI Model Advertisement

We will create a **PEAT AI Model Capability Advertisement Schema** that shifts focus from "what models are installed" to "what AI capabilities are operationally available" across the distributed system.

**Design Philosophy:**
1. **Operational First**: Track runtime status, not just deployment state
2. **Performance Aware**: Include actual performance metrics, not just design specs
3. **Resource Constrained**: Account for compute/memory/bandwidth limitations
4. **Hierarchically Aggregable**: Support squad/platoon/company capability rollups
5. **Differentially Efficient**: Only advertise capability changes (CRDT-based)
6. **Security Hardened**: Cryptographic provenance for contested environments
7. **Standards Compatible**: Synthesize Model Cards + ONNX + MLflow best practices

### Schema Architecture

#### Level 1: Platform AI Capability Advertisement

Each platform advertises its AI capabilities using a standardized schema integrated into the PEAT capability advertisement protocol:

```protobuf
syntax = "proto3";

package peat.ai.v1;

import "google/protobuf/timestamp.proto";

// Platform's AI/ML capability advertisement
message AICapabilityAdvertisement {
  // Platform identification
  string platform_id = 1;
  
  // Timestamp of this advertisement
  google.protobuf.Timestamp advertised_at = 2;
  
  // Deployed AI models providing operational capabilities
  repeated AIModelInstance models = 3;
  
  // Computational resource status
  ComputeResourceStatus resources = 4;
  
  // Overall AI capability health
  AICapabilityHealth health = 5;
}

// Individual AI model instance on the platform
message AIModelInstance {
  // Model identification
  string model_id = 1;  // e.g., "target_recognition"
  string model_version = 2;  // e.g., "4.2.1"
  string model_hash = 3;  // Content-addressed: "sha256:a7f8b3..."
  
  // Model metadata (based on Model Card standards)
  ModelMetadata metadata = 4;
  
  // Operational status
  ModelOperationalStatus operational_status = 5;
  
  // Runtime performance metrics
  ModelPerformanceMetrics performance = 6;
  
  // Model signature (I/O schema)
  ModelSignature signature = 7;
  
  // Security provenance
  ModelProvenance provenance = 8;
  
  // Resource requirements and consumption
  ModelResourceProfile resources = 9;
}

// Model metadata (Model Card inspired)
message ModelMetadata {
  // Basic information
  string model_name = 1;  // Human-readable name
  string model_type = 2;  // "classifier", "detector", "segmentor", etc.
  string model_domain = 3;  // "computer_vision", "nlp", "signal_processing"
  
  // Intended use
  string intended_use = 4;  // "Target recognition for ISR operations"
  repeated string use_cases = 5;  // ["vehicle_detection", "personnel_detection"]
  repeated string out_of_scope = 6;  // ["medical_diagnosis", "facial_recognition"]
  
  // Training information
  string training_dataset = 7;  // "MIL_VEHICLE_DATASET_v3.2"
  google.protobuf.Timestamp training_date = 8;
  string training_framework = 9;  // "PyTorch 2.0", "TensorFlow 2.14"
  
  // Model architecture
  string architecture = 10;  // "ResNet50", "YOLOv8", "BERT-base"
  int64 parameter_count = 11;  // Number of model parameters
  int64 model_size_bytes = 12;  // Size of model artifact
  
  // Performance baselines (design specifications)
  map<string, double> design_metrics = 13;  
  // e.g., {"precision": 0.94, "recall": 0.89, "f1": 0.915}
  
  // Limitations and biases
  repeated string known_limitations = 14;
  repeated string bias_considerations = 15;
  
  // Licensing and attribution
  string license = 16;  // "Apache-2.0", "MIT", "PROPRIETARY"
  repeated string creators = 17;  // ["ML_OPS_Team_Alpha", "DARPA_PROGRAM_X"]
  
  // Additional metadata
  map<string, string> custom_metadata = 18;
}

// Operational status of the model
message ModelOperationalStatus {
  enum Status {
    STATUS_UNKNOWN = 0;
    STATUS_OPERATIONAL = 1;      // Fully operational
    STATUS_DEGRADED = 2;         // Running but performance issues
    STATUS_LOADING = 3;          // Model loading into memory
    STATUS_OFFLINE = 4;          // Not available
    STATUS_ERROR = 5;            // Fatal error condition
    STATUS_UPDATING = 6;         // Being updated to new version
  }
  
  Status status = 1;
  
  // Detailed status information
  string status_message = 2;  // Human-readable status details
  google.protobuf.Timestamp status_since = 3;  // When status changed
  
  // Availability metrics
  double availability_percent = 4;  // Uptime percentage (last 24h)
  int32 inference_queue_depth = 5;  // Pending inference requests
  
  // Capacity information
  int32 max_concurrent_inferences = 6;  // Maximum parallel requests
  int32 active_inferences = 7;  // Currently processing
  
  // Error tracking
  int64 total_inferences = 8;
  int64 successful_inferences = 9;
  int64 failed_inferences = 10;
  double error_rate_percent = 11;
}

// Runtime performance metrics (actual vs. design)
message ModelPerformanceMetrics {
  // Statistical performance (measured during operation)
  map<string, double> runtime_metrics = 1;
  // e.g., {"precision": 0.91, "recall": 0.87, "f1": 0.89}
  // NOTE: May differ from design_metrics due to operational conditions
  
  // Inference performance
  InferencePerformance inference = 2;
  
  // Confidence calibration
  ConfidenceCalibration confidence = 3;
  
  // Performance degradation indicators
  PerformanceDegradation degradation = 4;
  
  // Last performance assessment
  google.protobuf.Timestamp last_assessment = 5;
  int32 assessment_sample_size = 6;  // Number of inferences sampled
}

message InferencePerformance {
  // Latency statistics (milliseconds)
  double mean_latency_ms = 1;
  double p50_latency_ms = 2;
  double p95_latency_ms = 3;
  double p99_latency_ms = 4;
  double max_latency_ms = 5;
  
  // Throughput
  double inferences_per_second = 6;
  
  // Resource efficiency
  double mean_gpu_utilization_percent = 7;
  double mean_cpu_utilization_percent = 8;
  double mean_memory_mb = 9;
}

message ConfidenceCalibration {
  // How well model confidence scores match actual accuracy
  double calibration_error = 1;  // Expected Calibration Error (ECE)
  
  // Confidence distribution
  map<string, int32> confidence_histogram = 2;
  // e.g., {"0.0-0.1": 5, "0.1-0.2": 12, ..., "0.9-1.0": 823}
}

message PerformanceDegradation {
  // Comparison to baseline performance
  bool is_degraded = 1;
  double degradation_percent = 2;  // % drop from baseline
  
  // Potential causes
  repeated string suspected_causes = 3;
  // e.g., ["thermal_throttling", "adversarial_inputs", "sensor_drift"]
  
  // Degradation timeline
  google.protobuf.Timestamp degradation_detected = 4;
  
  // Recommended actions
  repeated string recommended_actions = 5;
  // e.g., ["replace_model", "recalibrate_sensors", "reduce_inference_rate"]
}

// Model input/output signature (MLflow inspired)
message ModelSignature {
  // Input schema
  repeated TensorSchema inputs = 1;
  
  // Output schema
  repeated TensorSchema outputs = 2;
  
  // Parameter schema (for parameterized models)
  map<string, string> parameters = 3;
}

message TensorSchema {
  string name = 1;
  string dtype = 2;  // "float32", "int64", "uint8", etc.
  repeated int32 shape = 3;  // [-1, 224, 224, 3] where -1 is batch dimension
  string description = 4;  // "RGB image normalized to [0,1]"
  
  // Optional constraints
  double min_value = 5;
  double max_value = 6;
}

// Security provenance (STANAG 4778 inspired)
message ModelProvenance {
  // Content addressing
  string artifact_hash = 1;  // sha256 of model artifact
  
  // Signature chain (zero-trust verification)
  repeated ProvenanceSignature signatures = 2;
  
  // Trust policy
  TrustPolicy trust_policy = 3;
  
  // Deployment authorization
  DeploymentAuthorization authorization = 4;
  
  // Audit trail
  repeated ProvenanceEvent audit_trail = 5;
}

message ProvenanceSignature {
  string signer = 1;  // "ml_ops_team_alpha"
  string public_key = 2;  // "ed25519:abc123..."
  string signature = 3;  // "ed25519:sig_data..."
  google.protobuf.Timestamp timestamp = 4;
  string role = 5;  // "artifact_creator", "deployment_authority", "security_reviewer"
}

message TrustPolicy {
  int32 required_signatures = 1;
  repeated string required_roles = 2;
  int32 max_age_hours = 3;  // Model must be signed within this timeframe
}

message DeploymentAuthorization {
  string authorized_by = 1;
  google.protobuf.Timestamp authorized_at = 2;
  string authorization_reason = 3;
  repeated string restrictions = 4;  // e.g., ["max_classification_level_SECRET"]
}

message ProvenanceEvent {
  google.protobuf.Timestamp timestamp = 1;
  string event_type = 2;  // "deployed", "updated", "validated", "failed"
  string actor = 3;
  string details = 4;
}

// Resource profile
message ModelResourceProfile {
  // Requirements (minimum needed)
  ResourceRequirements requirements = 1;
  
  // Current consumption
  ResourceConsumption consumption = 2;
}

message ResourceRequirements {
  // Compute requirements
  double min_gpu_memory_gb = 1;
  int32 min_cpu_cores = 2;
  double min_ram_gb = 3;
  
  // Storage requirements
  int64 storage_bytes = 4;
  
  // Performance requirements
  double target_latency_ms = 5;
  double target_throughput_fps = 6;
  
  // Network requirements (for federated/distributed models)
  double bandwidth_mbps = 7;
}

message ResourceConsumption {
  // Current resource usage
  double gpu_memory_gb = 1;
  double gpu_utilization_percent = 2;
  double cpu_utilization_percent = 3;
  double ram_gb = 4;
  double disk_io_mbps = 5;
  
  // Measurement timestamp
  google.protobuf.Timestamp measured_at = 6;
}

// Compute resource status
message ComputeResourceStatus {
  // Available resources
  GPUStatus gpu = 1;
  CPUStatus cpu = 2;
  MemoryStatus memory = 3;
  StorageStatus storage = 4;
  
  // Overall health
  double compute_health_score = 5;  // 0.0-1.0
}

message GPUStatus {
  bool available = 1;
  string gpu_model = 2;  // "NVIDIA RTX 4000", "AMD MI250"
  double total_memory_gb = 3;
  double available_memory_gb = 4;
  double temperature_celsius = 5;
  double utilization_percent = 6;
}

message CPUStatus {
  int32 total_cores = 1;
  int32 available_cores = 2;
  double utilization_percent = 3;
  double temperature_celsius = 4;
}

message MemoryStatus {
  double total_ram_gb = 1;
  double available_ram_gb = 2;
  double swap_total_gb = 3;
  double swap_used_gb = 4;
}

message StorageStatus {
  double total_gb = 1;
  double available_gb = 2;
  double io_utilization_percent = 3;
}

// Overall AI capability health
message AICapabilityHealth {
  enum HealthStatus {
    HEALTH_UNKNOWN = 0;
    HEALTH_OPTIMAL = 1;      // All models operational, resources sufficient
    HEALTH_GOOD = 2;         // Minor issues, full capability
    HEALTH_DEGRADED = 3;     // Some models degraded, reduced capability
    HEALTH_CRITICAL = 4;     // Significant capability loss
    HEALTH_FAILED = 5;       // No AI capability available
  }
  
  HealthStatus overall_status = 1;
  double health_score = 2;  // 0.0-1.0 composite health metric
  
  // Component health breakdown
  int32 total_models = 3;
  int32 operational_models = 4;
  int32 degraded_models = 5;
  int32 offline_models = 6;
  
  // Critical issues
  repeated string critical_issues = 7;
  
  // Health history
  google.protobuf.Timestamp last_healthy = 8;
}
```

#### Level 2: Hierarchical Capability Aggregation

Squad/Platoon/Company level aggregated AI capabilities:

```protobuf
// Aggregated AI capability at formation level
message AggregatedAICapability {
  // Formation identification
  string formation_id = 1;  // "alpha_squad", "first_platoon"
  FormationLevel level = 2;
  
  // Aggregated timestamp (most recent update)
  google.protobuf.Timestamp aggregated_at = 3;
  
  // Capability summary by model type
  repeated ModelCapabilitySummary capabilities = 4;
  
  // Resource availability across formation
  AggregatedResourceStatus resources = 5;
  
  // Formation-level health
  FormationAIHealth health = 6;
  
  // Capability gaps and risks
  repeated CapabilityGap gaps = 7;
}

enum FormationLevel {
  LEVEL_PLATFORM = 0;
  LEVEL_TEAM = 1;      // 2-3 platforms
  LEVEL_SQUAD = 2;     // 2-3 teams
  LEVEL_PLATOON = 3;   // 2-3 squads
  LEVEL_COMPANY = 4;   // 2-3 platoons
}

message ModelCapabilitySummary {
  // Model identification
  string model_id = 1;
  string model_type = 2;
  
  // Version distribution across formation
  repeated VersionDistribution versions = 3;
  
  // Aggregate performance
  AggregatePerformance performance = 4;
  
  // Availability
  int32 total_instances = 5;
  int32 operational_instances = 6;
  int32 degraded_instances = 7;
  
  // Formation capability
  FormationCapability capability = 8;
}

message VersionDistribution {
  string version = 1;
  int32 count = 2;
  double mean_performance_score = 3;
}

message AggregatePerformance {
  // Statistical aggregation of performance across instances
  double mean_precision = 1;
  double mean_recall = 2;
  double mean_f1 = 3;
  
  // Performance variance (heterogeneity indicator)
  double precision_stddev = 4;
  double recall_stddev = 5;
  
  // Latency aggregation
  double mean_latency_ms = 6;
  double p95_latency_ms = 7;
  
  // Quality assessment
  double performance_uniformity = 8;  // 0.0-1.0, higher = more uniform
}

message FormationCapability {
  // What the formation can collectively accomplish
  string capability_description = 1;
  
  // Capacity metrics
  double estimated_throughput = 2;  // e.g., inferences/sec for the formation
  double quality_of_service = 3;  // 0.0-1.0 based on performance + availability
  
  // Coverage and redundancy
  int32 redundancy_factor = 4;  // How many instances can fail before capability lost
  
  // Emergent capabilities (only available through coordination)
  repeated string emergent_capabilities = 5;
  // e.g., ["3d_reconstruction", "multi_view_tracking", "consensus_classification"]
}

message AggregatedResourceStatus {
  // Total and available resources across formation
  double total_gpu_memory_gb = 1;
  double available_gpu_memory_gb = 2;
  int32 total_gpu_count = 3;
  
  int32 total_cpu_cores = 4;
  int32 available_cpu_cores = 5;
  
  double total_ram_gb = 6;
  double available_ram_gb = 7;
  
  // Resource health
  double resource_utilization_percent = 8;
  double resource_health_score = 9;  // 0.0-1.0
}

message FormationAIHealth {
  AICapabilityHealth.HealthStatus overall_status = 1;
  double health_score = 2;
  
  // Platform-level health distribution
  int32 total_platforms = 3;
  int32 healthy_platforms = 4;
  int32 degraded_platforms = 5;
  int32 failed_platforms = 6;
  
  // Critical issues at formation level
  repeated string formation_issues = 7;
  
  // Readiness assessment
  bool mission_ready = 8;
  string readiness_assessment = 9;
}

message CapabilityGap {
  string gap_description = 1;
  string gap_type = 2;  // "VERSION_FRAGMENTATION", "INSUFFICIENT_CAPACITY", "PERFORMANCE_DEGRADATION"
  string severity = 3;  // "CRITICAL", "HIGH", "MEDIUM", "LOW"
  
  // Impact assessment
  repeated string affected_missions = 4;
  double capability_impact_percent = 5;
  
  // Mitigation options
  repeated string mitigation_options = 6;
  
  // Gap timeline
  google.protobuf.Timestamp detected_at = 7;
  google.protobuf.Timestamp must_resolve_by = 8;
}
```

#### Level 3: Differential Updates (CRDT Integration)

Following ADR-007 Automerge principles, AI capability advertisements are synchronized as CRDTs with differential propagation:

```javascript
// Automerge CRDT representation of AI capabilities
{
  "platform_id": "platform_007_alpha_squad",
  "ai_capabilities": {
    // Models represented as nested CRDTs
    "models": {
      "target_recognition": {
        "version": "4.2.1",
        "hash": "sha256:a7f8b3...",
        "operational_status": {
          "status": "OPERATIONAL",
          "availability_percent": 99.2,
          "last_update": "2025-11-16T14:23:00Z"
        },
        "performance": {
          "runtime_metrics": {
            "precision": 0.91,  // CRDT: LWW register
            "recall": 0.87,
            "f1": 0.89
          },
          "inference": {
            "mean_latency_ms": 47.3,  // CRDT: Counter with averaging
            "p95_latency_ms": 62.1
          }
        },
        // Metadata stored but not synchronized in differential updates
        // (fetched on-demand via content hash)
        "metadata_hash": "sha256:metadata_hash..."
      }
    },
    "resources": {
      "gpu": {
        "available_memory_gb": 3.2,  // CRDT: LWW register
        "utilization_percent": 67.5
      }
    }
  },
  "last_updated": "2025-11-16T14:23:17Z"  // CRDT: LWW timestamp
}

// Differential update (only changes)
{
  "platform_id": "platform_007_alpha_squad",
  "updates": {
    "models.target_recognition.operational_status.status": "DEGRADED",
    "models.target_recognition.performance.runtime_metrics.precision": 0.88,  // Dropped!
    "models.target_recognition.performance.runtime_metrics.recall": 0.84
  },
  "timestamp": "2025-11-16T14:28:42Z"
}
```

**Bandwidth Efficiency:**
- Full advertisement: ~5-10KB per platform
- Differential update: ~200-500 bytes (only changed fields)
- **50-200x reduction** in steady-state synchronization overhead
- Hierarchical aggregation further reduces bandwidth at higher echelons

### Integration with Existing Standards

#### Model Card Compatibility

The `ModelMetadata` message is designed for **bidirectional mapping** with Model Card schemas:

```python
# Export PEAT AI capability to Model Card format
def export_model_card(ai_model_instance):
    return {
        "model_details": {
            "name": ai_model_instance.metadata.model_name,
            "version": ai_model_instance.model_version,
            "type": ai_model_instance.metadata.model_type,
            "architecture": ai_model_instance.metadata.architecture,
            "date": ai_model_instance.metadata.training_date,
            "creators": ai_model_instance.metadata.creators,
            "license": ai_model_instance.metadata.license
        },
        "intended_use": {
            "primary_uses": ai_model_instance.metadata.use_cases,
            "out_of_scope": ai_model_instance.metadata.out_of_scope
        },
        "factors": {
            "relevant_factors": parse_custom_metadata(ai_model_instance.metadata.custom_metadata)
        },
        "metrics": {
            "performance_measures": ai_model_instance.metadata.design_metrics,
            "runtime_performance": ai_model_instance.performance.runtime_metrics
        },
        "considerations": {
            "limitations": ai_model_instance.metadata.known_limitations,
            "biases": ai_model_instance.metadata.bias_considerations
        }
    }

# Import Model Card to PEAT format
def import_model_card(model_card_json):
    metadata = ModelMetadata()
    metadata.model_name = model_card_json["model_details"]["name"]
    metadata.model_version = model_card_json["model_details"]["version"]
    # ... map all fields
    return metadata
```

#### ONNX Metadata Integration

```python
# Augment ONNX model with PEAT metadata
import onnx
import json

model = onnx.load('target_recognition_v4.2.1.onnx')

# Add PEAT metadata to ONNX metadata_props
peat_meta = model.metadata_props.add()
peat_meta.key = 'peat_model_metadata'
peat_meta.value = json.dumps({
    "model_type": "classifier",
    "intended_use": "Target recognition for ISR operations",
    "design_metrics": {"precision": 0.94, "recall": 0.89},
    "trust_policy": {"required_signatures": 2, "required_roles": ["artifact_creator", "deployment_authority"]}
})

# Content-address the model
import hashlib
model_bytes = model.SerializeToString()
model_hash = f"sha256:{hashlib.sha256(model_bytes).hexdigest()}"

onnx.save(model, f'target_recognition_v4.2.1.onnx')
```

#### MLflow Registry Synchronization

```python
# Register PEAT-tracked model in MLflow
import mlflow

def register_peat_model_to_mlflow(ai_model_instance, model_artifact_path):
    # Create MLflow signature from PEAT ModelSignature
    from mlflow.models import infer_signature
    signature = convert_peat_signature_to_mlflow(ai_model_instance.signature)
    
    # Register with metadata
    mlflow.pyfunc.log_model(
        artifact_path=model_artifact_path,
        python_model=load_model_artifact(ai_model_instance.model_hash),
        signature=signature,
        registered_model_name=ai_model_instance.metadata.model_name,
        metadata={
            "peat_model_id": ai_model_instance.model_id,
            "peat_version": ai_model_instance.model_version,
            "peat_hash": ai_model_instance.model_hash,
            "design_precision": ai_model_instance.metadata.design_metrics["precision"],
            "intended_use": ai_model_instance.metadata.intended_use
        }
    )
    
    # Track operational status in MLflow tags
    client = mlflow.tracking.MlflowClient()
    model_version = client.search_model_versions(f"name='{ai_model_instance.metadata.model_name}'")[0]
    client.set_model_version_tag(
        name=ai_model_instance.metadata.model_name,
        version=model_version.version,
        key="operational_status",
        value=ai_model_instance.operational_status.status
    )
```

#### NATO STANAG Compliance

**STANAG 4778 Metadata Binding:**
```python
# Cryptographically bind PEAT metadata to model artifact
def bind_metadata_stanag_4778(model_artifact_bytes, ai_model_instance):
    from cryptography.hazmat.primitives import hashes
    from cryptography.hazmat.primitives.asymmetric import ed25519
    
    # Create XML security label (STANAG 4778)
    xml_label = f"""<?xml version="1.0" encoding="UTF-8"?>
    <SecurityLabel xmlns="urn:nato:stanag:4778">
        <ObjectHash algorithm="SHA256">{ai_model_instance.model_hash}</ObjectHash>
        <Classification level="UNCLASSIFIED"/>
        <Signatures>
            {generate_signature_xml(ai_model_instance.provenance.signatures)}
        </Signatures>
        <Metadata>
            <ModelID>{ai_model_instance.model_id}</ModelID>
            <ModelVersion>{ai_model_instance.model_version}</ModelVersion>
            <IntendedUse>{ai_model_instance.metadata.intended_use}</IntendedUse>
        </Metadata>
    </SecurityLabel>
    """
    
    # Bind label to artifact
    return create_bound_artifact(model_artifact_bytes, xml_label)
```

**STANAG 5636 NCMS Searchability:**
```xml
<!-- Map PEAT AI capabilities to NATO Core Metadata Specification -->
<NCMS:Resource xmlns:NCMS="urn:nato:stanag:5636">
    <NCMS:Identifier>urn:peat:model:target_recognition:4.2.1</NCMS:Identifier>
    <NCMS:Title>Target Recognition Model v4.2.1</NCMS:Title>
    <NCMS:Type>AI_ML_Model</NCMS:Type>
    <NCMS:Format>application/onnx</NCMS:Format>
    <NCMS:Description>Object detection and classification for ground vehicles and personnel</NCMS:Description>
    <NCMS:Creator>ML_OPS_Team_Alpha</NCMS:Creator>
    <NCMS:Date>2025-08-15</NCMS:Date>
    <NCMS:Subject>
        <NCMS:Keyword>target_recognition</NCMS:Keyword>
        <NCMS:Keyword>computer_vision</NCMS:Keyword>
        <NCMS:Keyword>isr</NCMS:Keyword>
    </NCMS:Subject>
</NCMS:Resource>
```

### API and Usage Patterns

#### Platform-Level API

```rust
// Rust implementation for edge platforms
use peat_ai_capability::*;

// Initialize AI capability advertiser
let mut advertiser = AICapabilityAdvertiser::new("platform_007");

// Register deployed model
advertiser.register_model(AIModelInstance {
    model_id: "target_recognition".to_string(),
    model_version: "4.2.1".to_string(),
    model_hash: "sha256:a7f8b3...".to_string(),
    metadata: load_model_card("target_recognition_v4.2.1.json"),
    operational_status: ModelOperationalStatus {
        status: Status::Operational,
        availability_percent: 100.0,
        ..Default::default()
    },
    performance: ModelPerformanceMetrics::default(),
    signature: load_model_signature("target_recognition_v4.2.1.onnx"),
    provenance: load_provenance("target_recognition_v4.2.1.provenance"),
    resources: ModelResourceProfile::default(),
});

// Update operational status based on runtime monitoring
advertiser.update_operational_status("target_recognition", |status| {
    status.active_inferences = get_queue_depth();
    status.error_rate_percent = calculate_error_rate();
});

// Update performance metrics based on inference results
advertiser.update_performance("target_recognition", |perf| {
    perf.runtime_metrics.insert("precision".to_string(), 0.91);
    perf.runtime_metrics.insert("recall".to_string(), 0.87);
    perf.inference.mean_latency_ms = 47.3;
});

// Advertise capabilities (differential CRDT sync)
let advertisement = advertiser.generate_advertisement();
peat_sync_engine.publish(advertisement);  // Uses ADR-007 Automerge sync
```

#### Squad/Platoon Aggregation

```rust
// Hierarchical aggregation at squad level
use peat_ai_capability::aggregation::*;

let mut aggregator = AICapabilityAggregator::new("alpha_squad", FormationLevel::Squad);

// Receive platform advertisements via CRDT sync
for platform_ad in incoming_advertisements {
    aggregator.ingest_platform_capability(platform_ad);
}

// Generate aggregated capability summary
let squad_capability = aggregator.aggregate();

// Identify capability gaps
let gaps = squad_capability.identify_gaps(&mission_requirements);
for gap in gaps {
    if gap.severity == "CRITICAL" {
        alert_c2(gap);
    }
}

// Propagate aggregated capability upward
peat_sync_engine.publish_aggregated(squad_capability);
```

#### C2 Capability Query Interface

```rust
// Company C2 queries available AI capabilities
use peat_ai_capability::query::*;

let query = AICapabilityQuery::new()
    .model_type("classifier")
    .model_domain("computer_vision")
    .min_precision(0.90)
    .min_recall(0.85)
    .operational_status(Status::Operational)
    .formation_level(FormationLevel::Platoon);

let results = peat_registry.query_capabilities(query);

println!("Available ISR capabilities:");
for result in results {
    println!("  {}: {} platforms with {:.2}% precision",
             result.formation_id,
             result.operational_instances,
             result.performance.mean_precision * 100.0);
}

// Task assignment based on capability
assign_isr_mission(
    target_area: "Grid_7",
    required_capability: "target_recognition",
    platforms: results.get(0).select_best_platforms(3)
);
```

### Governance and Compliance Features

#### Model Versioning and Approval Workflow

```rust
// Deployment authorization enforcement
fn deploy_model(model: AIModelInstance, platform_id: &str) -> Result<(), DeploymentError> {
    // Verify provenance signatures
    verify_provenance(&model.provenance)?;
    
    // Check trust policy
    if !satisfies_trust_policy(&model.provenance) {
        return Err(DeploymentError::InsufficientSignatures);
    }
    
    // Verify deployment authorization
    if model.provenance.authorization.authorized_by.is_empty() {
        return Err(DeploymentError::NoAuthorization);
    }
    
    // Check classification restrictions
    let platform_clearance = get_platform_clearance(platform_id);
    for restriction in &model.provenance.authorization.restrictions {
        if !platform_clearance.satisfies(restriction) {
            return Err(DeploymentError::ClassificationViolation);
        }
    }
    
    // Deploy and track
    deploy_to_platform(model, platform_id)?;
    audit_log_deployment(&model, platform_id);
    
    Ok(())
}
```

#### Performance Degradation Monitoring

```rust
// Automated performance monitoring and alerting
struct PerformanceDegradationDetector {
    baseline_metrics: HashMap<String, f64>,
    alert_threshold_percent: f64,
}

impl PerformanceDegradationDetector {
    fn check_degradation(&self, current_metrics: &ModelPerformanceMetrics) 
        -> Option<PerformanceDegradation> {
        
        let precision_drop = self.calculate_drop("precision", 
                                                  current_metrics.runtime_metrics["precision"]);
        
        if precision_drop > self.alert_threshold_percent {
            return Some(PerformanceDegradation {
                is_degraded: true,
                degradation_percent: precision_drop,
                suspected_causes: self.diagnose_causes(current_metrics),
                degradation_detected: Timestamp::now(),
                recommended_actions: vec![
                    "replace_model".to_string(),
                    "investigate_sensor_drift".to_string()
                ],
            });
        }
        
        None
    }
    
    fn diagnose_causes(&self, metrics: &ModelPerformanceMetrics) -> Vec<String> {
        let mut causes = Vec::new();
        
        if metrics.inference.mean_latency_ms > 100.0 {
            causes.push("thermal_throttling".to_string());
        }
        if metrics.confidence.calibration_error > 0.2 {
            causes.push("distribution_shift".to_string());
        }
        
        causes
    }
}
```

## Consequences

### Positive

**Operational Capability Transparency**
- C2 has real-time visibility into actual AI capability state across formation
- Enables capability-based mission planning instead of platform-count planning
- Performance degradation detected automatically before mission impact
- Intelligent task allocation routes missions to best-suited platforms

**Software Logistics Optimization**
- Model updates coordinated based on operational priorities (ADR-013)
- Differential capability advertisement reduces bandwidth by 50-200x
- Version fragmentation visible and quantified at formation level
- Rollback capabilities support rapid recovery from problematic updates

**Standards Compatibility and Interoperability**
- Bidirectional mapping with Model Card standards (industry best practice)
- ONNX metadata integration (cross-framework compatibility)
- MLflow registry synchronization (ML operations tooling)
- NATO STANAG compliance (STANAG 4778, 5636) enables alliance interoperability

**Security and Governance**
- Cryptographic provenance (content-addressing + signature chains)
- Zero-trust verification at every platform
- Deployment authorization enforcement
- Comprehensive audit trails for compliance

**NWIC PAC Proposal Strength**
- Demonstrates novel approach to AI coordination at scale
- Shows technical feasibility with proven standards integration
- Addresses DoD AI governance concerns proactively
- Provides clear path to NATO standardization

### Negative

**Implementation Complexity**
- Comprehensive schema requires significant development effort
- Runtime performance monitoring adds computational overhead
- Provenance verification increases deployment latency
- Bidirectional standards mapping requires maintenance as standards evolve

**Operational Overhead**
- Platforms must continuously monitor and report AI performance
- Network bandwidth consumed by capability advertisements (even with differentials)
- C2 operators need training on capability-based planning paradigm
- Aggregation errors could propagate through hierarchy

**Privacy and OPSEC Considerations**
- AI capability advertisements reveal system composition to adversaries
- Performance metrics could expose platform locations/activities
- Model version information aids adversarial ML attacks
- Mitigation: Encrypt advertisements, rate-limit updates, aggregate at higher echelons

**Standards Evolution Risk**
- Model Card, ONNX, MLflow standards may evolve incompatibly
- NATO STANAGs update on multi-year cycles
- PEAT schema may need versioning strategy to handle changes
- Risk of becoming tied to deprecated standards

**Performance Measurement Challenges**
- Runtime metrics may not reflect true model quality (sampling bias)
- Adversarial inputs could skew performance measurements
- Environmental factors confound performance attribution
- Requires robust ground truth for validation (often unavailable)

### Mitigations

**Complexity Management:**
- Phased implementation (basic schema → full provenance → aggregation)
- Reference implementation with comprehensive testing
- Developer tooling for schema generation and validation
- Clear migration path from existing ML workflows

**Bandwidth Optimization:**
- Differential CRDT updates (ADR-007) minimize synchronization overhead
- Hierarchical aggregation reduces advertisement at higher echelons
- Configurable update rates based on operational tempo
- Emergency mode for bandwidth-constrained scenarios

**OPSEC Protection:**
- Encrypt all capability advertisements with formation keys
- Aggregate at squad level to reduce platform-specific exposure
- Rate-limiting and jitter to prevent traffic analysis
- Redact sensitive metadata in contested environments

**Standards Compatibility:**
- Version negotiation protocol for schema evolution
- Graceful degradation when newer standards unsupported
- Regular review and update cycle aligned with major standards
- Community engagement with standards bodies (ONNX, MLflow, NATO STO)

## Implementation Roadmap

### Phase 1: Core Schema Definition (Months 1-2)
- Finalize protobuf schema (Level 1: Platform advertisement)
- Implement Rust/Python codegen for schema
- Create reference test fixtures
- Document schema with comprehensive examples
- **Deliverable**: ADR-018 approved, schema v0.1.0 published

### Phase 2: Platform Integration (Months 2-4)
- Implement Rust AICapabilityAdvertiser library
- Integrate with ONNX Runtime for model signature extraction
- Add performance monitoring and operational status tracking
- Implement differential CRDT synchronization (ADR-007 integration)
- **Deliverable**: Platform-level capability advertisement functional

### Phase 3: Aggregation and Querying (Months 4-6)
- Implement hierarchical aggregation (Level 2: Formation capabilities)
- Create C2 query interface and API
- Build capability gap detection algorithms
- Develop visualization tools for capability state
- **Deliverable**: End-to-end capability visibility from platform to company

### Phase 4: Standards Integration (Months 6-8)
- Bidirectional Model Card mapping
- ONNX metadata integration
- MLflow registry synchronization
- STANAG 4778/5636 compliance validation
- **Deliverable**: PEAT AI capabilities compatible with industry/military standards

### Phase 5: Security and Governance (Months 8-10)
- Implement cryptographic provenance verification
- Deploy signature chain enforcement
- Build deployment authorization framework
- Create audit logging and compliance reporting
- **Deliverable**: Production-ready security model

### Phase 6: Validation and NWIC PAC Demo (Months 10-12)
- Large-scale testing with Shadow simulator (ADR-008)
- Demonstrate 1000+ platform AI capability coordination
- Performance benchmarking and optimization
- NWIC PAC proposal demonstration
- **Deliverable**: Validated at TRL 6-7, ready for NWIC PAC submission

## Open Questions

1. **Performance Metric Ground Truth**: How do we establish ground truth for runtime performance metrics in operational environments where labeled data is scarce?

2. **Adversarial Robustness**: Should the schema include adversarial robustness metrics (e.g., resistance to evasion attacks)? How do we measure this operationally?

3. **Federated Learning Integration**: How should we extend the schema to support federated learning scenarios where models are trained across distributed platforms?

4. **Multi-Modal Models**: How do we represent models that consume multiple input modalities (e.g., vision + LIDAR + RF) in a coherent signature?

5. **Model Explainability**: Should we include explainability metadata (e.g., attention maps, SHAP values) for interpretable AI requirements?

6. **Lifecycle Management**: What triggers automatic model retirement? Version deprecation policies?

7. **Standardization Timeline**: When should we engage NATO STO for formal STANAG proposal? After TRL 6 validation?

## References

### Industry Standards
- Mitchell et al., "Model Cards for Model Reporting" (2019)
- Google Model Card Toolkit: https://github.com/tensorflow/model-card-toolkit
- ONNX Metadata Specification: https://github.com/onnx/onnx/blob/main/docs/MetadataProps.md
- MLflow Model Registry: https://mlflow.org/docs/latest/model-registry.html
- HuggingFace Model Cards: https://huggingface.co/docs/hub/model-cards

### Military Standards
- STANAG 4774: XML Security Labels for NATO
- STANAG 4778: Metadata Binding for Classified Information
- STANAG 5636: NATO Core Metadata Specification (NCMS)
- STANAG 4559: NATO ISR Interoperability (NSILI)
- DoD Metadata Guidance (2024): https://www.ai.mil/docs/DoD_Metadata_Guidance.pdf

### Related ADRs
- ADR-001: CAP Protocol PoC - Foundation for capability advertisement
- ADR-007: Automerge Sync Engine - CRDT-based differential synchronization
- ADR-009: Bidirectional Hierarchical Flows - Model distribution downward
- ADR-012: Schema Definition and Protocol Extensibility - Schema framework
- ADR-013: Distributed Software/AI Operations - Capability-focused operations
- ADR-015: Experimental Validation - Hierarchical aggregation testing

### Technical Resources
- Responsible AI Model Cards: https://modelcards.withgoogle.com/
- ONNX Model Zoo: https://github.com/onnx/models
- MLflow Documentation: https://mlflow.org/docs/latest/
- NATO Standardization Office: https://nso.nato.int/

## Approval

This ADR requires approval from:
- [ ] Technical Architecture Board
- [ ] NWIC PAC Proposal Team
- [ ] Security/Compliance Review
- [ ] NATO Interoperability Working Group (Advisory)
