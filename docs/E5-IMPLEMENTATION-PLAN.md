# E5 (Hierarchical Operations) Implementation Plan
**Epic:** E5 - Hierarchical Operations Phase
**Duration:** 3-4 weeks
**Prerequisites:** E1-E4 complete, refactoring complete
**Created:** 2025-10-31

---

## Overview

E5 implements Phase 3 of the CAP protocol: hierarchical message routing with zone-level coordination. This phase enables scalable coordination of 100+ nodes organized into cells and zones.

**Goal:** Implement hierarchical message routing (node → cell → zone) with O(n log n) message complexity.

**Success Criteria:**
- ✅ Nodes only message cell peers
- ✅ Cell leaders message zone level
- ✅ Cross-cell messages rejected
- ✅ Message complexity is O(n log n)
- ✅ 100 nodes, 20 cells, 4 zones coordinate successfully

---

## Phase 0: Prerequisites (Week 0 - Before E5)

### 0.1 Refactoring Tasks

**Task 1: Add zone_id to PlatformState** (0.5 days)
```rust
// File: cap-protocol/src/models/node.rs
pub struct PlatformState {
    pub cell_id: Option<String>,
    pub zone_id: Option<String>,  // NEW - for direct zone queries
    // ... existing fields
}
```

**Rationale:** Enables efficient "all nodes in zone X" queries without traversing cell relationships.

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
    node_to_cell: Arc<RwLock<HashMap<String, String>>>,
    cell_to_zone: Arc<RwLock<HashMap<String, String>>>,
    last_refresh: Arc<RwLock<Instant>>,
    refresh_interval: Duration,
}

impl RoutingCache {
    pub fn new(refresh_interval: Duration) -> Self;
    pub async fn get_node_cell(&self, node_id: &str) -> Option<String>;
    pub async fn get_cell_zone(&self, cell_id: &str) -> Option<String>;
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
    pub async fn update_state(&self, node_id: &str, state: &PlatformState)
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
- Zone coordinator pattern (leader-based vs. distributed)
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

/// Hierarchical routing table (node → cell → zone)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingTable {
    /// Node → Cell mappings
    pub node_assignments: HashMap<String, String>,
    /// Cell → Zone mappings
    pub cell_assignments: HashMap<String, String>,
    /// Cell leaders (for upward routing privileges)
    pub cell_leaders: HashMap<String, String>,  // cell_id → leader_node_id
    /// Timestamp for conflict resolution (LWW)
    pub timestamp: u64,
}

impl RoutingTable {
    pub fn new() -> Self;

    /// Assign node to squad
    pub fn assign_node(&mut self, node_id: String, cell_id: String);

    /// Assign cell to platoon
    pub fn assign_cell(&mut self, cell_id: String, zone_id: String);

    /// Set cell leader
    pub fn set_cell_leader(&mut self, cell_id: String, leader_id: String);

    /// Get node's squad
    pub fn get_node_cell(&self, node_id: &str) -> Option<&String>;

    /// Get cell's platoon
    pub fn get_cell_zone(&self, cell_id: &str) -> Option<&String>;

    /// Get node's zone (two-hop lookup)
    pub fn get_node_zone(&self, node_id: &str) -> Option<String>;

    /// Check if node is cell leader
    pub fn is_cell_leader(&self, node_id: &str) -> bool;

    /// Get all nodes in squad
    pub fn get_cell_nodes(&self, cell_id: &str) -> Vec<String>;

    /// Get all cells in platoon
    pub fn get_zone_cells(&self, zone_id: &str) -> Vec<String>;

