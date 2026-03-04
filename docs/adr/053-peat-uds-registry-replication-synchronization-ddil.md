# ADR-053: Peat UDS Registry Replication-to-Synchronization for DDIL Networks

**Status**: Proposed
**Date**: 2026-03-04
**Authors**: Kit Plummer, Codex
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)
**Relates To**: ADR-013 (Distributed Software Ops), ADR-019 (QoS and Data Prioritization), ADR-025 (Blob Transfer Abstraction), ADR-032 (Pluggable Transport), ADR-045 (Zarf/UDS Integration), ADR-052 (OCI Distribution Registry Replication)

---

## Executive Summary

This ADR defines Peat support for UDS/OCI registry propagation as a **continuous synchronization system**, not just periodic replication jobs. The approach is designed for distributed, hierarchical, and partially connected networks operating under DDIL constraints.

Core decision:

1. Keep `distribution/distribution` and OCI Distribution API semantics as the registry data plane.
2. Treat Peat as synchronization control plane for intent, topology-aware scheduling, and convergence tracking.
3. Model UDS artifact movement as `replication -> checkpointed transfer -> convergence synchronization`.

---

## Context

ADR-045 established Zarf/UDS integration and separation of concerns between metadata/control and OCI artifact transfer. ADR-052 introduced OCI-scale replication patterns.

What remains: a UDS-specific operational model where distributed registries in enterprise, regional, FOB, and edge positions converge toward a common desired state despite DDIL behavior:

1. Links are degraded/intermittent and may be denied for long windows.
2. Parent/child registry paths can change dynamically due to mission movement.
3. Nodes must continue forward progress from checkpoints after outages.
4. Synchronization status must be observable by topology segment and QoS class.

---

## Decision Drivers

1. OCI compatibility with standard clients and registries.
2. Deterministic digest-based integrity for package content and metadata.
3. Bounded bandwidth usage with policy-based prioritization.
4. Eventual convergence across many registry tiers without central bottlenecks.
5. Operational simplicity for UDS package lifecycle in disconnected theaters.

---

## Decision

### 1. Replication Becomes Synchronization Lifecycle

Peat will treat replication as only one stage in a larger synchronization lifecycle:

1. Resolve desired UDS package/artifact state to immutable digests.
2. Compute per-target/per-tier digest deltas.
3. Execute resilient transfer with resume checkpoints.
4. Verify manifests/blobs/referrers at destination.
5. Mark target convergence state and continuously reconcile drift.

This changes runtime behavior from one-shot copy to continuous convergence.

### 2. Topology-Aware Distributed Registry Graph

Peat will model registries as a directed parent-preference graph:

- Core/enterprise registry tier
- Regional/FOB relay registry tier
- Mobile/edge registry tier

Per-target policy includes:

- preferred parent order,
- alternate/failover parent,
- max upstream fan-out,
- rollout wave assignment,
- byte/time budgets.

Scheduler decisions must favor nearest converged parent before escalating to upper tiers.

### 3. DDIL Policy Classes

Synchronization policy is expressed as DDIL-aware classes:

- `mission-critical`: strict priority, aggressive retry, reserved bandwidth.
- `mission-support`: bounded concurrency, normal retry, adaptive pacing.
- `background`: opportunistic transfer, suspendable under link pressure.

QoS mapping follows ADR-019 semantics and must be enforceable per link and per target group.

### 4. Convergence Includes Referrers

Synchronization completeness includes required referrers (signatures, SBOMs, attestations) when policy requires them. A target is converged only when both subject manifests and required referrers are present and validated.

### 5. Checkpointed Resume Is Mandatory

All long-running blob transfers must persist checkpoint data sufficient to continue after process restart or prolonged disconnect. Restarting from byte zero is non-compliant except when registry behavior prevents resumption.

---

## Architecture

### Control Plane Responsibilities

1. Track desired state for UDS artifact sets by scope/target selector.
2. Resolve topology and active parent path for each target.
3. Schedule transfers under DDIL/QoS/budget policies.
4. Persist checkpoints and reconciliation state.
5. Publish convergence metrics and drift alarms.

### Data Plane Responsibilities

1. Pull/push through OCI Distribution endpoints.
2. Perform digest existence checks before transfer.
3. Transfer missing blobs/manifests only.
4. Publish tags/channels only after content completeness checks.

---

## Operational Consequences

### Positive

1. Continuous convergence instead of ad hoc mirror drift.
2. Reduced retransmission through digest-level diffing and checkpoint resume.
3. Better scale through hierarchical fan-out and wave control.
4. UDS package promotion can be governed uniformly across DDIL conditions.

### Tradeoffs

1. Additional state management in planner/scheduler/checkpoint components.
2. Registry implementation differences may require compatibility adapters.
3. More telemetry and alerting surface area is required for safe operations.

---

## Implementation Guidance

### Phase 1

1. Define synchronization CRDs/documents and lifecycle state machine.
2. Implement per-target digest delta and checkpoint persistence.
3. Expose convergence status API (`pending`, `in-progress`, `degraded`, `converged`).

### Phase 2

1. Add topology graph execution and parent failover logic.
2. Add DDIL policy classes and budget enforcement.
3. Add wave-based rollout controls for large fan-out updates.

### Phase 3

1. Add referrers-required convergence gates.
2. Add drift detection/reconciliation loops.
3. Validate at scale across 1000+ logical targets in simulation.

---

## Open Questions

1. Which UDS package channels require strict referrer gating by default?
2. Should parent failover be fully automatic or approval-gated at certain tiers?
3. What is the minimum convergence SLA for `mission-critical` class in disconnected windows?

---

## References

1. OCI Distribution Specification: https://github.com/opencontainers/distribution-spec
2. Distribution reference implementation: https://github.com/distribution/distribution
3. Distribution documentation: https://distribution.github.io/distribution/
4. ADR-045: Zarf/UDS Integration for Tactical Software Delivery
5. ADR-052: Peat OCI Distribution Registry Replication at DIL and Massive Scale
