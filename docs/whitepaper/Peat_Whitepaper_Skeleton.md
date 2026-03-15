# Peat PROTOCOL: BREAKING THE 20-PLATFORM WALL

**Open Coordination Architecture for Human-Machine Teams at Scale**

---

**Defense Unicorns**
https://defenseunicorns.com
December 2025

---

## EXECUTIVE SUMMARY

[~1.5 pages — This is the expanded one-pager. Should work standalone as a briefing document.]

Military autonomous systems programs consistently plateau at approximately 20 platforms. This isn't a technology gap—it's an architecture gap rooted in fundamental mathematics.

**The Problem**

[2-3 sentences: O(n²) scaling, tactical bandwidth constraints, physical impossibility at scale]

**The Insight**

[2-3 sentences: Military hierarchy as evolved communication optimization, O(n log n), hiding in plain sight]

**The Solution**

[2-3 sentences: Peat Protocol, hierarchical CRDT coordination, key validation results]

**The Imperative**

[2-3 sentences: Open architecture, coalition interoperability, 12-24 month window]

**Key Findings**

- The ~20 platform ceiling is architectural, not technological
- Military hierarchy is the coordination primitive that enables scale
- Peat achieves 95-99% bandwidth reduction through hierarchical aggregation
- Open standards are strategic necessity, not preference
- The architecture decision window is 12-24 months

**Document Navigation**

Readers from autonomy programs: See Section I.
C2 and doctrine community: See Section III.
Data architecture and AI community: See Section IV.

---

## I. THE SCALING CRISIS

**Thesis:** Every autonomous coordination program hits the same wall at ~20 platforms because O(n²) scaling is an architectural constraint, not a technology gap.

---

### 1.1 The Evidence Pattern

[Target: ~1 page]

The pattern is consistent across programs, services, and nations.

[Content to develop:]
- DIU Common Operational Database experience
- Observable plateau across swarm programs (pattern-level, not program-specific criticism)
- Demonstrations succeed; operational scaling fails
- The "integration challenges" diagnosis that masks the real issue

[Placeholder for specific examples/evidence]

---

### 1.2 The Mathematics of Failure

[Target: ~1 page]

The limitation is not software. It is not hardware. It is mathematics.

[Content to develop:]
- O(n²) message complexity explained accessibly
  - n=20: 400 messages per cycle (manageable)
  - n=100: 10,000 messages (saturation)
  - n=1,000: 1,000,000 messages (impossible)
- Tactical network reality: 9.6Kbps – 1Mbps
- Why "better algorithms" optimize within the constraint, don't escape it
- Why "more bandwidth" shifts the wall, doesn't remove it
- This is physics, not engineering

[Placeholder for diagram: O(n²) scaling curve with tactical bandwidth overlay]

---

### 1.3 The Operational Cost

[Target: ~0.5-1 page]

The ceiling has consequences.

[Content to develop:]
- Company-level human-machine formations remain theoretical
- Commanders forced to choose: scale OR coordination
- Centralized alternatives create single points of failure
- Brittleness in contested/degraded environments
- What missions can't be executed with current architecture

---

### Key Finding: Section I

> "The ~20 platform ceiling isn't a technology gap—it's an architecture gap. No optimization within mesh topologies escapes O(n²) scaling. The barrier is mathematical, and the solutions must be architectural."

---

## II. THE STANDARDS PARADOX

**Thesis:** Existing interoperability standards address platform control, not coordination at scale. The layer is missing, not broken.

---

### 2.1 What Exists—and What It Does Well

[Target: ~0.75 page]

The defense community has built extensive interoperability infrastructure.

[Content to develop:]
- STANAG 4586/4817: UAV/UGV control standards (successful at their purpose)
- JAUS: Service-oriented platform architecture
- FACE: Software portability
- ROS2/DDS: Messaging and middleware
- Link 16: Tactical data exchange

These standards work. The problem is what they don't address.

