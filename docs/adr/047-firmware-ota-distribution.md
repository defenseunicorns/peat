# ADR-047: Firmware OTA Distribution via PEAT Mesh

**Status**: Accepted
**Date**: 2026-02-22
**Authors**: Kit Plummer, Claude
**Relates to**: ADR-013 (Distributed Software Ops), ADR-022 (Edge MLOps), ADR-025 (Blob Transfer), ADR-026 (Software Orchestration), ADR-035 (PEAT-Lite), ADR-045 (Zarf/UDS Integration)

## Context

### The Missing Middle

PEAT has strong coverage for two ends of the software delivery spectrum:

- **Server/K8s workloads** — Zarf packages containers, PEAT coordinates deployment (ADR-045)
- **Sensor/MCU nodes** — PEAT-Lite enables ESP32-class devices to participate in the mesh (ADR-035)

Between these sits a large class of platforms that need **firmware updates** but don't run Kubernetes:

| Platform | Processor | OS/Runtime | Examples |
|----------|-----------|------------|----------|
| Drone autopilots | STM32, NXP i.MX | NuttX, FreeRTOS | PX4, ArduPilot |
| Vehicle ECUs | ARM Cortex-R/M | RTOS, bare metal | Engine, braking, comms modules |
| Radio systems | FPGA + DSP | Custom, embedded Linux | Software-defined radios, tactical radios |
| Camera/sensor payloads | ARM, RISC-V | Embedded Linux, RTOS | EO/IR systems, LIDAR processors |
| Robotics controllers | Various ARM | ROS2/embedded Linux | Motor controllers, navigation boards |
| Gateway devices | ARM Cortex-A | Embedded Linux | Network bridges, protocol translators |
| Weapon system controllers | Safety-rated MCU | Safety-certified RTOS | Fire control, fuzing, guidance |

These platforms share common characteristics:
- **No container runtime** — firmware is a monolithic binary image, not a layered OCI artifact
- **No on-device orchestrator** — no K8s, no Zarf, no package manager
- **Hardware-coupled** — firmware must match exact board revision, peripheral configuration
- **Boot-critical** — a failed update can brick the device (unlike a failed container deploy that gets restarted)
- **Resource constrained** — limited storage, often single-digit MB of RAM
- **Safety-critical** — some platforms have certification requirements (DO-178C, IEC 61508)

### Why Existing OTA Solutions Fall Short

Several open-source and commercial firmware OTA solutions exist:

| Solution | Approach | Gap for Tactical Edge |
|----------|----------|----------------------|
| **Mender** | Client-server, A/B partition updates | Requires continuous server connectivity |
| **SWUpdate** | Local update agent, delta updates | No distributed coordination or fleet management |
| **RAUC** | Slot-based updates, cryptographic verification | Single-device focus, no mesh distribution |
| **Balena** | Container-based device management | Assumes connectivity to balenaCloud |
| **hawkBit** | Eclipse IoT, campaign management | Centralized server architecture |

Common gaps across all:
1. **Centralized architecture** — require connectivity to a management server
2. **No mesh distribution** — can't leverage peer-to-peer transfer in bandwidth-constrained environments
3. **No hierarchical coordination** — flat fleet model, no echelon-based rollout
4. **No cross-platform orchestration** — firmware-only; can't coordinate with model/container updates on the same platform
5. **No DIL operation** — designed for IoT with reliable cloud connectivity, not contested networks

### The Multi-Artifact Platform Problem

Modern tactical platforms are not single-firmware devices. A typical drone carries:

```
Drone Platform (single asset)
├── Autopilot firmware        (STM32/NuttX)      ← Firmware OTA
├── Companion computer OS     (Jetson/Linux)      ← Container or firmware
├── AI perception model       (ONNX on Jetson)    ← Model delivery (ADR-022)
├── Camera sensor firmware    (FPGA bitstream)     ← Firmware OTA
├── Radio firmware            (SDR baseband)       ← Firmware OTA
├── Battery management FW     (BMS MCU)            ← Firmware OTA
└── Mission config/ROE        (all processors)     ← Config sync via CRDT
```

Today, updating this platform requires 4+ separate systems with no coordination between them. An operator cannot answer: "Is this drone fully updated and mission-ready?" without checking each system independently.

### Customer Demand Signal

Defense and intelligence customers are asking for:
1. **Deliver firmware to platforms that don't run K8s** — extend UDS beyond the enterprise edge
2. **Deliver AI models to inference hardware** — GPU nodes, edge accelerators
3. **Unified fleet visibility** — "what firmware/model/config is running on every asset?"
4. **Coordinated multi-artifact updates** — update autopilot firmware AND perception model as an atomic operation
5. **Disconnected operation** — updates must work over intermittent tactical links

## Decision

### Extend PEAT's Distribution Layer to Firmware Targets

Add firmware as a first-class artifact type alongside containers (Zarf), AI models (ONNX), and configuration (CRDT), using the same PEAT protocol primitives for coordination.

### Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                        PEAT Protocol Layer                            │
│  BlobStore, DeploymentDirective, CapabilityAdvertisement, PeatEvent  │
│  Convergence Tracking, QoS, Hierarchical Distribution                │
├───────────┬───────────┬───────────┬──────────────┬───────────────────┤
│ Zarf      │ Firmware  │ AI Model  │ Config       │ PEAT-Lite         │
│ Adapter   │ OTA Agent │ Runtime   │ Sync         │ Gossip            │
│           │           │           │              │                   │
│ K8s/K3s   │ Embedded  │ GPU/NPU   │ All nodes    │ MCU sensors       │
│ clusters  │ platforms │ platforms │              │                   │
└───────────┴───────────┴───────────┴──────────────┴───────────────────┘
```

### 1. Firmware Manifest Schema

Firmware manifests describe what's being delivered and to what hardware:

```protobuf
message FirmwareManifest {
  // Identity
  string firmware_id = 1;           // e.g., "px4-autopilot"
  string version = 2;               // e.g., "1.14.3"
  string display_name = 3;          // Human-readable name

  // Artifact
  string blob_hash = 4;             // Content-addressed hash (BlobStore)
  uint64 size_bytes = 5;
  FirmwareFormat format = 6;

  // Hardware Compatibility
  repeated HardwareTarget targets = 7;

  // Update Policy
  UpdatePolicy update_policy = 8;

  // Provenance
  Provenance provenance = 9;

  // Dependencies
  repeated FirmwareDependency dependencies = 10;
}

enum FirmwareFormat {
  RAW_BINARY = 0;          // Flat binary image
  ELF = 1;                 // ELF executable
  INTEL_HEX = 2;           // Intel HEX format
  SREC = 3;                // Motorola S-record
  UF2 = 4;                 // USB Flashing Format
  FPGA_BITSTREAM = 5;      // FPGA configuration
  DELTA_PATCH = 6;         // Binary diff against known base version
  SIGNED_ENVELOPE = 7;     // Encrypted/signed wrapper (unwrapped on device)
}

message HardwareTarget {
  string board_id = 1;              // e.g., "pixhawk6x", "stm32h7-rev-b"
  string cpu_architecture = 2;      // e.g., "arm-cortex-m7", "aarch64"
  string bootloader_version_min = 3;// Minimum bootloader version required
  string bootloader_version_max = 4;// Maximum compatible bootloader
  string board_revision_min = 5;    // Minimum board hardware revision
  repeated string peripheral_requirements = 6;  // Required peripherals
}

message UpdatePolicy {
  ActivationMode activation = 1;
  RollbackPolicy rollback = 2;
  repeated SafetyConstraint safety_constraints = 3;
  uint32 max_concurrent_updates = 4;  // Per formation
  bool requires_human_approval = 5;
}

enum ActivationMode {
  IMMEDIATE_REBOOT = 0;    // Apply and reboot now
  DEFERRED_REBOOT = 1;     // Stage now, reboot at maintenance window
  HOT_SWAP = 2;            // Live update without reboot (if supported)
  MANUAL_ACTIVATION = 3;   // Stage only, operator triggers activation
}

message RollbackPolicy {
  bool auto_rollback_on_boot_failure = 1;
  uint32 boot_verification_timeout_sec = 2;  // How long to wait for health check
  string golden_image_version = 3;           // Fallback if all else fails
  uint32 max_rollback_attempts = 4;
}

message SafetyConstraint {
  string constraint_type = 1;       // "min_battery", "stable_power", "not_in_flight"
  string constraint_value = 2;      // "80", "true", "true"
  string description = 3;           // Human-readable explanation
}

message FirmwareDependency {
  string firmware_id = 1;           // Depends on this other firmware
  string version_min = 2;           // Minimum version
  string version_max = 3;           // Maximum version
  DependencyType type = 4;
}

enum DependencyType {
  REQUIRES = 0;             // Must be present before this firmware installs
  CONFLICTS = 1;            // Must NOT be present
  CO_DEPLOY = 2;            // Should be deployed together (atomic group)
}

message Provenance {
  string signed_by = 1;
  bytes signature = 2;
  string trust_chain = 3;
  string build_id = 4;              // CI/CD build identifier
  string source_commit = 5;         // Source control reference
  string sbom_reference = 6;        // Software Bill of Materials
  google.protobuf.Timestamp built_at = 7;
}
```

### 2. Firmware OTA Lifecycle

The update lifecycle is more complex than container or model deployment because of the boot-critical nature:

```
┌──────────────────────────────────────────────────────────────────────┐
│                    Firmware OTA State Machine                         │
│                                                                      │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐       │
│  │ AVAILABLE│───▶│DOWNLOADING│───▶│ STAGED   │───▶│ACTIVATING│       │
│  │          │    │          │    │          │    │          │       │
│  └──────────┘    └────┬─────┘    └────┬─────┘    └────┬─────┘       │
│                       │               │               │              │
│                       ▼               ▼               ▼              │
│                  ┌──────────┐    ┌──────────┐    ┌──────────┐       │
│                  │  FAILED  │    │  FAILED  │    │VERIFYING │       │
│                  │(download)│    │(staging) │    │  (boot)  │       │
│                  └──────────┘    └──────────┘    └────┬─────┘       │
│                                                       │              │
│                                            ┌──────────┴──────────┐   │
│                                            ▼                     ▼   │
│                                      ┌──────────┐         ┌────────┐│
│                                      │COMMITTED │         │ROLLBACK││
│                                      │ (active) │         │        ││
│                                      └──────────┘         └────────┘│
└──────────────────────────────────────────────────────────────────────┘
```

**State definitions:**

| State | Description | PEAT Action |
|-------|-------------|-------------|
| `AVAILABLE` | Firmware manifest received via PEAT sync | Node evaluates hardware compatibility |
| `DOWNLOADING` | Blob transfer in progress via BlobStore | Progress reported via PeatEvent |
| `STAGED` | Firmware written to inactive partition/slot | Awaiting activation trigger |
| `ACTIVATING` | Reboot initiated (or hot-swap in progress) | Node goes temporarily offline |
| `VERIFYING` | New firmware booted, running health checks | Boot verification timer started |
| `COMMITTED` | Health checks passed, new firmware is active | CapabilityAdvertisement updated |
| `ROLLBACK` | Boot verification failed, reverted to previous | PeatEvent with failure details |
| `FAILED` | Unrecoverable failure at any stage | Alert event, manual intervention needed |

### 3. Firmware OTA Agent

A lightweight agent that runs on (or alongside) firmware targets:

```rust
/// Firmware OTA Agent — runs on the target platform or its companion processor
///
/// Responsibilities:
/// - Receive deployment directives via PEAT mesh
/// - Download firmware blobs via BlobStore
/// - Manage partition staging and activation
/// - Report status through PEAT events
///
/// The agent is transport-agnostic — it works over QUIC, BLE, or UDP
/// depending on what the platform supports.

