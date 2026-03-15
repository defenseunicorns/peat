# ADR-009: Bidirectional Hierarchical Flows - Command, Control, and Software Logistics

**Status**: Accepted (Implementation in progress)
**Date**: 2025-11-05
**Updated**: 2025-11-18
**Authors**: Claude, Kit Plummer
**Relates to**: ADR-001 (Peat Protocol), ADR-004 (Human-Machine Composition), ADR-007 (Automerge Sync), ADR-008 (Network Simulation), ADR-021 (Document-Oriented Architecture)

## Context

### The Missing Half of the Story

CAP documentation through ADR-007 has extensively covered **upward aggregation**:
- Capability advertisement from platforms → squads → platoons → company
- Sensor data hierarchical compression
- Status updates flowing to C2
- Emergent team capability discovery

However, this represents only **half of operational requirements**. Real distributed operations require equally robust **downward distribution**:
- **Command & Control**: Orders, ROE updates, mission taskings, Commander's Intent
- **Software Logistics**: AI models, capability packages, firmware updates, configuration changes
- **Decision Distribution**: Authority delegation, decision rules, behavioral constraints
- **Context Propagation**: Enemy disposition, friendly force locations, no-strike zones, deconfliction data

The current documentation creates an impression that CAP is primarily an **upward data aggregation protocol**, when in reality it must be a **full-duplex hierarchical coordination system** for contested environments.

### The Operational Reality

Military operations require bidirectional information flow:

**Upward (Tactical to Strategic):**
- "Squad Alpha has 3 ISR-capable platforms with 45min fuel remaining"
- "Detected 12 enemy vehicles in Grid 7"
- "Platform 3 lost strike capability due to payload malfunction"

**Downward (Strategic to Tactical):**
- "Priority shift: Focus ISR on Grid 9, enemy armor detected"
- "ROE update: No strike within 500m of Grid 7 due to civilian presence"
- "Deploy new target recognition model v4.2 to all ISR platforms"

**Horizontal (Peer Coordination):**
- "Squad Alpha handing off track custody to Squad Bravo"
- "Deconfliction: Squad Charlie executing strike in Grid 8 at 14:35Z"

### The Contested Communications Challenge

Bidirectional flows face asymmetric challenges in contested environments:

**Upward Challenges:**
- High data volume (many sensors → few C2 nodes)
- Requires compression and aggregation
- Can tolerate some staleness (minutes)
- Bandwidth-limited

**Downward Challenges:**
- Critical timeliness (orders must arrive before execution window)
- Cannot tolerate loss (unlike redundant sensor data)
- Often large payloads (AI models = tens of MB)
- Must work through network partitions

**Traditional architectures fail** because they treat all data flows equally, creating a "broadcast storm" where critical commands compete with routine telemetry for bandwidth.

### Business Drivers

1. **Distributed C2 Doctrine**: JADC2, MDO, and distributed operations require pushing decision authority to the edge
2. **Edge AI Deployment**: AI models must be distributed TO platforms, not just aggregate results FROM them
3. **Dynamic Mission Adaptation**: Missions change mid-execution; updated guidance must reach platforms
4. **Software-Defined Capabilities**: Modern platforms gain/lose capabilities via software, not just hardware
5. **Contested Environment Reality**: Cannot assume continuous connectivity to central authorities

### Technical Constraints

1. **Asymmetric Bandwidth**: Uplinks often better than downlinks (tactical radios, satellite)
2. **Intermittent Connectivity**: Network partitions lasting minutes to hours
3. **Priority Inversion Risk**: Bulk data can starve critical commands
4. **Storage Limits**: Edge platforms have limited storage for queued data
5. **Security Requirements**: Different data requires different protection levels
6. **Multicast Efficiency**: Commands often target groups, not individuals

## Decision

We will **explicitly architect CAP as a full-duplex hierarchical synchronization system** with distinct strategies for upward aggregation, downward distribution, and horizontal coordination.

### Architectural Principles

#### 1. Direction-Aware Collections

Organize Ditto collections by primary flow direction and access patterns:

