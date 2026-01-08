# Policy Engine & Command Timeout Design

**Status**: Design Phase
**Branch**: `feature/policy-engine-timeouts`
**Related**: [BIDIRECTIONAL_FLOW.md](BIDIRECTIONAL_FLOW.md), [ADR-014](adr/014-distributed-coordination-primitives.md)

## Overview

Implement production-ready policy enforcement and timeout mechanisms for the hierarchical command system. This addresses Known Limitations #2, #3, and #4 from BIDIRECTIONAL_FLOW.md.

## Objectives

1. **Conflict Resolution**: Enforce conflict policies when commands compete for the same resources/targets
2. **Buffer Management**: Enforce buffer policies during network partitions
3. **Command Timeout**: Automatic expiration and cleanup of commands based on TTL
4. **Acknowledgment Timeout**: Timeout mechanisms for acknowledgment collection

## Architecture

### 1. Policy Engine Module

```
hive-protocol/src/command/
├── mod.rs
├── coordinator.rs
├── routing.rs
├── policy.rs          # NEW - Policy enforcement engine
└── timeout.rs         # NEW - Timeout management
```

### 2. Policy Engine Components

#### 2.1 Conflict Resolver

Enforces `ConflictPolicy` when multiple commands affect the same target:

```rust
pub struct ConflictResolver {
    /// Active commands indexed by target
    target_commands: Arc<RwLock<HashMap<String, Vec<HierarchicalCommand>>>>,
}

impl ConflictResolver {
    /// Check if new command conflicts with existing commands
    pub fn check_conflict(&self, command: &HierarchicalCommand) -> ConflictResult {
        // Implementation based on conflict_policy
    }

    /// Resolve conflict according to policy
    pub fn resolve(&self, commands: Vec<HierarchicalCommand>) -> HierarchicalCommand {
        match policy {
            LAST_WRITE_WINS => /* most recent */,
            HIGHEST_PRIORITY_WINS => /* highest priority */,
            HIGHEST_AUTHORITY_WINS => /* highest authority level */,
            MERGE_COMPATIBLE => /* attempt merge */,
            REJECT_CONFLICT => /* reject new */,
        }
    }
}
```

**Policies**:
- `LAST_WRITE_WINS`: Use `issued_at` timestamp, most recent wins
- `HIGHEST_PRIORITY_WINS`: Compare `priority` enum values
- `HIGHEST_AUTHORITY_WINS`: Derive authority from originator's hierarchy level
- `MERGE_COMPATIBLE`: Check if commands are compatible (same type, non-conflicting params)
- `REJECT_CONFLICT`: Reject new command, keep existing

#### 2.2 Buffer Manager

Enforces `BufferPolicy` during network partitions:

```rust
pub struct BufferManager {
    /// Buffered commands waiting for delivery
    buffered: Arc<RwLock<HashMap<String, BufferedCommand>>>,

    /// Partition state detector
    partition_detector: Arc<PartitionDetector>,
}

struct BufferedCommand {
    command: HierarchicalCommand,
    target_id: String,
    buffered_at: SystemTime,
    retry_count: u32,
}

impl BufferManager {
    /// Handle command when target is unreachable
    pub async fn handle_unreachable(
        &self,
        command: HierarchicalCommand,
        target_id: String,
    ) -> Result<BufferAction> {
        match command.buffer_policy {
            BUFFER_AND_RETRY => {
                self.buffer_command(command, target_id).await;
                Ok(BufferAction::Buffered)
            }
            DROP_ON_PARTITION => Ok(BufferAction::Dropped),
            REQUIRE_IMMEDIATE_DELIVERY => Err(Error::DeliveryFailed),
        }
    }

    /// Retry buffered commands when partition heals
    pub async fn retry_buffered(&self) {
        // Check partition state, retry commands
    }
}
```

**Policies**:
- `BUFFER_AND_RETRY`: Queue commands, deliver when partition heals
- `DROP_ON_PARTITION`: Silently drop if target unreachable
- `REQUIRE_IMMEDIATE_DELIVERY`: Fail if cannot deliver immediately

#### 2.3 Timeout Manager

Handles command expiration and acknowledgment timeouts:

