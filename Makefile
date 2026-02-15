.PHONY: help clean clean-ditto build test test-unit test-integration test-e2e test-fast fmt clippy check pre-commit ci \
       build-ble-responder deploy-ble-responder start-ble-responder stop-ble-responder \
       build-ble-test-app deploy-ble-test-app ble-test ble-test-logs ble-responder-logs clean-ble-test \
       build-iroh-test-peer deploy-iroh-test-peer start-iroh-test-peer stop-iroh-test-peer \
       dual-transport-test iroh-test-peer-logs

# ============================================
# HIVE Protocol Development Makefile
# ============================================

help:
	@echo "HIVE Protocol Development & Testing"
	@echo ""
	@echo "Development:"
	@echo "  build        - Build all crates"
	@echo "  fmt          - Format code"
	@echo "  clippy       - Run linter"
	@echo "  check        - Run fmt + clippy + test"
	@echo "  clean        - Remove build artifacts"
	@echo "  clean-labs   - Clean up all containerlab topologies and containers"
	@echo ""
	@echo "Testing (Tiered - use these for fast iteration):"
	@echo "  test-fast    - Unit tests only, ~30s (use during development)"
	@echo "  test-unit    - Unit tests with nextest (~30s)"
	@echo "  test-integration - Integration tests, no E2E (~2 min)"
	@echo "  test-e2e     - E2E tests only (~5 min)"
	@echo "  test         - All tests (unit + integration + E2E)"
	@echo ""
	@echo "Quick Validation:"
	@echo "  validate              - Quick validation (Traditional 24-node) ⭐ Start here!"
	@echo "  validate-event-driven - Validate event-driven hierarchical (24-node, 60s)"
	@echo ""
	@echo "Architecture Comparison (O(n²) vs O(n log n)):"
	@echo "  baseline-client-server    - Traditional hub-spoke (all node counts)"
	@echo "  baseline-mesh             - Traditional P2P mesh (all node counts)"
	@echo "  hive-hierarchical         - HIVE hierarchical (all node counts)"
	@echo "  compare-architectures     - Run all 3 architectures for comparison"
	@echo ""
	@echo "Bandwidth Testing:"
	@echo "  bandwidth-matrix          - Test all architectures × bandwidths"
	@echo "  bandwidth-constrained BW=x - Test specific bandwidth (1gbps|100mbps|1mbps|256kbps)"
	@echo ""
	@echo "Backend Comparison:"
	@echo "  backend-comparison              - Quick comparison: Ditto vs Automerge (24-node, 4 bandwidths)"
	@echo "  backend-comparison-hierarchical - Traditional vs Hierarchical (O(n²) vs O(n log n)) ⭐ Recommended!"
	@echo "  traditional-baseline            - Traditional tests only (run once, save for reuse)"
	@echo "  hierarchical-only               - Hierarchical only (reuses traditional, faster iteration)"
	@echo "  backend-comparison-scaling      - Legacy: Full scaling study (48 tests)"
	@echo ""
	@echo "Full Experimental Matrix:"
	@echo "  matrix-full              - Complete: 3 architectures × all sizes × 4 bandwidths"
	@echo "  matrix-analyze DIR=x     - Analyze results from matrix run"
	@echo ""
	@echo "Build Commands:"
	@echo "  build                    - Build all Rust crates"
	@echo "  build-docker             - Build Docker image (run once before tests)"
	@echo ""
	@echo "Android (ATAK Plugin):"
	@echo "  build-android            - Cross-compile hive-ffi for Android"
	@echo "  build-atak-plugin        - Build ATAK plugin APK (includes native libs)"
	@echo "  deploy-atak-plugin       - Deploy APK to connected device"
	@echo "  android                  - Build and deploy ATAK plugin"
	@echo "  clean-android            - Clean Android build artifacts"
	@echo ""
	@echo "BLE Functional Test (Pi-to-Android):"
	@echo "  build-ble-responder      - Cross-compile ble_responder for Pi (aarch64)"
	@echo "  deploy-ble-responder     - Deploy ble_responder to rpi-ci"
	@echo "  start-ble-responder      - Start ble_responder on rpi-ci"
	@echo "  stop-ble-responder       - Stop ble_responder on rpi-ci"
	@echo "  build-ble-test-app       - Build Android BLE test APK (includes native libs)"
	@echo "  deploy-ble-test-app      - Deploy BLE test APK to connected Android device"
	@echo "  ble-test                 - Full pipeline: build + deploy all + start responder"
	@echo "  ble-test-logs            - Show logcat from running BLE test"
	@echo "  clean-ble-test           - Clean BLE test build artifacts"
	@echo ""
	@echo "Dual-Transport Test (BLE + QUIC on same Pi):"
	@echo "  build-iroh-test-peer     - Cross-compile iroh_test_peer for Pi (aarch64)"
	@echo "  deploy-iroh-test-peer    - Deploy iroh_test_peer to rpi-ci"
	@echo "  start-iroh-test-peer     - Start iroh_test_peer on rpi-ci"
	@echo "  stop-iroh-test-peer      - Stop iroh_test_peer on rpi-ci"
	@echo "  dual-transport-test      - Full dual-transport pipeline (BLE + QUIC)"
	@echo ""
	@echo "Legacy E-Series Tests (for reference):"
	@echo "  e11-modes                - Test HIVE modes (legacy)"
	@echo "  e12-comprehensive        - Full validation suite (legacy)"

