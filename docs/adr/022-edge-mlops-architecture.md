# ADR-022: Edge MLOps Architecture

## Status
Proposed

## Context

### The Problem: MLOps Assumes Connectivity

Traditional Machine Learning Operations (MLOps) architectures assume:
- **Centralized data lakes** (S3, Snowflake, data warehouses) for training data
- **Continuous connectivity** to model registries and serving infrastructure
- **Real-time telemetry** streaming to monitoring dashboards
- **Centralized training** with GPU clusters in cloud/datacenter
- **Push-based deployment** from central registry to edge endpoints

**This breaks catastrophically in tactical edge environments:**

**Bandwidth Constraints:**
- 9.6Kbps-1Mbps tactical networks cannot support GB-scale model transfers
- Cannot stream raw sensor data (video, LiDAR, SAR) to central data lake
- Real-time telemetry streaming consumes limited bandwidth

**Intermittent Connectivity:**
- Platforms operate disconnected for hours to days
- Cannot depend on model registry availability during missions
- Training data cannot be collected in real-time

**Operational Requirements:**
- Models must run at the edge where decisions are made
- Performance degradation must be detected locally
- Model updates must propagate during limited connectivity windows
- Training data collection cannot compromise mission bandwidth

### Military AI/ML Use Cases

**Perception Models:**
- Target recognition (YOLOv8, EfficientDet): 100-500MB
- Object tracking (DeepSORT, ByteTrack): 50-200MB
- Semantic segmentation (DeepLabV3): 200-800MB
- SAR/EO/IR fusion models: 500MB-2GB

**Decision Models:**
- Route planning (RL policies): 10-50MB
- Threat assessment (ensemble classifiers): 20-100MB
- ROE compliance (rule-based + ML hybrid): 5-50MB
- Behavior prediction (LSTM, Transformer): 50-300MB

**Mission-Specific Models:**
- Facial recognition (when authorized): 100-500MB
- Vehicle identification: 200-800MB
- Communications intelligence: 50-200MB
- Sensor fusion: 100-500MB

**Operational Characteristics:**
- Models deployed to 10-1000+ edge platforms
- Updates needed for: performance improvements, new threats, ROE changes
- Performance monitoring required for operational capability assessment
- Training data: operational encounters, red team exercises, simulation

### Existing Approaches Fall Short

**Traditional MLOps (MLflow, Kubeflow, SageMaker):**
- Designed for datacenter/cloud deployment
- Assumes reliable connectivity to central services
- Push-based deployment doesn't handle disconnection
- No hierarchical distribution strategy
- No differential model propagation

**Edge AI Frameworks (TensorFlow Lite, ONNX Runtime):**
- Focus on inference optimization (quantization, pruning)
- Assume models are pre-deployed or updated manually
- No distributed model lifecycle management
- No performance monitoring infrastructure
- No training data collection strategy

**Federated Learning Platforms (Flower, PySyft):**
- Designed for privacy-preserving training across organizations
- Assume periodic connectivity to aggregation server
- High bandwidth requirements for gradient exchange
- Not optimized for military hierarchy
- No integration with operational C2 systems

**Container Orchestration (Kubernetes, K3s):**
- Can deploy ML models in containers
- Assumes control plane connectivity
- Layer-based distribution still large (hundreds of MB)
- No hierarchical propagation strategies
- No military-specific capability modeling

## Decision

### Core Principle: Edge-First MLOps

**Invert traditional MLOps assumptions:**

**Instead of:**
- Central data lake → Distributed training → Push to edge
- Continuous monitoring telemetry → Central dashboards
- Model registry as source of truth

**PEAT Enables:**
- Edge-first inference with local model execution
- Hierarchical model distribution via differential sync
- Aggregated performance monitoring through hierarchy
- Opportunistic training data collection when bandwidth permits
- Operational capability assessment at each echelon

**Design Philosophy:**
- **Models run at the edge** where decisions are made
- **Model updates propagate hierarchically** using PEAT's differential sync
- **Performance metrics aggregate upward** through command hierarchy
- **Training happens offline** or via federated learning
- **Capability, not inventory** is the operational metric

### Model Format Standards: ONNX as Foundation

**PEAT uses ONNX (Open Neural Network Exchange) as the standard model interchange format** for edge AI operations. This decision provides critical benefits for military edge deployments:

#### Why ONNX for Tactical Edge

**Vendor Neutrality:**
- Open standard not controlled by single vendor (Meta, Google, etc.)
- Reduces vendor lock-in concerns for government procurement
- Enables multi-vendor ecosystem of model providers
- Framework-agnostic: train in PyTorch/TensorFlow/JAX, deploy as ONNX

**Security & Auditability:**
- Human-readable graph structure (Protocol Buffers format)
- Clear visibility into model architecture and operators
- Easier to audit for malicious operations vs opaque binaries
- Whitelisting of approved ONNX operators for security hardening
- Cryptographic signing of ONNX files for provenance

**Hardware Portability:**
- Single model format runs on diverse tactical platforms
- ONNX Runtime supports CPU, GPU, NPU, edge accelerators
- Execution providers optimize for specific hardware:
  - `CUDAExecutionProvider` - NVIDIA GPUs
  - `TensorRTExecutionProvider` - NVIDIA optimization
  - `OpenVINOExecutionProvider` - Intel edge hardware
  - `CoreMLExecutionProvider` - Apple silicon
  - `CPUExecutionProvider` - Universal fallback
- Critical for heterogeneous edge (UAVs, UGVs, maritime, dismounted)

**NATO/Allied Interoperability:**
- Emerging as standard in defense/intelligence community
- Facilitates model sharing across nations (AUKUS, Five Eyes)
- Allied forces can deploy same models without framework dependencies
- Supports multinational exercises and operations

**Performance & Optimization:**
- ONNX Runtime highly optimized for inference workloads
- Built-in quantization support (INT8, FP16, dynamic)
- Graph optimization passes (operator fusion, constant folding)
- Competitive with or faster than native framework inference
- Smaller model sizes after optimization

**Ecosystem Maturity:**
- ONNX Model Zoo provides pre-trained baselines
- Contractor/vendor support widely available
- Integration with MLOps tools (MLflow, Azure ML)
- Active development and governance (LF AI & Data)

#### ONNX Integration in PEAT

