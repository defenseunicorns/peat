# ADR-014: Distributed Coordination Primitives for Tactical Operations

**Status**: Draft  
**Date**: 2025-11-07  
**Authors**: Kit  
**Supersedes**: Extends ADR-008 (Bidirectional Flows), ADR-009 (Hierarchical Flows)  
**Related**: ADR-007 (Automerge Sync), ADR-010 (Transport Layer), ADR-013 (Software Operations)

## Context

ADR-007 through ADR-009 established CRDT-based eventual consistency for shared battlefield state. However, tactical operations require coordination primitives beyond simple state synchronization:

- **Track Engagement Deconfliction**: Two platforms must not simultaneously engage the same target
- **Fire Control Coordination**: Multiple weapons systems must coordinate targeting sequences
- **Formation Control**: Squad maneuvers require synchronized state transitions
- **Resource Allocation**: Shared resources (sensor dwell time, ammunition types) need distributed management
- **Safety Zones**: Dynamic no-strike zones must be enforced across platforms

Traditional distributed coordination algorithms (Paxos, Raft, 2PC) assume:
- Reliable network connectivity
- Low latency communication (<100ms)
- Majority node availability
- Stable leader election

**None of these assumptions hold in DIL tactical networks.** We need coordination primitives that:
- Function during network partitions
- Tolerate high latency (seconds to minutes)
- Maintain safety without majority consensus
- Degrade gracefully to autonomous operation
- Resolve conflicts deterministically when network reconverges

## Problem Statement

Design a coordination layer that sits **on top of** CRDT-based state synchronization to enable distributed tactical operations with strong safety guarantees, while operating under DIL network constraints.

### Key Requirements

1. **Deconfliction**: Prevent duplicate engagement of targets
2. **Conflict Resolution**: Deterministic resolution when coordination fails
3. **Time-Bounded Operations**: Coordination with temporal constraints
4. **Hierarchical Override**: Higher echelons can preempt local coordination
5. **Partition Tolerance**: Maintain safety during network splits
6. **Audit Trail**: Complete history of coordination decisions

### Non-Goals

- Strong consistency (impossible in DIL environments)
- Guaranteed serialization of all operations
- Sub-second coordination latency
- Byzantine fault tolerance (assume non-adversarial failures)

## Architecture

### Coordination Layer Model

```rust
/// Coordination layer sits between tactical logic and CRDT sync
pub struct CoordinationLayer {
    /// Underlying CRDT state sync
    sync_engine: AutomergeSync,
    
    /// Distributed claims tracking
    claims: ClaimRegistry,
    
    /// Conflict resolution rules
    resolver: ConflictResolver,
    
    /// Spatial deconfliction
    exclusion_zones: ExclusionZoneManager,
    
    /// Time synchronization (loosely synchronized clocks)
    time_sync: LooseTimeSync,
}

/// Distributed claim for coordinated operations
pub struct Claim {
    /// Unique claim identifier
    pub id: ClaimId,
    
    /// Resource being claimed (track, zone, sensor)
    pub resource: ResourceId,
    
    /// Platform making claim
    pub claimant: PlatformId,
    
    /// Claim type determines conflict resolution
    pub claim_type: ClaimType,
    
    /// Priority for conflict resolution
    pub priority: Priority,
    
    /// When claim expires (time-bounded)
    pub expires_at: Timestamp,
    
    /// Mission context for resolution
    pub context: MissionContext,
}

pub enum ClaimType {
    /// Exclusive claim (only one allowed)
    Exclusive { 
        timeout: Duration,
        preemptible: bool 
    },
    
    /// Shared claim (multiple allowed with coordination)
    Shared { 
        max_participants: u8,
        coordination_window: Duration 
    },
    
    /// Advisory claim (informational, no blocking)
    Advisory { 
        notification_only: bool 
    },
}
```

### Track Engagement Coordination