pub struct FirmwareOtaAgent {
    /// PEAT mesh connection (may be QUIC, BLE, or UDP)
    peat_node: Arc<dyn PeatNode>,

    /// Local firmware partition manager
    partition_mgr: Box<dyn PartitionManager>,

    /// Hardware identity
    hardware_info: HardwareInfo,

    /// Current firmware state
    state: RwLock<FirmwareState>,
}

/// Platform-specific partition management
///
/// Implementations exist for different update mechanisms:
/// - A/B partition scheme (Linux, Android)
/// - Bank-swap (STM32 dual-bank flash)
/// - External flash staging (SPI NOR/NAND)
/// - File-based (embedded Linux with initramfs)
#[async_trait]
pub trait PartitionManager: Send + Sync {
    /// Get current active firmware info
    async fn active_firmware(&self) -> Result<FirmwareInfo>;

    /// Get inactive/staging slot info
    async fn staging_slot(&self) -> Result<SlotInfo>;

    /// Write firmware image to staging slot
    async fn stage_firmware(
        &self,
        image: &[u8],
        manifest: &FirmwareManifest,
    ) -> Result<()>;

    /// Mark staging slot as bootable and trigger activation
    async fn activate(&self) -> Result<()>;

    /// Confirm current firmware is good (commit after boot verification)
    async fn commit(&self) -> Result<()>;

    /// Revert to previous firmware
    async fn rollback(&self) -> Result<()>;

    /// Check if platform meets safety constraints for update
    async fn check_safety_constraints(
        &self,
        constraints: &[SafetyConstraint],
    ) -> Result<Vec<ConstraintResult>>;
}

pub struct HardwareInfo {
    pub board_id: String,
    pub board_revision: String,
    pub cpu_architecture: String,
    pub bootloader_version: String,
    pub current_firmware_version: String,
    pub storage_available_bytes: u64,
    pub peripherals: Vec<String>,
}
```

**Agent operation flow:**

```rust
impl FirmwareOtaAgent {
    /// Main loop — watches for deployment directives and executes them
    pub async fn run(&self) -> Result<()> {
        // 1. Advertise hardware capabilities on startup
        self.advertise_hardware().await?;

        // 2. Watch for firmware deployment directives targeting this node
        let mut directives = self.peat_node
            .subscribe_directives("firmware_deployments")
            .await?;

        while let Some(directive) = directives.next().await {
            // 3. Verify directive is signed by authorized source
            if !self.verify_directive_signature(&directive)? {
                self.report_event(FirmwareEvent::RejectedUnauthorized {
                    directive_id: directive.id.clone(),
                }).await?;
                continue;
            }

            // 4. Check hardware compatibility
            let manifest = &directive.firmware_manifest;
            if !self.is_compatible(manifest) {
                self.report_event(FirmwareEvent::IncompatibleHardware {
                    directive_id: directive.id.clone(),
                    reason: self.compatibility_mismatch(manifest),
                }).await?;
                continue;
            }

            // 5. Check safety constraints
            let safety_results = self.partition_mgr
                .check_safety_constraints(&manifest.update_policy.safety_constraints)
                .await?;
            if safety_results.iter().any(|r| !r.passed) {
                self.report_event(FirmwareEvent::SafetyConstraintFailed {
                    directive_id: directive.id.clone(),
                    failures: safety_results.iter()
                        .filter(|r| !r.passed)
                        .cloned()
                        .collect(),
                }).await?;
                continue;
            }

            // 6. Execute the update
            self.execute_update(directive).await?;
        }

        Ok(())
    }

    async fn execute_update(&self, directive: DeploymentDirective) -> Result<()> {
        let manifest = &directive.firmware_manifest;
        let directive_id = &directive.id;

        // Download firmware blob
        self.update_state(FirmwareOtaState::Downloading).await;
        self.report_status(directive_id, "DOWNLOADING").await?;

        let blob = self.peat_node.blob_store()
            .fetch_blob(&manifest.blob_hash, |progress| {
                // Report download progress periodically
            }).await?;

        // Verify hash
        if !verify_hash(&blob.data, &manifest.blob_hash) {
            self.report_event(FirmwareEvent::HashVerificationFailed {
                directive_id: directive_id.clone(),
            }).await?;
            return Err(anyhow!("Firmware hash verification failed"));
        }

        // Stage to inactive partition
        self.update_state(FirmwareOtaState::Staging).await;
        self.report_status(directive_id, "STAGED").await?;

        self.partition_mgr
            .stage_firmware(&blob.data, manifest)
            .await?;

        // Activate based on policy
        match manifest.update_policy.activation {
            ActivationMode::ImmediateReboot => {
                self.update_state(FirmwareOtaState::Activating).await;
                self.report_status(directive_id, "ACTIVATING").await?;
                self.partition_mgr.activate().await?;
                // Device reboots here — post-boot verification happens
                // in the boot_verification() method on next startup
            }
            ActivationMode::DeferredReboot => {
                // Stay in STAGED state until maintenance window
                self.report_status(directive_id, "STAGED_AWAITING_WINDOW").await?;
            }
            ActivationMode::ManualActivation => {
                self.report_status(directive_id, "STAGED_AWAITING_MANUAL").await?;
            }
            ActivationMode::HotSwap => {
                // Platform-specific live update
                self.partition_mgr.activate().await?;
                self.boot_verification(directive_id).await?;
            }
        }

        Ok(())
    }