```python
# Example: ONNX model in PEAT registry
{
  "model_id": "target_recognition_yolov8",
  "version": "4.2.1",
  "format": "onnx",
  "onnx_opset": 18,
  "hash": "sha256:a7f8b3c4d5e6...",
  
  # Multiple optimized variants for different hardware
  "variants": {
    "fp32_cuda": {
      "file": "model_fp32_cuda.onnx",
      "size_bytes": 487326720,
      "execution_providers": ["CUDAExecutionProvider"],
      "target_hardware": "gpu_nvidia",
      "precision": "float32"
    },
    "fp16_cuda": {
      "file": "model_fp16_cuda.onnx", 
      "size_bytes": 243663360,  // ~50% size reduction
      "execution_providers": ["CUDAExecutionProvider"],
      "target_hardware": "gpu_nvidia",
      "precision": "float16"
    },
    "int8_cpu": {
      "file": "model_int8_cpu.onnx",
      "size_bytes": 121831680,  // ~75% size reduction
      "execution_providers": ["CPUExecutionProvider"],
      "target_hardware": "cpu_x86",
      "precision": "int8"
    },
    "int8_openvino": {
      "file": "model_int8_openvino.onnx",
      "size_bytes": 121831680,
      "execution_providers": ["OpenVINOExecutionProvider"],
      "target_hardware": "intel_edge",
      "precision": "int8"
    }
  },
  
  # ONNX metadata aids security review
  "onnx_metadata": {
    "producer_name": "pytorch",
    "producer_version": "2.1.0",
    "domain": "ai.onnx",
    "model_version": 1,
    "doc_string": "YOLOv8n object detection model, optimized for tactical edge",
    "operators_used": [
      "Conv", "BatchNormalization", "Relu", 
      "MaxPool", "Concat", "Sigmoid"  // All standard ONNX ops
    ]
  },
  
  # Security verification
  "security_review": {
    "reviewed_by": "ai_security_team",
    "review_date": "2025-11-01T10:00:00Z",
    "onnx_graph_inspected": true,
    "operators_whitelisted": true,
    "no_custom_operators": true,
    "weight_ranges_verified": true,
    "approved_for_deployment": true
  },
  
  # AFRL AI Passport integration
  "ai_passport": {
    "passport_id": "afrl-aipassport://model/yolov8n-4.2.1",
    "onnx_hash_verified": true,
    "test_results": {
      "mAP_50": 0.89,
      "mAP_50_95": 0.67,
      "avg_inference_ms": 42,
      "tested_runtime": "ONNX Runtime 1.16.3",
      "tested_providers": ["CUDAExecutionProvider", "CPUExecutionProvider"]
    }
  }
}
```

**PEAT's ONNX Runtime Integration:**

```python
class PeatMLRuntime:
    """ONNX-first ML runtime for edge platforms"""
    
    def __init__(self, platform_id: str, peat_sync: PeatSyncEngine):
        self.platform_id = platform_id
        self.peat_sync = peat_sync
        self.sessions = {}
        
        # Detect available hardware and execution providers
        self.execution_providers = self.detect_execution_providers()
        
    def detect_execution_providers(self) -> List[str]:
        """Detect optimal ONNX execution providers for this platform"""
        available_providers = onnxruntime.get_available_providers()
        
        # Priority order for tactical edge
        preferred_order = [
            'TensorrtExecutionProvider',  # NVIDIA GPU with TensorRT
            'CUDAExecutionProvider',      # NVIDIA GPU
            'OpenVINOExecutionProvider',  # Intel edge accelerators
            'CoreMLExecutionProvider',    # Apple silicon
            'CPUExecutionProvider'        # Universal fallback
        ]
        
        return [p for p in preferred_order if p in available_providers]
        
    def load_model(self, model_spec: ModelSpec):
        """Load ONNX model with automatic variant selection"""
        # Fetch model metadata from PEAT
        model_metadata = self.peat_sync.get_artifact_metadata(
            collection="models.registry",
            artifact_id=f"{model_spec.model_id}:{model_spec.version}"
        )
        
        # Select optimal variant for this platform
        variant = self.select_optimal_variant(
            model_metadata.variants,
            self.execution_providers,
            self.get_hardware_capabilities()
        )
        
        # Fetch ONNX model file via PEAT differential sync
        onnx_model_path = self.peat_sync.fetch_artifact(
            artifact_id=variant.file,
            priority="normal"
        )
        
        # Verify ONNX model hash
        if not self.verify_onnx_hash(onnx_model_path, variant.hash):
            raise SecurityException("ONNX model hash verification failed")
            
        # Load into ONNX Runtime with optimal providers
        session_options = onnxruntime.SessionOptions()
        session_options.graph_optimization_level = (
            onnxruntime.GraphOptimizationLevel.ORT_ENABLE_ALL
        )
        
        session = onnxruntime.InferenceSession(
            onnx_model_path,
            sess_options=session_options,
            providers=self.execution_providers
        )
        
        self.sessions[model_spec.model_id] = {
            "session": session,
            "variant": variant.name,
            "providers": session.get_providers(),  # Actual providers used
            "version": model_spec.version,
            "loaded_at": datetime.now()
        }
        
        # Update capability state in PEAT
        self.peat_sync.update_capability_state(
            platform_id=self.platform_id,
            capability=model_spec.model_id,
            status="operational",
            version=model_spec.version,
            runtime_info={
                "format": "onnx",
                "variant": variant.name,
                "execution_providers": session.get_providers()
            }
        )
        
        return session
        
    def select_optimal_variant(
        self,
        variants: Dict[str, ModelVariant],
        execution_providers: List[str],
        hardware: HardwareCapabilities
    ) -> ModelVariant:
        """Select best ONNX variant for platform hardware"""
        
        # GPU available with sufficient memory
        if ('CUDAExecutionProvider' in execution_providers and 
            hardware.gpu_mem_gb >= 2.0):
            if 'fp16_cuda' in variants:
                return variants['fp16_cuda']  # Best GPU performance
            return variants['fp32_cuda']
            
        # Intel edge accelerator
        if 'OpenVINOExecutionProvider' in execution_providers:
            return variants['int8_openvino']
            
        # Fallback to quantized CPU
        return variants['int8_cpu']
```

**Differential Propagation of ONNX Models:**

ONNX's structured format enables intelligent differential sync:

```javascript
// ONNX graph structure enables layer-level chunking
{
  "model_id": "target_recognition_yolov8",
  "base_version": "4.2.0",
  "target_version": "4.2.1",
  
  // ONNX graph parsed to identify changed components
  "onnx_delta": {
    "changed_initializers": [
      // Only weights that changed
      "model.22.cv2.0.0.conv.weight",
      "model.22.cv3.0.0.conv.weight"
    ],
    "changed_nodes": [],  // No architecture changes
    "added_nodes": [],
    "removed_nodes": [],
    
    // Differential sync payload
    "delta_size_bytes": 16777216,  // 16MB of changed weights
    "base_size_bytes": 486912000,  // 464MB full model
    "compression_ratio": 29,        // 29x bandwidth savings
    
    // Chunks map to ONNX graph structure
    "chunks": [
      {
        "chunk_id": "initializer_model.22.cv2.0.0.conv.weight",
        "offset_in_onnx": 387234816,
        "size_bytes": 8388608,
        "hash": "sha256:chunk_hash_1"
      },
      {
        "chunk_id": "initializer_model.22.cv3.0.0.conv.weight",
        "offset_in_onnx": 395623424,
        "size_bytes": 8388608,
        "hash": "sha256:chunk_hash_2"
      }
    ]
  }
}
```

**Benefits for PEAT:**
- ONNX graph structure enables semantic chunking (by layer/operator)
- Changed weights identified at granular level
- Architecture changes (nodes/edges) detected separately
- Shared initializers across model versions deduplicated
- Quantized variants share graph structure, differ only in weights

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Battalion C2 (Model Hub)                    │
│  • Model Registry (authoritative source)                        │
│  • Aggregated Performance Dashboard                             │
│  • Training Data Staging (when platforms reconnect)             │
│  • Federated Learning Aggregator (optional)                     │
│  • AI Passport Integration (AFRL model validation)              │
└────────────────────────┬────────────────────────────────────────┘
                         │ [Differential Model Sync]
              ┌──────────┴──────────┐
              │                     │
    ┌─────────▼─────────┐  ┌────────▼──────────┐
    │  Company C2 Alpha │  │  Company C2 Bravo │
    │  • Model Cache    │  │  • Model Cache    │
    │  • Perf Aggreg.   │  │  • Perf Aggreg.   │
    └─────────┬─────────┘  └────────┬──────────┘
              │                     │
       ┌──────┴──────┐       ┌─────┴──────┐
       │             │       │            │
  ┌────▼───┐   ┌────▼───┐  ┌▼─────┐  ┌──▼─────┐
  │Platoon 1│   │Platoon 2│  │Plat 3│  │Plat 4 │
  │• Cache  │   │• Cache  │  │Cache │  │Cache  │
  └────┬───┘   └────┬───┘  └┬─────┘  └──┬─────┘
       │            │       │            │
    [Squads]    [Squads] [Squads]    [Squads]
       │            │       │            │
  [Platforms]  [Platforms] [Platforms] [Platforms]
  • Run Models Locally
  • Measure Performance
  • Collect Training Metadata
