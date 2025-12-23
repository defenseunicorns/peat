#!/bin/bash
# Build script for HIVE iOS/macOS test app
#
# This script builds the Rust hive-apple-ffi library for Apple platforms
# and creates an xcframework for use with the SwiftUI app.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FFI_DIR="$SCRIPT_DIR/hive-apple-ffi"
BUILD_DIR="$SCRIPT_DIR/build"
XCFRAMEWORK_DIR="$BUILD_DIR/HiveFFI.xcframework"

echo "=== HIVE iOS Build Script ==="
echo "FFI dir: $FFI_DIR"
echo ""

# Check for required tools
command -v cargo >/dev/null 2>&1 || { echo "Error: cargo not found"; exit 1; }
command -v xcodebuild >/dev/null 2>&1 || { echo "Error: xcodebuild not found"; exit 1; }

# Create build directory
mkdir -p "$BUILD_DIR"

# Build for each target
echo "=== Building hive-apple-ffi for Apple platforms ==="

cd "$FFI_DIR"

# macOS (arm64)
echo "Building for macOS (arm64)..."
cargo build --features macos --release --target aarch64-apple-darwin

# macOS (x86_64) - for Intel Macs
echo "Building for macOS (x86_64)..."
if rustup target list --installed | grep -q x86_64-apple-darwin; then
    cargo build --features macos --release --target x86_64-apple-darwin
else
    echo "  Target x86_64-apple-darwin not installed, skipping"
fi

# iOS device (arm64)
echo "Building for iOS (arm64)..."
if rustup target list --installed | grep -q aarch64-apple-ios; then
    cargo build --features ios --release --target aarch64-apple-ios
else
    echo "  Target aarch64-apple-ios not installed, skipping"
    echo "  Run: rustup target add aarch64-apple-ios"
fi

# iOS Simulator (arm64)
echo "Building for iOS Simulator (arm64)..."
if rustup target list --installed | grep -q aarch64-apple-ios-sim; then
    cargo build --features ios --release --target aarch64-apple-ios-sim
else
    echo "  Target aarch64-apple-ios-sim not installed, skipping"
    echo "  Run: rustup target add aarch64-apple-ios-sim"
fi

echo ""
echo "=== Generating Swift bindings ==="

# Generate Swift bindings (use the macOS arm64 dylib)
cargo run --features macos --bin uniffi-bindgen generate \
    --library target/aarch64-apple-darwin/release/libhive_apple_ffi.dylib \
    --language swift \
    --out-dir "$BUILD_DIR/generated"

# Copy Swift file to project
cp "$BUILD_DIR/generated/hive_apple_ffi.swift" "$SCRIPT_DIR/HiveTest/HiveBridge/"

echo ""
echo "=== Creating xcframework ==="

# Clean up old xcframework
rm -rf "$XCFRAMEWORK_DIR"

# Create include directory for headers
INCLUDE_DIR="$BUILD_DIR/include"
mkdir -p "$INCLUDE_DIR"
cp "$BUILD_DIR/generated/hive_apple_ffiFFI.h" "$INCLUDE_DIR/"
cp "$BUILD_DIR/generated/hive_apple_ffiFFI.modulemap" "$INCLUDE_DIR/module.modulemap"

# Create xcframework from the available libraries
FRAMEWORKS=()

# macOS arm64
MACOS_ARM64_LIB="$FFI_DIR/target/aarch64-apple-darwin/release/libhive_apple_ffi.a"
if [ -f "$MACOS_ARM64_LIB" ]; then
    echo "Adding macOS arm64..."
    mkdir -p "$BUILD_DIR/macos-arm64"
    cp "$MACOS_ARM64_LIB" "$BUILD_DIR/macos-arm64/"
    cp -r "$INCLUDE_DIR" "$BUILD_DIR/macos-arm64/include"

    # Check if we also have x86_64 to create universal binary
    MACOS_X64_LIB="$FFI_DIR/target/x86_64-apple-darwin/release/libhive_apple_ffi.a"
    if [ -f "$MACOS_X64_LIB" ]; then
        echo "Creating universal macOS binary..."
        mkdir -p "$BUILD_DIR/macos-universal"
        lipo -create "$MACOS_ARM64_LIB" "$MACOS_X64_LIB" -output "$BUILD_DIR/macos-universal/libhive_apple_ffi.a"
        cp -r "$INCLUDE_DIR" "$BUILD_DIR/macos-universal/include"
        FRAMEWORKS+=(-library "$BUILD_DIR/macos-universal/libhive_apple_ffi.a" -headers "$BUILD_DIR/macos-universal/include")
    else
        FRAMEWORKS+=(-library "$BUILD_DIR/macos-arm64/libhive_apple_ffi.a" -headers "$BUILD_DIR/macos-arm64/include")
    fi
fi

# iOS device
IOS_ARM64_LIB="$FFI_DIR/target/aarch64-apple-ios/release/libhive_apple_ffi.a"
if [ -f "$IOS_ARM64_LIB" ]; then
    echo "Adding iOS arm64..."
    mkdir -p "$BUILD_DIR/ios-arm64"
    cp "$IOS_ARM64_LIB" "$BUILD_DIR/ios-arm64/"
    cp -r "$INCLUDE_DIR" "$BUILD_DIR/ios-arm64/include"
    FRAMEWORKS+=(-library "$BUILD_DIR/ios-arm64/libhive_apple_ffi.a" -headers "$BUILD_DIR/ios-arm64/include")
fi

# iOS Simulator
IOS_SIM_LIB="$FFI_DIR/target/aarch64-apple-ios-sim/release/libhive_apple_ffi.a"
if [ -f "$IOS_SIM_LIB" ]; then
    echo "Adding iOS Simulator arm64..."
    mkdir -p "$BUILD_DIR/ios-sim-arm64"
    cp "$IOS_SIM_LIB" "$BUILD_DIR/ios-sim-arm64/"
    cp -r "$INCLUDE_DIR" "$BUILD_DIR/ios-sim-arm64/include"
    FRAMEWORKS+=(-library "$BUILD_DIR/ios-sim-arm64/libhive_apple_ffi.a" -headers "$BUILD_DIR/ios-sim-arm64/include")
fi

# Create xcframework
if [ ${#FRAMEWORKS[@]} -gt 0 ]; then
    xcodebuild -create-xcframework \
        "${FRAMEWORKS[@]}" \
        -output "$XCFRAMEWORK_DIR"
    echo ""
    echo "=== xcframework created: $XCFRAMEWORK_DIR ==="
else
    echo "Error: No libraries available to create xcframework"
    exit 1
fi

echo ""
echo "=== Build complete ==="
echo ""
echo "Generated files:"
echo "  - Swift bindings: HiveTest/HiveBridge/hive_apple_ffi.swift"
echo "  - xcframework: build/HiveFFI.xcframework"
echo ""
echo "To run the app:"
echo "  1. Open HiveTest in Xcode"
echo "  2. Add HiveFFI.xcframework to the project"
echo "  3. Build and run"