    /// Called on startup after a firmware update activation
    async fn boot_verification(&self, directive_id: &str) -> Result<()> {
        self.update_state(FirmwareOtaState::Verifying).await;
        self.report_status(directive_id, "VERIFYING").await?;

        let timeout = self.pending_manifest()
            .map(|m| m.update_policy.rollback.boot_verification_timeout_sec)
            .unwrap_or(60);

        // Run platform-specific health checks
        match tokio::time::timeout(
            Duration::from_secs(timeout as u64),
            self.run_health_checks(),
        ).await {
            Ok(Ok(())) => {
                // Health checks passed — commit the new firmware
                self.partition_mgr.commit().await?;
                self.update_state(FirmwareOtaState::Committed).await;
                self.report_status(directive_id, "COMMITTED").await?;

                // Update capability advertisement with new version
                self.advertise_hardware().await?;
            }
            _ => {
                // Health checks failed or timed out — rollback
                self.partition_mgr.rollback().await?;
                self.update_state(FirmwareOtaState::RolledBack).await;
                self.report_event(FirmwareEvent::RollbackTriggered {
                    directive_id: directive_id.to_string(),
                    reason: "Boot verification failed".to_string(),
                }).await?;
            }
        }

        Ok(())
    }
}
```

### 4. Hardware Capability Advertisement

Firmware targets advertise their hardware identity and current firmware state through PEAT's existing `CapabilityAdvertisement`:

```protobuf
message FirmwareCapability {
  // Hardware identity
  string board_id = 1;
  string board_revision = 2;
  string cpu_architecture = 3;
  string bootloader_version = 4;

  // Current firmware state
  string firmware_id = 5;
  string firmware_version = 6;
  string firmware_hash = 7;
  FirmwareOtaState ota_state = 8;

  // Resources
  uint64 staging_storage_bytes = 9;
  uint64 battery_percent = 10;
  bool external_power = 11;

  // OTA agent capabilities
  repeated FirmwareFormat supported_formats = 12;
  bool supports_delta_updates = 13;
  bool supports_hot_swap = 14;
  bool supports_a_b_partitions = 15;
}
```

This enables the control plane to:
- **Discover** all firmware targets in the mesh and their hardware types
- **Assess compatibility** before issuing deployment directives
- **Track convergence** — which devices have which firmware version
- **Identify stragglers** — devices stuck in DOWNLOADING, STAGED, or FAILED states

### 5. Delta/Differential Firmware Updates

For bandwidth-constrained tactical networks, full firmware image transfers are expensive. PEAT supports differential firmware updates using binary diff algorithms:

```
Full PX4 firmware image:     2.1 MB
Binary diff (v1.14.2→1.14.3): 180 KB  (91% reduction)
```

**Approach:**

```rust
/// Generate delta patch between firmware versions
///
/// Uses bsdiff/bspatch algorithm for binary deltas.
/// The patch is stored as a regular blob in BlobStore
/// with FirmwareFormat::DELTA_PATCH.
pub struct FirmwareDelta {
    pub base_version: String,
    pub base_hash: String,
    pub target_version: String,
    pub target_hash: String,
    pub patch_blob_hash: String,
    pub patch_size_bytes: u64,
    pub full_image_size_bytes: u64,
}
```

The firmware manifest declares whether a delta is available:

```json
{
  "firmware_id": "px4-autopilot",
  "version": "1.14.3",
  "full_image": {
    "blob_hash": "sha256:abc123...",
    "size_bytes": 2202009
  },
  "deltas": [
    {
      "base_version": "1.14.2",
      "base_hash": "sha256:def456...",
      "patch_blob_hash": "sha256:789abc...",
      "patch_size_bytes": 184320
    },
    {
      "base_version": "1.14.1",
      "base_hash": "sha256:ghi789...",
      "patch_blob_hash": "sha256:jkl012...",
      "patch_size_bytes": 312400
    }
  ]
}
```

The OTA agent checks its current version, selects the appropriate delta if available, and falls back to full image if no delta exists for its current version.

### 6. Multi-Artifact Coordinated Updates

For platforms that carry multiple firmware images and models, PEAT supports coordinated deployment:

```protobuf
message PlatformUpdateBundle {
  string bundle_id = 1;
  string platform_type = 2;          // e.g., "recon-drone-mk3"
  string bundle_version = 3;         // e.g., "2026-Q1-release"

  // All artifacts to deploy as a unit
  repeated BundleArtifact artifacts = 4;

  // Ordering constraints
  repeated DeploymentStep steps = 5;

  // Bundle-level verification
  BundleVerification verification = 6;
}

message BundleArtifact {
  string artifact_id = 1;
  oneof artifact {
    FirmwareManifest firmware = 2;
    ModelManifest model = 3;
    ZarfPackageAvailable container = 4;
    ConfigPackage config = 5;
  }
}

message DeploymentStep {
  uint32 order = 1;                  // Execution order
  repeated string artifact_ids = 2;  // Artifacts in this step (parallel)
  string precondition = 3;           // Must be true before step executes
  string postcondition = 4;          // Must be true after step completes
}

message BundleVerification {
  // System-level health check after all artifacts deployed
  string health_check_command = 1;
  uint32 verification_timeout_sec = 2;
  RollbackScope rollback_on_failure = 3;
}