```

### Component Architecture

#### 1. Edge Model Runtime

Each platform runs models locally with instrumentation:

```python
# Edge platform ML runtime
class PeatMLRuntime:
    def __init__(self, platform_id: str, peat_sync: PeatSyncEngine):
        self.platform_id = platform_id
        self.peat_sync = peat_sync
        self.models = {}
        self.performance_metrics = {}
        
    def load_model(self, model_spec: ModelSpec):
        """Load model from PEAT-synced model registry"""
        model_id = model_spec.model_id
        version = model_spec.version
        
        # Check if model available in local PEAT state
        model_artifact = self.peat_sync.get_artifact(
            collection="models.registry",
            artifact_id=f"{model_id}:{version}"
        )
        
        if model_artifact is None:
            # Request from parent via PEAT sync
            self.peat_sync.request_artifact(
                artifact_id=f"{model_id}:{version}",
                priority="normal"
            )
            return None
            
        # Verify signature chain (ADR-006 integration)
        if not self.verify_model_provenance(model_artifact):
            raise SecurityException("Model signature verification failed")
            
        # Load model into inference engine
        model = self.inference_engine.load(model_artifact.path)
        self.models[model_id] = {
            "model": model,
            "version": version,
            "hash": model_artifact.hash,
            "loaded_at": datetime.now(),
            "inference_count": 0
        }
        
        # Update capability state in PEAT
        self.peat_sync.update_capability_state(
            platform_id=self.platform_id,
            capability=model_id,
            status="operational",
            version=version
        )
        
        return model
        
    def infer(self, model_id: str, input_data):
        """Run inference with performance tracking"""
        if model_id not in self.models:
            raise ModelNotLoadedException(f"Model {model_id} not loaded")
            
        model_info = self.models[model_id]
        model = model_info["model"]
        
        # Instrumented inference
        start_time = time.perf_counter()
        result = model.infer(input_data)
        latency_ms = (time.perf_counter() - start_time) * 1000
        
        # Track performance metrics
        self.update_performance_metrics(
            model_id=model_id,
            latency_ms=latency_ms,
            result=result
        )
        
        # Log inference metadata (for training data collection)
        if self.should_log_inference(result):
            self.log_inference_metadata(
                model_id=model_id,
                input_metadata=self.extract_metadata(input_data),
                result=result,
                context=self.get_operational_context()
            )
        
        model_info["inference_count"] += 1
        return result
        
    def update_performance_metrics(self, model_id: str, latency_ms: float, result):
        """Track local performance metrics"""
        if model_id not in self.performance_metrics:
            self.performance_metrics[model_id] = {
                "latency_ms": RollingAverage(window=1000),
                "confidence": RollingAverage(window=1000),
                "false_positive_rate": FalsePositiveTracker(),
                "inference_count": 0,
                "last_updated": datetime.now()
            }
            
        metrics = self.performance_metrics[model_id]
        metrics["latency_ms"].add(latency_ms)
        metrics["confidence"].add(result.confidence)
        metrics["inference_count"] += 1
        metrics["last_updated"] = datetime.now()
        
        # Periodically sync performance metrics up through PEAT
        if metrics["inference_count"] % 100 == 0:
            self.sync_performance_metrics(model_id)
            
    def sync_performance_metrics(self, model_id: str):
        """Aggregate performance metrics into PEAT state"""
        metrics = self.performance_metrics[model_id]
        
        self.peat_sync.update_capability_state(
            platform_id=self.platform_id,
            capability=model_id,
            performance={
                "avg_latency_ms": metrics["latency_ms"].average(),
                "avg_confidence": metrics["confidence"].average(),
                "false_positive_rate": metrics["false_positive_rate"].rate(),
                "inference_count": metrics["inference_count"],
                "measured_at": metrics["last_updated"]
            }
        )
```

#### 2. Hierarchical Model Distribution

Building on ADR-013's differential propagation:

```javascript
// Model distribution strategy
{
  "model_id": "target_recognition_yolov8",
  "versions": [
    {
      "version": "4.2.1",
      "hash": "sha256:a7f8b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2",
      "size_bytes": 487326720,  // ~465MB
      "chunks": [
        {
          "chunk_id": "chunk_000",
          "hash": "sha256:chunk_000_hash",
          "offset": 0,
          "size_bytes": 4194304  // 4MB chunks
        }
        // ... 115 more chunks
      ],
      "metadata": {
        "framework": "pytorch",
        "quantization": "int8",
        "target_hardware": ["cuda", "cpu"],
        "min_gpu_mem_gb": 2.0,
        "test_results": {
          "mAP_50": 0.89,
          "mAP_50_95": 0.67,
          "inference_time_ms": 42
        }
      },
      "provenance": {
        "created_by": "ml_ops_team",
        "trained_on": "dataset_v3.2",
        "validation_report": "gs://reports/yolov8_4.2.1_validation.pdf",
        "ai_passport_id": "afrl-aipassport://model/12345",  // AFRL integration
        "signatures": [
          {
            "signer": "ml_ops_team_lead",
            "role": "model_creator",
            "signature": "ed25519:sig_data..."
          },
          {
            "signer": "battalion_ai_officer",
            "role": "deployment_authority", 
            "signature": "ed25519:sig_data..."
          }
        ]
      }
    },
    {
      "version": "4.2.0",
      "hash": "sha256:previous_version_hash",
      "size_bytes": 486912000
    }
  ],
  "delta_4.2.0_to_4.2.1": {
    "changed_chunks": [
      "chunk_017",  // Only 4 chunks changed
      "chunk_018",
      "chunk_089",
      "chunk_103"
    ],
    "delta_size_bytes": 16777216,  // 16MB instead of 465MB
    "compression_ratio": 29  // 29x bandwidth savings
  }
}
```

**Distribution Flow:**

```rust
// Hierarchical model propagation
impl ModelDistribution {
    pub async fn distribute_model_update(
        &self,
        model_id: String,
        new_version: String,
        priority: Priority
    ) -> Result<DistributionStatus> {
        // Step 1: Determine which platforms need update
        let platform_versions = self.query_platform_versions(&model_id).await?;
        
        // Step 2: Compute optimal distribution strategy
        let strategy = if platform_versions.all_have_recent_version() {
            // Most platforms have v4.2.0, pushing v4.2.1
            DistributionStrategy::Delta {
                base_version: "4.2.0",
                delta_size_mb: 16,
                estimated_time_min: 8  // At 1Mbps avg
            }
        } else {
            // Some platforms need full model
            DistributionStrategy::Full {
                size_mb: 465,
                estimated_time_min: 62
            }
        };
        
        // Step 3: Schedule distribution based on operational tempo
        let schedule = self.compute_distribution_schedule(
            strategy,
            priority,
            self.get_mission_tempo()
        );
        
        // Step 4: Initiate hierarchical cascade
        // Battalion → Company (2 nodes)
        self.push_to_subordinates(
            model_id,
            new_version,
            strategy,
            vec![company_alpha, company_bravo]
        ).await?;
        
        // Companies automatically propagate to Platoons
        // Platoons to Squads, Squads to Platforms
        // All via PEAT sync protocol
        
        // Step 5: Monitor convergence
        let convergence = self.monitor_convergence(
            model_id,
            new_version,
            timeout_minutes: 30
        ).await?;
        
        Ok(convergence)
    }
    
