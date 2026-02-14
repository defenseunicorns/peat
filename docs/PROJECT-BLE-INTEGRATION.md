# BLE Integration Project Plan

**Created**: 2026-01-28
**Status**: Active
**Related ADRs**: ADR-039, ADR-047
**Related PRs**: #634 (send_to overrides), #635 (Pi-to-Pi test results)

---

## Overview

This project plan tracks the integration of hive-btle and hive-lite into the HIVE framework and ATAK plugin, implementing ADR-047 (Android BLE Hybrid Integration).

---

## Current Sprint: BLE Foundation

### Milestone 1: BLE Functional Testing (Pi-to-Pi) - COMPLETE

Validate hive-btle mesh protocol between two identical Linux nodes before Android integration.

| Task | Status | Notes |
|------|--------|-------|
| Set up second Raspberry Pi 5 with matching OS | DONE | rpi-ci + rpi-ci2, both Ubuntu 24.04, BlueZ 5.72 |
| Run ble_responder on Pi #1 | DONE | `--mesh-id CITEST --callsign PI-RESP` on rpi-ci |
| Run ble_test_client on Pi #2 | DONE | `--adapter hci0 --mesh-id CITEST` on rpi-ci2 |
| Verify discovery (nodes see each other) | DONE | Instant discovery, also sees WearOS devices |
| Verify basic sync (counter, callsign) | DONE | Bidirectional: PI-RESP <-> TEST-CLI in ~277ms |
| Verify CannedMessage round-trip (CHECK_IN -> ACK) | TODO | Requires encryption + hive-lite-sync feature |
| Document results | DONE | See Test Results below |

**Test Results (2026-02-13)**:
- Pi-to-Pi BLE mesh sync: **PASSED**
- Discovery: instant (cached scan results)
- Connection: sub-second (with advertising pause/resume fix)
- GATT read/write: 51 bytes bidirectional
- Callsign merge: PI-RESP and TEST-CLI exchanged successfully
- Total sync time: **277ms**
- Bug found & fixed: `on_ble_data_received_anonymous` rejected unencrypted docs (751c91e)

**Blockers**: RESOLVED
- ~~BlueZ version mismatch between Ubuntu 22.04 (5.64) and 24.04 (5.72)~~ Both Pis now on 24.04
- ~~Need second Pi with identical OS~~ rpi-ci2 set up at 192.168.228.65

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

### Milestone 4: ADR-047 Phase 3 - Dual-Active Transport Integration

Run Iroh and BLE simultaneously with per-collection transport routing.

**Design**: Both transports are always active. Each collection/document/subscription
declares its transport requirements — either explicit transport binding or autopace
(let the system select based on PACE scoring and availability). This is **not** a
failover model; it's a dual-active model where different data flows use different
transports concurrently.

**Per-collection routing options**:
- `transport: iroh` — always use Iroh (large payloads, reliable)
- `transport: ble` — always use BLE (WearTAK, offline proximity)
- `transport: bypass` — UDP bypass channel (low-latency ephemeral)
- `transport: pace` — PACE-based selection (score transports, pick best available)

PACE is a configuration option, not the default behavior. Collections that specify
a transport get that transport; collections that specify `pace` get dynamic selection.

**Example**:
```
Collection "beacons"     -> transport: pace (scores Iroh vs BLE, picks best)
Collection "positions"   -> transport: bypass (low-latency ephemeral UDP)
Collection "canned_msgs" -> transport: ble (BLE-only, WearTAK sync)
Collection "documents"   -> transport: iroh (large payloads, reliable)
```

**Precedent**: `BypassCollectionConfig` in `bypass.rs` already binds collections to
specific transport settings (transport mode, encoding, TTL, priority). The same pattern
generalizes to all transports with PACE as one of the routing strategies.

**Post-ADR-049 Status**: The ADR-049 refactor (phases 0-7, merged 2026-02-13) moved
the transport layer into `hive-mesh`. Much of the original M4 work is now complete:

- `HiveBleTransport<A>` lives in `hive-mesh/src/transport/btle.rs` and fully implements
  both `MeshTransport` and `Transport` traits
- `bluetooth` feature flag exists in both `hive-mesh` and `hive-protocol` Cargo.toml
- `TransportManager` supports PACE-based registration via `register_instance()`
- `BleTranslator` in `hive-protocol/src/sync/ble_translation.rs` (764 lines) bridges
  hive-btle CRDTs to Automerge documents for WearTAK
- `hive-ffi` already imports `HiveBleTransport` under `#[cfg(feature = "bluetooth")]`
- `BypassCollectionConfig` already demonstrates per-collection transport binding

