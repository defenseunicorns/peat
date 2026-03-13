# ADR-021: Document-Oriented Architecture and Update Semantics

**Status:** Proposed
**Date:** 2025-11-17
**Decision Makers:** Research Team
**Technical Story:** E12 validation revealed architectural violation - summary documents being created repeatedly instead of updated in-place

## Context and Problem Statement

During E12 comprehensive validation testing, we discovered a critical architectural violation in the hierarchical aggregation implementation. Analysis of the 48-node hierarchical test revealed:

**Observed Behavior:**
- `squad-1A-summary`: Created **21 times** in 90 seconds (21 SquadSummaryCreated events)
- `squad-1A-summary`: Received **3,987 times** across the mesh
- Total across all 8 squads: **20,348 DocumentReceived events** for summary documents

**Expected Behavior:**
- `squad-1A-summary`: Created **ONCE**, then **updated** 20+ times via deltas
- `squad-1A-summary`: Propagated efficiently through CRDT merge, not full replication
- Total: ~1,000 events (one creation + incremental updates)

**Impact:**
This represents a **20× bandwidth amplification** where we're recreating documents instead of updating them, defeating the entire purpose of CRDT-based differential synchronization.

### The Core Architectural Principle

Peat Protocol is designed around a **document-oriented architecture** where:

1. **Each entity is represented by exactly ONE living document**
   - Soldier/Platform/UAV → `sim_doc_{node_id}` (1 document)
   - Squad → `squad-{squad_id}-summary` (1 document)
   - Platoon → `platoon-{platoon_id}-summary` (1 document)

2. **Documents evolve through delta updates, never recreation**
   - State changes modify the existing document
   - CRDT operations ensure merge semantics
   - Updates propagate as minimal deltas, not full replacements

3. **Document volume is fixed per tier (O(1) cardinality)**
   - 8 squads = 8 summary documents (constant)
   - Content within documents may grow, but document count does not

4. **Bidirectional delta flows**
   - **Upward (tactical → strategic):** Capabilities, status, products, emergent behaviors
   - **Downward (strategic → tactical):** Tasking, configuration, intent, constraints

This principle was **implied but never explicitly stated** in ADR-001, ADR-009, and ADR-015. The absence of this explicit constraint led to an implementation that creates new documents instead of updating existing ones.

## Decision Drivers

### Evidence from E12 Validation

**Test Configuration:**
- 48 nodes organized into 8 squads (squad-1A through squad-4B)
- 90-second observation window
- Mode 4 hierarchical aggregation enabled

**Measured Violations:**

| Metric | Observed | Expected | Violation Factor |
|--------|----------|----------|------------------|
| SquadSummaryCreated (squad-1A) | 21 events | 1 event | 21× |
| DocumentReceived (squad-1A) | 3,987 events | ~150-200 updates | 20-26× |
| Total summary docs received | 20,348 | ~1,000 | 20× |
| Unique summary documents | 8 | 8 | ✓ Correct |

**Analysis:**
- The system correctly maintains **8 unique documents** (cardinality is correct)
- But it **recreates them repeatedly** (lifecycle is wrong)
- Each recreation triggers full P2P mesh propagation (~190 nodes × 21 recreations = 3,987 events)

### Architectural Impact

**Current (Broken) Flow:**
```
Every 4 seconds:
  Squad Member updates position
    ↓
  Squad Leader receives update
    ↓
  Squad Leader runs: upsert_squad_summary()
    ↓
  NEW DOCUMENT CREATED: squad-1A-summary (version N+1)
    ↓
  Full document propagates across entire P2P mesh
    ↓
  All 48 nodes receive 8 complete summary documents
    ↓
  Bandwidth: 20× expected, scales O(n²)
```

**Correct (Intended) Flow:**
```
Once at squad formation:
  Squad Leader creates: squad-1A-summary (version 1)

Every 4 seconds:
  Squad Member updates position
    ↓
  Squad Leader receives delta
    ↓
  Squad Leader runs: update_squad_summary_delta()
    ↓
  EXISTING DOCUMENT UPDATED: squad-1A-summary (delta applied)
    ↓
  Minimal delta propagates through hierarchy
    ↓
  Only affected nodes receive relevant changes
    ↓
  Bandwidth: As designed, scales O(log n)
```