enum RollbackScope {
  ROLLBACK_ALL = 0;      // Revert entire bundle
  ROLLBACK_FAILED = 1;   // Revert only the failed artifact
  NO_ROLLBACK = 2;       // Leave as-is, alert operator
}
```

**Example: Drone platform update bundle:**

```json
{
  "bundle_id": "recon-drone-mk3-2026q1",
  "platform_type": "recon-drone-mk3",
  "bundle_version": "2026-Q1",
  "artifacts": [
    {
      "artifact_id": "autopilot-fw",
      "firmware": {
        "firmware_id": "px4-autopilot",
        "version": "1.14.3"
      }
    },
    {
      "artifact_id": "perception-model",
      "model": {
        "model_id": "target_recognition_yolov8",
        "version": "4.2.1"
      }
    },
    {
      "artifact_id": "radio-fw",
      "firmware": {
        "firmware_id": "sdr-baseband",
        "version": "3.7.0"
      }
    },
    {
      "artifact_id": "mission-config",
      "config": {
        "config_id": "roe-2026q1",
        "version": "1.0.0"
      }
    }
  ],
  "steps": [
    {
      "order": 1,
      "artifact_ids": ["radio-fw"],
      "postcondition": "radio_link_active"
    },
    {
      "order": 2,
      "artifact_ids": ["autopilot-fw", "perception-model"],
      "precondition": "radio_link_active",
      "postcondition": "autopilot_healthy AND model_loaded"
    },
    {
      "order": 3,
      "artifact_ids": ["mission-config"],
      "precondition": "autopilot_healthy",
      "postcondition": "mission_config_applied"
    }
  ],
  "verification": {
    "health_check_command": "full_system_bist",
    "verification_timeout_sec": 120,
    "rollback_on_failure": "ROLLBACK_ALL"
  }
}
```

### 7. Fleet Firmware Management

Firmware state for all devices aggregates through PEAT's hierarchy:

```
┌──────────────────────────────────────────────────────────────────────┐
│                    Battalion (Fleet View)                              │
│                                                                       │
│  Firmware: px4-autopilot                                             │
│  ┌────────────────────────────────────────────────────────────┐      │
│  │ v1.14.3 ████████████████████████████████░░░░░  85% (170)  │      │
│  │ v1.14.2 ██████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  12% (24)   │      │
│  │ v1.14.1 █░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   3% (6)    │      │
│  └────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  Convergence to v1.14.3:                                             │
│  ├─ Company Alpha: 95% (38/40 drones)                                │
│  │   ├─ Platoon 1: 100% (10/10) ✓                                   │
│  │   ├─ Platoon 2: 90% (9/10) — 1 downloading                       │
│  │   ├─ Platoon 3: 100% (10/10) ✓                                   │
│  │   └─ Platoon 4: 90% (9/10) — 1 failed (battery constraint)       │
│  ├─ Company Bravo: 82% (41/50 drones)                                │
│  │   └─ ...                                                          │
│  └─ Stragglers:                                                      │
│      ├─ drone-047: FAILED — battery below 80%                        │
│      ├─ drone-112: DOWNLOADING — 67% complete (low bandwidth link)   │
│      └─ drone-189: STAGED — awaiting maintenance window              │
│                                                                       │
│  Blockers: 2 safety constraint failures, 1 connectivity issue        │
└──────────────────────────────────────────────────────────────────────┘
```

This is implemented using the same convergence tracking pattern from ADR-013:

```rust
/// Fleet firmware status — aggregated through PEAT hierarchy
pub struct FleetFirmwareStatus {
    pub firmware_id: String,
    pub target_version: String,
    pub total_targets: usize,

    /// Version distribution across fleet
    pub version_distribution: HashMap<String, usize>,

    /// Convergence state
    pub converged: usize,
    pub downloading: usize,
    pub staged: usize,
    pub activating: usize,
    pub failed: usize,

    /// Blockers preventing convergence
    pub blockers: Vec<ConvergenceBlocker>,

