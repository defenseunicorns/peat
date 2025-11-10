# Policy Engine & CRDT Backend Integration

**Status**: Implementation Phase
**Related**: [ADR-014](adr/014-distributed-coordination-primitives.md), [POLICY_ENGINE_DESIGN.md](POLICY_ENGINE_DESIGN.md)

## Problem Statement

The CAP Protocol policy engine provides conflict resolution policies (e.g., `HighestPriorityWins`, `LowestAttributeWins`) that operate at the **application semantic layer**. However, CRDT backends like Ditto resolve conflicts at the **CRDT structural layer** using built-in semantics (typically Last-Write-Wins based on timestamps).

This creates a fundamental timing problem: **policy evaluation happens AFTER CRDT merge has already chosen a winner**.

### Concrete Example

```
Node A: Write command { id: "cmd-1", priority: IMMEDIATE (5), timestamp: 100 }
Node B: Write command { id: "cmd-1", priority: ROUTINE (1), timestamp: 101 }  [concurrent write]
        ↓
   Ditto Sync & CRDT Merge
        ↓
   Ditto LWW Resolution: Node B wins (timestamp 101 > 100)
        ↓
   Result after merge: { id: "cmd-1", priority: ROUTINE (1), timestamp: 101 }
        ↓
   Policy Engine sees only the "winner"
        ↓
   ❌ HighestPriorityWins policy cannot be applied - IMMEDIATE command was lost!
```

**Root cause**: Ditto uses Last-Write-Wins (LWW) semantics for register types. The CRDT merge happens **before** the application policy engine sees the conflicting values.

## Architecture: CRDT Layer vs Application Layer

```
┌─────────────────────────────────────────────────────────────┐
│                   Application Layer                          │
│  - Semantic policies (priority, authority, mission context)  │
│  - Policy Engine (Conflictable trait, ResolutionPolicy)      │
│  - Command semantics (IMMEDIATE > ROUTINE)                   │
└─────────────────────────────────────────────────────────────┘
                            ↑
                            │ Policy applies HERE
                            │ (too late!)
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                    CRDT Structural Layer                     │
│  - Structural conflict resolution (LWW, Version Vectors)     │
│  - Ditto Register: timestamp-based LWW                       │
│  - Merge happens HERE (first!)                               │
└─────────────────────────────────────────────────────────────┘
```

**Key insight**: Policies must influence writes **BEFORE** CRDT merge, not after.

## Solution: Optimistic Concurrency Control (OCC)

### Approach

Use **conditional updates** with WHERE clauses to encode policy logic directly into the write operation. This ensures policy checks happen **before** Ditto's CRDT merge.

### Implementation

#### 1. Conditional Update API