# ============================================
# Development
# ============================================

build:
	@echo "Building all crates..."
	cargo build

build-docker:
	@echo "Building Docker image for hive-sim..."
	@cd hive-sim && docker build -f Dockerfile -t hive-sim-node:latest ..
	@echo "✓ Docker image built: hive-sim-node:latest"

# ============================================
# Tiered Testing (for fast development iteration)
# ============================================

# test-fast: Quickest feedback loop for development (~30s)
# Use this during active development for rapid iteration
test-fast: clean-ditto
	@echo "Running unit tests (fast mode)..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo nextest run --lib --no-fail-fast; \
		else \
			cargo nextest run --lib --no-fail-fast; \
		fi; \
	else \
		echo "Note: Install cargo-nextest for 2x faster tests: cargo install cargo-nextest"; \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo test --lib; \
		else \
			cargo test --lib; \
		fi; \
	fi

# test-unit: Unit tests only with nextest (~30s)
test-unit: clean-ditto
	@echo "Running unit tests..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo nextest run --lib --workspace --exclude hive-ffi; \
		else \
			cargo nextest run --lib --workspace --exclude hive-ffi; \
		fi; \
	else \
		echo "Note: Install cargo-nextest for 2x faster tests: cargo install cargo-nextest"; \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo test --lib --workspace --exclude hive-ffi; \
		else \
			cargo test --lib --workspace --exclude hive-ffi; \
		fi; \
	fi

# test-integration: Integration tests excluding E2E (~2 min)
test-integration: clean-ditto
	@echo "Running integration tests (excluding E2E)..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo nextest run --workspace --exclude hive-ffi -E 'not test(e2e)'; \
		else \
			cargo nextest run --workspace --exclude hive-ffi -E 'not test(e2e)'; \
		fi; \
	else \
		echo "Note: Install cargo-nextest for 2x faster tests: cargo install cargo-nextest"; \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo test --workspace --exclude hive-ffi; \
		else \
			cargo test --workspace --exclude hive-ffi; \
		fi; \
	fi

# test-e2e: E2E tests only (~5 min)
test-e2e: clean-ditto
	@echo "Running E2E tests..."
	@if [ ! -f .env ]; then \
		echo "⚠️  Warning: .env file not found. Ditto tests may be skipped."; \
		echo "   Create .env with DITTO_APP_ID, DITTO_OFFLINE_TOKEN, DITTO_SHARED_KEY"; \
	fi
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo nextest run --workspace --exclude hive-ffi -E 'test(e2e)'; \
		else \
			cargo nextest run --workspace --exclude hive-ffi -E 'test(e2e)'; \
		fi; \
	else \
		echo "Note: Install cargo-nextest for 2x faster tests: cargo install cargo-nextest"; \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo test --workspace --exclude hive-ffi e2e; \
		else \
			cargo test --workspace --exclude hive-ffi e2e; \
		fi; \
	fi

# test: Run all tests (unit + integration + E2E)
test: clean-ditto
	@echo "Running all tests..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo nextest run --workspace --exclude hive-ffi; \
		else \
			cargo nextest run --workspace --exclude hive-ffi; \
		fi; \
	else \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo test --workspace --exclude hive-ffi; \
		else \
			cargo test --workspace --exclude hive-ffi; \
		fi; \
	fi

