# Prior Art Disclosure - Addendum for Patent Applications

**Instructions**: Add this section to both provisional patent applications after the "Background" section and before "Summary of the Invention".

---

## Related Work and Differentiation from Prior Art

The inventors acknowledge prior work in distributed autonomous systems coordination, particularly the **COD (Collaborative Operations in Denied Environments)** project developed under Defense Innovation Unit (DIU) contract.

### COD Project Context

**COD Overview**:
- Developed by Ditto Technologies under DIU contract (2021-2023)
- Public information: https://www.diu.mil/solutions/portfolio/catalog/a0T83000000EttSEAS-a0hcr000003k909AAA
- Mission: Enable commercial AI solutions for Department of Defense in denied/degraded environments
- Focus: Resilient mesh networking for autonomous coordination

**Inventor's Prior Involvement**:
- Inventor Kit Plummer contributed to COD development while employed at Ditto Technologies
- COD work focused on resilient peer-to-peer networking and mesh coordination
- General autonomous systems expertise and domain knowledge gained through COD participation

**Concepts Derived from COD**:
- **Prioritization**: Concept of priority-based data synchronization in bandwidth-constrained environments
  - COD demonstrated prioritizing critical mission data over routine telemetry
  - Peat Protocol may incorporate similar prioritization concepts (not currently claimed in this application)

**Note**: Prioritization concepts are **NOT claimed** in the present patent applications. If future patent applications include prioritization, they will explicitly cite COD as prior art and claim only novel improvements.

### Differentiation: Peat Protocol Innovations

The present invention (Peat Protocol) was developed independently at Defense Unicorns LLC (2024-2025) and differs substantially from COD in the following critical ways:

#### Innovation 1: Hierarchical Capability Composition (Not in COD)

**COD Approach**:
- Flat peer-to-peer mesh networking
- No hierarchical organization of autonomous agents
- No capability composition semantics

