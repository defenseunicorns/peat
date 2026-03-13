# Upward Aggregation Policy Flexibility

## Problem Statement

**Current State**: Upward capability aggregation has **hard-coded** behaviors:
- Confidence aggregation strategy (weighted average + redundancy bonus)
- Authority weighting (Commander=+10%, Supervisor=+5%)
- Readiness score weights (Communication=30%, Sensor=25%, etc.)
- Oversight requirements (Payload requires Commander authority)
- Mission readiness thresholds (0.8 for critical, 0.7 otherwise)

**Issue**: These hard-coded values don't adapt to different:
- Mission types (ISR vs kinetic vs humanitarian)
- Operational contexts (permissive vs contested environments)
- Capability criticality (sensor failure acceptable vs unacceptable)
- Human authority requirements (high oversight vs autonomous operations)

**Goal**: Apply the same **policy-based flexibility** we designed for downward flow to upward capability aggregation.

## Design Philosophy

> **"The same flexibility that enables integrators to configure downward commands should apply to upward capability aggregation"**

### Core Principle

Upward aggregation should be **configurable per capability type and mission context**, not globally hard-coded.

## Proposed Schema Extension

### 1. Aggregation Policy Message

```protobuf
// New file: peat-schema/proto/aggregation.proto

syntax = "proto3";

package cap.aggregation.v1;

// Aggregation policy configuration
//
// Defines how capabilities are aggregated from squad members to squad leader,
// and how readiness scores are calculated. Enables mission-specific tuning.
message AggregationPolicy {
  // Unique policy ID
  string policy_id = 1;

  // Human-readable name
  string name = 2;

  // Description of when to use this policy
  string description = 3;

  // How to aggregate confidence scores
  ConfidenceAggregationStrategy confidence_strategy = 10;

  // How to weight authority levels
  AuthorityWeighting authority_weighting = 11;

  // How to calculate readiness scores
  ReadinessScoring readiness_scoring = 12;

  // What oversight requirements to enforce
  OversightPolicy oversight_policy = 13;

  // Mission readiness thresholds
  ReadinessThresholds readiness_thresholds = 14;
}

// Confidence aggregation strategy
message ConfidenceAggregationStrategy {
  // Aggregation method
  enum Method {
    METHOD_UNSPECIFIED = 0;
    WEIGHTED_AVERAGE = 1;         // Average with redundancy bonus (current default)
    MINIMUM_CONFIDENCE = 2;       // Take lowest confidence (pessimistic)
    MAXIMUM_CONFIDENCE = 3;       // Take highest confidence (optimistic)
    MEDIAN_CONFIDENCE = 4;        // Take median (outlier-resistant)
    PROBABILISTIC_OR = 5;         // 1 - ∏(1 - cᵢ) (redundancy as backup)
  }

  Method method = 1;

  // Redundancy bonus configuration (for WEIGHTED_AVERAGE)
  message RedundancyBonus {
    // Bonus for 2 contributors
    float two_node_bonus = 1;     // Default: 0.05

    // Bonus for 3-4 contributors
    float three_to_four_bonus = 2; // Default: 0.10

    // Bonus for 5+ contributors
    float five_plus_bonus = 3;    // Default: 0.15

    // Maximum bonus cap
    float max_bonus = 4;          // Default: 0.20
  }

  RedundancyBonus redundancy_bonus = 2;
}

// Authority level weighting
message AuthorityWeighting {
  // Weight configuration
  enum WeightingMode {
    WEIGHTING_MODE_UNSPECIFIED = 0;
    ADDITIVE_BONUS = 1;           // Add bonus to confidence (current)
    MULTIPLICATIVE_FACTOR = 2;    // Multiply confidence by factor
    THRESHOLD_GATE = 3;           // Must meet minimum authority or block
    NO_WEIGHTING = 4;             // Ignore authority (fully autonomous)
  }

  WeightingMode mode = 1;

  // Authority level bonuses/factors
  message LevelWeights {
    float commander = 1;          // Default: 0.10 (additive) or 1.10 (multiplicative)
    float supervisor = 2;         // Default: 0.05 or 1.05
    float advisor = 3;            // Default: 0.03 or 1.03
    float observer = 4;           // Default: 0.00 or 1.00
    float unspecified = 5;        // Default: 0.00 or 1.00
  }

  LevelWeights weights = 2;
}

// Readiness scoring configuration
message ReadinessScoring {
  // Capability type weights
  message CapabilityWeights {
    float communication = 1;      // Default: 0.30
    float sensor = 2;             // Default: 0.25
    float compute = 3;            // Default: 0.20
    float payload = 4;            // Default: 0.15
    float mobility = 5;           // Default: 0.10
  }

  CapabilityWeights weights = 1;

  // How to handle missing capabilities
  enum MissingCapabilityHandling {
    MISSING_HANDLING_UNSPECIFIED = 0;
    ZERO_SCORE = 1;               // Missing capability scores 0 (strict)
    SKIP_MISSING = 2;             // Only score present capabilities (lenient)
    PENALTY_SCORE = 3;            // Missing capability scores negative (punitive)
  }

  MissingCapabilityHandling missing_handling = 2;

  // Penalty for missing critical capabilities
  float missing_penalty = 3;    // Default: -0.20
}

// Oversight policy
message OversightPolicy {
  // Oversight requirement mode
  enum Mode {
    MODE_UNSPECIFIED = 0;
    CAPABILITY_BASED = 1;         // Specific capability types require oversight
    AUTHORITY_THRESHOLD = 2;      // Require minimum authority level
    MISSION_CRITICALITY = 3;      // Based on mission type
    ALWAYS_REQUIRED = 4;          // All operations require human oversight
    NEVER_REQUIRED = 5;           // Fully autonomous operations allowed
  }

  Mode mode = 1;

  // Capabilities requiring oversight (for CAPABILITY_BASED)
  message CapabilityOversight {
    bool payload_requires_oversight = 1;          // Default: true
    bool communication_requires_oversight = 2;    // Default: true
    bool sensor_requires_oversight = 3;           // Default: false
    bool compute_requires_oversight = 4;          // Default: false
    bool mobility_requires_oversight = 5;         // Default: false
  }

  CapabilityOversight capability_oversight = 2;

  // Minimum authority level required (for AUTHORITY_THRESHOLD)
  string min_authority_level = 3;  // e.g., "Supervisor"
}

// Mission readiness thresholds
message ReadinessThresholds {
  // Threshold for mission-ready with oversight
  float with_oversight_threshold = 1;   // Default: 0.80

  // Threshold for mission-ready without oversight
  float autonomous_threshold = 2;       // Default: 0.70

  // Threshold for degraded operations
  float degraded_threshold = 3;         // Default: 0.50

  // Threshold for mission abort
  float abort_threshold = 4;            // Default: 0.30
}
```

