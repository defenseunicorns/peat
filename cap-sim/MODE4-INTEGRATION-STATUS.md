# Mode 4 Integration Status

**Date:** 2025-11-08
**Status:** Infrastructure Complete, Queries TODO
**Branch:** e8-network-simulation-adr

---

## Summary

Successfully integrated Mode 4 (hierarchical aggregation) infrastructure into `cap_sim_node.rs`. The aggregation tasks spawn and run correctly, but member state queries need implementation before aggregation produces results.

---

## What Works ✅

### 1. Core Infrastructure
- **Hierarchical mode detection**: MODE environment variable detection working
- **Aggregation task spawning**: Both squad and platoon leader aggregation loops spawn successfully
- **Backend downcasting**: Added `as_any()` to DataSyncBackend trait for accessing DittoStore
- **Mode 4 topology**: Created platoon-24node-client-server-mode4.yaml with hierarchical configuration

### 2. Code Integration
**Files Modified:**
- `cap-protocol/examples/cap_sim_node.rs`
  - Added imports for StateAggregator, DittoStore, Arc
  - Added MODE environment variable detection (lines 206-213)
  - Created squad_leader_aggregation_loop() (lines 113-160)
  - Created platoon_leader_aggregation_loop() (lines 162-208)
  - Spawned aggregation tasks based on ROLE (lines 327-386)
  - Added "hierarchical" to mode matching (lines 437-441)
  - Updated subscription queries for hierarchical mode (lines 394-417)

- `cap-protocol/src/sync/traits.rs`
  - Added `as_any()` method to DataSyncBackend trait (lines 226-229)

- `cap-protocol/src/sync/ditto.rs`
  - Implemented `as_any()` for DittoBackend (lines 141-143)

- `cap-sim/topologies/platoon-24node-client-server-mode4.yaml`
  - 24-node topology with MODE=hierarchical
  - 3 squad leaders + 1 platoon leader + 20 members

### 3. Deployment Verification
**Confirmed Working:**
```
[squad-alpha-leader] Started squad leader aggregation for squad-alpha
[squad-bravo-leader] Started squad leader aggregation for squad-bravo
[squad-charlie-leader] Started squad leader aggregation for squad-charlie
[platoon-leader] Started platoon leader aggregation for platoon-1
```

- No "Invalid mode: hierarchical" errors
- Ditto initialization working with environment variables
- Aggregation loops running in background tasks
- Backend-specific feature access (DittoStore) working via downcasting

---

## What's Missing ❌

### 1. Member State Queries (CRITICAL)

**Location:** `cap-protocol/examples/cap_sim_node.rs:123-133`

**Current Placeholder:**
```rust
// Query squad members' NodeState from Ditto
let query = format!(
    "SELECT * FROM node_states WHERE squad_id == '{}'",
    squad_id
);

// TODO: Implement proper query to get member states
let member_states: Vec<(cap_schema::node::v1::NodeConfig, cap_schema::node::v1::NodeState)> = vec![];
```

**What's Needed:**
1. Implement actual DQL query to fetch member NodeStates from Ditto
2. Parse query results into Vec<(NodeConfig, NodeState)>
3. May need to verify collection name ("node_states" vs "sim_poc")
4. May need to ensure NodeState documents include squad_id field

**Impact:**
Without this, aggregation loops run but never produce SquadSummary or PlatoonSummary documents because the condition `if !member_states.is_empty()` is never true.

### 2. Collection Schema Questions

**Open Questions:**
- Does the `sim_poc` collection store NodeState documents?
- Do NodeState documents include `squad_id` for filtering?
- Should squad leaders query from a separate `node_states` collection?
- Do we need to create SquadSummary/PlatoonSummary collections in Ditto?

### 3. Writer Mode Timeout Issue

**Current Behavior:**
Nodes in hierarchical mode run writer_mode, which times out waiting for acknowledgments:
```
✗✗✗ POC FAILED: Timeout: Not all acknowledgments received ✗✗✗
```