# Run baseline comparison tests only (Containerlab-based)
test-baseline:
	@echo "Running baseline comparison tests..."
	@cd hive-sim && ./run-baseline-comparison.sh

fmt:
	@echo "Formatting code..."
	cargo fmt --all

clippy:
	@echo "Running clippy..."
	cargo clippy --all-targets --all-features -- -D warnings

check: fmt clippy test
	@echo "✅ All checks passed!"

pre-commit: clean-ditto
	@echo "Running pre-commit checks..."
	@cargo fmt --all
	@cargo clippy --all-targets --all-features --workspace --exclude hive-ffi --exclude hive-inference -- -D warnings
	@cargo clippy --all-targets --workspace -p hive-inference -- -D warnings
	@$(MAKE) test-unit
	@echo "✅ Pre-commit checks passed!"

ci: clean-ditto
	@echo "Running CI pipeline..."
	@cargo fmt --all -- --check
	@cargo clippy --all-targets --all-features --workspace --exclude hive-ffi --exclude hive-inference -- -D warnings
	@cargo clippy --all-targets --workspace -p hive-inference -- -D warnings
	@$(MAKE) test-integration
	@echo "✅ CI pipeline passed!"

clean: clean-ditto
	@echo "Cleaning build artifacts..."
	cargo clean

clean-ditto:
	@find . -type d -name ".ditto*" -exec rm -rf {} + 2>/dev/null || true
	@rm -rf /tmp/hive-persistence-test-* 2>/dev/null || true

clean-labs:
	@echo "Cleaning up all containerlab topologies..."
	@cd hive-sim && ./cleanup-all-labs.sh

# ============================================
# Quick Validation
# ============================================

validate:
	@$(MAKE) -C hive-sim validate

# ============================================
# Architecture Comparison (O(n²) vs O(n))
# ============================================

# Traditional Client-Server (Hub-Spoke) - Shows O(n²) at HQ bottleneck
baseline-client-server:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Traditional Client-Server Baseline                       ║"
	@echo "║  Hub-and-Spoke: All nodes → HQ → All nodes                ║"
	@echo "║  Expected: O(n²) message volume at HQ                     ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd hive-sim && ./test-traditional-client-server-suite.sh

# Traditional P2P Mesh - Shows O(n²) connections AND messages
baseline-mesh:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Traditional P2P Mesh Baseline                            ║"
	@echo "║  Full Mesh: Every node ↔ Every other node                 ║"
	@echo "║  Expected: O(n²) connections + O(n²) messages per node    ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd hive-sim && ./test-traditional-mesh-suite.sh

# HIVE Hierarchical - Shows O(n log n) scaling
hive-hierarchical:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  HIVE Hierarchical Protocol                               ║"
	@echo "║  Hierarchical aggregation with differential filtering     ║"
	@echo "║  Expected: O(n log n) message volume via aggregation      ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd hive-sim && ./test-hive-hierarchical-suite.sh

# Run all three architectures for direct comparison
compare-architectures:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Architecture Comparison Suite                            ║"
	@echo "║  Testing all 3 architectures at multiple scales           ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@$(MAKE) baseline-client-server
	@$(MAKE) baseline-mesh
	@$(MAKE) hive-hierarchical
	@echo ""
	@echo "✅ Architecture comparison complete"
	@echo "📊 Compare results in hive-sim/architecture-comparison-*/"

# ============================================
# Backend Comparison
# ============================================

backend-comparison:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Backend Comparison: Ditto vs Automerge-Iroh             ║"
	@echo "║  2 Backends × 4 Bandwidths = 8 test runs                  ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd hive-sim && ./test-backend-comparison.sh

# Quick validation: Test event-driven hierarchical (24-node only)
validate-event-driven:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Event-Driven Hierarchical Validation (24 nodes)          ║"
	@echo "║  Zero polling - measuring REAL empirical latencies        ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd hive-sim && ./validate-event-driven.sh

backend-comparison-scaling:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Backend Scaling Comparison                               ║"
	@echo "║  2 Backends × 3 Node Counts × 2 Topologies × 4 BW         ║"
	@echo "║  = 48 test runs with real sync metrics                    ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "⚠️  WARNING: This will take several hours"
	@echo "   Tests: 24/48/96 nodes × Traditional/Hierarchical"
	@echo "   Metrics: Document sync latency (P50/P95/P99)"
	@echo ""
	@cd hive-sim && ./test-backend-comparison-scaling.sh

