# CAP Protocol Documentation Index

> **Navigation Guide**: All documentation for the CAP Protocol project, organized by category and purpose.

## Quick Start

| Document | Purpose | Audience |
|----------|---------|----------|
| [README.md](../README.md) | Project overview and getting started | All users |
| [DEVELOPMENT.md](../DEVELOPMENT.md) | Development setup and workflow | Contributors |
| [CLAUDE.md](../CLAUDE.md) | AI assistant context and guidelines | AI assistants |

## Architecture Decision Records (ADRs)

ADRs document significant architectural decisions and their rationale.

| ADR | Title | Date | Status |
|-----|-------|------|--------|
| [001](adr/001-cap-protocol-poc.md) | CAP Protocol POC | 2024-10-28 | Implemented |
| [002](adr/002-beacon-storage-architecture.md) | Beacon Storage Architecture | 2024-10-29 | Implemented |
| [004](adr/004-human-machine-cell-composition.md) | Human-Machine Cell Composition | 2024-10-30 | Implemented |

**Summary**: [ARCHITECTURE-DECISION-SUMMARY.md](ARCHITECTURE-DECISION-SUMMARY.md)

## Technical Design Documents

In-depth technical analysis and design explorations.

| Document | Topic | Purpose |
|----------|-------|---------|
| [CAP_Architecture_EventStreaming_vs_DeltaSync.md](CAP_Architecture_EventStreaming_vs_DeltaSync.md) | Event Streaming vs Delta Sync | Evaluates synchronization approaches for distributed state |
| [human-machine-teaming-design.md](human-machine-teaming-design.md) | Human-Machine Teaming | Design for human-in-the-loop authority and cell composition |
| [Ditto-SDK-Integration-Notes.md](Ditto-SDK-Integration-Notes.md) | Ditto SDK Integration | Integration notes and patterns for Ditto CRDT mesh |
| [E8_PROTOBUF_MIGRATION_HANDOFF.md](E8_PROTOBUF_MIGRATION_HANDOFF.md) | Protobuf Migration Guide | E8 simulation team handoff for ADR-012 Phase 5 changes |

## Project Planning

| Document | Purpose |
|----------|---------|
| [CAP-POC-Project-Plan.md](CAP-POC-Project-Plan.md) | POC project plan, epics, and milestones |

## Testing Documentation

Comprehensive testing strategy and implementation guides.

| Document | Scope | Purpose |
|----------|-------|---------|
| [TESTING_STRATEGY.md](TESTING_STRATEGY.md) | Workspace-wide | Testing philosophy, pyramid, and E2E requirements |
| [cap-protocol/docs/testing/e2e-cell-formation.md](../cap-protocol/docs/testing/e2e-cell-formation.md) | Cell Formation E2E | Detailed E2E test scenarios and matrix |

### Key Testing Concepts

- **Unit Tests**: Business logic validation (70% of effort, inline in `src/`)
- **Integration Tests**: Component interaction (20% of effort, `tests/*_integration.rs`)
- **E2E Tests**: Real Ditto P2P sync validation (10% of effort, 100% of mission assurance value, `tests/*_e2e.rs`)

**Critical**: E2E tests validate distributed CRDT mesh behavior - see [TESTING_STRATEGY.md](TESTING_STRATEGY.md) for why this matters for safety-critical autonomous systems.

## Codebase Documentation

### CAP Protocol Core (`cap-protocol/`)

| Module | Documentation |
|--------|---------------|
| Testing | [cap-protocol/docs/testing/](../cap-protocol/docs/testing/) |

## Documentation Categories

### By Audience

- **New Contributors**: README.md → DEVELOPMENT.md → ADRs
- **AI Assistants**: CLAUDE.md → INDEX.md → ADRs
- **Architects**: ADRs → Technical Design Docs → TESTING_STRATEGY.md
- **Developers**: DEVELOPMENT.md → TESTING_STRATEGY.md → Module docs
- **QA/Testing**: TESTING_STRATEGY.md → E2E test docs

### By Topic

