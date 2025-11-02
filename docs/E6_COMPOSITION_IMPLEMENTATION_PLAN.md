# E6: Capability Composition Engine - Implementation Plan

## Overview

Epic 6 implements capability composition rules that detect emergent team capabilities and constraints from individual node capabilities. This enables hierarchical reasoning about what teams can accomplish together.

## Phases

### E6.1: Composition Rule Framework ✓ Next

**Goal**: Core trait and registry infrastructure for composition rules

**Deliverables**:
- `CompositionRule` trait defining rule interface
- `CompositionEngine` with rule registry
- Rule execution pipeline
- Basic unit tests

**Files**:
- `cap-protocol/src/composition/rules.rs` - trait definition
- `cap-protocol/src/composition/engine.rs` - engine and registry
- `cap-protocol/src/composition/mod.rs` - public API

**Acceptance Criteria**:
- Rules can be registered and executed
- Supports multiple rules per capability type
- Returns composed capabilities with confidence scores

---

### E6.2: Additive Composition Rules

**Goal**: Implement composition patterns where capabilities sum

**Examples**:
- Coverage area: sum of sensor footprints
- Lift capacity: sum of payload capacities
- Communication bandwidth: sum of link capacities

**Deliverables**:
- `AdditiveRule` implementation
- Metadata aggregation (union of coverage zones)
- Confidence score combination (weighted average)
- Property tests (associative, commutative)

---

### E6.3: Emergent Composition Rules

**Goal**: Detect emergent capabilities from capability chains

**Examples**:
- ISR Chain: Sensor + Compute + Communication → ISR capability
- 3D Mapping: Lidar + IMU + GPS → Mapping capability
- Strike Chain: Sensor + Weapon + Datalink → Strike capability

**Deliverables**:
- `EmergentRule` with pattern matching
- Chain detection algorithms
- Confidence propagation through chains
- Integration tests with real scenarios

---

### E6.4: Redundant Composition Rules

**Goal**: Calculate reliability and availability from redundancy

**Examples**:
- Detection reliability: 1 - ∏(1 - p_i) for n sensors
- Continuous coverage: temporal overlap analysis
- Communication redundancy: multiple paths

**Deliverables**:
- `RedundantRule` implementation
- Reliability calculations
- Temporal overlap detection
- Coverage continuity analysis

---

### E6.5: Constraint-Based Composition

**Goal**: Compute team constraints from individual limits

**Examples**:
- Team speed: min(node speeds)
- Communication range: min(node ranges)
- Endurance: min(node fuel)

**Deliverables**:
- `ConstraintRule` implementation
- Min/max constraint propagation
- Integration with cell formation
- E2E tests with multi-node scenarios

---

## Testing Strategy

**Unit Tests** (70% effort):
- Each rule type with edge cases
- Property tests for mathematical laws
- Confidence score validation

**Integration Tests** (20% effort):
- Multi-rule composition scenarios
- Rule interaction validation
- Performance benchmarks

**E2E Tests** (10% effort):
- Full cell formation with composition
- Emergent capability detection in real scenarios
- Observer-based validation with Ditto sync

## Success Metrics

- All composition patterns implemented
- 90%+ test coverage maintained
- Property tests pass for associative/commutative rules
- ISR chain detection working in E2E tests
- Documentation updated with examples

## Timeline Estimate

- E6.1: 2-3 hours (framework)
- E6.2: 2 hours (additive rules)
- E6.3: 3-4 hours (emergent detection)
- E6.4: 2 hours (redundancy)
- E6.5: 2 hours (constraints)

**Total**: 11-13 hours for complete E6 implementation

---

**Status**: Starting E6.1 implementation
**Last Updated**: 2025-11-02
