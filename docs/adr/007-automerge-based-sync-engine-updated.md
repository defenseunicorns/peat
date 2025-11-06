# ADR-007: CRDT-Based Sync Engine - Automerge vs Loro Evaluation

**Status**: Under Evaluation (E8 Phase)
**Date**: 2025-11-05 (Updated)
**Original Date**: 2025-11-04
**Authors**: Codex, Kit Plummer
**Supersedes**: ADR-005 (Data Sync Abstraction Layer)
**Replaces**: ADR-002 (Beacon Storage Architecture - Ditto-based)

## Executive Summary

This ADR proposes evaluating **both Automerge and Loro** as CRDT backend options during Exercise 8 (E8), with final selection based on CAP-specific performance benchmarks. Both are open-source, production-ready CRDT libraries that eliminate Ditto licensing constraints while positioning CAP as GOTS software. The evaluation will determine which backend best serves CAP's tactical requirements for bandwidth-constrained, hierarchical autonomous coordination.

**Key Decision Points:**
- ✅ **Eliminate Ditto**: Both options remove licensing constraints
- ✅ **GOTS Positioning**: Both enable open-source GOTS strategy
- ⚖️ **Backend Selection**: E8 evaluation will determine Automerge vs Loro
- 🎯 **Decision Criteria**: Bandwidth efficiency, edge device performance, hierarchical data model fit

## Context

### Business Constraints

**Critical Requirement**: Eliminate Ditto licensing dependency to avoid:
- **Vendor lock-in** with proprietary SDK
- **Licensing costs** for production tactical deployments
- **Legal constraints** on distribution and modification
- **Support dependencies** on third-party vendor availability

### Strategic Value: OSS/GOTS and NATO Standardization

**Government Off-The-Shelf (GOTS) Opportunity**

An open-source, Automerge-based sync engine positions CAP Protocol as **Government Off-The-Shelf (GOTS)** software, providing critical advantages:

1. **Open Architecture Compliance**
   - Aligns with DoD's **Modular Open Systems Approach (MOSA)**
   - Supports **Open Mission Systems (OMS)** initiative for unmanned systems
   - Enables vendor-neutral integration across platforms
   - Facilitates competition and innovation in tactical autonomous systems

2. **NATO Standardization Path**
   - **STANAG Candidate**: CAP Protocol + automerge-edge could become a NATO standard for autonomous platform coordination
   - **Interoperability**: Allied forces can adopt without licensing barriers
   - **Multi-National Development**: NATO members can contribute improvements
   - **Coalition Operations**: Shared technology base for combined operations

3. **Acquisition Benefits**
   - **Reduced Program Risk**: No proprietary dependencies to negotiate
   - **Faster ATO Process**: Full source code inspection for cybersecurity review
   - **Lower TCO**: No per-unit licensing fees for fleet deployments
   - **Sovereign Control**: Nations maintain full control over critical infrastructure

4. **Industrial Base Advantages**
   - **Prime Contractor Friendly**: Defense contractors can integrate freely
   - **SME Participation**: Lower barriers for small innovative companies
   - **Technology Transfer**: Allies can adapt for national requirements
   - **Export Control**: Simpler ITAR/EAR compliance for open-source components

### Comparison: Proprietary vs GOTS

| Aspect | Ditto (Proprietary) | automerge-edge (GOTS) |
|--------|---------------------|------------------------|
| **Licensing** | Commercial, per-seat | Apache-2.0 or MIT (free) |
| **Source Access** | SDK only, core closed | Full source transparency |
| **Modification Rights** | Restricted by EULA | Unlimited modification |
| **NATO Sharing** | License complications | Freely sharable |
| **Multi-National Dev** | Blocked by IP | Encouraged and supported |
| **Acquisition** | Complex procurement | Simplified as GOTS |
| **Vendor Lock-in** | High | Zero |
| **Export Control** | Complex | Streamlined for OSS |
| **Standardization** | Proprietary barriers | Open standards candidate |
| **Long-term Support** | Vendor-dependent | Community + govt sustainment |

### NATO Standardization Precedents

Historical examples of successful defense technology standardization:

1. **Link 16 (STANAG 5516)** - Tactical data link standard
2. **CBRN Systems (AEP-66)** - Chemical, Biological, Radiological, Nuclear detection
3. **ATDL-1/VMF (STANAG 5500)** - Variable message format for tactical messaging
4. **ASTERIX (STANAG 4761)** - Air traffic surveillance data format

**CAP Protocol + automerge-edge** could become the **STANAG for autonomous platform coordination**, analogous to how Link 16 standardized data sharing between manned platforms.

### Open Architecture Alignment

The DoD's **Digital Engineering Strategy** and **Modular Open Systems Approach (MOSA)** explicitly require:

> "Use of open standards, architectures, and practices to enable innovation, competition, and evolutionary acquisition throughout the system lifecycle."

**automerge-edge meets these requirements** (regardless of CRDT backend):

✅ **Open Standards**: Uses IETF protocols (TCP, TLS), standard encoding (columnar format)
✅ **Modular Design**: Pluggable discovery, transport, storage, security
✅ **Well-Defined Interfaces**: Clear API boundaries, documented behavior
✅ **Data Rights**: Full Government Purpose Rights (GPR) via permissive license
✅ **Vendor Neutrality**: No single-source dependency
✅ **Technology Refresh**: Can swap components without system redesign

## CRDT Backend Evaluation: Automerge vs Loro

### Overview

Both **Automerge** and **Loro** are mature, open-source CRDT libraries that meet CAP's requirements. This section analyzes their trade-offs to guide the E8 evaluation.

### Comparison Matrix

| **Criterion** | **Automerge** | **Loro** | **Relevance to CAP** |
|---------------|---------------|----------|----------------------|
| **Maturity** | 5+ years, battle-tested | 1+ year, v1.0 stable | High (risk mitigation) |
| **Performance** | Good, proven | 3-15x faster in benchmarks | High (edge devices) |
| **Compression** | Excellent (85-95%) | Good, slightly larger docs | Critical (bandwidth) |
| **Rust Native** | Yes | Yes | High (memory safety) |
| **Encoding** | Columnar (custom) | Columnar (custom) | High (efficiency) |
| **License** | MIT | MIT | High (GOTS compatibility) |
| **Ecosystem** | Larger, established | Growing, active | Medium (community support) |
| **Bundle Size (WASM)** | 1.7MB | 2.9MB | Low (server-side use) |
| **Hierarchical Data** | Maps/Lists | Native Trees | High (4-level hierarchy) |
| **Time Travel** | Yes | Yes, advanced snapshots | Medium (debugging) |
| **Community** | Martin Kleppmann backing | Responsive team | Medium (long-term support) |

### Performance Benchmarks

From public benchmarks on real-world editing dataset (259,778 operations):

| **Metric** | **Automerge** | **Loro** | **Winner** |
|------------|---------------|----------|------------|
| Apply time | 7,109 ms | 2,271 ms | Loro (3.1x) |
| Encode time | 165 ms | 11 ms | Loro (15x) |
| Parse time | 1,185 ms | 6 ms | Loro (197x) |
| Document size | 129 KB | 231 KB | Automerge (44% smaller) |

**Scaling Test** (26M operations, 10M characters):
- **Automerge**: Ran out of memory
- **Loro**: Completed in 75 seconds with 26.8MB document

### Encoding Analysis: Why Both Beat CBOR

Both use **columnar encoding** optimized for CRDTs vs Ditto's CBOR approach:

**Compression Techniques (Both Libraries):**
1. **LEB128 variable-length integers** - Small values = fewer bytes
2. **Run-length encoding (RLE)** - Compress repeated values
3. **Delta encoding** - Store differences, not absolute values
4. **Actor ID deduplication** - Reference IDs by index
5. **Optional DEFLATE** - Additional compression layer

**Example: 100 Position Updates**
```
CBOR (Ditto):      ~6,000 bytes (full documents)
Automerge Delta:   ~500 bytes (columnar + RLE)
Loro Delta:        ~800 bytes (faster encoding)
```

**Verdict**: Both massively superior to CBOR for CRDT operations. Automerge has slight compression edge; Loro has massive speed advantage.

### CAP-Specific Considerations

#### 1. Hierarchical Data Model (Node → Cell → Squad → Wing)

**Automerge Approach:**
```rust
// Nested maps and lists
doc.put("wing", Map {
  "squads": List [
    Map { "cells": List [ Map { "nodes": [...] } ] }
  ]
})
```

**Loro Approach:**
```rust
// Native tree structure
let tree = doc.get_tree("hierarchy");
let wing = tree.create_node(None, "wing_1");
let squad = tree.create_node(Some(wing), "squad_1");
let cell = tree.create_node(Some(squad), "cell_1");
// Natural hierarchical operations: move, get_parent, get_children
```

**Impact**: Loro's native tree CRDT may simplify CAP's hierarchical operations and reduce code complexity.

#### 2. High-Frequency Telemetry (Position @ 10Hz)

**Automerge**: Proven but slower parse times (1.2s for 260K ops)
**Loro**: 197x faster parsing (6ms) = better for real-time telemetry streams

**Impact**: Loro's speed advantage matters for 100 platforms @ 10Hz = 1000 updates/sec

#### 3. Bandwidth Constraints (Tactical Radio)

**Automerge**: 44% smaller documents (129KB vs 231KB in benchmarks)
**Loro**: Faster encoding compensates for slightly larger size

**Impact**: Link 16 @ 30-230 Kbps makes every byte count. Need CAP-specific bandwidth tests.

#### 4. Edge Device Performance (Raspberry Pi 4)

**Automerge**: Lower memory footprint, proven on embedded
**Loro**: 3x faster operations = lower CPU usage = better battery life

**Impact**: Both viable, but Loro's speed may extend mission duration on battery power.

#### 5. Time Travel / Versioning