backend-comparison-hierarchical:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Hierarchical Backend Comparison                          ║"
	@echo "║  Traditional (O(n²)) vs Hierarchical (O(n log n))         ║"
	@echo "║  2 Backends × 3 Scales × 2 Topologies × 4 BW = 48 tests   ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd hive-sim && ./test-backend-comparison-hierarchical.sh

# Traditional baseline tests - run once and reuse (NO CRDT)
traditional-baseline:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Traditional Baseline Tests (NO CRDT)                     ║"
	@echo "║  Hub-Spoke Client-Server (O(n²) scaling)                  ║"
	@echo "║  4 Scales × 4 BW = 16 tests (~45 mins)                    ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd hive-sim && ./test-traditional-baseline.sh

# Hierarchical tests only - reuse traditional results from previous run
hierarchical-only:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Hierarchical Tests Only (Faster Iteration)              ║"
	@echo "║  Reuses traditional results, only tests hierarchical      ║"
	@echo "║  2 Backends × 3 Scales × 4 BW = 24 tests (~60 mins)       ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd hive-sim && ./test-hierarchical-only.sh

# ============================================
# Bandwidth Testing
# ============================================

# Test all architectures across different bandwidth constraints
bandwidth-matrix:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Bandwidth Constraint Matrix                              ║"
	@echo "║  3 Architectures × 4 Bandwidths = 12 test runs            ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd hive-sim && ./test-bandwidth-matrix.sh

# Test specific bandwidth constraint
bandwidth-constrained:
	@if [ -z "$(BW)" ]; then \
		echo "Error: BW parameter required"; \
		echo "Usage: make bandwidth-constrained BW=256kbps"; \
		echo "Options: 1gbps, 100mbps, 1mbps, 256kbps"; \
		exit 1; \
	fi
	@echo "Testing all architectures at $(BW) bandwidth..."
	@cd hive-sim && ./test-bandwidth-constrained.sh $(BW)

# ============================================
# Full Experimental Matrix
# ============================================

# Complete matrix: 3 architectures × all sizes × 4 bandwidths
matrix-full:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Complete Experimental Matrix                             ║"
	@echo "║  3 Architectures × N Sizes × 4 Bandwidths                 ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "⚠️  WARNING: This will take several hours"
	@echo ""
	@cd hive-sim && ./run-complete-matrix.sh

# Analyze matrix results
matrix-analyze:
	@if [ -z "$(DIR)" ]; then \
		echo "Usage: make matrix-analyze DIR=<results-directory>"; \
		echo "Example: make matrix-analyze DIR=hive-sim/matrix-results-20251118-120000"; \
		exit 1; \
	fi
	@echo "Generating comparative analysis for $(DIR)..."
	@cd hive-sim && python3 analyze-matrix-results.py $(DIR)

# ============================================
# Android Cross-Compilation
# ============================================

# Build hive-ffi native library for Android
# Requires: cargo-ndk (cargo install cargo-ndk)
# Outputs to: atak-plugin/app/libs/{arm64-v8a,armeabi-v7a}/libhive_ffi.so
build-android:
	@echo "Building hive-ffi for Android..."
	@command -v cargo-ndk >/dev/null 2>&1 || { echo "Error: cargo-ndk not found. Install with: cargo install cargo-ndk"; exit 1; }
	@export PATH="$$HOME/Android/Sdk/ndk/27.0.12077973/toolchains/llvm/prebuilt/linux-x86_64/bin:$$PATH" && \
		cargo ndk -t arm64-v8a -t armeabi-v7a -o atak-plugin/app/libs build --release -p hive-ffi --features bluetooth
	@echo "✓ Native libraries built:"
	@ls -la atak-plugin/app/libs/arm64-v8a/libhive_ffi.so atak-plugin/app/libs/armeabi-v7a/libhive_ffi.so

# Build ATAK plugin with native libs
build-atak-plugin: build-android
	@echo "Building ATAK plugin..."
	@cd atak-plugin && ./gradlew assembleCivDebug
	@echo "✓ ATAK plugin built"

# Deploy ATAK plugin to connected device
deploy-atak-plugin:
	@echo "Deploying ATAK plugin..."
	@adb install -r atak-plugin/app/build/outputs/apk/civ/debug/ATAK-Plugin-HIVE-*.apk
	@echo "✓ Deployed to device"