```rust
// UPWARD FLOW: Many writers (platforms) → Few readers (leaders)
// Characteristics: High volume, compressible, can tolerate staleness
collections:
  - "platforms.telemetry"      // Individual platform states
  - "platforms.detections"     // Sensor contacts
  - "squads.capabilities"      // Aggregated squad capabilities
  - "platoon.readiness"        // Rollup of platoon status

// DOWNWARD FLOW: Few writers (leaders) → Many readers (platforms)  
// Characteristics: Low volume, high priority, cannot tolerate loss
collections:
  - "company.orders"           // Strategic directives
  - "platoon.taskings"         // Operational assignments
  - "squad.coordination"       // Tactical plans
  - "software.packages"        // AI models, configs, updates

// BIDIRECTIONAL: Any level can write/read
// Characteristics: Shared context, eventual consistency acceptable
collections:
  - "shared.enemy_disposition" // Common operating picture
  - "shared.no_strike_zones"   // Deconfliction data
  - "shared.blue_force_track"  // Friendly locations
```

#### 2. Priority-Based Synchronization

Extend priority system to handle both directions:

```rust
pub enum SyncPriority {
    // DOWNWARD PRIORITIES (orders must arrive)
    Critical,        // ROE changes, abort commands (immediate)
    High,            // Mission re-tasking (seconds)
    Normal,          // Routine orders (minutes)
    
    // UPWARD PRIORITIES (status must report)  
    Urgent,          // Platform failure, enemy contact (immediate)
    Important,       // Capability changes (seconds)
    Routine,         // Position updates (minutes)
    
    // BACKGROUND (eventual delivery acceptable)
    Bulk,            // Software packages (hours/days)
    Archive,         // Historical data (opportunistic)
}

impl SyncStrategy for BidirectionalSync {
    fn route(&self, collection: &str, priority: SyncPriority) -> SyncPath {
        match (collection, priority) {
            // Critical downward commands bypass normal routing
            ("company.orders", Critical) => SyncPath::DirectBroadcast,
            
            // Bulk upward data uses hierarchical aggregation
            ("platforms.telemetry", Routine) => SyncPath::HierarchicalAggregation,
            
            // Large downward payloads use multicast trees
            ("software.packages", Bulk) => SyncPath::MulticastTree,
            
            // Urgent upward alerts shortcut to C2
            ("platforms.detections", Urgent) => SyncPath::DirectToC2,
        }
    }
}
```

#### 3. Multicast Distribution Trees

For downward distribution (especially software), use hierarchical multicast:

```
Company HQ: Has AI model v4.2 (45MB)
    ↓ (distribute to platoon leaders)
Platoon 1 Leader: Receives, caches, redistributes
    ↓ (multicast to squad leaders)
    ├─ Squad A Leader: Receives, caches, redistributes
    │   ↓ (local mesh to platforms)
    │   ├─ Platform 1: Receives
    │   ├─ Platform 2: Receives
    │   └─ Platform 3: Receives
    │
    └─ Squad B Leader: Receives, caches, redistributes
        └─ ...

Benefits:
- Single long-haul transmission (Company → Platoon)
- Parallel distribution within platoons
- Caching at each level for resilience
- Resume-able if connection breaks
```

Implementation:

```rust
pub struct MulticastTree {
    pub root: NodeId,              // Company HQ
    pub intermediate: Vec<NodeId>, // Platoon/Squad leaders
    pub leaves: Vec<NodeId>,       // Platforms
}

impl MulticastTree {
    pub async fn distribute_package(&self, package: SoftwarePackage) {
        // Phase 1: Root → Intermediate nodes
        for node in &self.intermediate {
            self.transmit_with_resume(package.clone(), node).await;
        }
        
        // Phase 2: Intermediate → Leaves (parallel)
        for intermediate in &self.intermediate {
            tokio::spawn(async move {
                let leaves = self.leaves_for(intermediate);
                for leaf in leaves {
                    self.transmit_with_resume(package.clone(), leaf).await;
                }
            });
        }
    }
    
    async fn transmit_with_resume(&self, package: SoftwarePackage, target: NodeId) {
        let chunks = package.split_into_chunks(1_000_000); // 1MB chunks
        for (i, chunk) in chunks.enumerate() {
            loop {
                match self.send_chunk(chunk, target, i).await {
                    Ok(_) => break,
                    Err(NetworkError::Disconnected) => {
                        // Wait for reconnection, resume from this chunk
                        self.wait_for_reconnection(target).await;
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
}
```