```rust
/// Track engagement deconfliction protocol
pub struct TrackEngagementCoordination {
    /// Shared enemy track awareness (CRDT layer)
    tracks: SharedContext<EnemyTrack>,
    
    /// Active engagement claims
    engagement_claims: ClaimRegistry,
    
    /// Spatial exclusion zones for active engagements
    active_zones: Vec<EngagementZone>,
    
    /// Conflict resolution strategy
    resolver: EngagementConflictResolver,
}

impl TrackEngagementCoordination {
    /// Attempt to claim a target for engagement
    pub async fn claim_target(
        &mut self,
        track_id: TrackId,
        weapon_system: WeaponSystem,
        engagement_plan: EngagementPlan,
    ) -> Result<EngagementClaim, ClaimError> {
        // 1. Check if track already claimed
        if let Some(existing) = self.engagement_claims.get(track_id) {
            // Conflict detected - apply resolution rules
            return self.resolver.resolve_conflict(existing, claim)?;
        }
        
        // 2. Check spatial deconfliction
        if self.violates_exclusion_zones(&engagement_plan.geometry) {
            return Err(ClaimError::DeconflictionViolation);
        }
        
        // 3. Create time-bounded claim
        let claim = Claim {
            id: ClaimId::new(),
            resource: ResourceId::Track(track_id),
            claimant: self.platform_id,
            claim_type: ClaimType::Exclusive {
                timeout: engagement_plan.estimated_duration,
                preemptible: true, // Higher priority can override
            },
            priority: self.calculate_priority(&engagement_plan),
            expires_at: now() + engagement_plan.estimated_duration,
            context: engagement_plan.mission_context,
        };
        
        // 4. Publish claim via CRDT
        self.engagement_claims.insert(claim.clone())?;
        
        // 5. Create exclusion zone
        let zone = EngagementZone {
            geometry: engagement_plan.weapon_geometry,
            active_until: claim.expires_at,
            claimant: self.platform_id,
        };
        self.active_zones.push(zone);
        
        Ok(EngagementClaim::new(claim))
    }
    
    /// Handle concurrent claim conflict
    fn resolve_conflict(
        &self,
        existing: &Claim,
        new: Claim,
    ) -> Result<EngagementClaim, ClaimError> {
        // Priority-based resolution
        match new.priority.cmp(&existing.priority) {
            Ordering::Greater => {
                // New claim has higher priority
                if existing.claim_type.is_preemptible() {
                    // Revoke existing, grant new
                    self.revoke_claim(existing.id)?;
                    Ok(EngagementClaim::new(new))
                } else {
                    Err(ClaimError::HigherPriorityNonPreemptible)
                }
            }
            Ordering::Less => {
                // Existing claim has higher priority
                Err(ClaimError::ConflictWithHigherPriority)
            }
            Ordering::Equal => {
                // Same priority - use deterministic tiebreaker
                self.deterministic_tiebreak(existing, &new)
            }
        }
    }
    
    /// Deterministic tiebreaking for same-priority conflicts
    fn deterministic_tiebreak(
        &self,
        existing: &Claim,
        new: &Claim,
    ) -> Result<EngagementClaim, ClaimError> {
        // Tiebreak rules (in order):
        // 1. Earlier timestamp wins (first-come-first-served)
        // 2. If simultaneous (within clock sync tolerance), platform ID wins
        // 3. Mission context factors (target value, rules of engagement)
        
        let time_delta = new.timestamp() - existing.timestamp();
        
        if time_delta.abs() > CLOCK_SYNC_TOLERANCE {
            // Clear temporal ordering
            if time_delta < 0 {
                Err(ClaimError::ConflictWithEarlierClaim)
            } else {
                self.revoke_claim(existing.id)?;
                Ok(EngagementClaim::new(new.clone()))
            }
        } else {
            // Simultaneous within clock tolerance - use platform ID
            if new.claimant < existing.claimant {
                self.revoke_claim(existing.id)?;
                Ok(EngagementClaim::new(new.clone()))
            } else {
                Err(ClaimError::ConflictWithLowerPlatformId)
            }
        }
    }
}
```

### Priority Calculation

