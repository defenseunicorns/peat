---
title: "PRD-003-sync-mode-enforcement: Sync Mode Enforcement (LatestOnly, WindowedHistory)"
status: Draft
issue: "#666"
epic: "#670 — QoS, TTL, and Garbage Collection"
adrs: [016, 019, 034]
---

## Implementation Spec: Sync Mode Enforcement (LatestOnly, WindowedHistory)

### Objective

Wire `SyncModeRegistry` into every sync path in `AutomergeSyncCoordinator` so that per-collection sync modes are actually enforced. Today the registry and enum exist but only the batch path (`create_batch_for_documents`) and the single-doc `initiate_sync_inner` consult it. The `WindowedHistory` arm falls through to full delta sync with a `// Phase 2` comment. This issue closes that gap: LatestOnly collections send state snapshots, WindowedHistory collections send time-filtered deltas, and FullHistory remains unchanged.

---

### Current State

| Component | File | Status |
|-----------|------|--------|
| `SyncMode` enum | `peat-mesh/src/qos/sync_mode.rs` | Complete (FullHistory, LatestOnly, WindowedHistory) |
| `SyncModeRegistry` | `peat-mesh/src/qos/sync_mode.rs` | Complete (per-collection defaults, runtime override) |
| `AutomergeSyncCoordinator` field | `peat-mesh/src/storage/automerge_sync.rs:524` | Exists (`sync_mode_registry: Arc<SyncModeRegistry>`) |
| `sync_mode_for_doc()` | `peat-mesh/src/storage/automerge_sync.rs:687` | Exists (extracts collection, queries registry) |
| `initiate_sync_inner()` | `peat-mesh/src/storage/automerge_sync.rs:775` | LatestOnly path works; WindowedHistory falls through to FullHistory |
| `create_batch_for_documents()` | `peat-mesh/src/storage/automerge_sync.rs:1473` | LatestOnly path works; WindowedHistory falls through to FullHistory |
| Wire format `0x02` (WindowedHistory) | `peat-mesh/src/storage/automerge_sync.rs:96` | Defined in `SyncMessageType` but never sent or parsed in `receive_sync_payload_from_stream` (line 1276 returns `Unknown`) |
| `apply_state_snapshot()` | `peat-mesh/src/storage/automerge_sync.rs:2196` | Complete (merge-on-receive) |
| `PeerSyncStats` | `peat-mesh/src/storage/automerge_sync.rs:475` | Tracks bytes/count but not per-mode breakdown |
| Receive path `0x02` handler | `peat-mesh/src/storage/automerge_sync.rs:1276` | Missing — falls into `other => Unknown` error |

**Key gap:** WindowedHistory is defined at every layer (enum, wire byte, registry defaults) but has zero functional implementation. LatestOnly is wired in `initiate_sync_inner` and `create_batch_for_documents` but not in the `sync_all_documents_with_peer` reconnection path, where the 300x savings actually matter.

---

### Implementation Steps

#### Step 1: Implement WindowedHistory send path

**File:** `peat-mesh/src/storage/automerge_sync.rs`

In `initiate_sync_inner()` (line 813), replace the current fallthrough:

```rust
// BEFORE (line 813-817):
SyncMode::FullHistory | SyncMode::WindowedHistory { .. } => {
    // WindowedHistory uses same path but receiver will filter (Phase 2)
    self.initiate_delta_sync(doc_key, peer_id, &doc).await
}

// AFTER:
SyncMode::FullHistory => {
    self.initiate_delta_sync(doc_key, peer_id, &doc).await
}
SyncMode::WindowedHistory { window_seconds } => {
    self.initiate_windowed_sync(doc_key, peer_id, &doc, window_seconds).await
}
```

Add a new method `initiate_windowed_sync()` that:
1. Calls `doc.save_after(window_cutoff_heads)` or, since Automerge does not natively support time-based filtering, uses the following approach:
   - Generate a full `doc.save()` (compact state)
   - Compute the cutoff timestamp: `SystemTime::now() - Duration::from_secs(window_seconds)`
   - Use `doc.get_changes(&[])` to get all changes, then filter to only those with timestamps >= cutoff
   - Re-encode only the filtered changes plus the base document state
2. Falls back to full delta sync if the window covers all history (no savings)
3. Sends using wire type `0x02` (WindowedHistory)

**Practical approach for Automerge:** Since Automerge changes don't have reliable wallclock timestamps, use the change's `timestamp()` field (Unix millis, set at creation). Filter changes where `change.timestamp() >= cutoff_millis`. If no changes are older than the window, fall through to FullHistory (no filtering needed).