**Automerge**: Full history retained
**Loro**: Shallow snapshots for fast loading (8.4MB for 360K ops)

**Impact**: Loro's snapshots could dramatically speed up node re-sync after network partition.

### Risk Assessment

| **Risk** | **Automerge** | **Loro** |
|----------|---------------|----------|
| Breaking Changes | Low (mature API) | Medium (v1.0 but newer) |
| Project Abandonment | Low (established) | Medium (smaller team) |
| Performance Scaling | Known limitations | Unknown at CAP scale |
| Documentation Gaps | Few | Some (newer project) |
| DoD Certification | Lower risk (proven) | Moderate risk (newer) |
| Community Support | Established | Growing rapidly |

### Strategic Considerations

**Automerge Advantages:**
- ✅ **Proven stability** - 5 years in production apps
- ✅ **Better compression** - 44% smaller documents
- ✅ **Established ecosystem** - More tooling and examples
- ✅ **Lower certification risk** - Easier DoD/NATO approval
- ✅ **Martin Kleppmann** - CRDT pioneer backing

**Loro Advantages:**
- ✅ **3-15x faster** - Dramatic performance advantage
- ✅ **Native trees** - Perfect for CAP hierarchy
- ✅ **Active development** - Rapid bug fixes and improvements
- ✅ **Lower CPU usage** - Better for battery-constrained platforms
- ✅ **Modern design** - Learns from Automerge/Yjs experience

### Recommendation: Evaluate Both in E8

**Why not choose now?**
1. Generic benchmarks don't reflect CAP's specific patterns
2. Hierarchical aggregation performance unknown
3. Bandwidth impact needs tactical radio testing
4. E8 abstraction work supports both backends

**E8 Evaluation Plan** (see detailed section below):
1. Design backend-agnostic abstraction layer
2. Implement both Automerge and Loro backends
3. Run CAP-specific benchmarks (position updates, capability aggregation, cold sync)
4. Measure bandwidth, latency, CPU, memory
5. Select based on data, not assumptions

**Expected Outcome:**
- If bandwidth is bottleneck → **Automerge** (better compression)
- If CPU/latency is bottleneck → **Loro** (3x faster)
- If hierarchy complexity is high → **Loro** (native trees)
- If certification risk is critical → **Automerge** (proven)

**Most Likely Result**: Loro wins on performance, but Automerge wins on compression. Final choice depends on which constraint dominates CAP's operational profile.

### Coalition Operations Benefits

**Scenario: US + NATO Allies conduct joint autonomous ISR mission**

With Ditto (Proprietary):
- ❌ Each nation needs separate licenses
- ❌ Export approvals for SDK distribution
- ❌ Cannot modify for national requirements
- ❌ Vendor must approve multi-national access
- ❌ Complex procurement across nations

With automerge-edge (GOTS):
- ✅ All nations freely adopt and deploy
- ✅ Simplified technology transfer
- ✅ Each nation can adapt to doctrine
- ✅ Collaborative development and testing
- ✅ Single acquisition for entire coalition

### Industry and Academic Collaboration

Open-source approach enables broader innovation ecosystem:

**Defense Contractors**:
- General Dynamics, Lockheed Martin, Northrop Grumman can integrate freely
- Small defense tech companies (Shield AI, Anduril, etc.) can build on foundation
- International partners (BAE Systems, Thales, etc.) can contribute

**Research Institutions**:
- MIT, CMU, Stanford can extend for research
- NATO Science & Technology Organization (STO) can evaluate and standardize
- DARPA programs can build on proven foundation

**Open Source Community**:
- Rust ecosystem benefits from mature P2P sync library
- Feedback loop improves robustness
- Security researchers can audit and report vulnerabilities

### Path to NATO STANAG

**Proposed Timeline**:

1. **Year 1: Demonstrate in CAP Protocol**
   - Prove capability in US tactical autonomous systems
   - Publish performance benchmarks and test results
   - Present at DoD and NATO conferences

2. **Year 2: Multi-National Trials**
   - Coordinate with NATO NIAG (Industrial Advisory Group)
   - Conduct interoperability tests with allied systems
   - Gather feedback from NATO member nations

3. **Year 3: Draft STANAG Proposal**
   - Work with NATO Standardization Office (NSO)
   - Define conformance requirements
   - Establish certification process

4. **Year 4-5: Ratification and Adoption**
   - NATO member approval process
   - Integration into allied autonomous systems
   - Establish maintenance and evolution governance

### Government Sustainment Model

Unlike proprietary software dependent on vendor lifecycle, GOTS software has sustainable government ownership:

**Sustainment Options**:
1. **Government In-House**: DoD software factories (Kessel Run, AFWerX) can maintain
2. **Contractor Support**: Any qualified contractor can provide support (competition)
3. **Federally Funded R&D**: SBIR/STTR programs can fund enhancements
4. **Open Source Community**: Leverage broader ecosystem contributions

### Risk Mitigation for OSS Approach

**Concern**: "Open source means less secure"

**Reality**: Security through obscurity is not effective. Open source enables:
- Public security audits (more eyes on code)
- Faster vulnerability patching (community response)
- Cryptographic verification (no hidden backdoors)
- Government security teams can audit directly

**Concern**: "No vendor support"

**Reality**: GOTS software can have multiple support providers:
- Government software factories
- Prime contractors (competed)
- Original developers (commercial support model)
- NATO member nation support teams

**Concern**: "Adversaries can study the code"

**Reality**: Security should not depend on secrecy of algorithms:
- Military cryptography is public (AES, SHA-256, etc.)
- Link 16 specifications are documented
- Security comes from key management, not code secrecy
- adversaries will reverse-engineer anyway

### Technical Analysis

After prototyping with Ditto SDK, we've identified fundamental limitations:
1. **Document update semantics** - No true upsert, creating duplicate documents
2. **Query limitations** - No ORDER BY, limited filtering, no aggregations
3. **Wire protocol inefficiency** - CBOR-based vs superior columnar encoding
4. **Complexity** - Large SDK with many unnecessary features for our use case
5. **Testing brittleness** - Requires real Ditto instances, complicates CI/CD

### Strategic Decision: Rip-and-Replace with Backend Evaluation

**Decision**: **Rip-and-replace Ditto** with custom implementation. **Evaluate both Automerge and Loro** during E8 phase.

**Rationale for Eliminating Ditto**:
- **Licensing makes production deployment non-viable**
- **Abstraction overhead not justified** if Ditto won't be used
- **Cleaner codebase**: Direct CRDT integration avoids indirection
- **Faster development**: Focus on one architecture pattern
- **Better testing**: Mock-friendly architecture without SDK dependencies

**Rationale for Dual Backend Evaluation**:
- **Perfect timing**: Starting abstraction design now (no sunk cost)
- **Unknown trade-offs**: Generic benchmarks don't match CAP patterns
- **Different strengths**: Automerge (compression) vs Loro (speed)
- **Low cost**: Abstraction layer supports both, 2-week evaluation
- **Data-driven**: Choose based on CAP-specific measurements

**Migration Path**:
1. Keep Ditto for **reference only** during E8 development (compare behaviors)
2. Design **backend-agnostic abstraction** (`CrdtBackend` trait)
3. Implement **both** Automerge and Loro backends in parallel
4. Run **CAP-specific benchmarks** during E8
5. **Select winner** based on performance data
6. **Remove non-selected backend** and Ditto references
7. Move forward with single production implementation

## E8 Evaluation Framework

Exercise 8 (E8) will serve as the evaluation phase to select between Automerge and Loro through empirical testing with CAP-specific workloads.

### Phase 1: Abstraction Design (Week 1, Days 1-3)

**Objective**: Design backend-agnostic trait that both CRDTs can implement

**Deliverables**:
```rust
/// Core abstraction for CRDT backends
pub trait CrdtBackend: Send + Sync {
    type DocumentId: Clone + Send + Sync;
    type Version: Clone + Send + Sync;
    type Delta: Send + Sync;
    
    // Document lifecycle
    fn create_document(&self, collection: &str) -> Result<Self::DocumentId>;
    fn delete_document(&mut self, id: &Self::DocumentId) -> Result<()>;
    
    // CRDT operations
    fn apply_delta(&mut self, id: &Self::DocumentId, delta: &Self::Delta) -> Result<()>;
    fn generate_delta(&self, id: &Self::DocumentId, from: Option<&Self::Version>) -> Result<Self::Delta>;
    fn merge(&mut self, id: &Self::DocumentId, remote_delta: &Self::Delta) -> Result<()>;
    fn get_version(&self, id: &Self::DocumentId) -> Result<Self::Version>;
    
    // CAP-specific operations
    fn update_node_state(&mut self, node_id: &str, state: NodeState) -> Result<()>;
    fn update_node_position(&mut self, node_id: &str, pos: Position) -> Result<()>;
    fn update_node_capability(&mut self, node_id: &str, cap: Capability) -> Result<()>;
    fn get_node_state(&self, node_id: &str) -> Result<Option<NodeState>>;
    
    // Hierarchical operations  
    fn create_cell(&mut self, cell_id: &str, member_ids: Vec<String>) -> Result<()>;
    fn update_cell_leader(&mut self, cell_id: &str, leader_id: String) -> Result<()>;
    fn get_cell_aggregate(&self, cell_id: &str) -> Result<CellState>;
    fn move_node_to_cell(&mut self, node_id: &str, cell_id: &str) -> Result<()>;
    
    // Sync operations
    fn export_snapshot(&self) -> Result<Vec<u8>>;
    fn import_snapshot(&mut self, snapshot: &[u8]) -> Result<()>;
    fn get_changes_since(&self, version: &Self::Version) -> Result<Vec<Self::Delta>>;
    
    // Metrics for evaluation
    fn get_metrics(&self) -> BackendMetrics;
}

#[derive(Debug, Clone)]
pub struct BackendMetrics {
    pub total_operations: usize,
    pub document_size_bytes: usize,
    pub last_delta_size_bytes: usize,
    pub peak_memory_bytes: usize,
    pub avg_operation_micros: u64,
}

// Concrete implementations
pub struct AutomergeBackend { /* ... */ }
pub struct LoroBackend { /* ... */ }
```

