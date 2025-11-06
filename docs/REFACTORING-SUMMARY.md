# CAP Protocol Major Refactoring - Executive Summary

**Date**: 2025-11-06
**Status**: Planning Complete, Ready to Execute
**Timeline**: 16-22 weeks
**Outcome**: Production-ready open-source CAP Protocol

---

## The Big Picture

We're undertaking a major architectural refactoring to:
1. **Eliminate Ditto licensing dependency** → Save $X00K+ in licensing costs
2. **Enable multi-transport support** → Integrate with ROS2, gRPC, legacy C2 systems
3. **Implement superior networking** → Multi-path QUIC via Iroh (Starlink + MANET + 5G simultaneously)
4. **Schema-first architecture** → Protobuf definitions enable multi-language support

---

## What's Changing

### Before (Current State)
```
cap-protocol (monolithic, Ditto SDK embedded)
├─ Inline message schemas (Rust structs)
├─ TCP-only transport
├─ Proprietary Ditto CRDT
└─ Hard coupling to Ditto SDK
```

**Problems**:
- ❌ Ditto licensing blocks open-source deployment
- ❌ Cannot integrate with ROS2, gRPC, or legacy systems
- ❌ TCP limitations (no multi-path, slow failover)
- ❌ Vendor lock-in

### After (Target Architecture)
```
cap-schema (Protobuf definitions, ontology)
    ↓
cap-core (Cell formation, composition engine)
    ↓ ↓
cap-transport        cap-persistence
├─ HTTP/WebSocket    ├─ Automerge CRDT
├─ gRPC              ├─ Iroh QUIC networking
├─ ROS2 DDS          └─ Multi-path support
└─ MQTT (future)
```

**Benefits**:
- ✅ Zero licensing costs (Apache-2.0 open source)
- ✅ Multi-transport (HTTP, gRPC, ROS2, WebSocket)
- ✅ Multi-path QUIC (Starlink + MANET + 5G)
- ✅ External system integration (Python, Java, C++)
- ✅ Schema versioning and code generation

---

## Two-Phase Approach

### Phase 1: ADR-012 - Schema & Protocol Extensibility (16 weeks)

**Critical Foundation** - Must complete before ADR-011

**What**: Separate schema from protocol, enable multiple transports

**Key Milestones**:
- **Weeks 1-2**: cap-schema (Protobuf definitions, code generation)
- **Weeks 3-4**: cap-transport (HTTP/WebSocket adapter)
- **Weeks 5-6**: cap-persistence (storage abstraction, REST API)
- **Weeks 7-10**: Protocol adapters (gRPC, ROS2)
- **Weeks 11-14**: cap-core refactoring (use new abstractions)
- **Weeks 15-16**: Integration validation (examples, performance testing)

**Deliverables**:
- cap-schema crate with Protobuf messages
- cap-transport with 4 adapters (HTTP, WebSocket, gRPC, ROS2)
- cap-persistence with external REST API
- Refactored cap-protocol using abstractions
- Integration examples (Python, Java, ROS2, JavaScript)

**Success Metrics**:
- All E2E tests passing
- < 5% performance regression vs baseline
- Integration examples working
- Documentation complete

### Phase 2: ADR-011 - Automerge + Iroh (6 weeks)

**BLOCKED BY**: ADR-012 completion (schema & transport abstractions required first)

**What**: Replace Ditto with Automerge + Iroh

**Key Milestones**:
- **Weeks 1-2**: Automerge DataStore implementation
- **Weeks 3-4**: Iroh QUIC networking integration
- **Weeks 5-6**: Multi-path support + Ditto deprecation

**Deliverables**:
- Automerge-based DataStore
- Iroh QUIC networking layer
- Multi-path networking (3+ interfaces simultaneously)
- Ditto migration tooling
- Performance benchmarks

**Success Metrics**:
- Performance ≥ Ditto baseline
- Multi-path networking validated
- Connection migration < 1 second
- Zero licensing costs

---

## Why ADR-012 Blocks ADR-011

**Dependency Chain**:
```
ADR-012: Schema Definition
    ↓ defines WHAT messages look like
cap-schema (Protobuf)
    ↓ drives
Automerge document structure
    ↓ stored in
ADR-011: Automerge + Iroh
```

**Rationale**:
1. **Schema Contract**: Automerge needs to know what to sync → Protobuf defines structure
2. **Transport Independence**: Iroh is ONE transport option → Need abstraction layer for HTTP/gRPC/ROS2 too
3. **Integration Points**: External systems need schemas before they can consume sync data

**Bottom Line**: Can't implement sync engine without knowing what we're syncing

---

## Key Documents

All planning documents are in `/docs`:

1. **REFACTORING-PLAYBOOK.md** (this is the detailed guide)
   - 16-week phase-by-phase breakdown
   - Technical details for each task
   - Testing strategy
   - Risk management
   - Success metrics

