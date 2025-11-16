# ADR-015: Experimental Validation Framework and Hierarchical Aggregation Requirements

**Status:** Accepted
**Date:** 2025-11-08
**Decision Makers:** Research Team
**Technical Story:** E8 Phase 3A validation revealed critical architectural requirements for hierarchical data aggregation vs full replication

## Context and Problem Statement

During E8 experimental validation, we have developed a comprehensive test laboratory infrastructure using ContainerLab to validate HIVE protocol performance at scale. Initial testing (2-node POC, 12-node squad) showed promising results, and we expanded to 24-node platoon hierarchies (Phase 3A) as documented in EXPERIMENT-PORTFOLIO.md.

However, **Phase 3A validation (2025-11-08) revealed a critical architectural concern:**

**Observed Performance:**
- 24 nodes: 54-second convergence, 6.1-second mean latency
- All 24 nodes achieved "POC SUCCESS" (100% sync)
- Linear scaling from 12-node baseline (~26s → ~54s for 2× nodes)

**The Problem:** This performance suggests we may be testing **full n-squared replication** rather than **hierarchical aggregation with capability filtering** - which defeats the entire purpose of CAP's design.

### Core Architectural Question

**What should "convergence" mean in a hierarchical military formation?**

**Option A: Full Replication (what we may be testing):**
```
Platoon Leader receives:
  ├─ All 24 raw node states (n-squared data)
  ├─ Every soldier's individual position, health, ammo, etc.
  └─ Result: Data explosion as we scale (impractical at 96+ nodes)
```

