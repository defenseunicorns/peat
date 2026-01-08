# HIVE Protocol - Patent Strategy

**Status**: Proposed
**Date**: 2025-11-04
**Author**: Codex (AI Assistant)
**Reviewed By**: Kit Plummer

## Executive Summary

**Recommendation**: File 2 provisional patent applications immediately (within 1 week) covering HIVE Protocol's core innovations, then open-source the automerge-edge infrastructure.

- **Cost**: $260 (DIY provisionals) or $4K-$6K (with attorney)
- **Timeline**: File this week, then open-source freely
- **Decision Point**: Month 12 - convert to utility patents or abandon
- **Strategic Value**: Defensive protection + licensing optionality + "patent pending" status

## Background

### Current Situation

- **No public disclosure yet** - Only private conversations with customers/integrators
- **12-month grace period** - US law allows filing within 12 months of first public disclosure
- **Optimal timing** - File provisionals BEFORE open-sourcing automerge-edge
- **Low risk/cost** - Provisionals are inexpensive and buy 12 months of decision time

### Apache-2.0 License Context

The Apache-2.0 license provides **patent protection for users**, not patent generation:

- **Defensive patent grant**: Contributors automatically grant patent licenses
- **Patent retaliation**: If someone sues over patents, they lose their license
- **User protection**: Downstream users get patent rights to use the software

This protects *users* from patent trolls, but doesn't prevent (r)evolve from filing patents on innovations.

## Patent Strategy Analysis

### Arguments FOR Filing Patents

1. **Defensive Portfolio**
   - Protects against patent trolls in defense industry
   - Provides negotiating leverage if competitors assert patents
   - Common practice in defense tech (Anduril, Palantir, Shield AI all file patents)

2. **Technology Transfer Value**
   - Patents can be licensed to primes (Lockheed, Northrop, BAE Systems)
   - Government may want exclusive or government-purpose rights
   - Increases company valuation for investors/acquisition

3. **Government Contracts**
   - DoD SBIR/STTR programs favor patentable innovations
   - "Intellectual property rights" are negotiable in contracts
   - Patents demonstrate technical novelty/non-obviousness

4. **Open Source + Patents Coexist**
   - Red Hat, Google, IBM all file patents on open-source software
   - Strategy: "Open source implementation, patented innovation"
   - Protects commercial services/support business model

### Arguments AGAINST Filing Patents

1. **Cost and Effort**
   - $10K-$30K per patent (filing + prosecution)
   - 2-3 years to issuance
   - Ongoing maintenance fees
   - Diverts resources from development

2. **Open Source Philosophy**
   - Patents contradict GOTS/open collaboration narrative
   - May deter NATO allies or academic contributors
   - Could be seen as "rug-pull" on open community