    /// Per-formation breakdown
    pub formation_status: HashMap<String, FormationFirmwareStatus>,
}
```

### 8. Transport Considerations for Firmware Targets

Firmware targets may connect to the PEAT mesh via different transports:

| Transport | Bandwidth | Use Case | Blob Transfer |
|-----------|-----------|----------|---------------|
| QUIC (via companion CPU) | High | Drone companion computer, vehicle gateway | Full-speed blob transfer |
| BLE (direct) | Low (≈100KB/s) | Close-range maintenance, sensor payloads | Small firmware only |
| UDP/PEAT-Lite | Medium | WiFi-connected embedded Linux | Chunked blob transfer |
| Serial bridge | Low | MCUs behind a gateway | Gateway proxies blob download |

For devices that can't directly participate in PEAT mesh (bare-metal MCUs with no network stack), a **companion/gateway pattern** is used:

```
┌─────────────┐     Serial/SPI     ┌──────────────┐     QUIC     ┌──────────┐
│  MCU with   │◄──────────────────▶│  Companion   │◄────────────▶│  PEAT    │
│  firmware   │   flash commands   │  processor   │  mesh peer   │  Mesh    │
│  (target)   │                    │  (OTA agent) │              │          │
└─────────────┘                    └──────────────┘              └──────────┘
```

The OTA agent runs on the companion processor and manages the MCU's firmware via standard flashing interfaces (SWD, JTAG, UART bootloader, etc.).

### 9. Safety and Security

Firmware updates are safety-critical. Additional safeguards beyond standard PEAT security (ADR-006):

**Pre-flight checks before activation:**
- Battery level above threshold (prevents bricking during flash)
- Platform in safe state (not in flight, not in motion, not actively engaged)
- Stable power source confirmed
- Sufficient staging storage available
- Hardware compatibility verified against manifest
- All dependency versions satisfied

**Cryptographic verification chain:**
- Firmware image signed by build system
- Signature countersigned by deployment authority
- OTA agent verifies both signatures before staging
- Bootloader verifies image signature before booting (hardware root of trust)

**Rollback guarantee:**
- A/B partition scheme ensures one known-good slot at all times
- Boot verification timer — if health checks don't pass within timeout, automatic rollback
- Golden image — if both A and B slots fail, fall back to factory firmware in protected storage
- All state transitions logged as PEAT events for audit trail

## Implementation: PEAT-Lite ESP32 OTA (Phase 0)

The first concrete implementation of firmware OTA in the PEAT ecosystem targets PEAT-Lite on ESP32 (M5Stack Core2). This validates the core protocol and A/B partition update lifecycle end-to-end over the existing UDP gossip transport.

### Wire Protocol

OTA messages extend the existing PEAT-Lite packet format (ADR-035). All messages share the standard 16-byte header:

```
┌──────────┬─────────┬──────────┬──────────┬──────────┬──────────────┐
│  MAGIC   │ Version │   Type   │  Flags   │  NodeID  │   SeqNum     │
│ "PEAT"   │  0x01   │  0x10-16 │  2 bytes │  4 bytes │   4 bytes    │
└──────────┴─────────┴──────────┴──────────┴──────────┴──────────────┘
```

Seven OTA message types are defined in `peat-lite-protocol`:

| Type | Code | Direction | Payload |
|------|------|-----------|---------|
| `OtaOffer` | `0x10` | Full → Lite | version[16] + size[4] + chunks[2] + chunk_size[2] + sha256[32] + session_id[2] + flags[2] + signature[64]? |
| `OtaAccept` | `0x11` | Lite → Full | session_id[2] + resume_chunk[2] |
| `OtaData` | `0x12` | Full → Lite | session_id[2] + chunk_num[2] + chunk_len[2] + data[≤448] |
| `OtaAck` | `0x13` | Lite → Full | session_id[2] + acked_chunk[2] |
| `OtaComplete` | `0x14` | Full → Lite | session_id[2] |
| `OtaResult` | `0x15` | Lite → Full | session_id[2] + result_code[1] + reserved[1] |
| `OtaAbort` | `0x16` | Either | session_id[2] + reason[1] + reserved[1] |

The offer supports two formats: legacy unsigned (76 bytes) and v2 signed (140 bytes, with Ed25519 signature over the SHA256 digest). The `OTA_FLAG_SIGNED` (0x0001) flag distinguishes them.

Result codes: `SUCCESS` (0x00), `HASH_MISMATCH` (0x01), `FLASH_ERROR` (0x02), `INVALID_OFFER` (0x03), `SIGNATURE_INVALID` (0x04), `SIGNATURE_REQUIRED` (0x05).

Abort reasons: `TIMEOUT` (0x01), `SESSION_MISMATCH` (0x02), `USER_CANCEL` (0x03), `TOO_MANY_RETRIES` (0x04).

### Transfer Protocol

Stop-and-wait: the sender transmits one chunk at a time and waits for an `OtaAck` before sending the next. This keeps the receiver simple (no reordering buffer, no window management) and naturally adapts to the link speed.

```
Sender (Full node)                    Receiver (Lite node)
    │                                       │
    │──── OtaOffer ────────────────────────▶│ Parse offer, verify signature,
    │                                       │ erase target partition
    │◀──── OtaAccept ──────────────────────│
    │                                       │
    │──── OtaData (chunk 0) ──────────────▶│ Write to flash, update SHA256
    │◀──── OtaAck (chunk 0) ──────────────│
    │                                       │
    │──── OtaData (chunk 1) ──────────────▶│
    │◀──── OtaAck (chunk 1) ──────────────│
    │          ...                          │
    │──── OtaData (chunk N-1) ────────────▶│
    │◀──── OtaAck (chunk N-1) ────────────│
    │                                       │
    │──── OtaComplete ────────────────────▶│ Verify SHA256, write validation
    │                                       │ record, update otadata
    │◀──── OtaResult (SUCCESS) ───────────│
    │                                       │ Reboot into new firmware