#### 4. Pre-Positioning for Disconnection

Anticipate network partitions by pre-distributing critical data:

```rust
pub struct PrepositionedData {
    // Mission data loaded before insertion
    pub roe_rules: RulesOfEngagement,
    pub no_strike_zones: Vec<Boundary>,
    pub decision_matrices: DecisionLogic,
    pub fallback_behaviors: EmergencyProtocols,
    
    // Software loaded at rally points
    pub ai_models: HashMap<String, ModelPackage>,
    pub capability_configs: Vec<Configuration>,
    pub threat_libraries: ThreatDatabase,
}

impl Squad {
    pub fn preposition_for_mission(&mut self, mission: &Mission) {
        // Load before leaving controlled network
        self.cached_data = PrepositionedData {
            roe_rules: mission.roe.clone(),
            no_strike_zones: mission.deconfliction_zones(),
            decision_matrices: mission.decision_logic(),
            fallback_behaviors: mission.emergency_protocols(),
            
            ai_models: self.download_models_for_mission(mission),
            capability_configs: mission.platform_configs(),
            threat_libraries: mission.threat_data(),
        };
        
        // Platforms operate autonomously using cached data during disconnection
        // Updates sync opportunistically when connected
    }
}
```

#### 5. Delta-Based Model Distribution

AI models are large but change incrementally. Use CRDT deltas:

```rust
pub struct ModelPackage {
    pub model_id: String,
    pub version: String,
    pub full_size_mb: usize,
    pub base_version: Option<String>, // Previous version for delta
}

impl ModelPackage {
    pub fn compute_delta_from(&self, base: &ModelPackage) -> ModelDelta {
        // Only transmit changed layers
        // YOLOv8 v3 → v4 might be 45MB full, 8MB delta
        ModelDelta {
            base_version: base.version.clone(),
            changed_layers: self.diff_layers(base),
            size_mb: 8, // Much smaller
        }
    }
}

// Distribution strategy
impl SoftwareDistribution {
    pub async fn distribute_model(&self, model: ModelPackage) {
        // Check what versions platforms already have
        let platform_versions = self.query_installed_versions(&model.model_id);
        
        if platform_versions.all_have("v3") && model.version == "v4" {
            // Distribute delta (8MB instead of 45MB)
            let delta = model.compute_delta_from("v3");
            self.multicast(delta).await;
        } else {
            // Some platforms need full model
            self.multicast(model).await;
        }
    }
}
```

#### 6. Command Acknowledgment & Audit

Unlike sensor data (many redundant sources), commands must be tracked:

```rust
pub struct CommandPacket {
    pub command_id: uuid::Uuid,
    pub issued_by: NodeId,
    pub issued_at: Timestamp,
    pub command_type: CommandType,
    pub payload: CommandPayload,
    pub requires_ack: bool,
    pub expires_at: Option<Timestamp>,
}

pub struct CommandAcknowledgment {
    pub command_id: uuid::Uuid,
    pub acknowledged_by: NodeId,
    pub acknowledged_at: Timestamp,
    pub status: AckStatus,
}

pub enum AckStatus {
    Received,      // Command received
    Understood,    // Command parsed and validated
    Executing,     // Command in progress
    Completed,     // Command executed successfully
    Failed(String), // Command execution failed
    Rejected(String), // Command rejected (policy violation)
}

impl CommandTracking {
    pub async fn issue_command(&self, cmd: CommandPacket) -> Result<()> {
        // Persist command for audit
        self.store_command(&cmd).await?;
        
        // Distribute via appropriate path
        let recipients = self.determine_recipients(&cmd);
        self.multicast_with_priority(cmd, recipients, Priority::High).await?;
        
        // Track acknowledgments
        if cmd.requires_ack {
            self.wait_for_acks(cmd.command_id, recipients, Duration::from_secs(30)).await?;
        }
        
        Ok(())
    }
    
    pub async fn handle_ack(&self, ack: CommandAcknowledgment) {
        // Record acknowledgment
        self.store_ack(&ack).await;
        
        // Alert if command fails
        if matches!(ack.status, AckStatus::Failed(_) | AckStatus::Rejected(_)) {
            self.alert_command_failure(ack).await;
        }
    }
}
```