    fn compute_distribution_schedule(
        &self,
        strategy: DistributionStrategy,
        priority: Priority,
        mission_tempo: MissionTempo
    ) -> DistributionSchedule {
        match (priority, mission_tempo) {
            (Priority::Critical, _) => {
                // ROE update, safety fix - push immediately
                DistributionSchedule::Immediate
            },
            (Priority::High, MissionTempo::Combat) => {
                // Defer until mission phase transition
                DistributionSchedule::NextMaintenanceWindow
            },
            (Priority::Normal, MissionTempo::Combat) => {
                // Wait until return to controlled network
                DistributionSchedule::PostMission
            },
            (_, MissionTempo::Training | MissionTempo::Standby) => {
                // Safe to update during low-tempo operations
                DistributionSchedule::NextSyncWindow
            }
        }
    }
}
```

#### 3. Hierarchical Performance Monitoring

Performance metrics aggregate up through PEAT hierarchy:

```javascript
// Platform-level performance (raw)
{
  "platform_id": "uav_007_alpha_squad",
  "model_id": "target_recognition_yolov8",
  "version": "4.2.1",
  "performance": {
    "avg_latency_ms": 45.3,
    "p95_latency_ms": 67.2,
    "p99_latency_ms": 89.1,
    "avg_confidence": 0.87,
    "false_positive_rate": 0.03,
    "false_negative_rate": 0.11,
    "inference_count": 23847,
    "measured_over_hours": 8.5,
    "last_updated": "2025-11-17T14:23:17Z"
  },
  "resource_usage": {
    "gpu_mem_usage_mb": 1834,
    "cpu_usage_percent": 23,
    "power_draw_watts": 45
  }
}

// Squad-level aggregation (4 platforms)
{
  "squad_id": "alpha_squad",
  "model_id": "target_recognition_yolov8",
  "version": "4.2.1",
  "platforms": [
    {"id": "uav_007", "status": "operational"},
    {"id": "uav_008", "status": "operational"},
    {"id": "uav_009", "status": "degraded", "reason": "high_latency"},
    {"id": "ugv_001", "status": "operational"}
  ],
  "aggregated_performance": {
    "avg_latency_ms": 52.1,  // Avg across 4 platforms
    "max_latency_ms": 124.5,  // uav_009 is slow
    "avg_confidence": 0.85,
    "total_inferences": 87234,
    "operational_platforms": 4,
    "degraded_platforms": 1
  },
  "capability_assessment": {
    "status": "degraded",
    "reason": "One platform experiencing high latency",
    "mission_impact": "minimal",
    "recommendation": "Monitor uav_009, consider model rollback if persists"
  }
}

// Platoon-level aggregation (4 squads)
{
  "platoon_id": "platoon_1",
  "model_id": "target_recognition_yolov8",
  "version": "4.2.1",
  "squads": [
    {"id": "alpha_squad", "status": "degraded", "platforms": 4},
    {"id": "bravo_squad", "status": "operational", "platforms": 4},
    {"id": "charlie_squad", "status": "operational", "platforms": 3},
    {"id": "delta_squad", "status": "operational", "platforms": 4}
  ],
  "aggregated_performance": {
    "avg_latency_ms": 48.7,
    "avg_confidence": 0.86,
    "total_inferences": 312456,
    "operational_platforms": 15,
    "degraded_platforms": 1
  },
  "capability_assessment": {
    "status": "operational",
    "operational_capacity": "93.75%",  // 15/16 platforms operational
    "mission_ready": true,
    "notes": "Minor degradation in Alpha Squad, does not impact platoon capability"
  }
}

// Company-level aggregation (4 platoons)
{
  "company_id": "company_alpha",
  "model_id": "target_recognition_yolov8",
  "version_distribution": {
    "4.2.1": 58,  // 58 platforms on latest
    "4.2.0": 5,   // 5 platforms on previous
    "4.1.9": 1    // 1 platform needs update
  },
  "aggregated_performance": {
    "avg_latency_ms": 49.2,
    "avg_confidence": 0.86,
    "total_inferences": 1247893,
    "operational_platforms": 58,
    "degraded_platforms": 3,
    "outdated_platforms": 3
  },
  "capability_assessment": {
    "status": "operational",
    "operational_capacity": "90.6%",
    "mission_ready": true,
    "recommendations": [
      "Update 6 platforms to version 4.2.1",
      "Investigate Alpha Squad degradation",
      "Consider rollback if degradation spreads"
    ]
  }
}
```

**Key Properties:**
- **Hierarchical aggregation** reduces data volume (64 platform reports → 1 company summary)
- **Capability focus** (operational vs degraded) not just metrics
- **Actionable recommendations** at appropriate echelon
- **Version distribution visibility** enables convergence tracking

#### 4. Training Data Collection

Cannot stream raw data (video, images) over tactical networks. Three strategies:

**Strategy A: Metadata-Driven Collection**
```python
class TrainingDataCollector:
    def should_log_inference(self, result: InferenceResult) -> bool:
        """Determine if inference should be logged for training"""
        # Log interesting cases, not everything
        return (
            result.confidence < 0.7 or  # Low confidence
            result.user_correction is not None or  # Operator corrected
            result.is_edge_case or  # Unusual scenario
            random.random() < 0.01  # 1% random sampling
        )
        
    def log_inference_metadata(
        self,
        model_id: str,
        input_metadata: Dict,
        result: InferenceResult,
        context: OperationalContext
    ):
        """Log metadata, not raw data"""
        metadata = {
            "timestamp": datetime.now(),
            "platform_id": self.platform_id,
            "model_id": model_id,
            "model_version": self.models[model_id]["version"],
            
            # Input characteristics (not raw data)
            "input_metadata": {
                "resolution": input_metadata.resolution,
                "lighting_conditions": input_metadata.lighting,
                "range_meters": input_metadata.range,
                "sensor_type": input_metadata.sensor,
                "data_hash": hash(input_data)  // For deduplication
            },
            
            # Result
            "prediction": result.class_label,
            "confidence": result.confidence,
            "bounding_boxes": result.boxes,
            
            # User correction (if any)
            "user_correction": result.user_correction,
            
            # Operational context
            "mission_phase": context.mission_phase,
            "environment": context.environment,
            "threat_level": context.threat_level,
            
            # Storage reference (for bulk retrieval later)
            "storage_path": f"local://data/inference_{timestamp}.pkl"
        }
        
        # Store locally
        self.local_storage.store(metadata)
        
        # Metadata propagates up via PEAT (small)
        # Raw data stays local until platform returns to base
        self.peat_sync.log_training_metadata(metadata)
```

**Bulk data collection when connectivity permits:**
```python
class TrainingDataUpload:
    async def opportunistic_upload(self):
        """When on controlled network, upload raw training data"""
        # Detect we're on high-bandwidth network
        if not self.is_on_controlled_network():
            return
            
        # Find logged inferences with raw data available
        pending = self.local_storage.get_pending_uploads()
        
        for metadata in pending:
            # Upload raw sensor data
            raw_data = self.local_storage.load_raw(metadata["storage_path"])
            
            await self.upload_to_data_lake(
                metadata=metadata,
                raw_data=raw_data,
                destination="battalion_training_staging"
            )
            
            # Mark as uploaded
            self.local_storage.mark_uploaded(metadata)
