# Peat Protocol - M5Stack Core2 Demo

Demonstration of Peat-Lite mesh networking on ESP32 using BLE.

## Hardware

- **M5Stack Core2** (ESP32-D0WDQ6-V3)
- 16MB Flash, 8MB PSRAM
- Built-in accelerometer (MPU6886)
- Touch screen + 3 touch buttons
- AXP192 PMU for power management

## Prerequisites

### 1. Install ESP-IDF Rust toolchain

```bash
# Install espup (ESP Rust toolchain manager)
cargo install espup

# Install ESP32 toolchain
espup install

# Source the environment (add to .bashrc/.zshrc)
source ~/export-esp.sh
```

### 2. Install flashing tools

```bash
cargo install espflash
cargo install ldproxy
```

## Building

```bash
# Navigate to this directory
cd examples/m5stack-core2-peat

# Build (debug)
cargo build

# Build (release, recommended for flashing)
cargo build --release
```

## Flashing

```bash
# Flash and monitor (release build recommended)
cargo run --release

# Or manually
espflash flash --monitor target/xtensa-esp32-espidf/release/m5stack-core2-peat
```

## What It Does

1. **Advertises** as a Peat node via BLE beacons
2. **Exposes** GATT service with Peat characteristics
3. **Reads** sensor data (buttons, accelerometer, battery)
4. **Syncs** CRDT state with connected nodes

## Peat Beacon Format

The node advertises with:
- Service UUID: `0xF47A` (16-bit alias for Peat)
- Compact beacon: Node ID, hierarchy level, battery, connection capacity

## Testing with Android

1. Flash two M5Stack Core2 units with unique node IDs
2. Install Peat ATAK plugin on Android tablet
3. Scan for Peat nodes
4. Connect and observe sync

## Configuration

Edit `src/main.rs`:
- `NODE_ID`: Unique identifier for this node
- `DEVICE_NAME`: BLE device name
- `ADVERTISING_INTERVAL_MS`: Beacon interval

## Known limitations

This example targets a small (≤4-node) BLE mesh — sensor + watch + ATAK
plugin. Two structural choices in `src/nimble.rs` and `sdkconfig.defaults`
trade flexibility for stability and need to be revisited before any
larger-scale validation:

- **`PERIPHERAL_ONLY = true`** — the M5Stack never initiates outbound BLE
  connections; it only advertises and accepts incoming connections. This
  removes the connect/disconnect storms that overloaded NimBLE under
  concurrent bidirectional discovery, but it means M5Stack-to-M5Stack
  links require *some other* node (a watch or phone) in range that
  actively scans + connects. Two M5Stacks in range of only each other
  will not link, even though both are advertising. Flip to `false` and
  re-validate the per-peer + global connect-backoffs under sustained
  load before relying on M5Stack-as-central topologies.
- **`MULTIHOP_FORWARD = false`** — receiving and merging a peer document
  no longer triggers an immediate fanout to other connections. State
  propagates via the 5 s periodic gossip instead. For 1-hop topologies
  this is invisible; for an N-hop chain, end-to-end convergence becomes
  ~`N × 5 s` instead of ~one round-trip per hop. Flip to `true` for
  multi-hop scenarios and re-soak.
- **`CONFIG_BT_NIMBLE_MAX_CONNECTIONS = 2`** — halved from the
  ESP-IDF default of 4. Halves the worst-case mbuf cleanup chain on
  disconnect, which is what was tripping the task watchdog. The
  larger-mesh demo will need this raised; doing so re-introduces the
  cleanup pressure, so the doc-drain cap (`DOC_DRAIN_PER_TICK`) and
  the per-peer/global connect backoffs all need to hold under the
  higher ceiling.
- **Demo-only embedded mesh genesis.** The `SHARED_GENESIS_BASE64`
  fallback in `src/main.rs` is the badge-demo genesis. For any
  non-demo build, set `PEAT_GENESIS_BASE64=<base64>` at build time so
  the secret lives in the build environment rather than repo history.
  The runtime emits a `WARN` at boot when the fallback is in use.
  ADR-044 is the planned path for real (MLS-based) group key
  distribution.

## Troubleshooting

### Build fails with "esp toolchain not found"
```bash
source ~/export-esp.sh
```

### Flash fails
```bash
# Check USB connection
ls /dev/tty.usb*

# Use explicit port
espflash flash --port /dev/tty.usbserial-xxxxx target/xtensa-esp32-espidf/release/m5stack-core2-peat
```

### Monitor shows garbage
- Baud rate should be 115200 (default)
- Try `espflash monitor` separately

## License

Apache-2.0
