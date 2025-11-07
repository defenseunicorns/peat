# E8 Simulation Team: Protobuf Migration Handoff

**Date**: 2025-11-07
**PR**: #57 (Merged to main)
**Status**: Ready for Integration

## Overview

The `main` branch now includes a **complete protobuf migration** (ADR-012 Phase 5) that affects how you interact with CAP protocol models in the simulation environment. This document provides everything you need to update your simulation code.

---

## What Changed?

### Before (Hand-Written Structs)
```rust
use cap_protocol::models::{CellConfig, CellState, NodeConfig};

// Old way - direct struct construction
let config = CellConfig {
    id: "cell-1".to_string(),
    max_size: 12,
    min_size: 6,
    members: HashSet::new(),
};

let mut cell = CellState {
    config: config,
    members: HashSet::new(),
    capabilities: Vec::new(),
    // ...
};

cell.members.insert("soldier-1".to_string());
```

### After (Protobuf Types + Extension Traits)
```rust
use cap_protocol::models::{CellConfig, CellState, NodeConfig};
use cap_protocol::models::{CellConfigExt, CellStateExt, NodeConfigExt};

// New way - extension trait methods
let config = CellConfig::with_id("cell-1".to_string(), 12);

let mut cell = CellState::new(config);
cell.add_member("soldier-1".to_string());
```

**Key Change**: Types are now protobuf-generated, and you use **extension traits** for construction and manipulation.

---

## Migration Guide for E8 Simulation Code

### 1. Update Imports

**Add Extension Traits** to your imports:

```rust
// cap-sim/src/node.rs (example)

// Before
use cap_protocol::models::{CellConfig, CellState, NodeConfig};

// After
use cap_protocol::models::{
    CellConfig, CellState, NodeConfig,
    CellConfigExt, CellStateExt, NodeConfigExt,  // Add extension traits
};
```

**Why?** Extension traits provide `new()`, `add_member()`, etc. methods that aren't part of the protobuf-generated types.

### 2. Replace Direct Field Access with Extension Methods

#### Cell Operations

**Before**:
```rust
let mut cell = CellState {
    config: CellConfig::new(12),
    members: HashSet::new(),
    capabilities: Vec::new(),
    leader_id: None,
    // ...
};

cell.members.insert("soldier-1".to_string());
cell.members.insert("soldier-2".to_string());

if cell.members.contains("soldier-1") {
    // ...
}
```

**After**:
```rust
let config = CellConfig::new(12);
let mut cell = CellState::new(config);

cell.add_member("soldier-1".to_string());
cell.add_member("soldier-2".to_string());

if cell.is_member("soldier-1") {
    // ...
}
```

#### Node Operations

**Before**:
```rust
let mut node_config = NodeConfig {
    id: "uav-1".to_string(),
    platform_type: "uav".to_string(),
    capabilities: Vec::new(),
    // ...
};

node_config.capabilities.push(capability);
```

**After**:
```rust
let mut node_config = NodeConfig::new("uav".to_string());
node_config.set_id("uav-1".to_string());
node_config.add_capability(capability);
```

### 3. Replace HashSet with Vec

Protobuf uses `Vec<String>` for repeated fields (e.g., `members`), not `HashSet<String>`.

**Before**:
```rust
let members: HashSet<String> = cell.members.iter().cloned().collect();
for member in &cell.members {
    // HashSet iteration
}
```

**After**:
```rust
let members: Vec<String> = cell.members.clone();  // Already a Vec
for member in &cell.members {
    // Vec iteration
}
```

**Extension traits handle deduplication internally** when using methods like `add_member()`.

### 4. Handle Option-Wrapped Fields

Many fields are now `Option<T>` due to protobuf's optional semantics.

**Before**:
```rust
let timestamp: u64 = cell.timestamp;
let config_id = cell.config.id;
```