```rust
impl DittoStore {
    /// Conditional update that enforces policy at write time
    ///
    /// Returns `Ok(true)` if update succeeded (policy passed)
    /// Returns `Ok(false)` if update failed (existing document wins per policy)
    pub async fn conditional_update_command(
        &self,
        command: &HierarchicalCommand,
        policy: ConflictPolicy,
    ) -> Result<bool> {
        let (where_clause, params) = self.build_policy_where_clause(command, policy)?;

        let query = format!(
            "UPDATE hierarchical_commands
             SET status = :status,
                 data = :data,
                 priority = :priority,
                 issued_at = :issued_at,
                 last_modified = :now
             WHERE _id = :id AND ({})",
            where_clause
        );

        let result = self.ditto
            .store()
            .execute_v2((query, params))
            .await
            .map_err(|e| Error::storage_error(
                format!("Conditional update failed: {}", e),
                "conditional_update_command",
                Some("hierarchical_commands".to_string()),
            ))?;

        // Check if any documents were mutated
        let success = !result.mutated_document_ids().is_empty();

        if !success {
            debug!(
                "Conditional update rejected for command {} - existing command wins per policy {:?}",
                command.command_id, policy
            );
        }

        Ok(success)
    }

    fn build_policy_where_clause(
        &self,
        command: &HierarchicalCommand,
        policy: ConflictPolicy,
    ) -> Result<(String, serde_json::Value)> {
        let mut params = serde_json::json!({
            "id": command.command_id,
            "status": command.status,
            "data": serde_json::to_value(command)?,
            "priority": command.priority,
            "issued_at": command.issued_at.as_ref().map(|t| t.seconds).unwrap_or(0),
            "now": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        });

        let where_clause = match policy {
            ConflictPolicy::HighestPriorityWins => {
                // Only update if new priority is higher OR equal/higher with newer timestamp
                params["new_priority"] = serde_json::json!(command.priority);
                params["new_time"] = serde_json::json!(
                    command.issued_at.as_ref().map(|t| t.seconds).unwrap_or(0)
                );

                "(priority < :new_priority OR (priority = :new_priority AND issued_at < :new_time))"
            },

            ConflictPolicy::HighestAuthorityWins => {
                // Derive authority level from originator_id
                let new_authority = self.derive_authority_level(&command.originator_id);
                params["new_authority"] = serde_json::json!(new_authority);

                // Need to compute existing authority in query (limitation: requires DQL function)
                // For now, use simpler check on originator_id prefix
                if command.originator_id.starts_with("zone-") {
                    "NOT (originator_id LIKE 'zone-%')"  // Only override non-zone commands
                } else if command.originator_id.starts_with("platoon-") || command.originator_id.starts_with("squad-") {
                    "(NOT (originator_id LIKE 'zone-%')) AND (NOT (originator_id LIKE 'platoon-%')) AND (NOT (originator_id LIKE 'squad-%'))"
                } else {
                    "true"  // Node-level authority, always override
                }
            },

            ConflictPolicy::LastWriteWins => {
                // Only update if new timestamp is newer
                params["new_time"] = serde_json::json!(
                    command.issued_at.as_ref().map(|t| t.seconds).unwrap_or(0)
                );
                "issued_at < :new_time"
            },

            ConflictPolicy::MergeCompatible => {
                // TODO: Implement actual compatibility checking
                // For now, allow all updates
                "true"
            },

            ConflictPolicy::RejectConflict => {
                // Never update existing documents
                "false"
            },

            ConflictPolicy::Unspecified => {
                return Err(Error::InvalidInput(
                    "Conflict policy must be specified".to_string()
                ));
            },
        };

        Ok((where_clause.to_string(), params))
    }

    fn derive_authority_level(&self, node_id: &str) -> u32 {
        if node_id.starts_with("zone-") {
            3
        } else if node_id.starts_with("platoon-") || node_id.starts_with("squad-") {
            2
        } else {
            1
        }
    }
}
```

#### 2. Integration with CommandCoordinator

```rust
impl CommandCoordinator {
    pub async fn issue_command(&self, command: HierarchicalCommand) -> Result<()> {
        // Use conditional update instead of direct upsert
        let success = self.store
            .conditional_update_command(&command, command.conflict_policy())
            .await?;

        if !success {
            // Policy check failed - existing command wins
            info!(
                "Command {} rejected by policy {:?} - existing command has higher precedence",
                command.command_id,
                command.conflict_policy()
            );
            return Err(Error::ConflictDetected(format!(
                "Command rejected by conflict policy {:?}",
                command.conflict_policy()
            )));
        }

        // Command accepted - register for tracking
        self.active_commands
            .write()
            .await
            .insert(command.command_id.clone(), command.clone());

        Ok(())
    }
}
```

### Benefits of OCC Approach

1. **Policy enforced at write time**: WHERE clause evaluates BEFORE Ditto's CRDT merge
2. **Race-safe**: Multiple nodes can attempt concurrent writes; only policy-compliant writes succeed
3. **No additional state**: Uses existing document fields for comparison
4. **Efficient**: Single DQL statement, no read-modify-write cycle

### Limitations

1. **Requires document to exist**: First write always succeeds (no prior document to compare)
2. **Limited DQL expressiveness**: Complex policies (e.g., HighestAuthorityWins with computed authority) are difficult to express in WHERE clause
3. **Ditto-specific**: Not all CRDT backends support conditional updates

## Backend-Specific Policy Support

### Ditto (Current Implementation)

| Policy | Native Support | OCC Support | Notes |
|--------|---------------|-------------|-------|
| LastWriteWins | ✅ Native | ✅ Yes | Ditto's default LWW semantics |
| HighestPriorityWins | ❌ No | ✅ Yes | Requires OCC with `priority < :new_priority` |
| HighestAuthorityWins | ❌ No | ⚠️ Partial | Limited by DQL expressiveness |
| LowestAttributeWins | ❌ No | ✅ Yes | Requires OCC with attribute comparison |
| MergeCompatible | ❌ No | ⚠️ TODO | Requires complex compatibility logic |
| RejectConflict | ❌ No | ✅ Yes | WHERE clause `false` prevents updates |

