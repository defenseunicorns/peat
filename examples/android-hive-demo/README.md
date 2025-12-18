# HIVE BLE Demo App

A simple Android application demonstrating HIVE BLE mesh connectivity with M5Stack Core2 devices.

## Features

- **Scan** for HIVE BLE nodes (devices advertising the HIVE service UUID `0xD479`)
- **Connect** to discovered M5Stack Core2 nodes
- **Advertise** as a HIVE node for other devices to discover
- **Sync** CRDT data over BLE GATT characteristics

## Requirements

- Android 6.0 (API 23) or later
- Bluetooth Low Energy support
- For BLE 5.0 features: Android 8.0 (API 26) or later

## Building

### Prerequisites

1. Install the Android NDK and set `ANDROID_NDK_HOME`
2. Install Rust Android targets:
   ```bash
   rustup target add aarch64-linux-android armv7-linux-androideabi
   ```
3. Install `cargo-ndk`:
   ```bash
   cargo install cargo-ndk
   ```

### Build Native Library

First, build the hive-btle native library for Android:

```bash
cd ../../hive-btle
cargo ndk -t arm64-v8a -t armeabi-v7a build --release --features android
```

Copy the built libraries to the jniLibs directory:

```bash
mkdir -p android/src/main/jniLibs/arm64-v8a android/src/main/jniLibs/armeabi-v7a
cp ../../target/aarch64-linux-android/release/libhive_btle.so android/src/main/jniLibs/arm64-v8a/
cp ../../target/armv7-linux-androideabi/release/libhive_btle.so android/src/main/jniLibs/armeabi-v7a/
```

### Build the App

```bash
./gradlew assembleDebug
```

## Usage

1. Launch the app on an Android device
2. Grant Bluetooth permissions when prompted
3. Tap "Start Scan" to discover nearby HIVE nodes
4. Tap a discovered device to connect
5. Tap "Start Advertise" to make this device discoverable

## HIVE BLE Service

The app uses the HIVE BLE service with the following characteristics:

| UUID | Name | Description |
|------|------|-------------|
| `0xD479` | Service | HIVE BLE Service |
| `0x0001` | Node Info | Node ID, capabilities |
| `0x0002` | Sync State | Vector clock |
| `0x0003` | Sync Data | CRDT deltas |
| `0x0004` | Command | Control commands |
| `0x0005` | Status | Connection status |

## Testing with M5Stack Core2

1. Flash the M5Stack Core2 with the hive-btle ESP32 firmware
2. Power on the M5Stack - it will advertise as `HIVE-XXXXXXXX`
3. Use this demo app to scan and connect
4. Observe CRDT sync between devices

## License

Apache-2.0