2. **GITHUB-ISSUES.md** (issue templates ready to create)
   - 14 detailed GitHub issues
   - 2 Epic issues (#44, #52)
   - Clear acceptance criteria
   - Effort estimates

3. **ADR-011**: CRDT + Networking Stack Selection (Automerge + Iroh)
   - Full technical analysis
   - Ditto vs Automerge+Iroh comparison
   - Multi-path networking requirements
   - 76KB comprehensive document

4. **ADR-012**: Schema Definition and Protocol Extensibility
   - Schema-first architecture
   - Multi-transport support
   - Integration enablement
   - 53KB comprehensive document

5. **ADR-010**: Transport Layer (SUPERSEDED)
   - Marked as superseded by ADR-011
   - Historical context for TCP/UDP debate
   - Explains why QUIC is superior

---

## Timeline Summary

| Phase | Weeks | Deliverable | Status |
|-------|-------|-------------|--------|
| **Preparation** | Week 0 | Workspace setup, GitHub issues | ⬜ Ready to start |
| **ADR-012** | Weeks 1-16 | Schema & transport abstraction | ⬜ Blocked by prep |
| **ADR-011** | Weeks 17-22 | Automerge + Iroh implementation | ⬜ Blocked by ADR-012 |
| **Total** | 22 weeks | Production-ready v2.0 | |

**Target Completion**: Q2 2026

---

## Resource Requirements

**Team**:
- 1-2 developers full-time (16-22 weeks)
- ROS2 expertise for Phase 5 (optional, can defer)
- DevOps for CI/CD setup (1-2 days)

**Tools & Dependencies**:
- Protobuf compiler (`protoc`)
- gRPC tools (`tonic`, `prost`)
- ROS2 development environment (for Phase 5)
- Automerge crate (`automerge = "0.7.1"`)
- Iroh crate (`iroh = "0.x"`)

**Infrastructure**:
- CI for new crates (GitHub Actions)
- Staging environment for testing
- ContainerLab for network simulation

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **Timeline overruns** | Medium | High | Feature flags, incremental delivery |
| **Performance degradation** | Medium | High | Benchmark every phase, optimize early |
| **Breaking changes** | High | Medium | Backward compatibility, migration guides |
| **Automerge + Iroh issues** | Medium | High | Keep Ditto as fallback during transition |
| **External integration complexity** | Medium | Medium | Start simple (HTTP), defer ROS2 if needed |

**Overall Risk**: Medium (manageable with incremental approach and feature flags)

---

## Success Metrics

### Technical

| Metric | Target | Measure |
|--------|--------|---------|
| Schema overhead | < 5% vs JSON | Protobuf efficiency |
| Transport latency | < 10ms HTTP, < 5ms gRPC | Usability |
| Query performance | < 50ms | Responsiveness |
| Sync throughput | > 1000 ops/sec | Scale |
| Memory usage | < 500MB for 100 nodes | Efficiency |
| Code coverage | > 80% | Quality |

### Integration

| Metric | Target | Measure |
|--------|--------|---------|
| ROS2 latency | < 100ms | Real-time robotics |
| gRPC latency | p99 < 20ms | C2 responsiveness |
| Multi-language | Rust, Python, C++, Java | Ecosystem |
| API uptime | > 99.9% | Reliability |

### Business

| Metric | Target | Impact |
|--------|--------|--------|
| Licensing cost | $0 | Cost savings |
| Integration effort | < 1 week per transport | Extensibility |
| Time to production | 16-22 weeks | Schedule |
| NATO STANAG ready | Q2 2026 | Strategic goal |

---

## Next Steps

### Immediate (This Week)
1. ✅ Finalize ADR-011 and ADR-012
2. ✅ Create playbook and issue templates
3. ✅ Update ADR-010 to "Superseded"
4. ⬜ Team review and approval
5. ⬜ Create GitHub issues
6. ⬜ Set up project board

### Week 1 (Kickoff)
1. Create workspace structure for new crates
2. Set up CI for cap-schema
3. Begin Protobuf message definitions (Phase 1)
4. Team kickoff meeting

### Monthly Checkpoints
- **Month 1** (Weeks 1-4): cap-schema + cap-transport foundation
- **Month 2** (Weeks 5-8): cap-persistence + gRPC adapter
- **Month 3** (Weeks 9-12): ROS2 adapter + cap-core refactoring start
- **Month 4** (Weeks 13-16): cap-core refactoring complete + integration validation
- **Month 5** (Weeks 17-20): Automerge + Iroh implementation
- **Month 6** (Weeks 21-22): Final testing + production release

---

## Decision Points

| Week | Decision | Go/No-Go Criteria |
|------|----------|-------------------|
| **Week 6** | Continue to protocol adapters? | cap-schema complete, transport working |
| **Week 10** | Defer ROS2 integration? | Evaluate complexity vs value |
| **Week 14** | Ditto deprecation timeline? | Automerge+Iroh readiness |
| **Week 16** | Production readiness? | All metrics met, docs complete |
| **Week 22** | v2.0 release? | Final validation passed |

---

## Communication Plan

**Internal**:
- Weekly: Progress updates on GitHub project board
- Bi-weekly: Team sync on blockers and decisions
- Monthly: Executive summary for stakeholders

**External**:
- Blog posts: Major milestones
- Documentation: Continuous updates
- Demos: Integration examples as released

---

## Questions?

See detailed planning documents:
- **REFACTORING-PLAYBOOK.md** - Technical details and phase breakdowns
- **GITHUB-ISSUES.md** - Issue templates and task lists
- **ADR-011** - Automerge + Iroh technical decision
- **ADR-012** - Schema & protocol extensibility architecture

Ready to execute!