**Possible Solutions:**
- Adjust timeout behavior for hierarchical mode
- Use reader_mode instead of writer_mode for leaders
- Disable acknowledgment requirements in hierarchical mode

---

## Next Steps for Protocol Team

### Priority 1: Implement Member State Queries
**Location:** `squad_leader_aggregation_loop()` in cap_sim_node.rs

**Required:**
1. Replace placeholder `vec![]` with actual DittoStore query
2. Query should fetch NodeState documents for squad members
3. Parse results into Vec<(NodeConfig, NodeState)>

**Example pseudo-code:**
```rust
// Get member states from Ditto
let store_arc = Arc::clone(&store);
let member_configs = vec![/* get from somewhere */];
let mut member_states = Vec::new();

for config in member_configs {
    if let Ok(Some(state)) = store_arc.get_node_state(&config.node_id).await {
        member_states.push((config, state));
    }
}
```

### Priority 2: Verify Collection Schema
1. Confirm sim_poc collection structure
2. Verify squad_id field exists in NodeState
3. Ensure SquadSummary/PlatoonSummary storage is configured

### Priority 3: Address Writer Mode Timeout
1. Consider alternative mode behavior for leaders
2. Adjust timeout settings for hierarchical mode
3. Test aggregation with actual member data

---

## Testing Verification

### Deploy Mode 4:
```bash
cd cap-sim
set -a && source .env && set +a
containerlab deploy -t topologies/platoon-24node-client-server-mode4.yaml --reconfigure
sleep 90

# Verify aggregation tasks started
docker logs clab-cap-platoon-mode4-client-server-squad-alpha-leader 2>&1 | grep "Started squad"
docker logs clab-cap-platoon-mode4-client-server-platoon-leader 2>&1 | grep "Started platoon"

# Check for aggregation output (will be empty until queries implemented)
docker logs clab-cap-platoon-mode4-client-server-squad-alpha-leader 2>&1 | grep "Aggregated squad"

# Cleanup
containerlab destroy -t topologies/platoon-24node-client-server-mode4.yaml
```

### Expected Results (After Query Implementation):
```
[squad-alpha-leader] ✓ Aggregated squad squad-alpha (7 members)
[squad-bravo-leader] ✓ Aggregated squad squad-bravo (8 members)
[squad-charlie-leader] ✓ Aggregated squad squad-charlie (8 members)
[platoon-leader] ✓ Aggregated platoon platoon-1 (3 squads, 24 total members)
```

---

## Architecture Notes

### Hierarchical Aggregation Flow:
1. **Regular soldiers** (MODE=reader): Publish individual NodeState every 5s
2. **Squad leaders** (MODE=hierarchical + ROLE=squad_leader):
   - Publish individual NodeState every 5s (via writer_mode)
   - Aggregate 7-8 member NodeStates → SquadSummary every 5s (via aggregation loop)
3. **Platoon leader** (MODE=hierarchical + ROLE=platoon_leader):
   - Aggregate 3 SquadSummaries → PlatoonSummary every 5s (via aggregation loop)
   - Subscribes ONLY to squad_summaries (not individual NodeStates)

### Expected Bandwidth Reduction:
- **Mode 2 (Full Replication)**: 576 ops (24 nodes × 24 ops each)
- **Mode 4 (Hierarchical)**: 27 ops (24 NodeStates + 3 SquadSummaries)
- **Reduction**: 95%

---

## Files Modified

```
cap-protocol/examples/cap_sim_node.rs      - Main integration (6 code sections)
cap-protocol/src/sync/traits.rs            - Added as_any() method
cap-protocol/src/sync/ditto.rs              - Implemented as_any()
cap-sim/topologies/platoon-24node-client-server-mode4.yaml - New topology
cap-sim/MODE4-INTEGRATION-STATUS.md         - This file
```

---

## Contact

For questions about Mode 4 integration, see:
- MODE4-IMPLEMENTATION-TASK.md - Original implementation guide
- cap-protocol/src/hierarchy/state_aggregation.rs - StateAggregator API
- cap-protocol/src/storage/HIERARCHICAL_SUMMARIES.md - DittoStore API
