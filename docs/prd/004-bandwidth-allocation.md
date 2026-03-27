---
title: "PRD-004-bandwidth-allocation: Bandwidth Allocation to Sync Transport"
status: Draft
issue: "#665"
epic: "#670 — QoS, TTL, and Garbage Collection"
adrs: [016, 019, 034]
---

## Implementation Spec: Wire Bandwidth Allocation to Sync Transport Layer

### Objective

Connect the existing QoS bandwidth allocation, priority queuing, and preemption subsystems to the actual sync/transport hot path so that data priority is enforced at the wire level, not just classified and stored.

---

### Current State

**Implemented (all in peat-mesh and re-exported through peat-protocol):**

| Component | Location | Status |
|---|---|---|
| `BandwidthAllocation` / `BandwidthQuota` / `TokenBucket` | `peat-mesh/src/qos/bandwidth.rs` | Fully implemented with `acquire_async()`, `can_transmit()`, per-class token buckets |
| `PreemptionController` / `ActiveTransfer` | `peat-mesh/src/qos/preemption.rs` | Fully implemented with `register_transfer()`, `should_preempt()`, `pause_transfers_below()`, `resume_transfers()` |
| `PrioritySyncQueue` / `PendingSync` | `peat-protocol/src/qos/sync_queue.rs` | Fully implemented with `enqueue()`, `dequeue_highest()`, `dequeue_bytes()`, aging promotions |
| `DataType` classification | `peat-protocol/src/qos/classification.rs` | 20 predefined types mapped to P1-P5 classes |
| `BandwidthConfig` / `QuotaConfig` | `peat-mesh/src/qos/bandwidth.rs` | Serializable config with `build()` to create `BandwidthAllocation`, `validate()` |

**Not wired (the gap):**

1. `AutomergeSyncCoordinator` (peat-mesh) calls `send_sync_message_for_doc()` and `send_state_snapshot()` without acquiring bandwidth permits.
2. `sync_all_documents_with_peer()` iterates documents in HashMap order (arbitrary) — not priority order.
3. `PreemptionController` is never consulted; no `ActiveTransfer` is registered for Iroh streams.
4. Wire format (`SyncMessageType` enum, byte prefix) has no QoS class field — receiving peer cannot prioritize inbound messages.
5. `BandwidthAllocation::default_tactical()` is hardcoded to 1 Mbps; no runtime config path exists.

---

### Implementation Steps

#### Step 1: Add `BandwidthAllocation` and `PreemptionController` to `AutomergeSyncCoordinator`

**File:** `peat-mesh/src/storage/automerge_sync.rs`

- Add two new fields to the `AutomergeSyncCoordinator` struct (around line 502):
  ```rust
  bandwidth: Arc<BandwidthAllocation>,
  preemption: Arc<PreemptionController>,
  ```
- Update `new()`, `with_flow_control()`, and `with_sync_modes()` constructors to accept an optional `BandwidthConfig` parameter. When `None`, use `BandwidthAllocation::default_tactical()`.
- Add a `with_bandwidth(config: BandwidthConfig) -> Self` constructor variant.
- Add a public `set_bandwidth(&self, bps: u64)` method for runtime reconfiguration (requires making `bandwidth` an `Arc<RwLock<BandwidthAllocation>>`).

**Estimated: ~40 lines**

#### Step 2: Acquire bandwidth permits before sending

**File:** `peat-mesh/src/storage/automerge_sync.rs`

- In `send_sync_message_for_doc()` (line 1062), before writing to the stream:
  1. Classify the doc_key to determine `QoSClass` (derive from collection prefix using `SyncDirection::from_doc_key()` as a heuristic, or add a new `classify_doc_key()` method).
  2. Call `self.bandwidth.acquire_async(class, encoded.len()).await`.
  3. If permit is `None` and class is Critical/High, trigger preemption via `self.preemption.should_preempt(class)` / `pause_transfers_below(class)`.
  4. If permit is still `None` (non-preemptable class, bandwidth exhausted), return a new `SyncError::BandwidthExhausted` error (add variant to `sync_errors.rs`).
  5. Hold the `BandwidthPermit` until the `write_all()` calls complete.

- Apply the same pattern to `send_state_snapshot()` (line 871) and `send_batch_message()`.

**Estimated: ~60 lines across three methods**

#### Step 3: Register active transfers with PreemptionController

**File:** `peat-mesh/src/storage/automerge_sync.rs`

