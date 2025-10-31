# E5 (Hierarchical Operations) Implementation Plan
**Epic:** E5 - Hierarchical Operations Phase
**Duration:** 3-4 weeks
**Prerequisites:** E1-E4 complete, refactoring complete
**Created:** 2025-10-31

---

## Overview

E5 implements Phase 3 of the CAP protocol: hierarchical message routing with platoon-level coordination. This phase enables scalable coordination of 100+ platforms organized into squads and platoons.

**Goal:** Implement hierarchical message routing (platform → squad → platoon) with O(n log n) message complexity.

**Success Criteria:**
- ✅ Platforms only message squad peers
- ✅ Squad leaders message platoon level
- ✅ Cross-squad messages rejected
- ✅ Message complexity is O(n log n)
- ✅ 100 platforms, 20 squads, 4 platoons coordinate successfully

---

## Phase 0: Prerequisites (Week 0 - Before E5)

### 0.1 Refactoring Tasks

**Task 1: Add platoon_id to PlatformState** (0.5 days)
```rust
// File: cap-protocol/src/models/platform.rs
pub struct PlatformState {
    pub squad_id: Option<String>,
    pub platoon_id: Option<String>,  // NEW - for direct platoon queries
    // ... existing fields
}
```

**Rationale:** Enables efficient "all platforms in platoon X" queries without traversing squad relationships.

**Testing:**
- Update serialization tests
- Update merge() logic
- Update Ditto schema

---

**Task 2: Implement Routing Table Cache** (1 day)
```rust
// File: cap-protocol/src/hierarchy/routing_cache.rs (NEW)
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::time::Instant;

pub struct RoutingCache {
    platform_to_squad: Arc<RwLock<HashMap<String, String>>>,
    squad_to_platoon: Arc<RwLock<HashMap<String, String>>>,
    last_refresh: Arc<RwLock<Instant>>,
    refresh_interval: Duration,
}

impl RoutingCache {
    pub fn new(refresh_interval: Duration) -> Self;
    pub async fn get_platform_squad(&self, platform_id: &str) -> Option<String>;
    pub async fn get_squad_platoon(&self, squad_id: &str) -> Option<String>;
    pub async fn refresh(&self, store: &DittoStore) -> Result<()>;
    pub fn invalidate(&self);
}
```

**Rationale:** Avoids repeated Ditto queries for routing lookups. Read-heavy workload benefits from RwLock.

**Testing:**
- Cache hit/miss scenarios
- Refresh logic
- Concurrent access
- Cache invalidation on membership changes

---

**Task 3: Implement State Update Throttling** (1-2 days)
```rust
// File: cap-protocol/src/storage/throttled_updates.rs (NEW)
use tokio::time::{Duration, Instant};

pub struct ThrottledPlatformStore {
    inner: PlatformStore,
    pending_updates: Arc<Mutex<HashMap<String, PlatformState>>>,
    last_sync: Arc<Mutex<Instant>>,
    sync_interval: Duration,
}

impl ThrottledPlatformStore {
    pub fn new(store: PlatformStore, sync_interval: Duration) -> Self;

    /// Queue state update, sync if interval elapsed
    pub async fn update_state(&self, platform_id: &str, state: &PlatformState)
        -> Result<()>;

    /// Force flush pending updates
    pub async fn flush(&self) -> Result<()>;
}
```

**Rationale:** Reduces Ditto sync traffic from high-frequency updates (position, heartbeat). Batch updates every 1 second instead of immediate sync.

**Testing:**
- Throttling behavior
- Flush on shutdown
- Concurrent updates
- Performance benchmarks (sync ops/sec reduction)

---

**Task 4: Create E5 Design Document (ADR-005)** (0.5 days)

Document architectural decisions for:
- Hierarchical routing approach (wrapper vs. refactor)
- Platoon coordinator pattern (leader-based vs. distributed)
- Message transport (Ditto vs. P2P vs. hybrid)
- Flow control strategy

**Deliverable:** `docs/adr/005-hierarchical-operations-design.md`

---

**Task 5: Set Up Performance Benchmarking** (0.5 days)

Create benchmarking framework for:
- Message complexity measurement
- Query latency tracking
- Routing table convergence time
- Ditto sync bandwidth monitoring

**Tool:** cargo-criterion or custom metrics collection

**Deliverable:** `cap-protocol/benches/hierarchy_benchmarks.rs`

---

**Prerequisites Total: 4-5 days (1 week)**

---

## Phase 1: E5.1 - Hierarchical Message Router (Days 1-4)

### Story: E5.1 - Hierarchical Message Router

