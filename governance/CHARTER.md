# HIVE Protocol Project Charter

## 1. Mission

The HIVE Protocol project exists to develop and maintain an open standard for hierarchical coordination of autonomous systems, enabling scalable, partition-tolerant coordination across defense, commercial, and research applications.

## 2. Vision

A world where autonomous systems from any vendor, nation, or organization can coordinate effectively using a common, open protocol—reducing integration costs, enabling interoperability, and accelerating innovation.

## 3. Principles

### 3.1 Open Standards

- The specification is freely available (CC0/CC BY licensed)
- Anyone can implement the protocol without licensing fees
- Multiple implementations ensure ecosystem health

### 3.2 Technical Excellence

- Decisions are driven by technical merit and evidence
- Claims require validation through experimentation
- Flaky tests and unverified claims are unacceptable

### 3.3 Inclusive Collaboration

- Contributions are welcome from individuals, companies, and governments
- Merit-based advancement regardless of affiliation
- Transparent decision-making processes

### 3.4 Practical Utility

- The protocol must solve real operational problems
- Standards must be implementable on constrained platforms
- Complexity is justified only by operational need

## 4. Scope

### 4.1 In Scope

- Protocol specification development and maintenance
- Reference implementation development
- Conformance test suite development
- Documentation and educational materials
- Interoperability testing and certification
- Standards body submissions (IETF, NATO, etc.)

### 4.2 Out of Scope

- Specific platform integrations (except as examples)
- Proprietary extensions incompatible with the standard
- Military doctrine or tactics development
- Weapon system development

## 5. Governance Structure

### 5.1 Project Lead

The Project Lead provides technical direction and makes final decisions when consensus cannot be reached.

**Current Project Lead**: Kit Plummer (kit@revolveteam.com)

### 5.2 Contributors

Anyone who contributes to the project through:
- Code contributions
- Documentation improvements
- Bug reports and feature requests
- Testing and validation
- Specification review and feedback

### 5.3 Maintainers

Contributors who have demonstrated:
- Sustained, quality contributions
- Understanding of project goals and architecture
- Commitment to collaborative development

Maintainers have commit access and participate in design decisions.

### 5.4 Technical Steering Committee (Future)

When the project reaches sufficient scale, a Technical Steering Committee will be formed to:
- Guide technical direction
- Resolve design disputes
- Approve specification changes
- Manage standards submissions

## 6. Decision Making

### 6.1 Consensus-Seeking

Decisions are made through consensus-seeking:

1. Proposal is submitted (GitHub issue or RFC)
2. Community discussion and feedback
3. Iteration based on feedback
4. Call for consensus
5. If consensus, decision is adopted
6. If not, escalation to Project Lead

### 6.2 Specification Changes

Changes to the normative specification require:

1. RFC document describing the change
2. Reference implementation demonstrating feasibility
3. Review period (minimum 2 weeks)
4. No unresolved objections from Maintainers

### 6.3 Breaking Changes

Breaking changes to the wire format require:

1. New major version number
2. Migration guide
3. Extended review period (minimum 4 weeks)
4. Explicit approval from Project Lead

## 7. Intellectual Property

### 7.1 Specification License

- Specification documents: CC BY 4.0
- Protocol Buffer definitions: CC0 1.0 (public domain)

### 7.2 Implementation License

- Reference implementation: MIT OR Apache-2.0
- Contributors retain copyright, grant license to project

### 7.3 Patent Policy

See [PATENT_PLEDGE.md](PATENT_PLEDGE.md) for the project's patent commitment.

Key points:
- Patent grant for conforming implementations
- Defensive termination for patent aggressors
- Royalty-free for standards compliance

## 8. Standards Strategy

### 8.1 IETF Track

1. Internet-Draft submission
2. Working group adoption (if applicable)
3. Standards track publication

### 8.2 NATO Track

1. Technical demonstrations with allied nations
2. Study submission to NATO Standardization Office
3. STANAG proposal development
4. Ratification process

### 8.3 Other Standards Bodies

The project may engage with:
- IEEE (robotics standards)
- SAE (autonomous vehicle standards)
- ISO (quality and safety standards)

## 9. Code of Conduct

All participants are expected to:

- Be respectful and constructive
- Focus on technical merit
- Welcome newcomers
- Assume good faith
- Respect confidentiality when required

Violations should be reported to the Project Lead.

## 10. Amendments

This charter may be amended by:

1. Proposal submitted as GitHub issue
2. Community discussion (minimum 2 weeks)
3. Approval by Project Lead
4. For major changes, approval by majority of Maintainers

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2025-12 | Initial charter |

---

## Contact

- GitHub: https://github.com/kitplummer/hive
- Email: kit@revolveteam.com