```rust
/// Calculate engagement priority based on mission context
pub struct EngagementConflictResolver {
    /// Mission parameters influencing priority
    mission_params: MissionParameters,
    
    /// Platform capabilities
    platform_capabilities: PlatformCapabilities,
}

impl EngagementConflictResolver {
    /// Calculate priority for engagement claim
    fn calculate_priority(&self, plan: &EngagementPlan) -> Priority {
        let mut priority = 0.0;
        
        // 1. Target value/threat level (highest weight)
        priority += plan.target_value * 10.0;
        
        // 2. Weapon effectiveness (probability of kill)
        priority += plan.pk * 5.0;
        
        // 3. Time criticality (is target fleeting?)
        if plan.time_window < Duration::from_secs(30) {
            priority += 3.0;
        }
        
        // 4. Platform position advantage
        priority += self.position_advantage(plan) * 2.0;
        
        // 5. Ammunition state (prefer platforms with more ammo)
        priority += self.ammo_efficiency_bonus();
        
        // 6. Hierarchical rank (break ties)
        priority += self.platform_capabilities.rank as f64 * 0.1;
        
        Priority::from_score(priority)
    }
}
```

### Spatial Deconfliction

```rust
/// Spatial exclusion zone for engagement deconfliction
pub struct EngagementZone {
    /// Geometric representation of engagement area
    pub geometry: ZoneGeometry,
    
    /// When zone becomes inactive
    pub active_until: Timestamp,
    
    /// Platform that created zone
    pub claimant: PlatformId,
    
    /// Zone type determines enforcement
    pub zone_type: ZoneType,
}

pub enum ZoneGeometry {
    /// Weapon engagement zone (cone from platform)
    WeaponEngagementZone {
        origin: Position,
        bearing: f64,
        range: f64,
        cone_angle: f64,
    },
    
    /// No-strike zone (protected area)
    NoStrikeZone {
        center: Position,
        radius: f64,
        altitude_band: Option<(f64, f64)>,
    },
    
    /// Blue force position (keep clear)
    BlueForcePosition {
        position: Position,
        uncertainty_radius: f64,
    },
}

pub enum ZoneType {
    /// Hard exclusion - prevents claims
    HardExclusion,
    
    /// Coordination required - allow with acknowledgment
    CoordinationRequired,
    
    /// Advisory only - informational
    Advisory,
}

impl ExclusionZoneManager {
    /// Check if engagement plan violates any zones
    fn violates_exclusion_zones(
        &self,
        engagement_geometry: &ZoneGeometry,
    ) -> bool {
        self.active_zones.iter().any(|zone| {
            zone.intersects(engagement_geometry) 
                && zone.zone_type == ZoneType::HardExclusion
        })
    }
    
    /// Publish new exclusion zone via CRDT
    fn publish_exclusion_zone(&mut self, zone: EngagementZone) {
        // Add to local registry
        self.active_zones.push(zone.clone());
        
        // Propagate via CRDT shared context
        self.shared_zones.insert(zone.id, zone);
    }
}
```

### Time-Bounded Claims

```rust
/// Claim registry with automatic expiration
pub struct ClaimRegistry {
    /// Active claims indexed by resource
    claims: HashMap<ResourceId, Claim>,
    
    /// Expiration queue (sorted by expires_at)
    expiration_queue: BinaryHeap<ClaimExpiry>,
}

impl ClaimRegistry {
    /// Insert claim with automatic expiration
    pub fn insert(&mut self, claim: Claim) -> Result<(), ClaimError> {
        // Check for existing claim
        if let Some(existing) = self.claims.get(&claim.resource) {
            return Err(ClaimError::ResourceAlreadyClaimed);
        }
        
        // Add to expiration queue
        self.expiration_queue.push(ClaimExpiry {
            resource: claim.resource,
            expires_at: claim.expires_at,
        });
        
        // Store claim
        self.claims.insert(claim.resource, claim);
        
        Ok(())
    }
    
    /// Process expired claims
    pub fn expire_claims(&mut self) {
        let now = Timestamp::now();
        
        while let Some(expiry) = self.expiration_queue.peek() {
            if expiry.expires_at > now {
                break; // No more expired claims
            }
            
            let expiry = self.expiration_queue.pop().unwrap();
            
            // Remove expired claim
            if let Some(claim) = self.claims.remove(&expiry.resource) {
                log::info!(
                    "Claim expired: {:?} by {:?}",
                    claim.resource,
                    claim.claimant
                );
                
                // Publish revocation via CRDT
                self.publish_revocation(claim.id);
            }
        }
    }
}
```

