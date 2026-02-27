# PEAT Protocol Demo - Sprint Plan

**Document Version**: 1.0  
**Organization**: (r)evolve - Revolve Team LLC  
**URL**: https://revolveteam.com  
**Demo Target**: POI Tracking Across Distributed Human-Machine-AI Teams

---

## Overview

This document provides a detailed sprint plan for coordinating five parallel development teams toward a successful PEAT Protocol demonstration. The plan maps team deliverables to vignette phases with clear dependencies and integration milestones.

## Team Summary

| Team | Focus | Lead | Workstation |
|------|-------|------|-------------|
| **Core** | Schema, protocol, Automerge/Iroh sync | TBD | Server 1 |
| **ATAK** | Android plugin, PEAT-TAK Bridge, CoT | TBD | Server 2 |
| **Experiments** | Containerlab, validation, metrics | TBD | Server 3 |
| **AI** | Jetson inference, YOLOv8, MLOps agent | TBD | Jetson Orin Nano |
| **PM** | Coordination, stakeholders, demo script | TBD | Laptop |

---

## Sprint Structure

- **Sprint Duration**: 2 weeks
- **Ceremonies**: 
  - Daily async standup (GitHub Issues update)
  - Weekly integration sync (30 min, all leads)
  - Sprint review/planning (1 hr, all leads)
- **Tools**: GitHub Issues, GitHub Projects board

---

## Sprint 1: Foundation & Contracts (Weeks 1-2)

### Goals
- All interface contracts approved
- Basic infrastructure running
- Teams can develop in parallel with mocks

### Core Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Define CapabilityAdvertisement schema | P1 | None | `schemas/capability.json` |
| Define TrackUpdate schema | P1 | None | `schemas/track-update.json` |
| Define MissionTask schema | P1 | None | `schemas/mission-task.json` |
| Set up Automerge document sync | P1 | None | Basic sync working |
| Provide schema validation library | P2 | Schemas | `peat-core/validate.rs` |
| Create mock data files | P2 | Schemas | `test-data/` |

**Exit Criteria**: All Phase 1-3 schemas defined and validated

### ATAK Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Set up Android dev environment | P1 | None | Build system working |
| Review CoT specification | P1 | None | Internal doc |
| Define CoT ↔ PEAT mapping | P1 | Core schemas | Mapping table |
| Scaffold PEAT-TAK Bridge | P1 | None | Bridge skeleton |
| Connect to TAK Server (hello world) | P2 | TAK Server available | CoT send/receive |

**Exit Criteria**: Bridge can send static CoT to TAK Server

### Experiments Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Create Containerlab topology | P1 | None | `peat-demo.clab.yml` |
| Deploy TAK Server container | P1 | None | TAK Server running |
| Set up WebTAK | P1 | TAK Server | WebTAK accessible |
| Create network scenario scripts | P2 | Topology | `scripts/set-network.sh` |
| Create POI injection scripts | P2 | None | `scripts/inject-poi.sh` |
| Document validation metrics | P1 | None | Metrics spec |

**Exit Criteria**: Containerlab topology deploys, TAK Server + WebTAK running

### AI Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Set up Jetson Orin Nano environment | P1 | None | Jetson booting |
| Install YOLOv8 + DeepSORT | P1 | Jetson | Inference running |
| Define capability struct | P1 | Core schema | Rust struct |
| Implement capability serialization | P1 | Struct | JSON output |
| Run inference on test video | P2 | YOLOv8 | Detections logged |

**Exit Criteria**: Jetson runs inference, emits valid CapabilityAdvertisement JSON

### PM Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Review all contract documents | P1 | Drafts | Comments/approval |
| Create GitHub labels | P1 | None | Labels applied |
| Create initial issues for Sprint 1 | P1 | Labels | Issues created |
| Set up GitHub Projects board | P1 | None | Kanban board |
| Draft demo script outline | P2 | Vignette | Script skeleton |

**Exit Criteria**: All contracts approved, project management infrastructure in place

### Sprint 1 Integration Milestone
- [ ] All teams can build/deploy independently
- [ ] Mock data flows: AI mock → Core validation ✓
- [ ] TAK Server accessible from Containerlab
- [ ] All interface contracts signed off

---

## Sprint 2: Phase 1 - Capability Advertisement (Weeks 3-4)

### Goals
- End-to-end capability flow: Jetson → Coordinator → TAK
- Teams visible on WebTAK map

### Core Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Implement capability collection sync | P1 | Automerge | Collection working |
| Implement team-level aggregation | P1 | Sync | Aggregated caps |
| Route capability to coordinator | P1 | Aggregation | Bridge receives |
| Add capability to Automerge doc | P1 | AI struct | Sync verified |

### ATAK Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Receive capability via PEAT sync | P1 | Core sync | Bridge receives |
| Convert capability → CoT registration | P1 | Mapping | CoT generated |
| Send formation to TAK Server | P1 | CoT | TAK receives |
| Display capability on ATAK plugin | P2 | Bridge | UI shows status |

