# Iroh Integration Plan - Next Steps

## Executive Summary

Based on ADR-011 ("Ditto vs Automerge+Iroh") and experiments team feedback, we recommend a **phased approach** that prioritizes validation over premature infrastructure work.

### Current Status (2025-11-08)

**Completed:**
- ✅ E11.1: Automerge POC integration (in-memory storage)
- ✅ E11.3: Automerge hierarchical summary storage
- ✅ E12.1: Hierarchical schema definitions
- ✅ E12.2: State aggregation logic
- ✅ E11.2: Ditto hierarchical summary storage
- ✅ ADR-011: Comprehensive gap analysis for Automerge+Iroh

**Current Reality:**
- Ditto backend is **production-ready** and **feature-complete**
- Automerge backend is **POC-ready** (feature-gated, in-memory only)
- Experiments team needs to **validate Mode 3/4 first** with Ditto
- Iroh integration is **well-planned** but not yet started

## Experiments Team Recommendation (ENDORSED)

> "The protocol team should finish Mode 3/4 validation with Ditto first, then add Automerge as a
> pluggable backend. CAP Protocol should be backend-agnostic anyway - the capability filtering
> and hierarchical aggregation logic shouldn't depend on which CRDT library is used."

**This is the correct approach.** Here's why:

1. **Validation First**: Prove Mode 3/4 works before comparing backends
2. **Risk Reduction**: Ditto is commercially supported, Automerge+Iroh is experimental
3. **Resource Efficiency**: Don't split effort between protocol validation and backend development
4. **Architecture Validated**: The backend abstraction is already proven to work

## Recommended Phased Approach

### Phase 1: Mode 3/4 Validation with Ditto (PRIORITY 1)

**Timeline**: Next 4-6 weeks
**Owner**: Experiments team
**Goal**: Validate O(log n) hierarchical aggregation

**What to Do:**
1. Use DittoStore for all Mode 3 experiments
2. Follow `cap-protocol/src/storage/HIERARCHICAL_SUMMARIES.md`
3. Validate:
   - Squad-level aggregation (members → leader)
   - Platoon-level aggregation (squad leaders → platoon leader)
   - Message complexity reduction (O(n²) → O(n log n))
   - Bandwidth savings (95%+ reduction)

**Success Criteria:**
- [ ] Mode 3 (CAP Differential) demonstrates O(n log n) message complexity
- [ ] Mode 4 validation complete
- [ ] Bandwidth measurements confirm 95%+ reduction
- [ ] E2E tests pass for hierarchical scenarios

**Blockers Removed:**
- Protocol team has delivered hierarchical summary storage for both backends
- Documentation is complete
- All tests passing

### Phase 2: Iroh Networking Integration (AFTER Mode 3/4 Validated)

**Timeline**: 16-20 weeks (per ADR-011)
**Owner**: Protocol team
**Goal**: Replace Ditto with Automerge+Iroh for production deployments

**Pre-Requisites:**
- ✅ Mode 3/4 validation complete (Phase 1)
- ✅ ADR-011 approved
- ✅ Team bandwidth available

**Implementation Phases** (from ADR-011):

#### 2.1: Core Foundation (Weeks 1-4)
- [ ] Iroh endpoint setup with multi-interface support
- [ ] Self-hosted relay servers on tactical infrastructure
- [ ] Basic Automerge-Iroh sync (two nodes exchange documents)
- [ ] In-memory to RocksDB migration

**Milestone**: Two platforms sync Automerge documents over Iroh QUIC

#### 2.2: Storage & Persistence (Weeks 5-8)
- [ ] RocksDB integration (per ADR-011 Gap 1)
- [ ] Repository pattern (per ADR-011 Gap 2)
- [ ] Collection abstraction
- [ ] Document TTL support (per ADR-011 Gap 5)

**Milestone**: Multi-document sync with persistence and TTL

#### 2.3: Discovery & Connectivity (Weeks 9-10)
- [ ] mDNS discovery plugin (per ADR-011 Gap 3)
- [ ] Static config loader
- [ ] Discovery integration with Iroh

**Milestone**: Nodes discover each other automatically on LAN

#### 2.4: Query Capabilities (Weeks 11-13)
- [ ] Predicate-based query engine (per ADR-011 Gap 3)
- [ ] Sorting and filtering
- [ ] Geohash indexing (per ADR-011 Gap 4)

**Milestone**: Can query documents by location and attributes

#### 2.5: Observability (Week 14)
- [ ] Change streams via tokio::watch (per ADR-011 Gap 6)
- [ ] Observable collections
- [ ] Event bus for UI updates

**Milestone**: UI reacts to remote document changes