3. **Enforcement Challenges**
   - Patents are only valuable if you can/will enforce
   - Litigation costs $500K-$5M+ per case
   - Government has sovereign immunity (can't sue DoD)

4. **Prior Art Risk**
   - Distributed systems, CRDTs, P2P mesh are well-studied
   - Automerge, Ditto, IPFS, libp2p have similar concepts
   - Risk of patent being invalidated or narrowed

## Recommended Strategy

### Core Principle

**"Patent the mission-specific innovations, open-source the infrastructure"**

This provides:
- ✅ Defensive protection for CAP's unique value
- ✅ Licensing revenue potential
- ✅ Pure open-source narrative for automerge-edge
- ✅ NATO standardization path for infrastructure
- ✅ Flexibility to choose strategy later

### What TO Patent

#### 1. Hierarchical Capability Composition Rules

**Innovation**: Additive, emergent, redundant, and constraint-based composition patterns for distributed autonomous systems.

**Core Claims**:
- Additive composition: Squad capabilities = union of member capabilities
- Emergent composition: New capabilities arise from cell formation
- Redundant composition: Capability requirements with min/max thresholds
- Constraint-based composition: Mutual exclusions, dependencies
- CRDT-based conflict resolution for composition rules
- O(n log n) message complexity through hierarchical aggregation

**Why Valuable**:
- Core differentiator from generic mesh networks
- Applicable beyond military: IoT orchestration, cloud resource management, autonomous fleets
- Defensive against competitors (Anduril, Shield AI)

**Prior Art Differentiation**:
- Existing: Flat capability matching (Kubernetes, Nomad, Docker Swarm)
- CAP: Hierarchical composition with emergent properties + CRDT consistency

**References**: ADR-004, E6.1, E6.2 implementations

#### 2. Graduated Human Authority Control for Distributed Autonomous Coordination

**Innovation**: Hierarchical human-in-the-loop authority levels enforced in distributed, partition-tolerant CRDT systems.

**Core Claims**:
- Authority level taxonomy (FULL_AUTO, SUPERVISED, HUMAN_APPROVAL, HUMAN_VETO, MANUAL)
- Distributed human-in-the-loop enforcement using CRDTs
- Authority level propagation through cell hierarchies
- Timeout/unavailability handling for approval requests
- Audit trail for autonomous decisions and human overrides

**Why Valuable**:
- Critical for safety-critical autonomous systems (DoD Directive 3000.09 compliance)
- Growing regulatory requirements for AI/autonomy (EU AI Act, etc.)
- Applicable to medical robotics, industrial automation, autonomous vehicles

**Prior Art Differentiation**:
- Existing: Centralized authority control (single human supervisor)
- CAP: Distributed authority with P2P mesh, eventual consistency, partition tolerance

**References**: ADR-004, human-machine-teaming-design.md

### What NOT to Patent

1. **Automerge-edge infrastructure** (storage, discovery, transport)
   - Generic distributed systems technology (extensive prior art)
   - Community contribution model incompatible with patents
   - Goal is NATO standardization (patents would hinder)

2. **CRDT implementations** (LWW-Register, OR-Set, PN-Counter)
   - Well-established computer science (academic literature)
   - Automerge already has implementations (derivative work issues)

3. **mDNS discovery, TCP/QUIC transport**
   - Standard protocols, no novelty

## Implementation Plan

### Phase 1: File Provisional Patents (Week 1)

**Timeline**: This week (before any public disclosure)

**Deliverables**:
1. Technical disclosure document for Provisional #1 (Capability Composition)
2. Technical disclosure document for Provisional #2 (Human Authority Control)
3. USPTO provisional patent applications filed

**Cost Options**:

| Option | Cost | Timeline | Pros | Cons |
|--------|------|----------|------|------|
| **DIY Filing** | $260 | 1 week | Fast, cheap, establishes priority | Claims may not be optimal |
| **Budget Attorney** | $4K-$6K | 2-4 weeks | Better claims, professional review | More expensive, slower |
| **Full-Service Firm** | $10K+ | 4-6 weeks | Maximum protection | Very expensive, very slow |

**Recommendation**: Start with DIY filing to establish priority date immediately, optionally hire attorney for utility conversion at month 12.

### Phase 2: Open Source Release (Week 2)

**Timeline**: Immediately after provisional filing

**Actions**:
- Open-source automerge-edge on GitHub (Apache-2.0 license)
- Release HIVE Protocol documentation
- Publish technical papers/blog posts
- Present at conferences

**Protection**: Provisionals filed before disclosure - priority date established

**Marketing**: Can optionally include "patent pending" notice in materials

### Phase 3: Market Validation (Months 1-12)

**Gather evidence to inform utility patent decision**:

- **Customer feedback**: Do customers value patent protection? Do contracts require IP?
- **Competitive landscape**: Are competitors filing similar patents? Any patent threats?
- **Investor requirements**: Do investors require patents for valuation?
- **Government contracts**: Do SBIR/STTR programs or prime contractors care?
- **Licensing opportunities**: Any interest in commercial licensing?

**Cost during this phase**: $0 (provisionals are valid for 12 months)

### Phase 4: Utility Patent Decision (Month 12)

**Decision Point**: November 2025

**Option A: Convert to Utility Patents**
- **Cost**: $20K-$40K ($10K-$20K per patent)
- **Timeline**: 2-3 years to issuance
- **Choose if**:
  - Government customer wants exclusive license
  - Competitor files similar patents
  - Investors require IP for valuation
  - Strong evidence of commercial value

**Option B: Abandon Provisionals**
- **Cost**: $0
- **Outcome**: Technical disclosures become defensive prior art
- **Choose if**:
  - Open-source strategy working well
  - No competitive patent threats
  - Customers don't care about patents
  - Want pure GOTS/open collaboration narrative

## Technical Disclosure Outlines

### Provisional #1: Hierarchical Capability Composition

**Title**: "Hierarchical Capability Composition for Distributed Autonomous Systems"

**Sections**:
1. **Background**
   - Problem: Flat capability matching doesn't scale for hierarchical autonomous systems
   - Prior art: Kubernetes pods, Docker Swarm services, Nomad jobs
   - Limitations: No emergent properties, no hierarchical aggregation, centralized orchestration

2. **Summary of Invention**
   - System and method for composing capabilities in hierarchical distributed autonomous systems
   - Uses CRDTs for conflict-free merging of capability advertisements
   - Supports additive, emergent, redundant, and constraint-based composition rules
   - Achieves O(n log n) message complexity through hierarchical aggregation

3. **Detailed Description**
   - Cell hierarchy (Platform → Squad → Platoon → Company)
   - Capability advertisement and propagation
   - Composition rule types (additive, emergent, redundant, constraint)
   - CRDT-based conflict resolution (LWW-Register for capability state)
   - Message complexity analysis (flat O(n²) vs hierarchical O(n log n))

4. **Example Implementation**
   - Code excerpts from E6.1 (composition rule framework)
   - Code excerpts from E6.2 (additive composition rules)
   - Use case: ISR mission with UAV + ground sensor + human analyst capabilities

5. **Claims** (rough claims for provisional)
   - System for hierarchical capability composition in autonomous systems
   - Method for emergent capability generation through cell formation
   - CRDT-based conflict resolution for distributed capability state
   - Hierarchical aggregation algorithm for O(n log n) message complexity

6. **Figures**
   - Cell hierarchy diagram
   - Capability composition flowchart
   - Message complexity comparison (flat vs hierarchical)
   - Example mission scenario (ISR cell formation)

### Provisional #2: Graduated Human Authority Control

**Title**: "Graduated Human Authority Control for Distributed Autonomous Coordination Systems"

**Sections**:
1. **Background**
   - Problem: Centralized human control doesn't work in distributed, partition-prone tactical networks
   - Prior art: Supervisory control systems, teleoperation, centralized HMI
   - Limitations: Single point of failure, doesn't handle network partitions, binary autonomy

2. **Summary of Invention**
   - System and method for graduated human authority control in distributed autonomous systems
   - Authority level taxonomy (FULL_AUTO, SUPERVISED, HUMAN_APPROVAL, HUMAN_VETO, MANUAL)
   - Distributed enforcement using CRDTs (survives network partitions)
   - Authority level propagation through cell hierarchies
   - Timeout handling for unavailable human operators

3. **Detailed Description**
   - Five authority levels and their semantics
   - CRDT-based authority state representation (LWW-Register)
   - Approval request protocol (distributed human-in-the-loop)
   - Timeout and unavailability handling
   - Audit trail for autonomous decisions and human overrides
   - Hierarchical authority propagation (children inherit parent constraints)

4. **Example Implementation**
   - Code excerpts from human-machine-teaming-design.md
   - Code excerpts from ADR-004 (authority level enforcement)
   - Use case: Autonomous weapon system with human approval required (DoD 3000.09)

5. **Claims** (rough claims for provisional)
   - System for graduated human authority in distributed autonomous systems
   - Method for distributing human approval requests in partition-tolerant network
   - CRDT-based authority state propagation through hierarchies
   - Timeout handling for unavailable human operators with safety fallbacks

6. **Figures**
   - Authority level taxonomy diagram
   - Approval request protocol flowchart
   - Network partition scenario (authority enforcement under partition)
   - Hierarchical authority propagation (parent constraints inherited by children)

## Risk Mitigation

### What if provisionals expire without conversion?

- ✅ Still created defensive prior art (blocks others from patenting)
- ✅ Only cost was $260 + weekend of work
- ✅ Still have full open-source rights
- ✅ Technical disclosures remain valuable documentation

### What if competitors file similar patents?

- ✅ Your provisional establishes priority date (first to file)
- ✅ Can cite your provisional as prior art in competitor's prosecution
- ✅ Can publish technical disclosures to create additional prior art

### What if you need to enforce later?

- ❌ Can't enforce after abandonment
- ✅ Can still use commercially (open source)
- ✅ Defensive value (prior art) persists forever

### What if open-source community objects to patents?

- ✅ Patents are on CAP-specific innovations, not automerge-edge infrastructure
- ✅ Apache-2.0 license already includes patent grant (users are protected)
- ✅ Common practice for commercial open-source (Red Hat, Google, etc.)
- ✅ Can donate patents to Open Invention Network or similar defensive pool

## Decision Criteria for Month 12

Convert to utility patents if **any** of these are true:

1. **Customer demand**: Government customer wants exclusive license or patent protection
2. **Competitive threat**: Competitor files similar patents or asserts patents against you
3. **Investor requirement**: Investors require patents for Series A valuation
4. **Licensing opportunity**: Strong interest in commercial licensing from primes/integrators
5. **Strategic value**: Patents provide negotiating leverage in M&A discussions

Abandon provisionals if **all** of these are true:

1. **No competitive threats**: No competitor patents filed, no patent assertions
2. **Customers don't care**: Government customers happy with GOTS approach
3. **Open-source momentum**: Strong community contributions, NATO interest
4. **Cost-benefit analysis**: $40K utility patents not worth strategic value

## Next Steps

### Immediate (This Week)

- [ ] Draft technical disclosure for Provisional #1 (Capability Composition)
- [ ] Draft technical disclosure for Provisional #2 (Human Authority Control)
- [ ] Decide: DIY filing or hire attorney
- [ ] File provisionals with USPTO ($260 or $4K-$6K)
- [ ] Receive filing receipts with priority dates

### Week 2

- [ ] Open-source automerge-edge on GitHub (now protected)
- [ ] Update HIVE Protocol documentation
- [ ] Optionally add "patent pending" notices

### Months 1-12

- [ ] Track customer feedback on patent value
- [ ] Monitor competitive patent filings (USPTO search, Google Patents)
- [ ] Collect evidence for utility patent decision
- [ ] Schedule Month 12 decision meeting (November 2025)

### Month 12 (November 2025)

- [ ] Review evidence collected during year
- [ ] Decide: Convert to utility or abandon
- [ ] If converting: Hire patent attorney for utility prosecution
- [ ] If abandoning: Publish technical disclosures as defensive prior art

## Resources

### USPTO Resources

- **Provisional Patent Application**: https://www.uspto.gov/patents/basics/types-patent-applications/provisional-application-patent
- **Filing Fees**: https://www.uspto.gov/learning-and-resources/fees-and-payment/uspto-fee-schedule
- **EFS-Web (Online Filing)**: https://www.uspto.gov/patents/apply/efs-web-patent

### Patent Attorney Referrals

- **National Law Review Directory**: https://www.natlawreview.com/
- **USPTO Registered Attorney Search**: https://oedci.uspto.gov/OEDCI/
- **Defense Tech Specialists**: Many patent attorneys specialize in defense/aerospace

### Prior Art Search

- **Google Patents**: https://patents.google.com/
- **USPTO Public Search**: https://ppubs.uspto.gov/pubwebapp/
- **Academic Literature**: IEEE Xplore, ACM Digital Library (CRDT research)

## References

- [ADR-004: Human-Machine Cell Composition](adr/004-human-machine-cell-composition.md)
- [ADR-007: Automerge-Based Sync Engine](adr/007-automerge-based-sync-engine.md)
- [Human-Machine Teaming Design](human-machine-teaming-design.md)
- [E6.1: Composition Rule Framework](../hive-protocol/src/cell/composition.rs)
- [E6.2: Additive Composition Rules](../hive-protocol/src/cell/additive_composition.rs)

## Approval

| Stakeholder | Date | Decision | Notes |
|-------------|------|----------|-------|
| Kit Plummer | TBD | Pending | Reviewing strategy |
| (r)evolve Leadership | TBD | Pending | Budget approval for attorney? |
| Legal Counsel | TBD | Pending | Optional - review before filing |

---

**Document Version**: 1.0
**Last Updated**: 2025-11-04
**Next Review**: After provisional filing (Week 2)