**Goal:** Implement routing table and hierarchical message router with routing rules enforcement.

### Task 1.1: Implement RoutingTable (1 day)

**File:** `cap-protocol/src/hierarchy/routing_table.rs`

```rust
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Hierarchical routing table (platform → squad → platoon)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingTable {
    /// Platform → Squad mappings
    pub platform_assignments: HashMap<String, String>,
    /// Squad → Platoon mappings
    pub squad_assignments: HashMap<String, String>,
    /// Squad leaders (for upward routing privileges)
    pub squad_leaders: HashMap<String, String>,  // squad_id → leader_platform_id
    /// Timestamp for conflict resolution (LWW)
    pub timestamp: u64,
}

impl RoutingTable {
    pub fn new() -> Self;

    /// Assign platform to squad
    pub fn assign_platform(&mut self, platform_id: String, squad_id: String);

    /// Assign squad to platoon
    pub fn assign_squad(&mut self, squad_id: String, platoon_id: String);

    /// Set squad leader
    pub fn set_squad_leader(&mut self, squad_id: String, leader_id: String);

    /// Get platform's squad
    pub fn get_platform_squad(&self, platform_id: &str) -> Option<&String>;

    /// Get squad's platoon
    pub fn get_squad_platoon(&self, squad_id: &str) -> Option<&String>;

    /// Get platform's platoon (two-hop lookup)
    pub fn get_platform_platoon(&self, platform_id: &str) -> Option<String>;

    /// Check if platform is squad leader
    pub fn is_squad_leader(&self, platform_id: &str) -> bool;

    /// Get all platforms in squad
    pub fn get_squad_platforms(&self, squad_id: &str) -> Vec<String>;

    /// Get all squads in platoon
    pub fn get_platoon_squads(&self, platoon_id: &str) -> Vec<String>;

    /// Merge with another routing table (CRDT merge semantics)
    pub fn merge(&mut self, other: &RoutingTable);
}
```

**Testing:**
- Basic assignment/lookup operations
- Two-hop lookups (platform → platoon)
- Leader privilege checks
- CRDT merge with concurrent updates
- Edge cases (missing mappings, cycles)

---

### Task 1.2: Implement HierarchicalRouter (2 days)

**File:** `cap-protocol/src/hierarchy/router.rs`

```rust
use crate::traits::MessageRouter;
use crate::squad::messaging::SquadMessageBus;
use crate::{Error, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Hierarchical message router enforcing routing rules
pub struct HierarchicalRouter {
    platform_id: String,
    routing_table: Arc<Mutex<RoutingTable>>,
    routing_cache: Arc<RoutingCache>,
    squad_router: Option<SquadRouter>,
    platoon_router: Option<PlatoonRouter>,
}

impl HierarchicalRouter {
    pub fn new(
        platform_id: String,
        routing_table: Arc<Mutex<RoutingTable>>,
        routing_cache: Arc<RoutingCache>,
    ) -> Self;

    /// Initialize squad-level routing
    pub async fn init_squad_router(&mut self, squad_id: String) -> Result<()>;

    /// Initialize platoon-level routing (leader only)
    pub async fn init_platoon_router(&mut self, platoon_id: String) -> Result<()>;

    /// Route message to target (enforces hierarchy rules)
    pub async fn route_message(&self, target: &str, message: Vec<u8>) -> Result<()>;

    /// Check if route is valid per hierarchy rules
    pub fn is_route_valid(&self, from: &str, to: &str) -> bool;

    /// Get valid message targets for this platform
    pub fn valid_targets(&self) -> Vec<String>;

    /// Update routing table (called on membership changes)
    pub async fn update_routing_table(&mut self, table: RoutingTable) -> Result<()>;
}

impl MessageRouter for HierarchicalRouter {
    async fn route(&mut self, message: Vec<u8>) -> Result<()>;
    fn is_route_valid(&self, from: &str, to: &str) -> bool;
    fn valid_targets(&self) -> Vec<String>;
}

/// Squad-level router wrapper
pub struct SquadRouter {
    squad_id: String,
    message_bus: Arc<SquadMessageBus>,
    routing_table: Arc<Mutex<RoutingTable>>,
}

impl SquadRouter {
    pub fn new(
        squad_id: String,
        message_bus: Arc<SquadMessageBus>,
        routing_table: Arc<Mutex<RoutingTable>>,
    ) -> Self;

    /// Send message to squad peer (validates target is squad member)
    pub async fn send_to_peer(&self, target: &str, message: Vec<u8>) -> Result<()>;

    /// Validate target is in same squad
    fn validate_target(&self, target: &str) -> Result<()>;
}
```