### Business Impact

1. **Bandwidth Explosion**
   - 20× more data transmitted than necessary
   - Violates tactical bandwidth constraints (9.6Kbps - 1Mbps)
   - Makes 96-node and larger formations impractical

2. **Latency Degradation**
   - Mean latency: 6.1s at 24 nodes (should be <2s)
   - Will scale linearly to 12s+ at 48 nodes, 24s+ at 96 nodes
   - Tactical decision-making requires <5s total propagation

3. **False Architecture Validation**
   - Tests measure wrong architecture (full replication vs. aggregation)
   - Performance characteristics don't match design intent
   - Scaling projections will be incorrect

4. **CRDT Benefits Negated**
   - Ditto's differential sync designed for exactly this use case
   - But only works if we update documents, not recreate them
   - We're paying CRDT overhead without getting CRDT benefits

## Considered Options

### Option 1: Continue Current Implementation (REJECTED)

Accept document recreation pattern as "good enough."

**Pros:**
- No implementation changes needed
- Tests show sync works (all nodes receive data)

**Cons:**
- **Violates core design principle** of differential sync
- **20× bandwidth waste** makes tactical networks impractical
- **Does not scale** beyond ~50 nodes
- **Defeats purpose of CRDT architecture**
- **False validation** of hierarchical aggregation claims

**Decision:** Rejected - fundamentally breaks architecture

### Option 2: Fix Update Semantics (RECOMMENDED)

Enforce document-oriented architecture with proper update semantics.

**Implementation Requirements:**

1. **Document Lifecycle Management**
   ```rust
   // Create-once pattern
   pub async fn create_squad_summary(&self, squad_id: &str, initial_state: SquadSummary)
       -> Result<()> {
       let doc_id = format!("{}-summary", squad_id);

       // Only create if doesn't exist
       if self.get_squad_summary(squad_id).await?.is_none() {
           self.insert_document(doc_id, initial_state).await?;
       }
       Ok(())
   }

   // Update-many pattern
   pub async fn update_squad_summary(&self, squad_id: &str, delta: SquadDelta)
       -> Result<()> {
       let doc_id = format!("{}-summary", squad_id);

       // Apply delta to existing document
       self.update_document_fields(doc_id, delta.into_updates()).await?;
       Ok(())
   }
   ```

2. **Cardinality Enforcement**
   ```rust
   // Validate document count per tier
   pub async fn validate_tier_cardinality(&self, tier: HierarchyTier) -> Result<()> {
       let doc_count = self.count_documents_for_tier(tier).await?;
       let expected_count = tier.expected_entity_count();

       if doc_count != expected_count {
           return Err(ArchitectureViolation::CardinalityMismatch {
               tier,
               expected: expected_count,
               actual: doc_count,
           });
       }
       Ok(())
   }
   ```

3. **Delta-Based Updates**
   ```rust
   pub struct SquadDelta {
       pub squad_id: String,
       pub timestamp_us: u64,
       pub updates: Vec<FieldUpdate>,
   }

   pub enum FieldUpdate {
       SetMemberCount(usize),
       UpdateCoverage(f32),
       AddCapability(String),
       RemoveCapability(String),
       UpdateReadiness(String),
   }

   impl SquadDelta {
       pub fn into_updates(self) -> Vec<(String, serde_json::Value)> {
           // Convert to Ditto field updates
           self.updates.iter().map(|u| match u {
               FieldUpdate::SetMemberCount(n) =>
                   ("member_count".to_string(), json!(n)),
               FieldUpdate::UpdateCoverage(c) =>
                   ("coverage_area_km2".to_string(), json!(c)),
               // ... other field mappings
           }).collect()
       }
   }
   ```

4. **Metrics to Validate Fix**
   ```rust
   pub struct DocumentLifecycleMetrics {
       pub document_id: String,
       pub created_at: Timestamp,
       pub create_count: usize,      // Should be 1
       pub update_count: usize,      // Should be many
       pub delta_bytes_total: usize,
       pub full_doc_size: usize,
       pub compression_ratio: f32,   // Should be >10×
   }
   ```

