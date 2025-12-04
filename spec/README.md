# HIVE Protocol Specification

This directory contains the **normative specification** for the Hierarchical Intelligence for Versatile Entities (HIVE) Protocol.

## Purpose

The contents of this directory define the **standard**—the protocol that any compliant implementation MUST follow. This specification is designed to be:

1. **Implementation-agnostic**: Any language or platform can implement HIVE using these specifications
2. **Testable**: Clear requirements enable conformance testing
3. **Extensible**: Versioned schemas with backward compatibility guarantees

## Directory Structure

```
spec/
├── README.md                           # This file
├── draft-hive-protocol-00.md           # Main protocol specification (RFC-style)
├── proto/                              # Normative Protocol Buffer definitions
│   └── cap/
│       └── v1/
│           ├── common.proto            # Common types
│           ├── node.proto              # Node model
│           ├── capability.proto        # Capability model
│           ├── cell.proto              # Cell (squad) model
│           ├── beacon.proto            # Discovery beacons
│           ├── composition.proto       # Composition rules
│           ├── hierarchy.proto         # Hierarchical aggregation
│           ├── command.proto           # Command dissemination
│           └── security.proto          # Security messages
├── examples/                           # Normative message examples
└── diagrams/                           # Specification diagrams
```

## Normative vs. Informative

| Content | Location | Status |
|---------|----------|--------|
| Protocol specification | `spec/` | **Normative** - defines REQUIRED behavior |
| Protocol Buffer schemas | `spec/proto/` | **Normative** - wire format definition |
| Reference implementation | `reference/` | **Informative** - one valid implementation |
| Architecture decisions | `docs/adr/` | **Informative** - design rationale |
| Experiments | `labs/` | **Informative** - research and validation |

## Versioning

The specification follows [Semantic Versioning](https://semver.org/):

- **Major version**: Breaking changes to wire format or protocol semantics
- **Minor version**: Backward-compatible additions
- **Patch version**: Clarifications and editorial changes

Current version: **0.1.0** (Draft)

## RFC 2119 Keywords

This specification uses keywords as defined in [RFC 2119](https://tools.ietf.org/html/rfc2119):

| Keyword | Meaning |
|---------|---------|
| **MUST** | Absolute requirement |
| **MUST NOT** | Absolute prohibition |
| **SHOULD** | Recommended but not required |
| **SHOULD NOT** | Not recommended but not prohibited |
| **MAY** | Optional |

## License

- **Specification documents** (`.md` files): [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) - share and adapt with attribution
- **Protocol Buffer definitions** (`.proto` files): [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) - public domain, no restrictions
- **Reference implementation** (in `/reference/`): MIT OR Apache-2.0

This licensing ensures anyone can implement the HIVE Protocol without legal barriers while maintaining attribution for the specification work.

## Standards Track

This specification is being developed with the goal of submission to:

1. **IETF** - Internet-Draft for broader internet/networking community adoption
2. **NATO** - STANAG proposal for defense interoperability (complements STANAG 4586, 7023)
3. **Open standards recognition** - enabling multi-vendor, multi-national implementations

## Related Documents

| Document | Purpose |
|----------|---------|
| [draft-hive-protocol-00.md](draft-hive-protocol-00.md) | Main protocol specification |
| [proto/README.md](proto/README.md) | Schema documentation |
| [../governance/CHARTER.md](../governance/CHARTER.md) | Project governance |
| [../governance/CONTRIBUTING.md](../governance/CONTRIBUTING.md) | Contribution guidelines |
| [../governance/PATENT_PLEDGE.md](../governance/PATENT_PLEDGE.md) | IP commitments for implementers |

## Contact

For questions about this specification:
- GitHub Issues: [github.com/kitplummer/hive/issues](https://github.com/kitplummer/hive/issues)
- Email: kit@revolveteam.com
