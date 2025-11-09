# Bidirectional Hierarchical Flow Design

## Executive Summary

### Design Philosophy: Policy-Based Flexibility

**CAP Protocol provides the mechanism, integrators provide the policy.**

This design enables bidirectional hierarchical flow (both upward status aggregation and downward command dissemination) through a **policy-based flexible architecture**. Rather than hard-coding operational behaviors, the protocol defines **policy enumerations** in the schema that integrators configure per-command based on their specific tactical requirements.

### Core Principle

> **"cap-schema and cap-protocol should provide the kind of flexibility that enables integrators to create best practices for different scenarios"**

#### Four Policy Dimensions

Every command includes configurable policies across four dimensions:

1. **Partition Handling** (`BufferPolicy`): How to handle network partitions
   - Buffer and retry? Drop if unreachable? Require immediate delivery?

2. **Conflict Resolution** (`ConflictPolicy`): How to resolve conflicting commands
   - Last writer wins? Priority wins? Explicit supersession? Merge compatible?

3. **Acknowledgment** (`AcknowledgmentPolicy`): How to handle acknowledgments
   - Auto-ack on delivery? Require explicit ack? No ack needed? Ack on execution?

4. **Leader Change** (`LeaderChangePolicy`): What to do if leader changes during execution
   - Reroute to new leader? Abort? Continue with original? Notify and confirm?

### Key Benefits

**For Integrators**:
- ✅ Full control over command behavior without modifying the protocol
- ✅ Different policies for different tactical scenarios
- ✅ Best practice presets for common use cases
- ✅ Clear, documented semantics in the schema

**For Protocol Developers**:
- ✅ No hard-coded assumptions about operational requirements
- ✅ Extensible - add new policies without breaking changes
- ✅ Testable - policy enforcement logic is isolated and clear
- ✅ Maintainable - behavior configuration separate from routing logic

**For the Mission**:
- ✅ Adaptable to diverse operational contexts (kinetic, ISR, humanitarian)
- ✅ Supports both mission-critical and routine traffic
- ✅ Enables tactical flexibility while maintaining protocol reliability
- ✅ Clear audit trail of policy decisions

### Example: Same Protocol, Different Behaviors

```rust
// Strike authorization - maximum reliability
let strike_order = CommandBuilder::mission_critical()
    .with_buffer_policy(BufferPolicy::BUFFER_AND_RETRY)
    .with_conflict_policy(ConflictPolicy::EXPLICIT_SUPERSEDE)
    .with_ack_policy(AcknowledgmentPolicy::REQUIRE_EXPLICIT_ACK)
    .with_leader_change_policy(LeaderChangePolicy::NOTIFY_AND_CONFIRM)
    .build();

// Enemy position update - stale data is worse than no data
let intel_update = CommandBuilder::tactical_update()
    .with_buffer_policy(BufferPolicy::DROP_ON_PARTITION)
    .with_conflict_policy(ConflictPolicy::LAST_WRITER_WINS)
    .with_ack_policy(AcknowledgmentPolicy::AUTO_ACK_ON_DELIVERY)
    .with_leader_change_policy(LeaderChangePolicy::REROUTE_TO_NEW_LEADER)
    .build();

// Routine status broadcast - fire and forget
let status_broadcast = CommandBuilder::routine_broadcast()
    .with_buffer_policy(BufferPolicy::DROP_ON_PARTITION)
    .with_ack_policy(AcknowledgmentPolicy::NO_ACK_REQUIRED)
    .build();
```

### Architecture Overview

**Three Layers of Abstraction**:

1. **Schema Layer** (`command.proto`): Defines policy enumerations and command structure
2. **Protocol Layer** (`cap-protocol`): Implements policy enforcement and routing logic
3. **Integration Layer** (integrator code): Selects policies based on operational requirements

**This separation enables**:
- Schema evolution without protocol changes
- Protocol testing with different policy combinations
- Integrator customization without forking the codebase

### Current Status

**Implemented**: Upward flow (status aggregation) ✅
**Designed**: Downward flow (command dissemination) with policy-based flexibility ✅
**Not Yet Built**: Implementation of downward routing and policy enforcement ⏳

**Next Step**: Review design with team, validate against operational requirements, then implement after Mode 3/4 validation completes.

---

## Table of Contents