**Pros:**
- **Restores architectural integrity**
- **20× bandwidth reduction** immediately
- **Enables scaling** to 96+ nodes as designed
- **Validates CRDT benefits** properly
- **Correct test measurements** going forward

**Cons:**
- **Implementation effort:** ~3-5 days
- **Re-baseline all tests** after fix
- **May reveal other issues** once correct architecture in place

**Decision:** Recommended - must fix before further scaling

### Option 3: Hybrid Approach

Fix squad-level only, defer platoon/company levels.

**Pros:**
- Partial fix for immediate validation
- Learn from squad-level before expanding

**Cons:**
- **Inconsistent architecture** across tiers
- **Technical debt** deferred
- **Still won't scale** beyond platoon level

**Decision:** Rejected - fix should be complete

## Decision Outcome

**Chosen option:** **Enforce document-oriented architecture with proper update semantics** (Option 2)

### Core Architectural Constraints

#### Constraint 1: Single Document Per Entity (Cardinality)

**Rule:** Each entity in the hierarchy is represented by exactly ONE document that lives for the entity's lifetime.

**Mapping:**
```
Entity                     Document ID Pattern              Cardinality
--------------------------------------------------------------------
Soldier/Platform/UAV   →   sim_doc_{node_id}               1 per node
Squad                  →   squad-{squad_id}-summary        1 per squad
Platoon                →   platoon-{platoon_id}-summary    1 per platoon
Company                →   company-{company_id}-summary    1 per company
Battalion              →   battalion-{battalion_id}-summary 1 per battalion
```

**Validation:**
- Document count for tier must equal entity count
- Creating duplicate document ID is an error
- Removing document requires entity destruction

#### Constraint 2: Update, Never Recreate (Lifecycle)

**Rule:** After creation, documents evolve through delta updates only. Recreation is prohibited.

**Lifecycle States:**
```
Created → Updating → Updating → ... → Updating → Destroyed
          (deltas)   (deltas)         (deltas)

NEVER:   Created → Destroyed → Created (recreation)
```

**Implementation:**
- `create_*_summary()` called ONCE during entity formation
- `update_*_summary()` called for all subsequent changes
- `destroy_*_summary()` called ONLY when entity disbanded

**Metrics:**
- `create_count == 1` (exactly one)
- `update_count >> 1` (many)
- `destroy_count == 0 or 1` (at most one)

#### Constraint 3: Delta-Based Evolution (Content)

**Rule:** Updates transmit only changed fields, not full document state.

**Delta Structure:**
```rust
pub struct DocumentDelta {
    pub doc_id: String,
    pub timestamp_us: u64,
    pub sequence: u64,              // Monotonic update counter
    pub field_updates: Vec<FieldUpdate>,
    pub delta_size_bytes: usize,    // Should be <<< full doc size
}
```

**Efficiency Target:**
- Delta size < 5% of full document size (average)
- Compression ratio > 20× for routine updates
- Only propagate changed fields

#### Constraint 4: Fixed Volume, Variable Content (Scaling)

**Rule:** Document count per tier is O(1) relative to entity count; content within documents may grow.

**Scaling Characteristics:**
```
Tier        Entity Count    Document Count    Bandwidth per Update
----------------------------------------------------------------------
Soldier     48 soldiers  →  48 docs        →  O(1) per soldier
Squad       8 squads     →  8 docs         →  O(k) per squad (k=6)
Platoon     2 platoons   →  2 docs         →  O(s) per platoon (s=4)
Company     1 company    →  1 doc          →  O(p) per company (p=2)

Total documents: 59 (not 48+8+2+1 duplicates per update)
Total bandwidth: O(n + n/k + n/k*s + n/k*s*p) ≈ O(n log n)
```

**Without this constraint:**
- Document count explodes with recreation
- Bandwidth becomes O(n²) or worse
- Tactical networks saturate

#### Constraint 5: Bidirectional Delta Flows (Direction)

**Rule:** Deltas flow in both directions through hierarchy with different content.

**Upward Flow (Tactical → Strategic):**
- Capabilities advertised
- Status updates
- Products created (ISR contacts, detections)
- Emergent behaviors discovered

