# HIVE Protocol Documentation Index

> **Navigation Guide**: All documentation for the HIVE Protocol project, organized by category and purpose.

---

## 📚 User Guides (Start Here)

| Guide | Audience | Description |
|-------|----------|-------------|
| [**Operator Guide**](guides/operator/OPERATOR_GUIDE.md) | System Admins, DevOps, Mission Operators | Installation, configuration, deployment, monitoring, troubleshooting |
| [**Developer Guide**](guides/developer/DEVELOPER_GUIDE.md) | Software Engineers, Contributors | Architecture, API reference, extending HIVE, testing, contributing |

### Quick Links by Task

| I want to... | Go to... |
|--------------|----------|
| Deploy HIVE in production | [Operator Guide: Deployment](guides/operator/OPERATOR_GUIDE.md#5-deployment-patterns) |
| Set up a development environment | [Developer Guide: Getting Started](guides/developer/DEVELOPER_GUIDE.md#2-getting-started) |
| Understand the architecture | [Developer Guide: Architecture](guides/developer/DEVELOPER_GUIDE.md#3-architecture) |
| Configure HIVE | [Operator Guide: Configuration](guides/operator/OPERATOR_GUIDE.md#4-configuration) |
| Write tests | [Developer Guide: Testing](guides/developer/DEVELOPER_GUIDE.md#8-testing) |
| Troubleshoot issues | [Operator Guide: Troubleshooting](guides/operator/OPERATOR_GUIDE.md#11-troubleshooting) |
| Integrate with TAK/ATAK | [Operator Guide: TAK Integration](guides/operator/OPERATOR_GUIDE.md#9-takatak-integration) |
| Extend HIVE with custom capabilities | [Developer Guide: Extending HIVE](guides/developer/DEVELOPER_GUIDE.md#7-extending-hive) |
| Contribute to HIVE | [Developer Guide: Contributing](guides/developer/DEVELOPER_GUIDE.md#12-contributing) |

---

## 🔍 For IP Due Diligence Reviewers

**Start Here**:
1. [**IP_OVERVIEW.md**](IP_OVERVIEW.md) - Comprehensive IP overview for technical evaluation
2. [**VALIDATION_RESULTS.md**](VALIDATION_RESULTS.md) - Experimental validation summary
3. [**patents/**](patents/) - Patent strategy and technical disclosures

**Key Technical Documents**:
- [Architecture Decision Records](adr/) - 16 ADRs documenting all major technical decisions
- [TESTING_STRATEGY.md](TESTING_STRATEGY.md) - Quality assurance approach
- [DEVELOPMENT.md](../DEVELOPMENT.md) - Development guide and code quality practices

## Quick Start

| Document | Purpose | Audience |
|----------|---------|----------|
| [**IP_OVERVIEW.md**](IP_OVERVIEW.md) | **Comprehensive IP overview** | **IP evaluators** |
| [README.md](../README.md) | Project overview and getting started | All users |
| [DEVELOPMENT.md](../DEVELOPMENT.md) | Development setup and workflow | Technical evaluators |

## Architecture Decision Records (ADRs)

ADRs document significant architectural decisions and their rationale.

| ADR | Title | Date | Status |
|-----|-------|------|--------|
| [001](adr/001-hive-protocol-poc.md) | HIVE Protocol POC | 2024-10-28 | Implemented |
| [002](adr/002-beacon-storage-architecture.md) | Beacon Storage Architecture | 2024-10-29 | Implemented |
| [004](adr/004-human-machine-cell-composition.md) | Human-Machine Cell Composition | 2024-10-30 | Implemented |
| [011](adr/011-ditto-vs-automerge-iroh.md) | Ditto vs Automerge/Iroh Backend Abstraction | 2024-11-15 | Accepted |
| [012](adr/012-schema-definition-protocol-extensibility.md) | Schema Definition and Protocol Extensibility | 2024-11-18 | Accepted |
| [013](adr/013-distributed-software-ai-operations.md) | Distributed Software AI Operations | 2024-11-20 | Accepted |
| [014](adr/014-distributed-coordination-primitives.md) | Distributed Coordination Primitives | 2024-12-15 | Accepted |
| [015](adr/015-experimental-validation-hierarchical-aggregation.md) | Experimental Validation - Hierarchical Aggregation | 2024-12-20 | Accepted |
| [016](adr/016-ttl-and-data-lifecycle-abstraction.md) | TTL and Data Lifecycle Abstraction | 2025-01-10 | Accepted |

**Summary**: [ARCHITECTURE-DECISION-SUMMARY.md](ARCHITECTURE-DECISION-SUMMARY.md)

## Technical Design Documents

In-depth technical analysis and design explorations.

| Document | Topic | Purpose |
|----------|-------|---------|
| [CAP_Architecture_EventStreaming_vs_DeltaSync.md](CAP_Architecture_EventStreaming_vs_DeltaSync.md) | Event Streaming vs Delta Sync | Evaluates synchronization approaches for distributed state |
| [human-machine-teaming-design.md](human-machine-teaming-design.md) | Human-Machine Teaming | Design for human-in-the-loop authority and cell composition |
| [Ditto-SDK-Integration-Notes.md](Ditto-SDK-Integration-Notes.md) | Ditto SDK Integration | Integration notes and patterns for Ditto CRDT mesh |
| [TTL_AND_DATA_LIFECYCLE_DESIGN.md](TTL_AND_DATA_LIFECYCLE_DESIGN.md) | TTL and Data Lifecycle (Ditto) | Ditto-specific implementation of ADR-016 lifecycle management |
| [POLICY_ENGINE_CRDT_INTEGRATION.md](POLICY_ENGINE_CRDT_INTEGRATION.md) | Policy Engine & CRDT Integration | Optimistic Concurrency Control for policy enforcement with Ditto LWW semantics |
| [PROTOBUF_MIGRATION_GUIDE.md](PROTOBUF_MIGRATION_GUIDE.md) | Protobuf Migration Guide | Technical guide for ADR-012 protobuf migration |

## Project Planning

| Document | Purpose |
|----------|---------|
| [CAP-POC-Project-Plan.md](CAP-POC-Project-Plan.md) | POC project plan, epics, and milestones |

## Testing Documentation

Comprehensive testing strategy and implementation guides.

| Document | Scope | Purpose |
|----------|-------|---------|
| [TESTING_STRATEGY.md](TESTING_STRATEGY.md) | Workspace-wide | Testing philosophy, pyramid, and E2E requirements |
| [hive-protocol/docs/testing/e2e-cell-formation.md](../hive-protocol/docs/testing/e2e-cell-formation.md) | Cell Formation E2E | Detailed E2E test scenarios and matrix |

### Key Testing Concepts

- **Unit Tests**: Business logic validation (70% of effort, inline in `src/`)
- **Integration Tests**: Component interaction (20% of effort, `tests/*_integration.rs`)
- **E2E Tests**: Real Ditto P2P sync validation (10% of effort, 100% of mission assurance value, `tests/*_e2e.rs`)

**Critical**: E2E tests validate distributed CRDT mesh behavior - see [TESTING_STRATEGY.md](TESTING_STRATEGY.md) for why this matters for safety-critical autonomous systems.

## Codebase Documentation

### HIVE Protocol Core (`hive-protocol/`)

| Module | Documentation |
|--------|---------------|
| Testing | [hive-protocol/docs/testing/](../hive-protocol/docs/testing/) |

## Documentation Categories

### By Audience

- **Operators/System Admins**: [Operator Guide](guides/operator/OPERATOR_GUIDE.md) → Configuration → Troubleshooting
- **Developers**: [Developer Guide](guides/developer/DEVELOPER_GUIDE.md) → Architecture → Testing → Contributing
- **IP Evaluators**: IP_OVERVIEW.md → VALIDATION_RESULTS.md → patents/ → ADRs
- **Technical Due Diligence**: IP_OVERVIEW.md → ADRs → TESTING_STRATEGY.md → DEVELOPMENT.md
- **Architects**: ADRs → Technical Design Docs → TESTING_STRATEGY.md
- **QA/Testing**: TESTING_STRATEGY.md → [Developer Guide: Testing](guides/developer/DEVELOPER_GUIDE.md#8-testing)

### By Topic

- **IP & Patents**: IP_OVERVIEW.md, patents/, VALIDATION_RESULTS.md
- **Architecture**: ADR-001, ADR-002, ADR-004, ADR-011, ADR-012, ADR-016, ARCHITECTURE-DECISION-SUMMARY.md
- **Validation**: VALIDATION_RESULTS.md, ADR-015
- **Synchronization**: CAP_Architecture_EventStreaming_vs_DeltaSync.md, Ditto-SDK-Integration-Notes.md
- **Data Lifecycle & TTL**: ADR-016, TTL_AND_DATA_LIFECYCLE_DESIGN.md
- **Policy Engine & Conflict Resolution**: POLICY_ENGINE_CRDT_INTEGRATION.md, POLICY_ENGINE_DESIGN.md, EXTENSIBLE_POLICY_ENGINE_DESIGN.md
- **Human-Machine Teaming**: ADR-004, human-machine-teaming-design.md
- **Testing**: TESTING_STRATEGY.md, e2e-cell-formation.md
- **Simulation**: NETWORK_SIMULATOR_EVALUATION.md, NETWORK_TOPOLOGY_MODES.md, SQUAD_TOPOLOGY_VALIDATION.md

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
└── hive-protocol/
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

- **...the intellectual property and innovations**: Start with [IP_OVERVIEW.md](IP_OVERVIEW.md)
- **...validation and testing results**: Read [VALIDATION_RESULTS.md](VALIDATION_RESULTS.md)
- **...patent strategy**: See [patents/](patents/) directory
- **...the overall architecture**: Start with [ARCHITECTURE-DECISION-SUMMARY.md](ARCHITECTURE-DECISION-SUMMARY.md)
- **...why we made a specific decision**: Check [ADRs](adr/)
- **...how to test**: Read [TESTING_STRATEGY.md](TESTING_STRATEGY.md)
- **...how Ditto integration works**: See [Ditto-SDK-Integration-Notes.md](Ditto-SDK-Integration-Notes.md)
- **...TTL and data lifecycle**: Start with [ADR-016](adr/016-ttl-and-data-lifecycle-abstraction.md) for abstraction, then [TTL_AND_DATA_LIFECYCLE_DESIGN.md](TTL_AND_DATA_LIFECYCLE_DESIGN.md) for Ditto specifics
- **...policy engine and conflict resolution**: See [POLICY_ENGINE_CRDT_INTEGRATION.md](POLICY_ENGINE_CRDT_INTEGRATION.md) for OCC approach with Ditto
- **...human-machine teaming**: Read [ADR-004](adr/004-human-machine-cell-composition.md)
- **...cell formation E2E tests**: See [e2e-cell-formation.md](../hive-protocol/docs/testing/e2e-cell-formation.md)
- **...protobuf migration**: See [PROTOBUF_MIGRATION_GUIDE.md](PROTOBUF_MIGRATION_GUIDE.md)

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

## Documentation Planning & Maintenance

| Document | Purpose |
|----------|---------|
| [DOCUMENTATION_PLAN.md](DOCUMENTATION_PLAN.md) | Documentation strategy, requirements, and standards |
| [DOCUMENTATION_ISSUES.md](DOCUMENTATION_ISSUES.md) | GitHub issue templates for documentation work |

---

**Last Updated**: 2025-12-08
**Maintained By**: HIVE Protocol Team
**Questions?**: Check DEVELOPMENT.md for contribution guidelines