- In `initiate_sync_inner()` (line 775), after determining the message size:
  1. Call `self.preemption.register_transfer(class, size, policy.preemptable).await` to get a `TransferId`.
  2. On send completion (success or error), call `self.preemption.complete_transfer(transfer_id).await`.
  3. Check `transfer.is_paused` before each `write_all()` chunk for large transfers — if paused, yield until resumed.

- For batch sends (`send_batch_message()`), register one `ActiveTransfer` per batch.

**Estimated: ~30 lines**

#### Step 4: Wire `PrioritySyncQueue` into reconnection sync

**File:** `peat-mesh/src/storage/automerge_sync.rs`

- Modify `sync_all_documents_with_peer()` (line 1432):
  1. Instead of iterating `store.scan_prefix("")` directly, build a `PrioritySyncQueue`.
  2. For each `(doc_key, doc)`, classify the document to get its `QoSClass` and create a `PendingSync` entry.
  3. Drain the queue with `dequeue_highest()` and sync in priority order.
  4. Apply aging before draining (call `queue.apply_aging()`).

- Add a doc_key-to-QoSClass classification helper:
  ```rust
  fn classify_doc_key(doc_key: &str) -> QoSClass {
      let collection = doc_key.split(':').next().unwrap_or(doc_key);
      match collection {
          "contact_reports" | "alerts" | "commands" => QoSClass::Critical,
          "targets" | "imagery" | "retasking" => QoSClass::High,
          "nodes" | "cells" | "formations" => QoSClass::Normal,
          "beacons" | "platforms" | "heartbeats" => QoSClass::Low,
          "models" | "logs" | "training" => QoSClass::Bulk,
          _ => QoSClass::Normal, // safe default
      }
  }
  ```

**Estimated: ~50 lines**

#### Step 5: Add `QoSClass` metadata to wire format

**File:** `peat-mesh/src/storage/automerge_sync.rs`

- Extend the wire format to include a QoS class byte after the message type prefix. New format:
  ```text
  [2 bytes: doc_key_len][N bytes: doc_key][1 byte: SyncMessageType][1 byte: QoSClass][4 bytes: msg_len][M bytes: msg]
  ```
  The added byte is `QoSClass as u8` (1-5).

- Update `send_sync_message_for_doc()` to write the QoS byte.
- Update the receive path (in `SyncChannelManager` or the accept loop handler) to read and use the QoS byte for inbound prioritization.
- For backward compatibility: QoS byte value `0x00` means "unclassified, treat as Normal."

**Files also touched:**
- `peat-mesh/src/storage/sync_channel.rs` — update frame parsing
- `peat-protocol/src/sync/automerge.rs` — update receive handler in `AutomergeIrohBackend`

**Estimated: ~40 lines across 3 files**

#### Step 6: Runtime-configurable bandwidth

**Files:**
- `peat-protocol/src/network/peer_config.rs` — Add optional `[qos]` section to `PeerConfig`:
  ```toml
  [qos]
  bandwidth_bps = 500000  # 500 Kbps
  # Optional per-class overrides
  [qos.quotas.Critical]
  min_guaranteed_percent = 25
  max_burst_percent = 90
  ```
- `peat-mesh/src/qos/bandwidth.rs` — `BandwidthConfig` already has `Serialize`/`Deserialize` derives; just needs to be wired into the config loading path.
- `peat-protocol/src/sync/automerge.rs` — When constructing `AutomergeSyncCoordinator` in `AutomergeIrohBackend`, pass `BandwidthConfig` from the loaded `PeerConfig`.

**Estimated: ~30 lines**

#### Step 7: Add `BandwidthExhausted` error variant

**File:** `peat-mesh/src/storage/sync_errors.rs`

- Add variant:
  ```rust
  BandwidthExhausted { class: QoSClass, requested_bytes: usize },
  ```
- Implement `Display` for the variant.
- Wire into `initiate_sync()` error classification (line 732).

**Estimated: ~10 lines**

---

### Code Sketch: Hot Path Change

The critical change in `send_sync_message_for_doc()`:

