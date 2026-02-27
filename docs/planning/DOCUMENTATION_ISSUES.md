# PEAT Documentation GitHub Issues

> **Purpose**: Copy these issue templates to create GitHub Issues for tracking documentation work.
> **Last Updated**: 2025-12-08

---

## Issue #1: Create Comprehensive Operator Guide

**Title**: `docs: Create comprehensive Operator Guide for PEAT deployment and operations`

**Labels**: `documentation`, `operator`, `priority:high`, `good first issue`

**Milestone**: Documentation v1.0

### Description

Create a production-grade operator guide that enables system administrators and mission operators to successfully deploy, configure, monitor, and troubleshoot PEAT systems without requiring deep code knowledge.

### Background

PEAT currently has extensive technical documentation (ADRs, design docs) but lacks structured operational documentation for the operator persona. This guide will bridge that gap and enable production deployments.

### Requirements

#### Must Have (P0)
- [ ] **Installation Guide**
  - Prerequisites (hardware, software, network)
  - Build from source instructions
  - Pre-built binary installation
  - Container/Docker deployment
  - Platform-specific notes (Linux, macOS, Android)

- [ ] **Quick Start**
  - 10-minute path to running first simulation
  - Verification steps

- [ ] **Configuration Reference**
  - All environment variables (`DITTO_APP_ID`, `DITTO_OFFLINE_TOKEN`, etc.)
  - Configuration file formats
  - Feature flags and backend selection
  - Network configuration

- [ ] **Deployment Patterns**
  - Single-node development deployment
  - Multi-node production deployment
  - Edge device deployment (Jetson, embedded)
  - Cloud deployment considerations

- [ ] **Troubleshooting Runbook**
  - Common issues with solutions
  - Diagnostic commands and tools
  - Log analysis guidance
  - Support escalation procedures

#### Should Have (P1)
- [ ] **Security Configuration**
  - PKI setup and certificate management
  - Formation key configuration
  - Encryption settings
  - Authentication configuration

- [ ] **Monitoring & Observability**
  - Available metrics
  - Logging configuration
  - Health check endpoints
  - Alerting recommendations

- [ ] **TAK/ATAK Integration**
  - ATAK plugin installation
  - CoT message configuration
  - Interoperability setup

#### Nice to Have (P2)
- [ ] Capacity planning guidelines
- [ ] Performance tuning recommendations
- [ ] Disaster recovery procedures
- [ ] Upgrade procedures

### Acceptance Criteria

1. New contributor can deploy PEAT simulation in < 10 minutes following the guide
2. All configuration options are documented with defaults and examples
3. At least 10 common issues documented in troubleshooting section
4. Guide tested by someone unfamiliar with PEAT codebase

### Technical Notes

- Location: `docs/guides/operator/OPERATOR_GUIDE.md`
- Use Mermaid diagrams for architecture visuals
- Include tested code/command examples
- Cross-reference existing ADRs where appropriate

### Related Documentation

- [DEVELOPMENT.md](../DEVELOPMENT.md) - Development setup (for reference)
- [TESTING_STRATEGY.md](TESTING_STRATEGY.md) - Testing approach
- [ADR-011](adr/011-ditto-vs-automerge-iroh.md) - Backend selection
- [ADR-017](adr/017-p2p-mesh-management-discovery.md) - Discovery configuration

---

## Issue #2: Create Comprehensive Developer Guide

**Title**: `docs: Create comprehensive Developer Guide for PEAT SDK and contribution`

**Labels**: `documentation`, `developer`, `priority:high`

**Milestone**: Documentation v1.0

### Description

Create a developer guide that enables software engineers to understand PEAT architecture, build applications using the PEAT SDK, contribute to the core protocol, and extend PEAT with custom functionality.

### Background

While ADRs provide decision rationale and code has inline documentation, there's no unified guide for developers to understand the system holistically and contribute effectively.

### Requirements

#### Must Have (P0)
- [ ] **Architecture Overview**
  - System architecture diagram
  - Crate dependency graph
  - Data flow through the system
  - CRDT usage patterns
  - Three-phase protocol walkthrough

- [ ] **Getting Started**
  - Development environment setup
  - IDE configuration (VS Code, CLion)
  - First build and test run
  - Project structure orientation

- [ ] **Core Concepts**
  - Nodes, Cells, and Zones
  - Capabilities and composition
  - Discovery strategies
  - Leader election
  - Hierarchical aggregation

- [ ] **API Reference**
  - Key public APIs with examples
  - Trait documentation
  - Error handling patterns
  - Async patterns used

- [ ] **Testing Guide**
  - Test pyramid and strategy
  - Writing unit tests
  - Writing integration tests
  - Writing E2E tests
  - Test fixtures and harnesses
  - Running tests with Makefile

#### Should Have (P1)
- [ ] **Extending PEAT**
  - Adding custom capabilities
  - Creating discovery strategies
  - Implementing composition rules
  - Adding policy rules
  - Custom QoS configurations

- [ ] **Backend Abstraction**
  - Ditto backend usage
  - Automerge backend usage
  - Switching between backends
  - Backend-specific considerations

- [ ] **Contributing Guide**
  - Code style and conventions
  - PR process and review guidelines
  - Commit message format
  - Documentation requirements

#### Nice to Have (P2)
- [ ] **Mobile Development**
  - peat-ffi usage
  - Kotlin bindings
  - Swift bindings
  - Android build process