### Hierarchical Override

```rust
/// Hierarchical coordination with override capability
pub struct HierarchicalCoordination {
    /// Current echelon level
    echelon: EchelonLevel,
    
    /// Parent echelon (if exists)
    parent: Option<EchelonId>,
}

impl HierarchicalCoordination {
    /// Submit claim with hierarchical context
    pub fn claim_with_hierarchy(
        &mut self,
        claim: Claim,
    ) -> Result<EngagementClaim, ClaimError> {
        // Tag claim with echelon level
        let mut claim = claim;
        claim.context.echelon = self.echelon;
        
        // Higher echelon claims are always preemptible
        if self.echelon > EchelonLevel::Platform {
            claim.claim_type = ClaimType::Exclusive {
                timeout: claim.expires_at - now(),
                preemptible: true, // Squad/platoon can override individual
            };
        }
        
        self.coordination.claim_target(claim)
    }
    
    /// Override lower-echelon claim
    pub fn override_claim(
        &mut self,
        existing_claim_id: ClaimId,
        new_claim: Claim,
    ) -> Result<EngagementClaim, ClaimError> {
        // Verify authority to override
        let existing = self.claims.get(existing_claim_id)?;
        
        if new_claim.context.echelon <= existing.context.echelon {
            return Err(ClaimError::InsufficientAuthority);
        }
        
        // Force revocation of lower claim
        self.revoke_claim(existing_claim_id)?;
        
        // Insert new claim
        self.insert_claim(new_claim)
    }
}
```

## Time Synchronization

```rust
/// Loose time synchronization for coordination
pub struct LooseTimeSync {
    /// Clock skew relative to reference
    skew: Duration,
    
    /// Uncertainty in clock synchronization
    uncertainty: Duration,
    
    /// Last sync with reference clock
    last_sync: Instant,
}

impl LooseTimeSync {
    /// Maximum acceptable clock drift for coordination
    const MAX_COORDINATION_SKEW: Duration = Duration::from_millis(500);
    
    /// Get current timestamp with uncertainty bounds
    pub fn now_with_uncertainty(&self) -> TimestampRange {
        let now = self.adjusted_now();
        TimestampRange {
            earliest: now - self.uncertainty,
            latest: now + self.uncertainty,
        }
    }
    
    /// Check if two timestamps are simultaneous within tolerance
    pub fn are_simultaneous(&self, t1: Timestamp, t2: Timestamp) -> bool {
        (t1 - t2).abs() <= self.uncertainty * 2
    }
}
```

## Conflict Resolution Strategies

### Resolution Decision Tree

```
Claim Conflict Detected
│
├─ Compare Priorities
│  ├─ New > Existing → Check Preemptibility
│  │  ├─ Preemptible → Revoke Existing, Grant New
│  │  └─ Non-Preemptible → Deny New
│  │
│  ├─ New < Existing → Deny New
│  │
│  └─ New == Existing → Deterministic Tiebreak
│     ├─ Compare Timestamps (outside sync tolerance)
│     │  └─ Earlier timestamp wins
│     │
│     └─ Simultaneous (within sync tolerance)
│        ├─ Compare Platform IDs (deterministic)
│        └─ Compare Mission Context (target value, etc.)
│
└─ Network Partition Handling
   ├─ Local Decision → Apply resolution rules
   └─ Convergence → CRDT merge resolves conflicts
```

### Partition Tolerance

During network partitions:
1. **Local Coordination**: Platforms coordinate within connected subset
2. **Conservative Engagement**: When uncertain, defer to safety rules
3. **Post-Partition Convergence**: CRDT merge brings claims together
4. **Conflict Detection**: Identify duplicate engagements after reconnection
5. **Audit and Learn**: Record conflicts for ROE/priority adjustment

