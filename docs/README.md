# Peat Protocol Demo - Documentation

**Organization**: Defense Unicorns  
**URL**: https://defenseunicorns.com

This directory contains documentation for the Peat Protocol POI Tracking Demo, coordinating five parallel development teams.

## Directory Structure

```
docs/
├── README.md                 # This file
├── contracts/                # Interface contracts between teams
│   ├── CONTRACT_CORE_AI_CAPABILITY.md      # Core ↔ AI: Capability Advertisement
│   ├── CONTRACT_CORE_AI_MLOPS.md           # Core ↔ AI: Model Distribution
│   ├── CONTRACT_CORE_ATAK_TAK_BRIDGE.md    # Core ↔ ATAK: TAK Integration
│   └── CONTRACT_EXPERIMENTS_ALL_VALIDATION.md  # Experiments ↔ All: Validation
└── planning/
    └── SPRINT_PLAN.md        # Detailed 12-week sprint plan
```

## Teams

| Team | Primary Focus | Key Deliverables |
|------|--------------|------------------|
| **Core** | Schema, protocol, Automerge/Iroh | CapabilityAdvertisement, TrackUpdate, MissionTask schemas |
| **ATAK** | Android plugin, Peat-TAK Bridge | CoT translation, TAK Server integration |
| **Experiments** | Containerlab, validation | Network simulation, metrics collection |
| **AI** | Jetson inference, YOLOv8, MLOps | Object tracking, model hot-swap |
| **PM** | Coordination, demo scripting | Sprint management, stakeholder comms |

## Demo Phases

The demo follows five phases from the vignette:

1. **Initialization & Capability Advertisement** - Teams form, capabilities flow to C2
2. **Mission Tasking** - C2 issues track command via TAK
3. **Active Tracking** - AI detects POI, tracks flow to WebTAK
4. **Track Handoff** - POI crosses boundary, Bravo acquires
5. **MLOps Model Distribution** - New model pushes, hot-swap, rollback

## Interface Contracts

Before implementation begins, teams must review and approve their interface contracts:

| Contract | Teams | Status |
|----------|-------|--------|
| [Core ↔ AI Capability](contracts/CONTRACT_CORE_AI_CAPABILITY.md) | Core, AI | ☐ Pending |
| [Core ↔ AI MLOps](contracts/CONTRACT_CORE_AI_MLOPS.md) | Core, AI | ☐ Pending |
| [Core ↔ ATAK Bridge](contracts/CONTRACT_CORE_ATAK_TAK_BRIDGE.md) | Core, ATAK | ☐ Pending |
| [Experiments ↔ All](contracts/CONTRACT_EXPERIMENTS_ALL_VALIDATION.md) | All | ☐ Pending |

**Approval Process:**
1. Both teams review contract document
2. Comment with questions/changes on GitHub Issue
3. Both teams add ✅ reaction to approve
4. PM updates status to "Approved"

## Sprint Plan

See [SPRINT_PLAN.md](planning/SPRINT_PLAN.md) for the detailed 12-week plan mapping team deliverables to demo phases.

### Key Milestones

| Week | Milestone | Demo Phase |
|------|-----------|------------|
| 2 | Contracts approved, infrastructure ready | - |
| 4 | Capability flows end-to-end | Phase 1 |
| 6 | Track updates on WebTAK | Phase 2-3 |
| 8 | Cross-network handoff works | Phase 4 |
| 10 | MLOps model distribution works | Phase 5 |
| 12 | Demo ready (3 rehearsals complete) | All |

## GitHub Setup

### Labels
Run the setup script to create all labels:
```bash
cd scripts/
chmod +x setup-labels.sh
./setup-labels.sh
```

### Issue Templates
Issue templates are in `.github/ISSUE_TEMPLATE/`:
- `schema-definition.yml` - Schema definitions (Core team)
- `interface-contract.yml` - Cross-team contracts
- `integration-task.yml` - Integration work items
- `feature-request.yml` - New features
- `bug-report.yml` - Bug reports
- `blocker.yml` - Blockers (high priority)

### Projects Board
Create a GitHub Projects board with columns:
```
| Backlog | Ready | In Progress | In Review | Integration Test | Done |
```

## Quick Links

- **Vignette Use Case**: [VIGNETTE_USE_CASE.md](../VIGNETTE_USE_CASE.md)
- **Data Flow Sequence**: [DATA_FLOW_SEQUENCE.md](../DATA_FLOW_SEQUENCE.md)
- **Capability Flow Architecture**: [CAPABILITY_FLOW_ARCHITECTURE.md](../CAPABILITY_FLOW_ARCHITECTURE.md)

---

*Document maintained by Defense Unicorns*