**After**:
```rust
use cap_protocol::models::Timestamp;

// Timestamp is now Option<Timestamp>
let timestamp_secs = cell.timestamp
    .as_ref()
    .map(|t| t.seconds)
    .unwrap_or(0);

// Config is now Option<CellConfig>
let config_id = cell.config
    .as_ref()
    .map(|c| c.id.as_str())
    .unwrap_or("<unknown>");

// Or use extension trait helper
let config_id = cell.get_id().unwrap_or("<unknown>");
```

### 5. Use Extension Trait Helpers

Extension traits provide convenience methods that hide protobuf complexity:

| Old Code | New Code |
|----------|----------|
| `cell.members.len()` | `cell.member_count()` |
| `cell.members.contains(&id)` | `cell.is_member(&id)` |
| `cell.members.len() >= cell.config.max_size` | `cell.is_full()` |
| `cell.members.is_empty()` | `cell.is_empty()` |
| `cell.config.id` | `cell.get_id().unwrap_or("<unknown>")` |

**Full list of methods**: See extension trait definitions in `cap-protocol/src/models/`.

---

## Common Patterns for Simulation

### Pattern 1: Creating Simulated Nodes

```rust
use cap_protocol::models::{
    NodeConfig, NodeConfigExt,
    Capability, CapabilityExt,
    CapabilityType,
};

fn create_simulated_uav(id: String) -> NodeConfig {
    let mut node = NodeConfig::new("uav".to_string());
    node.set_id(id.clone());

    // Add UAV capabilities
    node.add_capability(Capability::new(
        format!("{}-camera", id),
        "EO/IR Camera".to_string(),
        CapabilityType::Sensor,
        0.95,
    ));

    node.add_capability(Capability::new(
        format!("{}-isr", id),
        "ISR Package".to_string(),
        CapabilityType::Isr,
        0.90,
    ));

    node
}
```

### Pattern 2: Simulating Squad Formation

```rust
use cap_protocol::models::{
    CellConfig, CellConfigExt,
    CellState, CellStateExt,
};

fn simulate_squad_formation(nodes: Vec<NodeConfig>) -> CellState {
    let config = CellConfig::new(12)  // Max 12 nodes per squad
        .with_id("squad-1".to_string())
        .with_min_size(6);  // Min 6 nodes

    let mut cell = CellState::new(config);

    // Add nodes to cell
    for node in &nodes {
        if !cell.is_full() {
            cell.add_member(node.id.clone());

            // Aggregate node capabilities
            for cap in &node.capabilities {
                cell.add_capability(cap.clone());
            }
        }
    }

    // Set leader (assuming first node is squad leader)
    if let Some(leader_id) = nodes.first().map(|n| n.id.clone()) {
        cell.set_leader(leader_id).expect("Failed to set leader");
    }

    cell
}
```

### Pattern 3: CRDT Merge Simulation

```rust
use cap_protocol::models::{CellState, CellStateExt};

fn simulate_partition_heal(cell1: &mut CellState, cell2: &CellState) {
    // Merge cells after network partition heals
    cell1.merge(cell2);

    // Verify CRDT semantics preserved
    println!("Merged cell has {} members", cell1.member_count());
    println!("Merged cell has {} capabilities", cell1.capabilities.len());
}
```

### Pattern 4: Validating Cell Formation

```rust
use cap_protocol::models::{CellState, CellStateExt};

fn validate_cell_readiness(cell: &CellState) -> bool {
    // Check minimum requirements
    let has_enough_members = cell.member_count() >= 6;
    let has_leader = cell.leader_id.is_some();
    let is_valid = cell.is_valid();

    println!("Cell readiness:");
    println!("  Members: {} (need >= 6)", cell.member_count());
    println!("  Leader: {}", if has_leader { "✓" } else { "✗" });
    println!("  Valid: {}", if is_valid { "✓" } else { "✗" });

    has_enough_members && has_leader && is_valid
}
```

---