### 2. Best Practice Policy Presets

```protobuf
// Preset aggregation policies for common scenarios

// Preset 1: High-Risk Kinetic Operations
message HighRiskKineticPolicy {
  policy_id: "high-risk-kinetic"
  name: "High-Risk Kinetic Operations"
  description: "Conservative aggregation for strike missions"

  confidence_strategy: {
    method: MINIMUM_CONFIDENCE  // Pessimistic: take lowest confidence
    redundancy_bonus: { max_bonus: 0.05 }  // Minimal redundancy bonus
  }

  authority_weighting: {
    mode: ADDITIVE_BONUS
    weights: {
      commander: 0.15      // High commander bonus
      supervisor: 0.08
      advisor: 0.03
    }
  }

  readiness_scoring: {
    weights: {
      payload: 0.40        // Weapons systems critical
      communication: 0.30  // Command & control critical
      sensor: 0.20
      compute: 0.05
      mobility: 0.05
    }
    missing_handling: PENALTY_SCORE
    missing_penalty: -0.30
  }

  oversight_policy: {
    mode: CAPABILITY_BASED
    capability_oversight: {
      payload_requires_oversight: true
      communication_requires_oversight: true
    }
  }

  readiness_thresholds: {
    with_oversight_threshold: 0.90      // Very high bar
    autonomous_threshold: 0.95          // Even higher for autonomous
    degraded_threshold: 0.70
    abort_threshold: 0.50
  }
}

// Preset 2: ISR / Reconnaissance
message ISRReconnaissancePolicy {
  policy_id: "isr-recon"
  name: "ISR & Reconnaissance"
  description: "Optimized for sensor-heavy missions"

  confidence_strategy: {
    method: PROBABILISTIC_OR  // Redundancy as backup
    redundancy_bonus: { max_bonus: 0.20 }  // Higher redundancy bonus
  }

  authority_weighting: {
    mode: MULTIPLICATIVE_FACTOR
    weights: {
      commander: 1.05      // Lower authority impact
      supervisor: 1.03
      advisor: 1.01
    }
  }

  readiness_scoring: {
    weights: {
      sensor: 0.40         // Sensors critical
      communication: 0.30  // Comms for data exfil
      compute: 0.20        // Processing for analysis
      mobility: 0.05
      payload: 0.05        // Minimal weapons
    }
    missing_handling: SKIP_MISSING  // Lenient on missing non-critical
  }

  oversight_policy: {
    mode: CAPABILITY_BASED
    capability_oversight: {
      payload_requires_oversight: false      // No weapons
      communication_requires_oversight: false
      sensor_requires_oversight: false       // Fully autonomous sensors ok
    }
  }

  readiness_thresholds: {
    with_oversight_threshold: 0.70      // Lower bar
    autonomous_threshold: 0.65          // Autonomous sensors acceptable
    degraded_threshold: 0.40
    abort_threshold: 0.20
  }
}

// Preset 3: Humanitarian Assistance / Disaster Relief
message HumanitarianPolicy {
  policy_id: "humanitarian"
  name: "Humanitarian Assistance"
  description: "Low-risk operations with human oversight"

  confidence_strategy: {
    method: WEIGHTED_AVERAGE  // Standard aggregation
    redundancy_bonus: { max_bonus: 0.15 }
  }

  authority_weighting: {
    mode: THRESHOLD_GATE
    min_authority_level: "Observer"  // Minimal authority required
  }

  readiness_scoring: {
    weights: {
      communication: 0.35  // Coordination critical
      mobility: 0.25       // Movement for aid delivery
      sensor: 0.20         // Situational awareness
      compute: 0.15        // Planning
      payload: 0.05        // Minimal payload
    }
    missing_handling: SKIP_MISSING
  }

  oversight_policy: {
    mode: ALWAYS_REQUIRED   // Human oversight for safety
  }

  readiness_thresholds: {
    with_oversight_threshold: 0.60      // Lower technical bar
    autonomous_threshold: 0.90          // High bar for autonomous (rarely used)
    degraded_threshold: 0.40
    abort_threshold: 0.20
  }
}

// Preset 4: Fully Autonomous Swarm
message AutonomousSwarmPolicy {
  policy_id: "autonomous-swarm"
  name: "Fully Autonomous Swarm"
  description: "No human oversight required"

  confidence_strategy: {
    method: PROBABILISTIC_OR  // High redundancy value
    redundancy_bonus: { max_bonus: 0.25 }
  }

  authority_weighting: {
    mode: NO_WEIGHTING  // Ignore human authority
  }

  readiness_scoring: {
    weights: {
      communication: 0.30
      sensor: 0.25
      compute: 0.20
      mobility: 0.15
      payload: 0.10
    }
    missing_handling: SKIP_MISSING
  }

  oversight_policy: {
    mode: NEVER_REQUIRED  // Fully autonomous
  }

  readiness_thresholds: {
    with_oversight_threshold: 0.75
    autonomous_threshold: 0.70      // Lower bar for autonomous
    degraded_threshold: 0.50
    abort_threshold: 0.30
  }
}
```