**Success Criteria**:
- ✅ Trait compiles and passes basic tests
- ✅ Both implementations sketched out
- ✅ Team review and approval

### Phase 2: Parallel Implementation (Week 1-2, Days 4-10)

**Objective**: Implement both backends with feature parity

**Automerge Implementation** (Days 4-7):
```rust
pub struct AutomergeBackend {
    docs: HashMap<String, Automerge>,
    storage: Box<dyn Storage>,
    metrics: Arc<Mutex<BackendMetrics>>,
}

impl CrdtBackend for AutomergeBackend {
    type DocumentId = String;
    type Version = Vec<u8>; // Automerge's oplog version
    type Delta = Vec<u8>;   // Automerge sync message
    
    fn update_node_position(&mut self, node_id: &str, pos: Position) -> Result<()> {
        let doc = self.docs.get_mut(node_id)?;
        doc.transact(|tx| {
            tx.put(ROOT, "lat", pos.lat)?;
            tx.put(ROOT, "lon", pos.lon)?;
            tx.put(ROOT, "alt", pos.alt)?;
            Ok(())
        })
    }
    
    fn generate_delta(&self, id: &Self::DocumentId, from: Option<&Self::Version>) -> Result<Self::Delta> {
        let doc = self.docs.get(id)?;
        let sync_message = doc.generate_sync_message(from)?;
        Ok(sync_message.encode())
    }
    
    // ... rest of implementation
}
```

**Loro Implementation** (Days 4-7):
```rust
pub struct LoroBackend {
    docs: HashMap<String, LoroDoc>,
    tree: LoroTree, // For hierarchical structure
    storage: Box<dyn Storage>,
    metrics: Arc<Mutex<BackendMetrics>>,
}

impl CrdtBackend for LoroBackend {
    type DocumentId = String;
    type Version = Frontiers; // Loro's version vector
    type Delta = Vec<u8>;     // Loro update bytes
    
    fn update_node_position(&mut self, node_id: &str, pos: Position) -> Result<()> {
        let doc = self.docs.get_mut(node_id)?;
        let map = doc.get_map("state");
        map.insert("lat", pos.lat)?;
        map.insert("lon", pos.lon)?;
        map.insert("alt", pos.alt)?;
        Ok(())
    }
    
    fn move_node_to_cell(&mut self, node_id: &str, cell_id: &str) -> Result<()> {
        // Leverage Loro's native tree operations
        self.tree.mov(node_id, cell_id)?;
        Ok(())
    }
    
    // ... rest of implementation
}
```

**Integration Testing** (Days 8-10):
- Unit tests for each backend
- Cross-backend sync tests (Automerge ↔ Automerge, Loro ↔ Loro)
- Basic CAP scenarios (create nodes, update positions, form cells)

**Success Criteria**:
- ✅ Both backends pass all unit tests
- ✅ CAP operations work correctly in both
- ✅ No blocking bugs

### Phase 3: CAP-Specific Benchmarking (Week 2, Days 11-14)

**Objective**: Measure performance with realistic CAP workloads

**Benchmark Suite**:

```rust
// benchmarks/cap_scenarios.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn benchmark_backends(c: &mut Criterion) {
    let mut group = c.benchmark_group("CAP CRDT Backends");
    
    // Scenario 1: High-frequency telemetry (most critical)
    for backend in ["automerge", "loro"] {
        group.bench_with_input(
            BenchmarkId::new("position_updates_10hz_100nodes", backend),
            &backend,
            |b, &backend| {
                b.iter(|| {
                    let mut backend = create_backend(backend);
                    // 10 seconds of 10Hz updates from 100 nodes = 10,000 updates
                    for _ in 0..10_000 {
                        backend.update_position(
                            &random_node_id(),
                            black_box(random_position())
                        ).unwrap();
                    }
                });
            },
        );
    }
    
    // Scenario 2: Hierarchical capability aggregation
    for backend in ["automerge", "loro"] {
        group.bench_with_input(
            BenchmarkId::new("capability_aggregation_4levels", backend),
            &backend,
            |b, &backend| {
                b.iter(|| {
                    let mut backend = create_backend(backend);
                    // Create 100 nodes → 10 cells → 3 squads → 1 wing
                    setup_hierarchy(&mut backend, 100, 10, 3, 1);
                    // Update all node capabilities
                    for i in 0..100 {
                        backend.update_node_capability(
                            &format!("node_{}", i),
                            random_capability()
                        ).unwrap();
                    }
                    // Aggregate up hierarchy
                    let wing_caps = backend.get_cell_aggregate("wing_1").unwrap();
                    black_box(wing_caps);
                });
            },
        );
    }
    
    // Scenario 3: Delta size (bandwidth measurement)
    for backend in ["automerge", "loro"] {
        group.bench_with_input(
            BenchmarkId::new("delta_size_100_position_updates", backend),
            &backend,
            |b, &backend| {
                b.iter(|| {
                    let mut backend = create_backend(backend);
                    let start_version = backend.get_version("test").unwrap();
                    
                    // 100 position updates
                    for _ in 0..100 {
                        backend.update_position("node_1", random_position()).unwrap();
                    }
                    
                    // Measure delta size
                    let delta = backend.generate_delta("test", Some(&start_version)).unwrap();
                    black_box(delta.len()) // bytes
                });
            },
        );
    }
    
    // Scenario 4: Cold-start sync (critical for new node joining)
    for backend in ["automerge", "loro"] {
        group.bench_with_input(
            BenchmarkId::new("cold_sync_1hour_history", backend),
            &backend,
            |b, &backend| {
                b.iter(|| {
                    // Simulate 1 hour: 100 nodes * 10Hz * 3600s = 3.6M updates
                    let mut existing = create_backend(backend);
                    for _ in 0..36_000 {
                        existing.update_position(&random_node_id(), random_position()).unwrap();
                    }
                    
                    // New node syncs
                    let mut new_node = create_backend(backend);
                    let snapshot = existing.export_snapshot().unwrap();
                    new_node.import_snapshot(&snapshot).unwrap();
                    
                    black_box((snapshot.len(), new_node.get_metrics()));
                });
            },
        );
    }
    
    // Scenario 5: Memory usage under load
    for backend in ["automerge", "loro"] {
        group.bench_with_input(
            BenchmarkId::new("memory_100nodes_10min", backend),
            &backend,
            |b, &backend| {
                b.iter(|| {
                    let mut backend = create_backend(backend);
                    // 10 minutes: 100 nodes * 10Hz * 600s = 600K updates
                    for _ in 0..60_000 {
                        backend.update_position(&random_node_id(), random_position()).unwrap();
                    }
                    let metrics = backend.get_metrics();
                    black_box(metrics.peak_memory_bytes);
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(benches, benchmark_backends);
criterion_main!(benches);
```

**Measurements to Collect**:

| **Metric** | **Unit** | **Why It Matters** |
|------------|----------|-------------------|
| Position update latency | microseconds | Real-time telemetry requirement |
| Aggregation time | milliseconds | Leader election speed |
| Delta size (100 updates) | bytes | Bandwidth on tactical radio |
| Snapshot size (1hr history) | MB | Cold-sync time for new node |
| Peak memory usage | MB | Edge device constraints (4GB) |
| CPU usage (% of core) | percentage | Battery life impact |
| Operations per second | ops/sec | Throughput ceiling |

**Success Criteria**:
- ✅ All benchmarks complete successfully
- ✅ Results exported to JSON for analysis
- ✅ No crashes or memory leaks

### Phase 4: Analysis and Decision (Week 2, Days 15-16)

**Decision Matrix** (to be populated with benchmark results):

| **Criterion** | **Weight** | **Automerge** | **Loro** | **Weighted Score** |
|---------------|------------|---------------|----------|-------------------|
| **Performance** |
| Position update latency (µs) | 10 | ? | ? | |
| Aggregation speed (ms) | 8 | ? | ? | |
| Cold-sync time (s) | 7 | ? | ? | |
| Memory usage (MB) | 8 | ? | ? | |
| **Bandwidth** |
| Delta size - position (bytes) | 10 | ? | ? | |
| Delta size - capability (bytes) | 8 | ? | ? | |
| Snapshot size (MB) | 7 | ? | ? | |
| **Features** |
| Hierarchical model fit | 9 | ? | ? | |
| Time travel support | 5 | ? | ? | |
| Conflict resolution | 10 | ? | ? | |
| **Production** |
| Maturity (1-10) | 9 | 9 | 6 | |
| Documentation (1-10) | 7 | 8 | 6 | |
| Community (1-10) | 6 | 8 | 6 | |
| Certification risk (1-10) | 10 | 9 | 5 | |
| **Total** | **131** | **?** | **?** | |

**Decision Rules**:

1. **If Automerge delta size <20% smaller AND Loro not significantly faster** → **Choose Automerge** (bandwidth is critical)

2. **If Loro 3x+ faster AND delta size difference <30%** → **Choose Loro** (performance matters for edge devices)

3. **If hierarchy operations significantly cleaner in Loro** → **Weight towards Loro** (code maintainability)

4. **If memory usage exceeds 1GB in either** → **Disqualify** (edge device constraint)

5. **If DoD certification timeline is tight** → **Weight towards Automerge** (lower risk)

**Final Deliverable**: Updated ADR-007 with:
- Benchmark results table
- Selected backend with justification
- Architecture updated for winning choice
- Migration plan from Ditto complete

### Phase 5: Implementation (Week 3+)

**Path Forward**:
1. Remove non-selected backend code
2. Build `cap-protocol-core` on winning backend
3. Integrate with CellStore/NodeStore
4. Update E2E tests
5. Remove all Ditto dependencies
6. Document final architecture