```

**Strategy B: Federated Learning**
```python
class FederatedLearningClient:
    """Train local model adaptations, share gradients only"""
    
    async def local_training_round(
        self,
        model_id: str,
        training_data: LocalDataset
    ):
        """Train on local data, compute gradients"""
        model = self.models[model_id]
        
        # Train for N epochs on local data
        initial_weights = model.get_weights()
        
        for epoch in range(self.config.local_epochs):
            for batch in training_data:
                loss = model.train_step(batch)
                
        # Compute weight delta (gradient)
        final_weights = model.get_weights()
        weight_delta = final_weights - initial_weights
        
        # Compress and sign delta
        compressed_delta = compress(weight_delta)
        signed_delta = self.sign_update(compressed_delta)
        
        # Send delta up via PEAT (much smaller than raw data)
        self.peat_sync.send_federated_update(
            model_id=model_id,
            delta=signed_delta,
            training_samples=len(training_data),
            local_loss=loss
        )
        
        # Don't apply yet - wait for aggregated update from server

class FederatedLearningAggregator:
    """At Battalion/Company C2: Aggregate updates from subordinates"""
    
    async def aggregate_federated_updates(
        self,
        model_id: str,
        updates: List[FederatedUpdate]
    ):
        """Federated averaging of weight deltas"""
        # Verify all updates are signed and from authorized nodes
        verified_updates = [u for u in updates if self.verify_update(u)]
        
        # Weighted average by number of training samples
        total_samples = sum(u.training_samples for u in verified_updates)
        
        aggregated_delta = sum(
            u.delta * (u.training_samples / total_samples)
            for u in verified_updates
        )
        
        # Apply to base model
        base_model = self.model_registry.get_model(model_id)
        updated_model = base_model.apply_delta(aggregated_delta)
        
        # Test updated model
        validation_results = self.validate_model(updated_model)
        
        if validation_results.meets_criteria():
            # Publish new version via PEAT
            new_version = self.model_registry.publish(
                model=updated_model,
                validation=validation_results,
                training_metadata={
                    "federated_round": self.round_number,
                    "participating_nodes": len(verified_updates),
                    "total_samples": total_samples
                }
            )
            
            # Distribute via differential propagation
            self.model_distribution.distribute_model_update(
                model_id=model_id,
                new_version=new_version,
                priority=Priority.Normal
            )
```

**Strategy C: Synthetic Data Generation**
```python
class SyntheticDataGenerator:
    """Generate training data from metadata, no raw data needed"""
    
    def generate_training_scenarios(
        self,
        failure_metadata: List[InferenceMetadata]
    ):
        """Use GANs/diffusion models to recreate failure scenarios"""
        
        # Analyze patterns in failure cases
        failure_patterns = self.analyze_failures(failure_metadata)
        
        # Generate synthetic data matching failure patterns
        synthetic_data = []
        for pattern in failure_patterns:
            # Use diffusion model conditioned on metadata
            generated = self.diffusion_model.generate(
                conditions={
                    "lighting": pattern.lighting_conditions,
                    "range": pattern.range_meters,
                    "object_class": pattern.predicted_class,
                    "confidence": pattern.confidence_range
                },
                num_samples=100
            )
            synthetic_data.extend(generated)
            
        # Retrain model on synthetic data
        improved_model = self.retrain(
            base_model=self.current_model,
            synthetic_data=synthetic_data,
            validation_set=self.holdout_data
        )
        
        return improved_model
```

#### 5. Agent Context Integration (MCP Bridge)

PEAT-synced state becomes context for AI agents:

```python
class PeatMCPBridge:
    """Bridge between PEAT distributed state and MCP agent context"""
    
    def __init__(self, peat_sync: PeatSyncEngine):
        self.peat_sync = peat_sync
        self.mcp_server = MCPServer()
        
        # Register PEAT collections as MCP resources
        self.register_peat_resources()
        
    def register_peat_resources(self):
        """Expose PEAT state as MCP resources for agents"""
        
        # Resource: Current model registry
        @self.mcp_server.resource("models://registry")
        def get_model_registry():
            return self.peat_sync.query_collection("models.registry")
            
        # Resource: Platform capabilities
        @self.mcp_server.resource("capabilities://platforms")
        def get_platform_capabilities():
            return self.peat_sync.query_collection("platforms.capabilities")
            
        # Resource: Aggregated performance metrics
        @self.mcp_server.resource("performance://aggregated")
        def get_performance_metrics():
            return self.peat_sync.query_aggregated_state(
                "platforms.performance",
                aggregation_level=self.get_echelon()
            )
            
        # Resource: Mission context
        @self.mcp_server.resource("mission://context")
        def get_mission_context():
            return {
                "roe": self.peat_sync.get("company.orders", "current_roe"),
                "objectives": self.peat_sync.get("platoon.taskings", "objectives"),
                "no_strike_zones": self.peat_sync.get("shared.no_strike_zones"),
                "threat_assessment": self.peat_sync.get("shared.enemy_disposition")
            }
            
        # Tool: Request model update
        @self.mcp_server.tool("request_model_update")
        def request_model_update(model_id: str, target_version: str, justification: str):
            """Agent can request model updates through PEAT"""
            return self.peat_sync.request_model_update(
                model_id=model_id,
                target_version=target_version,
                requested_by=self.agent_id,
                justification=justification
            )
            
        # Tool: Report model performance issue
        @self.mcp_server.tool("report_performance_issue")
        def report_performance_issue(model_id: str, issue_type: str, details: dict):
            """Agent can report degradation through PEAT"""
            return self.peat_sync.log_performance_issue(
                model_id=model_id,
                issue_type=issue_type,
                details=details,
                reported_by=self.agent_id
            )

class EdgeAgent:
    """AI agent using PEAT-synced context via MCP"""
    
    def __init__(self, agent_id: str, mcp_client: MCPClient):
        self.agent_id = agent_id
        self.mcp = mcp_client
        
    async def make_decision(self, situation: Situation):
        """Agent decision-making with PEAT context"""
        
        # Get current context from PEAT via MCP
        model_registry = await self.mcp.get_resource("models://registry")
        capabilities = await self.mcp.get_resource("capabilities://platforms")
        mission_context = await self.mcp.get_resource("mission://context")
        
        # Reasoning loop using context
        decision = await self.reason(
            situation=situation,
            available_capabilities=capabilities,
            mission_constraints=mission_context["roe"],
            model_versions=model_registry
        )
        
        # If decision requires updated model
        if decision.requires_capability_upgrade:
            await self.mcp.call_tool(
                "request_model_update",
                model_id=decision.required_model,
                target_version=decision.required_version,
                justification=decision.reasoning
            )
            
        return decision
