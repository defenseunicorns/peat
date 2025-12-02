# E13 Delta Sync Metrics Fix Plan

## Executive Summary

Current test results show **46% outlier latencies** (multi-second measurements) due to fundamental flaws in:
1. **Timestamp semantics**: Only tracking creation time, not last modification
2. **Metrics calculation**: Using stale timestamps after container restarts
3. **Test isolation**: No clean slate between test runs (topology+mode combinations)

## Root Cause Analysis

### Issue 1: Single Timestamp Field
**Location**: `cap-protocol/src/storage/ditto_store.rs:492`

```rust
// CURRENT - Only creation timestamp:
doc["timestamp_us"] = serde_json::Value::Number(timestamp_us.into());
```

**Problem**: This timestamp is set ONCE at document creation and NEVER updated on subsequent writes.

**Impact**: Delta sync latency cannot be measured because we don't track when updates occur.

### Issue 2: Incorrect Latency Calculation
**Location**: `cap-protocol/examples/cap_sim_node.rs:1079-1084`

```rust
// CURRENT - Always uses creation timestamp:
let inserted_at_us = doc.get("timestamp_us").as_u64().unwrap_or(0) as u128;
let latency_us = received_at_us.saturating_sub(inserted_at_us);
```

**Problem**: After container restarts, persisted documents have old `inserted_at_us` timestamps.

**Impact**: Latency = restart_delay + time_since_original_creation (meaningless for delta sync)

### Issue 3: No Test Isolation
**Location**: Test runner scripts don't clear Ditto storage between test runs

**Problem**: `/tmp/cap_sim_*` persists across test runs (different topology/mode combinations)

**Impact**: Documents from previous test runs contaminate new tests

## Required Fixes

### Fix 1: Add Last Modified Tracking to Storage Layer

**File**: `cap-protocol/src/storage/ditto_store.rs`

**Changes**:
```rust
// In upsert_squad_summary() at line 492, REPLACE:
doc["timestamp_us"] = serde_json::Value::Number(timestamp_us.into());

// WITH:
// Track both creation and last modification for proper delta sync metrics
doc["created_at_us"] = serde_json::Value::Number(timestamp_us.into());
doc["last_modified_us"] = serde_json::Value::Number(timestamp_us.into());
doc["version"] = serde_json::Value::Number(1); // Increment on updates
```

**Note**: For true update tracking, we need to:
1. Check if document already exists
2. If exists, preserve `created_at_us`, increment `version`, update `last_modified_us`
3. If new, set all three fields

### Fix 2: Distinguish Creation vs Update Latency

**File**: `cap-protocol/examples/cap_sim_node.rs`

**Changes** at line 1070-1137 (in `handle_document_change` function):

```rust
// REPLACE current timestamp extraction:
let inserted_at_us = if let Some(ts_value) = doc.get("timestamp_us") {
    ts_value.as_u64().unwrap_or(0) as u128
} else {
    0
};

// WITH proper delta sync metrics:
let created_at_us = if let Some(ts) = doc.get("created_at_us") {
    ts.as_u64().unwrap_or(0) as u128
} else {
    // Fallback to old timestamp_us for backwards compatibility
    if let Some(ts) = doc.get("timestamp_us") {
        ts.as_u64().unwrap_or(0) as u128
    } else {
        0
    }
};

let last_modified_us = if let Some(ts) = doc.get("last_modified_us") {
    ts.as_u64().unwrap_or(0) as u128
} else {
    created_at_us // Fallback
};

let version = if let Some(v) = doc.get("version") {
    v.as_u64().unwrap_or(1)
} else {
    1
};

// Track which documents we've seen to distinguish first reception from updates
let is_first_reception = !test_doc_timestamps.contains(&created_at_us);

// Calculate appropriate latency based on context
let (latency_us, latency_type) = if is_first_reception {
    // First reception: measure from creation
    (received_at_us.saturating_sub(created_at_us), "creation")
} else {
    // Subsequent reception (update or recovery): measure from last modification
    (received_at_us.saturating_sub(last_modified_us), "update")
};

let latency_ms = latency_us as f64 / 1000.0;
```

### Fix 3: Enhanced Metrics Event Types

**File**: `cap-protocol/examples/cap_sim_node.rs`

**Add new event types** at line 86-93:

```rust
DocumentReceived {
    node_id: String,
    doc_id: String,
    created_at_us: u128,      // When document was first created
    last_modified_us: u128,   // When document was last updated
    received_at_us: u128,     // When we received it
    latency_us: u128,         // Propagation time
    latency_ms: f64,
    version: u64,             // Document version
    is_first_reception: bool, // true = creation sync, false = update/recovery sync
    latency_type: String,     // "creation", "update", or "recovery"
},
```

### Fix 4: Clean Test Isolation

**Files**:
- `labs/e12-comprehensive-empirical-validation/scripts/run-e13v2-full-matrix.sh`
- `labs/e12-comprehensive-empirical-validation/scripts/run-e13v3-mode4-hierarchical.sh`

**Add before each test scale** (after `containerlab destroy`, before `containerlab deploy`):

```bash
# Clean slate: Remove all Ditto storage from previous test runs
echo "Ensuring clean slate for test run..."
sudo rm -rf /tmp/cap_sim_* 2>/dev/null || true

# Also clean any persisted storage in containers (if they exist)
for container in $(docker ps -a --filter "name=clab-" --format "{{.Names}}" 2>/dev/null); do
    docker exec $container rm -rf /tmp/cap_sim_* 2>/dev/null || true
done

echo "✓ Storage cleaned"
```

## Testing the Fix

After applying fixes:

1. **Rebuild container**:
   ```bash
   cd /home/kit/Code/revolve/cap
   make sim-build
   ```

2. **Run single test** to verify metrics:
   ```bash
   cd labs/e12-comprehensive-empirical-validation/scripts
   ./run-e13v2-full-matrix.sh
   ```

3. **Verify metrics** show proper latency distribution:
   ```bash
   # Should see ONLY creation sync in first iteration (7-40ms)
   # Should see NO multi-second outliers from restarts
   grep "latency_ms" e13v2-*/p2p-limited-12node-1gbps/*.log | \
     jq -r '.latency_ms' | sort -n
   ```

## Expected Outcome

**Before Fix**:
- P50: 39ms (real)
- P75: 12,833ms (contaminated by restarts)
- P90: 17,827ms (contaminated)
- 46% outliers >1 second

**After Fix**:
- Creation sync latency: 7-40ms (majority of samples)
- Update sync latency: <10ms (delta propagation only)
- Recovery latency: Tracked separately
- 0% contaminated outliers

## Implementation Priority

1. **Critical**: Fix 4 (test isolation) - Can apply immediately without code changes
2. **High**: Fix 1 (storage timestamps) - Enables proper delta sync tracking
3. **High**: Fix 2 & 3 (metrics calculation) - Provides correct measurements
4. **Validation**: Rerun tests and verify latency distribution

## Files to Modify

1. `cap-protocol/src/storage/ditto_store.rs` (lines 487-495)
2. `cap-protocol/examples/cap_sim_node.rs` (lines 83-115, 1070-1230)
3. `labs/e12-comprehensive-empirical-validation/scripts/run-e13v2-full-matrix.sh`
4. `labs/e12-comprehensive-empirical-validation/scripts/run-e13v3-mode4-hierarchical.sh`

## Backwards Compatibility

All fixes include fallbacks to `timestamp_us` for backwards compatibility with existing test data.