```rust
async fn send_sync_message_for_doc(
    &self,
    peer_id: EndpointId,
    doc_key: &str,
    message: &SyncMessage,
) -> Result<()> {
    let class = Self::classify_doc_key(doc_key);
    let encoded = message.clone().encode();
    let size = encoded.len();

    // Acquire bandwidth permit (may trigger preemption for P1/P2)
    let permit = match self.bandwidth.acquire_async(class, size).await {
        Some(p) => p,
        None if class.can_preempt(&QoSClass::Normal) => {
            // Preempt lower-priority transfers
            let paused = self.preemption.pause_transfers_below(class).await;
            tracing::info!("Preempted {} transfers for {:?} data", paused.len(), class);
            self.bandwidth.acquire_async(class, size).await
                .ok_or_else(|| anyhow::anyhow!("Bandwidth exhausted for {:?} after preemption", class))?
        }
        None => return Err(anyhow::anyhow!("Bandwidth exhausted for {:?}", class)),
    };

    // Register transfer for preemption tracking
    let transfer_id = self.preemption.register_transfer(class, size, class >= QoSClass::Normal).await;

    // Send with QoS metadata (existing send logic + QoS byte)
    let result = self.send_with_qos(peer_id, doc_key, class, &encoded).await;

    self.preemption.complete_transfer(transfer_id).await;
    drop(permit); // Release bandwidth quota

    result
}
```

---

### Testing Plan

#### Unit Tests (peat-mesh)

1. **Bandwidth permit gating** — Verify `send_sync_message_for_doc()` returns `BandwidthExhausted` when token bucket is drained. Use a `MockSyncTransport` with a 100-byte bandwidth allocation and attempt to send 200 bytes as P4.

2. **Priority ordering on reconnection** — Create 10 documents across P1-P5, call `sync_all_documents_with_peer()`, assert P1 docs sync before P4 via ordered mock transport call log.

3. **Preemption trigger** — Register a P5 `ActiveTransfer`, then send P1 data; verify `pause_transfers_below()` is called and the P5 transfer is paused.

4. **Wire format round-trip** — Encode a message with QoS byte, decode on receiver side, assert `QoSClass` is preserved.

5. **Aging promotion** — Enqueue P5 items, advance time by 1 hour, call `apply_aging()`, verify promotion to P4 queue.

#### Integration Tests (peat-protocol)

6. **Priority delivery under constraint** — Two-node test with simulated 500 Kbps link. Insert P1 (1 KB) and P4 (100 KB) documents simultaneously. Assert P1 arrives at peer within 1 second while P4 is still in progress.

7. **Preemption end-to-end** — Start syncing a 1 MB P5 model update. Mid-transfer, insert a P1 contact report. Verify the P1 arrives before the P5 completes, and the P5 eventually finishes after P1 is done.

8. **Runtime bandwidth reconfiguration** — Start with 1 Mbps config, change to 500 Kbps at runtime via `set_bandwidth()`, verify new rate is enforced.

9. **Backward compatibility** — Node running new wire format (with QoS byte) communicates with node running old format (without QoS byte). Verify graceful fallback.

---

### Acceptance Criteria

- [ ] P1 (Critical) data syncs before P4 (Low) after a network partition
- [ ] Bandwidth quotas enforced per QoS class (measurable in tests)
- [ ] Preemption: bulk transfer pauses when critical data arrives
- [ ] Integration test: simulated 500 Kbps link, P1 arrives in <1s while P4/P5 queued
- [ ] Wire format includes QoS class byte (backward compatible with 0x00 = Normal)
- [ ] Bandwidth configurable via `PeerConfig` TOML `[qos]` section
- [ ] Runtime bandwidth change via `set_bandwidth()` API
- [ ] No regression in existing sync e2e tests (`automerge_iroh_sync_e2e`, `tombstone_sync_e2e`, `issue_229_sync_e2e`)

---

### Estimated Effort

| Step | Lines | Complexity |
|---|---|---|
| 1. Add fields to coordinator | ~40 | Low |
| 2. Permit acquisition in send path | ~60 | Medium |
| 3. ActiveTransfer registration | ~30 | Low |
| 4. PrioritySyncQueue in reconnection | ~50 | Medium |
| 5. Wire format QoS byte | ~40 | Medium (backward compat) |
| 6. Runtime config | ~30 | Low |
| 7. Error variant | ~10 | Low |
| **Total implementation** | **~260** | |
| Tests (unit + integration) | ~300 | |
| **Grand total** | **~560** | |

Estimated calendar time: 3-5 days for one engineer, including tests.

---

### Risks and Mitigations

- **Backward compatibility**: The QoS wire byte uses `0x00` as "unclassified" so old nodes ignore it gracefully. Version negotiation during the formation handshake can advertise QoS support.
- **Deadlock risk**: `BandwidthAllocation` uses `tokio::sync::RwLock` internally. The async `acquire_async()` path avoids blocking the runtime. The sync `acquire()` path uses `try_write()` with a fallback.
- **Preemption latency**: Pausing an in-flight Iroh QUIC stream requires cooperative yielding between write chunks. For the initial implementation, preemption applies at the message boundary (between documents), not mid-stream.