```rust
async fn initiate_windowed_sync(
    &self,
    doc_key: &str,
    peer_id: EndpointId,
    doc: &Automerge,
    window_seconds: u64,
) -> Result<()> {
    let cutoff = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
        - (window_seconds as i64 * 1000);

    // Get all changes and filter by timestamp
    let all_changes = doc.get_changes(&[])?;
    let recent_changes: Vec<_> = all_changes
        .iter()
        .filter(|c| c.timestamp() >= cutoff)
        .collect();

    if recent_changes.len() == all_changes.len() {
        // Window covers all history, use normal delta sync
        return self.initiate_delta_sync(doc_key, peer_id, doc).await;
    }

    // Build a minimal document with only recent changes applied on top of
    // a base state. Send as WindowedHistory message type (0x02).
    // The payload is: [8 bytes: cutoff_millis][rest: doc.save() bytes]
    // Receiver uses the cutoff to understand what's included.
    let state_bytes = doc.save();
    self.send_windowed_snapshot(peer_id, doc_key, &state_bytes, cutoff).await
}
```

#### Step 2: Implement WindowedHistory receive path

**File:** `peat-mesh/src/storage/automerge_sync.rs`

In `receive_sync_payload_from_stream()` (around line 1276), add a `0x02` handler:

```rust
0x02 => {
    // WindowedHistory - treat as state snapshot with metadata
    tracing::debug!(
        "Received windowed history for {}: {} bytes",
        doc_key,
        buffer.len()
    );
    // WindowedHistory payload is functionally a state snapshot
    // (the sender already filtered). Apply the same merge logic.
    ReceivedSyncPayload::StateSnapshot(buffer)
}
```

This is the simplest correct approach: the sender has already filtered the history, so the receiver can treat it as a state snapshot and merge.

#### Step 3: Update batch sync path

**File:** `peat-mesh/src/storage/automerge_sync.rs`

In `create_batch_for_documents()` (line 1489-1506), split the WindowedHistory arm:

```rust
SyncMode::LatestOnly => {
    let state_bytes = doc.save();
    batch.add_snapshot(doc_key, state_bytes);
}
SyncMode::WindowedHistory { window_seconds } => {
    // Apply time-based filtering, then add as snapshot
    let state_bytes = doc.save();
    batch.add_entry(SyncEntry::new(
        doc_key.to_string(),
        SyncMessageType::WindowedHistory,
        state_bytes,
    ));
}
SyncMode::FullHistory => {
    let mut sync_state = SyncState::new();
    if let Some(message) = SyncDoc::generate_sync_message(&doc, &mut sync_state) {
        batch.add_delta(doc_key, &message);
    }
}
```

#### Step 4: Add per-mode metrics to PeerSyncStats

**File:** `peat-mesh/src/storage/automerge_sync.rs`

Extend `PeerSyncStats` (line 475):

```rust
pub struct PeerSyncStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub sync_count: u64,
    pub last_sync: Option<SystemTime>,
    pub failure_count: u64,
    // New fields for sync mode observability (Issue #666)
    pub latest_only_count: u64,
    pub latest_only_bytes: u64,
    pub windowed_count: u64,
    pub windowed_bytes: u64,
    pub full_history_count: u64,
    pub full_history_bytes: u64,
}
```

Update `send_state_snapshot()`, `initiate_delta_sync()`, and the new `initiate_windowed_sync()` to increment the appropriate counters after successful send.

#### Step 5: Add `SyncModeRegistry` configuration API

**File:** `peat-mesh/src/qos/sync_mode.rs`

The registry already supports `set()` and `remove()` at runtime. Add a bulk configuration method:

```rust
impl SyncModeRegistry {
    /// Configure multiple collections at once (e.g., from a config file)
    pub fn configure(&self, overrides: HashMap<String, SyncMode>) {
        let mut map = self.overrides.write().unwrap_or_else(|e| e.into_inner());
        for (collection, mode) in overrides {
            map.insert(collection, mode);
        }
    }

    /// Return a summary of effective modes for all known collections
    pub fn effective_modes(&self) -> HashMap<String, SyncMode> {
        let known = [
            "beacons", "platforms", "tracks", "nodes", "cells",
            "node_states", "squad_summaries", "platoon_summaries", "company_summaries",
            "contact_reports", "commands", "audit_logs", "alerts",
            "track_history", "capability_history",
        ];
        known.iter().map(|c| (c.to_string(), self.get(c))).collect()
    }
}
```