```rust
pub struct TimeoutManager {
    /// Commands with expiration times
    expiring_commands: Arc<RwLock<BTreeMap<SystemTime, Vec<String>>>>,

    /// Acknowledgment timeout tracking
    ack_timeouts: Arc<RwLock<HashMap<String, AckTimeout>>>,
}

struct AckTimeout {
    command_id: String,
    expected_acks: Vec<String>,  // node_ids
    received_acks: Vec<String>,
    expires_at: SystemTime,
}

impl TimeoutManager {
    /// Register command for expiration
    pub async fn register_expiration(&self, command: &HierarchicalCommand) {
        if let Some(expires_at) = command.expires_at {
            let expiry = SystemTime::UNIX_EPOCH + Duration::from_secs(expires_at.seconds);
            self.expiring_commands
                .write()
                .await
                .entry(expiry)
                .or_default()
                .push(command.command_id.clone());
        }
    }

    /// Check and process expired commands
    pub async fn process_expired(&self) -> Vec<String> {
        let now = SystemTime::now();
        let mut expired = Vec::new();

        let mut expiring = self.expiring_commands.write().await;
        let expired_keys: Vec<_> = expiring
            .range(..=now)
            .map(|(k, _)| *k)
            .collect();

        for key in expired_keys {
            if let Some(commands) = expiring.remove(&key) {
                expired.extend(commands);
            }
        }

        expired
    }

    /// Register acknowledgment timeout
    pub async fn register_ack_timeout(
        &self,
        command_id: String,
        expected_acks: Vec<String>,
        timeout: Duration,
    ) {
        let ack_timeout = AckTimeout {
            command_id: command_id.clone(),
            expected_acks,
            received_acks: Vec::new(),
            expires_at: SystemTime::now() + timeout,
        };

        self.ack_timeouts.write().await.insert(command_id, ack_timeout);
    }

    /// Update acknowledgment progress
    pub async fn record_ack(&self, command_id: &str, node_id: &str) {
        if let Some(timeout) = self.ack_timeouts.write().await.get_mut(command_id) {
            timeout.received_acks.push(node_id.to_string());
        }
    }

    /// Check for acknowledgment timeouts
    pub async fn check_ack_timeouts(&self) -> Vec<String> {
        let now = SystemTime::now();
        let timeouts = self.ack_timeouts.read().await;

        timeouts
            .iter()
            .filter(|(_, t)| t.expires_at <= now && t.received_acks.len() < t.expected_acks.len())
            .map(|(id, _)| id.clone())
            .collect()
    }
}
```

**Features**:
- **Command TTL**: Commands expire based on `expires_at` field
- **Auto-cleanup**: Expired commands removed from active set
- **Ack Timeout**: Track expected vs received acknowledgments
- **Timeout Events**: Generate events for expired commands/acks

### 3. Integration with CommandCoordinator

```rust
pub struct CommandCoordinator {
    // Existing fields...
    node_id: String,
    router: CommandRouter,
    active_commands: Arc<RwLock<HashMap<String, HierarchicalCommand>>>,
    acknowledgments: Arc<RwLock<HashMap<(String, String), CommandAcknowledgment>>>,
    command_status: Arc<RwLock<HashMap<String, CommandStatus>>>,

    // NEW: Policy engine components
    conflict_resolver: Arc<ConflictResolver>,
    buffer_manager: Arc<BufferManager>,
    timeout_manager: Arc<TimeoutManager>,
}

impl CommandCoordinator {
    /// Issue command with policy enforcement
    pub async fn issue_command(&self, command: HierarchicalCommand) -> Result<()> {
        // 1. Check for conflicts
        let conflict_result = self.conflict_resolver.check_conflict(&command).await;
        if let ConflictResult::Conflict(existing) = conflict_result {
            let resolved = self.conflict_resolver.resolve(vec![existing, command.clone()]).await;
            if resolved.command_id != command.command_id {
                return Err(Error::ConflictRejected);
            }
        }

        // 2. Register expiration if TTL present
        self.timeout_manager.register_expiration(&command).await;

        // 3. Store and route
        self.active_commands.write().await.insert(command.command_id.clone(), command.clone());
        self.route_command(&command).await?;

        // 4. Setup ack timeout if required
        if command.acknowledgment_policy != AcknowledgmentPolicy::NO_ACK_REQUIRED {
            let expected = self.get_expected_acks(&command).await;
            let timeout = Duration::from_secs(30); // Configurable
            self.timeout_manager
                .register_ack_timeout(command.command_id.clone(), expected, timeout)
                .await;
        }

        Ok(())
    }

    /// Background task for timeout processing
    pub async fn run_timeout_processor(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            // Process expired commands
            let expired = self.timeout_manager.process_expired().await;
            for command_id in expired {
                self.handle_expired_command(&command_id).await;
            }

            // Check ack timeouts
            let ack_timeouts = self.timeout_manager.check_ack_timeouts().await;
            for command_id in ack_timeouts {
                self.handle_ack_timeout(&command_id).await;
            }
        }
    }
}
```

## Implementation Plan

### Phase 1: Conflict Resolution (Week 1)