```

### Partition Layout

The ESP32 flash uses an A/B OTA partition scheme (`partitions_ota.csv`):

| Name | Type | Offset | Size |
|------|------|--------|------|
| `nvs` | NVS | 0x9000 | 20 KB |
| `otadata` | OTA data | 0xE000 | 8 KB |
| `ota_0` | App (OTA 0) | 0x10000 | 3 MB |
| `ota_1` | App (OTA 1) | 0x310000 | 3 MB |

The receiver reads `otadata` to determine which partition is currently active and writes the new firmware to the other. After SHA256 verification, it updates `otadata` with a new sequence number pointing to the newly written partition.

### Boot Validation and Rollback

A validation record at `VALIDATION_OFFSET` (0x9000, first NVS sector) tracks pending OTA updates:

```
┌──────────┬───────┬─────────┬──────┬──────────┬──────────┬──────────┬──────────┬─────┬─────┐
│  Magic   │ State │ Attempts│ Max  │ Prev Part│ Prev Seq │ New Part │ New Seq  │ CRC │ Pad │
│ "OTVA"   │  1B   │   1B    │  1B  │   4B     │   4B     │   4B     │   4B     │ 4B  │ 4B  │
└──────────┴───────┴─────────┴──────┴──────────┴──────────┴──────────┴──────────┴─────┴─────┘
```

**Critical ordering:** The validation record is written BEFORE `otadata` is updated. This ensures that if power is lost after writing `otadata` but before the new firmware validates, the next boot can detect the pending update and roll back.

Lifecycle:
1. **OTA complete** → Write validation record (`state=PENDING`, `boot_attempts=0`, `max_attempts=3`)
2. **OTA complete** → Update `otadata` to boot new partition
3. **Reboot** → `boot_validation_check()` runs early in startup, increments `boot_attempts`
4. **Successful network activity** → `ota_mark_validated()` clears the record (`state=IDLE`)
5. **If boot_attempts > max_attempts** → `rollback_to_previous()` rewrites `otadata` and reboots

### Ed25519 Signature Verification

The build system supports compile-time embedding of an Ed25519 public key:

```bash
# Build with signature verification enabled
OTA_SIGNING_PUBKEY="<64 hex chars>" cargo +esp build ...
```

`build.rs` generates a `const OTA_SIGNING_PUBKEY: Option<[u8; 32]>` that is `include!`'d into `ota.rs`. When present, the receiver requires a valid Ed25519 signature over the firmware's SHA256 digest in the offer. When absent (dev builds), signature verification is skipped.

### Receiver Implementation

The OTA receiver (`peat-lite/src/ota.rs`) is a state machine:

```
Idle → WaitingForData → Receiving → Verifying → ReadyToReboot
                ↓            ↓           ↓
              Failed       Failed      Failed
```

Key design choices:
- **Streaming SHA256**: hash is updated incrementally as each chunk arrives (no need to re-read from flash)
- **No heap allocation for firmware data**: chunks are written directly to flash from the UDP receive buffer
- **Duplicate chunk tolerance**: re-ACKs already-received chunks without error (handles retransmissions)
- **Session ID prevents cross-talk**: each OTA session has a unique ID; stale packets from a previous session are rejected

### Test Sender

`scripts/ota-sender.py` is a Python test tool that implements the sender side:

```bash
# Auto-discover device and push firmware
python3 scripts/ota-sender.py ota_firmware.bin

# Target a specific device
python3 scripts/ota-sender.py ota_firmware.bin --target 192.168.1.116 --version "0.1.0"
```

Features:
- Device discovery via heartbeat listening
- Stop-and-wait with configurable retries and timeout
- Progress bar with transfer rate and ETA
- Handles all OTA response types (Accept, Ack, Result, Abort)

### Verified Test Results

Tested 2026-02-22 on M5Stack Core2 (ESP32, rev v3.1) over WiFi:

```
Firmware:   580,960 bytes (peat-lite-wifi with m5stack-core2-wifi,ota features)
Chunks:     1,297 × 448 bytes
Transfer:   46.5 seconds at 12.2 KB/s
SHA256:     Verified on-device (streaming hash)
Signature:  Disabled (dev build, no OTA_SIGNING_PUBKEY)
Partition:  ota_0 → ota_1 (A/B switch)
Rollback:   Validation record written, boot attempt 1/3, then validated
Result:     SUCCESS — device rebooted into new firmware and rejoined mesh
```

Device-side log (abridged):
```
[OTA] Offer received from 4F544153
[OTA] Erasing 0x00310000..0x0039E000
[OTA] Accepted offer: session=35065, size=580960, chunks=1297, signed=false
[OTA] Progress: 5% (65/1297) ... 100% (1297/1297)
[OTA] otadata updated: seq=4294967295, partition=ota_1
[OTA] Update verified and committed! Ready to reboot.
[OTA] SUCCESS! Rebooting in 2 seconds...
[OTA] Boot validation: attempt 1/3
Got IP: 192.168.1.116
[OTA] Firmware validated! Clearing pending record.
```

### Crate Structure

| Crate | OTA Contribution |
|-------|-----------------|
| `peat-lite-protocol` | Wire constants (`OTA_CHUNK_DATA_SIZE`, result/abort codes, `OTA_FLAG_SIGNED`) and `MessageType` variants (0x10-0x16) |
| `peat-lite` | `ota` module (receiver state machine, flash operations, validation record, signature verification); `build.rs` (pubkey codegen); `wifi_main.rs` (OTA message dispatch) |
| `scripts/ota-sender.py` | Test sender tool (Python, no Rust deps) |

### Build Commands

```bash
# Check compiles (fast feedback)
make check-ota SSID=MyNet WIFI_PWD=secret

# Build release firmware with OTA
make build SSID=MyNet WIFI_PWD=secret

# Flash with OTA partition table and monitor
make flash SSID=MyNet WIFI_PWD=secret PORT=/dev/ttyACM1

# Generate OTA app image for over-the-air delivery
make build-image SSID=MyNet WIFI_PWD=secret