**Routing Rules:**
1. Platform can message: squad peers only (same squad)
2. Squad leader can message: squad peers + platoon level
3. Non-leader cannot message: cross-squad, platoon level
4. Reject all: direct cross-squad messages

**Testing:**
- Routing rule validation (intra-squad allowed, cross-squad rejected)
- Leader privilege enforcement (upward routing)
- Non-leader upward routing rejection
- Routing table updates
- Concurrent routing operations

---

### Task 1.3: Integration with SquadMessageBus (0.5 days)

**Changes:**
- Wrap existing SquadMessageBus with SquadRouter
- Add routing validation before message send
- No changes to SquadMessageBus internals (non-breaking)

**Testing:**
- Existing E4 tests still pass
- New routing validation tests

---

### Task 1.4: Routing Rules Tests (0.5 days)

**Test scenarios:**
- Platform → squad peer (allowed)
- Platform → different squad platform (rejected)
- Squad leader → platoon (allowed)
- Non-leader → platoon (rejected)
- Invalid target (non-existent platform)

**Deliverable:** `cap-protocol/src/hierarchy/router_tests.rs`

---

**Phase 1 Total: 4 days**

---

## Phase 2: E5.2 - Platoon Level Aggregation (Days 5-9)

### Story: E5.2 - Platoon Level Aggregation

**Goal:** Implement platoon model, coordinator, and squad-to-platoon messaging.

### Task 2.1: Create Platoon Model (0.5 days)

**File:** `cap-protocol/src/models/platoon.rs`

```rust
use crate::models::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Platoon configuration (G-Set)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatoonConfig {
    pub id: String,
    pub max_squads: usize,
    pub min_squads: usize,
    pub created_at: u64,
}

impl PlatoonConfig {
    pub fn new(max_squads: usize) -> Self;
}

/// Platoon runtime state (CRDT)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatoonState {
    pub config: PlatoonConfig,
    /// Platoon commander (LWW-Register)
    pub commander_id: Option<String>,
    /// Squad membership (OR-Set)
    pub squads: HashSet<String>,
    /// Aggregated capabilities from squads (G-Set)
    pub aggregated_capabilities: Vec<Capability>,
    /// Timestamp for LWW conflict resolution
    pub timestamp: u64,
}

impl PlatoonState {
    pub fn new(config: PlatoonConfig) -> Self;

    /// Add squad to platoon (OR-Set add)
    pub fn add_squad(&mut self, squad_id: String) -> bool;

    /// Remove squad from platoon (OR-Set remove)
    pub fn remove_squad(&mut self, squad_id: &str) -> bool;

    /// Set platoon commander (LWW-Register)
    pub fn set_commander(&mut self, commander_id: String) -> Result<()>;

    /// Add aggregated capability (G-Set add)
    pub fn add_capability(&mut self, capability: Capability);

    /// Check if platoon meets minimum size
    pub fn is_valid(&self) -> bool;

    /// Check if platoon is at capacity
    pub fn is_full(&self) -> bool;

    /// Merge with another platoon state (CRDT merge)
    pub fn merge(&mut self, other: &PlatoonState);

    /// Update timestamp
    fn update_timestamp(&mut self);
}
```

**Testing:**
- CRDT operations (add/remove squads, set commander)
- Merge semantics
- Validation logic (min/max squads)
- Serialization round-trip

---

### Task 2.2: Implement PlatoonStore (1 day)

**File:** `cap-protocol/src/storage/platoon_store.rs`

```rust
use crate::models::platoon::{PlatoonConfig, PlatoonState};
use crate::storage::ditto_store::DittoStore;
use crate::{Error, Result};

const PLATOON_COLLECTION: &str = "platoons";

/// Platoon storage manager
pub struct PlatoonStore {
    store: DittoStore,
}

impl PlatoonStore {
    pub fn new(store: DittoStore) -> Self;

    /// Store platoon state
    pub async fn store_platoon(&self, platoon: &PlatoonState) -> Result<String>;

    /// Retrieve platoon by ID
    pub async fn get_platoon(&self, platoon_id: &str) -> Result<Option<PlatoonState>>;

    /// Get all valid platoons
    pub async fn get_valid_platoons(&self) -> Result<Vec<PlatoonState>>;

    /// Get platoons with specific squad
    pub async fn get_platoons_with_squad(&self, squad_id: &str) -> Result<Vec<PlatoonState>>;

    /// Add squad to platoon
    pub async fn add_squad(&self, platoon_id: &str, squad_id: String) -> Result<()>;

    /// Remove squad from platoon
    pub async fn remove_squad(&self, platoon_id: &str, squad_id: &str) -> Result<()>;

    /// Set platoon commander
    pub async fn set_commander(&self, platoon_id: &str, commander_id: String) -> Result<()>;

    /// Delete platoon
    pub async fn delete_platoon(&self, platoon_id: &str) -> Result<()>;
}
```