#### Step 6: Verify sync_all_documents_with_peer uses mode-aware path

**File:** `peat-mesh/src/storage/automerge_sync.rs`

`sync_all_documents_with_peer()` (line 1432) already calls `sync_document_with_peer()` which calls `initiate_sync()` which calls `initiate_sync_inner()`. Since Step 1 fixes `initiate_sync_inner()`, the reconnection path is automatically fixed. **No code change needed here**, but this must be verified in testing.

---

### Testing Plan

#### Unit Tests (in `peat-mesh/src/qos/sync_mode.rs`)

1. **`test_windowed_history_window_seconds`** — Verify `window_seconds()` returns correct values and `None` for non-windowed modes.
2. **`test_registry_configure_bulk`** — Test bulk configuration method.
3. **`test_registry_effective_modes`** — Verify effective modes include overrides.

#### Unit Tests (in `peat-mesh/src/storage/automerge_sync.rs`)

4. **`test_sync_mode_for_doc_all_collections`** — Verify `sync_mode_for_doc()` returns correct mode for each known collection pattern (`beacons:node-1`, `commands:cmd-1`, `track_history:t-1`, etc.)
5. **`test_windowed_history_wire_format`** — Encode a `SyncEntry` with `SyncMessageType::WindowedHistory`, decode it, verify round-trip.
6. **`test_sync_batch_with_windowed_entry`** — Add a windowed history entry to a batch, encode/decode, verify type is preserved.

#### Integration Tests (new file: `peat-mesh/tests/sync_mode_integration.rs`)

7. **`test_latest_only_reconnection_traffic`** — Set up two nodes with beacons collection. Node A writes 300 beacon updates while disconnected. On reconnection, measure that exactly 1 state snapshot (not 300 deltas) is sent. Assert total bytes < 2x single doc.save() size.
8. **`test_full_history_preserves_all_deltas`** — Set up two nodes with contact_reports collection. Node A writes 50 contact reports. On sync, verify all 50 are present on Node B with full change history (`doc.get_changes(&[]).len() >= 50`).
9. **`test_windowed_history_filters_old`** — Set up two nodes with track_history collection (window=5s). Write changes spanning 10 seconds (some older, some recent). On sync, verify only changes within the 5-second window arrive. (This may need a mock clock or short windows.)
10. **`test_mode_switch_at_runtime`** — Start syncing beacons as LatestOnly, switch to FullHistory via `registry.set()`, verify next sync uses delta protocol.
11. **`test_mixed_batch_sync`** — Create a batch containing beacons (LatestOnly), commands (FullHistory), and track_history (WindowedHistory). Verify each entry has correct `SyncMessageType` in the encoded batch.

---

### Acceptance Criteria

- [ ] Beacons sync as LatestOnly: reconnection after 5 min sends 1 state snapshot per document, not N deltas
- [ ] Contact reports sync as FullHistory: all deltas preserved, full change history available on receiver
- [ ] WindowedHistory(300) on track_history: only changes within the last 5 minutes are included in sync payload
- [ ] Wire type `0x02` is sent for WindowedHistory and handled on receive without error
- [ ] `PeerSyncStats` tracks per-mode message counts and byte totals
- [ ] Runtime mode changes via `SyncModeRegistry::set()` take effect on next sync cycle
- [ ] No regression: existing FullHistory collections (commands, contact_reports, audit_logs) continue to sync all deltas
- [ ] Measurable reduction in reconnection sync traffic: integration test asserts LatestOnly payload < 5% of equivalent FullHistory payload for 300+ accumulated changes

---

### Estimated Effort

| Task | Lines | Effort |
|------|-------|--------|
| Step 1: WindowedHistory send path | ~50 | 2-3 hours |
| Step 2: WindowedHistory receive path | ~10 | 30 min |
| Step 3: Batch sync path update | ~15 | 30 min |
| Step 4: Per-mode metrics | ~30 | 1 hour |
| Step 5: Registry configuration API | ~25 | 30 min |
| Step 6: Verify reconnection path | 0 (testing only) | 30 min |
| Unit tests | ~120 | 2 hours |
| Integration tests | ~250 | 3-4 hours |
| **Total** | **~500** | **~2-3 days** |

The core enforcement change is small (~100 lines of production code). The majority of effort is in integration tests that verify the 300x traffic reduction claim with real Automerge documents over simulated reconnection scenarios.
