# Peat Protocol Specifications

> **Heads-up.** The current normative drafts and Protobuf schemas live in
> [`/spec/`](../../spec/) (IRTF DINRG submission set). The documents in
> *this* directory are an earlier internal specification format kept for
> historical reference and will likely be reorganized or removed when
> the IRTF draft set fully supersedes them.

**Status**: Draft (legacy format)
**Target**: IETF Informational RFC
**Version**: 0.1.0

## Overview

This directory contains formal protocol specifications for Peat (Hierarchical Intelligence for Virtual Environments). These documents are intended to evolve toward IETF RFC-style specifications suitable for public review and interoperability testing.

## Document Status

| Document | Title | Status | Last Updated |
|----------|-------|--------|--------------|
| [001-transport](001-transport.md) | Transport Layer | Draft | 2025-01-07 |
| [002-sync](002-sync.md) | Synchronization Protocol | Draft | 2025-01-07 |
| [003-schema](003-schema.md) | Data Schema Definitions | Draft | 2025-01-07 |
| [004-coordination](004-coordination.md) | Coordination Protocol | Draft | 2025-01-07 |
| [005-security](005-security.md) | Security Framework | Draft | 2025-01-07 |

## Status Definitions

- **Draft**: Initial specification, subject to significant change
- **Review**: Stable for review, collecting feedback
- **Stable**: Ready for implementation, changes require versioning
- **Final**: Specification complete, published

## Reading Order

For new readers, we recommend:

1. **Architecture Overview**: [../ARCHITECTURE.md](../ARCHITECTURE.md)
2. **Schema Definitions**: [003-schema.md](003-schema.md) - Understand the data model
3. **Transport Layer**: [001-transport.md](001-transport.md) - Wire formats and connectivity
4. **Sync Protocol**: [002-sync.md](002-sync.md) - CRDT semantics
5. **Coordination**: [004-coordination.md](004-coordination.md) - Cell management
6. **Security**: [005-security.md](005-security.md) - Auth and encryption

## Conventions

These specifications follow conventions from RFC 2119:

- **MUST**, **REQUIRED**, **SHALL**: Absolute requirement
- **MUST NOT**, **SHALL NOT**: Absolute prohibition
- **SHOULD**, **RECOMMENDED**: Valid reasons may exist to ignore
- **SHOULD NOT**, **NOT RECOMMENDED**: Valid reasons may exist to do otherwise
- **MAY**, **OPTIONAL**: Truly optional

## Relationship to ADRs

Architecture Decision Records (ADRs) in `docs/adr/` capture the rationale and evolution of design decisions. These protocol specifications distill ADRs into normative requirements for interoperability.

| Spec | Primary ADRs |
|------|--------------|
| 001-transport | ADR-010, ADR-030, ADR-032, ADR-042/043 |
| 002-sync | ADR-005, ADR-007, ADR-011, ADR-016 |
| 003-schema | ADR-012, ADR-020, ADR-028 |
| 004-coordination | ADR-004, ADR-014, ADR-024, ADR-027 |
| 005-security | ADR-006, ADR-044 |

## Feedback

Submit feedback via GitHub Issues with the label `spec/feedback`:

```
gh issue create --label "spec/feedback" --title "Spec 001: [Your Title]"
```

## License

These specifications are licensed under [MIT](../../LICENSE).
