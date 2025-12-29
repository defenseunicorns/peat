.PHONY: help build-android build-android-demo deploy-android clean check-android check-rust check

# ============================================
# HIVE BLE Android Build & Deploy
# ============================================

# Configuration
ANDROID_SDK ?= $(HOME)/Android/Sdk
ADB ?= $(ANDROID_SDK)/platform-tools/adb
JAVA_HOME := $(HOME)/.local/share/mise/installs/java/temurin-17.0.17+10
DEMO_APK ?= ../examples/android-hive-demo/app/build/outputs/apk/debug/app-debug.apk

help:
	@echo "HIVE BLE Android Build & Deploy"
	@echo ""
	@echo "Targets:"
	@echo "  build-android      - Build native library for Android (arm64-v8a)"
	@echo "  build-android-demo - Build the Android demo APK"
	@echo "  deploy-android     - Deploy demo APK to connected device"
	@echo "  android            - Build everything and deploy to device"
	@echo "  check-rust         - Check Rust library compiles for Android"
	@echo "  check-android      - Check for Kotlin compilation errors"
	@echo "  check              - Check both Rust and Kotlin"
	@echo "  clean              - Clean build artifacts"
	@echo ""
	@echo "Configuration (override with make VAR=value):"
	@echo "  ANDROID_SDK=$(ANDROID_SDK)"
	@echo "  JAVA_HOME=$(JAVA_HOME)"

# Build native library for Android (both architectures)
build-android:
	@echo "Building hive-btle native library for Android (arm64-v8a, armeabi-v7a)..."
	CXXFLAGS="-include cstdint" cargo ndk -t arm64-v8a -t armeabi-v7a -o android/src/main/jniLibs build --release -p hive-btle --features android
	@echo "✓ Native libraries built:"
	@echo "  - android/src/main/jniLibs/arm64-v8a/libhive_btle.so"
	@echo "  - android/src/main/jniLibs/armeabi-v7a/libhive_btle.so"

# Build Android demo APK
build-android-demo: build-android
	@echo "Building Android demo APK..."
	JAVA_HOME=$(JAVA_HOME) ../examples/android-hive-demo/gradlew -p ../examples/android-hive-demo assembleDebug
	@echo "✓ APK built: $(DEMO_APK)"

# Deploy to connected Android device
deploy-android:
	@echo "Deploying to Android device..."
	$(ADB) install -r $(DEMO_APK)
	@echo "✓ Deployed to device"

# Full build and deploy
android: build-android-demo deploy-android
	@echo "✓ Build and deploy complete"

# Check Rust library for Android (requires Android target)
check-rust:
	@echo "Checking hive-btle Rust library for Android..."
	CXXFLAGS="-include cstdint" cargo ndk -t arm64-v8a check -p hive-btle --features android

# Check for Kotlin compilation errors
check-android:
	@echo "Checking Android demo for compilation errors..."
	JAVA_HOME=$(JAVA_HOME) ../examples/android-hive-demo/gradlew -p ../examples/android-hive-demo compileDebugKotlin 2>&1 | grep -E "e:|error:|Error:" || true

# Check both Rust and Kotlin
check: check-rust check-android
	@echo "✓ All checks complete"

# Clean build artifacts
clean:
	@echo "Cleaning Android build artifacts..."
	rm -rf android/src/main/jniLibs/arm64-v8a/libhive_btle.so
	rm -rf android/src/main/jniLibs/armeabi-v7a/libhive_btle.so
	rm -rf ../examples/android-hive-demo/app/build
	@echo "✓ Clean complete"