### Experiments Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Deploy Alpha team nodes | P1 | Topology | Alpha-1,2,3 running |
| Deploy Bravo team nodes | P1 | Topology | Bravo-1,2,3 running |
| Deploy Coordinator node | P1 | Topology | Bridge running |
| Measure capability latency | P1 | All nodes | Latency < 5s |
| Create Phase 1 validation script | P1 | Metrics | `validate-phase1.sh` |

### AI Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Emit capability on startup | P1 | Serialization | Auto-advertise |
| Sync capability to team leader | P1 | Core sync | Sync working |
| Update capability on status change | P2 | Emit | Status updates |
| Emit heartbeat every 30s | P2 | Emit | Heartbeat working |

### Sprint 2 Integration Milestone
- [ ] **Demo Checkpoint**: Capability flows end-to-end
- [ ] Jetson emits capability → Core syncs → Bridge converts → TAK displays
- [ ] WebTAK shows "Platoon" formation with 2 teams
- [ ] Latency < 5 seconds

---

## Sprint 3: Phase 2-3 - Tasking & Tracking (Weeks 5-6)

### Goals
- Mission tasking flows from C2 to teams
- Track updates flow from AI to WebTAK
- Bandwidth reduction demonstrated (< 10 Kbps)

### Core Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Implement track collection sync | P1 | TrackUpdate schema | Collection working |
| Implement MissionTask downward sync | P1 | MissionTask schema | Tasks flow down |
| Add track aggregation (optional) | P2 | Track sync | Aggregated tracks |

### ATAK Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Convert CoT mission → PEAT task | P1 | Mapping | Task generated |
| Route task to teams via bridge | P1 | Core sync | Teams receive |
| Convert track → CoT position event | P1 | Mapping | CoT generated |
| Display track on WebTAK | P1 | CoT | Track visible |
| Show track details (confidence, model) | P2 | CoT detail | Details visible |

### Experiments Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Inject POI movement path | P1 | Scripts | POI moves |
| Measure track latency | P1 | Track flow | Latency < 2s |
| Measure bandwidth usage | P1 | tcpdump | < 10 Kbps |
| Create Phase 3 validation script | P1 | Metrics | `validate-phase3.sh` |

### AI Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Emit TrackUpdate on detection | P1 | Inference | Tracks emitted |
| Include model_version in track | P1 | Track struct | Version present |
| Track at 2 Hz minimum | P1 | Emit rate | Rate verified |
| Handle simulated detections | P2 | Experiments | Sim mode works |

### Sprint 3 Integration Milestone
- [ ] **Demo Checkpoint**: Track flows end-to-end
- [ ] WebTAK creates mission → Teams receive task
- [ ] Jetson detects POI → Track appears on WebTAK < 5s
- [ ] Bandwidth < 10 Kbps (vs 5 Mbps video)
- [ ] Track updates at 2 Hz

---

## Sprint 4: Phase 4 - Cross-Network Handoff (Weeks 7-8)

### Goals
- POI crosses from Alpha sector to Bravo sector
- Seamless track continuity (same track ID)
- Handoff gap < 10 seconds

### Core Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Implement handoff message | P1 | New schema | Handoff schema |
| Route handoff via coordinator | P1 | Bridge | Cross-network sync |
| Maintain track ID across teams | P1 | Handoff | ID continuity |

### ATAK Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Display handoff transition | P2 | Track flow | Visual indicator |
| Show track history (both teams) | P3 | CoT detail | History visible |

### Experiments Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Script POI boundary crossing | P1 | POI scripts | Crossing scripted |
| Measure handoff gap | P1 | Timeline | Gap < 10s |
| Measure handoff accuracy | P1 | Ground truth | > 95% |
| Test network partition scenario | P2 | Scenarios | Partition tested |

### AI Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Detect approaching boundary | P2 | Geofence | Boundary detection |
| Prepare handoff package | P1 | Core schema | Package generated |
| Accept handoff from Alpha | P1 | Bravo-3 | Handoff received |
| Acquire track from description | P1 | Detection | Track acquired |

### Sprint 4 Integration Milestone
- [ ] **Demo Checkpoint**: Handoff works
- [ ] POI crosses boundary → Bravo acquires → Same track ID continues
- [ ] Handoff gap < 10 seconds
- [ ] WebTAK shows continuous track across teams

---

## Sprint 5: Phase 5 - MLOps Model Distribution (Weeks 9-10)

### Goals
- Model pushes from C2 to edge nodes
- Hot-swap without tracking interruption > 5s
- Rollback on failure

### Core Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Implement blob store | P1 | New component | Chunked transfer |
| Implement ModelUpdatePackage sync | P1 | Schema | Package syncs |
| Implement deployment status collection | P1 | Schema | Status flows up |
| Cache model at coordinator | P2 | Blob store | Caching works |

### ATAK Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Display model version in track | P2 | CoT detail | Version visible |
| Show deployment status (optional) | P3 | Status | Status visible |

