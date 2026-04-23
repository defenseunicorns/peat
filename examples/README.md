# Examples

This directory holds two kinds of projects, side by side.

## Sample applications

Illustrative apps that adopters can read or copy from when building their own
Peat integrations.

| Path | Description |
|------|-------------|
| `peat-tak-bridge/` | Reference TAK Server ↔ Peat bridge service (Rust workspace crate) |
| `android-peat-demo/` | Android demo app built on the Peat FFI |
| `ios-demo/` | iOS (`PeatInventory`) SwiftUI demo using the UniFFI bindings |
| `kotlin-test/` | Minimal Kotlin JVM program exercising the UniFFI-generated Kotlin bindings |
| `m5stack-core2-peat/` | Embedded ESP32 example (excluded from the Cargo workspace — needs its own toolchain) |

## Functional-test harnesses

Not sample code — these are internal rigs used to validate BLE transport and
FFI behavior on real hardware. See `docs/FUNCTIONAL-TESTING.md`.

| Path | Description |
|------|-------------|
| `peat-ble-test/` | Host-side BLE test binary (Linux / macOS / Windows) |
| `android-ble-test/` | Android companion APK for Pi ↔ Android BLE scenarios |

## Workspace membership

The Rust crates here (`peat-tak-bridge`, `peat-ble-test`) are regular members
of the top-level Cargo workspace. New `examples/*` crates should be added to
`members` in the root `Cargo.toml` unless they need a separate toolchain, in
which case add them to `exclude` (like `m5stack-core2-peat`).
