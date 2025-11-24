# Lab 4 Phase 4: Comprehensive Document Reception Tracking

**Date**: 2025-11-24
**Status**: Implementation in progress

---

## Implementation Strategy

### Goal
Add `DocumentReceived` metrics at ALL hierarchy tiers to measure complete propagation paths:

1. **Soldier tier**: Observe peer NodeState documents + SquadSummary from leader
2. **Squad leader tier**: Observe soldier NodeState documents + peer SquadSummaries
3. **Platoon leader tier**: Already has DocumentReceived from Ditto backend
4. **Company leader tier**: Will observe platoon summaries

### Approach

**Pattern** (from cap_sim_node.rs):
```rust
// 1. Create query for documents to observe
let query = Query::All;  // or specific filter

// 2. Create observer stream
let mut change_stream = backend.document_store().observe("sim_poc", &query)?;

// 3. Spawn task to handle events
tokio::spawn(async move {
    while let Ok(Some(change)) = change_stream.next().await {
        match change {
            ChangeEvent::Initial { documents } => {
                // Handle initial document sync
            }
            ChangeEvent::Updated { document, .. } => {
                // Handle document updates - MAIN PROPAGATION TRACKING
            }
            ChangeEvent::Removed { .. } => {
                // Handle removals
            }
        }
    }
});
```

### Code Modifications

#### 1. Soldier Tier (soldier_capability_mode)

**Add after soldier sends updates** (around line 1644):

```rust
// Spawn observer for peer NodeState documents (lateral propagation)
let soldier_observer_backend = backend.clone();
let soldier_node_id = node_id.to_string();
tokio::spawn(async move {
    println!("METRICS: [{}] Starting peer NodeState observer...", soldier_node_id);

    let query = Query::All;
    if let Ok(mut stream) = soldier_observer_backend.document_store().observe("sim_poc", &query) {
        while let Ok(Some(change)) = stream.next().await {
            match change {
                ChangeEvent::Updated { document, .. } => {
                    let received_at_us = now_micros();
                    if let Some(doc_id) = &document.id {
                        // Track peer soldier documents
                        if doc_id.starts_with("sim_doc_") && doc_id != &format!("sim_doc_{}", soldier_node_id) {
                            // Extract timestamps
                            let created_at_us = document.get("timestamp_us")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u128)
                                .unwrap_or(0);

                            if created_at_us > 0 {
                                let latency_us = received_at_us.saturating_sub(created_at_us);
                                let latency_ms = latency_us as f64 / 1000.0;

                                log_metrics(&MetricsEvent::DocumentReceived {
                                    node_id: soldier_node_id.clone(),
                                    doc_id: doc_id.clone(),
                                    created_at_us,
                                    last_modified_us: created_at_us,
                                    received_at_us,
                                    latency_us,
                                    latency_ms,
                                    version: 1,
                                    is_first_reception: false,
                                    latency_type: "peer_soldier".to_string(),
                                });
                            }
                        }

                        // Track squad summary from leader (downward propagation)
                        if doc_id.contains("-summary") {
                            let created_at_us = document.get("created_at_us")
                                .or_else(|| document.get("timestamp_us"))
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u128)
                                .unwrap_or(0);

                            if created_at_us > 0 {
                                let latency_us = received_at_us.saturating_sub(created_at_us);
                                let latency_ms = latency_us as f64 / 1000.0;

                                log_metrics(&MetricsEvent::DocumentReceived {
                                    node_id: soldier_node_id.clone(),
                                    doc_id: doc_id.clone(),
                                    created_at_us,
                                    last_modified_us: created_at_us,
                                    received_at_us,
                                    latency_us,
                                    latency_ms,
                                    version: 1,
                                    is_first_reception: false,
                                    latency_type: "squad_summary_downward".to_string(),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
});
```

#### 2. Squad Leader Tier

**Add to squad_leader_aggregation_loop** (around line 473):

