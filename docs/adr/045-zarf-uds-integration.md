# ADR-045: Zarf/UDS Integration for Tactical Software Delivery

**Status**: Proposed
**Date**: 2025-01-09
**Authors**: Kit Plummer, Codex
**Relates to**: ADR-013 (Distributed Software Ops), ADR-022 (Edge MLOps), ADR-025 (Blob Transfer)

## Context

### The Software Supply Chain Gap

Modern tactical environments require software delivery to edge platforms that are:
- **Intermittently connected** - Hours to days without network access
- **Hierarchically organized** - Cloud → FOB → Vehicle → Dismount
- **Resource constrained** - Limited bandwidth, storage, compute
- **Security sensitive** - Air-gapped, SBOM requirements, provenance verification

Existing solutions address parts of this problem:

| Tool | Strength | Gap |
|------|----------|-----|
| **Zarf** | Air-gap packaging, OCI distribution | Single cluster focus, no mesh coordination |
| **Kubernetes** | Container orchestration | Assumes connected control plane |
| **GitOps (Flux/Argo)** | Declarative deployment | Requires Git connectivity |
| **HIVE** | Mesh sync, hierarchical coordination | No container/K8s deployment |

### Defense Unicorns Ecosystem

[Defense Unicorns](https://github.com/defenseunicorns) provides open-source tools for secure software delivery:

- **Zarf**: Declarative air-gap package manager for Kubernetes
  - Bundles container images, Helm charts, and manifests into portable tarballs
  - Publishes/consumes packages as OCI artifacts
  - Includes built-in registry and Git server for disconnected operation
  - Generates SBOMs automatically

- **UDS Core**: Secure runtime platform (Istio, Keycloak, monitoring, Pepr)
  - Deployed as a Zarf package
  - Provides baseline security posture for mission workloads

- **Pepr**: TypeScript K8s middleware for policy enforcement and automation

### Integration Opportunity

HIVE + Zarf/UDS creates a complete tactical software delivery stack:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     Cloud / Enterprise                                   │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                  │
│  │ Zarf Build  │───▶│ OCI Registry│───▶│ HIVE Gateway│                  │
│  │  Pipeline   │    │  (packages) │    │  (metadata) │                  │
│  └─────────────┘    └─────────────┘    └──────┬──────┘                  │
└─────────────────────────────────────────────────┼────────────────────────┘
                                                  │ HIVE Sync
                    ┌─────────────────────────────┼─────────────────────────┐
                    │           FOB / Base        ▼                         │
                    │  ┌─────────────┐    ┌─────────────┐                   │
                    │  │ Zarf Mirror │◀───│ HIVE Node   │                   │
                    │  │  Registry   │    │ (metadata)  │                   │
                    │  └──────┬──────┘    └──────┬──────┘                   │
                    └─────────┼──────────────────┼─────────────────────────┘
                              │                  │ HIVE Sync
              ┌───────────────┼──────────────────┼───────────────┐
              │     Vehicle   ▼                  ▼               │
              │  ┌─────────────┐    ┌─────────────┐              │
              │  │ Zarf Deploy │◀───│ HIVE Node   │              │
              │  │   (K3s)     │    │ (commands)  │              │
              │  └─────────────┘    └─────────────┘              │
              └──────────────────────────────────────────────────┘
```

**HIVE provides:**
- Package metadata propagation across the mesh
- Deployment intent/command distribution
- Status aggregation up the hierarchy
- Store-and-forward for disconnected nodes
- Targeted delivery to specific platforms

**Zarf provides:**
- Actual package transfer (OCI pull)
- K8s deployment execution
- SBOM and provenance
- Air-gap operation

## Decision

### 1. HIVE as Metadata Backplane

HIVE synchronizes **metadata about packages and deployments**, not the packages themselves:

```protobuf
// Package availability advertisement
message ZarfPackageAvailable {
  string package_name = 1;       // e.g., "uds-core"
  string version = 2;            // e.g., "0.31.1"
  string oci_reference = 3;      // e.g., "oci://registry.local/uds-core:0.31.1"
  string sha256 = 4;             // Package digest
  uint64 size_bytes = 5;         // For bandwidth planning
  repeated string architectures = 6;  // ["amd64", "arm64"]
  string sbom_reference = 7;     // SBOM location
  google.protobuf.Timestamp available_at = 8;
  string source_node_id = 9;     // Which node has this package
}

// Deployment intent (command)
message DeploymentIntent {
  string intent_id = 1;
  string package_name = 2;
  string target_version = 3;
  repeated string target_nodes = 4;  // Empty = all capable nodes
  string target_selector = 5;        // Alternative: label selector
  DeploymentStrategy strategy = 6;
  google.protobuf.Timestamp deadline = 7;
  string issuer_node_id = 8;
  bytes signature = 9;               // Signed by authorized issuer
}

enum DeploymentStrategy {
  ROLLING = 0;      // Deploy as connectivity allows
  IMMEDIATE = 1;    // Deploy ASAP, report failures
  CANARY = 2;       // Deploy to subset first
  SCHEDULED = 3;    // Deploy at specific time
}

// Deployment status (aggregated up hierarchy)
message DeploymentStatus {
  string intent_id = 1;
  string node_id = 2;
  DeploymentState state = 3;
  string current_version = 4;
  string target_version = 5;
  string error_message = 6;
  google.protobuf.Timestamp last_updated = 7;
}

enum DeploymentState {
  PENDING = 0;      // Intent received, not started
  DOWNLOADING = 1;  // Pulling package
  DEPLOYING = 2;    // Zarf deploy in progress
  DEPLOYED = 3;     // Successfully deployed
  FAILED = 4;       // Deployment failed
  ROLLED_BACK = 5;  // Rolled back to previous
}
```

### 2. Collections Structure

```
zarf_packages/           # Available packages (replicated)
  {package_name}_{version}  → ZarfPackageAvailable

deployment_intents/      # Deployment commands (targeted delivery)
  {intent_id}            → DeploymentIntent

deployment_status/       # Status per node (aggregated)
  {intent_id}_{node_id}  → DeploymentStatus

package_mirrors/         # Which registries have which packages
  {node_id}_{package}    → MirrorStatus
```

### 3. Deployment Flow

```
1. BUILD (Cloud)
   ├─ CI/CD builds Zarf package
   ├─ Pushes to OCI registry
   └─ Publishes ZarfPackageAvailable to HIVE

2. PROPAGATE (HIVE Sync)
   ├─ Package metadata syncs through hierarchy
   ├─ Each node learns what packages exist
   └─ Mirrors can pre-pull packages

3. COMMAND (Operator)
   ├─ Operator creates DeploymentIntent
   ├─ Targets specific nodes or selectors
   └─ Intent syncs to target nodes

4. EXECUTE (Edge Node)
   ├─ Node receives DeploymentIntent
   ├─ Pulls package from nearest mirror
   ├─ Executes: zarf package deploy
   └─ Reports DeploymentStatus

5. AGGREGATE (HIVE Hierarchy)
   ├─ Status documents sync upward
   ├─ Leaders aggregate subordinate status
   └─ Operator sees convergence progress
```

### 4. Targeted Delivery Dependency

Effective deployment coordination requires **targeted message delivery** - the ability to address documents to specific nodes rather than broadcasting to entire mesh.

**Requirement**: ADR-046 (Targeted Message Delivery) should define:
- Document addressing by node ID or selector
- Filtering at sync layer (don't replicate everywhere)
- Delivery confirmation semantics

For this ADR, we assume targeted delivery exists and specify the interface:

```rust
// Write deployment intent to specific nodes
store.write_targeted(
    "deployment_intents",
    &intent,
    TargetSpec::Nodes(vec!["node-1", "node-2", "node-3"]),
).await?;

// Or by selector
store.write_targeted(
    "deployment_intents",
    &intent,
    TargetSpec::Selector("platform=vehicle,region=grid7"),
).await?;
```

### 5. Integration Points

#### HIVE Side

```rust
// New crate: hive-zarf (or module in hive-protocol)

pub struct ZarfIntegration {
    store: Arc<dyn DocumentStore>,
    zarf_binary: PathBuf,
}

impl ZarfIntegration {
    /// Watch for deployment intents targeting this node
    pub async fn watch_intents(&self) -> impl Stream<Item = DeploymentIntent>;

    /// Execute a Zarf deployment
    pub async fn deploy(&self, intent: &DeploymentIntent) -> Result<DeploymentStatus>;

    /// Advertise locally available packages
    pub async fn advertise_packages(&self) -> Result<()>;

    /// Report deployment status
    pub async fn report_status(&self, status: DeploymentStatus) -> Result<()>;
}
```

#### Zarf Side (Pepr Controller)

Alternatively, integrate via Kubernetes:

```typescript
// Pepr capability that watches HIVE and triggers Zarf
When(HiveDeploymentIntent)
  .IsCreated()
  .Then(async (intent) => {
    // Pull package from nearest HIVE-advertised mirror
    const mirror = await findNearestMirror(intent.packageName);

    // Execute Zarf deployment
    await exec(`zarf package deploy ${mirror}/${intent.packageName}`);

    // Report status back to HIVE
    await reportDeploymentStatus(intent.id, "DEPLOYED");
  });
```

### 6. Security Considerations

- **Deployment intents MUST be signed** by authorized issuer (uses HIVE security layer)
- **Package verification** via Zarf's built-in signature/SBOM verification
- **RBAC**: Only authorized nodes can issue deployment intents
- **Audit trail**: All intents and status changes recorded in CRDT history

## Consequences

### Positive

- **Complete stack**: HIVE + Zarf covers cloud-to-edge software delivery
- **Disconnected operation**: Both tools designed for air-gap/intermittent connectivity
- **Open source**: Full stack is FOSS, no vendor lock-in
- **Separation of concerns**: HIVE does coordination, Zarf does deployment
- **Existing ecosystem**: Leverage UDS Core, Pepr, existing Zarf packages

### Negative

- **Additional dependency**: Requires Zarf binary on edge nodes
- **Kubernetes assumption**: Zarf targets K8s (though K3s works on small edge)
- **Complexity**: Two systems to operate and debug
- **Targeted delivery**: Requires ADR-046 implementation first

### Neutral

- **Not replacing Zarf features**: HIVE doesn't do OCI, Helm, or K8s deployment
- **Not replacing HIVE features**: Zarf doesn't do mesh sync or CRDT

## Alternatives Considered

### 1. HIVE-Native Package Distribution

Build package distribution into HIVE using blob transfer (ADR-025).

**Rejected**: Reinventing Zarf's capabilities. Zarf already handles air-gap packaging well.

### 2. GitOps with Flux/Argo

Use GitOps for deployment coordination.

**Rejected**: Requires Git connectivity. Doesn't handle intermittent mesh.

### 3. Direct Zarf Push

Use Zarf's OCI push capabilities directly without HIVE.

**Rejected**: No mesh coordination, no status aggregation, no store-and-forward.

## Implementation Plan

### Phase 1: Schema & Collections
- Define Protobuf messages for package/intent/status
- Create collections in hive-schema
- Basic CRUD operations

### Phase 2: Targeted Delivery (ADR-046)
- Implement node-addressed document delivery
- Selector-based targeting
- Delivery confirmation

### Phase 3: HIVE-Zarf Bridge
- Watch for intents, execute Zarf
- Package advertisement
- Status reporting

### Phase 4: Pepr Integration (Optional)
- Kubernetes-native integration via Pepr
- CRD-based interface

## References

- [Zarf Documentation](https://docs.zarf.dev/)
- [UDS Core](https://github.com/defenseunicorns/uds-core)
- [Pepr](https://github.com/defenseunicorns/pepr)
- [Defense Unicorns GitHub](https://github.com/defenseunicorns)
- ADR-013: Distributed Software and AI Operations
- ADR-022: Edge MLOps Architecture
- ADR-025: Blob Transfer Protocol