**Rollback Plan**:
- If benchmarks show both inadequate: Re-evaluate or hybrid approach
- If E8 timeline slips: Make decision based on initial data, iterate later
- If showstopper bug found: Keep Ditto temporarily, file issues with CRDT project

### Crate Architecture (Backend-Agnostic)

```
cap-sync-engine/              # Working name (was "automerge-edge")
├── Cargo.toml               # Standalone crate, published to crates.io
├── README.md                # General-purpose marketing (not CAP-specific)
├── LICENSE                  # Apache-2.0 or MIT (permissive)
│
├── src/
│   ├── lib.rs               # Public API
│   │
│   ├── backend/             # CRDT backend abstraction
│   │   ├── mod.rs           # CrdtBackend trait definition
│   │   ├── automerge.rs     # Automerge implementation (E8 evaluation)
│   │   ├── loro.rs          # Loro implementation (E8 evaluation)
│   │   └── metrics.rs       # Performance metrics collection
│   │
│   ├── core/                # Backend-agnostic core logic
│   │   ├── mod.rs
│   │   ├── document.rs      # Document management
│   │   ├── sync.rs          # Sync protocol
│   │   └── storage.rs       # Persistence layer (pluggable)
│   │
│   ├── discovery/           # Peer discovery (pluggable)
│   │   ├── mod.rs
│   │   ├── mdns.rs          # mDNS/DNS-SD discovery
│   │   ├── manual.rs        # Manual peer configuration
│   │   └── traits.rs        # Discovery plugin trait
│   │
│   ├── transport/           # Network transports (pluggable)
│   │   ├── mod.rs
│   │   ├── tcp.rs           # TCP transport
│   │   ├── quic.rs          # QUIC transport (future)
│   │   └── traits.rs        # Transport plugin trait
│   │
│   ├── repo/                # Repository (multi-document management)
│   │   ├── mod.rs
│   │   ├── repository.rs    # Main API
│   │   ├── collection.rs    # Collection abstraction
│   │   └── query.rs         # Query engine
│   │
│   ├── sync/                # Synchronization coordination
│   │   ├── mod.rs
│   │   ├── peer_manager.rs  # Peer lifecycle
│   │   ├── sync_engine.rs   # Orchestrate sync across peers
│   │   └── priority.rs      # Priority-based sync (optional)
│   │
│   └── security/            # Security layer (from ADR-006)
│       ├── mod.rs
│       ├── auth.rs          # Device/user authentication
│       ├── authz.rs         # Authorization (RBAC)
│       ├── crypto.rs        # Encryption
│       └── audit.rs         # Audit logging
│
├── examples/
│   ├── basic_sync.rs        # Simple two-peer sync
│   ├── collections.rs       # Collection-based usage
│   ├── offline_notes.rs     # Offline notes app
│   └── cap_hierarchy.rs     # CAP-specific hierarchy example
│
├── benches/
│   ├── cap_scenarios.rs     # CAP-specific benchmarks (E8)
│   └── generic.rs           # Generic CRDT benchmarks
│
└── tests/
    ├── sync_tests.rs        # Two-peer sync tests
    ├── partition_tests.rs   # Network partition tolerance
    ├── backend_tests.rs     # Backend-agnostic tests
    └── e2e_tests.rs         # End-to-end scenarios
```

**Note**: After E8 evaluation, non-selected backend will be removed, and crate may be renamed based on final choice (e.g., `cap-sync-engine` or `automerge-edge` or `loro-edge`).

### Core Design Principles

1. **Automerge as CRDT Foundation**
   - Use `automerge` crate for all CRDT operations
   - Leverage columnar encoding (85-95% compression)
   - Rich CRDT types: maps, lists, text, counters

2. **Modular Architecture**
   - **Discovery** is pluggable (mDNS, manual, Bluetooth, etc.)
   - **Transport** is pluggable (TCP, QUIC, WebSocket, etc.)
   - **Storage** is pluggable (RocksDB, SQLite, in-memory)
   - **Security** is optional but integrated (from ADR-006)

3. **General-Purpose by Design**
   - Not CAP-specific - usable for any offline-first app
   - Collections API similar to MongoDB/Ditto
   - Observable changes for reactive UIs
   - Works on mobile, embedded, server, desktop

4. **Production-Ready**
   - Comprehensive error handling
   - Instrumentation and metrics
   - Testing at all levels (unit, integration, E2E)
   - Performance benchmarks vs Ditto

## Architecture Deep-Dive

### 1. Automerge Integration Layer

Automerge provides CRDTs, but we need to add:

```rust
use automerge::{Automerge, ReadDoc, transaction::Transactable};

/// Wrapper around Automerge document with metadata
pub struct Document {
    /// Underlying Automerge document
    doc: Automerge,

    /// Document ID (UUID)
    id: DocumentId,

    /// Collection name (for organization)
    collection: String,

    /// Local metadata (not synced)
    metadata: DocumentMetadata,
}

impl Document {
    /// Create new document
    pub fn new(collection: impl Into<String>) -> Self {
        Self {
            doc: Automerge::new(),
            id: DocumentId::new_v4(),
            collection: collection.into(),
            metadata: DocumentMetadata::default(),
        }
    }

    /// Update document (transactional)
    pub fn update<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Automerge) -> Result<R>,
    {
        let result = f(&mut self.doc)?;
        self.metadata.updated_at = SystemTime::now();
        Ok(result)
    }

    /// Get value at path
    pub fn get(&self, path: &[&str]) -> Result<Value> {
        let mut obj = self.doc.root();
        for &key in path {
            obj = self.doc.get(obj, key)?;
        }
        Ok(Value::from_automerge(obj))
    }

    /// Set value at path
    pub fn set(&mut self, path: &[&str], value: Value) -> Result<()> {
        self.doc.transaction(|tx| {
            let mut obj = tx.root();
            for &key in &path[..path.len() - 1] {
                obj = tx.get(obj, key)?;
            }
            tx.put(obj, path[path.len() - 1], value.to_automerge())?;
            Ok(())
        })
    }

    /// Generate sync message for peer
    pub fn generate_sync_message(&mut self, peer_state: &SyncState) -> Result<Vec<u8>> {
        automerge::sync::generate_sync_message(&mut self.doc, peer_state)
    }

    /// Receive sync message from peer
    pub fn receive_sync_message(&mut self, message: &[u8]) -> Result<()> {
        automerge::sync::receive_sync_message(&mut self.doc, message)
    }

    /// Get document as JSON (for querying)
    pub fn to_json(&self) -> Result<serde_json::Value> {
        automerge::export(&self.doc)
    }
}
```

### 2. Repository API (High-Level Interface)

```rust
/// Repository manages multiple documents with collections
pub struct Repository {
    /// Storage backend
    storage: Box<dyn StorageBackend>,

    /// Peer manager (discovery + connections)
    peers: Arc<PeerManager>,

    /// Sync engine (coordinates sync across peers)
    sync: Arc<SyncEngine>,

    /// Security manager (optional)
    security: Option<Arc<SecurityManager>>,

    /// Document cache (in-memory)
    documents: Arc<RwLock<HashMap<DocumentId, Document>>>,
}

impl Repository {
    /// Create new repository with RocksDB storage
    pub async fn new(path: impl AsRef<Path>) -> Result<Self> {
        let storage = RocksDbStorage::new(path)?;
        let peers = Arc::new(PeerManager::new());
        let sync = Arc::new(SyncEngine::new(peers.clone()));

        Ok(Self {
            storage: Box::new(storage),
            peers,
            sync,
            security: None,
            documents: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create in-memory repository (for testing)
    pub fn new_in_memory() -> Self {
        Self {
            storage: Box::new(InMemoryStorage::new()),
            peers: Arc::new(PeerManager::new()),
            sync: Arc::new(SyncEngine::new(peers.clone())),
            security: None,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Access a collection
    pub fn collection(&self, name: impl Into<String>) -> Collection {
        Collection::new(name.into(), self.clone())
    }

    /// Start peer discovery
    pub async fn start_discovery(&self, config: DiscoveryConfig) -> Result<()> {
        match config.method {
            DiscoveryMethod::Mdns => {
                let discovery = MdnsDiscovery::new(config)?;
                self.peers.add_discovery(Box::new(discovery)).await?;
            }
            DiscoveryMethod::Manual(addrs) => {
                for addr in addrs {
                    self.peers.add_manual_peer(addr).await?;
                }
            }
        }
        Ok(())
    }

    /// Start synchronization
    pub async fn start_sync(&self) -> Result<()> {
        self.sync.start().await
    }

    /// Connect to specific peer
    pub async fn connect(&self, addr: &str) -> Result<PeerId> {
        let transport = TcpTransport::new();
        self.peers.connect(addr, Box::new(transport)).await
    }
}

/// Collection provides MongoDB-like API
pub struct Collection {
    name: String,
    repo: Repository,
}

impl Collection {
    /// Insert document into collection
    pub async fn insert(&self, data: serde_json::Value) -> Result<DocumentId> {
        let mut doc = Document::new(&self.name);

        // Convert JSON to Automerge operations
        doc.update(|automerge| {
            populate_from_json(automerge, data)?;
            Ok(())
        })?;

        // Store locally
        let doc_id = doc.id;
        self.repo.storage.store(&doc).await?;
        self.repo.documents.write().await.insert(doc_id, doc.clone());

        // Broadcast to peers
        self.repo.sync.broadcast_document(&doc).await?;

        Ok(doc_id)
    }

    /// Find documents matching query
    pub async fn find(&self, query: &str) -> Result<Vec<serde_json::Value>> {
        // Load all documents in collection
        let docs = self.repo.storage.load_collection(&self.name).await?;

        // Convert to JSON for querying
        let json_docs: Vec<_> = docs
            .into_iter()
            .map(|doc| doc.to_json())
            .collect::<Result<_>>()?;

        // Apply query (simple implementation - can be optimized)
        let filtered = json_docs
            .into_iter()
            .filter(|doc| matches_query(doc, query))
            .collect();

        Ok(filtered)
    }

    /// Find one document
    pub async fn find_one(&self, query: &str) -> Result<Option<serde_json::Value>> {
        self.find(query).await.map(|mut docs| docs.pop())
    }

    /// Update documents matching query
    pub async fn update(&self, query: &str, update: serde_json::Value) -> Result<usize> {
        let docs = self.repo.storage.load_collection(&self.name).await?;
        let mut count = 0;

        for mut doc in docs {
            if matches_query(&doc.to_json()?, query) {
                doc.update(|automerge| {
                    apply_update(automerge, &update)?;
                    Ok(())
                })?;

                self.repo.storage.store(&doc).await?;
                self.repo.sync.broadcast_document(&doc).await?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Observe changes to collection
    pub fn observe(&self) -> ChangeStream {
        // Return a stream of changes
        self.repo.sync.subscribe_collection(&self.name)
    }
}
```