**Recommendation**: Use `LastWriteWins` for Ditto unless you have strong requirements for other policies. OCC adds complexity and may have edge cases.

### Automerge (Future)

Automerge supports **custom conflict resolution functions** via OpSet merging. This allows policies to be implemented at the CRDT layer natively.

```rust
// Automerge (conceptual - not implemented)
impl ConflictResolver for HighestPriorityWinsResolver {
    fn resolve(&self, op_a: &Op, op_b: &Op) -> Op {
        let priority_a = extract_priority(op_a);
        let priority_b = extract_priority(op_b);

        if priority_a > priority_b {
            op_a.clone()
        } else {
            op_b.clone()
        }
    }
}
```

**Automerge advantage**: All policies can be implemented natively without OCC workarounds.

### Yjs (Future)

Yjs uses a similar approach to Automerge with custom conflict resolvers. Policies can be implemented via `Y.Doc` merge hooks.

## Application-Managed Policies

The CAP Protocol policy engine provides **mechanism**, not **policy mandate**. Applications may need custom conflict resolution logic based on:

- Mission context (ISR mission > logistics mission)
- Operator authority (human override > autonomous decision)
- Environmental conditions (degraded comms → prefer local decisions)
- Safety constraints (abort command > continue command)

### Pattern: Custom Policy Implementation

```rust
/// Mission-specific policy: ISR (surveillance) commands override kinetic commands
pub struct MissionPriorityPolicy;

impl ResolutionPolicy<HierarchicalCommand> for MissionPriorityPolicy {
    fn resolve(&self, mut items: Vec<HierarchicalCommand>) -> Result<HierarchicalCommand> {
        items.sort_by(|a, b| {
            let a_priority = self.mission_priority(&a.command_type);
            let b_priority = self.mission_priority(&b.command_type);
            b_priority.cmp(&a_priority)  // Descending
        });

        Ok(items.into_iter().next().unwrap())
    }

    fn name(&self) -> &str {
        "MISSION_PRIORITY_ISR_FIRST"
    }
}

impl MissionPriorityPolicy {
    fn mission_priority(&self, command_type: &str) -> u32 {
        match command_type {
            "ABORT" => 100,              // Safety first
            "ISR" | "SURVEILLANCE" => 50, // Mission-critical intel
            "KINETIC" => 30,             // Offensive operations
            "LOGISTICS" => 10,           // Support operations
            _ => 1,
        }
    }
}
```

### Pattern: Hybrid OCC + Application Policy

```rust
impl CommandCoordinator {
    /// Issue command with custom application policy
    pub async fn issue_command_with_policy<P: ResolutionPolicy<HierarchicalCommand>>(
        &self,
        command: HierarchicalCommand,
        policy: &P,
    ) -> Result<()> {
        // 1. Check for existing conflicting commands (application layer)
        let existing = self.query_conflicting_commands(&command).await?;

        if !existing.is_empty() {
            // 2. Apply application policy
            let mut candidates = existing;
            candidates.push(command.clone());

            let winner = policy.resolve(candidates)?;

            if winner.command_id != command.command_id {
                // Existing command wins per application policy
                return Err(Error::ConflictDetected(format!(
                    "Command rejected by application policy: {}",
                    policy.name()
                )));
            }
        }

        // 3. Use OCC for CRDT-level safety (fallback to LWW)
        let success = self.store
            .conditional_update_command(&command, ConflictPolicy::LastWriteWins)
            .await?;

        if !success {
            return Err(Error::ConflictDetected(
                "Command rejected by CRDT-level conflict resolution".to_string()
            ));
        }

        Ok(())
    }
}
```

## Testing Strategy

### Unit Tests

Test OCC WHERE clause generation:
```rust
#[test]
fn test_highest_priority_wins_where_clause() {
    let store = DittoStore::new(config);
    let command = create_test_command(priority: 5, timestamp: 100);

    let (where_clause, params) = store
        .build_policy_where_clause(&command, ConflictPolicy::HighestPriorityWins)
        .unwrap();

    assert!(where_clause.contains("priority < :new_priority"));
    assert_eq!(params["new_priority"], 5);
}
```

### Integration Tests