## Breaking Changes Checklist

### ✅ Must Update

- [ ] **Import extension traits** alongside model types
- [ ] **Replace direct field construction** with extension trait methods
- [ ] **Replace `.insert()`** on members with `.add_member()`
- [ ] **Replace `.contains()`** on members with `.is_member()`
- [ ] **Handle `Option<T>`** fields (timestamp, config, etc.)
- [ ] **Update timestamp handling** to use `Option<Timestamp>`

### ⚠️ Should Update (for best practices)

- [ ] **Use extension trait helpers** instead of direct field access
- [ ] **Use `.merge()`** for CRDT operations
- [ ] **Use `.is_full()`, `.is_empty()`** instead of manual checks
- [ ] **Use `.get_id()`** instead of accessing `config.id` directly

### ⏸️ No Change Required

- **Protocol behavior**: CRDT semantics unchanged
- **Ditto storage**: Still works with protobuf types
- **Test infrastructure**: E2E harness unchanged
- **Network simulation**: ContainerLab configs unchanged

---

## Example: Updating `cap_sim_node.rs`

### Before
```rust
use cap_protocol::models::{CellConfig, CellState, NodeConfig};
use cap_protocol::storage::{DittoStore, CellStore};

fn main() {
    // Create node config
    let mut node_config = NodeConfig {
        id: env::var("NODE_ID").unwrap(),
        platform_type: env::var("PLATFORM_TYPE").unwrap(),
        capabilities: Vec::new(),
    };

    // Create cell
    let mut cell_state = CellState {
        config: CellConfig {
            id: "cell-1".to_string(),
            max_size: 12,
            min_size: 6,
            members: HashSet::new(),
        },
        members: HashSet::new(),
        capabilities: Vec::new(),
        leader_id: None,
    };

    cell_state.members.insert(node_config.id.clone());
}
```

### After
```rust
use cap_protocol::models::{
    CellConfig, CellConfigExt,
    CellState, CellStateExt,
    NodeConfig, NodeConfigExt,
};
use cap_protocol::storage::{DittoStore, CellStore};

fn main() {
    // Create node config
    let node_id = env::var("NODE_ID").unwrap();
    let platform_type = env::var("PLATFORM_TYPE").unwrap();

    let mut node_config = NodeConfig::new(platform_type);
    node_config.set_id(node_id.clone());

    // Create cell
    let config = CellConfig::with_id("cell-1".to_string(), 12)
        .with_min_size(6);

    let mut cell_state = CellState::new(config);
    cell_state.add_member(node_id);
}
```

---

## Testing Your Changes

### 1. Run Unit Tests
```bash
cd cap-protocol
cargo test
```

**Expected**: All 330+ tests should pass.

### 2. Run E2E Tests
```bash
cd cap-protocol
make test-e2e
```

**Expected**: All distributed CRDT tests should pass.

### 3. Test Simulation Locally

Before deploying to ContainerLab, test your simulation code:

```bash
cd cap-sim
cargo build --release
./target/release/cap_sim_node
```

**Expected**: Node initializes with protobuf types, no compilation errors.

### 4. Deploy ContainerLab Topology

```bash
cd cap-sim
sudo containerlab deploy -t topologies/squad-12node.yaml --env-file ../.env
```

**Expected**: 12 nodes deploy, document syncs across all nodes within 15 seconds.

---

## Documentation References

### Extension Trait Definitions
- **Cell**: `cap-protocol/src/models/cell/mod.rs`
- **Node**: `cap-protocol/src/models/node.rs`
- **Zone**: `cap-protocol/src/models/zone.rs`
- **Capability**: `cap-protocol/src/models/capability.rs`
- **Operator**: `cap-protocol/src/models/operator.rs`

### Integration Tests
- **Cross-model**: `cap-protocol/tests/models_integration.rs`
- **Cell E2E**: `cap-protocol/tests/hierarchy_e2e.rs`
- **Load testing**: `cap-protocol/tests/load_testing_e2e.rs`