### 3. Peer Discovery (Pluggable)

```rust
/// Discovery plugin trait
#[async_trait]
pub trait DiscoveryPlugin: Send + Sync {
    /// Start discovery
    async fn start(&mut self) -> Result<()>;

    /// Stop discovery
    async fn stop(&mut self) -> Result<()>;

    /// Get discovered peers
    async fn discovered_peers(&self) -> Vec<PeerInfo>;

    /// Stream of discovery events
    fn event_stream(&self) -> mpsc::UnboundedReceiver<DiscoveryEvent>;
}

/// mDNS-based discovery (for local networks)
pub struct MdnsDiscovery {
    service_name: String,
    port: u16,
    discovered: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
    events: mpsc::UnboundedSender<DiscoveryEvent>,
}

#[async_trait]
impl DiscoveryPlugin for MdnsDiscovery {
    async fn start(&mut self) -> Result<()> {
        // Register mDNS service
        let mdns = mdns_sd::ServiceDaemon::new()?;
        let service_type = format!("_{}.{}", self.service_name, "_tcp.local.");

        mdns.register(mdns_sd::ServiceInfo::new(
            &service_type,
            &format!("{}-{}", self.service_name, uuid::Uuid::new_v4()),
            &format!("{}:{}", get_local_ip()?, self.port),
            "automerge-edge discovery",
        )?)?;

        // Browse for other peers
        let receiver = mdns.browse(&service_type)?;
        let discovered = self.discovered.clone();
        let events = self.events.clone();

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    mdns_sd::ServiceEvent::ServiceResolved(info) => {
                        let peer_info = PeerInfo {
                            peer_id: PeerId::from_string(&info.get_fullname()),
                            address: info.get_addresses().iter().next().unwrap().to_string(),
                            port: info.get_port(),
                        };

                        discovered.write().await.insert(peer_info.peer_id, peer_info.clone());
                        events.send(DiscoveryEvent::PeerFound(peer_info)).ok();
                    }
                    mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => {
                        let peer_id = PeerId::from_string(&fullname);
                        discovered.write().await.remove(&peer_id);
                        events.send(DiscoveryEvent::PeerLost(peer_id)).ok();
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.discovered.read().await.values().cloned().collect()
    }

    fn event_stream(&self) -> mpsc::UnboundedReceiver<DiscoveryEvent> {
        // Clone receiver
        self.events.subscribe()
    }
}

/// Manual peer configuration (for tactical edge with known addresses)
pub struct ManualDiscovery {
    peers: Vec<PeerInfo>,
}

impl ManualDiscovery {
    pub fn new(peers: Vec<PeerInfo>) -> Self {
        Self { peers }
    }
}

#[async_trait]
impl DiscoveryPlugin for ManualDiscovery {
    async fn start(&mut self) -> Result<()> {
        // Nothing to start - peers are static
        Ok(())
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.peers.clone()
    }

    // ... other methods
}
```

### 4. Transport Layer (Pluggable)

```rust
/// Transport plugin trait
#[async_trait]
pub trait Transport: Send + Sync {
    /// Connect to peer
    async fn connect(&self, address: &str) -> Result<Box<dyn Connection>>;

    /// Listen for incoming connections
    async fn listen(&self, address: &str) -> Result<Box<dyn Listener>>;
}

/// Connection abstraction
#[async_trait]
pub trait Connection: Send + Sync {
    /// Send message
    async fn send(&mut self, message: &[u8]) -> Result<()>;

    /// Receive message
    async fn recv(&mut self) -> Result<Vec<u8>>;

    /// Get peer ID
    fn peer_id(&self) -> PeerId;

    /// Close connection
    async fn close(&mut self) -> Result<()>;
}

/// TCP transport implementation
pub struct TcpTransport;

#[async_trait]
impl Transport for TcpTransport {
    async fn connect(&self, address: &str) -> Result<Box<dyn Connection>> {
        let stream = TcpStream::connect(address).await?;
        Ok(Box::new(TcpConnection::new(stream)))
    }

    async fn listen(&self, address: &str) -> Result<Box<dyn Listener>> {
        let listener = TcpListener::bind(address).await?;
        Ok(Box::new(TcpListener { listener }))
    }
}

/// TCP connection wrapper
pub struct TcpConnection {
    stream: TcpStream,
    peer_id: PeerId,
    read_buf: BytesMut,
}

#[async_trait]
impl Connection for TcpConnection {
    async fn send(&mut self, message: &[u8]) -> Result<()> {
        // Frame message with length prefix
        let len = message.len() as u32;
        self.stream.write_u32(len).await?;
        self.stream.write_all(message).await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>> {
        // Read length prefix
        let len = self.stream.read_u32().await? as usize;

        // Read message
        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;
        Ok(buf)
    }

    fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    async fn close(&mut self) -> Result<()> {
        self.stream.shutdown().await?;
        Ok(())
    }
}
```

### 5. Sync Engine (Orchestration)

```rust
/// Sync engine coordinates synchronization across peers
pub struct SyncEngine {
    peer_manager: Arc<PeerManager>,
    sync_states: Arc<RwLock<HashMap<(DocumentId, PeerId), SyncState>>>,
    change_subscribers: Arc<RwLock<HashMap<String, Vec<mpsc::UnboundedSender<Change>>>>>,
}

impl SyncEngine {
    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        Self {
            peer_manager,
            sync_states: Arc::new(RwLock::new(HashMap::new())),
            change_subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start sync loop for all connected peers
    pub async fn start(&self) -> Result<()> {
        let peers = self.peer_manager.connected_peers().await;

        for peer_id in peers {
            self.start_peer_sync(peer_id).await?;
        }

        // Subscribe to peer events
        let mut events = self.peer_manager.event_stream();
        let engine = self.clone();

        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                match event {
                    PeerEvent::Connected(peer_id) => {
                        engine.start_peer_sync(peer_id).await.ok();
                    }
                    PeerEvent::Disconnected(peer_id) => {
                        engine.stop_peer_sync(peer_id).await.ok();
                    }
                }
            }
        });

        Ok(())
    }

    /// Sync a specific document with peer
    async fn sync_document(
        &self,
        doc: &mut Document,
        peer_id: PeerId,
        conn: &mut dyn Connection,
    ) -> Result<()> {
        // Get sync state for this doc/peer pair
        let mut sync_states = self.sync_states.write().await;
        let sync_state = sync_states
            .entry((doc.id, peer_id))
            .or_insert_with(|| SyncState::new());

        // Generate sync message
        let message = doc.generate_sync_message(sync_state)?;

        // Send to peer
        conn.send(&message).await?;

        // Receive response
        let response = conn.recv().await?;

        // Apply changes
        doc.receive_sync_message(&response)?;

        // Notify subscribers of changes
        self.notify_subscribers(&doc.collection, doc).await;

        Ok(())
    }

    /// Subscribe to changes in a collection
    pub fn subscribe_collection(&self, collection: &str) -> ChangeStream {
        let (tx, rx) = mpsc::unbounded_channel();

        self.change_subscribers
            .write()
            .unwrap()
            .entry(collection.to_string())
            .or_insert_with(Vec::new)
            .push(tx);

        ChangeStream { receiver: rx }
    }

    /// Broadcast document to all peers
    pub async fn broadcast_document(&self, doc: &Document) -> Result<()> {
        let peers = self.peer_manager.connected_peers().await;

        for peer_id in peers {
            if let Some(mut conn) = self.peer_manager.get_connection(peer_id).await {
                self.sync_document(&mut doc.clone(), peer_id, &mut *conn).await?;
            }
        }

        Ok(())
    }
}
```

## Integration with Security (ADR-006)

Security integrates at multiple layers:

```rust
impl Repository {
    /// Create repository with security enabled
    pub async fn new_secure(
        path: impl AsRef<Path>,
        security_config: SecurityConfig,
    ) -> Result<Self> {
        let mut repo = Self::new(path).await?;

        // Initialize security manager
        let security = SecurityManager::new(security_config)?;
        repo.security = Some(Arc::new(security));

        // Wrap transport with TLS
        let tls_transport = TlsTransport::wrap(TcpTransport::new(), security.clone());
        repo.peers.set_transport(Box::new(tls_transport)).await;

        Ok(repo)
    }
}

/// Secure collection wrapper
impl Collection {
    /// Insert with authorization check
    pub async fn insert_secure(
        &self,
        data: serde_json::Value,
        entity: &AuthenticatedEntity,
    ) -> Result<DocumentId> {
        // Check authorization
        if let Some(security) = &self.repo.security {
            security.authorize(entity, Permission::WriteCollection, &self.name)?;
        }

        // Encrypt document
        let encrypted = if let Some(security) = &self.repo.security {
            security.encrypt_document(&data)?
        } else {
            data
        };

        // Store
        self.insert(encrypted).await
    }
}
```

## CAP Protocol Integration

