# Functional Testing Guide

**Created**: 2026-02-15
**Status**: Active
**Related ADRs**: ADR-039 (HIVE-BTLE Mesh Transport), ADR-047 (Android BLE Hybrid Integration)
**Related Docs**: [PROJECT-BLE-INTEGRATION.md](PROJECT-BLE-INTEGRATION.md), [TESTING_STRATEGY.md](TESTING_STRATEGY.md)

---

## Overview

This document describes the functional test infrastructure for HIVE's transport layer. Functional tests validate **real hardware** running real protocol stacks over real radios and networks — they are not simulations or mocks.

The test infrastructure is designed for extension. Each platform (Android, macOS, iOS, Windows, Linux) gets its own test harness that exercises the same feature set through the same phases. Adding a new platform means implementing a test client for that platform and adding corresponding Makefile targets.

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Test Orchestrator                      │
│                    (make dual-transport-test)             │
│                                                          │
│  1. Cross-compile Pi binaries (aarch64)                  │
│  2. Build platform test app (APK / .app / .exe)          │
│  3. Deploy all binaries                                  │
│  4. Start Pi services                                    │
│  5. Launch platform test client with peer info            │
│  6. Collect results from BOTH sides                      │
│  7. Report final verdict                                 │
└──────────┬──────────────────────────────────┬────────────┘
           │                                  │
           ▼                                  ▼