---

### 2.2 The Missing Layer

[Target: ~0.75 page]

Control is not coordination.

[Content to develop:]
- Controlling one platform ≠ orchestrating many
- All existing standards assume coordination happens "somewhere else"
- The gap: hierarchical coordination at scale
- Integration tax: more standards, more complexity, no scaling solution
- Each standard solves its problem; none solve THE problem

[Placeholder for diagram: Standards stack showing missing coordination layer]

---

### 2.3 The Proprietary Trap

[Target: ~0.5 page]

In the absence of open standards, proprietary solutions fill the gap.

[Content to develop:]
- Current coordination approaches are vendor-specific
- Coalition interoperability impossible across proprietary protocols
- Lock-in creates acquisition complexity
- Innovation constrained by vertical integration
- Each program's choice compounds the fragmentation

---

### Key Finding: Section II

> "Existing standards solve platform-level interoperability. None address hierarchical coordination at scale—they assume that layer exists. It doesn't. Peat fills this gap."

---

## III. THE HIERARCHY INSIGHT

**Thesis:** Military hierarchy isn't bureaucratic overhead—it's evolved communication optimization. Peat makes this pattern technical architecture.

---

### 3.1 The Reframe

[Target: ~1 page]

The conversation about autonomous systems treats hierarchy as obstacle. It is the solution.

[Content to develop:]
- Military organizations coordinated thousands in contested environments for centuries
- Hierarchy isn't about authority—it's about information flow
- Cognitive science: span of control exists for information-processing reasons
- Hierarchy compresses O(n²) to O(n log n)
- The pattern scales because it was designed to scale

Why does a platoon leader track 3 squad summaries instead of 24 platform states? Because human cognition—and network bandwidth—cannot process the alternative. Hierarchy is compression.

---

### 3.2 Three Information Flows

[Target: ~1 page]

Hierarchy defines how information moves.

[Content to develop:]

**Upward: Aggregation**
- Raw state → summarized capabilities
- Detail appropriate to echelon
- What the next level needs to make decisions

**Downward: Dissemination**
- Commander intent → specific taskings
- Translated at each level to actionable guidance
- "What" not "how"

**Lateral: Coordination**
- Peer synchronization within echelon
- Adjacent unit awareness
- Boundary management

These flows map directly to military doctrine because doctrine encodes the patterns that work.

[Placeholder for diagram: Three flows in hierarchy visual]

---

### 3.3 Capability Composition

[Target: ~0.75 page]

The hierarchy doesn't just aggregate status—it composes capabilities.

[Content to develop:]
- Platforms advertise what they can do, not just where they are
- Teams create emergent capabilities (ISR + compute + transport = deployable edge processing)
- Commanders task by requirement: "I need persistent surveillance of sector X"
- System handles platform allocation
- Automatic reallocation on platform failure
- Capability-based tasking vs. platform-by-platform management

---

### 3.4 Human-Machine Authority

[Target: ~0.75 page]

Hierarchy preserves human decision authority where it matters.

[Content to develop:]
- Configurable authority boundaries at each echelon
- What can execute autonomously vs. what requires approval
- Humans handle judgment; machines handle routine
- Graceful degradation: autonomous operation within last-known intent
- Disconnected operations continue with pre-delegated authorities
- Trust as architectural data, not just human factors

---

### Key Finding: Section III

> "Military hierarchy isn't the obstacle to autonomous coordination—it's the solution. The pattern reduces O(n²) to O(n log n) because it was evolved to solve exactly this problem. Peat implements hierarchy as technical architecture."

---

## IV. TECHNICAL ARCHITECTURE

**Thesis:** Peat achieves scalable coordination through CRDT-based eventual consistency, hierarchical aggregation, and edge-native design.

---

### 4.1 Core Mechanisms

[Target: ~1 page]

Coordination at scale requires different foundations than traditional distributed systems.

[Content to develop:]