**Testing:**
- Store/retrieve operations
- Query methods
- Concurrent updates
- CRDT merge via Ditto

---

### Task 2.3: Implement PlatoonMessageBus (1 day)

**File:** `cap-protocol/src/hierarchy/platoon_messaging.rs`

```rust
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Platoon-level message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlatoonMessage {
    /// Squad summary from squad leader
    SquadSummary {
        squad_id: String,
        member_count: usize,
        capabilities: Vec<Capability>,
        readiness: f32,
    },
    /// Commander announcement
    CommanderAnnounce {
        commander_id: String,
        timestamp: u64,
    },
    /// Platoon status update
    StatusUpdate {
        status: PlatoonStatus,
        timestamp: u64,
    },
    /// Task assignment to squad
    TaskAssignment {
        target_squad: String,
        task: Task,
    },
}

/// Platoon message bus (squad leader → platoon level)
pub struct PlatoonMessageBus {
    platoon_id: String,
    platform_id: String,  // Must be squad leader
    outbound_queue: Arc<Mutex<VecDeque<PlatoonMessage>>>,
    subscribers: Arc<Mutex<Vec<PlatoonMessageHandler>>>,
}

impl PlatoonMessageBus {
    pub fn new(platoon_id: String, platform_id: String) -> Self;

    /// Publish message to platoon (squad leaders only)
    pub async fn publish(&self, message: PlatoonMessage) -> Result<()>;

    /// Subscribe to platoon messages
    pub fn subscribe(&mut self, handler: PlatoonMessageHandler);

    /// Process pending outbound messages
    pub async fn flush(&mut self) -> Result<()>;
}

pub type PlatoonMessageHandler = Arc<dyn Fn(PlatoonMessage) + Send + Sync>;
```

**Testing:**
- Message publishing
- Subscription and delivery
- Squad leader validation
- Message ordering

---

### Task 2.4: Implement PlatoonCoordinator (1.5 days)

**File:** `cap-protocol/src/hierarchy/platoon.rs`

```rust
use crate::models::platoon::{PlatoonConfig, PlatoonState};
use crate::models::squad::SquadState;
use crate::storage::platoon_store::PlatoonStore;
use crate::{Error, Result};

/// Platoon formation status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatoonFormationStatus {
    Forming,
    Ready,
    Degraded,
}

/// Platoon formation coordinator (run by platoon commander)
pub struct PlatoonCoordinator {
    pub platoon_id: String,
    pub min_squads: usize,
    pub min_readiness: f32,
    pub status: PlatoonFormationStatus,
}

impl PlatoonCoordinator {
    pub fn new(platoon_id: String, min_squads: usize, min_readiness: f32) -> Self;

    /// Check if platoon formation is complete
    pub fn check_formation_complete(
        &mut self,
        squads: &[SquadState],
        commander_id: Option<&str>,
    ) -> Result<bool>;

    /// Aggregate capabilities from squads
    pub fn aggregate_capabilities(&self, squads: &[SquadState]) -> Vec<Capability>;

    /// Detect emergent platoon-level capabilities
    pub fn detect_emergent_capabilities(&self, squads: &[SquadState]) -> Vec<Capability>;

    /// Check if platoon can transition to operations phase
    pub fn can_transition_to_operations(&self) -> bool;

    /// Get formation metrics
    pub fn get_metrics(&self) -> PlatoonMetrics;
}

/// Platoon formation metrics
#[derive(Debug, Clone)]
pub struct PlatoonMetrics {
    pub squad_count: usize,
    pub total_platforms: usize,
    pub average_squad_readiness: f32,
    pub capability_count: usize,
    pub formation_time_ms: u64,
}
```

**Testing:**
- Formation completion detection
- Capability aggregation
- Emergent capability detection
- Metrics collection

---

### Task 2.5: Integration Tests (0.5 days)

**Scenarios:**
- 4 squads form platoon
- Commander election
- Squad summary propagation
- Capability aggregation
- 3-level hierarchy (platform → squad → platoon)

**Deliverable:** `cap-protocol/src/hierarchy/platoon_tests.rs`

---

**Phase 2 Total: 5 days**

---

## Phase 3: E5.3/E5.4 - Priority & Flow Control (Days 10-14)