| Task | Status | Notes |
|------|--------|-------|
| ~~Add `bluetooth` feature flag~~ | DONE | `hive-mesh` and `hive-protocol` |
| ~~`HiveBleTransport` implements Transport trait~~ | DONE | `hive-mesh/src/transport/btle.rs` |
| ~~TransportManager supports BLE registration~~ | DONE | `register_instance()` API |
| ~~BLE-to-Automerge translation layer~~ | DONE | `ble_translation.rs` — BleTranslator |
| ~~Collection transport routing config~~ | DONE | `CollectionRouteTable`, `CollectionTransportRoute` in `manager.rs` |
| ~~`route_message()` supports per-collection transport~~ | DONE | `route_collection()` + `RouteDecision::TransportInstance` |
| ~~PACE as transport config option~~ | DONE | `CollectionTransportRoute::Pace` with optional policy override |
| ~~Create FFI bootstrap for dual-active transport~~ | DONE | `hive-ffi`: construct `TransportManager` with both Iroh + BLE |
| Android bootstrap: Kotlin -> JNI -> HiveBleTransport | TODO | Instantiate `AndroidBleDelegate`, pass through JNI |
| ~~Integration test: dual-active (Iroh + BLE concurrent)~~ | DONE | `dual_active_transport_e2e.rs` (mock) + `dual_active_simultaneous.rs` (real Iroh) |
| ~~CannedMessage round-trip over BLE~~ | DONE | `canned_message_sync.rs` — 3 tests with encrypted BLE round-trip |

**Deliverables**:
- [x] `HiveBleTransport` implements full Transport trait surface
- [x] TransportManager can register and select BLE transport
- [x] BLE translation layer bridges CRDTs to Automerge
- [x] Per-collection transport routing (explicit or autopace)
- [x] Both Iroh and BLE active simultaneously
- [x] FFI bootstrap creates dual-active TransportManager
- [ ] Android bootstrap wires Kotlin delegate through JNI

---

### Milestone 5: ADR-047 Phase 4 - ATAK Plugin Migration

Migrate ATAK plugin from dual-system to unified transport.

| Task | Status | Notes |
|------|--------|-------|
| Update `HivePluginLifecycle` to use unified transport | TODO | |
| Remove direct `HiveBleManager` usage | TODO | Use TransportManager |
| Update `HiveDropDownReceiver` UI | TODO | Single peer list |
| Test WearTAK interoperability | TODO | Genesis sync via BleTranslator |
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
| 2026-02-13 | Pi-to-Pi BLE functional test | PASSED - rpi-ci <-> rpi-ci2, 277ms sync |
| 2026-02-13 | Fixed anonymous receive for unencrypted docs | `on_ble_data_received_anonymous` now handles both |
| 2026-02-13 | Released hive-btle v0.1.0 | crates.io + Maven Central |
| 2026-02-13 | Updated hive workspace to hive-btle 0.1.0 | From 0.1.0-rc.30 |
| 2026-02-13 | Released hive-btle v0.1.1 | Added `send_to()` primitives, crates.io + Maven Central |
| 2026-02-13 | ADR-049 transport extraction merged | `HiveBleTransport` now in `hive-mesh/src/transport/btle.rs` |
| 2026-02-13 | M4 re-evaluated against hive-mesh | Most transport wiring already done by ADR-049 |
| 2026-02-14 | Per-collection transport routing | `CollectionRouteTable`, `route_collection()`, PACE config option |

---

## Known Issues

| Issue | Status | Workaround |
|-------|--------|------------|
| ~~BlueZ 5.64 vs 5.72 discovery incompatibility~~ | Resolved | Both Pis on Ubuntu 24.04 (BlueZ 5.72) |
| ~~kitlab <-> rpi-ci BLE discovery fails~~ | Resolved | Using rpi-ci <-> rpi-ci2 (matching OS) |
| CannedMessage round-trip not yet tested | Open | Requires encryption + hive-lite-sync feature |

---

## Dependencies

### External
- hive-btle v0.1.1 (crates.io / Maven Central `com.revolveteam:hive:0.1.1`)
- hive-lite (path: `../hive-lite`)

### Hardware
- Raspberry Pi 5 x2: rpi-ci (D8:3A:DD:F5:FD:53), rpi-ci2 (D8:3A:DD:F6:1B:89, 192.168.228.65)
- Android device for ATAK testing
- WearOS devices: WEAROS-5122, WEAROS-6441 (discovered during Pi testing)

---

## References

- [ADR-039: HIVE-BTLE Mesh Transport](adr/039-hive-btle-mesh-transport.md)
- [ADR-047: Android BLE Hybrid Integration](adr/047-android-ble-hybrid-integration.md)
- [ADR-049: hive-mesh Extraction](adr/049-hive-mesh-extraction.md) - Transport layer refactor
- [ROADMAP.md](../ROADMAP.md) - High-level HIVE Protocol roadmap
- [hive-btle on crates.io](https://crates.io/crates/hive-btle)
- [hive-btle on Radicle](https://app.radicle.xyz/nodes/rosa.radicle.xyz/rad:z458mp9Um3AYNQQFMdHaNEUtmiohq)