**CRDTs for Eventual Consistency**
- Conflict-free Replicated Data Types: merge without coordination
- No consensus required—critical for DIL environments
- Deterministic convergence regardless of message order
- Partition tolerance as feature, not failure mode

**Why Consensus Fails**
- Consensus (Paxos, Raft) requires majority availability
- DIL networks: disconnected, intermittent, limited
- Consensus blocks; CRDTs proceed
- Tactical networks are DIL by definition

**Reconvergence**
- Nodes operate autonomously during partition
- Deterministic state merge on reconnection
- No "split brain"—mathematical guarantees

---

### 4.2 Hierarchical Aggregation

[Target: ~1 page]

The bandwidth reduction comes from aggregation, not compression.

[Content to develop:]

**Differential Synchronization**
- Only changes propagate
- CRDT operations, not full state
- Further reduction: sync summaries, not details

**Aggregation Algebra**
- How individual capabilities combine
- Squad capabilities → platoon capabilities → company capabilities
- Each level: appropriate granularity for decisions at that level
- 87% reduction: platoon leader sees 3 summaries, not 24 states

**Validated Results**
- 95-99% bandwidth reduction vs. full replication
- O(n log n) message complexity confirmed
- Tactical networks that choke at 50 nodes → 1,000+ coordination

---

### 4.3 Three-Layer Architecture

[Target: ~0.75 page]

Separation enables integration flexibility.

[Content to develop:]

**cap-schema**
- Message definitions
- Capability ontology
- Protocol-buffer based, language-agnostic
- Extensible for domain-specific needs

**cap-transport**
- Protocol adapters
- Tactical radios, satellite links, mesh networks, QUIC
- Multi-path: simultaneous bearers
- Network-agnostic coordination logic

**cap-core**
- Coordination logic
- Aggregation rules
- Hierarchical routing
- Authority enforcement

[Placeholder for diagram: Three-layer stack with integration points]

---

### 4.4 AI as Coordinator

[Target: ~0.75 page]

AI participates in the hierarchy, not just at the edge.

[Content to develop:]
- Current model: AI for perception on individual platforms
- Peat model: AI as team member at every echelon
- Formation-level AI: optimization within commander's intent
- Platform-level AI: perception, local decisions
- AI capability advertisement: "Which assets can identify [target type] with >90% confidence?"
- Edge-native: works disconnected, syncs when possible
- Models run where decisions are made

---

### 4.5 Validation Status

[Target: ~0.5 page]

Laboratory validation confirms the architecture.

[Content to develop:]

| Phase | Configuration | Results |
|-------|--------------|---------|
| 1 | 2-node bidirectional | <1s latency, 100% consistency |
| 2 | 12-node squad | 26s convergence |
| 3 | 24-node platoon | 54s convergence, 6.1s mean |

- Bandwidth: 95-99% reduction confirmed
- Architecture validated for 1,000+ (simulation)
- Integration pathways proven: TAK/CoT, ROS2, STANAG 4586 bridging
- TRL 4-5 achieved

---

### Key Finding: Section IV

> "Peat achieves O(n log n) scaling through hierarchical CRDT-based coordination—enabling 1,000+ platform operations on networks that saturate at 50 with mesh approaches. Lab validation confirms 95-99% bandwidth reduction."

---

## V. THE OPEN ARCHITECTURE IMPERATIVE

**Thesis:** Coordination infrastructure must be open—for coalition interoperability, innovation velocity, and acquisition simplicity.

---

### 5.1 The TCP/IP Lesson

[Target: ~0.75 page]

Infrastructure wins by enabling, not capturing.

[Content to develop:]
- TCP/IP became universal by being nobody's proprietary advantage
- Open protocols create ecosystems; proprietary protocols create dependencies
- The internet's architecture enabled permissionless innovation
- Peat positions as coordination substrate—not product, but infrastructure
- Revenue from services, not protocol licensing

---

### 5.2 Coalition Requirements