### Story: E5.3 - Priority-Based Routing

**Goal:** Extend priority system to hierarchical routing with per-hop priority queues.

### Task 3.1: Extend MessagePriority (0.5 days)

**File:** `cap-protocol/src/squad/messaging.rs` (extend existing)

- Already has 4 priority levels: Low, Normal, High, Critical
- Add hierarchical context: intra-squad vs. upward propagation
- Document priority escalation rules

**Testing:**
- Priority ordering within hierarchy
- Priority inversion scenarios

---

### Story: E5.4 - Message Flow Control

**Goal:** Implement bandwidth limits and backpressure for hierarchical messaging.

### Task 3.2: Implement FlowController (2 days)

**File:** `cap-protocol/src/hierarchy/flow_control.rs`

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;
use std::time::{Duration, Instant};

/// Per-link bandwidth limits
#[derive(Debug, Clone)]
pub struct BandwidthLimit {
    /// Messages per second
    pub messages_per_sec: usize,
    /// Bytes per second
    pub bytes_per_sec: usize,
}

/// Flow control for hierarchical message routing
pub struct FlowController {
    /// Rate limiters per routing level
    squad_limiter: Arc<RateLimiter>,
    platoon_limiter: Arc<RateLimiter>,
    /// Backpressure state
    backpressure: Arc<Mutex<BackpressureState>>,
    /// Message dropping policy
    drop_policy: MessageDropPolicy,
}

impl FlowController {
    pub fn new(
        squad_limit: BandwidthLimit,
        platoon_limit: BandwidthLimit,
        drop_policy: MessageDropPolicy,
    ) -> Self;

    /// Acquire permit to send message
    pub async fn acquire_permit(&self, level: RoutingLevel) -> Result<Permit>;

    /// Check if backpressure is active
    pub fn has_backpressure(&self) -> bool;

    /// Apply backpressure (slow down message generation)
    pub async fn apply_backpressure(&self, level: RoutingLevel) -> Result<()>;

    /// Release backpressure
    pub fn release_backpressure(&self);

    /// Drop message according to policy
    pub fn should_drop(&self, message: &Message) -> bool;

    /// Get flow control metrics
    pub fn get_metrics(&self) -> FlowMetrics;
}

/// Rate limiter using token bucket algorithm
pub struct RateLimiter {
    tokens: Arc<Mutex<f64>>,
    capacity: f64,
    refill_rate: f64,  // tokens per second
    last_refill: Arc<Mutex<Instant>>,
}

impl RateLimiter {
    pub fn new(capacity: usize, refill_rate: usize) -> Self;
    pub async fn acquire(&self) -> Result<()>;
    fn refill(&self);
}

/// Message dropping policy on overload
#[derive(Debug, Clone, Copy)]
pub enum MessageDropPolicy {
    /// Drop lowest priority messages first
    DropLowPriority,
    /// Drop oldest messages first
    DropOldest,
    /// Drop randomly (fairness)
    DropRandom,
    /// Never drop (backpressure only)
    NeverDrop,
}

/// Backpressure state
#[derive(Debug, Clone)]
pub struct BackpressureState {
    pub active: bool,
    pub level: RoutingLevel,
    pub queue_depth: usize,
    pub utilization: f32,  // 0.0-1.0
}

/// Flow control metrics
#[derive(Debug, Clone)]
pub struct FlowMetrics {
    pub messages_sent: usize,
    pub messages_dropped: usize,
    pub backpressure_events: usize,
    pub average_queue_depth: f32,
    pub bandwidth_utilization: f32,
}
```

**Testing:**
- Rate limiting behavior
- Backpressure activation/release
- Message dropping policies
- Concurrent flow control
- Performance (throughput under limits)

---

### Task 3.3: Integrate with HierarchicalRouter (1 day)

**Changes:**
- Add FlowController to HierarchicalRouter
- Check flow control before routing message
- Apply backpressure when limits reached
- Drop messages according to policy

**Testing:**
- Flow control integration
- Priority + flow control interaction
- Backpressure propagation

---

### Task 3.4: Metrics Collection (0.5 days)

**File:** `cap-protocol/src/hierarchy/metrics.rs`

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Hierarchical routing metrics
pub struct RoutingMetrics {
    /// Message counts per level
    pub squad_messages: Arc<AtomicU64>,
    pub platoon_messages: Arc<AtomicU64>,
    /// Hop count distribution
    pub hop_counts: Arc<Mutex<Vec<usize>>>,
    /// Latency tracking
    pub latencies_ms: Arc<Mutex<Vec<u64>>>,
    /// Dropped messages
    pub dropped_messages: Arc<AtomicU64>,
}

impl RoutingMetrics {
    pub fn new() -> Self;
    pub fn record_message(&self, level: RoutingLevel);
    pub fn record_hops(&self, hops: usize);
    pub fn record_latency(&self, latency_ms: u64);
    pub fn record_drop(&self);
    pub fn get_summary(&self) -> MetricsSummary;
}

/// Metrics summary
#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub total_messages: u64,
    pub average_hops: f32,
    pub average_latency_ms: f32,
    pub drop_rate: f32,
    pub messages_per_sec: f32,
}
```