    /// Merge with another routing table (CRDT merge semantics)
    pub fn merge(&mut self, other: &RoutingTable);
}
```

**Testing:**
- Basic assignment/lookup operations
- Two-hop lookups (node → zone)
- Leader privilege checks
- CRDT merge with concurrent updates
- Edge cases (missing mappings, cycles)

---

### Task 1.2: Implement HierarchicalRouter (2 days)

**File:** `cap-protocol/src/hierarchy/router.rs`

```rust
use crate::traits::MessageRouter;
use crate::cell::messaging::SquadMessageBus;
use crate::{Error, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Hierarchical message router enforcing routing rules
pub struct HierarchicalRouter {
    node_id: String,
    routing_table: Arc<Mutex<RoutingTable>>,
    routing_cache: Arc<RoutingCache>,
    cell_router: Option<SquadRouter>,
    zone_router: Option<PlatoonRouter>,
}

impl HierarchicalRouter {
    pub fn new(
        node_id: String,
        routing_table: Arc<Mutex<RoutingTable>>,
        routing_cache: Arc<RoutingCache>,
    ) -> Self;

    /// Initialize cell-level routing
    pub async fn init_cell_router(&mut self, cell_id: String) -> Result<()>;

    /// Initialize zone-level routing (leader only)
    pub async fn init_zone_router(&mut self, zone_id: String) -> Result<()>;

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

/// Cell-level router wrapper
pub struct SquadRouter {
    cell_id: String,
    message_bus: Arc<SquadMessageBus>,
    routing_table: Arc<Mutex<RoutingTable>>,
}

impl SquadRouter {
    pub fn new(
        cell_id: String,
        message_bus: Arc<SquadMessageBus>,
        routing_table: Arc<Mutex<RoutingTable>>,
    ) -> Self;

    /// Send message to cell peer (validates target is cell member)
    pub async fn send_to_peer(&self, target: &str, message: Vec<u8>) -> Result<()>;

    /// Validate target is in same squad
    fn validate_target(&self, target: &str) -> Result<()>;
}
```

**Routing Rules:**
1. Node can message: cell peers only (same cell)
2. Cell leader can message: cell peers + zone level
3. Non-leader cannot message: cross-cell, zone level
4. Reject all: direct cross-cell messages

**Testing:**
- Routing rule validation (intra-cell allowed, cross-cell rejected)
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
- Node → cell peer (allowed)
- Node → different cell node (rejected)
- Cell leader → zone (allowed)
- Non-leader → zone (rejected)
- Invalid target (non-existent node)

**Deliverable:** `cap-protocol/src/hierarchy/router_tests.rs`

---

**Phase 1 Total: 4 days**

---

## Phase 2: E5.2 - Zone Level Aggregation (Days 5-9)

### Story: E5.2 - Zone Level Aggregation

**Goal:** Implement zone model, coordinator, and cell-to-zone messaging.

### Task 2.1: Create Zone Model (0.5 days)

**File:** `cap-protocol/src/models/zone.rs`

```rust
use crate::models::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Zone configuration (G-Set)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatoonConfig {
    pub id: String,
    pub max_cells: usize,
    pub min_cells: usize,
    pub created_at: u64,
}

impl PlatoonConfig {
    pub fn new(max_cells: usize) -> Self;
}

/// Zone runtime state (CRDT)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatoonState {
    pub config: PlatoonConfig,
    /// Zone commander (LWW-Register)
    pub commander_id: Option<String>,
    /// Cell membership (OR-Set)
    pub cells: HashSet<String>,
    /// Aggregated capabilities from cells (G-Set)
    pub aggregated_capabilities: Vec<Capability>,
    /// Timestamp for LWW conflict resolution
    pub timestamp: u64,
}

impl PlatoonState {
    pub fn new(config: PlatoonConfig) -> Self;

    /// Add cell to zone (OR-Set add)
    pub fn add_cell(&mut self, cell_id: String) -> bool;

    /// Remove cell from zone (OR-Set remove)
    pub fn remove_cell(&mut self, cell_id: &str) -> bool;

    /// Set zone commander (LWW-Register)
    pub fn set_commander(&mut self, commander_id: String) -> Result<()>;

    /// Add aggregated capability (G-Set add)
    pub fn add_capability(&mut self, capability: Capability);

    /// Check if zone meets minimum size
    pub fn is_valid(&self) -> bool;

    /// Check if zone is at capacity
    pub fn is_full(&self) -> bool;

    /// Merge with another zone state (CRDT merge)
    pub fn merge(&mut self, other: &PlatoonState);

    /// Update timestamp
    fn update_timestamp(&mut self);
}
```

**Testing:**
- CRDT operations (add/remove cells, set commander)
- Merge semantics
- Validation logic (min/max cells)
- Serialization round-trip

---

### Task 2.2: Implement PlatoonStore (1 day)

**File:** `cap-protocol/src/storage/zone_store.rs`

```rust
use crate::models::zone::{PlatoonConfig, PlatoonState};
use crate::storage::ditto_store::DittoStore;
use crate::{Error, Result};

const PLATOON_COLLECTION: &str = "zones";

/// Zone storage manager
pub struct PlatoonStore {
    store: DittoStore,
}

impl PlatoonStore {
    pub fn new(store: DittoStore) -> Self;

    /// Store zone state
    pub async fn store_zone(&self, zone: &PlatoonState) -> Result<String>;

    /// Retrieve zone by ID
    pub async fn get_zone(&self, zone_id: &str) -> Result<Option<PlatoonState>>;

    /// Get all valid platoons
    pub async fn get_valid_zones(&self) -> Result<Vec<PlatoonState>>;

    /// Get zones with specific squad
    pub async fn get_zones_with_cell(&self, cell_id: &str) -> Result<Vec<PlatoonState>>;

    /// Add cell to platoon
    pub async fn add_cell(&self, zone_id: &str, cell_id: String) -> Result<()>;

    /// Remove cell from platoon
    pub async fn remove_cell(&self, zone_id: &str, cell_id: &str) -> Result<()>;

    /// Set zone commander
    pub async fn set_commander(&self, zone_id: &str, commander_id: String) -> Result<()>;

    /// Delete platoon
    pub async fn delete_zone(&self, zone_id: &str) -> Result<()>;
}
```

**Testing:**
- Store/retrieve operations
- Query methods
- Concurrent updates
- CRDT merge via Ditto

---

### Task 2.3: Implement PlatoonMessageBus (1 day)

**File:** `cap-protocol/src/hierarchy/zone_messaging.rs`

```rust
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Zone-level message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlatoonMessage {
    /// Cell summary from cell leader
    SquadSummary {
        cell_id: String,
        member_count: usize,
        capabilities: Vec<Capability>,
        readiness: f32,
    },
    /// Commander announcement
    CommanderAnnounce {
        commander_id: String,
        timestamp: u64,
    },
    /// Zone status update
    StatusUpdate {
        status: PlatoonStatus,
        timestamp: u64,
    },
    /// Task assignment to squad
    TaskAssignment {
        target_cell: String,
        task: Task,
    },
}

/// Zone message bus (cell leader → zone level)
pub struct PlatoonMessageBus {
    zone_id: String,
    node_id: String,  // Must be cell leader
    outbound_queue: Arc<Mutex<VecDeque<PlatoonMessage>>>,
    subscribers: Arc<Mutex<Vec<PlatoonMessageHandler>>>,
}

impl PlatoonMessageBus {
    pub fn new(zone_id: String, node_id: String) -> Self;

    /// Publish message to zone (cell leaders only)
    pub async fn publish(&self, message: PlatoonMessage) -> Result<()>;

    /// Subscribe to zone messages
    pub fn subscribe(&mut self, handler: PlatoonMessageHandler);

    /// Process pending outbound messages
    pub async fn flush(&mut self) -> Result<()>;
}

pub type PlatoonMessageHandler = Arc<dyn Fn(PlatoonMessage) + Send + Sync>;
```

**Testing:**
- Message publishing
- Subscription and delivery
- Cell leader validation
- Message ordering

---

### Task 2.4: Implement PlatoonCoordinator (1.5 days)

**File:** `cap-protocol/src/hierarchy/zone.rs`

```rust
use crate::models::zone::{PlatoonConfig, PlatoonState};
use crate::models::cell::SquadState;
use crate::storage::zone_store::PlatoonStore;
use crate::{Error, Result};

/// Zone formation status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatoonFormationStatus {
    Forming,
    Ready,
    Degraded,
}