CAP Protocol uses `automerge-edge` as a library:

```rust
// In cap-protocol/Cargo.toml
[dependencies]
automerge-edge = { version = "0.1", features = ["security", "priority-sync"] }

// In cap-protocol/src/storage/mod.rs
use automerge_edge::{Repository, Collection};

pub struct CellStore {
    repo: Arc<Repository>,
    collection: Collection,
}

impl CellStore {
    pub async fn new(repo: Arc<Repository>) -> Result<Self> {
        Ok(Self {
            collection: repo.collection("cells"),
            repo,
        })
    }

    pub async fn store_cell(&self, cell: &CellState) -> Result<String> {
        let data = serde_json::to_value(cell)?;
        let doc_id = self.collection.insert(data).await?;
        Ok(doc_id.to_string())
    }

    pub async fn get_cell(&self, cell_id: &str) -> Result<Option<CellState>> {
        let query = format!("config.id == '{}'", cell_id);
        let doc = self.collection.find_one(&query).await?;
        Ok(doc.map(|d| serde_json::from_value(d)).transpose()?)
    }

    pub async fn set_leader(&self, cell_id: &str, leader_id: String) -> Result<()> {
        let query = format!("config.id == '{}'", cell_id);
        let update = serde_json::json!({ "leader_id": leader_id });
        self.collection.update(&query, update).await?;
        Ok(())
    }
}
```

## Migration Strategy

### Phase 1: Create Standalone Crate (Weeks 1-8)

**Goal**: Build `automerge-edge` crate with basic functionality

Tasks:
- [ ] Set up standalone crate structure
- [ ] Integrate Automerge for CRDTs
- [ ] Implement Repository and Collection APIs
- [ ] Add RocksDB storage backend
- [ ] Implement TCP transport
- [ ] Implement mDNS discovery
- [ ] Write comprehensive tests
- [ ] **Milestone**: Two processes can sync documents over TCP

### Phase 2: Feature Parity with Ditto (Weeks 9-16)

**Goal**: Match capabilities currently used by CAP Protocol

Tasks:
- [ ] Collection queries (find, find_one, update)
- [ ] Observable changes (ChangeStream)
- [ ] Peer lifecycle management
- [ ] Connection recovery and retry
- [ ] **Milestone**: All CAP storage tests pass with automerge-edge

### Phase 3: Add Security (Weeks 17-20)

**Goal**: Integrate security from ADR-006

Tasks:
- [ ] Device authentication (PKI)
- [ ] TLS transport wrapper
- [ ] Authorization checks
- [ ] Encrypted storage
- [ ] **Milestone**: Secure sync with authenticated peers

### Phase 4: Replace Ditto in CAP Protocol (Weeks 21-24)

**Goal**: Complete migration

Tasks:
- [ ] Update CellStore to use automerge-edge
- [ ] Update NodeStore to use automerge-edge
- [ ] Update E2E tests
- [ ] Remove Ditto dependency
- [ ] Performance benchmarks (vs Ditto baseline)
- [ ] **Milestone**: CAP Protocol fully operational without Ditto

### Phase 5: Publish and Promote (Weeks 25+)

**Goal**: Make available to broader community

Tasks:
- [ ] Publish to crates.io
- [ ] Write comprehensive documentation
- [ ] Create tutorial and examples
- [ ] Blog post announcing Ditto alternative
- [ ] Engage with Automerge community

## Advantages of This Approach

### Technical Benefits

1. **Better CRDT Foundation**: Automerge's columnar encoding is superior to Ditto's CBOR
2. **Cleaner Architecture**: Direct integration, no abstraction overhead
3. **Testability**: Easy to mock, no SDK dependencies
4. **Transparency**: Full source code visibility and control
5. **Performance**: Can optimize for CAP's specific access patterns

### Business Benefits

1. **No Licensing Costs**: Open-source, permissive license
2. **No Vendor Lock-in**: Own the entire stack
3. **Reusable Asset**: Can be used in other projects
4. **Community Building**: Potential for external contributors
5. **Competitive Advantage**: Differentiated IP

### Ecosystem Benefits

1. **Fills Gap**: Automerge lacks networking/discovery
2. **General Purpose**: Useful beyond CAP Protocol
3. **Production Ready**: Unlike many CRDT research projects
4. **Modern Rust**: Idiomatic, async, type-safe
5. **Open Source**: Apache-2.0 or MIT license

## Risks and Mitigations

### Risk 1: Development Time

**Risk**: Building from scratch takes longer than using Ditto
**Mitigation**:
- Incremental development (MVP first)
- Leverage existing crates (Automerge, RocksDB, mDNS)
- Keep Ditto reference for behavior comparison

### Risk 2: Feature Gap

**Risk**: Missing advanced Ditto features (offline sync, conflict resolution)
**Mitigation**:
- Automerge handles CRDTs and conflict resolution
- Only implement features CAP actually needs
- Iterative development based on requirements

### Risk 3: Testing Complexity

**Risk**: Need to test P2P mesh behavior thoroughly
**Mitigation**:
- Learn from Ditto E2E test patterns
- Use test harness with multiple in-process instances
- Property-based testing for CRDT invariants

### Risk 4: Performance

**Risk**: Custom implementation may be slower than Ditto
**Mitigation**:
- Benchmark against Ditto throughout development
- Profile and optimize hot paths
- Automerge's columnar encoding is proven efficient

## Success Criteria

1. **Feature Complete**: Matches Ditto capabilities used by CAP
2. **Performance Equivalent**: Within 20% of Ditto on key metrics
3. **Test Coverage**: 80%+ coverage, all E2E tests passing
4. **Documentation**: Complete API docs and tutorials
5. **Zero Ditto Dependency**: CAP Protocol compiles without Ditto
6. **Reusability**: At least one example of non-CAP usage

## References