**Option B: Hierarchical Aggregation (CAP's intended design):**
```
Platoon Leader receives:
  ├─ Squad Alpha Summary (1 aggregated state from 8 nodes)
  ├─ Squad Bravo Summary (1 aggregated state from 8 nodes)
  └─ Squad Charlie Summary (1 aggregated state from 8 nodes)
  Result: O(log n) data growth (practical at 96+ nodes)
```

**If we're testing Option A, then:**
- 6-second latency at 24 nodes is alarming
- Scaling to 48 nodes: ~12s latency, ~120s convergence (unacceptable)
- Scaling to 96 nodes: ~24s latency, ~240s convergence (tactical failure)
- The entire premise of CAP's capability-based filtering is invalidated

**If we should be testing Option B, then:**
- We need to verify hierarchical aggregation is implemented
- We need different convergence criteria for different hierarchy levels
- We need to validate that filtering actually reduces data replication

## Decision Drivers

### Immediate Concerns (Blockers for Phase 3B/3C)

1. **Unclear what we're actually testing**
   - Is CAP_FILTER_ENABLED working in platoon tests?
   - What queries are nodes actually running (Query::All vs capability-filtered)?
   - Does Ditto support hierarchical aggregation patterns?

2. **Performance red flag**
   - 6-second mean latency for 24 nodes is concerning
   - Linear scaling 12→24 nodes suggests no hierarchical benefit
   - This will not scale to 96+ nodes (company-level operations)

3. **Wrong success criteria**
   - "POC SUCCESS" likely means "received data from all nodes"
   - Should mean "received relevant data for my role/level"
   - Different for squad member vs squad leader vs platoon leader

4. **Risk of wasted effort**
   - Cannot validate scaling to 48/96 nodes without correct architecture
   - Phase 3B/3C tests would measure wrong thing
   - Need to fix foundation before building higher

### Strategic Requirements (CAP Architecture)

From ADR-001 and ADR-009, CAP is designed for:

1. **Capability-Based Filtering**
   - Nodes subscribe only to data matching their capabilities
   - Reduces bandwidth by avoiding irrelevant replication
   - Critical for tactical bandwidth constraints (9.6Kbps - 1Mbps)

2. **Hierarchical Flows**
   - Bottom-up: Detailed data aggregates as it flows up hierarchy
   - Top-down: Commands/intent disseminate down hierarchy
   - Each level sees appropriate granularity for decision-making

3. **Role-Specific Convergence**
   - Squad members: Sync within squad (intra-squad convergence)
   - Squad leaders: Aggregate squad state, sync with platoon leader
   - Platoon leader: Maintain high-level view of 3 squad summaries
   - NOT: All 24 nodes have identical full state

## Considered Options

### Option 1: Continue Scaling Experiments (REJECTED)

Proceed with Phase 3B (48 nodes) and Phase 3C (96 nodes) testing.

**Pros:**
- Gather more data points
- Complete experiment portfolio as planned
- Demonstrate scaling to larger formations

**Cons:**
- **May be measuring wrong architecture** (full replication vs aggregation)
- **Wasted effort** if we're not testing what we think we're testing
- **Misleading results** could inform incorrect design decisions
- **Performance degradation** will continue linearly (or worse)

**Decision:** Rejected - cannot build on uncertain foundation

### Option 2: Pause and Validate Architecture (RECOMMENDED)

Halt Phase 3A/3B/3C scaling experiments. Conduct architectural validation to understand what we're actually testing.

**Investigation Steps:**

1. **Analyze Current Implementation**
   - Review Ditto integration code to understand query patterns
   - Check if CAP_FILTER_ENABLED actually changes behavior
   - Verify what data platoon leader receives (raw 24 states vs 3 summaries)
   - Examine logs from 24-node validation run

2. **Define Hierarchical Convergence**
   - Document what each hierarchy level should see
   - Create separate success criteria:
     - Squad-level convergence (8 nodes share within squad)
     - Platoon-level convergence (platoon leader has 3 squad summaries)
   - Update "POC SUCCESS" validation logic

3. **Verify Capability Filtering**
   - Confirm filtering reduces data replication
   - Measure bandwidth with filtering ON vs OFF
   - Validate that nodes don't receive irrelevant data

4. **Design or Fix Aggregation**
   - If hierarchical aggregation doesn't exist: Document as gap
   - If it exists but isn't enabled: Fix configuration
   - If it exists and is enabled: Debug performance issue

5. **Re-baseline Before Resuming**
   - Re-run 24-node validation with correct architecture
   - Verify convergence time improves (should be similar to 12-node)
   - Document actual scaling characteristics

**Pros:**
- **Ensures we're testing the right thing** before scaling further
- **Prevents wasted effort** on incorrect architecture
- **Identifies true performance bottlenecks** vs measurement errors
- **Builds correct foundation** for future scaling

**Cons:**
- **Delays Phase 3B/3C** experiments (48/96 nodes)
- **Requires architectural investigation** (~1-2 weeks)
- **May reveal implementation gaps** requiring significant work

**Decision:** Recommended - better to get it right than to waste effort scaling the wrong architecture

### Option 3: Dual-Track Approach

Continue some experiments while investigating architecture in parallel.

**Pros:**
- Maintains experimental momentum
- Provides more data for comparison

**Cons:**
- **Resource split** between investigation and experimentation
- **Confusion** if results are contradictory
- **Technical debt** if we build on wrong assumptions

**Decision:** Rejected - focus resources on validation first

## Decision Outcome

**Chosen option:** **Pause scaling experiments (Phase 3B/3C) and validate hierarchical aggregation architecture.**

### Immediate Actions (Next 2 Weeks)

**Week 1: Investigation and Analysis**

1. **Analyze 24-Node Validation Logs** (Day 1-2)
   ```bash
   # Review what platoon leader actually received
   grep -r "document" hive-sim/validation-platoon-24node-*/platoon-leader.log
   grep -r "Query::" hive-sim/validation-platoon-24node-*/platoon-leader.log
   grep -r "capability" hive-sim/validation-platoon-24node-*/
   ```

2. **Review Ditto Integration Code** (Day 2-4)
   - Examine `hive-sim-node` implementation
   - Understand query patterns (Query::All vs capability-filtered)
   - Verify CAP_FILTER_ENABLED actually changes behavior
   - Document current data flow patterns

3. **Define Hierarchical Convergence Requirements** (Day 4-5)
   - Document what each role should see:
     - Squad member: Squad-local data + relevant platoon directives
     - Squad leader: Aggregated squad state + squad member data
     - Platoon leader: 3 squad summaries (NOT 24 raw states)
   - Create formal success criteria for each level

**Week 2: Design and Decision**

4. **Assess Architectural Gap** (Day 6-7)
   - Is hierarchical aggregation implemented?
   - If not: Estimate effort to implement
   - If yes but not working: Identify bugs/config issues
   - Document findings in this ADR (update status)

5. **Create Remediation Plan** (Day 8-10)
   - Option A: Fix configuration/enablement (if aggregation exists)
   - Option B: Implement hierarchical aggregation (if missing)
   - Option C: Defer and document limitation (if too complex)
   - Estimate timeline and resources

6. **Go/No-Go Decision** (Day 10)
   - Can we fix quickly? → Proceed with fix, re-baseline, resume experiments
   - Major implementation needed? → Document as ADR-016, plan implementation
   - Unclear/risky? → Defer scaling experiments, focus on other priorities

### Lab Infrastructure Status

**Completed and Validated:**
- ✅ ContainerLab-based test harness
- ✅ 2-node POC validation (bidirectional sync)
- ✅ 12-node squad validation (3 topology modes)
- ✅ 24-node platoon topologies (4 modes: client-server, hub-spoke, dynamic-mesh, hybrid)
- ✅ Validation smoke test infrastructure
- ✅ Metrics collection and analysis framework
- ✅ Experiment portfolio roadmap (Phase 3A-3D)

**Blocked Pending Architectural Validation:**
- ⏸️ Phase 3A full bandwidth testing (24 nodes × 4 bandwidths)
- ⏸️ Phase 3B planning (48-node half-company)
- ⏸️ Phase 3C planning (96-node company)
- ⏸️ Phase 3D planning (192-node battalion)

**Lab Infrastructure Remains Useful:**
- Can test architectural fixes immediately (24-node validation in ~2 minutes)
- Topologies are correct regardless of aggregation implementation
- Metrics framework will measure improved performance
- Foundation is solid - just need to validate what we're measuring

### Success Criteria for Resuming Experiments

Before resuming Phase 3A/3B/3C testing, we must verify:

1. ✅ **Hierarchical aggregation is implemented and working**
   - Platoon leader receives 3 squad summaries (not 24 raw states)
   - Squad leaders aggregate their squad's data
   - Capability filtering reduces data replication

2. ✅ **Convergence time improves with hierarchy**
   - 24-node platoon convergence similar to 12-node squad (~26-30s)
   - NOT linear scaling (2× nodes → 2× convergence)
   - Demonstrates hierarchical benefit

3. ✅ **Latency remains acceptable**
   - Mean latency < 2s (not 6s) for 24 nodes
   - P90 latency < 4s (not 12s)
   - Indicates efficient aggregation, not full replication

4. ✅ **Bandwidth reduction validated**
   - CAP Differential shows >60% reduction vs Traditional IoT
   - Filtering measurably reduces bytes transmitted
   - Scales sub-linearly with node count

5. ✅ **Correct success criteria defined**
   - Different validation for squad vs platoon vs company levels
   - "POC SUCCESS" means role-appropriate convergence
   - Tests measure architectural intent, not just "all nodes synced"

### Documentation Updates Required

1. **Update ADR-008** (Network Simulation Layer)
   - Add section on hierarchical aggregation validation
   - Document Phase 3A findings and pause decision
   - Reference this ADR for architectural concerns

2. **Update EXPERIMENT-PORTFOLIO.md**
   - Mark Phase 3A as "Under Architectural Review"
   - Add prerequisite: Hierarchical aggregation validation
   - Update go/no-go gates to include aggregation verification

3. **Create Follow-up ADR** (if major work needed)
   - ADR-016: Hierarchical Data Aggregation Implementation
   - Define aggregation patterns (sum, average, max, custom)
   - Document Ditto integration approach

## Consequences

### Positive

1. **Architectural Clarity**
   - Forces us to understand what we're actually testing
   - Validates that CAP's core design (filtering, aggregation) is implemented
   - Ensures experiments measure the right architecture

2. **Prevents Wasted Effort**
   - Avoids scaling to 48/96 nodes with wrong architecture
   - Identifies performance issues at 24 nodes (manageable) vs 96 nodes (crisis)
   - Focuses resources on fixing foundation first

3. **Stronger Validation**
   - Correct success criteria lead to meaningful results
   - Performance improvements will be real, not artifacts
   - Stakeholder confidence in experimental findings

4. **Better Design**
   - May identify missing capabilities (aggregation patterns)
   - Informs future Ditto integration decisions
   - Validates ADR-001/ADR-009 architectural assumptions

### Negative

1. **Delays Scaling Experiments**
   - Phase 3B (48 nodes) delayed 2-4 weeks
   - Phase 3C (96 nodes) delayed further
   - May impact project timeline if scaling validation was critical path

2. **Uncertainty**
   - Don't know what we'll find during investigation
   - May discover significant implementation gaps
   - Could require major architectural work (weeks/months)

3. **Resource Impact**
   - Investigation requires focused effort (not parallel with other work)
   - May need to implement missing aggregation patterns
   - Testing/validation overhead after fixes

### Risks and Mitigation

**Risk 1: Investigation reveals no hierarchical aggregation exists**
- **Impact:** HIGH - major implementation required
- **Likelihood:** MEDIUM - Ditto may not support this pattern natively
- **Mitigation:**
  - Document as known limitation
  - Implement application-level aggregation outside Ditto
  - Consider alternative sync engines (Automerge, Iroh) if Ditto cannot support

**Risk 2: Aggregation exists but has fundamental performance issues**
- **Impact:** HIGH - cannot scale as intended
- **Likelihood:** LOW - but possible with CRDT overhead
- **Mitigation:**
  - Profile and optimize aggregation code
  - Consider lazy aggregation (on-demand vs real-time)
  - Document acceptable performance tradeoffs

**Risk 3: Architecture is correct, tests are wrong**
- **Impact:** MEDIUM - need to fix tests, not architecture
- **Likelihood:** MEDIUM - validation logic may be too simplistic
- **Mitigation:**
  - Update "POC SUCCESS" criteria to be role-aware
  - Create separate convergence metrics per hierarchy level
  - Re-run validation with correct criteria

**Risk 4: Delays cascade to other project milestones**
- **Impact:** MEDIUM - depends on project timeline
- **Likelihood:** MEDIUM - depends on findings
- **Mitigation:**
  - Timebox investigation to 2 weeks
  - Make go/no-go decision at end of Week 2
  - Communicate findings to stakeholders early

## Related Decisions

- **ADR-001:** HIVE Protocol POC - Established capability-based filtering as core design
- **ADR-008:** Network Simulation Layer - Defined experimental validation approach
- **ADR-009:** Bidirectional Hierarchical Flows - Documented hierarchical data patterns
- **EXPERIMENT-PORTFOLIO.md:** Phase 3A-3D scaling roadmap

## Future Work

### If Hierarchical Aggregation Exists and Works

1. Re-run 24-node validation with correct criteria
2. Resume Phase 3B (48 nodes) with confidence
3. Proceed through Phase 3C/3D as planned
4. Document aggregation patterns for future reference

### If Hierarchical Aggregation Needs Implementation

1. Create ADR-016: Hierarchical Data Aggregation Patterns
2. Design aggregation API (sum, average, max, custom reducers)
3. Implement in hive-protocol or hive-transport layer
4. Integrate with Ditto sync (or alternative if needed)
5. Re-baseline all experiments with aggregation enabled

### If Architectural Issues Are Fundamental

1. Document limitations clearly
2. Consider alternative sync engines (Automerge, Iroh)
3. Re-evaluate ADR-011 (Ditto vs Automerge/Iroh)
4. May need to pivot architecture significantly

## Notes and Open Questions

**Open Questions (to be answered during investigation):**

1. **Does Ditto support aggregation queries?**
   - Can we write aggregation logic in queries?
   - Or must we implement in application layer?

2. **What is the current data flow?**
   - Does platoon leader receive all 24 raw documents?
   - Or does it receive aggregated summaries?

3. **How does CAP_FILTER_ENABLED work?**
   - Does it change query patterns?
   - Does it reduce data replication?
   - Is it actually enabled in platoon tests?

4. **What should "convergence" measure?**
   - Time for all raw data to replicate everywhere? (wrong)
   - Time for each hierarchy level to have appropriate view? (right)
   - How do we instrument this correctly?

5. **Is 6-second latency acceptable?**
   - For tactical operations: probably not
   - For some use cases: maybe
   - What are the actual requirements?

**Investigation Artifacts (to be created):**

- [ ] Analysis report: "What 24-Node Validation Actually Tested"
- [ ] Code review: Ditto integration query patterns
- [ ] Design doc: Hierarchical convergence requirements
- [ ] Gap analysis: Current vs required architecture
- [ ] Remediation plan: Steps to fix (if feasible)
- [ ] Updated ADR-008: Phase 3A findings section

---

**Decision Record:**
- **Proposed:** 2025-11-08 (following Phase 3A validation)
- **Accepted:** 2025-11-08 (immediate pause of scaling experiments)
- **Supersedes:** None
- **Superseded by:** TBD (ADR-016 if major implementation needed)

**Authors:** CAP Research Team
**Reviewers:** TBD (pending investigation findings)