# Full Android build and deploy
android: build-atak-plugin deploy-atak-plugin
	@echo "✓ Android build and deploy complete"

# Clean Android build artifacts
clean-android:
	@echo "Cleaning Android build artifacts..."
	@rm -rf atak-plugin/app/libs/arm64-v8a/libhive_ffi.so
	@rm -rf atak-plugin/app/libs/armeabi-v7a/libhive_ffi.so
	@rm -rf atak-plugin/app/build
	@echo "✓ Android artifacts cleaned"

# ============================================
# BLE Functional Test (Pi-to-Android)
# ============================================
# Proves dual-transport (Iroh QUIC + BLE) with Pi running ble_responder
# and Android device running the BLE test app.
#
# Prerequisites:
#   - cross (cargo install cross)
#   - cargo-ndk (cargo install cargo-ndk)
#   - SSH access to rpi-ci (kit@rpi-ci)
#   - Android device connected via ADB
#   - ANDROID_HOME or Android SDK at ~/Android/Sdk

HIVE_BTLE_DIR ?= $(HOME)/Code/revolve/hive-btle
BLE_TEST_PI ?= rpi-ci
BLE_TEST_PI_USER ?= kit
BLE_TEST_MESH_ID ?= FUNCTEST
BLE_TEST_CALLSIGN ?= PI-RESP

# QUIC test peer (iroh_test_peer) runs on the SAME Pi as ble_responder
# Both coexist: BLE uses BlueZ D-Bus, QUIC uses the network stack
IROH_TEST_PORT ?= 42009
BLE_TEST_PI_IP ?= 192.168.228.13

# Cross-compile ble_responder for Raspberry Pi (aarch64)
build-ble-responder:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Building ble_responder for aarch64 (Raspberry Pi)        ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@command -v cross >/dev/null 2>&1 || { echo "Error: cross not found. Install with: cargo install cross"; exit 1; }
	cd $(HIVE_BTLE_DIR) && cross build --release \
		--target aarch64-unknown-linux-gnu \
		--features linux \
		--example ble_responder
	@echo "✓ ble_responder built:"
	@ls -la $(HIVE_BTLE_DIR)/target/aarch64-unknown-linux-gnu/release/examples/ble_responder

# Deploy ble_responder binary to Pi
deploy-ble-responder: build-ble-responder
	@echo "Deploying ble_responder to $(BLE_TEST_PI_USER)@$(BLE_TEST_PI)..."
	scp $(HIVE_BTLE_DIR)/target/aarch64-unknown-linux-gnu/release/examples/ble_responder \
		$(BLE_TEST_PI_USER)@$(BLE_TEST_PI):~/ble_responder
	@echo "✓ Deployed to $(BLE_TEST_PI):~/ble_responder"

# Start ble_responder on Pi (backgrounded, logs to ~/ble_responder.log)
start-ble-responder:
	@echo "Starting ble_responder on $(BLE_TEST_PI)..."
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pkill -x ble_responder 2>/dev/null || true'
	@sleep 1
	ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) \
		'nohup ~/ble_responder --mesh-id $(BLE_TEST_MESH_ID) --callsign $(BLE_TEST_CALLSIGN) \
		> ~/ble_responder.log 2>&1 & echo $$!'
	@sleep 2
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pgrep -x ble_responder >/dev/null && echo "✓ ble_responder running (PID: $$(pgrep -x ble_responder))" || echo "✗ ble_responder failed to start"'

# Stop ble_responder on Pi
stop-ble-responder:
	@echo "Stopping ble_responder on $(BLE_TEST_PI)..."
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pkill -x ble_responder 2>/dev/null && echo "✓ Stopped" || echo "Not running"'

# Build Android BLE test APK (cross-compile libhive_ffi + Gradle build)
build-ble-test-app: build-android
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Building Android BLE Test App                            ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@mkdir -p android-ble-test/app/src/main/jniLibs/arm64-v8a
	cp atak-plugin/app/libs/arm64-v8a/libhive_ffi.so \
		android-ble-test/app/src/main/jniLibs/arm64-v8a/
	@echo "✓ Copied libhive_ffi.so to android-ble-test jniLibs"
	cd android-ble-test && ./gradlew assembleDebug
	@echo "✓ BLE test APK built:"
	@ls -la android-ble-test/app/build/outputs/apk/debug/app-debug.apk