**Downward Flow (Strategic → Tactical):**
- Tasking assigned
- Configuration updated
- Intent propagated
- Constraints set (ROE, no-strike zones)

**Horizontal Flow (Peer → Peer):**
- Deconfliction coordination
- Track handoff
- Resource sharing

**Content, Not Cardinality:**
- Same documents, different delta content
- Direction determines what fields update
- All flows use delta-based updates

### Implementation Plan

#### Phase 1: Fix Squad-Level Aggregation (Week 1)

**Tasks:**
1. **Refactor `upsert_squad_summary()` → `create/update` pattern**
   - Location: `peat-protocol/src/storage/ditto_store.rs:484`
   - Split into `create_squad_summary()` (called once)
   - And `update_squad_summary()` (called many times)

2. **Implement delta generation for squad updates**
   - Create `SquadDelta` struct with field-level updates
   - Convert from full `SquadSummary` to delta fields
   - Apply deltas via Ditto update operations (not upsert)

3. **Add lifecycle metrics**
   - Track `create_count`, `update_count` per document
   - Emit `DocumentCreated` vs `DocumentUpdated` events
   - Validate in tests: `create_count == 1` invariant

4. **Re-run 48-node validation**
   - Verify summary documents created once
   - Confirm updates use deltas
   - Measure bandwidth reduction

**Success Criteria:**
- SquadSummaryCreated: 8 events total (not 168)
- DocumentUpdated: ~160 events (20× per squad)
- Bandwidth reduction: >10× vs current implementation

#### Phase 2: Extend to Platoon/Company (Week 2)

**Tasks:**
1. Apply same pattern to `upsert_platoon_summary()`
2. Implement `CompanyDelta` for battalion-level
3. Add tier-level cardinality validation
4. Create integration tests for full hierarchy

**Success Criteria:**
- All tiers follow create-once/update-many pattern
- Document count matches entity count per tier
- No recreation events in validation tests

#### Phase 3: Validation and Documentation (Week 3)

**Tasks:**
1. Re-baseline all E12 tests with corrected architecture
2. Update ADR-015 with findings
3. Document delta patterns for future tiers
4. Create test suite for architectural constraints

**Success Criteria:**
- E12 comprehensive suite passes with correct semantics
- Bandwidth matches O(n log n) projection
- Latency <2s mean for 48-node test

### Success Metrics

**Before Fix (E12 Current Results):**
- SquadSummaryCreated: 21 per squad × 8 squads = 168 events
- DocumentReceived: 20,348 events
- Mean latency: 6.1s
- Scaling: Linear O(n)

**After Fix (Target):**
- SquadSummaryCreated: 1 per squad × 8 squads = 8 events (21× reduction)
- DocumentReceived: ~1,000 events (20× reduction)
- Mean latency: <2s (3× improvement)
- Scaling: O(log n) as designed

**Test Validation:**
```rust
#[test]
async fn test_document_lifecycle_invariant() {
    let metrics = collect_document_metrics("squad-1A-summary").await;

    // Core invariant: Created exactly once
    assert_eq!(metrics.create_count, 1,
        "Summary document must be created exactly once");

    // Updated many times
    assert!(metrics.update_count > 10,
        "Summary should be updated frequently");

    // Delta efficiency
    let avg_delta_size = metrics.delta_bytes_total / metrics.update_count;
    let compression = metrics.full_doc_size as f32 / avg_delta_size as f32;
    assert!(compression > 10.0,
        "Deltas should be >10× more efficient than full doc");
}
```

## Consequences

### Positive

1. **Architectural Integrity Restored**
   - System behavior matches design intent
   - CRDT benefits properly realized
   - Differential sync actually reduces bandwidth

2. **Bandwidth Reduction**
   - 20× reduction in summary document traffic
   - Makes tactical networks (9.6Kbps) practical
   - Enables scaling to 96+ nodes

3. **Latency Improvement**
   - Reduced data volume → faster propagation
   - Target: <2s mean latency (from 6.1s)
   - Meets tactical decision-making requirements

4. **Correct Validation**
   - Tests measure intended architecture
   - Performance projections accurate
   - Stakeholder confidence in results