```rust
/// Handle coordination during network partition
impl CoordinationLayer {
    /// Merge coordination state after partition heals
    pub fn merge_partition_state(
        &mut self,
        remote_claims: Vec<Claim>,
    ) -> PartitionMergeReport {
        let mut conflicts = Vec::new();
        
        for remote_claim in remote_claims {
            if let Some(local_claim) = self.claims.get(&remote_claim.resource) {
                // Conflict detected - both sides claimed same resource
                conflicts.push(ClaimConflict {
                    resource: remote_claim.resource,
                    local: local_claim.clone(),
                    remote: remote_claim,
                });
                
                // Apply deterministic resolution
                let winner = self.resolver.resolve_conflict(
                    local_claim,
                    remote_claim,
                )?;
                
                // Update local state to match resolution
                self.claims.insert(winner.resource, winner);
            } else {
                // No conflict - accept remote claim
                self.claims.insert(remote_claim.resource, remote_claim);
            }
        }
        
        PartitionMergeReport {
            conflicts_detected: conflicts.len(),
            conflicts,
            resolution_applied: true,
        }
    }
}
```

## Safety Guarantees

### Coordination Safety Properties

1. **Deconfliction Invariant**: At most one active engagement per track (within connected component)
2. **Spatial Exclusion**: No engagements violate no-strike zones
3. **Temporal Bounds**: All claims expire within finite time
4. **Hierarchical Consistency**: Higher echelon can always override lower
5. **Eventual Consistency**: After partition, deterministic resolution produces same result

### Safety Under Partition

```rust
/// Safety checks before engagement
impl EngagementSafety {
    /// Verify engagement is safe to execute
    pub fn verify_safe_to_engage(
        &self,
        claim: &EngagementClaim,
        current_geometry: &WeaponGeometry,
    ) -> Result<(), SafetyViolation> {
        // 1. Verify claim is still valid
        if claim.has_expired() {
            return Err(SafetyViolation::ExpiredClaim);
        }
        
        // 2. Check no-strike zones (even if claim was valid)
        if self.violates_no_strike_zone(current_geometry) {
            return Err(SafetyViolation::NoStrikeZoneViolation);
        }
        
        // 3. Verify blue force deconfliction
        if self.risks_fratricide(current_geometry) {
            return Err(SafetyViolation::FratricideRisk);
        }
        
        // 4. Check rules of engagement
        if !self.roe_allows_engagement(&claim.context) {
            return Err(SafetyViolation::ROEViolation);
        }
        
        Ok(())
    }
}
```

## Implementation Considerations

### Claim Storage

Claims are stored in **two layers**:
1. **Local Registry**: Fast lookup for coordination decisions (HashMap)
2. **CRDT Shared Context**: Distributed via Automerge for propagation

```rust
pub struct ClaimPersistence {
    /// Local fast lookup
    local: ClaimRegistry,
    
    /// CRDT-backed persistent storage
    shared: SharedContext<Claim>,
}

impl ClaimPersistence {
    /// Insert claim locally and propagate
    pub fn insert(&mut self, claim: Claim) -> Result<(), ClaimError> {
        // 1. Validate locally
        self.local.insert(claim.clone())?;
        
        // 2. Propagate via CRDT
        self.shared.insert(claim.id, claim)?;
        
        Ok(())
    }
    
    /// Merge remote claims from CRDT sync
    pub fn on_remote_change(&mut self, claim: Claim) {
        // Check for conflicts with local state
        if let Some(local) = self.local.get(&claim.resource) {
            // Apply conflict resolution
            let winner = self.resolver.resolve(local, &claim);
            self.local.insert(winner);
        } else {
            // No conflict - accept remote claim
            self.local.insert(claim);
        }
    }
}
```

### Performance

- **Claim Lookup**: O(1) via HashMap
- **Expiration Processing**: O(log n) via BinaryHeap
- **Conflict Resolution**: O(1) priority comparison
- **Zone Intersection**: O(n) zones × O(1) geometry check

### Memory Overhead

```
Per-Claim Storage:
- Claim struct: ~200 bytes
- HashMap entry: ~32 bytes
- CRDT overhead: ~100 bytes/claim (Automerge encoding)
Total: ~332 bytes/claim

1000 active claims = ~324 KB
```

## Alternatives Considered

### 1. **Full Consensus (Raft/Paxos)**