# Deploy BLE test APK to connected Android device
deploy-ble-test-app:
	@echo "Deploying BLE test app to Android device..."
	@adb devices | grep -q 'device$$' || { echo "Error: No Android device connected"; exit 1; }
	adb install -r android-ble-test/app/build/outputs/apk/debug/app-debug.apk
	@echo "✓ Deployed to device"
	@echo "Launch with: adb shell am start -n com.revolveteam.hive.test/.MainActivity"

# Full BLE test pipeline: build everything, deploy, start responder
ble-test: deploy-ble-responder build-ble-test-app deploy-ble-test-app start-ble-responder
	@echo ""
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  BLE Functional Test Ready                                ║"
	@echo "╠════════════════════════════════════════════════════════════╣"
	@echo "║  Pi:      ble_responder running on $(BLE_TEST_PI)"
	@echo "║  Android: BLE test app installed"
	@echo "║                                                            ║"
	@echo "║  Launching test automatically...                          ║"
	@echo "║                                                            ║"
	@echo "║  Monitor:                                                  ║"
	@echo "║    make ble-test-logs        (Android logcat)             ║"
	@echo "║    make ble-responder-logs   (Pi responder log)           ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	adb shell am start -n com.revolveteam.hive.test/.MainActivity \
		--ez auto_run true

# Show Android logcat for BLE test
ble-test-logs:
	adb logcat -s HiveTest:V BleGattClient:V HiveJni:V HiveNativeLoader:V

# Show Pi responder logs
ble-responder-logs:
	ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'tail -f ~/ble_responder.log'

# Clean BLE test artifacts
clean-ble-test: stop-ble-responder
	@echo "Cleaning BLE test artifacts..."
	@rm -rf android-ble-test/app/build
	@rm -rf android-ble-test/app/src/main/jniLibs/arm64-v8a/libhive_ffi.so
	@echo "✓ BLE test artifacts cleaned"

# ============================================
# Dual-Transport Test (BLE + QUIC via iroh_test_peer)
# ============================================
# Proves simultaneous BLE + QUIC data sync with Android using ONE Pi.
# Both binaries run on the same Pi (rpi-ci):
#   - ble_responder: BLE GATT advertising + document sync (BlueZ D-Bus)
#   - iroh_test_peer: QUIC/mDNS peer + Automerge platform sync (network)
# Android discovers both: BLE advertisements + mDNS peer.

# Cross-compile iroh_test_peer for Raspberry Pi (aarch64)
build-iroh-test-peer:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Building iroh_test_peer for aarch64 (Raspberry Pi)       ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@command -v cross >/dev/null 2>&1 || { echo "Error: cross not found. Install with: cargo install cross"; exit 1; }
	CXXFLAGS="-include cstdint" cross build --release \
		--target aarch64-unknown-linux-gnu \
		--example iroh_test_peer \
		-p hive-ffi --features sync
	@echo "✓ iroh_test_peer built:"
	@ls -la target/aarch64-unknown-linux-gnu/release/examples/iroh_test_peer

# Deploy iroh_test_peer binary to Pi (same host as ble_responder)
deploy-iroh-test-peer: build-iroh-test-peer
	@echo "Deploying iroh_test_peer to $(BLE_TEST_PI_USER)@$(BLE_TEST_PI)..."
	scp target/aarch64-unknown-linux-gnu/release/examples/iroh_test_peer \
		$(BLE_TEST_PI_USER)@$(BLE_TEST_PI):~/iroh_test_peer
	@echo "✓ Deployed to $(BLE_TEST_PI):~/iroh_test_peer"

# Start iroh_test_peer on Pi (backgrounded, logs to ~/iroh_test_peer.log)
start-iroh-test-peer:
	@echo "Starting iroh_test_peer on $(BLE_TEST_PI)..."
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pkill -x iroh_test_peer 2>/dev/null || true'
	@sleep 1
	ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) \
		'nohup ~/iroh_test_peer > ~/iroh_test_peer.log 2>&1 & echo $$!'
	@sleep 3
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pgrep -x iroh_test_peer >/dev/null && echo "✓ iroh_test_peer running (PID: $$(pgrep -x iroh_test_peer))" || echo "✗ iroh_test_peer failed to start"'