```rust
// Spawn observer for soldier NodeState documents (upward propagation)
let squad_observer_backend = backend.clone();
let squad_node_id = node_id.to_string();
let squad_members_clone = squad_members.clone();
tokio::spawn(async move {
    println!("METRICS: [{}] Starting soldier NodeState observer...", squad_node_id);

    let query = Query::All;
    if let Ok(mut stream) = squad_observer_backend.document_store().observe("sim_poc", &query) {
        while let Ok(Some(change)) = stream.next().await {
            match change {
                ChangeEvent::Updated { document, .. } => {
                    let received_at_us = now_micros();
                    if let Some(doc_id) = &document.id {
                        // Track soldier documents from squad members
                        if doc_id.starts_with("sim_doc_") {
                            // Check if this is from a squad member
                            let is_squad_member = squad_members_clone.iter()
                                .any(|member| doc_id == &format!("sim_doc_{}", member));

                            if is_squad_member {
                                let created_at_us = document.get("timestamp_us")
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u128)
                                    .unwrap_or(0);

                                if created_at_us > 0 {
                                    let latency_us = received_at_us.saturating_sub(created_at_us);
                                    let latency_ms = latency_us as f64 / 1000.0;

                                    log_metrics(&MetricsEvent::DocumentReceived {
                                        node_id: squad_node_id.clone(),
                                        doc_id: doc_id.clone(),
                                        created_at_us,
                                        last_modified_us: created_at_us,
                                        received_at_us,
                                        latency_us,
                                        latency_ms,
                                        version: 1,
                                        is_first_reception: false,
                                        latency_type: "soldier_to_squad_leader".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
});
```

#### 3. Add created_at_us to Summary Documents

**Modify SquadSummary creation** (around line 583):

```rust
// Add created_at_us timestamp to summary
summary_doc.insert("created_at_us", serde_json::json!(now_micros()));
```

**Modify PlatoonSummary creation** (similar change):

```rust
summary_doc.insert("created_at_us", serde_json::json!(now_micros()));
```

---

## Key Implementation Details

### Timestamp Fields

**NodeState documents** (soldiers):
- `timestamp_us`: When soldier created the update
- Use this as `created_at_us` for propagation tracking

**Summary documents** (squad/platoon):
- Need to ADD `created_at_us` field when creating summary
- Currently only have processing_time_us in AggregationCompleted event

### Latency Types

To distinguish different propagation paths:

```rust
latency_type: "peer_soldier"                // Soldier → Soldier (same squad)
latency_type: "soldier_to_squad_leader"     // Soldier → Squad Leader
latency_type: "squad_summary_downward"      // Squad Leader → Soldier
latency_type: "squad_to_platoon_leader"     // Squad Leader → Platoon Leader (already have)
latency_type: "platoon_to_company_leader"   // Platoon → Company
```

### Observer Lifecycle

**Challenge**: Observers need to run for duration of test
**Solution**: Spawn as independent tokio tasks, let them run until process exits

**Pattern**:
```rust
tokio::spawn(async move {
    // Runs independently, doesn't block main logic
    // Exits when process exits
});
```

---

## Testing Validation

### Quick Test Expectations

After implementation, `quick-test-lab4.sh` should show:

```
Squad leader aggregation operations: 9017
Soldier peer document receptions: ~60-120 (depends on timing)
Soldier receives squad summary: ~3-10 (per soldier)
Squad leader receives soldier NodeState: ~60+ (6 soldiers × 10 updates)
```

### Metrics to Extract

**Upward Propagation**:
- Soldier → Squad Leader latency (P50/P95)
- Squad Leader → Platoon Leader latency (P50/P95) ✅ already have

**Lateral Propagation**:
- Soldier → Peer Soldier latency (P50/P95) within squad
- Soldier → Peer Soldier latency (P50/P95) across squads

**Downward Propagation**:
- Squad Leader → Soldier latency (P50/P95)

---

## Next Steps

1. ✅ Plan implementation strategy
2. ⏳ Implement soldier tier observation
3. ⏳ Implement squad leader tier observation
4. ⏳ Add created_at_us to summary documents
5. ⏳ Rebuild Docker image
6. ⏳ Run quick validation test
7. ⏳ Verify all metrics appear in logs
8. ⏳ Run full Lab 4 test suite