### Experiments Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Set up MLOps server container | P1 | Topology | Server running |
| Host model v1.3.0 package | P1 | Model file | Package available |
| Measure distribution time | P1 | Timing | < 5 min @ 500 Kbps |
| Measure hot-swap interruption | P1 | Gap analysis | < 5s |
| Test rollback scenario | P1 | Fault injection | Rollback works |

### AI Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Implement model download | P1 | Core blob | Download works |
| Implement hash verification | P1 | Download | Hash verified |
| Implement hot-swap | P1 | Verification | Swap works |
| Re-advertise after swap | P1 | Swap | New version advertised |
| Implement rollback | P1 | Failure detection | Rollback works |
| Report deployment status | P1 | Core schema | Status reported |

### Sprint 5 Integration Milestone
- [ ] **Demo Checkpoint**: MLOps works end-to-end
- [ ] Model v1.3.0 pushed → Both Jetsons receive → Hot-swap → New version active
- [ ] Tracking interruption < 5 seconds
- [ ] Capability shows v1.3.0 within 10 seconds
- [ ] Rollback completes within 30 seconds (fault injection)

---

## Sprint 6: Demo Rehearsal & Polish (Weeks 11-12)

### Goals
- Full demo runs without issues
- Demo script finalized
- Backup plans in place

### All Teams
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Rehearsal 1 (rough) | P1 | All phases | Issues identified |
| Fix issues from Rehearsal 1 | P1 | Rehearsal 1 | Issues closed |
| Rehearsal 2 (smooth) | P1 | Fixes | Timing validated |
| Fix issues from Rehearsal 2 | P1 | Rehearsal 2 | Issues closed |
| Rehearsal 3 (final) | P1 | Fixes | Demo ready |
| Create backup procedures | P1 | Rehearsals | Backup doc |

### PM Team
| Task | Priority | Dependency | Deliverable |
|------|----------|------------|-------------|
| Finalize demo script | P1 | Rehearsals | Script complete |
| Create timing cue sheet | P1 | Script | Cue sheet |
| Prepare stakeholder briefing | P1 | Script | Briefing slides |
| Document lessons learned | P2 | Demo | Lessons doc |

### Sprint 6 Integration Milestone
- [ ] **Demo Ready**: 3 successful rehearsals
- [ ] All phases demonstrated in sequence
- [ ] Backup procedures documented and tested
- [ ] Demo script with timing cues complete

---

## Critical Path

```
Week 1-2: Contracts & Infrastructure
    │
    ▼
Week 3-4: Phase 1 (Capability) ←── First integration milestone
    │
    ▼
Week 5-6: Phase 2-3 (Tasking & Tracking) ←── Core demo value
    │
    ▼
Week 7-8: Phase 4 (Handoff) ←── Differentiator
    │
    ▼
Week 9-10: Phase 5 (MLOps) ←── Advanced capability
    │
    ▼
Week 11-12: Rehearsals & Demo ←── Delivery
```

## Risk Register

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Jetson hardware delay | Medium | High | Order backup, develop on x86 emulator |
| TAK Server licensing | Low | Medium | Use FreeTAKServer as backup |
| Cross-team integration delays | High | Medium | Mock interfaces, parallel development |
| Network simulation complexity | Medium | Medium | Start simple, add complexity later |
| Model hot-swap interrupts tracking | Medium | High | Pre-test extensively, have fallback |

## Communication Plan

### Daily (Async)
- Update GitHub Issues with progress
- Tag blockers with `type/blocker`
- @mention blocking team

### Weekly (30 min sync)
- Review sprint progress
- Surface integration issues
- Update critical path

### Sprint Boundary (1 hr)
- Demo phase milestone
- Sprint retrospective
- Next sprint planning

---

## Appendix: GitHub Issue Creation Checklist

### Sprint 1 Issues to Create

**Core Team:**
- [ ] [SCHEMA] Define CapabilityAdvertisement schema
- [ ] [SCHEMA] Define TrackUpdate schema
- [ ] [SCHEMA] Define MissionTask schema
- [ ] [FEATURE] Implement Automerge document sync
- [ ] [FEATURE] Create schema validation library

**ATAK Team:**
- [ ] [FEATURE] Set up Android development environment
- [ ] [CONTRACT] Define CoT ↔ PEAT mapping
- [ ] [FEATURE] Scaffold PEAT-TAK Bridge
- [ ] [INTEGRATION] Connect Bridge to TAK Server

**Experiments Team:**
- [ ] [FEATURE] Create Containerlab topology
- [ ] [FEATURE] Deploy TAK Server container
- [ ] [FEATURE] Create network scenario scripts
- [ ] [FEATURE] Define validation metrics

**AI Team:**
- [ ] [FEATURE] Set up Jetson Orin Nano environment
- [ ] [FEATURE] Install YOLOv8 + DeepSORT
- [ ] [FEATURE] Implement capability struct and serialization

---

*Document maintained by (r)evolve - Revolve Team LLC*  
*https://revolveteam.com*
