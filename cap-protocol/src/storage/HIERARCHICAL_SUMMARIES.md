# Hierarchical Summary Storage (E11.2)

## Overview

DittoStore now supports storage and retrieval of hierarchical aggregation summaries (SquadSummary, PlatoonSummary) to enable Mode 3 (CAP Differential) testing.

## API

### Squad Summaries

```rust
use cap_protocol::storage::DittoStore;
use cap_schema::hierarchy::v1::SquadSummary;

// Store a squad summary
let summary: SquadSummary = /* ... */;
store.upsert_squad_summary("squad-alpha", &summary).await?;

// Retrieve a squad summary
let retrieved = store.get_squad_summary("squad-alpha").await?;
match retrieved {
    Some(summary) => println!("Found: {}", summary.squad_id),
    None => println!("Not found"),
}
```

### Platoon Summaries

```rust
use cap_schema::hierarchy::v1::PlatoonSummary;

// Store a platoon summary
let summary: PlatoonSummary = /* ... */;
store.upsert_platoon_summary("platoon-1", &summary).await?;

// Retrieve a platoon summary
let retrieved = store.get_platoon_summary("platoon-1").await?;
```

## Storage Details

- **Encoding**: Protobuf messages are encoded to bytes, then base64-encoded for JSON storage
- **Collections**:
  - `squad_summaries` - SquadSummary documents
  - `platoon_summaries` - PlatoonSummary documents
- **Document Structure**:
  ```json
  {
    "_id": "squad-alpha",
    "squad_id": "squad-alpha",
    "leader_id": "node-1",
    "member_count": 8,
    "data": "base64-encoded-protobuf-bytes",
    "type": "squad_summary"
  }
  ```

## Integration with StateAggregator

For Mode 3 testing, use StateAggregator to create summaries, then store them:

```rust
use cap_protocol::hierarchy::StateAggregator;
use cap_protocol::storage::DittoStore;

// 1. Aggregate squad state from member NodeStates
let squad_summary = StateAggregator::aggregate_squad(
    "squad-alpha",
    "node-1",  // leader_id
    members,   // Vec<(NodeConfig, NodeState)>
)?;

// 2. Store the aggregated summary
store.upsert_squad_summary("squad-alpha", &squad_summary).await?;

// 3. Leader publishes summary (via Ditto sync)
// Platoon leader can now query squad_summaries collection

// 4. Platoon leader retrieves squad summaries
let squad_alpha = store.get_squad_summary("squad-alpha").await?;
let squad_bravo = store.get_squad_summary("squad-bravo").await?;

// 5. Aggregate platoon from squad summaries
let platoon_summary = StateAggregator::aggregate_platoon(
    "platoon-1",
    "node-1",
    vec![squad_alpha.unwrap(), squad_bravo.unwrap()],
)?;

// 6. Store platoon summary
store.upsert_platoon_summary("platoon-1", &platoon_summary).await?;
```

## Testing

See `cap-protocol/src/storage/ditto_store.rs`:
- `test_squad_summary_storage()` - Example squad summary test
- `test_platoon_summary_storage()` - Example platoon summary test

## Next Steps for Experiments Team

To implement Mode 3 (CAP Differential) in your framework:

1. **Baseline (Mode 1)**: Full NodeState replication (no filtering)
2. **Mode 2 (CAP Full)**: CRDT-enabled NodeState replication (no aggregation)
3. **Mode 3 (CAP Differential)**:
   - Squad members → Squad leader (full NodeState)
   - Squad leaders → Platoon leader (SquadSummary only)
   - Platoon leaders → Company leader (PlatoonSummary only)

This achieves O(log n) message complexity vs O(n²) in Mode 1/2.

## References

- [ADR-015](../../../docs/adr/015-experimental-validation-hierarchical-aggregation.md) - Experimental validation plan
- [hierarchy.proto](../../../cap-schema/proto/hierarchy.proto) - Schema definitions
- [state_aggregation.rs](../../hierarchy/state_aggregation.rs) - Aggregation logic
