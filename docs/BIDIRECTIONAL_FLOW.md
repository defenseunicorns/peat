# Bidirectional Hierarchical Flow - User Guide

**Status**: ✅ Ready for Experiments Team Validation
**PR**: #64
**Branch**: `feature/bidirectional-hierarchy`

## Overview

The CAP Protocol now supports **full-duplex bidirectional hierarchical flow**, enabling both downward command dissemination and upward status aggregation through the same hierarchy.

```text
┌────────────────────────────────────────────────┐
│  Zone Leader                                   │
│  - Issues commands ↓                           │
│  - Receives acknowledgments ↑                  │
└──────────────────┬─────────────────────────────┘
                   │
        ┌──────────┴──────────┐
        ▼                     ▼
  ┌───────────┐         ┌───────────┐
  │  Squad 1  │         │  Squad 2  │
  │  Leader   │         │  Leader   │
  │  ↓     ↑  │         │  ↓     ↑  │
  └─────┬─────┘         └─────┬─────┘
        │                     │
   ┌────┴────┐           ┌────┴────┐
   ▼         ▼           ▼         ▼
 Node-1   Node-2      Node-3   Node-4
 (Execute Commands, Send Acks)
```

## Features

### Downward Flow: Command Dissemination

- **Policy-based routing**: Commands carry routing, buffering, conflict resolution, and acknowledgment policies
- **Hierarchical propagation**: Commands flow from zone → squad → individual nodes
- **Target scoping**: Individual, Squad, Platoon, or Broadcast targeting
- **CRDT-based sync**: Commands propagate via Ditto mesh (eventual consistency)

### Upward Flow: Acknowledgment Aggregation

- **Acknowledgment tracking**: Nodes send acks back through hierarchy
- **Status reporting**: Track RECEIVED, COMPLETED, or FAILED states
- **Independent tracking**: Each command has separate ack collection
- **CRDT-based sync**: Acknowledgments propagate via Ditto mesh

## Architecture

### Components

1. **CommandRouter** (`cap-protocol/src/command/routing.rs`)
   - Target resolution (Individual, Squad, Platoon, Broadcast)
   - Determines if node should execute or route command
   - Returns subordinate targets for routing

2. **CommandCoordinator** (`cap-protocol/src/command/coordinator.rs`)
   - Command lifecycle management (issue, receive, execute)
   - Acknowledgment generation and tracking
   - Status monitoring

3. **DittoStore** (`cap-protocol/src/storage/ditto_store.rs`)
   - Command persistence (`upsert_command`, `get_command`)
   - Acknowledgment persistence (`upsert_command_ack`, `query_command_acks`)
   - CRDT sync via Ditto collections

### Ditto Collections

**`hierarchical_commands`** - Stores commands
```json
{
  "_id": "cmd-001",
  "command_id": "cmd-001",
  "originator_id": "leader-node",
  "priority": 3,
  "data": "<base64-encoded-protobuf>",
  "type": "hierarchical_command"
}
```

**`command_acknowledgments`** - Stores acknowledgments
```json
{
  "_id": "cmd-001-node-1",
  "command_id": "cmd-001",
  "node_id": "node-1",
  "status": 2,
  "data": "<base64-encoded-protobuf>",
  "type": "command_acknowledgment"
}
```

## Usage

### Basic Command Flow

```rust
use cap_protocol::command::CommandCoordinator;
use cap_protocol::storage::DittoStore;
use cap_schema::command::v1::{CommandTarget, HierarchicalCommand, command_target::Scope};

// 1. Create coordinator (leader node)
let coordinator = CommandCoordinator::new(
    Some("squad-alpha".to_string()),  // squad_id
    "leader-node".to_string(),         // node_id
    vec!["node-1".to_string(), "node-2".to_string()], // squad_members
);

// 2. Create command
let command = HierarchicalCommand {
    command_id: "cmd-001".to_string(),
    originator_id: "leader-node".to_string(),
    target: Some(CommandTarget {
        scope: Scope::Squad as i32,
        target_ids: vec!["squad-alpha".to_string()],
    }),
    priority: 3,              // IMMEDIATE
    acknowledgment_policy: 4, // BOTH (RECEIVED + COMPLETED)
    buffer_policy: 1,         // BUFFER_AND_RETRY
    conflict_policy: 2,       // HIGHEST_PRIORITY_WINS
    leader_change_policy: 1,  // BUFFER_UNTIL_STABLE
    ..Default::default()
};

// 3. Issue command (stores locally and routes to subordinates)
coordinator.issue_command(command.clone()).await?;

// 4. Persist to Ditto for CRDT sync
store.upsert_command(&command.command_id, &command).await?;
```