```

**Architecture:**
```
┌─────────────────────────────────────┐
│  AI Agent (ReAct, function calling) │
│         ↕ [MCP Protocol]            │
│  MCP Server (Context Provider)      │
│         ↕ [PEAT Bridge]             │
│  PEAT Sync Engine                   │
│    • Model Registry                 │
│    • Capability State               │
│    • Performance Metrics            │
│    • Mission Context                │
│         ↕ [Hierarchical Sync]       │
│  Parent Node in Hierarchy           │
└─────────────────────────────────────┘
```

**Value Proposition:**
- **MCP standardizes** agent-to-context interface
- **PEAT ensures** context is available, current, consistent in DIL environments
- **Separation of concerns**: MCP = local API, PEAT = distributed state
- **Agents reason over hierarchically-appropriate context** (platform sees squad, squad sees platoon, etc.)

### Implementation Phases

#### Phase 1: Foundation (Months 1-3)
- **Model distribution infrastructure** using ADR-013 differential propagation
- **Edge runtime instrumentation** for performance tracking
- **Basic performance aggregation** through PEAT hierarchy
- **Content-addressed model storage** with signature verification

**Success Criteria:**
- Can distribute 500MB model update using <50MB bandwidth (10x reduction)
- Performance metrics from 64 platforms aggregate to company level
- Model provenance verified end-to-end

#### Phase 2: Operational MLOps (Months 3-6)
- **Training metadata collection** at edge
- **Capability assessment dashboard** at each echelon
- **Automated convergence monitoring** for model updates
- **Rollback automation** for problematic models

**Success Criteria:**
- Training metadata from 200+ platforms collected without impacting mission bandwidth
- Commanders see capability state (operational/degraded) not just version numbers
- Can detect and rollback bad model update within 5 minutes

#### Phase 3: Federated Learning (Months 6-9)
- **Federated learning client** at edge platforms
- **Federated aggregation** at company/battalion level
- **On-device fine-tuning** for local adaptation
- **Privacy-preserving gradient sharing**

**Success Criteria:**
- Platforms can train on local data, share only gradients
- Aggregated model improvements without centralizing raw data
- Model performance improves from operational feedback

#### Phase 4: Agent Integration (Months 9-12)
- **MCP bridge** exposing PEAT state to agents
- **Hierarchical agent architecture** (agents at each echelon)
- **Agent-driven model requests** through PEAT
- **Multi-echelon agentic coordination**

**Success Criteria:**
- Agents can query PEAT-synced context via MCP
- Agents at different echelons see appropriate abstraction levels
- Agent decisions propagate through PEAT hierarchy

## Consequences

### Positive

**Operational in DIL Environments:**
- Models run locally where decisions are made
- Hierarchical distribution works with intermittent connectivity
- Performance monitoring doesn't require continuous telemetry
- Training data collection doesn't consume mission bandwidth

**Bandwidth Efficiency:**
- Differential model propagation: 10-100x reduction
- Hierarchical aggregation: Only summaries flow upward
- Metadata-driven collection: Training data collected opportunistically
- Federated learning: Only gradients shared, not raw data

**Operational Capability Focus:**
- Commanders see "can we execute mission" not "what version is installed"
- Performance aggregates to capability assessment at each echelon
- Degradation detected early through hierarchical monitoring
- Rollback automation recovers from bad updates

**Military Integration:**
- Fits command hierarchy naturally
- Integrates with AFRL AI Passport for model validation
- Supports NATO standardization efforts
- Works with existing C2 systems

**Agent Enablement:**
- MCP provides standard interface for agent context
- PEAT ensures context availability in disconnected environments
- Hierarchical abstraction matches agent decision scope
- Agents can request updates through PEAT infrastructure

### Negative

**Implementation Complexity:**
- Differential model propagation requires sophisticated chunking
- Federated learning adds cryptographic and coordination overhead
- MCP bridge requires maintaining adapter layer
- Multiple training strategies complicate operations

**Storage Requirements:**
- Content-addressed storage keeps multiple model versions
- Federated learning requires local training data storage
- Metadata collection requires persistent local storage
- Chunk deduplication needs index structures

**Training Data Limitations:**
- Cannot collect raw data in real-time (bandwidth)
- Federated learning may not work for all model types
- Synthetic data generation has quality limitations
- Metadata alone may not capture all failure modes

**Operational Coordination:**
- Model updates require coordination across hierarchy
- Convergence monitoring adds cognitive load
- Federated learning rounds require synchronization
- Version heterogeneity during updates may cause issues

### Mitigations

**Complexity Management:**
- Start with simple top-down model distribution (Phase 1)
- Add federated learning only where beneficial (Phase 3)
- MCP bridge optional for teams not using agents (Phase 4)
- Provide operational dashboards that hide complexity

**Storage Optimization:**
- Aggressive chunk deduplication across models
- Retention policies for old model versions
- Compression for metadata and gradients
- Storage usage monitoring and alerts

**Training Data Quality:**
- Combine multiple strategies (metadata + federated + synthetic)
- Prioritize high-value failure cases for raw data collection
- Validate synthetic data against held-out real data
- Use human-in-the-loop for critical model updates

**Operational Support:**
- Clear capability assessment dashboards
- Automated model update scheduling based on mission tempo
- Rollback playbooks for common scenarios
- Training for operators on interpreting degradation signals

## Integration Points

### With ADR-013 (Distributed Software & AI Operations)
- **Foundation:** ADR-022 builds on ADR-013's differential propagation
- **Extends:** Adds ML-specific considerations (models, training, performance)
- **Shares:** Content-addressed storage, signature chains, hierarchical distribution
- **Complements:** ADR-013 is general software, ADR-022 is ML-specific

### With ADR-006 (Security, Authentication, Authorization)
- **Model provenance:** Signature chains for model verification
- **Federated learning:** Cryptographic verification of gradient updates
- **Agent authorization:** MCP tools respect PEAT authorization model
- **Training data:** Encryption of sensitive training metadata

### With ADR-007 (Automerge-Based Sync Engine)
- **Performance metrics:** Sync via Automerge CRDTs
- **Model registry:** Distributed model metadata in Automerge
- **Training metadata:** Lightweight metadata sync
- **Capability state:** Automerge enables conflict-free capability aggregation

### With ADR-009 (Bidirectional Hierarchical Flows)
- **Models flow down:** Distribution from Battalion → Platforms
- **Performance flows up:** Metrics aggregate from Platforms → Battalion
- **Federated updates flow up:** Gradients from Platforms → Aggregator
- **Agent decisions flow bidirectionally:** Context down, requests up

### With ADR-004 (Human-Machine Squad Composition)
- **Authority for model deployment:** Who can approve model updates
- **Cognitive load:** Model performance affects operator workload
- **Agent authority:** AI agents using models have dynamic authority
- **Human oversight:** Humans approve critical model changes

### With ADR-019 (TTL and Data Lifecycle)
- **Model versioning:** Deprecated models have TTL
- **Performance metrics:** Aggregate with time-based retention
- **Training metadata:** TTL for uploaded data
- **Cache expiry:** Model caches expire based on policy

## Alternatives Considered

### Alternative 1: Traditional Centralized MLOps (MLflow, Kubeflow)
**Approach:** Push-based deployment from central registry

**Rejected Because:**
- Assumes continuous connectivity to central services
- No hierarchical distribution strategy
- Full model transfers wasteful on tactical networks
- No offline-first design for DIL environments
- Not designed for military hierarchy

### Alternative 2: Edge AI Frameworks Only (TFLite, ONNX Runtime)
**Approach:** Deploy optimized models, no lifecycle management

**Rejected Because:**
- No distributed model update mechanism
- Manual updates don't scale to 1000+ platforms
- No performance monitoring infrastructure
- No training data collection strategy
- Missing operational capability focus

### Alternative 3: Pure Federated Learning (Flower, PySyft)
**Approach:** Focus entirely on federated training

**Rejected Because:**
- Doesn't address model distribution problem
- High bandwidth for gradient exchange
- Assumes periodic connectivity to aggregator
- Not optimized for military hierarchy
- Missing operational monitoring

### Alternative 4: Container-Based ML Deployment (KubeFlow, Seldon)
**Approach:** Package models in containers, use K8s

**Rejected Because:**
- Assumes control plane connectivity
- Layer-based distribution still large
- No hierarchical propagation
- Not designed for contested environments
- Heavy resource requirements for edge

**Why PEAT Edge MLOps:**
- **Hierarchical by design:** Matches military organization
- **Offline-first:** Works in DIL environments
- **Differential propagation:** Optimal for bandwidth constraints
- **Capability focus:** Operational assessment, not just inventory
- **Integrated:** Leverages existing PEAT infrastructure for distribution, monitoring, and coordination

## References

- ADR-013: Distributed Software and AI Operations
- ADR-007: Automerge-Based Sync Engine
- ADR-009: Bidirectional Hierarchical Flows
- ADR-006: Security, Authentication, and Authorization
- ADR-004: Human-Machine Squad Composition
- ADR-019: TTL and Data Lifecycle Abstraction
- AFRL AI Passport System: Model validation and attestation
- MCP (Model Context Protocol): Anthropic's agent context standard
- "Federated Learning: Strategies for Improving Communication Efficiency" (Konečný et al., 2016)
- "Communication-Efficient Learning of Deep Networks from Decentralized Data" (McMahan et al., 2017)
- "TinyML: Machine Learning with TensorFlow Lite on Arduino and Ultra-Low-Power Microcontrollers" (Warden & Situnayake, 2019)

## Future Considerations

**Advanced Model Optimization:**
- Neural architecture search at edge
- Automated quantization based on platform capabilities
- Knowledge distillation for resource-constrained platforms
- Dynamic model compression during distribution

**Multi-Domain Learning:**
- Cross-domain transfer learning (air, ground, maritime)
- Multi-task models shared across domains
- Domain-specific fine-tuning via federated learning
- Unified model registry across joint operations

**Adversarial Robustness:**
- Adversarial training at edge
- Federated adversarial learning
- Robustness verification before deployment
- Automatic rollback on adversarial attack detection

**Explainability and Auditing:**
- Model explainability at edge (SHAP, LIME)
- Decision audit trails linking to model versions
- Compliance verification for ROE adherence
- Operator trust through transparency

**AutoML at the Edge:**
- Automated hyperparameter tuning
- Neural architecture search for platform-specific optimization
- Curriculum learning based on mission progression
- Meta-learning for rapid adaptation

---

## Appendix A: Model Format Comparison for Tactical Edge

This appendix compares the three primary model formats considered for PEAT edge deployments: **ONNX**, **TensorFlow Lite (TFLite)**, and **Native Framework Formats** (PyTorch .pt/.pth, TensorFlow SavedModel).

### Evaluation Criteria for Military Edge

| Criterion | Weight | Rationale |
|-----------|--------|-----------|
| **Vendor Neutrality** | High | Government procurement requires avoiding vendor lock-in |
| **Security Auditability** | Critical | Models must be inspectable for malicious operators/backdoors |
| **Hardware Portability** | High | Diverse tactical platforms (CPU, GPU, NPU, edge accelerators) |
| **Performance** | High | Real-time inference requirements (object detection, tracking) |
| **Size/Bandwidth** | Critical | Tactical networks are bandwidth-constrained (9.6Kbps-1Mbps) |
| **NATO Interoperability** | High | Allied forces must share models across nations |
| **Ecosystem Maturity** | Medium | Contractor support, tooling, documentation |
| **Quantization Support** | High | Reduced model size and faster inference on edge hardware |

### Detailed Comparison

#### 1. ONNX (Open Neural Network Exchange)

**Strengths:**
- ✅ **Vendor neutral:** Linux Foundation project, not controlled by single company
- ✅ **Security auditable:** Human-readable Protocol Buffers format, clear graph structure
- ✅ **Hardware portable:** ONNX Runtime supports 15+ execution providers (CUDA, TensorRT, OpenVINO, CoreML, DirectML, etc.)
- ✅ **Framework agnostic:** Convert from PyTorch, TensorFlow, JAX, ONNX is the "assembly language" for ML
- ✅ **Performance:** ONNX Runtime highly optimized, often faster than native frameworks for inference
- ✅ **Quantization:** INT8, FP16, dynamic quantization built-in
- ✅ **NATO-friendly:** Emerging standard in defense/intel, facilitates allied model sharing
- ✅ **Differential sync friendly:** Structured format enables semantic chunking by layers

**Weaknesses:**
- ⚠️ **Conversion overhead:** Training in native framework, convert to ONNX for deployment
- ⚠️ **Operator coverage:** Some cutting-edge operators may not be in ONNX spec yet
- ⚠️ **Debugging:** Errors in ONNX conversion can be opaque

**Tactical Edge Suitability:** ⭐⭐⭐⭐⭐ (5/5)

**PEAT Integration:**
```python
# ONNX as standard format
model_variants = {
    "fp32_cuda": "model.onnx",      # 487MB - GPU platforms
    "fp16_cuda": "model_fp16.onnx", # 244MB - GPU with half precision
    "int8_cpu": "model_int8.onnx",  # 122MB - CPU/edge platforms
}

