# Mode 4 Implementation Task

**Status:** Ready to implement
**Priority:** P1 - Critical path for 95% bandwidth reduction validation
**Estimated Effort:** 2-3 hours
**Prerequisites:** ✅ E12 merged (commit 724f012)

---

## Objective

Integrate E12 hierarchical state aggregation into `cap_sim_node.rs` to enable Mode 4 testing.

**Expected Result:**
- Squad leaders aggregate 8 NodeStates → 1 SquadSummary every 5 seconds
- Platoon leader aggregates 3 SquadSummaries → 1 PlatoonSummary every 5 seconds
- 95% bandwidth reduction (27 ops vs 576 ops for 24 nodes)

---

## Implementation Checklist

### 1. Add Imports to cap_sim_node.rs

```rust
use cap_protocol::hierarchy::StateAggregator;
use cap_protocol::storage::DittoStore;
use cap_schema::hierarchy::v1::{SquadSummary, PlatoonSummary, NodeConfig, NodeState};
use std::collections::HashMap;
```

**File:** `peat-protocol/examples/cap_sim_node.rs`
**Location:** Add after line 50 (existing imports)

### 2. Add MODE Environment Variable Detection

```rust
// After line ~190 (where CAP_FILTER_ENABLED is read)
let hierarchical_mode = std::env::var("MODE")
    .unwrap_or_else(|_| String::new())
    .to_lowercase() == "hierarchical";

if hierarchical_mode {
    println!("[{}] MODE 4: Hierarchical aggregation enabled", node_id);
}
```

**File:** `peat-protocol/examples/cap_sim_node.rs`
**Location:** ~Line 195 (after CAP_FILTER_ENABLED detection)

### 3. Implement Squad Leader Aggregation Loop

**Location:** Add new async function before `main()`

```rust
async fn squad_leader_aggregation_loop(
    backend: Arc<DittoBackend>,
    squad_id: String,
    node_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = DittoStore::new(backend);

    loop {
        // 1. Query squad members' NodeState from Ditto
        // Note: Implement query_squad_members() or use raw DQL query
        let query = format!(
            "SELECT * FROM node_states WHERE squad_id == '{}'",
            squad_id
        );

        // Get member states (simplified - real implementation needs proper querying)
        let member_states: Vec<(NodeConfig, NodeState)> = vec![]; // TODO: Implement query

        if !member_states.is_empty() {
            // 2. Aggregate into SquadSummary
            match StateAggregator::aggregate_squad(
                &squad_id,
                &node_id,
                member_states,
            ) {
                Ok(squad_summary) => {
                    // 3. Publish to squad_summaries collection
                    if let Err(e) = store.upsert_squad_summary(&squad_id, &squad_summary).await {
                        eprintln!("[{}] Failed to upsert squad summary: {}", node_id, e);
                    } else {
                        println!(
                            "[{}] ✓ Aggregated squad {} ({} members)",
                            node_id, squad_id, squad_summary.member_count
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[{}] Failed to aggregate squad: {}", node_id, e);
                }
            }
        }

        // Wait 5 seconds before next aggregation
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
```

### 4. Implement Platoon Leader Aggregation Loop

**Location:** Add new async function after squad_leader_aggregation_loop()

```rust
async fn platoon_leader_aggregation_loop(
    backend: Arc<DittoBackend>,
    platoon_id: String,
    node_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = DittoStore::new(backend);

    loop {
        // 1. Query squad summaries from Ditto
        let squad_ids = vec!["squad-alpha", "squad-bravo", "squad-charlie"];
        let mut squad_summaries = Vec::new();

        for squad_id in &squad_ids {
            if let Ok(Some(summary)) = store.get_squad_summary(squad_id).await {
                squad_summaries.push(summary);
            }
        }

        if squad_summaries.len() == 3 {
            // 2. Aggregate into PlatoonSummary
            match StateAggregator::aggregate_platoon(
                &platoon_id,
                &node_id,
                squad_summaries,
            ) {
                Ok(platoon_summary) => {
                    // 3. Publish to platoon_summaries collection
                    if let Err(e) = store.upsert_platoon_summary(&platoon_id, &platoon_summary).await {
                        eprintln!("[{}] Failed to upsert platoon summary: {}", node_id, e);
                    } else {
                        println!(
                            "[{}] ✓ Aggregated platoon {} ({} squads, {} total members)",
                            node_id, platoon_id, platoon_summary.squad_count, platoon_summary.total_member_count
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[{}] Failed to aggregate platoon: {}", node_id, e);
                }
            }
        }

        // Wait 5 seconds before next aggregation
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
```

### 5. Spawn Aggregation Tasks in main()

**Location:** Add in main(), after backend initialization (~line 300)