### Receiving and Executing Commands

```rust
// Member node receives command (synced via Ditto)
let synced_command = store.get_command("cmd-001").await?;

if let Some(cmd) = synced_command {
    // Coordinator processes command (routes or executes based on target)
    coordinator.receive_command(cmd).await?;

    // If command targets this node, it executes and sends ack automatically
}
```

### Acknowledgment Collection

```rust
// Leader collects acknowledgments from all squad members
let acks = store.query_command_acks("cmd-001").await?;

println!("Received {} acknowledgments", acks.len());

for ack in acks {
    println!(
        "Node {}: status {}",
        ack.node_id,
        ack.status  // 1=RECEIVED, 2=COMPLETED, 3=FAILED
    );
}

// Check if all targets acknowledged
let all_acked = coordinator.is_command_acknowledged("cmd-001").await;
```

## Command Policies

### Acknowledgment Policy

- `NONE` (1): No acknowledgments required
- `RECEIVED_ONLY` (2): Ack when received
- `COMPLETED_ONLY` (3): Ack when completed
- `BOTH` (4): Ack on received AND completed

### Priority Levels

- `ROUTINE` (1): Low priority
- `PRIORITY` (2): Medium priority
- `IMMEDIATE` (3): High priority
- `FLASH` (4): Highest priority
- `FLASH_OVERRIDE` (5): Emergency override

### Conflict Resolution

- `TIMESTAMP_WINS` (1): Most recent command wins
- `HIGHEST_PRIORITY_WINS` (2): Higher priority overrides lower
- `BUFFER_ALL` (3): Queue all conflicting commands

### Buffer Policy

- `DROP_ON_DISCONNECT` (1): Drop buffered commands if disconnected
- `BUFFER_AND_RETRY` (2): Keep buffered commands and retry

## Testing

### Test Coverage

✅ **Unit Tests** (2 tests)
- `test_command_upsert_and_retrieve` - DittoStore command operations
- `test_command_acknowledgment_upsert_and_query` - DittoStore ack operations

✅ **Integration Tests** (7 tests in `command_lifecycle_integration.rs`)
- Command issuance and persistence
- Command reception and execution
- Acknowledgment generation and persistence
- Squad-level routing
- Policy-based behavior
- Non-applicable filtering
- Multiple ack collection

✅ **E2E Tests** (4 tests in `bidirectional_flow_e2e.rs`)
- Command propagation (leader → member)
- Acknowledgment propagation (member → leader)
- Full-duplex flow (squad command + multi-ack)
- Concurrent commands with independent tracking

### Running Tests

```bash
# Unit tests only
cargo test --lib

# Integration tests
cargo test --test command_lifecycle_integration -- --test-threads=1

# E2E tests (requires DITTO_APP_ID and DITTO_SHARED_KEY)
cargo test --test bidirectional_flow_e2e -- --test-threads=1

# All tests
make test
```

## Sync Requirements

**Critical**: For commands and acks to sync between peers via Ditto, you **must** register sync subscriptions:

```rust
// Register subscriptions on ALL peers that will sync commands/acks
let cmd_sub = store.ditto()
    .sync()
    .register_subscription_v2("SELECT * FROM hierarchical_commands")?;

let ack_sub = store.ditto()
    .sync()
    .register_subscription_v2("SELECT * FROM command_acknowledgments")?;
```

Without subscriptions, Ditto will not replicate the data across peers.

## Observer-Based Event Notification

For production use, leverage Ditto observers for event-driven notification instead of polling:

