# BLE Integration Project Plan

**Created**: 2026-01-28
**Status**: Active
**Related ADRs**: ADR-039, ADR-047
**Branch**: `feat/ble-cell-mapping`

---

## Overview

This project plan tracks the integration of hive-btle and hive-lite into the HIVE framework and ATAK plugin, implementing ADR-047 (Android BLE Hybrid Integration).

---

## Current Sprint: BLE Foundation

### Milestone 1: BLE Functional Testing (Pi-to-Pi)

Validate hive-btle mesh protocol between two identical Linux nodes before Android integration.

| Task | Status | Notes |
|------|--------|-------|
| Set up second Raspberry Pi 5 with matching OS | TODO | Same Ubuntu version to avoid BlueZ issues |
| Run ble_responder on Pi #1 | TODO | `--mesh-id TEST --callsign PI-RESP` |
| Run ble_test_client on Pi #2 | TODO | `--adapter hci0 --mesh-id TEST` |
| Verify discovery (nodes see each other) | TODO | Check logs for "Found HIVE node" |
| Verify basic sync (counter, callsign) | TODO | Check "SYNC RECEIVED" in logs |
| Verify CannedMessage round-trip (CHECK_IN -> ACK) | TODO | Test exits 0 on success |
| Document results | TODO | Update this file with findings |

**Blockers**:
- BlueZ version mismatch between Ubuntu 22.04 (5.64) and 24.04 (5.72) caused discovery failures
- Need second Pi with identical OS for reliable testing

---

### Milestone 2: ADR-047 Phase 1 - AndroidBridgeAdapter

Create the Rust-side bridge adapter that delegates BLE operations to Kotlin via JNI.

| Task | Status | Notes |
|------|--------|-------|
| Create `hive-btle/src/platform/android/mod.rs` | DONE | Updated with hybrid architecture docs |
| Create `hive-btle/src/platform/android/bridge_adapter.rs` | DONE | 700+ lines, full BleAdapter impl |
| Implement BleAdapter trait with JNI delegation | DONE | All methods implemented |
| Create `hive-btle/src/platform/android/jni_callbacks.rs` | DONE | 10 JNI callback functions |
| Add JNI dependencies to Cargo.toml | DONE | `jni = "0.21"` |
| Create mock JNI for unit testing | DONE | 5 unit tests passing |
| Verify compilation for Android targets | TODO | `cargo build --target aarch64-linux-android` |

**Deliverables**:
- [x] `AndroidBridgeAdapter` compiles
- [x] JNI callback signatures match Kotlin expectations
- [x] Unit tests pass (5/5 passing)

---

### Milestone 3: ADR-047 Phase 2 - Kotlin AndroidBleDelegate

Create the Kotlin-side BLE implementation that handles Android radio operations.

| Task | Status | Notes |
|------|--------|-------|
| Create `hive-btle/android/` module structure | EXISTS | Already has Gradle setup |
| Create `AndroidBleDelegate.kt` | DONE | 700+ lines, full implementation |
| Implement BLE scanning with HIVE filter | DONE | ScanCallback with service UUID filter |
| Implement BLE advertising with beacon data | DONE | AdvertiseCallback |
| Implement GATT server (sync_state characteristic) | DONE | BluetoothGattServerCallback |
| Implement GATT client (connect, read, write) | DONE | BluetoothGattCallback |
| Wire up native JNI callbacks | DONE | 10 `external fun` declarations |
| Test on Android device | TODO | Manual testing (requires device) |

**Deliverables**:
- [x] `AndroidBleDelegate` scans and discovers HIVE nodes
- [x] GATT server accepts connections
- [ ] Data exchange works with Pi responder (requires testing)

---

### Milestone 4: ADR-047 Phase 3 - TransportManager Integration

Integrate the hybrid BLE transport into the unified TransportManager.