┌─────────────────────┐          ┌─────────────────────────┐
│   Raspberry Pi       │          │   Test Client            │
│   (rpi-ci)           │          │   (platform under test)  │
│                      │          │                          │
│  ble_responder       │◄──BLE──►│  BLE: scan, GATT sync    │
│  (hive-btle, BlueZ)  │          │                          │
│                      │          │                          │
│  iroh_test_peer      │◄─QUIC──►│  QUIC: Iroh Automerge    │
│  (hive-ffi, Iroh)    │          │       platform sync      │
└─────────────────────┘          └─────────────────────────┘
```

Both Pi services run on the **same Raspberry Pi** (rpi-ci). BLE uses the BlueZ D-Bus stack; QUIC uses the network stack. They do not conflict.

### Key Principles

- **Both sides verify**: The test only passes when the Pi AND the client both confirm bidirectional data exchange.
- **Feature parity**: Every platform test client implements the same phases and validates the same features.
- **Automated**: All tests run headlessly via `make` targets — no manual button tapping or GUI interaction.
- **Extensible**: Adding a platform means adding a test client + Makefile targets. The Pi infrastructure stays the same.

---

## Infrastructure

### Raspberry Pi (Shared Test Peer)

| Host | Role | OS | BLE | IP |
|------|------|----|-----|----|
| rpi-ci | BLE responder + QUIC peer | Ubuntu 24.04, BlueZ 5.72 | D8:3A:DD:F5:FD:53 | 192.168.228.13 |

**Services running during test:**

| Binary | Source | Transport | What It Does |
|--------|--------|-----------|-------------|
| `ble_responder` | hive-btle `examples/ble_responder.rs` | BLE GATT | Advertises HIVE service, accepts GATT connections, exchanges HiveDocument (GCounter + callsign) |
| `iroh_test_peer` | hive-ffi `examples/iroh_test_peer.rs` | QUIC/Iroh | Publishes "PI-QUIC" platform, waits for client's platform via Automerge sync |

### Build Requirements

| Tool | Purpose | Install |
|------|---------|---------|
| `cross` | Cross-compile Rust for aarch64 | `cargo install cross` |
| `cargo-ndk` | Cross-compile Rust for Android | `cargo install cargo-ndk` |
| Docker | Required by `cross` | System package |
| Android SDK + NDK | Build Android APK | `$ANDROID_HOME` or `~/Android/Sdk` |

### Network Requirements

- SSH access to rpi-ci (`kit@rpi-ci`)
- Bluetooth range between Pi and test device
- IP connectivity between Pi and test device (for QUIC)
- Note: mDNS may not work on enterprise WiFi — the test falls back to direct peer connect

---

## Feature-to-Phase Mapping

Each phase validates a specific feature. The mapping below shows which capability is tested, which transport carries it, and whether it's a hard requirement for the test to pass.

### Dual-Transport Test (11 phases)

| Phase | Name | Feature Under Test | Transport | Pass Criteria | Required |
|-------|------|--------------------|-----------|---------------|----------|
| 1 | JNI Init | Native library loading, JNI binding | — | `hiveVersion()` returns non-empty | Yes |
| 2 | Dual Node Created | Node creation with BLE + QUIC | Both | Node handle != 0, node ID non-empty | Yes |
| 3 | Iroh Active | QUIC transport initialization | QUIC | Node ID valid, handle active | Yes |
| 4 | BLE Discovery | BLE scanning, advertisement parsing | BLE | HIVE service UUID found within 15s | Yes |
| 5 | BLE GATT Sync | GATT connect, characteristic R/W, document exchange | BLE | Bytes received > 0, peer node ID parsed | Yes |
| 6 | Publish Platform | Automerge document creation, local store write | QUIC | `publishPlatformJni` returns true | Yes |
| 7 | QUIC Peer Connect | Iroh peer discovery (mDNS or direct connect) | QUIC | Peer count > 0 within 25s | Yes |
| 8 | QUIC Data Received | Automerge sync, remote document merge | QUIC | "PI-QUIC" platform appears within 30s | Yes |
| 9 | BLE State Signaled | TransportManager BLE state bridge | BLE | `bleIsAvailable` = true, BLE peer count >= 1 | Yes |
| 10 | Dual Transport Verified | Both transports carried data independently | Both | iroh peers >= 1 AND ble peers >= 1 AND QUIC data received | Yes |
| 11 | Sync Hold | Connection lifetime for bidirectional sync | Both | Holds 15s so Pi can receive client data | Yes |

### BLE-Only Test (7 phases)

When QUIC peer info is not provided, the test falls back to BLE-only mode:

| Phase | Name | Feature Under Test | Transport | Pass Criteria |
|-------|------|--------------------|-----------|---------------|
| 1 | JNI Init | Native library loading | — | Version non-empty |
| 2 | Dual Node Created | Node creation with BLE | Both | Handle != 0 |
| 3 | Iroh Active | QUIC transport init | QUIC | Node ID valid |
| 4 | BLE Discovery | BLE scanning | BLE | HIVE service found |
| 5 | BLE GATT Sync | GATT document exchange | BLE | Bytes received > 0 |
| 6 | BLE State Signaled | TransportManager bridge | BLE | Available + peers |
| 7 | Dual Transport | Both transports active | Both | Iroh active + BLE peers >= 1 |

### Pi-Side Verification

The Pi independently verifies it received data from the client:

| Check | What | Pass Criteria |
|-------|------|---------------|
| iroh_test_peer | Received client platform | "ANDROID-DUAL" (or equivalent) appears within 60s |
| ble_responder | Accepted GATT sync | Log shows connection + data exchange |

---

## Feature Coverage Matrix

This matrix tracks which features are tested on which platforms. Add a column for each new platform.

| Feature | Android | macOS | iOS | Windows | Linux CLI |
|---------|---------|-------|-----|---------|-----------|
| Native library loading | Phase 1 | — | — | — | — |
| Dual-transport node creation | Phase 2 | — | — | — | — |
| BLE scanning/discovery | Phase 4 | — | — | — | — |
| BLE GATT sync (bidirectional) | Phase 5 | — | — | — | — |
| Automerge platform publish | Phase 6 | — | — | — | — |
| QUIC peer connect (mDNS) | Phase 7 | — | — | — | — |
| QUIC peer connect (direct) | Phase 7 fallback | — | — | — | — |
| Automerge sync (receive remote) | Phase 8 | — | — | — | — |
| BLE state → TransportManager | Phase 9 | — | — | — | — |
| Simultaneous BLE + QUIC data | Phase 10 | — | — | — | — |
| Connection hold for peer sync | Phase 11 | — | — | — | — |
| Pi receives client data | Pi-side check | — | — | — | — |

**Legend**: Phase N = implemented and passing, — = not yet implemented

---

## Running Tests

### Full Dual-Transport Test

```bash
make dual-transport-test
```

This single command:
1. Cross-compiles `ble_responder` (aarch64) from hive-btle
2. Cross-compiles `iroh_test_peer` (aarch64) from hive-ffi
3. Deploys both to rpi-ci via scp
4. Builds Android native lib (`libhive_ffi.so` with `--features bluetooth`)
5. Builds Android test APK via Gradle
6. Deploys APK to connected Android device
7. Starts both Pi services
8. Captures `PEER_NODE_ID` from iroh_test_peer log
9. Launches Android app with `--ez auto_run true --es quic_node_id <id> --es quic_address <ip:port>`
10. Waits for Android test completion (up to 90s)
11. Waits for Pi iroh_test_peer to finish (up to 30s)
12. Reports results from **both** sides
13. Final verdict: PASS only if both sides passed

### BLE-Only Test

```bash
make ble-test
```

Runs phases 1-7 (BLE only, no QUIC peer info passed to Android).

### Individual Targets

```bash
make build-ble-responder       # Cross-compile ble_responder
make deploy-ble-responder      # scp to Pi
make start-ble-responder       # Start on Pi (backgrounded)
make stop-ble-responder        # Kill on Pi