**Testing:**
- Metrics collection accuracy
- Concurrent metrics updates
- Summary computation

---

**Phase 3 Total: 4 days**

---

## Phase 4: E5.5 - Hierarchy Maintenance (Days 15-19)

### Story: E5.5 - Dynamic Hierarchy Rebalancing

**Goal:** Implement squad merge/split and routing table maintenance.

### Task 4.1: Squad Merge/Split Logic (2 days)

**File:** `cap-protocol/src/hierarchy/maintenance.rs`

```rust
use crate::models::squad::SquadState;
use crate::models::platoon::PlatoonState;
use crate::{Error, Result};

/// Hierarchy maintenance coordinator
pub struct HierarchyMaintainer {
    /// Squad size constraints
    pub min_squad_size: usize,
    pub max_squad_size: usize,
    /// Platoon size constraints
    pub min_platoon_squads: usize,
    pub max_platoon_squads: usize,
}

impl HierarchyMaintainer {
    pub fn new(
        min_squad_size: usize,
        max_squad_size: usize,
        min_platoon_squads: usize,
        max_platoon_squads: usize,
    ) -> Self;

    /// Check if squad needs rebalancing
    pub fn needs_rebalance(&self, squad: &SquadState) -> RebalanceAction;

    /// Merge undersized squads
    pub async fn merge_squads(
        &self,
        squad1: &SquadState,
        squad2: &SquadState,
    ) -> Result<SquadState>;

    /// Split oversized squad
    pub async fn split_squad(&self, squad: &SquadState) -> Result<(SquadState, SquadState)>;

    /// Find merge candidate (nearest squad with capacity)
    pub fn find_merge_candidate(
        &self,
        squad: &SquadState,
        candidates: &[SquadState],
    ) -> Option<String>;

    /// Check if platoon needs rebalancing
    pub fn needs_platoon_rebalance(&self, platoon: &PlatoonState) -> bool;

    /// Get rebalancing metrics
    pub fn get_metrics(&self) -> MaintenanceMetrics;
}

/// Rebalance action recommendation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebalanceAction {
    None,
    Merge,
    Split,
}

/// Maintenance metrics
#[derive(Debug, Clone)]
pub struct MaintenanceMetrics {
    pub merge_count: usize,
    pub split_count: usize,
    pub rebalance_disruptions: usize,
}
```

**Testing:**
- Squad merge logic
- Squad split logic
- Merge candidate selection
- Edge cases (min size = 1, max size = capacity)

---

### Task 4.2: Routing Table Updates (1 day)

**File:** `cap-protocol/src/hierarchy/routing_table.rs` (extend)

```rust
impl RoutingTable {
    /// Update assignments after squad merge
    pub fn handle_squad_merge(
        &mut self,
        old_squad1: &str,
        old_squad2: &str,
        new_squad: &str,
    );

    /// Update assignments after squad split
    pub fn handle_squad_split(
        &mut self,
        old_squad: &str,
        new_squad1: &str,
        new_squad2: &str,
        squad1_platforms: &[String],
        squad2_platforms: &[String],
    );

    /// Validate routing table consistency
    pub fn validate(&self) -> Result<()>;

    /// Detect and resolve routing table conflicts
    pub fn resolve_conflicts(&mut self);
}
```

**Testing:**
- Routing table updates on merge
- Routing table updates on split
- Consistency validation
- Conflict resolution

---

### Task 4.3: Disruption Minimization (1 day)

**Strategies:**
- Prefer merge over split (less disruptive)
- Batch rebalancing operations
- Coordinate leader elections during rebalance
- Graceful transition (finish in-flight messages)

**File:** `cap-protocol/src/hierarchy/maintenance.rs` (extend)