```rust
use tokio::sync::mpsc;

// 1. Register sync subscription (required for sync AND observers)
let ack_sub = store.ditto()
    .sync()
    .register_subscription_v2("SELECT * FROM command_acknowledgments WHERE command_id = 'cmd-001'")?;

// 2. Create channel for observer events
let (tx, mut rx) = mpsc::unbounded_channel();

// 3. Register observer for acknowledgment changes
let observer = store.ditto()
    .store()
    .register_observer_v2(
        "SELECT * FROM command_acknowledgments WHERE command_id = 'cmd-001'",
        move |result| {
            // This closure fires whenever acks change
            let _ = tx.send(());  // Notify waiting code
        }
    )?;

// 4. Wait for ack events (no polling!)
tokio::select! {
    _ = rx.recv() => {
        // Acknowledgment received! Query the latest acks
        let acks = store.query_command_acks("cmd-001").await?;
        println!("Received {} acknowledgments", acks.len());
    }
    _ = tokio::time::sleep(Duration::from_secs(30)) => {
        println!("Timeout waiting for acknowledgments");
    }
}

// Keep observer alive for duration of monitoring
drop(observer);  // Cleanup when done
```

**Benefits of Observer Pattern:**
- Event-driven (no CPU waste from polling)
- Immediate notification when acks arrive
- Deterministic testing (no arbitrary timeouts)
- Follows CAP Protocol testing philosophy

## Example: Squad Mission Command

```rust
use cap_protocol::command::CommandCoordinator;
use cap_schema::command::v1::{CommandTarget, HierarchicalCommand, command_target::Scope};

// Zone leader issues mission command to all squads
let mission_command = HierarchicalCommand {
    command_id: "mission-alpha-001".to_string(),
    originator_id: "zone-leader".to_string(),
    target: Some(CommandTarget {
        scope: Scope::Platoon as i32,
        target_ids: vec!["platoon-1".to_string()],
    }),
    priority: 4,  // FLASH - high priority
    acknowledgment_policy: 4,  // BOTH - track received and completed
    buffer_policy: 2,  // BUFFER_AND_RETRY - don't drop on disconnect
    conflict_policy: 2,  // HIGHEST_PRIORITY_WINS
    ..Default::default()
};

// Leader issues and persists
coordinator.issue_command(mission_command.clone()).await?;
store.upsert_command(&mission_command.command_id, &mission_command).await?;

// Squad leaders receive via Ditto sync and route to members
// Members execute and send acks back through hierarchy

// Leader monitors completion
loop {
    let acks = store.query_command_acks("mission-alpha-001").await?;

    let received = acks.iter().filter(|a| a.status >= 1).count();
    let completed = acks.iter().filter(|a| a.status == 2).count();

    println!("Mission status: {}/{} received, {}/{} completed",
        received, expected_count, completed, expected_count);

    if coordinator.is_command_acknowledged("mission-alpha-001").await {
        println!("Mission complete!");
        break;
    }

    tokio::time::sleep(Duration::from_secs(5)).await;
}
```

## Known Limitations

1. **Manual subscription management**: Users must register subscriptions. Future: Auto-register in DittoStore.

2. **No conflict resolution enforcement**: Conflict policies defined but not enforced. Future: Implement policy engine.

3. **No command buffering**: Buffer policies defined but not enforced. Future: Implement buffer manager.

4. **No timeout handling**: Commands don't timeout. Future: Add TTL and timeout mechanisms.

**Note**: Observer-based notification is now documented (see Observer-Based Event Notification section above). The examples show polling for simplicity, but production code should use observers.

## Next Steps for Experiments Team

1. **Validate test coverage**: Run all tests and verify they pass in your environment
2. **Test real mesh**: Deploy to multiple physical nodes with Ditto sync
3. **Measure latency**: Profile command propagation and ack collection times
4. **Test failure modes**: Network partitions, node failures, leader changes
5. **Provide feedback**: Report issues, edge cases, and feature requests

## Related Documentation

- [ADR-014: Distributed Coordination Primitives](../docs/adr/014-distributed-coordination-primitives.md)
- [ADR-009: Bidirectional Hierarchical Flows](../docs/adr/009-bidirectional-hierarchical-flows.md)
- [Command Module Documentation](../cap-protocol/src/command/mod.rs)
- [E2E Test Documentation](../cap-protocol/tests/bidirectional_flow_e2e.rs)

## Support

For questions or issues:
- Review E2E tests for usage patterns
- Check integration tests for single-node examples
- Review command module documentation
- Open GitHub issue with `bidirectional-flow` label