#### 7. Decision Logic Distribution

Pre-distribute decision rules for autonomous operation:

```rust
pub struct DecisionPackage {
    pub rule_id: String,
    pub version: String,
    pub scope: DecisionScope,
    pub rules: Vec<DecisionRule>,
    pub fallback: FallbackBehavior,
}

pub struct DecisionRule {
    pub condition: Condition,
    pub action: Action,
    pub requires_approval: Option<AuthorityLevel>,
    pub timeout: Option<Duration>,
}

// Example: Engagement decision logic
let engagement_rules = DecisionPackage {
    rule_id: "engagement_roe_v4".to_string(),
    version: "4.0".to_string(),
    scope: DecisionScope::AllPlatforms,
    rules: vec![
        DecisionRule {
            condition: Condition::And(vec![
                Condition::TargetType("infantry"),
                Condition::InZone("engagement_area_bravo"),
                Condition::NoCiviliansWithin(50.0), // meters
            ]),
            action: Action::If(
                Condition::LeaderAvailable,
                Box::new(Action::RequestApproval),
                Box::new(Action::AutoApproveWithin(Duration::from_secs(1800))), // 30min window
            ),
            requires_approval: Some(AuthorityLevel::SquadLeader),
            timeout: Some(Duration::from_secs(300)),
        },
    ],
    fallback: FallbackBehavior::Conservative, // When in doubt, don't engage
};

impl Platform {
    pub async fn make_decision(&self, situation: Situation) -> Decision {
        // Use locally cached decision logic
        let rules = self.cached_decision_rules.get(&situation.decision_type)?;
        
        // Evaluate rules
        for rule in &rules.rules {
            if rule.condition.evaluate(&situation) {
                // Check if approval required and leader available
                if let Some(authority) = rule.requires_approval {
                    if self.can_reach_authority(authority) {
                        return self.request_approval(situation, rule).await;
                    } else {
                        // Leader unreachable, use timeout/fallback
                        return self.autonomous_decision(situation, rule);
                    }
                }
                return self.execute_action(&rule.action, situation);
            }
        }
        
        // No rule matched, use fallback
        self.fallback_behavior(situation)
    }
}
```

#### 8. Contextual Data Propagation

Shared battlefield awareness flows in all directions:

```rust
pub struct SharedContext {
    // Enemy disposition (any echelon can contribute)
    pub enemy_tracks: Vec<Track>,
    
    // Friendly forces (bidirectional awareness)
    pub blue_force_tracks: Vec<Track>,
    
    // No-strike zones (company sets, squads add)
    pub no_strike_zones: Vec<Zone>,
    
    // Deconfliction (horizontal coordination)
    pub active_strikes: Vec<StrikeNotification>,
}

// Collection: "shared.battlespace_context"
impl SharedContext {
    pub fn update_enemy_track(&mut self, track: Track, contributor: NodeId) {
        // Anyone can contribute
        // CRDT merge handles conflicts
        self.enemy_tracks.upsert(track);
        
        // Propagates to all relevant nodes
        // Platforms get awareness without asking
    }
    
    pub fn add_no_strike_zone(&mut self, zone: Zone, authority: AuthorityLevel) {
        // Verify authority
        if authority >= AuthorityLevel::PlatoonLeader {
            self.no_strike_zones.push(zone);
            // Immediately propagates downward
            // All platforms update local caches
        }
    }
}
```

### Synchronization Strategies by Data Type