```rust
impl HierarchyMaintainer {
    /// Plan rebalancing with minimal disruption
    pub fn plan_rebalance(
        &self,
        squads: &[SquadState],
    ) -> Vec<RebalanceOperation>;

    /// Execute rebalancing plan
    pub async fn execute_rebalance(
        &self,
        operations: Vec<RebalanceOperation>,
    ) -> Result<RebalanceResult>;

    /// Wait for in-flight messages before rebalance
    async fn drain_messages(&self, squad_id: &str) -> Result<()>;
}

/// Rebalance operation
#[derive(Debug, Clone)]
pub enum RebalanceOperation {
    Merge { squad1: String, squad2: String },
    Split { squad: String },
    Reassign { platform: String, from_squad: String, to_squad: String },
}

/// Rebalance result
#[derive(Debug, Clone)]
pub struct RebalanceResult {
    pub operations_completed: usize,
    pub operations_failed: usize,
    pub disruption_time_ms: u64,
}
```

**Testing:**
- Rebalance planning
- Batch operation execution
- Disruption measurement
- In-flight message handling

---

### Task 4.4: Integration Tests (0.5 days)

**Scenarios:**
- Squad underflow → merge
- Squad overflow → split
- Routing table consistency after rebalance
- Message delivery during rebalance
- Multiple concurrent rebalances

---

**Phase 4 Total: 5 days**

---

## Phase 5: Integration & E2E Testing (Days 20-22)

### Task 5.1: End-to-End Scenarios (1.5 days)

**File:** `cap-protocol/tests/e2e_hierarchical_operations.rs`

**Scenarios:**
1. **Full Hierarchy Formation**
   - 100 platforms bootstrap into 20 squads
   - 20 squads form 4 platoons
   - Validate routing table convergence
   - Test message flow at all levels

2. **Hierarchical Message Routing**
   - Platform messages squad peers (allowed)
   - Platform messages cross-squad (rejected)
   - Squad leader messages platoon (allowed)
   - Non-leader messages platoon (rejected)

3. **Dynamic Rebalancing**
   - Squad underflow triggers merge
   - Squad overflow triggers split
   - Routing table updates correctly
   - Messages continue to flow during rebalance

4. **Flow Control**
   - Bandwidth limits enforced
   - Backpressure propagation
   - Message dropping under overload
   - Priority handling

5. **Failure Scenarios**
   - Squad leader failure → new election
   - Platoon commander failure → new commander
   - Routing table recovery
   - Message delivery guarantees

**Testing Infrastructure:**
- Use E2E harness from `cap-protocol/src/testing/e2e_harness.rs`
- Ditto observers for state change detection
- Metrics collection for validation

---

### Task 5.2: Performance Benchmarks (1 day)

**File:** `cap-protocol/benches/hierarchy_benchmarks.rs`

**Benchmarks:**
1. **Message Complexity**
   - Measure total message count for coordination
   - Target: O(n log n) behavior
   - Compare to baseline (E4 squad-level only)

2. **Query Performance**
   - Routing table lookups
   - Hierarchical queries (platform → platoon)
   - Cache hit rates

3. **Throughput**
   - Messages per second at each level
   - Flow control overhead
   - Backpressure impact

4. **Latency**
   - Message hop latency
   - Routing decision time
   - End-to-end delivery time

5. **Scalability**
   - 100, 200, 500 platforms
   - Formation time
   - Routing table convergence

**Success Criteria:**
- ✅ Message complexity O(n log n)
- ✅ Routing lookup < 1ms
- ✅ Throughput > 1000 msg/sec
- ✅ Average latency < 100ms

---

### Task 5.3: Documentation Updates (0.5 days)

**Updates:**
- `docs/CAP-POC-Project-Plan.md`: Mark E5 complete
- `docs/ARCHITECTURE-DECISION-SUMMARY.md`: Add E5 design
- `docs/TESTING_STRATEGY.md`: Add E5 test scenarios
- `README.md`: Update with E5 status

**New Docs:**
- `docs/adr/005-hierarchical-operations-design.md`: E5 ADR
- `docs/E5-PERFORMANCE-REPORT.md`: Benchmark results

---

**Phase 5 Total: 3 days**

---

## Timeline Summary

| Phase | Tasks | Duration | Days |
|-------|-------|----------|------|
| Phase 0 | Prerequisites & Refactoring | 1 week | 0 |
| Phase 1 | E5.1 - Hierarchical Message Router | 4 days | 1-4 |
| Phase 2 | E5.2 - Platoon Level Aggregation | 5 days | 5-9 |
| Phase 3 | E5.3/E5.4 - Priority & Flow Control | 4 days | 10-13 |
| Phase 4 | E5.5 - Hierarchy Maintenance | 5 days | 14-18 |
| Phase 5 | Integration & E2E Testing | 3 days | 19-21 |
| **Total** | | **22 days** | **~4 weeks** |