| Task | Status | Notes |
|------|--------|-------|
| Add `bluetooth` feature flag to hive-ffi | TODO | Cargo.toml |
| Create `create_with_ble_transport` FFI function | TODO | hive-ffi/src/lib.rs |
| Register HiveBleTransport with TransportManager | TODO | Unified transport |
| Expose via UniFFI/JNI | TODO | Kotlin bindings |
| Integration test with dual transport | TODO | Iroh + BLE |

**Deliverables**:
- [ ] Single `HiveNode` API with BLE transport
- [ ] TransportManager routes data to both transports
- [ ] PACE policy selects appropriate transport

---

### Milestone 5: ADR-047 Phase 4 - ATAK Plugin Migration

Migrate ATAK plugin from dual-system to unified transport.

| Task | Status | Notes |
|------|--------|-------|
| Update `HivePluginLifecycle` to use unified transport | TODO | |
| Remove direct `HiveBleManager` usage | TODO | Use TransportManager |
| Update `HiveDropDownReceiver` UI | TODO | Single peer list |
| Test WearTAK interoperability | TODO | Genesis sync |
| Add deprecation warnings for old API | TODO | Smooth transition |

**Deliverables**:
- [ ] ATAK plugin uses single HiveNode API
- [ ] WearTAK devices sync via unified transport
- [ ] No regression in functionality

---

### Milestone 6: ADR-047 Phase 5 - Cleanup

Final cleanup and documentation.

| Task | Status | Notes |
|------|--------|-------|
| Remove deprecated `HiveBleManager` | TODO | After migration complete |
| Update ADR-039 with implementation notes | TODO | |
| Update ADR-047 status to Accepted | TODO | |
| Battery consumption benchmark | TODO | <5% regression target |
| Performance profiling (callback latency) | TODO | <10ms target |
| Update ATAK plugin documentation | TODO | |

**Deliverables**:
- [ ] Clean codebase
- [ ] Battery benchmark results documented
- [ ] All ADRs updated

---

## Completed Tasks

| Date | Task | Notes |
|------|------|-------|
| 2026-01-28 | Created ADR-047 | Android BLE Hybrid Integration Architecture |
| 2026-01-28 | Updated hive-btle to path dependency | `{ path = "../hive-btle" }` |
| 2026-01-28 | Added hive-lite as path dependency | `{ path = "../hive-lite" }` |
| 2026-01-28 | Workspace compiles with path deps | Both hive-btle and hive-lite |
| 2026-01-28 | Implemented AndroidBridgeAdapter | `bridge_adapter.rs` - 700+ lines |
| 2026-01-28 | Implemented JNI callbacks | `jni_callbacks.rs` - 10 native functions |
| 2026-01-28 | Updated Android module | `mod.rs` with hybrid architecture docs |
| 2026-01-28 | Added JNI dependency | `jni = "0.21"` to hive-btle Cargo.toml |
| 2026-01-28 | Unit tests passing | 5 bridge adapter tests |
| 2026-01-28 | Created AndroidBleDelegate.kt | Kotlin BLE handler with JNI callbacks |

---

## Known Issues

| Issue | Status | Workaround |
|-------|--------|------------|
| BlueZ 5.64 vs 5.72 discovery incompatibility | Investigating | Use matching OS versions |
| kitlab <-> rpi-ci BLE discovery fails | Blocked | Waiting for second Pi setup |

---

## Dependencies

### External
- hive-btle (path: `../hive-btle`, v0.1.0-rc.29)
- hive-lite (path: `../hive-lite`)

### Hardware
- Raspberry Pi 5 x2 (matching OS)
- Android device for ATAK testing
- WearOS device for WearTAK testing

---

## References

- [ADR-039: HIVE-BTLE Mesh Transport](adr/039-hive-btle-mesh-transport.md)
- [ADR-047: Android BLE Hybrid Integration](adr/047-android-ble-hybrid-integration.md)
- [ROADMAP.md](../ROADMAP.md) - High-level HIVE Protocol roadmap
- [hive-btle examples](https://github.com/kitplummer/hive-btle/tree/main/examples)