| Data Type | Primary Direction | Volume | Latency | Strategy |
|-----------|------------------|---------|---------|----------|
| Telemetry | Upward | High | Tolerant (minutes) | Hierarchical aggregation, delta sync |
| Detections | Upward | Medium | Important (seconds) | Priority routing, direct to C2 |
| Commands | Downward | Low | Critical (seconds) | Direct broadcast, ack required |
| ROE Updates | Downward | Low | Critical (seconds) | Direct broadcast, persistent cache |
| AI Models | Downward | Very High | Tolerant (hours) | Multicast tree, chunked, delta-based |
| Configs | Downward | Medium | Normal (minutes) | Multicast tree, versioned |
| Enemy Tracks | Bidirectional | Medium | Important (seconds) | Shared collection, CRDT merge |
| No-Strike Zones | Downward/Bi | Low | Critical (seconds) | Shared collection, immediate propagate |
| Deconfliction | Horizontal | Low | Critical (seconds) | Peer broadcast, ack required |

### Implementation Phases

#### Phase 1: Collection Architecture (Weeks 1-2)

1. Define collection taxonomy by flow direction
2. Implement direction-aware sync policies
3. Add priority routing for downward commands
4. Create command acknowledgment system

#### Phase 2: Software Distribution (Weeks 3-4)

1. Implement multicast tree construction
2. Add chunked transmission with resume
3. Create delta-based model distribution
4. Add caching at intermediate nodes

#### Phase 3: Decision Distribution (Weeks 5-6)

1. Define decision package schema
2. Implement pre-positioning loader
3. Add offline decision evaluation
4. Create approval request routing

#### Phase 4: Context Propagation (Weeks 7-8)

1. Implement shared context collections
2. Add bidirectional merge logic
3. Create deconfliction coordination
4. Add audit logging for all flows

## Consequences

### Positive

1. **Complete Operational Picture**: Both "data up" and "commands down" explicitly supported
2. **Contested Environment Readiness**: Pre-positioning and caching enable autonomous operation
3. **Efficient Software Logistics**: Multicast and delta-based distribution reduce bandwidth 10-100x
4. **Distributed C2**: Decision logic distribution enables true edge autonomy
5. **Audit Compliance**: Command tracking provides accountability trail
6. **Bandwidth Optimization**: Direction-aware strategies prevent priority inversion
7. **Resilience**: Intermediate caching survives network partitions

### Negative

1. **Complexity**: Managing bidirectional flows more complex than unidirectional
2. **Storage Requirements**: Caching at multiple levels increases storage needs
3. **Consistency Challenges**: Multiple copies of data must stay synchronized
4. **Testing Difficulty**: Must test all flow directions and priority interactions
5. **Security Surface**: More data flows = more attack vectors to secure

### Risks & Mitigations

**Risk**: Downward commands lost during network partition  
**Mitigation**: Persistent caching, retry with exponential backoff, eventual delivery guarantee

**Risk**: Outdated decision logic after update  
**Mitigation**: Version tracking, automatic invalidation of old versions, forced updates

**Risk**: Storage overflow on intermediate nodes  
**Mitigation**: Priority-based eviction, archive to external storage, compression

**Risk**: Command spoofing or tampering  
**Mitigation**: Cryptographic signatures on all commands (ADR-006), chain of custody audit

**Risk**: Priority inversion between upward and downward flows  
**Mitigation**: Separate bandwidth allocation per direction, preemption for critical commands

## Related Decisions

- **ADR-001 (Peat Protocol POC)**: Establishes hierarchical architecture for upward flows
- **ADR-004 (Human-Machine Composition)**: Defines authority model for decision delegation
- **ADR-006 (Security)**: Cryptographic signatures for command authentication
- **ADR-007 (Automerge Sync)**: CRDT foundation enables bidirectional sync

## Future Considerations

1. **Adaptive Bandwidth Allocation**: Dynamically adjust upward/downward bandwidth split based on mission phase
2. **Predictive Pre-positioning**: ML to predict what data/software will be needed, preload proactively
3. **Multi-Domain Coordination**: Extend to air, maritime, cyber domains with domain-specific optimizations
4. **Coalition Operations**: Different authority models for multi-national forces
5. **Electromagnetic Spectrum Awareness**: Integrate with EW systems for adaptive routing