- **Architecture**: ADR-001, ADR-002, ADR-004, ADR-012, ARCHITECTURE-DECISION-SUMMARY.md
- **Synchronization**: CAP_Architecture_EventStreaming_vs_DeltaSync.md, Ditto-SDK-Integration-Notes.md
- **Human-Machine Teaming**: ADR-004, human-machine-teaming-design.md
- **Testing**: TESTING_STRATEGY.md, e2e-cell-formation.md
- **Project Management**: CAP-POC-Project-Plan.md
- **Simulation**: E8_PROTOBUF_MIGRATION_HANDOFF.md, E8_PHASE1_SQUAD_TOPOLOGY.md, E8_TOPOLOGY_MODES.md

## Documentation Conventions

### File Organization

```
cap/
├── README.md                    # Project overview
├── DEVELOPMENT.md              # Dev setup
├── CLAUDE.md                   # AI context
├── docs/
│   ├── INDEX.md               # This file
│   ├── TESTING_STRATEGY.md    # Testing philosophy
│   ├── adr/                   # Architecture Decision Records
│   │   ├── 001-*.md
│   │   ├── 002-*.md
│   │   └── 004-*.md
│   └── [technical docs]       # Design documents, plans
└── cap-protocol/
    └── docs/
        └── testing/           # Module-specific test docs
```

### Naming Conventions

- **ADRs**: `NNN-lowercase-with-hyphens.md` (where NNN is zero-padded number)
- **Technical Docs**: `PascalCase_or_kebab-case.md` (descriptive names)
- **Indexes**: `UPPERCASE.md` (INDEX.md, README.md)

### When to Create Documentation

| Scenario | Document Type | Location |
|----------|---------------|----------|
| Significant architectural decision | ADR | `docs/adr/NNN-title.md` |
| Technical design exploration | Design Doc | `docs/Title.md` |
| Module-specific implementation | Module Doc | `[module]/docs/topic.md` |
| Testing strategy/scenarios | Test Doc | `docs/TESTING_STRATEGY.md` or module test docs |
| Project planning | Plan | `docs/Plan-Name.md` |

## Finding What You Need

### "I need to understand..."

- **...the overall architecture**: Start with [ARCHITECTURE-DECISION-SUMMARY.md](ARCHITECTURE-DECISION-SUMMARY.md)
- **...why we made a specific decision**: Check [ADRs](adr/)
- **...how to test**: Read [TESTING_STRATEGY.md](TESTING_STRATEGY.md)
- **...how Ditto integration works**: See [Ditto-SDK-Integration-Notes.md](Ditto-SDK-Integration-Notes.md)
- **...human-machine teaming**: Read [ADR-004](adr/004-human-machine-cell-composition.md)
- **...cell formation E2E tests**: See [e2e-cell-formation.md](../cap-protocol/docs/testing/e2e-cell-formation.md)
- **...protobuf migration for simulation**: See [E8_PROTOBUF_MIGRATION_HANDOFF.md](E8_PROTOBUF_MIGRATION_HANDOFF.md)

### "I'm working on..."

- **...adding a new feature**: Review relevant ADRs, add tests per TESTING_STRATEGY.md
- **...fixing a bug**: Write regression test (see TESTING_STRATEGY.md), check ADRs for design intent
- **...refactoring**: Ensure test coverage, review ADRs for architectural constraints
- **...E2E testing**: Follow [TESTING_STRATEGY.md](TESTING_STRATEGY.md) E2E requirements

## Keeping Documentation Updated

### Documentation is Code

- All docs are version controlled
- Changes reviewed in PRs alongside code
- Outdated docs are worse than no docs - update or delete

### Maintenance Checklist

When making significant changes:

- [ ] Update relevant ADRs (or create new one)
- [ ] Update technical design docs
- [ ] Update test documentation if test strategy changes
- [ ] Update this INDEX.md if adding/moving docs
- [ ] Update README.md if project scope changes
- [ ] Update CLAUDE.md if AI context needs refreshing

---

**Last Updated**: 2025-11-07
**Maintained By**: CAP Protocol Team
**Questions?**: Check DEVELOPMENT.md for contribution guidelines