## Integration with SquadSummary

Update SquadSummary to include the aggregation policy used:

```protobuf
message SquadSummary {
  // ... existing fields ...

  // Aggregation policy used to create this summary
  string aggregation_policy_id = 20;

  // Metadata about aggregation
  aggregation.v1.AggregationMetadata aggregation_metadata = 21;
}

message AggregationMetadata {
  // Confidence aggregation method used
  string confidence_method = 1;

  // Authority weighting applied
  bool authority_weighted = 2;

  // Oversight requirements checked
  bool oversight_checked = 3;

  // Number of capabilities aggregated
  uint32 capabilities_aggregated = 4;

  // Number of capabilities excluded (non-operational nodes)
  uint32 capabilities_excluded = 5;
}
```

## Implementation Approach

### Phase 1: Schema Definition (1 week)
1. Create `aggregation.proto` with policy messages
2. Add preset policies to schema
3. Generate Rust bindings
4. Add to peat-schema crate

### Phase 2: Policy-Driven Aggregation (2 weeks)
1. Refactor `CapabilityAggregator` to accept `AggregationPolicy`
2. Implement strategy pattern for confidence aggregation methods
3. Implement configurable authority weighting
4. Implement configurable readiness scoring
5. Implement configurable oversight requirements

### Phase 3: Integration (1 week)
1. Update `StateAggregator` to use policy-driven aggregation
2. Add policy selection to aggregation calls
3. Store policy ID in SquadSummary
4. Add aggregation metadata tracking