#### 2.6: Security Integration (Weeks 15-18)
- [ ] PKI-based device authentication (per ADR-011 Gap 7 + ADR-006)
- [ ] Authorization layer (RBAC)
- [ ] Encrypted storage
- [ ] Audit logging

**Milestone**: Secure, authenticated P2P mesh

#### 2.7: Optimization & Testing (Weeks 19-20)
- [ ] Multi-path benchmarking
- [ ] Network failure testing
- [ ] Performance optimization
- [ ] Integration test suite

**Milestone**: Full feature parity with Ditto for CAP use cases

### Phase 3: Backend Comparison & Selection (AFTER Iroh Integration)

**Timeline**: 2-4 weeks
**Goal**: Benchmark Ditto vs Automerge+Iroh

**Benchmark Scenarios** (from ADR-011):
1. High-loss tactical radio (20% packet loss)
2. Network interface handoff (Ethernet → MANET)
3. Multi-path bandwidth utilization (Starlink + MANET + 5G)
4. Delta compression efficiency

**Expected Results** (from ADR-011):
- 5x faster on lossy links (QUIC vs TCP)
- 10x faster recovery (connection migration)
- Best-of-both-worlds multi-path
- 64x smaller updates (columnar encoding)

**Decision Criteria:**
- Performance benchmarks
- Operational requirements (licensing, support)
- Long-term maintainability
- Cost considerations

## Immediate Next Steps

### For Experiments Team (Now)

1. **Use Ditto for Mode 3/4 validation** - it's production-ready
2. **Reference the hierarchical summaries guide** - `cap-protocol/src/storage/HIERARCHICAL_SUMMARIES.md`
3. **Run E2E tests** - `make test-e2e` validates hierarchical scenarios
4. **Collect measurements** - message counts, bandwidth usage

### For Protocol Team (Now)

1. **Support experiments team** - answer questions, fix bugs
2. **Monitor ADR-011 status** - wait for approval
3. **Plan Iroh integration kickoff** - identify team members, allocate time
4. **Keep Automerge backend dormant** - it's complete but not priority

### For Protocol Team (After Phase 1 Complete)

1. **Review ADR-011 with stakeholders** - get formal approval
2. **Create Iroh integration epic** - break down into stories
3. **Set up dev environment** - Iroh dependencies, relay servers
4. **Start Phase 2.1** - Core Foundation (Weeks 1-4)

## Risk Analysis

### Risk: Premature Iroh Investment

**Problem**: Starting Iroh work before Mode 3/4 validation wastes effort if hierarchical approach doesn't work

**Mitigation**: **WAIT for Phase 1 completion** ✅

### Risk: Ditto Dependency Increases

**Problem**: More Mode 3/4 work with Ditto creates more code to migrate

**Mitigation**:
- CAP Protocol logic is **already backend-agnostic** ✅
- Storage layer follows interface-based design ✅
- Migration path is clear (swap DittoStore → AutomergeStore + Iroh)

### Risk: Timeline Underestimation

**Problem**: ADR-011's 20-week timeline may be optimistic

**Mitigation**:
- Prioritize MVP features (storage, sync, basic discovery)
- Defer nice-to-haves (advanced queries, optimization)
- Can release with feature gaps and iterate
- Ditto remains available as fallback

## Success Criteria

### Phase 1 Success (Mode 3/4 Validation)
- [ ] O(n log n) message complexity demonstrated
- [ ] 95%+ bandwidth reduction measured
- [ ] E2E tests pass for hierarchical scenarios
- [ ] Experiments team confirms readiness for backend comparison

### Phase 2 Success (Iroh Integration)
- [ ] All ADR-011 gaps filled
- [ ] Feature parity with Ditto for CAP use cases
- [ ] Multi-path QUIC working on tactical networks
- [ ] Security layer integrated (PKI, encryption, RBAC)
- [ ] 80%+ test coverage

### Phase 3 Success (Backend Selection)
- [ ] Performance benchmarks collected
- [ ] Decision made: Ditto vs Automerge+Iroh vs Both
- [ ] Production deployment plan finalized

## Conclusion

**DO NOT start Iroh integration yet.**

Instead:
1. ✅ Support experiments team with Mode 3/4 validation (Ditto backend)
2. ✅ Wait for Phase 1 completion and measurements
3. ✅ Get ADR-011 approved while waiting
4. ⏳ **THEN** start 20-week Iroh integration (Phase 2)

The Automerge+Iroh path is well-planned in ADR-011, but **validation comes first**.

---

**Status**: Recommendation
**Date**: 2025-11-08
**Authors**: Codex (AI Assistant)
**References**: ADR-011, Experiments Team Feedback, E11 Implementation Status