5. **Scalability Unlocked**
   - O(log n) scaling as designed
   - 96-node company: ~3-4s propagation (not 24s+)
   - 192-node battalion: ~5-6s propagation (practical)

### Negative

1. **Implementation Effort**
   - 2-3 weeks to fix, test, re-baseline
   - Delays Phase 3B/3C experiments
   - Requires code review and refactoring

2. **Test Invalidation**
   - All E12 results must be re-baselined
   - Cannot compare pre/post-fix directly
   - Documentation updates required

3. **Potential Hidden Issues**
   - Fix may reveal other architectural gaps
   - Performance may be limited by other factors
   - Could discover Ditto limitations

### Risks and Mitigations

**Risk 1: Ditto doesn't support field-level updates efficiently**
- **Likelihood:** LOW - Ditto designed for this pattern
- **Impact:** MEDIUM - would need workarounds
- **Mitigation:** Spike Ditto update APIs early (Day 1)

**Risk 2: Delta generation adds latency**
- **Likelihood:** LOW - deltas are smaller, should be faster
- **Impact:** LOW - could optimize if needed
- **Mitigation:** Profile before/after, optimize if >10ms overhead

**Risk 3: Tests still show poor performance after fix**
- **Likelihood:** MEDIUM - may reveal other bottlenecks
- **Impact:** HIGH - would need further investigation
- **Mitigation:** Incremental validation, identify true bottlenecks

**Risk 4: Breaking changes to API**
- **Likelihood:** HIGH - signature changes required
- **Impact:** MEDIUM - affects calling code
- **Mitigation:** Update all call sites atomically, comprehensive tests

## Related Decisions

- **ADR-001 (Peat Protocol POC):** Defined CRDT-based architecture, implied but didn't mandate update semantics
- **ADR-009 (Bidirectional Hierarchical Flows):** Covered flow direction, not document lifecycle
- **ADR-011 (Ditto vs Automerge/Iroh):** Chose Ditto for CRDT support - must use properly
- **ADR-015 (Hierarchical Aggregation):** Identified full replication vs aggregation concern, didn't specify document semantics

## Future Work

1. **Extend to All Entity Types**
   - Apply pattern to UAVs, UGVs, command posts
   - Consistent lifecycle across all document types

2. **Automatic Delta Generation**
   - Derive `Delta` types from entity structs
   - Macro to generate `create/update/destroy` API

3. **Delta Compression**
   - CRDT deltas + content compression (zstd, etc.)
   - Target: 50× reduction for large updates

4. **Lifecycle Monitoring**
   - Runtime validation of cardinality invariants
   - Alert on document recreation (should never happen)
   - Dashboard for document lifecycle metrics

5. **Garbage Collection**
   - Remove destroyed entity documents after TTL
   - Archive historical states for analytics
   - Prevent storage exhaustion

## References

- E12 Validation Results: 48-node test showing 21× SquadSummaryCreated
- Ditto Documentation: Update operations and CRDT semantics
- CRDT Literature: Differential synchronization patterns
- ADR-001, ADR-009, ADR-015: Related architectural decisions

## Validation Checklist

Before accepting this ADR as "Implemented":

- [ ] `create_squad_summary()` and `update_squad_summary()` separated
- [ ] `create_platoon_summary()` and `update_platoon_summary()` separated
- [ ] Delta structs defined: `SquadDelta`, `PlatoonDelta`
- [ ] Lifecycle metrics emitted: `DocumentCreated`, `DocumentUpdated`
- [ ] Test added: `test_document_lifecycle_invariant()`
- [ ] 48-node test re-run: SquadSummaryCreated == 8 (not 168)
- [ ] Bandwidth validated: >10× reduction in summary traffic
- [ ] Latency validated: <2s mean (from 6.1s)
- [ ] ADR-015 updated with findings and fix
- [ ] E12 comprehensive results re-baselined

---

**Decision Record:**
- **Proposed:** 2025-11-17 (following E12 analysis)
- **Accepted:** TBD (pending review)
- **Implemented:** TBD (pending Phase 1-3 completion)

**Authors:** CAP Research Team
**Reviewers:** TBD