Test conditional update behavior:
```rust
#[tokio::test]
async fn test_occ_rejects_lower_priority_command() {
    let store = DittoStore::new(config);

    // Insert high-priority command
    let cmd_high = create_test_command(id: "cmd-1", priority: 5, timestamp: 100);
    store.upsert_command(&cmd_high).await.unwrap();

    // Attempt to overwrite with low-priority command
    let cmd_low = create_test_command(id: "cmd-1", priority: 1, timestamp: 101);
    let success = store
        .conditional_update_command(&cmd_low, ConflictPolicy::HighestPriorityWins)
        .await
        .unwrap();

    assert!(!success, "Lower priority command should be rejected");

    // Verify high-priority command still present
    let result = store.query_command("cmd-1").await.unwrap();
    assert_eq!(result.priority, 5);
}
```

### E2E Tests

Test distributed OCC behavior across Ditto mesh:
```rust
#[tokio::test]
async fn test_distributed_priority_resolution() {
    let harness = E2EHarness::new("occ_priority", 2).await;

    // Node 0: Issue high-priority command
    let cmd_high = create_test_command(id: "cmd-1", priority: 5, timestamp: 100);
    harness.coordinators[0]
        .issue_command(cmd_high.clone())
        .await
        .unwrap();

    // Node 1: Concurrently issue low-priority command
    let cmd_low = create_test_command(id: "cmd-1", priority: 1, timestamp: 101);
    let result = harness.coordinators[1]
        .issue_command(cmd_low.clone())
        .await;

    // Wait for sync
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Both nodes should converge to high-priority command
    for coordinator in &harness.coordinators {
        let final_cmd = coordinator.get_command("cmd-1").await.unwrap();
        assert_eq!(final_cmd.priority, 5, "High-priority command should win");
    }
}
```

## Documentation Requirements

### 1. Policy Compatibility Matrix

Document which policies work with which backends:

| Backend | LastWriteWins | HighestPriorityWins | HighestAuthorityWins | Custom Policies |
|---------|---------------|---------------------|----------------------|-----------------|
| Ditto (OCC) | ✅ Native | ✅ OCC | ⚠️ OCC (limited) | ✅ Application-layer |
| Automerge | ✅ Native | ✅ Native | ✅ Native | ✅ Native + App-layer |
| Yjs | ✅ Native | ✅ Native | ✅ Native | ✅ Native + App-layer |

### 2. Migration Guide

Provide examples for migrating from current policy engine to OCC:

```rust
// Before (broken - policy applied too late)
coordinator.issue_command(command).await?;

// After (OCC - policy enforced at write time)
let success = store.conditional_update_command(&command, policy).await?;
if !success {
    return Err(Error::ConflictDetected("Policy check failed"));
}
```

### 3. Application Policy Guide

Document how to implement custom policies:
- Extend `ResolutionPolicy<T>` trait
- Use `issue_command_with_policy()` for application-layer resolution
- Combine with OCC for CRDT-layer safety

## Future Enhancements

1. **DQL Function Extensions**: Propose Ditto SDK enhancements to support computed fields in WHERE clauses (e.g., `WHERE compute_authority(originator_id) < :new_authority`)

2. **Policy Analytics**: Track policy decisions for operational insights:
   - Commands rejected by policy
   - Policy distribution (which policies are used most)
   - Conflict resolution latency

3. **Adaptive Policies**: Adjust policies based on operational context:
   - Network partition → prefer local decisions
   - Mission-critical phase → prefer high-authority commands
   - Training mode → prefer human-authored commands

4. **Multi-Backend Abstraction**: Create unified policy API that works across Ditto, Automerge, Yjs with backend-specific optimizations

## Summary

**Current state**: Policy engine operates after CRDT merge, cannot influence conflict resolution for non-LWW policies.

**OCC solution**: Use conditional UPDATE with WHERE clauses to encode policy logic at write time, ensuring policy enforcement before CRDT merge.

**Backend flexibility**: Document that future backends (Automerge, Yjs) can implement policies natively. Applications can implement custom policies at application layer regardless of backend.

**Recommendation**:
- Use `LastWriteWins` for Ditto unless strong requirements exist
- Implement OCC for `HighestPriorityWins` if needed (with awareness of limitations)
- Design applications with policy flexibility in mind for future backend migrations
- Emphasize that CAP Protocol provides **mechanism** (Conflictable trait, ResolutionPolicy interface), not **policy mandate**

## Related Documentation

- [POLICY_ENGINE_DESIGN.md](POLICY_ENGINE_DESIGN.md) - Original policy engine design
- [ADR-014](adr/014-distributed-coordination-primitives.md) - Distributed coordination primitives
- [EXTENSIBLE_POLICY_ENGINE_DESIGN.md](EXTENSIBLE_POLICY_ENGINE_DESIGN.md) - Generic policy engine with traits