# Differential sync: 16MB delta between versions vs 487MB full model
# PEAT distributes only changed weights using ONNX graph structure
```

**Size Analysis:**
| Model | PyTorch | ONNX FP32 | ONNX FP16 | ONNX INT8 | Bandwidth Savings |
|-------|---------|-----------|-----------|-----------|-------------------|
| YOLOv8n | 6.2MB | 6.2MB | 3.1MB | 1.6MB | 74% (INT8 vs FP32) |
| YOLOv8m | 49.7MB | 49.7MB | 24.9MB | 12.5MB | 75% |
| YOLOv8x | 136.7MB | 136.7MB | 68.4MB | 34.2MB | 75% |
| ResNet50 | 97.8MB | 97.8MB | 48.9MB | 24.5MB | 75% |
| EfficientNet-B0 | 20.5MB | 20.5MB | 10.3MB | 5.2MB | 75% |

**Performance Benchmarks:**
| Model | Platform | PyTorch | ONNX Runtime | Speedup |
|-------|----------|---------|--------------|---------|
| YOLOv8n | NVIDIA Jetson Xavier | 28ms | 22ms | 1.27x |
| YOLOv8m | NVIDIA RTX 4090 | 8ms | 6ms | 1.33x |
| ResNet50 | Intel i7-12700K (CPU) | 45ms | 32ms | 1.41x |
| EfficientNet | Qualcomm Snapdragon 888 | 67ms | 52ms | 1.29x |

#### 2. TensorFlow Lite (TFLite)

**Strengths:**
- ✅ **Optimized for mobile/edge:** Designed specifically for resource-constrained devices
- ✅ **Small model size:** Aggressive optimization and quantization
- ✅ **Hardware acceleration:** TFLite delegates for GPU, NPU, DSP
- ✅ **Quantization:** Post-training and quantization-aware training support
- ✅ **Mature ecosystem:** Extensive mobile deployment experience
- ✅ **On-device ML focus:** Optimized for battery-powered, memory-constrained platforms

**Weaknesses:**
- ❌ **Google-centric:** Controlled by Google, less vendor neutral
- ❌ **TensorFlow lock-in:** Primarily designed for TensorFlow models (PyTorch conversion awkward)
- ❌ **Limited operator coverage:** Many operators not supported, must convert to TFLite-compatible ops
- ❌ **Less hardware portable:** Fewer execution backends than ONNX Runtime
- ❌ **Opaque format:** FlatBuffers format harder to audit than ONNX
- ❌ **NATO concern:** Google ownership raises sovereignty concerns for some allies

**Tactical Edge Suitability:** ⭐⭐⭐ (3/5)

**PEAT Integration Challenges:**
```python
# TFLite more difficult to integrate into PEAT
# - Conversion from PyTorch requires TF intermediate step
# - Less semantic structure for differential sync
# - Fewer hardware backend options
# - Google ownership raises procurement concerns
```

**Size Analysis:**
| Model | TensorFlow | TFLite FP32 | TFLite FP16 | TFLite INT8 |
|-------|------------|-------------|-------------|-------------|
| MobileNetV2 | 14.0MB | 14.0MB | 7.1MB | 3.6MB |
| EfficientNet-Lite0 | 20.1MB | 20.1MB | 10.1MB | 5.1MB |
| SSD MobileNet | 27.3MB | 27.3MB | 13.7MB | 6.9MB |

**Performance:** Generally good on mobile/embedded, but less optimized for server-class edge hardware (e.g., NVIDIA Jetson)

#### 3. Native Framework Formats (PyTorch .pt, TensorFlow SavedModel)

**Strengths:**
- ✅ **No conversion:** Train and deploy in same format
- ✅ **Full operator support:** All framework operators available
- ✅ **Debugging:** Easier to debug in native framework
- ✅ **Flexibility:** Can use framework-specific features

**Weaknesses:**
- ❌ **Vendor lock-in:** PyTorch (Meta), TensorFlow (Google)
- ❌ **Less hardware portable:** Limited to framework-supported backends
- ❌ **Security concerns:** Opaque serialization formats (pickle in PyTorch)
- ❌ **Larger models:** Less aggressive optimization than ONNX/TFLite
- ❌ **Framework dependencies:** Must deploy full framework (PyTorch: 700MB+, TF: 400MB+)
- ❌ **NATO interoperability:** Different allies may use different frameworks
- ❌ **Differential sync harder:** Less structured format for semantic chunking

**Tactical Edge Suitability:** ⭐⭐ (2/5)

**PEAT Integration Challenges:**
```python
# Native formats problematic for PEAT
# - Large runtime dependencies (PyTorch 700MB + model 500MB = 1.2GB)
# - Vendor lock-in unacceptable for government procurement
# - Security: PyTorch uses pickle (arbitrary code execution risk)
# - No standard format across contractors/allies
```

**Size Analysis:**
| Component | PyTorch | TensorFlow | ONNX Runtime |
|-----------|---------|------------|--------------|
| Runtime | 700MB | 400MB | 15MB |
| YOLOv8m Model | 49.7MB | 51.2MB | 49.7MB (FP32) / 12.5MB (INT8) |
| **Total Edge Footprint** | **749.7MB** | **451.2MB** | **27.5MB (INT8)** |

**Bandwidth Impact:**
- Deploying PyTorch YOLOv8m to 100 platforms: 74.97GB
- Deploying TensorFlow to 100 platforms: 45.12GB  
- Deploying ONNX INT8 to 100 platforms: 2.75GB
- **ONNX saves 72.22GB (96% reduction)** vs PyTorch

### Recommendation Matrix

| Use Case | Recommended Format | Rationale |
|----------|-------------------|-----------|
| **Military tactical edge** | ONNX | Vendor neutral, auditable, portable, NATO-friendly |
| **DIL environments** | ONNX | Small size (INT8), differential sync friendly |
| **Multi-vendor procurement** | ONNX | Contractors can use any framework, convert to ONNX |
| **NATO/allied operations** | ONNX | Standard format enables model sharing across nations |
| **Heterogeneous hardware** | ONNX | Single model runs on CPU/GPU/NPU via execution providers |
| **Security-critical** | ONNX | Human-readable graph, operator whitelisting |
| **Extremely constrained** | TFLite | When <10MB total footprint required |
| **Research/experimentation** | Native | Rapid iteration, full operator support |

### PEAT Architecture Decision

**PEAT adopts ONNX as the standard model format** for the following reasons:

1. **Vendor Neutrality:** Critical for government procurement and multi-vendor ecosystem
2. **Security:** Auditable graph structure enables malware detection and operator whitelisting
3. **Portability:** Single format for diverse tactical platforms (air, ground, maritime, dismounted)
4. **NATO Interoperability:** Facilitates AUKUS/Five Eyes model sharing without framework dependencies
5. **Bandwidth Efficiency:** Structured format enables intelligent differential sync (layer-level deltas)
6. **Performance:** ONNX Runtime competitive with or faster than native frameworks
7. **Quantization:** INT8 quantization reduces model size by 75% with minimal accuracy loss
8. **Ecosystem:** Contractor support, integration with AFRL AI Passport, MLOps tooling

**Operational Impact:**
```
Traditional PyTorch Deployment:
- Runtime: 700MB per platform
- Model: 500MB per model
- 200 platforms = 140GB runtime + 100GB models = 240GB total
- Update: 100GB for new model version