/// Zone formation coordinator (run by zone commander)
pub struct PlatoonCoordinator {
    pub zone_id: String,
    pub min_cells: usize,
    pub min_readiness: f32,
    pub status: PlatoonFormationStatus,
}

impl PlatoonCoordinator {
    pub fn new(zone_id: String, min_cells: usize, min_readiness: f32) -> Self;

    /// Check if zone formation is complete
    pub fn check_formation_complete(
        &mut self,
        cells: &[SquadState],
        commander_id: Option<&str>,
    ) -> Result<bool>;

    /// Aggregate capabilities from squads
    pub fn aggregate_capabilities(&self, cells: &[SquadState]) -> Vec<Capability>;

    /// Detect emergent zone-level capabilities
    pub fn detect_emergent_capabilities(&self, cells: &[SquadState]) -> Vec<Capability>;

    /// Check if zone can transition to operations phase
    pub fn can_transition_to_operations(&self) -> bool;

    /// Get formation metrics
    pub fn get_metrics(&self) -> PlatoonMetrics;
}

/// Zone formation metrics
#[derive(Debug, Clone)]
pub struct PlatoonMetrics {
    pub cell_count: usize,
    pub total_nodes: usize,
    pub average_cell_readiness: f32,
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
- 4 cells form platoon
- Commander election
- Cell summary propagation
- Capability aggregation
- 3-level hierarchy (node → cell → zone)

**Deliverable:** `cap-protocol/src/hierarchy/zone_tests.rs`

---

**Phase 2 Total: 5 days**

---

## Phase 3: E5.3/E5.4 - Priority & Flow Control (Days 10-14)

### Story: E5.3 - Priority-Based Routing

**Goal:** Extend priority system to hierarchical routing with per-hop priority queues.

### Task 3.1: Extend MessagePriority (0.5 days)

**File:** `cap-protocol/src/cell/messaging.rs` (extend existing)

- Already has 4 priority levels: Low, Normal, High, Critical
- Add hierarchical context: intra-cell vs. upward propagation
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
    cell_limiter: Arc<RateLimiter>,
    zone_limiter: Arc<RateLimiter>,
    /// Backpressure state
    backpressure: Arc<Mutex<BackpressureState>>,
    /// Message dropping policy
    drop_policy: MessageDropPolicy,
}

