# PEAT Protocol Development Roadmap

**Last Updated**: November 21, 2025
**GitHub Issues**: [View All Epics](https://github.com/defenseunicorns/peat/labels/epic)
**Timeline Visualization**: [Issue #118](https://github.com/defenseunicorns/peat/issues/118)

---

## 🎯 Executive Summary

The PEAT Protocol has a clear 12-month development pathway to achieve:
- **O(n log n) hierarchical coordination** at tactical scale (200+ nodes)
- **95%+ bandwidth reduction** through differential updates and aggregation
- **Security-ready tactical deployment** with PKI and encryption
- **AUKUS interoperability** via TAK/CoT integration
- **Path to NATO STANAG** standardization

**Critical Insight**: The project is 70% complete for research/demo with Ditto backend, but needs **EPIC 2 (P2P Mesh Intelligence)** to achieve true hierarchical coordination.

---

## 📊 Current Status

### What's Working ✅
- Core protocol phases (discovery, cell formation, hierarchical operations)
- Backend abstraction layer (can switch Ditto ↔ AutomergeIroh)
- 9 E2E tests passing (flakiness resolved as of Nov 21, 2025)
- Protobuf schema migration complete
- Network simulation with 4 topology modes
- Ditto backend: Production-ready

### What's In Progress 🚧
- AutomergeIroh backend: 70% complete (Phase 7: mDNS discovery)
- Documentation: 21 ADRs, comprehensive technical docs

### Critical Gaps ⚠️
- **P2P Mesh Intelligence** (80% of the problem Iroh doesn't solve)
- Security & Authentication (no auth currently - unacceptable for tactical)
- AI Model Capability Advertisement (customer requirement)
- QoS & Prioritization (customer requirement)
- TAK Integration (AUKUS requirement)

---

## 🗺️ Development Epics

### Immediate Priority (Next 3 Months)

#### ⚠️ EPIC 2: P2P Mesh Intelligence Layer - **CRITICAL PATH**
**GitHub**: [#105](https://github.com/defenseunicorns/peat/issues/105)
**Priority**: P0 (Blocks Everything)
**Timeline**: 12 weeks
**Status**: 0% Complete (NEW WORK)

**What It Solves**: The 80% of P2P mesh coordination that Iroh doesn't provide:
- Peer discovery (mDNS, static config, relay)
- Geographic beacon system with geohash clustering
- Hierarchical topology management (automatic parent/child relationships)
- Selective routing and data aggregation
- Mesh healing and failover

**Detailed Tasks**:
- [#113: Discovery Strategies Implementation](https://github.com/defenseunicorns/peat/issues/113) (Weeks 1-2)
- [#114: Geographic Beacon System](https://github.com/defenseunicorns/peat/issues/114) (Weeks 3-4)
- [#115: Hierarchical Topology Management](https://github.com/defenseunicorns/peat/issues/115) (Weeks 5-7)
- [#116: Data Flow Control & Routing](https://github.com/defenseunicorns/peat/issues/116) (Weeks 8-10)
- [#117: Mesh Healing & Resilience](https://github.com/defenseunicorns/peat/issues/117) (Weeks 11-12)

**Success Criteria**:
- 100 nodes form 4-level hierarchy automatically
- Platform telemetry aggregates at each level
- Bandwidth < O(n log n) proven in Containerlab
- Mesh recovers from node failures within 10 seconds

---

#### EPIC 1: Backend Strategy & Parity Validation
**GitHub**: [#104](https://github.com/defenseunicorns/peat/issues/104)
**Priority**: P0 (Foundation)
**Timeline**: 2-4 weeks
**Status**: 70% Complete

**Decision Point**: 6-month review (May 2026) - Continue Ditto OR migrate to AutomergeIroh based on GOTS requirements

---

#### EPIC 3: Security & Authorization Framework
**GitHub**: [#106](https://github.com/defenseunicorns/peat/issues/106)
**Priority**: P1 (Required for Tactical Deployment)
**Timeline**: 16 weeks
**Status**: 0% Complete (Can parallelize with EPIC 2)

**Architecture**: Multi-layer security with device PKI, RBAC, user authentication, encryption, and audit logging

---

### Short-Term (3-6 Months)

#### EPIC 4: AI Model Capability Advertisement
**GitHub**: [#107](https://github.com/defenseunicorns/peat/issues/107)
**Priority**: P1 (Customer Requirement)
**Timeline**: 8 weeks
**Depends On**: EPIC 2

**Value**: C2 can query "Which platforms have target_recognition >= v4.2?" and task accordingly

---

#### EPIC 5: Quality of Service & Data Prioritization
**GitHub**: [#108](https://github.com/defenseunicorns/peat/issues/108)
**Priority**: P1 (Customer Requirement)
**Timeline**: 10 weeks
**Depends On**: EPIC 2

**Value**: Critical contact reports sync in 4 seconds, not 4 minutes behind telemetry

---

#### EPIC 7: Advanced Networking & Transport Optimization
**GitHub**: [#110](https://github.com/defenseunicorns/peat/issues/110)
**Priority**: P2
**Timeline**: 8 weeks
**Depends On**: EPIC 2

**Value**: Multi-path QUIC validation, tactical network profiles, self-hosted relays

---

### Medium-Term (6-12 Months)

#### EPIC 6: TAK & Cursor-on-Target Integration
**GitHub**: [#109](https://github.com/defenseunicorns/peat/issues/109)
**Priority**: P2 (AUKUS Requirement)
**Timeline**: 12 weeks
**Depends On**: EPIC 2, EPIC 5

**Value**: PEAT-coordinated assets visible in ATAK, operators can control via TAK

---

#### EPIC 8: Validation & Scaling Proof
**GitHub**: [#111](https://github.com/defenseunicorns/peat/issues/111)
**Priority**: P2
**Timeline**: 8 weeks (ongoing)
**Depends On**: EPIC 2

**Value**: Empirical proof of O(n log n) scaling at 200+ nodes

---

#### EPIC 9: NATO STANAG Standardization
**GitHub**: [#112](https://github.com/defenseunicorns/peat/issues/112)
**Priority**: P3
**Timeline**: 12-24 months
**Depends On**: All technical epics

**Value**: Multi-national adoption, path to de facto standard

---

## 🔥 Critical Path Analysis

### Why EPIC 2 is the Bottleneck

**The 80/20 Rule**:
- **Iroh provides ~20%**: Point-to-point QUIC connections, multi-path networking
- **EPIC 2 provides ~80%**: Discovery, topology, routing, aggregation, healing

**Without EPIC 2**: PEAT is just an all-to-all mesh (O(n²)) - defeats the entire purpose

**With EPIC 2**: PEAT becomes true hierarchical coordination (O(n log n)) - transformational

### Dependency Chain

```
EPIC 2 (Mesh)
  ├─> EPIC 4 (AI Models) - needs hierarchical aggregation
  ├─> EPIC 5 (QoS) - needs routing layer
  ├─> EPIC 7 (Networking) - needs mesh to test
  ├─> EPIC 8 (Validation) - needs working system to validate
  └─> EPIC 6 (TAK) - needs mesh + QoS
        └─> EPIC 9 (NATO) - needs all technical work
```

**Parallelization Opportunities**:
- EPIC 1 (Backend evaluation) - different skillset
- EPIC 3 (Security) - different domain

---

## 📈 Success Metrics (12-Month Horizon)

### Technical Excellence
- [ ] 100 nodes forming 4-level hierarchy automatically
- [ ] O(n log n) bandwidth scaling proven empirically
- [ ] 95%+ bandwidth reduction via differential updates + aggregation
- [ ] < 10 second mesh healing after node failure
- [ ] Critical data syncs within 5 seconds (QoS P1)
- [ ] All communications encrypted and authenticated

### Operational Capability
- [ ] Complete ISR mission scenario (100 platforms)
- [ ] TAK integration allowing ATAK control of PEAT assets
- [ ] Model capability queries and intelligent tasking
- [ ] Network partition tolerance demonstrated
- [ ] Tactical radio network validated (25% packet loss)

### Strategic Positioning
- [ ] GOTS version operational (AutomergeIroh if selected)
- [ ] NATO engagement initiated
- [ ] AUKUS partner demonstrations completed
- [ ] Path to STANAG established
- [ ] Published performance data for customers

---

## 🎬 Next Actions

### Immediate (This Week)
1. ✅ **DONE**: Created all 9 Epic issues (#104-112)
2. ✅ **DONE**: Created detailed EPIC 2 task issues (#113-117)
3. ✅ **DONE**: Created timeline visualization (#118)
4. 🔜 **TODO**: Review and approve Epic roadmap with team
5. 🔜 **TODO**: Assign engineers to EPIC 2 (critical path)

### Next Week
1. Begin EPIC 2 implementation (Discovery Strategies)
2. Set up Containerlab test infrastructure
3. Create detailed task issues for EPIC 3 (Security)
4. Schedule 6-month backend review (calendar invite for May 2026)

### Next Month
1. EPIC 2 progress review (should be through Layer 1 Discovery)
2. Create detailed task issues for EPIC 4, 5, 6
3. Engage with customers on AI Model & QoS priorities
4. Begin NATO coordination planning

---

## 💡 Strategic Insights

### On Backend Selection (EPIC 1)
**Current Approach**: Continue with Ditto for research phase, re-evaluate in 6 months

**Ditto Advantages**:
- Battle-tested (8+ years production)
- All features working today
- Known performance characteristics

**AutomergeIroh Advantages**:
- Open source (Apache-2.0/MIT) - GOTS ready
- Multi-path QUIC with connection migration
- Superior compression (90% vs 60%)
- No vendor lock-in

**Decision Trigger**: GOTS deployment timeline. If government deployment < 12 months, accelerate AutomergeIroh completion.

---

### On P2P Mesh Intelligence (EPIC 2)
**This is not "nice to have" - it's existential**

Per ADR-017: Iroh provides excellent transport, but PEAT's value proposition is **hierarchical coordination intelligence**. Without EPIC 2:
- No geographic-based squad formation
- No automatic parent/child relationships
- No hierarchical aggregation (stuck at O(n²))
- No intelligent routing or bandwidth optimization
- No mesh healing or autonomous operation during partitions

**Bottom Line**: EPIC 2 is the difference between "another mesh protocol" and "transformational military coordination system"

---

### On Parallel Backend Development
Per CLAUDE.md: "We are not working to 'replace' Ditto. We are working to provide an alternative, pure OSS, capability that provides as close to parity as possible."

**Strategy**: Backend abstraction allows switching based on deployment:
- **Proprietary OK?** → Use Ditto (ready today)
- **GOTS required?** → Use AutomergeIroh (10-12 weeks to completion)

This de-risks both technology and licensing/procurement paths.

---

## 📚 References

### ADR Mapping
- **ADR-001**: PEAT Protocol POC Architecture → Foundation for all work
- **ADR-002**: Beacon Storage Architecture → Used in EPIC 2 (Geographic Beacons)
- **ADR-005**: Data Sync Abstraction Layer → Backend switching capability
- **ADR-006**: Security, Authentication, Authorization → EPIC 3
- **ADR-007**: Automerge-Based Sync Engine → AutomergeIroh evaluation
- **ADR-009**: Bidirectional Hierarchical Flows → Data routing architecture
- **ADR-010**: Transport Layer (UDP/TCP) → Network foundation
- **ADR-011**: Ditto vs Automerge/Iroh → Backend decision analysis (2,616 lines!)
- **ADR-012**: Schema Definition (Protobuf) → Completed, extensible schema
- **ADR-013**: Distributed Software/AI Ops → AI model deployment
- **ADR-015**: Testing Strategy → Validation approach
- **ADR-016**: TTL and Data Lifecycle → Beacon expiry, storage management
- **ADR-017**: P2P Mesh Management Discovery → EPIC 2 complete design (1,650 lines!)
- **ADR-018**: AI Model Capability Advertisement → EPIC 4
- **ADR-019**: QoS and Data Prioritization → EPIC 5
- **ADR-020**: TAK-CoT Integration → EPIC 6
- **ADR-021**: Document-Oriented Architecture → Data model
- **ADR-022**: Edge MLOps Architecture → AI model lifecycle

### GitHub Project
- **Epics**: https://github.com/defenseunicorns/peat/labels/epic
- **Timeline**: https://github.com/defenseunicorns/peat/issues/118
- **Critical Path Issues**: #113, #114, #115, #116, #117

---

## ⚠️ Risks & Mitigations

### Risk: EPIC 2 Underestimated
**Impact**: Every week of delay pushes entire roadmap
**Probability**: Medium
**Mitigation**:
- Detailed 5-task breakdown with 2-3 week sprints each
- Containerlab testing at each phase
- Early prototype to validate approach

### Risk: Containerlab Scale Limitations
**Impact**: Cannot test beyond ~200 nodes
**Probability**: Medium
**Mitigation**:
- Cloud deployment for 200+ node tests
- Mathematical modeling validated against 100-200 node empirical data
- Focus on proving O(n log n) property, not absolute node count

### Risk: mDNS Reliability in Containers
**Impact**: Discovery layer flakiness
**Probability**: Low
**Mitigation**:
- Static configuration provides fallback
- Relay-based discovery works in all scenarios
- Multiple discovery strategies (hybrid approach)

### Risk: Resource Constraints
**Impact**: Insufficient engineering capacity for 12-week critical path
**Probability**: Medium
**Mitigation**:
- Parallelize EPIC 3 (Security) with different team
- Defer EPIC 6 (TAK) if needed (P2 priority)
- Focus 100% best engineers on EPIC 2

---

## 🤝 Team Allocation Recommendations

### Team A: Mesh Intelligence (Critical Path) - **TOP PRIORITY**
- **Focus**: EPIC 2 - P2P Mesh Intelligence Layer
- **Duration**: 12 weeks
- **Skills**: Rust, distributed systems, networking, CRDT expertise
- **Size**: 2-3 senior engineers (cannot parallelize much - tight dependencies)

### Team B: Security (Parallel)
- **Focus**: EPIC 3 - Security Framework
- **Duration**: 16 weeks (can start immediately)
- **Skills**: PKI, cryptography, authorization systems
- **Size**: 1-2 engineers

### Team C: Backend Evaluation (Part-time)
- **Focus**: EPIC 1 - Benchmarking and decision support
- **Duration**: 2-4 weeks
- **Skills**: Performance testing, CRDT internals
- **Size**: 1 engineer (part-time or rotating)

### Future Teams (Post-EPIC 2)
- **Team D**: AI Models + QoS (EPIC 4, 5)
- **Team E**: TAK Integration (EPIC 6)
- **Team F**: Validation & Documentation (EPIC 8, 9)

---

**Questions?** Review [Issue #118](https://github.com/defenseunicorns/peat/issues/118) for detailed timeline and dependency graphs.