**CAP Innovation** (claimed in Provisional #1):
- Hierarchical cell structure (Platform → Squad → Platoon → Company)
- Four composition rule types:
  1. **Additive**: Union of member capabilities
  2. **Emergent**: New capabilities from specific combinations
  3. **Redundant**: Threshold-based capability requirements
  4. **Constraint-based**: Dependencies, exclusions, precedence
- O(n log n) message complexity through hierarchical aggregation
- CRDT-based composition state with conflict-free merging

**Key Distinction**: COD does not include hierarchical organization or capability composition semantics. CAP's composition engine is entirely novel.

#### Innovation 2: Graduated Human Authority Control (Not in COD)

**COD Approach**:
- Binary autonomy model (autonomous operation with human monitoring)
- Centralized human oversight where available
- No graduated authority levels

**CAP Innovation** (claimed in Provisional #2):
- Five-level authority taxonomy:
  1. FULL_AUTO - No approval required
  2. SUPERVISED - Human notified
  3. HUMAN_APPROVAL - Explicit approval before execution
  4. HUMAN_VETO - Execute unless human blocks
  5. MANUAL - Direct human control only
- Distributed approval/veto protocols using CRDTs
- Hierarchical authority constraint propagation
- Cryptographic audit trail for DoD 3000.09 compliance
- Timeout handling for human unavailability

**Key Distinction**: COD does not include graduated authority levels or distributed human-in-the-loop protocols. CAP's authority control system is entirely novel.

#### Innovation 3: CRDT-Based Hierarchical Coordination (Beyond COD)

**COD Approach**:
- Uses Ditto SDK for CRDT synchronization (peer-to-peer replication)
- Flat mesh topology
- General-purpose document synchronization

**CAP Innovation** (both provisionals):
- Hierarchical CRDT state (cells at multiple levels)
- Composition rules evaluate member CRDT state
- Authority constraints propagate through hierarchy
- Partition-tolerant reconciliation with hierarchical semantics

**Key Distinction**: While COD uses CRDTs for mesh networking, CAP extends CRDTs with hierarchical composition and authority control semantics not present in COD.

### Independent Development at Defense Unicorns LLC

Peat Protocol was developed independently at Defense Unicorns LLC using:

1. **Public Knowledge**:
   - Published CRDT literature (Shapiro et al., Automerge project)
   - Military doctrine for hierarchical command structures
   - DoD Directive 3000.09 on autonomous weapon systems
   - Academic research on human-robot teaming

2. **Original Innovation**:
   - Hierarchical capability composition algorithms (designed 2024-2025)
   - Graduated authority control taxonomy (designed 2024-2025)
   - CRDT-based composition engine (implemented 2024-2025)

3. **Clean-Room Implementation**:
   - Peat Protocol codebase written from scratch at Defense Unicorns
   - No COD source code incorporated
   - No Ditto proprietary algorithms used beyond publicly documented CRDT patterns

### Coordination with DIU

The inventors have proactively coordinated with the Defense Innovation Unit program manager for COD to:
- Ensure transparency about Peat Protocol development
- Confirm that CAP innovations extend beyond COD scope
- Maintain good faith with government sponsors

[Note: Update this section after DIU PM responds]

### Government Rights Disclosure

**COD Government Rights**:
- DIU/DoD holds Government Purpose Rights to COD technology under DFARS 252.227-7013/7014
- Government can use, modify, and share COD with contractors for government purposes
- Ditto Technologies retains commercial rights to COD

**Peat Protocol Government Rights**:
- No government funding for Peat Protocol development to date
- If future SBIR/STTR funding received, government rights will be disclosed per FAR 52.227-11
- Defense Unicorns LLC retains full commercial rights to CAP innovations

### Summary of Novelty

The present patent applications claim:

**Provisional #1 - Hierarchical Capability Composition**:
- ✅ Novel composition rule types (additive, emergent, redundant, constraint)
- ✅ O(n log n) hierarchical aggregation algorithm
- ✅ CRDT-based composition state management
- ❌ NOT claiming: General mesh networking (COD prior art)
- ❌ NOT claiming: Prioritization concepts (COD prior art, may claim in future)

**Provisional #2 - Graduated Human Authority Control**:
- ✅ Novel five-level authority taxonomy
- ✅ Distributed approval/veto protocols
- ✅ Hierarchical authority constraint propagation
- ✅ Cryptographic audit trail for DoD compliance
- ❌ NOT claiming: General human oversight (existing doctrine)
- ❌ NOT claiming: Resilient mesh networking (COD prior art)

### Conclusion

While the inventors gained valuable domain expertise through COD participation, **the core Peat Protocol innovations (hierarchical composition and graduated authority control) are independent and novel contributions** developed at Defense Unicorns LLC. This disclosure ensures transparency with USPTO and acknowledges prior art while clearly delineating CAP's novel contributions.

---

## Filing Instructions

**Where to insert**: After "Background" section, before "Summary of the Invention"

**For Provisional #1** (Capability Composition):
- Insert full "Related Work" section above
- Focus on composition vs COD's flat mesh

**For Provisional #2** (Human Authority Control):
- Insert full "Related Work" section above
- Focus on graduated authority vs COD's binary autonomy

**Section title**:
```markdown
## RELATED WORK AND PRIOR ART

[Insert content above]
```

---

## Legal Benefits of This Disclosure

1. **Good Faith with USPTO**: Shows awareness of prior art, strengthens patent
2. **Reduces Ditto Challenge Risk**: Acknowledges COD, focuses on differentiation
3. **Clear Inventorship**: Documents independent development at Defense Unicorns
4. **Government Transparency**: Discloses prior DIU work, maintains trust
5. **Patent Enforceability**: If challenged, can point to explicit prior art disclosure

---

**Status**: Ready to insert into provisional applications
**Required Before Filing**: Yes (shows good faith)
**Legal Review**: Recommended (have attorney review disclosure language)