**Tasks**:
1. Create `conflict_resolver.rs` module
2. Implement `ConflictResolver` struct
3. Add conflict detection logic
4. Implement each policy:
   - LAST_WRITE_WINS
   - HIGHEST_PRIORITY_WINS
   - HIGHEST_AUTHORITY_WINS
   - MERGE_COMPATIBLE
   - REJECT_CONFLICT
5. Unit tests for each policy
6. Integration with CommandCoordinator

**Tests**:
- Test each conflict policy independently
- Test priority ordering
- Test timestamp-based resolution
- Test authority hierarchy

### Phase 2: Buffer Management (Week 1-2)

**Tasks**:
1. Create `buffer_manager.rs` module
2. Implement `BufferManager` struct
3. Add partition detection (simple reachability check)
4. Implement each policy:
   - BUFFER_AND_RETRY
   - DROP_ON_PARTITION
   - REQUIRE_IMMEDIATE_DELIVERY
5. Add retry logic with backoff
6. Unit tests for buffering
7. Integration with CommandCoordinator

**Tests**:
- Test command buffering during partition
- Test retry on partition heal
- Test drop behavior
- Test immediate delivery requirement

### Phase 3: Timeout Management (Week 2)

**Tasks**:
1. Create `timeout_manager.rs` module
2. Implement `TimeoutManager` struct
3. Add command expiration tracking
4. Add acknowledgment timeout tracking
5. Implement background timeout processor
6. Add timeout event generation
7. Unit tests for timeouts
8. Integration with CommandCoordinator

**Tests**:
- Test command expiration
- Test ack timeout detection
- Test cleanup of expired commands
- Test timeout events

### Phase 4: Integration & E2E Tests (Week 2-3)

**Tasks**:
1. Update CommandCoordinator with all engines
2. Add background timeout processor task
3. Update DittoStore to support cleanup
4. Write integration tests
5. Write E2E tests with real timeouts
6. Performance testing
7. Documentation updates

**Tests**:
- E2E test: Command with TTL expires
- E2E test: Conflict resolution in mesh
- E2E test: Buffer and retry during partition
- E2E test: Ack timeout with partial responses

## Configuration

Add configuration options to CommandCoordinator:

```rust
pub struct PolicyConfig {
    /// Default command TTL if not specified
    pub default_ttl: Option<Duration>,

    /// Default acknowledgment timeout
    pub default_ack_timeout: Duration,

    /// Buffer retry interval
    pub buffer_retry_interval: Duration,

    /// Max buffer size (number of commands)
    pub max_buffer_size: usize,

    /// Max retry attempts for buffered commands
    pub max_retry_attempts: u32,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            default_ttl: None,  // No default TTL
            default_ack_timeout: Duration::from_secs(30),
            buffer_retry_interval: Duration::from_secs(5),
            max_buffer_size: 1000,
            max_retry_attempts: 3,
        }
    }
}
```

## Testing Strategy

### Unit Tests
- Each policy engine component tested independently
- Mock dependencies (router, partition detector)
- Test edge cases (simultaneous conflicts, multiple expirations)

### Integration Tests
- CommandCoordinator with all engines integrated
- Single-node scenario testing
- Policy interactions (conflict + timeout)

### E2E Tests
- Multi-peer scenarios with real Ditto mesh
- Network partition simulation
- Command expiration in distributed setting
- Conflict resolution across mesh

## Success Criteria

1. ✅ All 5 conflict policies implemented and tested
2. ✅ All 3 buffer policies implemented and tested
3. ✅ Command TTL enforcement working
4. ✅ Acknowledgment timeout detection working
5. ✅ Background processor handles 1000+ commands efficiently
6. ✅ E2E tests pass with <5% timeout false positives
7. ✅ Documentation updated with policy usage examples
8. ✅ Zero performance regression in existing tests

## Known Trade-offs

1. **Conflict Detection Scope**: Initial implementation detects conflicts at command reception, not pre-emptively
2. **Partition Detection**: Simple reachability check, not full network partition detection
3. **Buffer Size Limits**: Fixed buffer size, no prioritization for buffer overflow
4. **Timeout Granularity**: 1-second tick interval, not sub-second precision

## Future Enhancements

1. **Predictive Conflict Detection**: Analyze command intent to detect conflicts before execution
2. **Smart Buffer Prioritization**: Priority-based buffer eviction when buffer is full
3. **Adaptive Timeouts**: Adjust timeouts based on network conditions
4. **Policy Analytics**: Track policy decisions for operational insights

## Related Work

- [BIDIRECTIONAL_FLOW.md](BIDIRECTIONAL_FLOW.md) - Current implementation
- [ADR-014](adr/014-distributed-coordination-primitives.md) - Design decisions
- [command.proto](../hive-schema/proto/command.proto) - Schema definitions