### Phase 4: Best Practice Presets (1 week)
1. Implement preset factory functions
2. Document when to use each preset
3. Create helper APIs for common scenarios
4. Add integration examples

**Total**: 5 weeks

## Example Usage

### Before (Hard-Coded)
```rust
// No control over aggregation behavior
let capabilities = CapabilityAggregator::aggregate_capabilities(&members)?;
let readiness = CapabilityAggregator::calculate_readiness_score(&capabilities);
```

### After (Policy-Driven)
```rust
use cap_protocol::aggregation::{AggregationPolicy, PolicyPresets};

// High-risk kinetic mission
let policy = PolicyPresets::high_risk_kinetic();
let capabilities = CapabilityAggregator::aggregate_capabilities(&members, &policy)?;
let readiness = CapabilityAggregator::calculate_readiness_score(&capabilities, &policy);

// ISR mission
let policy = PolicyPresets::isr_reconnaissance();
let capabilities = CapabilityAggregator::aggregate_capabilities(&members, &policy)?;
let readiness = CapabilityAggregator::calculate_readiness_score(&capabilities, &policy);

// Custom policy for specific requirements
let custom_policy = AggregationPolicy::builder()
    .confidence_strategy(ConfidenceStrategy::MinimumConfidence)
    .authority_weighting(AuthorityWeighting::threshold_gate("Commander"))
    .with_readiness_threshold(0.95)
    .build();
```

## Symmetry with Downward Flow

This creates **perfect symmetry** between upward and downward flows:

| Aspect | Downward Flow | Upward Flow |
|--------|---------------|-------------|
| **Configurability** | Command policies | Aggregation policies |
| **Schema Definition** | `command.proto` | `aggregation.proto` |
| **Policy Dimensions** | 4 (partition, conflict, ack, leader) | 5 (confidence, authority, readiness, oversight, thresholds) |
| **Presets** | Mission-critical, tactical, routine, coordinated | High-risk kinetic, ISR, humanitarian, autonomous |
| **Integration** | CommandBuilder | AggregationPolicyBuilder |
| **Flexibility** | Per-command | Per-capability-type, per-mission |

## Benefits

**For Integrators**:
- ✅ Tune aggregation for mission type (ISR vs kinetic vs humanitarian)
- ✅ Adjust authority impact based on operational context
- ✅ Configure readiness thresholds per mission criticality
- ✅ Override oversight requirements for specific scenarios

**For Protocol Developers**:
- ✅ No hard-coded aggregation logic
- ✅ Extensible - add new aggregation methods without breaking changes
- ✅ Testable - policy-driven logic is isolated
- ✅ Maintainable - configuration separate from implementation

**For the Mission**:
- ✅ Same protocol adapts to diverse operational contexts
- ✅ Clear audit trail of aggregation policies used
- ✅ Supports both high-oversight and autonomous operations
- ✅ Mission-specific tuning without code changes

## Conclusion

**Current State**: Upward aggregation has hard-coded behaviors (redundancy bonuses, authority weights, readiness thresholds)

**Proposed State**: Policy-driven aggregation with the same flexibility as downward commands

**Philosophy**: **"Peat Protocol provides mechanism, integrators provide policy"** - this principle should apply **both ways** (upward AND downward)

---

**Status**: Design Proposal
**Date**: 2025-11-08
**Next Steps**:
1. Review with team
2. Validate presets against operational requirements
3. Implement after bidirectional command flow is complete
