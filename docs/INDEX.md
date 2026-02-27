# PEAT Protocol Documentation Index

> **Navigation Guide**: All documentation for the PEAT Protocol project, organized by category and purpose.

---

## Getting Started

| Document | Purpose | Audience |
|----------|---------|----------|
| [**ARCHITECTURE.md**](ARCHITECTURE.md) | System architecture overview | New developers |
| [**spec/**](spec/README.md) | Protocol specifications (IETF RFC style) | Protocol implementers |
| [README.md](../README.md) | Project overview and getting started | All users |
| [DEVELOPMENT.md](../DEVELOPMENT.md) | Development setup and workflow | Contributors |

### Quick Links by Task

| I want to... | Go to... |
|--------------|----------|
| Understand the architecture | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Read the protocol specs | [spec/](spec/README.md) |
| Set up a development environment | [DEVELOPMENT.md](../DEVELOPMENT.md) |
| See why decisions were made | [ADRs](adr/) |
| Write tests | [TESTING_STRATEGY.md](TESTING_STRATEGY.md) |
| Run or extend functional tests | [FUNCTIONAL-TESTING.md](FUNCTIONAL-TESTING.md) |
| Understand IP strategy | [IP_OVERVIEW.md](IP_OVERVIEW.md) |

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
| [001](adr/001-peat-protocol-poc.md) | PEAT Protocol POC | 2024-10-28 | Implemented |
| [002](adr/002-beacon-storage-architecture.md) | Beacon Storage Architecture | 2024-10-29 | Implemented |
| [004](adr/004-human-machine-cell-composition.md) | Human-Machine Cell Composition | 2024-10-30 | Implemented |
| [011](adr/011-ditto-vs-automerge-iroh.md) | Ditto vs Automerge/Iroh Backend Abstraction | 2024-11-15 | Accepted |
| [012](adr/012-schema-definition-protocol-extensibility.md) | Schema Definition and Protocol Extensibility | 2024-11-18 | Accepted |
| [013](adr/013-distributed-software-ai-operations.md) | Distributed Software AI Operations | 2024-11-20 | Accepted |
| [014](adr/014-distributed-coordination-primitives.md) | Distributed Coordination Primitives | 2024-12-15 | Accepted |
| [015](adr/015-experimental-validation-hierarchical-aggregation.md) | Experimental Validation - Hierarchical Aggregation | 2024-12-20 | Accepted |
| [016](adr/016-ttl-and-data-lifecycle-abstraction.md) | TTL and Data Lifecycle Abstraction | 2025-01-10 | Accepted |

**Summary**: [ARCHITECTURE-DECISION-SUMMARY.md](ARCHITECTURE-DECISION-SUMMARY.md)

## Protocol Specifications

IETF RFC-style specifications for protocol implementers. See [spec/README.md](spec/README.md) for full index.

| Spec | Topic |
|------|-------|
| [001-transport.md](spec/001-transport.md) | Wire formats, QUIC/Iroh, UDP bypass, PEAT-Lite |
| [002-sync.md](spec/002-sync.md) | CRDT semantics, Automerge, Negentropy |
| [003-schema.md](spec/003-schema.md) | Protobuf definitions, CoT mapping |
| [004-coordination.md](spec/004-coordination.md) | Cell formation, leader election, hierarchy |
| [005-security.md](spec/005-security.md) | Authentication, encryption, key management |

## Reference Documents

| Document | Purpose |
|----------|---------|
| [TTL_AND_DATA_LIFECYCLE_DESIGN.md](TTL_AND_DATA_LIFECYCLE_DESIGN.md) | Data lifecycle management design |
| [PROTOBUF_MIGRATION_GUIDE.md](PROTOBUF_MIGRATION_GUIDE.md) | Protobuf migration guide |
| [STORAGE_PERSISTENCE_DUE_DILIGENCE.md](STORAGE_PERSISTENCE_DUE_DILIGENCE.md) | Storage layer evaluation |

## Planning Documents

Active planning documents are in [planning/](planning/). Historical design explorations that informed ADRs.

## Testing Documentation

Comprehensive testing strategy and implementation guides.

| Document | Scope | Purpose |
|----------|-------|---------|
| [TESTING_STRATEGY.md](TESTING_STRATEGY.md) | Workspace-wide | Testing philosophy, pyramid, and E2E requirements |
| [FUNCTIONAL-TESTING.md](FUNCTIONAL-TESTING.md) | Transport layer | Hardware functional tests (BLE + QUIC), feature-to-phase mapping, platform extension guide |
| [peat-protocol/docs/testing/e2e-cell-formation.md](../peat-protocol/docs/testing/e2e-cell-formation.md) | Cell Formation E2E | Detailed E2E test scenarios and matrix |

### Key Testing Concepts

- **Unit Tests**: Business logic validation (70% of effort, inline in `src/`)
- **Integration Tests**: Component interaction (20% of effort, `tests/*_integration.rs`)
- **E2E Tests**: Real Ditto P2P sync validation (10% of effort, 100% of mission assurance value, `tests/*_e2e.rs`)

**Critical**: E2E tests validate distributed CRDT mesh behavior - see [TESTING_STRATEGY.md](TESTING_STRATEGY.md) for why this matters for safety-critical autonomous systems.

## Codebase Documentation

### PEAT Protocol Core (`peat-protocol/`)

| Module | Documentation |
|--------|---------------|
| Testing | [peat-protocol/docs/testing/](../peat-protocol/docs/testing/) |

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
- **Architecture**: ARCHITECTURE.md, ADRs, ARCHITECTURE-DECISION-SUMMARY.md
- **Protocol Specs**: spec/001-transport.md through spec/005-security.md
- **Validation**: VALIDATION_RESULTS.md, ADR-015
- **Data Lifecycle & TTL**: ADR-016, TTL_AND_DATA_LIFECYCLE_DESIGN.md
- **Human-Machine Teaming**: ADR-004
- **Testing**: TESTING_STRATEGY.md
- **Simulation**: NETWORK_SIMULATOR_EVALUATION.md

## Documentation Conventions

### File Organization

```
peat/
├── README.md                    # Project overview
├── DEVELOPMENT.md              # Dev setup
├── CLAUDE.md                   # AI context
├── docs/
│   ├── INDEX.md               # This file
│   ├── ARCHITECTURE.md        # System architecture
│   ├── TESTING_STRATEGY.md    # Testing philosophy
│   ├── adr/                   # Architecture Decision Records
│   ├── spec/                  # Protocol specifications (IETF style)
│   │   ├── 001-transport.md
│   │   ├── 002-sync.md
│   │   ├── 003-schema.md
│   │   ├── 004-coordination.md
│   │   └── 005-security.md
│   ├── planning/              # Active planning documents
│   ├── patents/               # IP documentation
│   └── [reference docs]       # Design documents
└── crates/
    └── [crate]/docs/          # Module-specific docs
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

- **...the system architecture**: Start with [ARCHITECTURE.md](ARCHITECTURE.md)
- **...the protocol specifications**: Read [spec/](spec/README.md)
- **...the intellectual property**: Start with [IP_OVERVIEW.md](IP_OVERVIEW.md)
- **...validation and testing results**: Read [VALIDATION_RESULTS.md](VALIDATION_RESULTS.md)
- **...patent strategy**: See [patents/](patents/) directory
- **...why we made a specific decision**: Check [ADRs](adr/)
- **...how to test**: Read [TESTING_STRATEGY.md](TESTING_STRATEGY.md)
- **...TTL and data lifecycle**: See [ADR-016](adr/016-ttl-and-data-lifecycle-abstraction.md) and [TTL_AND_DATA_LIFECYCLE_DESIGN.md](TTL_AND_DATA_LIFECYCLE_DESIGN.md)
- **...human-machine teaming**: Read [ADR-004](adr/004-human-machine-cell-composition.md)
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

**Last Updated**: 2025-01-08
**Maintained By**: PEAT Protocol Team
**Questions?**: Check DEVELOPMENT.md for contribution guidelines