### Architecture Decisions
- **ADR-012**: Schema-Driven Development (protobuf migration)
- **ADR-002**: Beacon Storage Architecture (CRDT patterns)
- **ADR-008**: Network Simulation Layer (ContainerLab)

---

## Migration Steps for E8 Team

### Phase 1: Update Core Simulation Code (Week 1)
1. ✅ Update `cap-sim/src/node.rs` imports
2. ✅ Replace direct struct construction with extension traits
3. ✅ Update member/capability operations
4. ✅ Run local tests

### Phase 2: Update ContainerLab Integration (Week 1-2)
1. ✅ Test with existing `squad-12node.yaml` topology
2. ✅ Validate document sync across 12 nodes
3. ✅ Verify CRDT merge operations

### Phase 3: Scale to Company Topology (Week 2-3)
1. 🔲 Generate 112-node company topology
2. 🔲 Test with protobuf types at scale
3. 🔲 Validate O(n log n) message complexity
4. 🔲 Measure bandwidth with real protobuf serialization

### Phase 4: Network Impairment Testing (Week 3-4)
1. 🔲 Apply `netem` constraints (latency, packet loss)
2. 🔲 Test CRDT resilience with protobuf types
3. 🔲 Measure sync times under realistic conditions

---

## FAQ

### Q: Why extension traits instead of direct methods?
**A**: Rust's orphan rule prevents us from implementing methods on types defined in external crates. Extension traits let us add methods to protobuf-generated types without modifying the generated code.

### Q: Are CRDT semantics still guaranteed?
**A**: Yes! Extension trait methods implement the same CRDT operations:
- **OR-Set**: `add_member()`, `remove_member()`
- **LWW-Register**: `set_leader()`, `set_coordinator()`
- **G-Set**: `add_capability()`
- **PN-Counter**: `consume_fuel()`, `replenish_fuel()`

### Q: Will Ditto sync still work?
**A**: Yes! Protobuf types serialize to Ditto collections exactly the same way. The `DittoStore` integration is unchanged.

### Q: What happens to my custom structs in `cap-sim`?
**A**: If you have custom structs (e.g., `SimulationConfig`), you can keep them. Only CAP protocol models (Node, Cell, Zone) have changed.

### Q: Do I need to update ContainerLab YAML files?
**A**: No! Topology YAML files are unchanged. Only the Rust code that creates/manipulates models needs updating.

### Q: Can I use the old `HashSet<String>` for members?
**A**: No. Protobuf uses `Vec<String>` for repeated fields. But extension traits handle deduplication, so you get set semantics via methods like `add_member()`.

---

## Support

### Questions?
- **Slack**: `#cap-protocol` channel
- **GitHub Issues**: [kitplummer/cap/issues](https://github.com/kitplummer/cap/issues)
- **Docs**: [docs/INDEX.md](../docs/INDEX.md)

### Found a Bug?
Open an issue with:
- Simulation scenario (ContainerLab topology)
- Error message
- Code snippet showing protobuf usage

### Need Help Migrating?
Tag `@protobuf-migration-team` in Slack with:
- File path (e.g., `cap-sim/src/node.rs`)
- Code you're trying to migrate
- Error message (if any)

---

## Summary

**What changed**: CAP protocol models are now protobuf types with extension traits.

**What you need to do**:
1. Import extension traits
2. Use extension methods instead of direct field access
3. Handle `Option<T>` fields
4. Replace `HashSet` with `Vec` for members

**What didn't change**:
- CRDT semantics
- Ditto storage
- ContainerLab topologies
- Test infrastructure

**Timeline**: Aim to complete migration in 2-3 weeks before scaling to 112-node company topology.

---

**Last Updated**: 2025-11-07
**Reviewed By**: Protobuf Migration Team
**Status**: Ready for E8 Team Integration