1. [Executive Summary](#executive-summary) - Design philosophy and key benefits
2. [Problem Statement](#problem-statement) - Current limitations and requirements
3. [Use Cases](#use-cases) - Operational scenarios for downward flow
4. [Architecture](#architecture) - Schema design and routing infrastructure
   - [Schema Design (`command.proto`)](#schema-design-new-commandproto)
   - [Routing Architecture](#routing-architecture)
   - [Storage Integration](#storage-integration)
5. [Policy-Based Flexibility](#policy-based-flexibility) - Configurable behavior control
   - [Partition Handling Policy](#1-partition-handling-policy)
   - [Conflict Resolution Policy](#2-conflict-resolution-policy)
   - [Acknowledgment Policy](#3-acknowledgment-policy)
   - [Leader Change Policy](#4-leader-change-policy)
   - [Best Practice Presets](#best-practice-policy-presets)
   - [Integration Examples](#integration-example)
   - [Runtime Enforcement](#policy-enforcement-at-runtime)
6. [Implementation Plan](#implementation-plan) - 9-week phased approach
7. [Success Criteria](#success-criteria) - Functional, performance, reliability metrics
8. [Summary](#summary-policy-based-flexibility) - Design advantages and next steps

---

## Problem Statement

**Current Limitation**: CAP Protocol's hierarchical aggregation only supports **upward flow** (bottom-up state summarization for situational awareness).

**Missing Capability**: **Downward flow** (top-down command dissemination for command & control).

### Current Flow (Upward Only)

```
Company Commander
    ↑ (PlatoonSummaries)
Platoon Leaders
    ↑ (SquadSummaries)
Squad Leaders
    ↑ (NodeStates)
Squad Members
```

### Required Flow (Bidirectional)

```
Company Commander
    ↑ Status          ↓ Commands
Platoon Leaders
    ↑ Status          ↓ Commands
Squad Leaders
    ↑ Status          ↓ Commands
Squad Members
```

## Use Cases

### UC-1: Mission Order Dissemination
**Scenario**: Company commander issues new mission to Platoon 2

**Flow**:
1. Company commander creates MissionOrder message
2. Message routed to Platoon 2 leader
3. Platoon leader acknowledges receipt
4. Platoon leader disseminates to Squad 2-1, 2-2, 2-3 leaders
5. Squad leaders acknowledge and disseminate to members

**Requirements**:
- Hierarchical routing (company → platoon → squad → member)
- Acknowledgment at each level
- Persistence (survives network partitions)
- Priority (preempts non-critical traffic)

### UC-2: Priority Target Engagement
**Scenario**: Platoon leader identifies high-value target, assigns to Squad 1

**Flow**:
1. Platoon leader creates EngagementOrder for Squad 1
2. Message routed to Squad 1 leader only (not other squads)
3. Squad leader acknowledges
4. Squad leader disseminates to capable squad members (e.g., sensor platforms)

**Requirements**:
- Selective routing (only to specific squads)
- Capability-based distribution (only to members with required capabilities)
- Time-sensitive delivery (low latency)

### UC-3: Formation Change
**Scenario**: Company commander orders all platoons to change formation

**Flow**:
1. Company commander creates FormationChange broadcast
2. Message sent to all platoon leaders
3. Each platoon leader acknowledges
4. Each platoon leader broadcasts to squad leaders
5. Squad leaders acknowledge and execute

**Requirements**:
- Broadcast to all subordinates at a level
- Coordinated execution (all units change together)
- Rollback on failure (if any unit can't comply)

## Architecture

### Schema Design (New: `command.proto`)

```protobuf
syntax = "proto3";

package cap.command.v1;

import "common.proto";

// Hierarchical command message
//
// Supports downward dissemination of commands, orders, and directives
// from higher echelons to lower echelons.
message HierarchicalCommand {
  // Unique command ID
  string command_id = 1;

  // Originating node ID (who issued the command)
  string originator_id = 2;

  // Target specification (who should receive this)
  CommandTarget target = 3;

  // Command type and payload
  oneof command_type {
    MissionOrder mission_order = 10;
    EngagementOrder engagement_order = 11;
    FormationChange formation_change = 12;
    PriorityUpdate priority_update = 13;
    CapabilityRequest capability_request = 14;
  }

  // Priority level (affects routing and delivery)
  CommandPriority priority = 20;

  // Timestamp when command was issued
  common.v1.Timestamp issued_at = 21;

  // Expiry time (command invalid after this time)
  common.v1.Timestamp expires_at = 22;

  // Acknowledgment required (deprecated - use acknowledgment_policy)
  bool requires_ack = 23 [deprecated = true];

  // Sequence number (for ordering)
  uint64 sequence = 24;

  // Policy Configurations (flexible behavior control)

  // How to handle network partitions
  BufferPolicy buffer_policy = 30;

  // How to resolve conflicting commands
  ConflictPolicy conflict_policy = 31;

  // How acknowledgments should be handled
  AcknowledgmentPolicy acknowledgment_policy = 32;

  // What to do if leader changes during execution
  LeaderChangePolicy leader_change_policy = 33;
}

// Command target specification
message CommandTarget {
  // Target scope
  oneof scope {
    // Specific node (direct addressing)
    string node_id = 1;

    // Specific squad
    string squad_id = 2;

    // Specific platoon
    string platoon_id = 3;

    // Broadcast to all subordinates at a level
    BroadcastTarget broadcast = 4;
  }

  // Optional: Filter by capability
  // Only nodes with these capabilities should process the command
  repeated string required_capabilities = 10;

  // Optional: Exclude specific nodes
  repeated string excluded_node_ids = 11;
}

// Broadcast target specification
message BroadcastTarget {
  // Echelon level to broadcast to
  enum Echelon {
    ECHELON_UNSPECIFIED = 0;
    ECHELON_SQUAD = 1;       // All squads in platoon
    ECHELON_PLATOON = 2;     // All platoons in company
    ECHELON_COMPANY = 3;     // All companies in battalion
  }

  Echelon target_echelon = 1;

  // Optional: Only units in this zone
  string zone_id = 2;
}

// Mission order (new objectives/tasks)
message MissionOrder {
  // Mission ID
  string mission_id = 1;

  // Objective description
  string objective = 2;

  // Target area of operations
  common.v1.Position target_position = 3;
  float radius_m = 4;

  // Execution time
  common.v1.Timestamp execute_at = 5;

  // Success criteria
  string success_criteria = 6;
}

// Engagement order (target-specific)
message EngagementOrder {
  // Target ID
  string target_id = 1;

  // Target position
  common.v1.Position target_position = 2;

  // Engagement type
  enum EngagementType {
    ENGAGEMENT_UNSPECIFIED = 0;
    ENGAGEMENT_OBSERVE = 1;      // ISR only
    ENGAGEMENT_TRACK = 2;         // Continuous tracking
    ENGAGEMENT_DESIGNATE = 3;     // Laser designation
    ENGAGEMENT_STRIKE = 4;        // Kinetic engagement
  }

  EngagementType engagement_type = 3;

  // Priority
  uint32 priority = 4;
}

// Formation change directive
message FormationChange {
  // New formation type
  enum FormationType {
    FORMATION_UNSPECIFIED = 0;
    FORMATION_COLUMN = 1;
    FORMATION_LINE = 2;
    FORMATION_WEDGE = 3;
    FORMATION_DIAMOND = 4;
    FORMATION_DISPERSED = 5;
  }

  FormationType formation_type = 1;

  // Spacing between units (meters)
  float spacing_m = 2;

  // Orientation (degrees from north)
  float orientation_deg = 3;
}

// Priority update (change mission priorities)
message PriorityUpdate {
  // New priority assignments
  message PriorityAssignment {
    string mission_id = 1;
    uint32 priority = 2;
  }

  repeated PriorityAssignment priorities = 1;
}

// Capability request (request specific capabilities)
message CapabilityRequest {
  // Required capability types
  repeated string capability_types = 1;

  // Number of platforms needed
  uint32 required_count = 2;

  // Duration needed (minutes)
  uint32 duration_minutes = 3;

  // Area of operations
  common.v1.Position center = 4;
  float radius_m = 5;
}

// Command priority
enum CommandPriority {
  PRIORITY_UNSPECIFIED = 0;
  PRIORITY_ROUTINE = 1;      // Normal traffic
  PRIORITY_PRIORITY = 2;     // Important, but not urgent
  PRIORITY_IMMEDIATE = 3;    // Time-sensitive
  PRIORITY_FLASH = 4;        // Emergency, preempt all other traffic
}

// Policy Enumerations (Flexible Behavior Control)

// Buffer policy: How to handle network partitions
enum BufferPolicy {
  BUFFER_POLICY_UNSPECIFIED = 0;
  BUFFER_AND_RETRY = 1;          // Buffer during partition, deliver when reconnected
  DROP_ON_PARTITION = 2;         // Drop command if target unreachable
  REQUIRE_IMMEDIATE_DELIVERY = 3; // Fail if cannot deliver immediately
}

// Conflict policy: How to handle conflicting commands
enum ConflictPolicy {
  CONFLICT_POLICY_UNSPECIFIED = 0;
  LAST_WRITER_WINS = 1;          // Most recent command wins (LWW)
  PRIORITY_WINS = 2;             // Higher priority command wins
  EXPLICIT_SUPERSEDE = 3;        // Only supersede if explicitly marked
  MERGE_COMPATIBLE = 4;          // Merge if commands are compatible
}

// Acknowledgment policy: How acknowledgments should be handled
enum AcknowledgmentPolicy {
  ACK_POLICY_UNSPECIFIED = 0;
  AUTO_ACK_ON_DELIVERY = 1;      // Automatic ACK_RECEIVED on delivery
  REQUIRE_EXPLICIT_ACK = 2;      // Handler must explicitly acknowledge
  NO_ACK_REQUIRED = 3;           // Fire-and-forget (best effort)
  ACK_ON_EXECUTION = 4;          // Only ACK after execution completes
}

// Leader change policy: What to do if leader changes during execution
enum LeaderChangePolicy {
  LEADER_CHANGE_POLICY_UNSPECIFIED = 0;
  REROUTE_TO_NEW_LEADER = 1;     // Send command to new leader
  ABORT_ON_LEADER_CHANGE = 2;    // Abort execution if leader changes
  CONTINUE_WITH_ORIGINAL = 3;    // Original leader continues execution
  NOTIFY_AND_CONFIRM = 4;        // Notify originator, wait for confirmation
}

// Command acknowledgment
message CommandAck {
  // Command being acknowledged
  string command_id = 1;

  // Node acknowledging
  string node_id = 2;

  // Acknowledgment status
  enum AckStatus {
    ACK_STATUS_UNSPECIFIED = 0;
    ACK_RECEIVED = 1;          // Command received
    ACK_ACCEPTED = 2;          // Command accepted, will execute
    ACK_REJECTED = 3;          // Command rejected, cannot execute
    ACK_COMPLETED = 4;         // Execution completed successfully
    ACK_FAILED = 5;            // Execution attempted but failed
  }

  AckStatus status = 3;

  // Optional: Reason for rejection/failure
  string reason = 4;

  // Timestamp of acknowledgment
  common.v1.Timestamp acked_at = 5;
}

// Command status tracking
message CommandStatus {
  // Command ID
  string command_id = 1;

  // Current status
  enum Status {
    STATUS_UNSPECIFIED = 0;
    STATUS_PENDING = 1;        // Not yet delivered
    STATUS_DELIVERED = 2;      // Delivered to target(s)
    STATUS_ACKNOWLEDGED = 3;   // Acknowledged by target(s)
    STATUS_IN_PROGRESS = 4;    // Execution started
    STATUS_COMPLETED = 5;      // Execution completed
    STATUS_FAILED = 6;         // Execution failed
    STATUS_EXPIRED = 7;        // Command expired before execution
    STATUS_SUPERSEDED = 8;     // Replaced by newer command
  }

  Status status = 2;

  // Acknowledgments received
  repeated CommandAck acknowledgments = 3;

  // Last update timestamp
  common.v1.Timestamp updated_at = 4;
}
```

### Routing Architecture

#### Downward Routing Table

Each node maintains a routing table for downward dissemination:

```rust
pub struct DownwardRoutingTable {
    // My immediate subordinates (direct reports)
    subordinates: HashMap<String, NodeInfo>,

    // Squads I command (if I'm a platoon leader)
    commanded_squads: HashMap<String, SquadInfo>,

    // Platoons I command (if I'm a company commander)
    commanded_platoons: HashMap<String, PlatoonInfo>,

    // My position in hierarchy
    my_echelon: Echelon,
    my_unit_id: String,
}

pub enum Echelon {
    Member,      // Squad member
    SquadLeader, // Squad leader
    PlatoonLeader, // Platoon leader
    CompanyCommander, // Company commander
}
```

#### Routing Logic

```rust
impl HierarchicalRouter {
    /// Route command downward through hierarchy
    pub async fn route_command_down(
        &self,
        command: HierarchicalCommand,
    ) -> Result<Vec<String>> {
        match &command.target.scope {
            Some(Scope::NodeId(target_id)) => {
                // Direct delivery to specific node
                self.route_to_node(command, target_id).await
            }
            Some(Scope::SquadId(squad_id)) => {
                // Route to squad leader, who disseminates to members
                self.route_to_squad(command, squad_id).await
            }
            Some(Scope::PlatoonId(platoon_id)) => {
                // Route to platoon leader, who disseminates to squads
                self.route_to_platoon(command, platoon_id).await
            }
            Some(Scope::Broadcast(broadcast)) => {
                // Broadcast to all subordinates at specified echelon
                self.broadcast_command(command, broadcast).await
            }
            None => {
                Err(Error::InvalidCommand("No target specified".into()))
            }
        }
    }

    /// Route to specific squad
    async fn route_to_squad(
        &self,
        command: HierarchicalCommand,
        squad_id: &str,
    ) -> Result<Vec<String>> {
        // Get squad leader from routing table
        let squad_leader = self.routing_table
            .get_squad_leader(squad_id)
            .ok_or(Error::SquadNotFound(squad_id.to_string()))?;

        // Send to squad leader
        self.send_command(squad_leader, command.clone()).await?;

        // Squad leader will disseminate to members
        Ok(vec![squad_leader.to_string()])
    }

    /// Broadcast to all subordinates
    async fn broadcast_command(
        &self,
        command: HierarchicalCommand,
        broadcast: &BroadcastTarget,
    ) -> Result<Vec<String>> {
        let targets = match broadcast.target_echelon {
            Echelon::Squad if self.is_platoon_leader() => {
                // I'm platoon leader, broadcast to all my squad leaders
                self.routing_table.get_squad_leaders()
            }
            Echelon::Platoon if self.is_company_commander() => {
                // I'm company commander, broadcast to all my platoon leaders
                self.routing_table.get_platoon_leaders()
            }
            _ => return Err(Error::InvalidBroadcast("Invalid echelon for my level".into())),
        };

        // Send to all targets
        let mut sent_to = Vec::new();
        for target in targets {
            self.send_command(&target, command.clone()).await?;
            sent_to.push(target);
        }

        Ok(sent_to)
    }
}
```

### Integration with Existing Router

The existing `Router` (in `cap-protocol/src/hierarchy/router.rs`) handles **upward routing** (squad → platoon → company).

We need to extend it with **downward routing**:

```rust
impl Router {
    // Existing: Upward routing
    pub fn route_message_upward(&self, msg: Message) -> Result<Option<TargetNode>> {
        // Routes NodeState → SquadSummary → PlatoonSummary
        // ... existing logic ...
    }

    // NEW: Downward routing
    pub fn route_command_downward(&self, cmd: HierarchicalCommand) -> Result<Vec<String>> {
        // Routes commands down the hierarchy
        match self.my_role {
            Role::CompanyCommander => self.route_to_platoons(cmd),
            Role::PlatoonLeader => self.route_to_squads(cmd),
            Role::SquadLeader => self.route_to_members(cmd),
            Role::Member => Err(Error::InvalidRoute("Members cannot route downward")),
        }
    }
}
```

### Storage Integration

Commands need to be persisted for:
1. **Reliability** - survive network partitions
2. **Acknowledgment tracking** - monitor execution status
3. **Audit trail** - compliance and after-action review

**DittoStore Extension**:
```rust
impl DittoStore {
    /// Store a hierarchical command
    pub async fn upsert_command(
        &self,
        command_id: &str,
        command: &HierarchicalCommand,
    ) -> Result<String> {
        let collection = self.ditto().store().collection("commands")?;

        // Encode as JSON (similar to hierarchical summaries)
        let json = serde_json::to_value(command)?;
        let mut doc = collection.find_by_id(DocumentId::new(command_id)?).exec().await?;

        if doc.is_none() {
            doc = Some(collection.new_document()?);
        }

        let mut doc = doc.unwrap();
        doc.set("command_id", command_id)?;
        doc.set("originator_id", &command.originator_id)?;
        doc.set("priority", command.priority as i32)?;
        doc.set("issued_at", command.issued_at)?;
        doc.set("data", json.to_string())?;

        collection.upsert(doc).await?;

        Ok(command_id.to_string())
    }

    /// Get a command by ID
    pub async fn get_command(
        &self,
        command_id: &str,
    ) -> Result<Option<HierarchicalCommand>> {
        let collection = self.ditto().store().collection("commands")?;
        let doc = collection.find_by_id(DocumentId::new(command_id)?).exec().await?;

        match doc {
            Some(doc) => {
                let json_str: String = doc.get("data")?;
                let command: HierarchicalCommand = serde_json::from_str(&json_str)?;
                Ok(Some(command))
            }
            None => Ok(None),
        }
    }

    /// Track command acknowledgment
    pub async fn record_command_ack(
        &self,
        ack: &CommandAck,
    ) -> Result<()> {
        let collection = self.ditto().store().collection("command_acks")?;

        let doc_id = format!("{}:{}", ack.command_id, ack.node_id);
        let mut doc = collection.new_document()?;

        doc.set("command_id", &ack.command_id)?;
        doc.set("node_id", &ack.node_id)?;
        doc.set("status", ack.status as i32)?;
        doc.set("reason", ack.reason.as_deref().unwrap_or(""))?;
        doc.set("acked_at", ack.acked_at)?;

        collection.upsert(doc).await?;

        Ok(())
    }
}
```

## Implementation Plan

### Phase 1: Schema and Core Types (1 week)

1. **Create `command.proto`** with hierarchical command messages
2. **Generate Rust bindings** via prost
3. **Add to `cap-schema` crate**
4. **Update documentation**

### Phase 2: Routing Infrastructure (2 weeks)

1. **Extend `DownwardRoutingTable`** with subordinate tracking
2. **Implement `route_command_down()` in Router**
3. **Add broadcast routing** for all-subordinates dissemination
4. **Add capability-based filtering** (only send to capable nodes)

### Phase 3: Storage Integration (1 week)

1. **Add `commands` collection** to DittoStore
2. **Implement `upsert_command()` and `get_command()`**
3. **Add `command_acks` collection** for acknowledgment tracking
4. **Add TTL support** for expired commands (auto-cleanup)

### Phase 4: Acknowledgment & Tracking (2 weeks)

1. **Implement acknowledgment protocol**
   - Automatic ACK_RECEIVED on delivery
   - Explicit ACK_ACCEPTED / ACK_REJECTED from handler
   - ACK_COMPLETED / ACK_FAILED on execution result
2. **Add command status tracking** (pending → delivered → completed)
3. **Add timeout handling** (resend if no ACK received)
4. **Add command supersession** (newer command replaces older)

### Phase 5: Message Bus Integration (1 week)

1. **Extend MessageBus** to handle HierarchicalCommand
2. **Add command handlers** (subscribe to specific command types)
3. **Priority-based delivery** (FLASH commands jump queue)
4. **Flow control** (backpressure if subordinate overloaded)

### Phase 6: Testing (2 weeks)

1. **Unit tests** for routing logic
2. **Integration tests** for storage
3. **E2E tests** for full flow:
   - Company → Platoon → Squad → Member
   - Acknowledgment propagation
   - Network partition recovery
4. **Performance tests** (command delivery latency)

**Total**: 9 weeks

## Success Criteria

### Functional

- [ ] Commands route correctly down hierarchy (company → platoon → squad → member)
- [ ] Broadcast reaches all subordinates at target echelon
- [ ] Capability-based filtering works (only capable nodes receive)
- [ ] Acknowledgments propagate upward
- [ ] Commands persist across network partitions
- [ ] Expired commands auto-cleanup

### Performance

- [ ] Command delivery < 500ms for single-hop (squad leader → member)
- [ ] Command delivery < 2s for multi-hop (company → squad member)
- [ ] Broadcast to 24 nodes completes < 3s
- [ ] Acknowledgment round-trip < 1s
- [ ] 100 commands/sec sustained throughput

### Reliability

- [ ] 99.9% delivery success rate on stable network
- [ ] Automatic retry on failed delivery
- [ ] Commands survive 30-second network partition
- [ ] No duplicate command execution (idempotency)

## Policy-Based Flexibility

CAP Protocol provides **flexible policies** rather than hard-coded behaviors, allowing integrators to configure the protocol based on their specific requirements.

### Configurable Policies (via Schema Enumerations)

All policy decisions are encoded in the command message itself, giving integrators full control:

#### 1. Partition Handling Policy

**Question**: Should squad leaders buffer commands during network partition?

**Answer**: Configurable via `BufferPolicy` enum in command:

```protobuf
enum BufferPolicy {
  BUFFER_POLICY_UNSPECIFIED = 0;
  BUFFER_AND_RETRY = 1;          // Buffer during partition, deliver when reconnected
  DROP_ON_PARTITION = 2;         // Drop command if target unreachable
  REQUIRE_IMMEDIATE_DELIVERY = 3; // Fail if cannot deliver immediately
}
```

**Use Cases**:
- Mission-critical orders → `BUFFER_AND_RETRY` (ensure delivery)
- Time-sensitive updates → `DROP_ON_PARTITION` (stale data is worse than no data)
- Real-time coordination → `REQUIRE_IMMEDIATE_DELIVERY` (abort if delayed)

#### 2. Conflict Resolution Policy

**Question**: How to handle conflicting commands?

**Answer**: Configurable via `ConflictPolicy` enum in command:

```protobuf
enum ConflictPolicy {
  CONFLICT_POLICY_UNSPECIFIED = 0;
  LAST_WRITER_WINS = 1;          // Most recent command wins (LWW)
  PRIORITY_WINS = 2;             // Higher priority command wins
  EXPLICIT_SUPERSEDE = 3;        // Only supersede if explicitly marked
  MERGE_COMPATIBLE = 4;          // Merge if commands are compatible
}
```

**Use Cases**:
- Rapid tactical changes → `LAST_WRITER_WINS` (latest intelligence wins)
- Emergency situations → `PRIORITY_WINS` (urgent commands override routine)
- Coordinated operations → `EXPLICIT_SUPERSEDE` (require explicit revocation)
- Parallel missions → `MERGE_COMPATIBLE` (execute both if non-conflicting)

#### 3. Acknowledgment Policy

**Question**: Should members auto-ack or require explicit acknowledgment?

**Answer**: Configurable via `AcknowledgmentPolicy` enum in command:

```protobuf
enum AcknowledgmentPolicy {
  ACK_POLICY_UNSPECIFIED = 0;
  AUTO_ACK_ON_DELIVERY = 1;      // Automatic ACK_RECEIVED on delivery
  REQUIRE_EXPLICIT_ACK = 2;      // Handler must explicitly acknowledge
  NO_ACK_REQUIRED = 3;           // Fire-and-forget (best effort)
  ACK_ON_EXECUTION = 4;          // Only ACK after execution completes
}
```

**Use Cases**:
- Status broadcasts → `AUTO_ACK_ON_DELIVERY` (receipt confirmation)
- Critical orders → `REQUIRE_EXPLICIT_ACK` (human/handler confirms understanding)
- Sensor updates → `NO_ACK_REQUIRED` (continuous stream, no confirmation needed)
- Mission execution → `ACK_ON_EXECUTION` (confirm completion, not just receipt)

#### 4. Leader Change Policy

**Question**: What happens if leader changes during command execution?

**Answer**: Configurable via `LeaderChangePolicy` enum in command:

```protobuf
enum LeaderChangePolicy {
  LEADER_CHANGE_POLICY_UNSPECIFIED = 0;
  REROUTE_TO_NEW_LEADER = 1;     // Send command to new leader
  ABORT_ON_LEADER_CHANGE = 2;    // Abort execution if leader changes
  CONTINUE_WITH_ORIGINAL = 3;    // Original leader continues execution
  NOTIFY_AND_CONFIRM = 4;        // Notify originator, wait for confirmation
}
```

**Use Cases**:
- Long-running missions → `REROUTE_TO_NEW_LEADER` (mission continues)
- Leader-specific orders → `CONTINUE_WITH_ORIGINAL` (order was for that specific leader)
- Critical coordination → `ABORT_ON_LEADER_CHANGE` (safety: don't execute with new leader)
- Approval-required → `NOTIFY_AND_CONFIRM` (get new approval from command)

### Best Practice Policy Presets

CAP Protocol can provide **recommended policy combinations** for common scenarios:

#### Preset 1: Mission-Critical Order
```rust
HierarchicalCommand {
    buffer_policy: BUFFER_AND_RETRY,
    conflict_policy: EXPLICIT_SUPERSEDE,
    acknowledgment_policy: REQUIRE_EXPLICIT_ACK,
    leader_change_policy: NOTIFY_AND_CONFIRM,
    priority: PRIORITY_FLASH,
    // ...
}
```
**Use Case**: Strike authorization, critical mission orders
**Rationale**: Maximum reliability, explicit control at every step

#### Preset 2: Tactical Update (Time-Sensitive)
```rust
HierarchicalCommand {
    buffer_policy: DROP_ON_PARTITION,
    conflict_policy: LAST_WRITER_WINS,
    acknowledgment_policy: AUTO_ACK_ON_DELIVERY,
    leader_change_policy: REROUTE_TO_NEW_LEADER,
    priority: PRIORITY_IMMEDIATE,
    // ...
}
```
**Use Case**: Enemy position updates, tactical intelligence
**Rationale**: Stale data is worse than no data, latest wins

#### Preset 3: Routine Status Broadcast
```rust
HierarchicalCommand {
    buffer_policy: DROP_ON_PARTITION,
    conflict_policy: LAST_WRITER_WINS,
    acknowledgment_policy: NO_ACK_REQUIRED,
    leader_change_policy: REROUTE_TO_NEW_LEADER,
    priority: PRIORITY_ROUTINE,
    // ...
}
```
**Use Case**: Periodic status updates, non-critical broadcasts
**Rationale**: Fire-and-forget, reduce overhead

#### Preset 4: Coordinated Formation Change
```rust
HierarchicalCommand {
    buffer_policy: REQUIRE_IMMEDIATE_DELIVERY,
    conflict_policy: EXPLICIT_SUPERSEDE,
    acknowledgment_policy: ACK_ON_EXECUTION,
    leader_change_policy: ABORT_ON_LEADER_CHANGE,
    priority: PRIORITY_PRIORITY,
    // ...
}
```
**Use Case**: Formation changes requiring synchronized execution
**Rationale**: All units must execute together, abort if coordination broken

### Integration Example

Integrators can create helper functions that encapsulate these policy decisions:

```rust
// lib.rs - Integrator's command builder

use cap_protocol::command::{
    HierarchicalCommand, BufferPolicy, ConflictPolicy,
    AcknowledgmentPolicy, LeaderChangePolicy, CommandPriority
};

pub struct CommandBuilder {
    command: HierarchicalCommand,
}

impl CommandBuilder {
    /// Create mission-critical order with maximum reliability
    pub fn mission_critical() -> Self {
        let mut command = HierarchicalCommand::default();
        command.buffer_policy = BufferPolicy::BufferAndRetry as i32;
        command.conflict_policy = ConflictPolicy::ExplicitSupersede as i32;
        command.acknowledgment_policy = AcknowledgmentPolicy::RequireExplicitAck as i32;
        command.leader_change_policy = LeaderChangePolicy::NotifyAndConfirm as i32;
        command.priority = CommandPriority::Flash as i32;

        Self { command }
    }

    /// Create time-sensitive tactical update
    pub fn tactical_update() -> Self {
        let mut command = HierarchicalCommand::default();
        command.buffer_policy = BufferPolicy::DropOnPartition as i32;
        command.conflict_policy = ConflictPolicy::LastWriterWins as i32;
        command.acknowledgment_policy = AcknowledgmentPolicy::AutoAckOnDelivery as i32;
        command.leader_change_policy = LeaderChangePolicy::RerouteToNewLeader as i32;
        command.priority = CommandPriority::Immediate as i32;

        Self { command }
    }

    /// Create routine status broadcast
    pub fn routine_broadcast() -> Self {
        let mut command = HierarchicalCommand::default();
        command.buffer_policy = BufferPolicy::DropOnPartition as i32;
        command.conflict_policy = ConflictPolicy::LastWriterWins as i32;
        command.acknowledgment_policy = AcknowledgmentPolicy::NoAckRequired as i32;
        command.leader_change_policy = LeaderChangePolicy::RerouteToNewLeader as i32;
        command.priority = CommandPriority::Routine as i32;

        Self { command }
    }

    /// Custom policies for specific requirements
    pub fn custom() -> Self {
        Self {
            command: HierarchicalCommand::default(),
        }
    }

    pub fn with_buffer_policy(mut self, policy: BufferPolicy) -> Self {
        self.command.buffer_policy = policy as i32;
        self
    }

    pub fn with_conflict_policy(mut self, policy: ConflictPolicy) -> Self {
        self.command.conflict_policy = policy as i32;
        self
    }

    pub fn build(self) -> HierarchicalCommand {
        self.command
    }
}

// Usage:
let strike_order = CommandBuilder::mission_critical()
    .with_target(platoon_2)
    .with_engagement_order(target_x)
    .build();

let intel_update = CommandBuilder::tactical_update()
    .with_target(all_squads)
    .with_mission_update(new_objectives)
    .build();
```

### Policy Enforcement at Runtime

CAP Protocol implementation will enforce these policies:

```rust
// cap-protocol/src/hierarchy/command_router.rs

impl CommandRouter {
    async fn route_command(&self, cmd: HierarchicalCommand) -> Result<()> {
        // Check buffer policy
        match BufferPolicy::from_i32(cmd.buffer_policy) {
            Some(BufferPolicy::RequireImmediateDelivery) => {
                // Fail immediately if target unreachable
                if !self.is_target_reachable(&cmd.target).await? {
                    return Err(Error::TargetUnreachable);
                }
            }
            Some(BufferPolicy::BufferAndRetry) => {
                // Buffer command for later delivery
                if !self.is_target_reachable(&cmd.target).await? {
                    self.buffer_command(cmd).await?;
                    return Ok(());
                }
            }
            Some(BufferPolicy::DropOnPartition) => {
                // Drop if unreachable
                if !self.is_target_reachable(&cmd.target).await? {
                    return Ok(()); // Silent drop
                }
            }
            _ => { /* Default behavior */ }
        }

        // Check conflict policy
        if let Some(existing) = self.find_conflicting_command(&cmd).await? {
            match ConflictPolicy::from_i32(cmd.conflict_policy) {
                Some(ConflictPolicy::PriorityWins) => {
                    if cmd.priority > existing.priority {
                        self.supersede_command(&existing, &cmd).await?;
                    } else {
                        return Err(Error::ConflictingCommand);
                    }
                }
                Some(ConflictPolicy::ExplicitSupersede) => {
                    return Err(Error::ConflictingCommand);
                }
                Some(ConflictPolicy::LastWriterWins) => {
                    self.supersede_command(&existing, &cmd).await?;
                }
                _ => { /* Default behavior */ }
            }
        }

        // Route command based on acknowledgment policy
        match AcknowledgmentPolicy::from_i32(cmd.acknowledgment_policy) {
            Some(AcknowledgmentPolicy::NoAckRequired) => {
                self.send_command_best_effort(&cmd).await?;
            }
            Some(AcknowledgmentPolicy::RequireExplicitAck) => {
                self.send_command_with_ack_required(&cmd).await?;
            }
            _ => { /* Default behavior */ }
        }

        Ok(())
    }
}
```

## Summary: Policy-Based Flexibility

This design provides **maximum flexibility** while maintaining **clear semantics**:

1. **Schema defines the vocabulary** - Policy enums in `command.proto`
2. **Protocol implements the semantics** - cap-protocol enforces policies
3. **Integrators configure behavior** - Choose policies per command
4. **Best practices as presets** - Recommended combinations for common scenarios

**Key Advantages**:
- No hard-coded assumptions about operational requirements
- Different commands can use different policies
- Integrators can adapt to their specific tactical scenarios
- Easy to add new policies without breaking existing code
- Clear documentation of behavioral expectations

**This is exactly the kind of flexibility cap-schema and cap-protocol should provide.**

## References

- `cap-protocol/src/hierarchy/router.rs` - Current upward routing
- `cap-protocol/src/hierarchy/routing_table.rs` - Routing table implementation
- `cap-protocol/src/cell/messaging.rs` - Message bus and priority handling
- `cap-schema/proto/hierarchy.proto` - Current hierarchical summaries

---

**Status**: Design Complete - Policy-Based Flexible Architecture
**Date**: 2025-11-08
**Updated**: 2025-11-08 (Added policy-based flexibility)
**Next Steps**:
1. Review with team
2. Validate policy presets against operational requirements
3. Begin implementation after Mode 3/4 validation completes