- [ ] **Edge AI Integration**
  - peat-inference overview
  - Model deployment
  - ONNX runtime integration
  - Video pipeline setup

### Acceptance Criteria

1. New developer can set up environment and run tests in < 30 minutes
2. Architecture is clear enough to navigate codebase confidently
3. At least 3 extension examples with working code
4. All public APIs documented with examples
5. Testing section enables writing all test types

### Technical Notes

- Location: `docs/guides/developer/DEVELOPER_GUIDE.md`
- Generate API docs from rustdoc where possible
- Include diagrams using Mermaid
- Provide complete, runnable code examples

### Related Documentation

- [DEVELOPMENT.md](../DEVELOPMENT.md) - Current dev setup
- [TESTING_STRATEGY.md](TESTING_STRATEGY.md) - Testing philosophy
- [All ADRs](adr/) - Technical decisions
- [PROTOBUF_MIGRATION_GUIDE.md](PROTOBUF_MIGRATION_GUIDE.md) - Schema work

---

## Issue #3: Create Quickstart Tutorial Series

**Title**: `docs: Create hands-on tutorial series for common PEAT use cases`

**Labels**: `documentation`, `tutorials`, `priority:medium`, `good first issue`

**Milestone**: Documentation v1.0

### Description

Create a series of hands-on tutorials that guide users through common PEAT use cases, from running their first simulation to building custom applications.

### Tutorials to Create

#### Tutorial 1: Your First PEAT Simulation (10 minutes)
- Clone and build
- Run peat-sim
- Observe cell formation
- Understand the output

#### Tutorial 2: Building a PEAT Application
- Set up a new Rust project
- Add peat-protocol dependency
- Create nodes with capabilities
- Join a cell
- Exchange messages

#### Tutorial 3: Adding Custom Capabilities
- Define a new capability type
- Implement composition rules
- Add to existing nodes
- Test the capability

#### Tutorial 4: Integrating with TAK/ATAK
- Install ATAK plugin
- Configure CoT translation
- Send position updates
- Receive commands

### Acceptance Criteria

1. Each tutorial completable in stated time by target audience
2. All code tested and runnable
3. Clear prerequisites and verification steps
4. Progressive complexity (Tutorial 1 → 4)

### Technical Notes

- Location: `docs/tutorials/`
- Include starter code repositories where helpful
- Provide "checkpoint" code for each major step

---

## Issue #4: Set Up Documentation Infrastructure

**Title**: `chore: Set up documentation infrastructure and automation`

**Labels**: `documentation`, `infrastructure`, `priority:medium`

**Milestone**: Documentation v1.0

### Description

Establish documentation infrastructure to ensure quality and maintainability of PEAT documentation.

### Requirements

- [ ] **Link Checking**
  - Add markdown link checker to CI
  - Check internal and external links
  - Report broken links in PR checks

- [ ] **Documentation Linting**
  - Consistent markdown formatting
  - Spelling check
  - Style guide enforcement

- [ ] **API Documentation**
  - Generate rustdoc for public APIs
  - Host or include in documentation site
  - Keep in sync with code

- [ ] **Search (Optional)**
  - If hosting docs site, add search
  - Index all documentation

### Acceptance Criteria

1. CI fails on broken links
2. Rustdoc generates without warnings
3. Documentation passes linting

### Technical Notes

- Consider using mdBook or similar for doc site
- Integrate with existing GitHub Actions

---

## Issue #5: Documentation Review and Feedback Process

**Title**: `docs: Establish documentation review and feedback process`

**Labels**: `documentation`, `process`, `priority:low`

**Milestone**: Documentation v1.1

### Description

Create processes for ongoing documentation maintenance and improvement based on user feedback.

### Requirements

- [ ] **Feedback Collection**
  - Add feedback links to documentation
  - Create documentation issue template
  - Establish triage process

- [ ] **Review Schedule**
  - Quarterly documentation review
  - Review checklist
  - Owner assignments

- [ ] **Metrics Tracking**
  - Track documentation-related issues
  - Monitor user onboarding success
  - Survey new users/contributors

### Acceptance Criteria

1. Clear process for reporting documentation issues
2. Quarterly review schedule documented
3. Metrics baseline established

---

## Issue Summary Table

| Issue | Title | Priority | Effort | Dependencies |
|-------|-------|----------|--------|--------------|
| #1 | Operator Guide | P0/High | Large | None |
| #2 | Developer Guide | P0/High | Large | None |
| #3 | Tutorial Series | P1/Medium | Medium | #1, #2 |
| #4 | Doc Infrastructure | P1/Medium | Small | None |
| #5 | Review Process | P2/Low | Small | #1, #2, #3, #4 |

---

## Labels to Create

| Label | Description | Color |
|-------|-------------|-------|
| `documentation` | Documentation improvements | `#0075ca` |
| `operator` | Operator/operations related | `#7057ff` |
| `developer` | Developer experience related | `#008672` |
| `tutorials` | Tutorial content | `#e4e669` |
| `infrastructure` | Build/CI infrastructure | `#d4c5f9` |
| `priority:high` | High priority | `#b60205` |
| `priority:medium` | Medium priority | `#fbca04` |
| `priority:low` | Low priority | `#0e8a16` |

---

**Note**: These issues can be created manually in GitHub or via the `gh` CLI when available.