make build-iroh-test-peer      # Cross-compile iroh_test_peer
make deploy-iroh-test-peer     # scp to Pi
make start-iroh-test-peer      # Start on Pi (backgrounded)
make stop-iroh-test-peer       # Kill on Pi

make build-ble-test-app        # Build Android APK
make deploy-ble-test-app       # Install APK via adb

make ble-test-logs             # Android logcat (filtered)
make ble-responder-logs        # Pi ble_responder log
make iroh-test-peer-logs       # Pi iroh_test_peer log

make clean-ble-test            # Remove build artifacts
```

### Configuration

Makefile variables (override with `make VAR=value`):

| Variable | Default | Description |
|----------|---------|-------------|
| `HIVE_BTLE_DIR` | `$(HOME)/Code/revolve/hive-btle` | Path to hive-btle repo |
| `BLE_TEST_PI` | `rpi-ci` | Pi hostname |
| `BLE_TEST_PI_USER` | `kit` | SSH user |
| `BLE_TEST_PI_IP` | `192.168.228.13` | Pi IP (for QUIC direct connect) |
| `BLE_TEST_MESH_ID` | `FUNCTEST` | BLE mesh formation ID |
| `BLE_TEST_CALLSIGN` | `PI-RESP` | ble_responder callsign |
| `IROH_TEST_PORT` | `42009` | iroh_test_peer bind port |

---

## Adding a New Platform

To add functional testing for a new platform (e.g., macOS), implement these components:

### 1. Test Client Application

Create a test client that implements the same phases as the Android `TestRunner.kt`. The client must:

- Accept peer info as launch arguments (node ID, address, auto-run flag)
- Execute all 11 phases in order
- Log results in a parseable format: `Phase N: <Name> ... PASS/FAIL`
- Print a summary line: `RESULT: N/N PASSED`
- Exit with code 0 on success, 1 on failure

**Reference implementation**: `android-ble-test/app/src/main/java/com/revolveteam/hive/test/TestRunner.kt`

**Platform-specific concerns:**

| Platform | BLE Stack | Build Tool | Deploy Method | Launch Method |
|----------|-----------|------------|---------------|---------------|
| Android | Android BLE API (via JNI) | Gradle + cargo-ndk | `adb install` | `adb shell am start --es/--ez` |
| macOS | CoreBluetooth | Xcode / swift build | scp or local | CLI binary or .app |
| iOS | CoreBluetooth | Xcode | Xcode / ios-deploy | xcrun simctl or device |
| Windows | WinRT BLE | cargo / Visual Studio | scp or local | CLI binary or .exe |
| Linux | BlueZ D-Bus | cargo | scp | SSH + binary |

### 2. Makefile Targets

Add these targets for each platform (replace `<platform>` with e.g., `macos`):

```makefile
# Build the platform test client
build-<platform>-test-client:

# Deploy to test device (scp, adb, xcodebuild, etc.)
deploy-<platform>-test-client:

# Full test pipeline
<platform>-dual-transport-test: deploy-ble-responder deploy-iroh-test-peer \
    build-<platform>-test-client deploy-<platform>-test-client \
    start-ble-responder start-iroh-test-peer
	# 1. Capture PEER_NODE_ID from iroh_test_peer log
	# 2. Launch test client with peer info
	# 3. Wait for completion
	# 4. Check BOTH sides
	# 5. Report verdict
```

### 3. Feature Flag Wiring

The test client links against `libhive_ffi` with appropriate features:

```toml
# Cargo.toml (for Rust-based clients) or build.gradle (Android) etc.
hive-ffi = { features = ["sync", "bluetooth"] }
```

Platform-specific BLE adapter is selected automatically via `cfg(target_os)` in hive-ffi/Cargo.toml.

### 4. Update Coverage Matrix

Add a column to the Feature Coverage Matrix above showing which phases pass on the new platform.

---

## Shared Test Formation

All test participants use the same FUNCTEST formation credentials:

| Parameter | Value | Used By |
|-----------|-------|---------|
| App ID / Mesh ID | `FUNCTEST` | All participants |
| Shared Key | `[0x01..0x20]` (base64: `AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=`) | QUIC sync (Iroh) |
| BLE Callsign (Pi) | `PI-RESP` | ble_responder |
| BLE Callsign (Client) | `ANDROID-TEST` (or platform equivalent) | Test client |
| QUIC Platform (Pi) | `PI-QUIC` (id: `pi-quic-test`) | iroh_test_peer |
| QUIC Platform (Client) | `ANDROID-DUAL` (id: `android-dual-test`) | Test client |

When adding a new platform, keep the same formation credentials. Only change the client's callsign and platform name to identify the platform (e.g., `MACOS-DUAL`, `macos-dual-test`).

---

## Troubleshooting

### Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Text file busy` on scp | Binary still running on Pi | `make stop-ble-responder stop-iroh-test-peer` before deploy |
| BLE GATT status 133 | Transient Android BLE error | Retry; toggle Bluetooth on Pi if persistent |
| mDNS discovery fails (0 peers) | Enterprise WiFi blocking multicast | Test falls back to `connectPeerJni` with direct address |
| Pi never receives client platform | Client disconnected before sync completed | Phase 11 (Sync Hold) keeps connection alive 15s |
| iroh_test_peer "failed to start" | Previous instance still running | `ssh kit@rpi-ci 'pkill -x iroh_test_peer'` |
| `protoc` too old in cross container | Ubuntu 16.04 has protoc 2.6.1 | Cross.toml downloads protoc v25.1 from GitHub releases |

### Logs

```bash
# Android test phases
adb logcat -s HiveTest:V BleGattClient:V HiveJni:V

# Pi BLE responder
ssh kit@rpi-ci 'tail -f ~/ble_responder.log'

# Pi QUIC peer
ssh kit@rpi-ci 'tail -f ~/iroh_test_peer.log'
```

---

## File Map

| File | Purpose |
|------|---------|
| `Makefile` (lines 449-677) | Test orchestration targets |
| `Cross.toml` | aarch64 cross-compilation config (protoc, libdbus) |
| `hive-ffi/examples/iroh_test_peer.rs` | Pi-side QUIC test peer |
| `hive-ffi/Cargo.toml` | Example + feature definitions |
| `hive-ffi/src/lib.rs` | JNI bindings (connectPeerJni, etc.) |
| `android-ble-test/.../test/TestRunner.kt` | Android 11-phase test orchestrator |
| `android-ble-test/.../test/MainActivity.kt` | Android UI + auto-run support |
| `android-ble-test/.../test/BleGattClient.kt` | Android BLE scan + GATT client |
| `android-ble-test/.../atak/hive/HiveJni.kt` | JNI declarations |