## References

- JADC2 Concept of Operations
- MDO Multi-Domain Operations doctrine
- Link 16 downlink message structure
- Over-the-Air (OTA) update best practices
- Byzantine Generals Problem (command authentication)

## Implementation Status (Updated 2025-11-18)

### Phase 1: Command Dissemination (In Progress)

**Completed:**
- Command schema fully defined in `peat-schema/proto/command.proto`:
  - `HierarchicalCommand` with policies (buffer, conflict, acknowledgment, leader change)
  - `CommandAcknowledgment` with ack status flow (received, accepted, completed, rejected, failed)
  - `CommandTarget` with scope (individual, squad, platoon, broadcast)
- Core logic implemented in `peat-protocol/src/command/`:
  - `CommandCoordinator` - Command lifecycle management with in-memory tracking
  - `CommandRouter` - Target resolution and routing logic (individual/squad/platoon/broadcast)
  - `ConflictResolver` - Policy-based conflict resolution (last-write-wins, highest-priority, highest-authority, merge-compatible, reject)
  - `TimeoutManager` - Expiration and acknowledgment timeout tracking
  - Unit tests for all components (100% coverage of coordinator logic)

**In Progress:**
- Ditto storage integration for command dissemination:
  - Creating `CommandStorage` trait (backend-agnostic, following ADR-021 pattern)
  - Implementing `DittoCommandStorage` for command publishing/subscribing
  - Collections: `hierarchical_commands`, `hierarchical_commands_acks`
  - Observer-based command reception (following E2E test harness patterns)

**Next Steps:**
1. Complete Ditto storage integration (commands + acknowledgments)
2. Write E2E tests validating command flow across real Ditto mesh
3. Validate policy-based routing in distributed scenarios
4. Measure acknowledgment latency and reliability

### Architecture Alignment with ADR-021

The command dissemination implementation follows the **Document-Oriented Architecture** pattern established in ADR-021:

**Upward Flow (Hierarchical Aggregation):**
- Storage trait: `SummaryStorage` (trait for backend flexibility)
- Ditto implementation: `DittoSummaryStorage`
- Coordinator: `HierarchicalAggregator`
- Pattern: Create-once (squad formation), update-many (delta updates)

**Downward Flow (Command Dissemination):**
- Storage trait: `CommandStorage` (trait for backend flexibility)
- Ditto implementation: `DittoCommandStorage` (in progress)
- Coordinator: `CommandCoordinator` (exists, needs storage integration)
- Pattern: Publish-once (command issuance), acknowledge-many (ack tracking)

This symmetric architecture ensures both upward and downward flows follow the same design principles, enabling future backend switching (Ditto ↔ Automerge/Iroh) without changing application logic.

### Lessons Learned

1. **Policy Flexibility is Critical**: The conflict resolution policies (highest-priority-wins, highest-authority-wins, etc.) proved essential during design validation. Different mission scenarios require different conflict resolution strategies.

2. **Acknowledgment Tracking Complexity**: Unlike upward aggregation (many redundant sensors), downward commands must be individually tracked. The `TimeoutManager` handles this with per-target timeout tracking.

3. **In-Memory First, Storage Second**: Implementing coordinator logic with in-memory state first (unit tests) before Ditto integration allowed rapid iteration on routing and conflict resolution logic.

4. **Separation of Concerns**: The three-layer architecture (Router → Coordinator → Storage) cleanly separates target resolution, lifecycle management, and persistence concerns.

## Conclusion

CAP is not just an upward capability aggregation protocol—it is a **full-duplex hierarchical coordination system** designed for contested environments. By explicitly architecting for bidirectional flows with direction-aware strategies, we enable:

- **Commanders** to push intent and decisions to the edge
- **Platforms** to aggregate capabilities and data upward
- **Software** to distribute efficiently through hierarchical multicast
- **Context** to propagate in all directions for shared awareness

This bidirectional architecture transforms CAP from a monitoring system into a complete distributed command and control framework for autonomous operations at scale.