**Rejected**: Requires majority availability and low latency. Cannot function during network partitions lasting minutes/hours.

### 2. **Pessimistic Locking**

**Rejected**: Blocking coordination incompatible with high-latency, unreliable networks. Would freeze operations during disconnection.

### 3. **Centralized Coordinator**

**Rejected**: Single point of failure. Cannot support autonomous operation during disconnection from higher echelons.

### 4. **No Coordination (Pure Autonomy)**

**Rejected**: Insufficient for safety-critical operations. Risk of duplicate engagement and fratricide unacceptable.

## Decision

Implement **optimistic coordination with deterministic conflict resolution**:

1. ✅ **Claim-Based Coordination**: Time-bounded claims tracked via CRDT
2. ✅ **Priority-Based Resolution**: Mission-aware priority calculation
3. ✅ **Spatial Deconfliction**: Exclusion zones with geometric intersection
4. ✅ **Hierarchical Override**: Higher echelon preemption capability
5. ✅ **Partition Tolerance**: Deterministic resolution after network reconvergence
6. ✅ **Safety First**: Conservative defaults when coordination uncertain

## Implementation Phases

### Phase 1: Core Claim Registry (Week 1-2)
- [ ] Implement `ClaimRegistry` with expiration
- [ ] Basic priority-based conflict resolution
- [ ] Time synchronization utilities
- [ ] Unit tests for deterministic tiebreaking

### Phase 2: Track Engagement (Week 3-4)
- [ ] `TrackEngagementCoordination` implementation
- [ ] Weapon engagement zone geometry
- [ ] Integration with shared context (ADR-008)
- [ ] Engagement claim lifecycle

### Phase 3: Spatial Deconfliction (Week 5-6)
- [ ] Exclusion zone manager
- [ ] Zone geometry intersection algorithms
- [ ] No-strike zone enforcement
- [ ] Blue force position tracking

### Phase 4: Hierarchical Coordination (Week 7-8)
- [ ] Echelon-aware claims
- [ ] Override mechanisms
- [ ] Squad-level coordination primitives
- [ ] Integration with hierarchical sync (ADR-009)

### Phase 5: Partition Handling (Week 9-10)
- [ ] Partition detection
- [ ] Local coordination during split
- [ ] Post-partition merge and conflict detection
- [ ] Audit trail for conflicts

### Phase 6: Safety & Testing (Week 11-12)
- [ ] Safety verification before engagement
- [ ] Formal verification of safety invariants
- [ ] Simulation testing with Shadow
- [ ] Performance benchmarking

## Success Metrics

1. **Coordination Latency**: <2 seconds claim acknowledgment in connected network
2. **Conflict Rate**: <5% duplicate claims under normal operation
3. **Deconfliction Success**: 100% prevention of safety violations (no-strike, fratricide)
4. **Partition Recovery**: Deterministic conflict resolution in <30 seconds after reconnection
5. **Scale**: Support 100+ simultaneous claims across 50 platforms
6. **Memory**: <10 MB coordination state for 1000 active claims

## Open Questions

1. **Time Synchronization**: What clock sync accuracy is achievable in tactical networks? (Target: <500ms)
2. **Claim Timeouts**: Optimal timeout values for different operation types? (Start: 30s-5min)
3. **Priority Tuning**: How to balance mission factors in priority calculation? (Requires field testing)
4. **Geometric Complexity**: How detailed should weapon engagement zones be? (Trade accuracy vs computation)
5. **Hierarchical Override Policy**: When should higher echelon auto-override vs request coordination? (ROE-dependent)

## References

- ADR-007: Automerge-based Sync Engine (CRDT foundation)
- ADR-008: Bidirectional Hierarchical Flows (shared context patterns)
- ADR-009: Hierarchical Flows (echelon structure)
- ADR-010: Transport Layer (network model)
- "Time, Clocks, and the Ordering of Events in a Distributed System" (Lamport 1978)
- "Consistency in Non-Transactional Distributed Storage Systems" (Viotti & Vukolić 2016)
- "CAP Twelve Years Later: How the 'Rules' Have Changed" (Brewer 2012)
- Military: Joint Pub 3-09 "Joint Fire Support" (coordination doctrine)