[Target: ~0.75 page]

Allied operations require shared infrastructure.

[Content to develop:]
- AUKUS trilateral coordination at scale
- NATO interoperability across 31 nations
- Proprietary protocols create licensing barriers on top of classification barriers
- Allies need source code inspection for trust
- "Releasability" easier when there's nothing proprietary to protect
- Coalition of the willing limited by coalition of the compatible

---

### 5.3 Innovation Economics

[Target: ~0.5 page]

Open standards accelerate capability development.

[Content to develop:]
- Open standards create "violent competition" at application layer
- Proprietary lock-in: local optimization, global stagnation
- Defense primes benefit from level playing field (reduced integration risk)
- SME participation expands innovation surface
- Best implementations win, not first movers with lock-in

---

### 5.4 Acquisition Alignment

[Target: ~0.5 page]

Open architecture aligns with policy.

[Content to develop:]
- MOSA (Modular Open Systems Approach) mandates
- NDAA requirements for open systems
- GOTS (Government Off-The-Shelf) positioning
- Reduced program risk through transparency
- Faster ATO: full source inspection available
- No per-unit licensing complexity

---

### Key Finding: Section V

> "Open coordination infrastructure isn't idealism—it's strategic necessity. Coalition interoperability, innovation velocity, and acquisition policy all point the same direction. The question is whether we get there by design or by crisis."

---

## VI. WHY NOW

**Thesis:** A 12-24 month window exists to establish open coordination standards before proprietary fragmentation locks in suboptimal architectures for a generation.

---

### 6.1 Converging Forces

[Target: ~0.75 page]

Multiple trends create urgency simultaneously.

[Content to develop:]

**Replicator Initiative**
- DoD explicitly prioritizing mass autonomous deployment
- Thousands of platforms, near-term timelines
- Coordination infrastructure is implicit requirement

**Ukraine Lessons**
- Adaptable, attritable systems beating exquisite platforms
- Rapid iteration over long development cycles
- Commercial technology integration at speed

**AI Maturation**
- Edge inference capable and deployable
- Foundation models enable AI coordination, not just perception
- The capability exists; the infrastructure doesn't

**Indo-Pacific Reality**
- AUKUS coordination across vast operational distances
- Allied interoperability as strategic necessity
- Tyranny of distance demands distributed coordination

---

### 6.2 The Standardization Race

[Target: ~0.5 page]

The architecture is being decided now.

[Content to develop:]
- Multiple proprietary approaches competing for adoption
- First adequate solution gets network effects
- Switching costs compound with each program decision
- NATO STANAG process: 4-5 years from concept to ratification
- Open standard foundation must be laid now to be ready in time

---

### 6.3 The Cost of Waiting

[Target: ~0.5 page]

Delay has compounding consequences.

[Content to develop:]
- Each proprietary adoption creates switching costs
- Integration complexity multiplies with incompatible approaches
- Coalition interoperability gaps widen
- Adversaries not waiting for Western consensus
- Architecture decisions made in 2025-2026 constrain options for a decade

---

### Key Finding: Section VI

> "The architecture decision is being made now. Open infrastructure that enables coalition coordination, or proprietary fragmentation that prevents it. The window for deliberate choice is 12-24 months."

---

## VII. PATH FORWARD

**Thesis:** Peat is validated, integration-ready, and positioned for standardization with clear pathways for pilot programs and adoption.

---

### 7.1 Current State

[Target: ~0.5 page]

Peat is ready for integration pilots.

[Content to develop:]
- TRL 4-5: Laboratory validated
- Reference implementation: Rust + Automerge + Iroh
- Three-layer architecture enables flexible integration
- Apache 2.0 licensing: no barriers to government or allied use
- Integration pathways proven: TAK/CoT, ROS2, STANAG 4586 bridging

---

### 7.2 Integration Strategy

[Target: ~0.5 page]

Programs choose integration depth based on requirements.