# Stop iroh_test_peer on Pi
stop-iroh-test-peer:
	@echo "Stopping iroh_test_peer on $(BLE_TEST_PI)..."
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pkill -x iroh_test_peer 2>/dev/null && echo "✓ Stopped" || echo "Not running"'

# Full dual-transport test pipeline (both peers on same Pi)
dual-transport-test: deploy-ble-responder deploy-iroh-test-peer build-ble-test-app deploy-ble-test-app start-ble-responder start-iroh-test-peer
	@echo ""
	@echo "Capturing iroh_test_peer node ID from $(BLE_TEST_PI)..."
	$(eval QUIC_NODE_ID := $(shell ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'grep PEER_NODE_ID ~/iroh_test_peer.log | head -1 | cut -d= -f2'))
	@if [ -z "$(QUIC_NODE_ID)" ]; then \
		echo "✗ Failed to capture PEER_NODE_ID from iroh_test_peer log"; \
		exit 1; \
	fi
	@echo "✓ QUIC peer node ID: $(QUIC_NODE_ID)"
	@echo ""
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Dual-Transport Test Ready (BLE + QUIC)                   ║"
	@echo "╠════════════════════════════════════════════════════════════╣"
	@echo "║  Pi:      $(BLE_TEST_PI) running ble_responder + iroh_test_peer"
	@echo "║  Android: BLE test app installed                          ║"
	@echo "║                                                            ║"
	@echo "║  Android discovers Pi via:                                ║"
	@echo "║    - BLE advertisements (GATT sync)                       ║"
	@echo "║    - mDNS (QUIC platform sync)                           ║"
	@echo "║                                                            ║"
	@echo "║  Launching test with QUIC peer info...                    ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	adb shell am start -n com.revolveteam.hive.test/.MainActivity \
		--es quic_node_id "$(QUIC_NODE_ID)" \
		--es quic_address "$(BLE_TEST_PI_IP):$(IROH_TEST_PORT)" \
		--ez auto_run true
	@echo ""
	@echo "Waiting for Android test to complete..."
	@for i in $$(seq 1 90); do \
		if adb logcat -d -s HiveTest 2>/dev/null | grep -q "^.*RESULT:"; then \
			break; \
		fi; \
		sleep 1; \
	done
	@echo ""
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Android Results                                          ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@adb logcat -d -s HiveTest 2>/dev/null | grep -E "Phase|RESULT" || echo "  (no output captured)"
	@echo ""
	@echo "Waiting for Pi iroh_test_peer to finish (up to 30s)..."
	@for i in $$(seq 1 30); do \
		if ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'test ! -e /proc/$$(pgrep -x iroh_test_peer 2>/dev/null || echo 0)/status 2>/dev/null'; then \
			break; \
		fi; \
		sleep 1; \
	done
	@echo ""
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Pi Results                                               ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'cat ~/iroh_test_peer.log' 2>/dev/null | grep -E "PEER_NODE_ID|Received|PASSED|FAILED" || echo "  (no output captured)"
	@echo ""
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Final Verdict                                            ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@ANDROID_OK=$$(adb logcat -d -s HiveTest 2>/dev/null | grep -c "RESULT:.*PASSED"); \
	PI_OK=$$(ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'grep -c "Test PASSED" ~/iroh_test_peer.log 2>/dev/null' || echo 0); \
	if [ "$$ANDROID_OK" -ge 1 ] && [ "$$PI_OK" -ge 1 ]; then \
		echo "  ✓ BOTH SIDES PASSED — dual-transport verified"; \
	else \
		echo "  ✗ TEST FAILED"; \
		[ "$$ANDROID_OK" -lt 1 ] && echo "    Android: FAILED"; \
		[ "$$PI_OK" -lt 1 ] && echo "    Pi:      FAILED"; \
		exit 1; \
	fi

# Show iroh_test_peer logs from Pi
iroh-test-peer-logs:
	ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'tail -f ~/iroh_test_peer.log'

# ============================================
# Legacy E-Series Tests (kept for compatibility)
# ============================================

e11-modes:
	@echo "Running E11 mode testing (legacy)..."
	@cd hive-sim && ./test-all-modes-report.sh

e12-comprehensive:
	@echo "Running E12 comprehensive validation (legacy)..."
	@cd labs/e12-comprehensive-empirical-validation/scripts && ./run-comprehensive-suite.sh