**Note:** Project plan estimate was 2 weeks (Week 5-7). Actual estimate is 4 weeks including prerequisites.

---

## Risks & Mitigation

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Routing table convergence issues | Medium | High | Extensive E2E testing, CRDT validation |
| Performance below O(n log n) | Medium | High | Early benchmarking, profiling, optimization |
| Flow control complexity | Medium | Medium | Incremental implementation, thorough testing |
| Rebalancing disrupts operations | Low | High | Disruption minimization strategies, graceful transitions |
| Integration breaks E4 tests | Low | Medium | Non-breaking wrapper approach, regression testing |

---

## Success Criteria Validation

| Criterion | Validation Method |
|-----------|------------------|
| Platforms only message squad peers | Routing rules unit tests, E2E rejection tests |
| Squad leaders message platoon | Leader privilege tests, platoon message delivery |
| Cross-squad messages rejected | Routing validation tests |
| Message complexity O(n log n) | Performance benchmarks with scaling tests |
| 100 platforms, 20 squads, 4 platoons | E2E scenario with metrics collection |

---

## Dependencies

**Required for E5:**
- ✅ E1 (Foundation) - Complete
- ✅ E2 (CRDT Models) - Complete
- ✅ E3 (Bootstrap) - Complete
- ✅ E4 (Squad Formation) - Complete
- ⏳ Prerequisites (refactoring) - In progress

**Deferred (not blocking):**
- E6 (Capability Composition Engine)
- E7 (Differential Updates System)
- E8 (Network Simulation Layer)
- E9 (Reference Application)

---

## Next Steps

**Immediate (Week 0):**
1. Complete prerequisite refactoring (4-5 days)
2. Create ADR-005 (E5 design document)
3. Set up benchmarking framework
4. Review and approve this plan

**Week 1 (Phase 1):**
1. Implement RoutingTable
2. Implement HierarchicalRouter
3. Integrate with SquadMessageBus
4. Write routing rules tests

**Week 2 (Phase 2):**
1. Create Platoon model and store
2. Implement PlatoonMessageBus
3. Implement PlatoonCoordinator
4. Integration tests

**Week 3 (Phase 3 & 4):**
1. Priority-based routing
2. Flow control implementation
3. Hierarchy maintenance
4. Rebalancing logic

**Week 4 (Phase 5):**
1. E2E testing
2. Performance benchmarks
3. Documentation
4. Code review and cleanup

---

## Appendix A: File Structure

```
cap-protocol/src/
├── hierarchy/
│   ├── mod.rs
│   ├── routing_table.rs        (NEW - Phase 1)
│   ├── routing_cache.rs        (NEW - Prerequisites)
│   ├── router.rs               (NEW - Phase 1)
│   ├── platoon_messaging.rs    (NEW - Phase 2)
│   ├── platoon.rs              (NEW - Phase 2)
│   ├── flow_control.rs         (NEW - Phase 3)
│   ├── maintenance.rs          (NEW - Phase 4)
│   └── metrics.rs              (NEW - Phase 3)
├── models/
│   ├── platoon.rs              (NEW - Phase 2)
│   └── platform.rs             (MODIFY - add platoon_id)
├── storage/
│   ├── platoon_store.rs        (NEW - Phase 2)
│   ├── throttled_updates.rs    (NEW - Prerequisites)
│   └── mod.rs                  (MODIFY - add platoon_store)
└── tests/
    └── e2e_hierarchical_operations.rs  (NEW - Phase 5)
```

---

## Appendix B: Key Interfaces

### MessageRouter Trait
```rust
pub trait MessageRouter: Send + Sync + Debug {
    async fn route(&mut self, message: Vec<u8>) -> Result<()>;
    fn is_route_valid(&self, from: &str, to: &str) -> bool;
    fn valid_targets(&self) -> Vec<String>;
}
```

### Routing Levels
```rust
pub enum RoutingLevel {
    Platform,   // Intra-squad messaging
    Squad,      // Squad-to-platoon messaging (leader only)
    Platoon,    // Platoon-level coordination
}
```

### Message Types
```rust
// Squad-level (existing)
pub enum SquadMessage { /* ... */ }

// Platoon-level (new)
pub enum PlatoonMessage {
    SquadSummary { /* ... */ },
    CommanderAnnounce { /* ... */ },
    StatusUpdate { /* ... */ },
    TaskAssignment { /* ... */ },
}
```

---

**Plan Status:** DRAFT - Awaiting Review
**Next Action:** Complete prerequisite refactoring, create ADR-005