PEAT with ONNX:
- Runtime: 15MB per platform (ONNX Runtime)
- Model: 125MB INT8 per model
- 200 platforms = 3GB runtime + 25GB models = 28GB total (88% reduction)
- Update: 3.2GB differential (97% reduction vs full model)
```

**Security Benefits:**
```python
# ONNX enables operator whitelisting
approved_operators = [
    "Conv", "BatchNormalization", "Relu", "MaxPool",
    "GlobalAveragePool", "Gemm", "Concat", "Reshape",
    "Sigmoid", "Softmax", "Transpose", "Add", "Mul"
]

def verify_onnx_security(model_path: str) -> bool:
    model = onnx.load(model_path)
    
    # Check all operators against whitelist
    for node in model.graph.node:
        if node.op_type not in approved_operators:
            logger.error(f"Unapproved operator: {node.op_type}")
            return False
            
    # Check for custom operators (potential malware)
    if any(node.domain != "ai.onnx" for node in model.graph.node):
        logger.error("Custom operators detected")
        return False
        
    return True
```

**NATO Standardization Argument:**
> "PEAT uses ONNX as the standard model interchange format, enabling allied forces to share AI capabilities without vendor lock-in. A US-trained ONNX model can deploy to UK, Australian, or Canadian platforms via PEAT's hierarchical distribution, supporting coalition operations and AUKUS Pillar II technology sharing objectives."

### Future Considerations

**Multi-Format Support:**
While ONNX is the standard, PEAT architecture allows for alternative formats when operationally necessary:
- TFLite for extremely constrained platforms (<100MB storage)
- Native formats for experimental/research deployments
- Emerging formats (e.g., MLIR, StableHLO) as they mature

**Conversion Pipeline:**
```
Contractor Training → Native Format (PyTorch/TF/JAX)
    ↓
ONNX Conversion & Optimization
    ↓
Security Review & Operator Whitelisting
    ↓
Quantization (FP16/INT8)
    ↓
AFRL AI Passport Validation
    ↓
PEAT Model Registry
    ↓
Hierarchical Distribution to Tactical Edge
```

---

**This ADR establishes PEAT as the enabling infrastructure for edge-first ML operations in contested tactical environments, supporting the full model lifecycle from distribution through training while maintaining operational capability focus throughout the hierarchy.**