# Send OTA update to a running device
python3 scripts/ota-sender.py peat-lite/ota_firmware.bin --target <device-ip>
```

## Implementation Phases

### Phase 1: Foundation (Months 1-2)

- Define `FirmwareManifest` and `FirmwareCapability` protobuf schemas in peat-schema
- Add `FirmwareFormat` as artifact type to existing `DeploymentDirective`
- Implement hardware compatibility matching logic
- Add firmware status tracking to convergence monitoring

**Success criteria:**
- Can publish firmware manifest to PEAT mesh
- Devices advertise hardware capabilities
- Compatibility matching prevents wrong firmware from being deployed

### Phase 2: OTA Agent Reference Implementation (Months 2-4)

- Implement `FirmwareOtaAgent` for embedded Linux (A/B partition)
- Implement `PartitionManager` for common schemes (RAUC-style, swupdate-style)
- Full OTA lifecycle: download → stage → activate → verify → commit/rollback
- Safety constraint checking

**Success criteria:**
- Can update firmware on Raspberry Pi via PEAT mesh
- Automatic rollback on boot verification failure
- Safety constraints enforced

### Phase 3: Delta Updates and Fleet Management (Months 4-6)

- Binary diff generation (bsdiff) and application (bspatch)
- Fleet firmware dashboard aggregation through hierarchy
- Multi-artifact bundle coordination (`PlatformUpdateBundle`)

**Success criteria:**
- Delta updates reduce bandwidth by >80% for incremental versions
- Fleet-wide firmware version visibility at each echelon
- Coordinated multi-artifact updates on drone platform

### Phase 4: Extended Platform Support (Months 6-9)

- STM32 bootloader integration (via companion processor pattern)
- FPGA bitstream update support
- BLE-based firmware delivery for close-range maintenance
- Integration with existing OTA tools (SWUpdate, RAUC) as backends

**Success criteria:**
- Can update STM32-based autopilot firmware via PEAT mesh
- BLE-based firmware update for sensor payloads
- SWUpdate/RAUC used as partition manager backend

## Consequences

### Positive

**Unified Delivery Platform:**
- Single coordination layer for firmware, models, containers, and config
- Operators get one view of "is this platform fully updated?"
- Same security/provenance model across all artifact types
- Same convergence tracking regardless of artifact type

**DIL-Native:**
- Mesh distribution works over intermittent tactical links
- Hierarchical caching reduces load on upstream links
- Store-and-forward ensures updates reach all nodes eventually
- Peer-assisted distribution when direct parent is unavailable

**Extends UDS to Every Target:**
- Zarf handles K8s workloads
- Firmware OTA handles embedded platforms
- AI model delivery handles GPU/NPU nodes
- PEAT-Lite handles sensor mesh
- All coordinated through a single protocol

**Safety:**
- Hardware compatibility verification prevents wrong firmware deployment
- Automatic rollback on boot verification failure
- Safety constraint enforcement (battery, power, operational state)
- Full audit trail of all firmware operations

### Negative

**Complexity:**
- Another adapter type to maintain alongside Zarf, model delivery, PEAT-Lite
- Platform-specific PartitionManager implementations needed for each hardware target
- Safety-critical code requires higher testing standards
- Multi-artifact coordination adds state machine complexity

**Hardware Diversity:**
- Each board type needs hardware compatibility entries
- Bootloader variations require testing across platforms
- Flash memory characteristics vary (write cycles, erase block sizes)
- Companion processor pattern adds deployment complexity

### Mitigations

**Complexity:**
- Start with embedded Linux (Raspberry Pi) as reference platform
- Use existing tools (RAUC, SWUpdate) as PartitionManager backends
- Shared convergence tracking and fleet management across all artifact types
- PlatformUpdateBundle is optional — single-artifact updates work standalone

**Hardware Diversity:**
- Maintain compatibility database as PEAT collection (auto-synced)
- Community-contributed PartitionManager implementations
- Companion processor pattern abstracts MCU differences behind serial/SPI interface

## Integration Points

### With ADR-013 (Distributed Software Ops)
- Extends capability-focused convergence to firmware state
- Same differential propagation principles (content-addressed, chunked)
- Same hierarchical distribution and peer-assisted spread
- Firmware state feeds into operational capability assessment

### With ADR-022 (Edge MLOps)
- Shared `PlatformUpdateBundle` for coordinated firmware + model updates
- Same convergence tracking dashboard
- Model delivery depends on firmware compatibility (e.g., GPU driver version)

### With ADR-025 (Blob Transfer)
- Firmware images stored and transferred via `BlobStore`
- Content-addressed deduplication across firmware versions
- Resumable transfers over unreliable links
- Progress tracking for large firmware images

### With ADR-035 (PEAT-Lite)
- PEAT-Lite nodes can receive firmware updates via gossip/BLE
- ESP32 OTA is a special case of firmware update
- PEAT-Lite capability flags indicate OTA support

### With ADR-045 (Zarf/UDS Integration)
- Firmware OTA extends UDS delivery to non-K8s targets
- Same metadata backplane (PEAT) coordinates both Zarf and firmware deployments
- Fleet view spans both K8s workloads and firmware targets

### With ADR-006 (Security)
- Firmware signature verification uses PEAT PKI
- Deployment authorization through PEAT RBAC
- Hardware root of trust for bootloader verification
- Audit trail for all firmware operations

## References

- ADR-013: Distributed Software and AI Operations
- ADR-022: Edge MLOps Architecture
- ADR-025: Blob Transfer Abstraction Layer
- ADR-026: Reference Implementation - Software Orchestration
- ADR-035: PEAT-Lite Embedded Sensor Nodes
- ADR-045: Zarf/UDS Integration
- [SWUpdate](https://sbabic.github.io/swupdate/) — Software Update for Embedded Linux
- [RAUC](https://rauc.io/) — Safe and Secure OTA Updates for Embedded Linux
- [Mender](https://mender.io/) — OTA Software Updates for IoT
- [The Update Framework (TUF)](https://theupdateframework.io/) — Securing Software Updates
- [bsdiff/bspatch](https://www.daemonology.net/bsdiff/) — Binary Diff/Patch
- DO-178C — Software Considerations in Airborne Systems and Equipment Certification
- IEC 61508 — Functional Safety of Electrical/Electronic/Programmable Electronic Safety-related Systems

---

**This ADR extends PEAT's distribution capabilities to firmware targets, enabling unified software delivery across the full spectrum from cloud/K8s through embedded platforms, all coordinated through a single mesh protocol.**