impl FlowController {
    pub fn new(
        cell_limit: BandwidthLimit,
        zone_limit: BandwidthLimit,
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
    pub cell_messages: Arc<AtomicU64>,
    pub zone_messages: Arc<AtomicU64>,
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

**Goal:** Implement cell merge/split and routing table maintenance.

### Task 4.1: Cell Merge/Split Logic (2 days)

**File:** `cap-protocol/src/hierarchy/maintenance.rs`

```rust
use crate::models::cell::SquadState;
use crate::models::zone::PlatoonState;
use crate::{Error, Result};

/// Hierarchy maintenance coordinator
pub struct HierarchyMaintainer {
    /// Cell size constraints
    pub min_cell_size: usize,
    pub max_cell_size: usize,
    /// Zone size constraints
    pub min_zone_cells: usize,
    pub max_zone_cells: usize,
}

impl HierarchyMaintainer {
    pub fn new(
        min_cell_size: usize,
        max_cell_size: usize,
        min_zone_cells: usize,
        max_zone_cells: usize,
    ) -> Self;

    /// Check if cell needs rebalancing
    pub fn needs_rebalance(&self, cell: &SquadState) -> RebalanceAction;

    /// Merge undersized squads
    pub async fn merge_cells(
        &self,
        cell1: &SquadState,
        cell2: &SquadState,
    ) -> Result<SquadState>;

    /// Split oversized squad
    pub async fn split_cell(&self, cell: &SquadState) -> Result<(SquadState, SquadState)>;

    /// Find merge candidate (nearest cell with capacity)
    pub fn find_merge_candidate(
        &self,
        cell: &SquadState,
        candidates: &[SquadState],
    ) -> Option<String>;

    /// Check if zone needs rebalancing
    pub fn needs_zone_rebalance(&self, zone: &PlatoonState) -> bool;

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
- Cell merge logic
- Cell split logic
- Merge candidate selection
- Edge cases (min size = 1, max size = capacity)

---

### Task 4.2: Routing Table Updates (1 day)

**File:** `cap-protocol/src/hierarchy/routing_table.rs` (extend)

```rust
impl RoutingTable {
    /// Update assignments after cell merge
    pub fn handle_cell_merge(
        &mut self,
        old_cell1: &str,
        old_cell2: &str,
        new_cell: &str,
    );

    /// Update assignments after cell split
    pub fn handle_cell_split(
        &mut self,
        old_cell: &str,
        new_cell1: &str,
        new_cell2: &str,
        cell1_nodes: &[String],
        cell2_nodes: &[String],
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
        cells: &[SquadState],
    ) -> Vec<RebalanceOperation>;

    /// Execute rebalancing plan
    pub async fn execute_rebalance(
        &self,
        operations: Vec<RebalanceOperation>,
    ) -> Result<RebalanceResult>;

    /// Wait for in-flight messages before rebalance
    async fn drain_messages(&self, cell_id: &str) -> Result<()>;
}

/// Rebalance operation
#[derive(Debug, Clone)]
pub enum RebalanceOperation {
    Merge { cell1: String, cell2: String },
    Split { cell: String },
    Reassign { node: String, from_cell: String, to_cell: String },
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
- Cell underflow → merge
- Cell overflow → split
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
   - 100 nodes discovery into 20 squads
   - 20 cells form 4 platoons
   - Validate routing table convergence
   - Test message flow at all levels

2. **Hierarchical Message Routing**
   - Node messages cell peers (allowed)
   - Node messages cross-cell (rejected)
   - Cell leader messages zone (allowed)
   - Non-leader messages zone (rejected)

3. **Dynamic Rebalancing**
   - Cell underflow triggers merge
   - Cell overflow triggers split
   - Routing table updates correctly
   - Messages continue to flow during rebalance

4. **Flow Control**
   - Bandwidth limits enforced
   - Backpressure propagation
   - Message dropping under overload
   - Priority handling

5. **Failure Scenarios**
   - Cell leader failure → new election
   - Zone commander failure → new commander
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
   - Compare to baseline (E4 cell-level only)

2. **Query Performance**
   - Routing table lookups
   - Hierarchical queries (node → zone)
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
| Phase 2 | E5.2 - Zone Level Aggregation | 5 days | 5-9 |
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
| Nodes only message cell peers | Routing rules unit tests, E2E rejection tests |
| Cell leaders message zone | Leader privilege tests, zone message delivery |
| Cross-cell messages rejected | Routing validation tests |
| Message complexity O(n log n) | Performance benchmarks with scaling tests |
| 100 nodes, 20 cells, 4 zones | E2E scenario with metrics collection |

---

## Dependencies

**Required for E5:**
- ✅ E1 (Foundation) - Complete
- ✅ E2 (CRDT Models) - Complete
- ✅ E3 (Discovery) - Complete
- ✅ E4 (Cell Formation) - Complete
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
1. Create Zone model and store
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
│   ├── zone_messaging.rs    (NEW - Phase 2)
│   ├── zone.rs              (NEW - Phase 2)
│   ├── flow_control.rs         (NEW - Phase 3)
│   ├── maintenance.rs          (NEW - Phase 4)
│   └── metrics.rs              (NEW - Phase 3)
├── models/
│   ├── zone.rs              (NEW - Phase 2)
│   └── node.rs             (MODIFY - add zone_id)
├── storage/
│   ├── zone_store.rs        (NEW - Phase 2)
│   ├── throttled_updates.rs    (NEW - Prerequisites)
│   └── mod.rs                  (MODIFY - add zone_store)
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
    Node,   // Intra-cell messaging
    Cell,      // Cell-to-zone messaging (leader only)
    Zone,    // Zone-level coordination
}
```

### Message Types
```rust
// Cell-level (existing)
pub enum SquadMessage { /* ... */ }

// Zone-level (new)
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