```rust
// Spawn hierarchical aggregation tasks based on role
if hierarchical_mode {
    let role = std::env::var("ROLE").unwrap_or_else(|_| "soldier".to_string());

    match role.as_str() {
        "squad_leader" => {
            let squad_id = std::env::var("SQUAD_ID")
                .unwrap_or_else(|_| panic!("SQUAD_ID required for squad_leader role"));

            let backend_clone = Arc::clone(&backend);
            let node_id_clone = node_id.clone();
            let squad_id_clone = squad_id.clone();

            tokio::spawn(async move {
                if let Err(e) = squad_leader_aggregation_loop(
                    backend_clone,
                    squad_id_clone,
                    node_id_clone,
                ).await {
                    eprintln!("Squad leader aggregation failed: {}", e);
                }
            });

            println!("[{}] Started squad leader aggregation for {}", node_id, squad_id);
        }
        "platoon_leader" => {
            let platoon_id = std::env::var("PLATOON_ID")
                .unwrap_or_else(|_| "platoon-1".to_string());

            let backend_clone = Arc::clone(&backend);
            let node_id_clone = node_id.clone();
            let platoon_id_clone = platoon_id.clone();

            tokio::spawn(async move {
                if let Err(e) = platoon_leader_aggregation_loop(
                    backend_clone,
                    platoon_id_clone,
                    node_id_clone,
                ).await {
                    eprintln!("Platoon leader aggregation failed: {}", e);
                }
            });

            println!("[{}] Started platoon leader aggregation for {}", node_id, platoon_id);
        }
        _ => {
            // Regular soldiers don't aggregate
        }
    }
}
```

### 6. Update Subscription Queries for Hierarchical Mode

**Location:** Modify existing subscription logic (~line 230)

```rust
// Existing CAP filter logic
let subscription_query = if cap_filter_enabled {
    if hierarchical_mode && role == "platoon_leader" {
        // Platoon leaders ONLY subscribe to squad_summaries, not individual NodeStates
        println!("[{}]   → Subscribing to squad_summaries (hierarchical mode)", node_id);
        Query::Custom("collection_name == 'squad_summaries'".to_string())
    } else {
        // Existing CAP-filtered query for soldiers and squad leaders
        println!("[{}]   → Using CAP-filtered query for role: {}", node_id, node_type);
        Query::Custom(format!(
            "public == true OR CONTAINS(authorized_roles, '{}')",
            node_type
        ))
    }
} else {
    // Existing full replication
    println!("[{}]   → Using full replication (Query::All)", node_id);
    Query::All
};
```

### 7. Create Mode 4 Topology File

**File:** `peat-sim/topologies/platoon-24node-client-server-mode4.yaml`

Copy from Mode 3 topology and add:

```yaml
# For all nodes:
env:
  MODE: "hierarchical"  # Enable Mode 4
  CAP_FILTER_ENABLED: "true"  # Keep filtering enabled

# For squad leaders, add:
env:
  ROLE: "squad_leader"
  SQUAD_ID: "squad-alpha"  # or squad-bravo, squad-charlie

# For platoon leader, add:
env:
  ROLE: "platoon_leader"
  PLATOON_ID: "platoon-1"
```

**Command:**
```bash
cd peat-sim/topologies
cp platoon-24node-client-server-mode3.yaml platoon-24node-client-server-mode4.yaml
# Then manually add MODE: "hierarchical" to all nodes
```

### 8. Build and Test

```bash
# Rebuild with Mode 4 integration
make sim-build

# Test Mode 4
cd peat-sim
containerlab deploy -t topologies/platoon-24node-client-server-mode4.yaml
sleep 90

# Verify aggregation logs
docker logs clab-cap-platoon-mode4-client-server-squad-alpha-leader | grep "Aggregated squad"
docker logs clab-cap-platoon-mode4-client-server-platoon-leader | grep "Aggregated platoon"

# Cleanup
containerlab destroy -t topologies/platoon-24node-client-server-mode4.yaml
```

---

## Validation Criteria

### Squad Leader Verification
```bash
grep "Aggregated squad" logs/squad-*-leader.log
# Expected: "Aggregated squad squad-alpha (8 members)" every 5 seconds
```

### Platoon Leader Verification
```bash
grep "Aggregated platoon" logs/platoon-leader.log
# Expected: "Aggregated platoon platoon-1 (3 squads, 24 total members)" every 5 seconds
```

### Bandwidth Reduction
```bash
# Compare sync operation counts:
# Mode 2: 576 ops (24 × 24)
# Mode 4: 27 ops (24 individual NodeStates + 3 SquadSummaries)
# Expected: 95% reduction
```

---

## Known Issues / TODOs

1. **NodeState Collection:** Need to verify how to query member NodeStates from Ditto
   - May need to add `node_states` collection to schema
   - Or use existing document queries with proper filtering

2. **SQUAD_ID Mapping:** Need to ensure each node knows its squad_id
   - Already in topology env vars
   - Just needs to be passed through

3. **Subscription Collections:** May need separate collections for:
   - `node_states` (individual soldier state)
   - `squad_summaries` (squad leader aggregations)
   - `platoon_summaries` (platoon leader aggregations)

4. **Error Handling:** Aggregation loops need robust error handling
   - Handle missing squad members gracefully
   - Continue aggregating even if some data is unavailable

---

## Success Metrics

- ✅ Squad leaders log "Aggregated squad..." every 5 seconds
- ✅ Platoon leader logs "Aggregated platoon..." every 5 seconds
- ✅ Mode 4 shows ~27 total sync operations (vs 576 in Mode 2)
- ✅ Convergence time <30 seconds
- ✅ No crashes or panics in aggregation loops

---

## Files to Modify

1. `peat-protocol/examples/cap_sim_node.rs` - Main integration work
2. `peat-sim/topologies/platoon-24node-client-server-mode4.yaml` - New topology
3. `peat-sim/test-four-way-comparison.sh` - Optional: extend test script

---

## References

- MODE4-INTEGRATION-GUIDE.md - High-level integration guide
- peat-protocol/src/hierarchy/state_aggregation.rs - StateAggregator API
- peat-protocol/src/storage/HIERARCHICAL_SUMMARIES.md - Storage API docs
- peat-schema/proto/hierarchy.proto - Protobuf types