**CRDT Libraries:**
- [Automerge Repository](https://github.com/automerge/automerge) - CRDT foundation (Option 1)
- [Automerge 2.0 Blog Post](https://automerge.org/blog/2023/11/06/automerge-2/) - Columnar encoding
- [Loro Repository](https://github.com/loro-dev/loro) - CRDT foundation (Option 2)
- [Loro 1.0 Announcement](https://loro.dev/blog/v1.0) - Performance improvements
- [Loro Documentation](https://loro.dev/docs) - API reference and guides
- [CRDT Benchmarks](https://github.com/dmonad/crdt-benchmarks) - Performance comparisons

**CAP Protocol:**
- [CAP_Rust_Implementation_Plan.md](../CAP_Rust_Implementation_Plan.md) - Detailed design
- [ADR-006](006-security-authentication-authorization.md) - Security integration
- [ADR-009](009-bidirectional-hierarchical-flows.md) - Hierarchical communication

**Reference Implementations:**
- [Ditto SDK Documentation](https://docs.ditto.live/) - Reference for feature parity (deprecated)

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-11-04 | Rip-and-replace Ditto with open-source CRDT | Licensing constraints, better architecture, GOTS positioning |
| 2025-11-05 | Evaluate both Automerge and Loro in E8 | No sunk cost, different strengths, data-driven decision needed |
| 2025-11-05 | Design backend-agnostic abstraction | Supports dual evaluation, future-proofs architecture |
| TBD (E8 End) | Select Automerge OR Loro | Based on CAP-specific benchmark results |
| TBD | Approved/Rejected | After team review and stakeholder alignment |

## Strategic Value Proposition

### For US Department of Defense

**Immediate Benefits**:
- Eliminate proprietary licensing costs ($X million saved over program lifecycle)
- Faster ATO/security certification (full source review)
- Sovereign control over critical autonomy infrastructure
- No vendor dependency for mission-critical systems

**Long-term Benefits**:
- Foundation for DoD-wide autonomous systems coordination
- Reference implementation for Modular Open Systems Approach (MOSA)
- Competitive market for integration and support contractors
- Technology advantage without vendor lock-in

### For NATO Allies

**Interoperability**:
- Common protocol for multi-national autonomous operations
- No licensing barriers for coalition partners
- Each nation can adapt to national doctrine
- Shared investment in common capability

**Industrial Base**:
- European defense contractors can integrate and support
- Stimulates allied autonomous systems development
- Technology transfer without ITAR complications
- Jobs and capability development in member nations

### For Defense Industry

**Prime Contractors**:
- Freedom to integrate without licensing negotiations
- Can offer differentiated solutions on common foundation
- Reduced program risk from vendor dependencies
- Competitive advantage with proven technology

**Small/Medium Enterprises**:
- Lower barriers to entry (no SDK licensing costs)
- Can innovate on proven foundation
- Compete for government contracts
- Contribute to standardization process

### For Open Source Ecosystem

**Rust Community**:
- Production-grade P2P sync library
- Real-world CRDT implementation at scale
- Embedded/edge computing example
- Security and cryptography patterns

**Automerge Project**:
- Networking layer contribution
- Discovery and transport extensions
- Production validation and feedback
- Expanded user base and use cases

### For Academia

**Research Value**:
- Open platform for CRDT research
- Distributed systems experimentation
- Human-autonomy teaming studies
- Coalition coordination algorithms

**Educational Value**:
- Real-world distributed systems example
- Security and cryptography case study
- Government software development model
- Open architecture principles

## Recommended Next Steps

### Immediate - E8 Evaluation Sprint (Next 2 Weeks)

**Week 1: Design and Parallel Implementation**

1. **Days 1-3: Abstraction Design**
   - Define `CrdtBackend` trait with CAP-specific operations
   - Design metrics collection framework
   - Create shared test scenarios
   - Team review and approval

2. **Days 4-7: Automerge Implementation**
   - Implement `AutomergeBackend` with all trait methods
   - Unit tests for Automerge backend
   - Integration with CAP data models

3. **Days 4-7: Loro Implementation** (parallel)
   - Implement `LoroBackend` with all trait methods
   - Unit tests for Loro backend
   - Test native tree structure for hierarchy

4. **Days 8-10: Integration Testing**
   - Cross-backend sync tests
   - CAP scenario tests (both backends)
   - Bug fixes and refinements

**Week 2: Benchmarking and Decision**

5. **Days 11-12: Benchmark Execution**
   - Run all CAP-specific benchmarks
   - Collect performance metrics
   - Measure bandwidth, latency, memory, CPU

6. **Days 13-14: Analysis and Selection**
   - Populate decision matrix
   - Team review of results
   - Select winning backend
   - Document decision rationale

7. **Day 15-16: Documentation Update**
   - Update ADR-007 with final choice
   - Remove non-selected backend code
   - Update architecture diagrams
   - Plan integration with CAP Protocol

### Short-term - Post-E8 (Weeks 3-12)

**Weeks 3-8: Core Development**
1. **Crate Development**
   - Build `cap-sync-engine` (or final name) on selected backend
   - Implement networking layer (TCP transport, mDNS discovery)
   - Add security layer (TLS, authentication from ADR-006)
   - Comprehensive testing (unit, integration, E2E)

2. **CAP Integration** 
   - Port CellStore/NodeStore to new sync engine
   - Update hierarchical operations
   - Migrate capability aggregation logic
   - Validate against existing CAP tests

**Weeks 9-12: Production Hardening**
3. **Performance Optimization**
   - Profile hot paths
   - Optimize delta generation
   - Tune memory usage
   - Benchmark at scale (100+ nodes)

4. **Documentation**
   - API documentation
   - Integration guide for CAP
   - Example applications
   - Migration guide from Ditto

5. **Remove Ditto**
   - Delete all Ditto dependencies
   - Remove Ditto examples (archive separately)
   - Update build configuration
   - Final E2E validation

### Medium-term - Community and Government (6-12 Months)

1. **Stakeholder Alignment**
   - Present results to government sponsors
   - Discuss GOTS strategy and benefits
   - Identify NATO contacts for coordination
   - Engage with selected CRDT project maintainers

2. **Open Source Release**
   - Select license (recommend: Apache-2.0)
   - Publish to crates.io
   - Set up GitHub repository (if not already public)
   - Define contribution process

3. **Community Building**
   - Present at RustConf / EuroRust
   - Blog posts about architecture and lessons learned
   - Engage with Rust embedded community
   - Attract early adopters

4. **Government Engagement**
   - Present at DoD software conferences
   - Demo at NATO NIAG meetings
   - Coordinate with program offices
   - Identify pilot programs beyond CAP

### Long-term - Standardization (1-3 Years)

1. **NATO STANAG Path** (Years 1-2)
   - Demonstrate in CAP Protocol deployments
   - Conduct multi-national trials
   - Gather feedback from allied systems
   - Publish technical specification

2. **Standardization** (Years 2-3)
   - Work with NATO Standardization Office
   - Draft STANAG proposal
   - Define conformance requirements
   - Ratification and adoption process

3. **Ecosystem Growth**
   - Support contractor integrations
   - Enable academic research
   - Foster community contributions
   - Expand use cases beyond tactical autonomy

4. **Sustainment**
   - Establish governance model
   - Define support options
   - Plan evolution roadmap
   - Ensure long-term viability

## Open Questions

### E8 Evaluation Phase Questions

1. **What if benchmark results are inconclusive?**
   - If both backends perform similarly across all metrics
   - Option: Choose Automerge for lower risk (maturity)
   - Option: Choose Loro for future performance potential
   - Recommendation: **Automerge** if tie (proven stability for DoD certification)

2. **What if neither backend meets performance requirements?**
   - If both fail critical benchmarks (e.g., >5% CPU or >1GB memory)
   - Option: Optimize implementation and re-test
   - Option: Consider hybrid approach (different backends for different operations)
   - Fallback: Keep Ditto temporarily, reassess CRDT approach
   - Recommendation: **Optimize first** - likely implementation issue, not fundamental limitation

3. **Should E8 timeline slip if evaluation incomplete?**
   - Trade-off: Rush decision vs. delay E8
   - Recommendation: **Make decision with partial data** if needed - can revisit in E9
   - Both are viable; imperfect choice beats paralysis

4. **What if we discover showstopper bug during evaluation?**
   - In either Automerge or Loro implementation
   - Action: File issue with upstream project
   - Recommendation: Continue with other backend, monitor issue resolution

### Post-Selection Questions

5. **Should we fork the selected CRDT or use as-is?**
   - Risk: Breaking changes in upstream
   - Option: Fork and vendor for stability
   - Recommendation: **Use as dependency initially**, fork only if:
     - Upstream makes breaking changes without migration path
     - Need to apply patches that won't be accepted upstream
     - DoD requires full control for certification

6. **What license should cap-sync-engine use?**
   - Apache-2.0 (matches both Automerge and Loro)
   - MIT for maximum permissiveness
   - Recommendation: **Apache-2.0** (DoD-friendly, NATO-compatible, patent protection)

7. **Should we target crates.io publication from day one?**
   - Or keep private until proven in CAP Protocol?
   - Recommendation: **Public from day one**:
     - Builds community early
     - Attracts contributors
     - Demonstrates commitment to OSS/GOTS
     - Enables external security audits

8. **How to handle schema evolution?**
   - Both CRDTs are schema-less, but CAP models are typed
   - Need versioning strategy for breaking changes
   - Recommendation: 
     - Use semantic versioning
     - Document migration paths
     - Test backward compatibility
     - Provide schema validation layer

9. **Should we contribute improvements back to upstream?**
   - e.g., networking layer, discovery, CAP-specific optimizations
   - Recommendation: **Yes** - but:
     - Keep networking/transport as separate modules first
     - Coordinate with maintainers before large PRs
     - Maintain good upstream relationship
     - Only contribute general-purpose code (not CAP-specific)

10. **When to engage NATO Standardization Office?**
    - Too early risks premature specification
    - Too late misses opportunity for input
    - Recommendation: **Year 2** after:
      - Proving capability in CAP Protocol
      - Gathering performance data
      - Before architecture solidifies
      - When ready for multi-national trials

11. **How to balance CAP-specific vs general-purpose?**
    - cap-sync-engine should be reusable
    - CAP-specific features in separate layer
    - Recommendation:
      - Keep sync engine pure and general-purpose
      - Build `cap-protocol-core` on top with CAP-specific logic
      - Hierarchical operations as CAP module, not sync engine feature
      - Enables broader adoption beyond CAP

12. **What if we need features from BOTH backends?**
    - E.g., Automerge compression + Loro trees
    - Option: Hybrid approach (use both for different data)
    - Option: Contribute tree CRDT to Automerge
    - Recommendation: **Single backend for simplicity** - complexity not worth marginal gains

13. **How to handle the non-selected backend's advantages?**
    - If we choose Automerge, we lose Loro's speed
    - If we choose Loro, we lose Automerge's compression
    - Recommendation:
      - **Optimize implementation** - may close gap
      - **Learn from other's design** - apply techniques
      - **Re-evaluate in 12 months** - technology evolves
      - Keep abstraction layer in case future swap needed

---

## Summary: Path Forward

### The Decision Framework

**What We're Deciding**: Choose between Automerge and Loro as the CRDT foundation for CAP Protocol's sync engine.

**Why This Matters**: 
- Eliminates Ditto licensing constraints (both options)
- Enables GOTS/OSS positioning for DoD/NATO (both options)
- Different performance trade-offs affect tactical operations
- Long-term maintainability and certification risk

**How We'll Decide**: Empirical evaluation during E8
1. Design backend-agnostic abstraction
2. Implement both backends
3. Run CAP-specific benchmarks
4. Select based on measured performance

**When We'll Decide**: End of E8 (2 weeks)

### Expected Outcomes

**Most Likely Scenario**: 
- Loro wins on raw performance (3-15x faster)
- Automerge wins on compression (44% smaller documents)
- Decision hinges on which constraint dominates:
  - **Bandwidth-constrained** → Automerge
  - **CPU-constrained** → Loro
  - **Hierarchy-heavy** → Loro (native trees)
  - **Risk-averse** → Automerge (more mature)

**Confidence Level**: High for both as viable options
- Both are production-ready open-source CRDTs
- Both eliminate Ditto licensing issues
- Both support GOTS strategy
- Either choice is defensible

**Risk Mitigation**:
- Backend abstraction allows future swap if needed
- 2-week evaluation prevents analysis paralysis
- Can re-evaluate in 12 months as technology matures

### Key Success Criteria

**E8 Evaluation Success**:
- ✅ Both backends implemented and tested
- ✅ CAP benchmarks complete with results
- ✅ Decision made with clear rationale
- ✅ Team alignment on selection

**Long-term Success**:
- ✅ CAP Protocol operational without Ditto
- ✅ Performance meets tactical requirements
- ✅ DoD/NATO certification achievable
- ✅ Community adoption and contributions
- ✅ Path to NATO STANAG established

### Commitment

This ADR commits to:
1. **Eliminate Ditto** - Licensing constraints make this non-negotiable
2. **Open-source approach** - GOTS positioning for government adoption
3. **Data-driven decision** - Select backend based on CAP-specific measurements
4. **2-week evaluation** - Balance thoroughness with decisiveness
5. **Pragmatic choice** - Either Automerge or Loro will work; imperfect choice beats paralysis

**Next Action**: Begin E8 evaluation sprint (Week 1, Day 1)

---

## E8 Evaluation Results (2025-11-06)

### Evaluation Summary

**Status**: Evaluation Complete - **Decision: Continue with Ditto**

The E8 evaluation successfully implemented and tested an AutomergeBackend to understand the gap between raw CRDT libraries and production-ready distributed systems. While Automerge 0.7.1 provides excellent CRDT sync primitives, the evaluation revealed a critical insight: **the CRDT sync protocol is only 20% of the problem; the network mesh layer is the other 80%**.

**Commit**: 94b7f10 "E8: Implement Automerge backend (Phase 1)"
- 750 lines of AutomergeBackend implementation
- All 15 tests passing (5 unit + 10 integration)
- Full DataSyncBackend trait implementation
- Benchmark suite created

### What We Built (AutomergeBackend Phase 1)

**Achievements**:
- ✅ Document storage with Automerge CRDTs
- ✅ CRDT sync protocol (`generate_sync_message` / `receive_sync_message`)
- ✅ Per-peer sync state management
- ✅ Document conversion layer (CAP models ↔ Automerge)
- ✅ Full trait compliance (DocumentStore, SyncEngine, PeerDiscovery stub, DataSyncBackend)
- ✅ Integration with CellStore/NodeStore

**What's Missing (The Network Gap)**:
- ❌ TCP transport layer (sockets, framing, connection management)
- ❌ Peer discovery (mDNS, Bluetooth LE)
- ❌ Mesh construction and maintenance
- ❌ Background sync coordination
- ❌ Multi-transport support
- ❌ Connection recovery and failover
- ❌ Multi-hop routing
- ❌ Backpressure and flow control

### The Network Stack Reality

This evaluation exposed a fundamental truth: **Ditto's value proposition is not the CRDT (Automerge is excellent), but the production-ready mesh networking stack**.

#### What Ditto Provides Beyond CRDT Sync

**1. Automatic Peer Discovery**
- **mDNS (Multicast DNS)**: Local network service discovery without infrastructure
- **Bluetooth LE**: Ad-hoc discovery in disconnected environments
- **Background scanning**: Continuous discovery without explicit user action
- **Service registration**: Automatic advertisement of node capabilities

**Ditto Implementation**: Built-in, multi-platform, battle-tested across thousands of deployments

**Automerge Gap**: None - would require 1-2 weeks to implement using `mdns-sd` and `btleplug` crates

**2. Mesh Construction and Maintenance**
- **Multi-transport coordination**: Simultaneous TCP + Bluetooth + mDNS connections
- **Topology adaptation**: Automatically adjusts to network changes
- **Connection pooling**: Manages multiple peer connections efficiently
- **Mesh healing**: Detects and recovers from network partitions
- **Transport preference**: Selects fastest available transport automatically

**Ditto Implementation**: Proprietary mesh protocol with 8+ years of development and field testing

**Automerge Gap**: 2-3 weeks to implement basic version, months for production-grade reliability

**3. Multi-Hop Routing**
- **Store-and-forward**: Messages route through intermediate peers
- **Path discovery**: Finds optimal routes through mesh topology
- **Loop prevention**: Avoids circular message propagation
- **TTL management**: Controls message lifetime and flooding

**Ditto Implementation**: Integrated into transport layer, handles NAT traversal

**Automerge Gap**: Not planned for E9 - would add 2-4 weeks and significant complexity

**4. Connection Lifecycle Management**
- **Automatic reconnection**: Exponential backoff on disconnects
- **Keepalive/heartbeat**: Detects dead connections quickly
- **Graceful degradation**: Continues operating with partial connectivity
- **Resource cleanup**: Prevents connection leaks
- **Timeout management**: Configurable per transport type

**Ditto Implementation**: Robust state machine handling edge cases (network switches, sleep/wake, airplane mode)

**Automerge Gap**: 1 week for basic implementation, ongoing work for edge cases

**5. Background Sync Coordination**
- **Task scheduling**: Coordinates sync operations across peers
- **Prioritization**: Syncs critical data first (e.g., CellState before telemetry)
- **Bandwidth management**: Throttles sync to avoid congestion
- **Conflict-free ordering**: Ensures deterministic merge behavior
- **Change notification**: Efficiently detects and propagates updates

**Ditto Implementation**: Built on Tokio, integrated with CRDT layer

**Automerge Gap**: 1-2 weeks for basic coordinator (see E9 Phase 1-3)

**6. Platform Integration**
- **iOS/Android/Desktop**: Native bindings for all platforms
- **Battery optimization**: Power-aware Bluetooth scanning
- **Network change handling**: Adapts to WiFi ↔ cellular ↔ offline transitions
- **Background execution**: Continues syncing when app is backgrounded
- **Privacy controls**: User-facing permissions management

**Ditto Implementation**: Years of mobile platform work

**Automerge Gap**: Platform-specific - would require native development expertise

#### E9 Implementation Plan: 3-4 Weeks to Basic Parity

See `docs/E9-NETWORK-TRANSPORT-LAYER-PLAN.md` for full details:

**Phase 1 (Week 1)**: TCP Transport Foundation
- WireProtocol (length-prefixed framing)
- Connection module (TcpStream wrapper)
- ConnectionManager (listener, pool)
- Manual peer addition (parse address, connect, handshake)
- SyncCoordinator (background sync task)

**Phase 2 (Week 2)**: mDNS Discovery
- mDNS service registration (`_automerge-cap._tcp.local`)
- Automatic peer discovery
- Connection pool integration

**Phase 3 (Week 3)**: Sync Optimization
- Selective sync (only changed documents)
- Backpressure handling (bounded queues)
- Connection recovery (reconnection logic)
- Performance tuning

**Phase 4 (Week 4, Optional)**: Bluetooth Support
- Bluetooth LE discovery
- Multi-transport coordination
- Transport fallback logic

**Total Effort**: 3-4 weeks for basic functionality, **months for production-grade reliability comparable to Ditto**

### Decision Rationale: Continue with Ditto

**Primary Reasons**:

1. **Network Stack Complexity Dominates CRDT Choice**
   - CRDT sync protocol: ~750 lines, 1 week of work (✅ complete in E8)
   - Network mesh layer: ~2000+ lines, 3-4 weeks minimum (❌ not started)
   - Production hardening: Months of edge case testing
   - **Verdict**: 80% of value is in the mesh, not the CRDT

2. **Ditto's Mesh is Battle-Tested**
   - 8+ years of development
   - Deployed in thousands of production apps
   - Mobile platform expertise (iOS, Android)
   - Multi-transport coordination already solved
   - Known failure modes and recovery strategies
   - **Verdict**: Re-implementing this is high risk with limited upside

3. **E9 Implementation is Substantial Work**
   - 3-4 weeks for basic TCP + mDNS
   - No multi-hop routing (limits tactical scenarios)
   - No Bluetooth support in Phase 1
   - Platform integration not addressed
   - Edge cases (NAT, sleep/wake, network changes) would take months
   - **Verdict**: Resource cost outweighs licensing concern at this stage

4. **Licensing Constraint is Future Problem**
   - CAP Protocol is pre-production (no deployments yet)
   - Ditto licensing negotiable for government/defense use
   - NATO STANAG timeline is 12-24 months out
   - Abstraction layer makes future swap feasible
   - **Verdict**: Defer licensing decision until closer to production deployment

5. **Focus on CAP Protocol Innovation**
   - CAP's value is in hierarchical capability composition, not CRDT implementation
   - Building network stack diverts from core research
   - Ditto enables faster iteration on cell formation logic
   - E2E tests validate distributed behavior, not transport internals
   - **Verdict**: Use best available tools, focus on novel contributions

**Secondary Considerations**:

- **Automerge document size advantage**: Relevant for bandwidth, but TCP compression can close gap
- **Loro performance advantage**: Not evaluated, but less mature ecosystem
- **GOTS positioning**: Valid long-term concern, but premature for research phase
- **Open-source preference**: Philosophical, but pragmatism wins for now

### Updated Strategy

**Immediate (Next 3 months)**:
- ✅ Continue with Ditto for all CAP Protocol development
- ✅ Focus on hierarchical composition rules (E6, E7)
- ✅ Validate cell formation logic with E2E tests
- ✅ Document Ditto-specific assumptions for future portability

**Mid-term (6-12 months)**:
- Evaluate Ditto licensing for government use cases
- Monitor Automerge/Loro ecosystem maturity
- Keep abstraction layer clean for potential backend swap
- Consider E9 implementation if licensing becomes blocker

**Long-term (12-24 months)**:
- Revisit CRDT backend decision before NATO STANAG submission
- Assess feasibility of GOTS version with Automerge + E9 network layer
- Evaluate commercial licensing vs open-source alternatives
- Make final call based on deployment requirements

### Lessons Learned

1. **CRDT libraries ≠ Distributed systems**: Automerge provides excellent CRDT primitives, but building a production mesh requires significant additional engineering.

2. **Network stack is underestimated**: The 3-4 week E9 estimate is for basic functionality; production-grade reliability (handling NAT, network transitions, battery optimization) takes months.

3. **Abstraction layer validated**: The DataSyncBackend abstraction successfully isolated CRDT concerns, making this evaluation possible. Swapping backends in the future is feasible.

4. **Benchmarks are valuable**: Creating CAP-specific benchmarks (CellState, NodeConfig) provides data for future decisions, even if not used immediately.

5. **Mesh features matter for tactical networks**: Multi-hop routing, Bluetooth support, and automatic recovery are critical for military use cases. Ditto's mesh handles these; basic TCP doesn't.

### Archive: AutomergeBackend Implementation

The AutomergeBackend implementation (commit 94b7f10) is preserved in the repository as:
- Reference implementation for future CRDT work
- Proof that backend abstraction works
- Benchmark baseline for performance comparisons
- Starting point if E9 is needed later

**Key files**:
- `cap-protocol/src/sync/automerge.rs` - Full backend implementation
- `cap-protocol/tests/automerge_backend_integration.rs` - Integration tests
- `cap-protocol/benches/backend_comparison.rs` - Performance benchmarks
- `docs/E9-NETWORK-TRANSPORT-LAYER-PLAN.md` - Network layer implementation plan

---

**Last Updated**: 2025-11-06
**Status**: Decided - Continue with Ditto (E8 Evaluation Complete)
**Review Date**: 6 months (2025-05-06) - Reassess licensing and GOTS requirements