[Content to develop:]

**Shallow Integration**
- Protocol adapters
- Peat coordinates existing C2 outputs
- Minimal changes to current systems

**Medium Integration**
- Capability translation layer
- Legacy platforms participate via gateway
- Incremental adoption

**Deep Integration**
- Native Peat implementation
- Full capability advertisement
- New platforms designed for Peat

---

### 7.3 Standardization Trajectory

[Target: ~0.5 page]

Multiple paths reinforce each other.

[Content to develop:]

**Near-term (12-24 months)**
- Open consortium formation
- Technical specification publication
- Reference implementation maturation
- IETF draft RFC

**Medium-term (2-3 years)**
- SAE standard via AS4 Unmanned Systems Committee
- Industry adoption and conformance testing
- Commercial implementations

**Long-term (4-5 years)**
- NATO STANAG pathway
- Allied nation adoption
- Full coalition interoperability

---

### 7.4 Recommendations

[Target: ~0.75 page]

**For Program Managers**
- Evaluate Peat integration for multi-platform coordination requirements
- Assess current architectures against O(n²) scaling limits
- Identify pilot opportunities for operational validation
- Engage with consortium development

**For Acquisition Professionals**
- Recognize coordination architecture as distinct requirement from platform control
- Apply MOSA preference to coordination layer procurement
- Consider GOTS benefits for coalition programs
- Evaluate proprietary dependencies for long-term risk

**For Technical Evaluators**
- Assess current program coordination scaling assumptions
- Evaluate proprietary dependencies for coalition interoperability risk
- Review Peat technical specifications for integration feasibility
- Consider three-layer architecture for incremental adoption

**For Strategic Decision-Makers**
- Prioritize open coordination standard development in autonomous systems roadmaps
- Engage NATO partners on coordination architecture interoperability
- Resource standardization participation through appropriate channels
- Recognize 12-24 month decision window

---

### Key Finding: Section VII

> "Peat is validated and integration-ready. The path from current state to NATO STANAG is clear. What's required is engagement: pilot programs, consortium participation, and strategic prioritization of open coordination infrastructure."

---

## CONCLUSION

[~0.5 page]

The 20-platform wall is real. It is mathematical. And it will constrain every autonomous systems program until we solve it architecturally.

The solution is not more technology. It is recognizing that military hierarchy—the organizational pattern that has enabled coordinated human action at scale for millennia—is the architecture that enables machine coordination at scale.

Peat Protocol implements this insight: hierarchical aggregation, capability composition, human-machine authority, built on CRDT foundations that work in the contested, bandwidth-constrained environments where these systems must operate.

The architecture decision is being made now. Every program that adopts proprietary coordination creates switching costs that compound coalition interoperability challenges. Open infrastructure enables the ecosystem; proprietary fragmentation constrains it.

The window is 12-24 months. The foundation must be built now.

---

**Defense Unicorns**
https://defenseunicorns.com

**Contact:** [contact info]

**Request technical documentation:** [link]

---

## APPENDICES

[To be developed as needed]

### Appendix A: Technical Specifications
- CRDT implementation details
- Protocol specifications
- Schema definitions
- API documentation

### Appendix B: Validation Data
- Test environment specifications
- Detailed performance measurements
- Scaling projections and analysis

### Appendix C: Standards Landscape
- Existing standards summary table
- Gap analysis detail
- Standardization pathway specifics

### Appendix D: Glossary
- Acronyms
- Technical terms
- Doctrine references

---

## DOCUMENT METADATA

**Version:** Draft 0.1
**Date:** December 2025
**Classification:** UNCLASSIFIED
**Distribution:** Public Release

**Suggested Citation:**
Plummer, K. (2025). "Peat Protocol: Breaking the 20-Platform Wall." Defense Unicorns.

---

*Word count target: 4,000-5,000 words body text (~15-18 pages formatted)*
*Current skeleton: ~2,800 words including placeholders*
