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
