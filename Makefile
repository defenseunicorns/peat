.PHONY: help clean build test test-unit test-integration test-e2e test-fast fmt clippy check pre-commit ci \
       build-ble-test-app deploy-ble-test-app ble-test ble-test-logs clean-ble-test \
       build-dual-test-peer deploy-dual-test-peer start-dual-test-peer stop-dual-test-peer \
       dual-transport-test dual-test-peer-logs \
       functional-suite functional-ble functional-android functional-k8s

# ============================================
# Peat Protocol Development Makefile
# ============================================

help:
	@echo "Peat Protocol Development & Testing"
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
	@echo "  peat-hierarchical         - Peat hierarchical (all node counts)"
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
	@echo "  build-android            - Cross-compile peat-ffi for Android"
	@echo "  build-atak-plugin        - Build ATAK plugin APK (includes native libs)"
	@echo "  deploy-atak-plugin       - Deploy APK to connected device"
	@echo "  android                  - Build and deploy ATAK plugin"
	@echo "  clean-android            - Clean Android build artifacts"
	@echo ""
	@echo "BLE Functional Test (Pi-to-Android):"
	@echo "  build-ble-test-app       - Build Android BLE test APK (includes native libs)"
	@echo "  deploy-ble-test-app      - Deploy BLE test APK to connected Android device"
	@echo "  ble-test                 - Full pipeline: build + deploy all + start peer"
	@echo "  ble-test-logs            - Show logcat from running BLE test"
	@echo "  clean-ble-test           - Clean BLE test build artifacts"
	@echo ""
	@echo "Dual-Transport Test (BLE + QUIC in single Pi binary):"
	@echo "  build-dual-test-peer     - Cross-compile dual_test_peer for Pi (aarch64)"
	@echo "  deploy-dual-test-peer    - Deploy dual_test_peer to rpi-ci"
	@echo "  start-dual-test-peer     - Start dual_test_peer on rpi-ci"
	@echo "  stop-dual-test-peer      - Stop dual_test_peer on rpi-ci"
	@echo "  dual-transport-test      - Full dual-transport pipeline (BLE + QUIC)"
	@echo ""
	@echo "Functional Test Suite (all hardware tests):"
	@echo "  functional-suite         - Run ALL functional tests (BLE + Android + k8s)"
	@echo "  functional-ble           - Run only rpi-rpi BLE test"
	@echo "  functional-android       - Run only rpi-android dual-transport test"
	@echo "  functional-k8s           - Run only k8s cluster test"
	@echo ""
	@echo "Legacy E-Series Tests (for reference):"
	@echo "  e11-modes                - Test Peat modes (legacy)"
	@echo "  e12-comprehensive        - Full validation suite (legacy)"

# ============================================
# Development
# ============================================

build:
	@echo "Building all crates..."
	cargo build

build-docker:
	@echo "Building Docker image for peat-sim..."
	@cd peat-sim && docker build -f Dockerfile -t peat-sim-node:latest ..
	@echo "✓ Docker image built: peat-sim-node:latest"

# ============================================
# Tiered Testing (for fast development iteration)
# ============================================

# test-fast: Quickest feedback loop for development (~30s)
# Use this during active development for rapid iteration
test-fast:
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
test-unit:
	@echo "Running unit tests..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo nextest run --lib --workspace --exclude peat-ffi; \
		else \
			cargo nextest run --lib --workspace --exclude peat-ffi; \
		fi; \
	else \
		echo "Note: Install cargo-nextest for 2x faster tests: cargo install cargo-nextest"; \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo test --lib --workspace --exclude peat-ffi; \
		else \
			cargo test --lib --workspace --exclude peat-ffi; \
		fi; \
	fi

# test-integration: Integration tests excluding E2E (~2 min)
test-integration:
	@echo "Running integration tests (excluding E2E)..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo nextest run --workspace --exclude peat-ffi -E 'not test(e2e)'; \
		else \
			cargo nextest run --workspace --exclude peat-ffi -E 'not test(e2e)'; \
		fi; \
	else \
		echo "Note: Install cargo-nextest for 2x faster tests: cargo install cargo-nextest"; \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo test --workspace --exclude peat-ffi; \
		else \
			cargo test --workspace --exclude peat-ffi; \
		fi; \
	fi

# test-e2e: E2E tests only (~5 min)
test-e2e:
	@echo "Running E2E tests..."
	@if [ ! -f .env ]; then \
		echo "ℹ️  No .env file found; tests needing PEAT_APP_ID / PEAT_SECRET_KEY may be skipped."; \
	fi
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo nextest run --workspace --exclude peat-ffi -E 'test(e2e)'; \
		else \
			cargo nextest run --workspace --exclude peat-ffi -E 'test(e2e)'; \
		fi; \
	else \
		echo "Note: Install cargo-nextest for 2x faster tests: cargo install cargo-nextest"; \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo test --workspace --exclude peat-ffi e2e; \
		else \
			cargo test --workspace --exclude peat-ffi e2e; \
		fi; \
	fi

# test: Run all tests (unit + integration + E2E)
test:
	@echo "Running all tests..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo nextest run --workspace --exclude peat-ffi; \
		else \
			cargo nextest run --workspace --exclude peat-ffi; \
		fi; \
	else \
		if [ -f .env ]; then \
			export $$(grep -v '^#' .env | xargs) && cargo test --workspace --exclude peat-ffi; \
		else \
			cargo test --workspace --exclude peat-ffi; \
		fi; \
	fi

# Run baseline comparison tests only (Containerlab-based)
test-baseline:
	@echo "Running baseline comparison tests..."
	@cd peat-sim && ./run-baseline-comparison.sh

fmt:
	@echo "Formatting code..."
	cargo fmt --all

clippy:
	@echo "Running clippy..."
	cargo clippy --all-targets --all-features -- -D warnings

check: fmt clippy test
	@echo "✅ All checks passed!"

pre-commit:
	@echo "Running pre-commit checks..."
	@cargo fmt --all
	@cargo clippy --all-targets --all-features --workspace --exclude peat-ffi -- -D warnings
	@$(MAKE) test-unit
	@echo "✅ Pre-commit checks passed!"

ci:
	@echo "Running CI pipeline..."
	@cargo fmt --all -- --check
	@cargo clippy --all-targets --all-features --workspace --exclude peat-ffi -- -D warnings
	@$(MAKE) test-integration
	@echo "✅ CI pipeline passed!"

clean:
	@echo "Cleaning build artifacts..."
	cargo clean

clean-labs:
	@echo "Cleaning up all containerlab topologies..."
	@cd peat-sim && ./cleanup-all-labs.sh

# ============================================
# Quick Validation
# ============================================

validate:
	@$(MAKE) -C peat-sim validate

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
	@cd peat-sim && ./test-traditional-client-server-suite.sh

# Traditional P2P Mesh - Shows O(n²) connections AND messages
baseline-mesh:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Traditional P2P Mesh Baseline                            ║"
	@echo "║  Full Mesh: Every node ↔ Every other node                 ║"
	@echo "║  Expected: O(n²) connections + O(n²) messages per node    ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd peat-sim && ./test-traditional-mesh-suite.sh

# Peat Hierarchical - Shows O(n log n) scaling
peat-hierarchical:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Peat Hierarchical Protocol                               ║"
	@echo "║  Hierarchical aggregation with differential filtering     ║"
	@echo "║  Expected: O(n log n) message volume via aggregation      ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd peat-sim && ./test-peat-hierarchical-suite.sh

# Run all three architectures for direct comparison
compare-architectures:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Architecture Comparison Suite                            ║"
	@echo "║  Testing all 3 architectures at multiple scales           ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@$(MAKE) baseline-client-server
	@$(MAKE) baseline-mesh
	@$(MAKE) peat-hierarchical
	@echo ""
	@echo "✅ Architecture comparison complete"
	@echo "📊 Compare results in peat-sim/architecture-comparison-*/"

# ============================================
# Backend Comparison
# ============================================

backend-comparison:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Backend Comparison: Ditto vs Automerge-Iroh             ║"
	@echo "║  2 Backends × 4 Bandwidths = 8 test runs                  ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd peat-sim && ./test-backend-comparison.sh

# Quick validation: Test event-driven hierarchical (24-node only)
validate-event-driven:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Event-Driven Hierarchical Validation (24 nodes)          ║"
	@echo "║  Zero polling - measuring REAL empirical latencies        ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd peat-sim && ./validate-event-driven.sh

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
	@cd peat-sim && ./test-backend-comparison-scaling.sh

backend-comparison-hierarchical:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Hierarchical Backend Comparison                          ║"
	@echo "║  Traditional (O(n²)) vs Hierarchical (O(n log n))         ║"
	@echo "║  2 Backends × 3 Scales × 2 Topologies × 4 BW = 48 tests   ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd peat-sim && ./test-backend-comparison-hierarchical.sh

# Traditional baseline tests - run once and reuse (NO CRDT)
traditional-baseline:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Traditional Baseline Tests (NO CRDT)                     ║"
	@echo "║  Hub-Spoke Client-Server (O(n²) scaling)                  ║"
	@echo "║  4 Scales × 4 BW = 16 tests (~45 mins)                    ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd peat-sim && ./test-traditional-baseline.sh

# Hierarchical tests only - reuse traditional results from previous run
hierarchical-only:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Hierarchical Tests Only (Faster Iteration)              ║"
	@echo "║  Reuses traditional results, only tests hierarchical      ║"
	@echo "║  2 Backends × 3 Scales × 4 BW = 24 tests (~60 mins)       ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd peat-sim && ./test-hierarchical-only.sh

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
	@cd peat-sim && ./test-bandwidth-matrix.sh

# Test specific bandwidth constraint
bandwidth-constrained:
	@if [ -z "$(BW)" ]; then \
		echo "Error: BW parameter required"; \
		echo "Usage: make bandwidth-constrained BW=256kbps"; \
		echo "Options: 1gbps, 100mbps, 1mbps, 256kbps"; \
		exit 1; \
	fi
	@echo "Testing all architectures at $(BW) bandwidth..."
	@cd peat-sim && ./test-bandwidth-constrained.sh $(BW)

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
	@cd peat-sim && ./run-complete-matrix.sh

# Analyze matrix results
matrix-analyze:
	@if [ -z "$(DIR)" ]; then \
		echo "Usage: make matrix-analyze DIR=<results-directory>"; \
		echo "Example: make matrix-analyze DIR=peat-sim/matrix-results-20251118-120000"; \
		exit 1; \
	fi
	@echo "Generating comparative analysis for $(DIR)..."
	@cd peat-sim && python3 analyze-matrix-results.py $(DIR)

# ============================================
# Android Cross-Compilation
# ============================================

# Build peat-ffi native library for Android
# Requires: cargo-ndk (cargo install cargo-ndk)
# Outputs to: atak-plugin/app/libs/{arm64-v8a,armeabi-v7a}/libpeat_ffi.so
build-android:
	@echo "Building peat-ffi for Android..."
	@command -v cargo-ndk >/dev/null 2>&1 || { echo "Error: cargo-ndk not found. Install with: cargo install cargo-ndk"; exit 1; }
	@export PATH="$$HOME/Android/Sdk/ndk/27.0.12077973/toolchains/llvm/prebuilt/linux-x86_64/bin:$$PATH" && \
		cargo ndk -t arm64-v8a -t armeabi-v7a -o atak-plugin/app/libs build --release -p peat-ffi --features bluetooth
	@echo "✓ Native libraries built:"
	@ls -la atak-plugin/app/libs/arm64-v8a/libpeat_ffi.so atak-plugin/app/libs/armeabi-v7a/libpeat_ffi.so

# Build ATAK plugin with native libs
# Kotlin 2.1.x can't parse Java 25 version strings, so pin to JDK 21
ATAK_JAVA_HOME ?= /usr/lib/jvm/java-21-openjdk
build-atak-plugin: build-android
	@echo "Building ATAK plugin (JDK 21)..."
	@cd atak-plugin && JAVA_HOME=$(ATAK_JAVA_HOME) ./gradlew assembleCivDebug
	@echo "✓ ATAK plugin built"

# Deploy ATAK plugin to connected device
deploy-atak-plugin:
	@echo "Deploying ATAK plugin..."
	@adb install -r atak-plugin/app/build/outputs/apk/civ/debug/ATAK-Plugin-Peat-*.apk
	@echo "✓ Deployed to device"

# Full Android build and deploy
android: build-atak-plugin deploy-atak-plugin
	@echo "✓ Android build and deploy complete"

# Clean Android build artifacts
clean-android:
	@echo "Cleaning Android build artifacts..."
	@rm -rf atak-plugin/app/libs/arm64-v8a/libpeat_ffi.so
	@rm -rf atak-plugin/app/libs/armeabi-v7a/libpeat_ffi.so
	@rm -rf atak-plugin/app/build
	@echo "✓ Android artifacts cleaned"

# ============================================
# Demo Automation (ATAK + Sim Mesh)
# ============================================
# Full build→deploy→configure→launch loop for development iteration.
#
# Quick reference:
#   make demo-atak          # Build + deploy + configure + launch ATAK plugin
#   make demo-restart-atak  # Stop + clear + relaunch (no rebuild)
#   make demo-sim           # Build docker + deploy containerlab + warmup
#   make demo               # Full loop: sim + ATAK
#   make demo-verify        # Check ATAK logs for cell sync status
#   make demo-stop          # Tear down sim + stop ATAK

TOPOLOGY ?= peat-sim/topologies/lab4-48n-1gbps.yaml
CLAB_PREFIX ?= clab-lab4-48n
COMMANDER_CONTAINER ?= $(CLAB_PREFIX)-company-ALPHA-commander
ATAK_PACKAGE ?= com.atakmap.app.civ
PEAT_PLUGIN_ID ?= com.defenseunicorns.atak.peat

# Full ATAK plugin build → deploy → configure → launch
demo-atak: build-atak-plugin deploy-atak-plugin configure-atak
	@sleep 2
	@$(MAKE) start-atak
	@echo "✓ ATAK plugin built, deployed, configured, and launched"

# Enable plugin + clear stale store (adb install always disables the plugin)
# Must stop ATAK first so SharedPreferences aren't held open
configure-atak:
	@echo "Stopping ATAK for configuration..."
	@adb shell am force-stop $(ATAK_PACKAGE) 2>/dev/null || true
	@sleep 1
	@echo "Enabling Peat plugin..."
	@adb shell "run-as $(ATAK_PACKAGE) sed -i 's/shouldLoad-$(PEAT_PLUGIN_ID)\" value=\"false\"/shouldLoad-$(PEAT_PLUGIN_ID)\" value=\"true\"/' /data/data/$(ATAK_PACKAGE)/shared_prefs/$(ATAK_PACKAGE)_preferences.xml" 2>/dev/null || echo "  (prefs file not found — first install, plugin will auto-enable)"
	@echo "Clearing stale Peat store..."
	@adb shell "run-as $(ATAK_PACKAGE) rm -rf /data/user/0/$(ATAK_PACKAGE)/files/peat" 2>/dev/null || true
	@echo "✓ ATAK configured for Peat plugin"

# Force-stop and relaunch ATAK (no rebuild)
start-atak:
	@echo "Starting ATAK..."
	@adb shell am force-stop $(ATAK_PACKAGE) 2>/dev/null || true
	@sleep 1
	@adb shell am start -n $(ATAK_PACKAGE)/com.atakmap.app.ATAKActivity
	@echo "✓ ATAK started"

# Stop ATAK
stop-atak:
	@echo "Stopping ATAK..."
	@adb shell am force-stop $(ATAK_PACKAGE) 2>/dev/null || true
	@echo "✓ ATAK stopped"

# Quick restart: stop → clear store → relaunch (skips build)
demo-restart-atak: stop-atak
	@echo "Clearing stale Peat store..."
	@adb shell "run-as $(ATAK_PACKAGE) rm -rf /data/user/0/$(ATAK_PACKAGE)/files/peat" 2>/dev/null || true
	@sleep 1
	@$(MAKE) start-atak
	@echo "✓ ATAK restarted with clean Peat store"

# Build docker image + deploy containerlab + wait for warmup
demo-sim: build-docker
	@echo "Deploying sim topology: $(TOPOLOGY)..."
	@sudo BACKEND=automerge CAP_IN_MEMORY=true containerlab deploy -t $(TOPOLOGY) --reconfigure --timeout 5m
	@echo "Waiting for sim warmup (both platoons reporting to commander)..."
	@for i in $$(seq 1 60); do \
		if docker exec $(COMMANDER_CONTAINER) cat /data/logs/company-ALPHA-commander.metrics.log 2>/dev/null | grep -q 'input_count.:2'; then \
			echo "✓ Sim warmed up — both platoons reporting"; \
			exit 0; \
		fi; \
		printf "  Waiting... (%d/60)\r" $$i; \
		sleep 5; \
	done; \
	echo "⚠ Warmup timeout — check $(COMMANDER_CONTAINER) logs manually"

# Tear down sim
demo-sim-destroy:
	@echo "Destroying sim topology..."
	@sudo containerlab destroy -t $(TOPOLOGY) --cleanup 2>/dev/null || true
	@echo "✓ Sim destroyed"

# Full demo loop: sim + ATAK
demo: demo-sim demo-atak
	@sleep 5
	@$(MAKE) demo-verify

# Verify ATAK sees the sim mesh
demo-verify:
	@echo "=== Recent Peat logs ==="
	@adb logcat -d -t 200 | grep -iE 'Cell.*updated|PeatNode|sync.*document|PEAT|formation.*handshake' || echo "(no recent Peat logs — ATAK may still be starting)"
	@echo ""
	@echo "=== Sim commander metrics ==="
	@docker exec $(COMMANDER_CONTAINER) tail -5 /data/logs/company-ALPHA-commander.metrics.log 2>/dev/null || echo "(sim not running)"

# DiSCO USV flotilla (runs alongside company-ALPHA)
DISCO_TOPOLOGY ?= peat-sim/topologies/disco-8usv.yaml
demo-disco:
	@echo "Deploying DiSCO USV flotilla..."
	@sudo BACKEND=automerge CAP_IN_MEMORY=true containerlab deploy -t $(DISCO_TOPOLOGY) --reconfigure --timeout 5m
	@echo "✓ DiSCO flotilla deployed"

demo-disco-destroy:
	@echo "Destroying DiSCO flotilla..."
	@sudo containerlab destroy -t $(DISCO_TOPOLOGY) --cleanup 2>/dev/null || true
	@echo "✓ DiSCO flotilla destroyed"

# ---- Demo Flow Control ----
# Pre-demo: build everything (run once before the demo)
demo-prep: build-docker build-atak-plugin deploy-atak-plugin
	@cp peat-sim/topologies/lab4-48n-1gbps.yaml /tmp/lab4-48n.yaml
	@echo "✓ Demo prep complete — Docker image + APK built and deployed"

# Clean reset: tear down everything, clear ATAK store, restart fresh
demo-reset: stop-atak
	@docker ps -a --filter "name=clab-" -q | xargs -r docker rm -f 2>/dev/null || true
	@docker network rm lab4-48n disco-8usv 2>/dev/null || true
	@adb shell "run-as $(ATAK_PACKAGE) rm -rf /data/user/0/$(ATAK_PACKAGE)/files/peat" 2>/dev/null || true
	@echo "✓ Clean reset complete"

# Phase 1: ATAK only (BRAVO cell = tablet + watch)
demo-phase1: configure-atak start-atak
	@echo "✓ Phase 1: ATAK running (BRAVO cell)"

# Phase 2: Bring up ALPHA company (48-node ground force)
demo-phase2:
	@cp peat-sim/topologies/lab4-48n-1gbps.yaml /tmp/lab4-48n.yaml
	@echo "Deploying ALPHA company..."
	@sudo BACKEND=automerge CAP_IN_MEMORY=true containerlab deploy -t $(TOPOLOGY) --reconfigure --timeout 5m
	@echo "✓ Phase 2: ALPHA company deployed"

# Phase 3: Bring up CHARLIE (DiSCO USV swarm)
demo-phase3:
	@echo "Deploying CHARLIE (DiSCO LightFish swarm)..."
	@sudo BACKEND=automerge CAP_IN_MEMORY=true containerlab deploy -t $(DISCO_TOPOLOGY) --reconfigure --timeout 5m
	@echo "✓ Phase 3: CHARLIE USV swarm deployed"

# Phase 4: Start red track scenario (hostile vessel approaches USV box)
# Sends SIGUSR1 to disco-leader container, which publishes START_SCENARIO
# to the "commands" collection. ATAK polls commands and triggers the scenario.
demo-phase4:
	@echo "Starting red track scenario via mesh command..."
	@docker kill -s USR1 clab-disco-8usv-disco-leader
	@echo "✓ Phase 4: START_SCENARIO published to mesh — ATAK will pick it up within ~10s"

# Stop red track scenario
demo-phase4-stop:
	@echo "Stopping red track scenario via mesh command..."
	@docker kill -s USR2 clab-disco-8usv-disco-leader
	@echo "✓ STOP_SCENARIO published to mesh"

# Stop everything
demo-stop: stop-atak demo-sim-destroy demo-disco-destroy
	@echo "✓ Demo environment torn down"

# ============================================
# BLE Functional Test (Pi-to-Android)
# ============================================
# Proves dual-transport (Iroh QUIC + BLE) with Pi running dual_test_peer
# (single binary, both transports) and Android running the BLE test app.
#
# Prerequisites:
#   - cross (cargo install cross)
#   - cargo-ndk (cargo install cargo-ndk)
#   - SSH access to rpi-ci (kit@rpi-ci)
#   - Android device connected via ADB
#   - ANDROID_HOME or Android SDK at ~/Android/Sdk

BLE_TEST_PI ?= rpi-ci
BLE_TEST_PI_USER ?= kit

# dual_test_peer runs both BLE + QUIC in a single process on the Pi
IROH_TEST_PORT ?= 42009
BLE_TEST_PI_IP ?= 192.168.228.13

# Build Android BLE test APK (cross-compile libpeat_ffi + Gradle build)
build-ble-test-app: build-android
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Building Android BLE Test App                            ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@mkdir -p examples/android-ble-test/app/src/main/jniLibs/arm64-v8a
	cp atak-plugin/app/libs/arm64-v8a/libpeat_ffi.so \
		examples/android-ble-test/app/src/main/jniLibs/arm64-v8a/
	@echo "✓ Copied libpeat_ffi.so to android-ble-test jniLibs"
	cd examples/android-ble-test && ./gradlew assembleDebug
	@echo "✓ BLE test APK built:"
	@ls -la examples/android-ble-test/app/build/outputs/apk/debug/app-debug.apk

# Deploy BLE test APK to connected Android device
deploy-ble-test-app:
	@echo "Deploying BLE test app to Android device..."
	@adb devices | grep -q 'device$$' || { echo "Error: No Android device connected"; exit 1; }
	adb install -r examples/android-ble-test/app/build/outputs/apk/debug/app-debug.apk
	@echo "✓ Deployed to device"
	@echo "Launch with: adb shell am start -n com.defenseunicorns.peat.test/.MainActivity"

# Full BLE test pipeline: build everything, deploy, start dual_test_peer (BLE-only mode)
ble-test: deploy-dual-test-peer build-ble-test-app deploy-ble-test-app start-dual-test-peer
	@echo ""
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  BLE Functional Test Ready                                ║"
	@echo "╠════════════════════════════════════════════════════════════╣"
	@echo "║  Pi:      dual_test_peer running on $(BLE_TEST_PI)"
	@echo "║  Android: BLE test app installed"
	@echo "║                                                            ║"
	@echo "║  Launching test automatically...                          ║"
	@echo "║                                                            ║"
	@echo "║  Monitor:                                                  ║"
	@echo "║    make ble-test-logs        (Android logcat)             ║"
	@echo "║    make dual-test-peer-logs  (Pi peer log)                ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	adb shell am start -n com.defenseunicorns.peat.test/.MainActivity \
		--ez auto_run true

# Show Android logcat for BLE test
ble-test-logs:
	adb logcat -s HiveTest:V BleGattClient:V HiveJni:V HiveNativeLoader:V

# Clean BLE test artifacts
clean-ble-test: stop-dual-test-peer
	@echo "Cleaning BLE test artifacts..."
	@rm -rf examples/android-ble-test/app/build
	@rm -rf examples/android-ble-test/app/src/main/jniLibs/arm64-v8a/libpeat_ffi.so
	@echo "✓ BLE test artifacts cleaned"

# ============================================
# Dual-Transport Test (BLE + QUIC via dual_test_peer)
# ============================================
# Proves simultaneous BLE + QUIC data sync with Android using ONE Pi.
# A single binary runs both transports on the same Pi (rpi-ci):
#   - dual_test_peer: BLE (BlueZ D-Bus) + QUIC (Iroh) via create_node(enable_ble=true)
# This matches the Android architecture (one node, both transports).

# Cross-compile dual_test_peer for Raspberry Pi (aarch64)
build-dual-test-peer:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Building dual_test_peer for aarch64 (Raspberry Pi)       ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@command -v cross >/dev/null 2>&1 || { echo "Error: cross not found. Install with: cargo install cross"; exit 1; }
	CXXFLAGS="-include cstdint" cross build --release \
		--target aarch64-unknown-linux-gnu \
		--example dual_test_peer \
		-p peat-ffi --features sync,bluetooth
	@echo "✓ dual_test_peer built:"
	@ls -la target/aarch64-unknown-linux-gnu/release/examples/dual_test_peer

# Deploy dual_test_peer binary to Pi
deploy-dual-test-peer: build-dual-test-peer
	@echo "Deploying dual_test_peer to $(BLE_TEST_PI_USER)@$(BLE_TEST_PI)..."
	scp target/aarch64-unknown-linux-gnu/release/examples/dual_test_peer \
		$(BLE_TEST_PI_USER)@$(BLE_TEST_PI):~/dual_test_peer
	@echo "✓ Deployed to $(BLE_TEST_PI):~/dual_test_peer"

# Start dual_test_peer on Pi (backgrounded, logs to ~/dual_test_peer.log)
start-dual-test-peer:
	@echo "Starting dual_test_peer on $(BLE_TEST_PI)..."
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pkill -x dual_test_peer 2>/dev/null || true'
	@sleep 1
	@echo "Resetting BLE adapter on $(BLE_TEST_PI) to clear stale state..."
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) \
		'bluetoothctl power off 2>/dev/null; sleep 1; bluetoothctl power on 2>/dev/null; sleep 1'
	@echo "✓ BLE adapter reset"
	ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) \
		'nohup ~/dual_test_peer > ~/dual_test_peer.log 2>&1 & echo $$!'
	@sleep 3
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pgrep -x dual_test_peer >/dev/null && echo "✓ dual_test_peer running (PID: $$(pgrep -x dual_test_peer))" || echo "✗ dual_test_peer failed to start"'

# Stop dual_test_peer on Pi
stop-dual-test-peer:
	@echo "Stopping dual_test_peer on $(BLE_TEST_PI)..."
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pkill -x dual_test_peer 2>/dev/null && echo "✓ Stopped" || echo "Not running"'
	@echo "Resetting BLE adapter on $(BLE_TEST_PI)..."
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) \
		'bluetoothctl power off 2>/dev/null; sleep 1; bluetoothctl power on 2>/dev/null' || true

# Full dual-transport test pipeline (single binary on Pi)
dual-transport-test: deploy-dual-test-peer build-ble-test-app deploy-ble-test-app start-dual-test-peer
	@echo ""
	@echo "Capturing dual_test_peer node ID from $(BLE_TEST_PI)..."
	$(eval QUIC_NODE_ID := $(shell ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'grep PEER_NODE_ID ~/dual_test_peer.log | head -1 | cut -d= -f2'))
	@if [ -z "$(QUIC_NODE_ID)" ]; then \
		echo "✗ Failed to capture PEER_NODE_ID from dual_test_peer log"; \
		exit 1; \
	fi
	@echo "✓ QUIC peer node ID: $(QUIC_NODE_ID)"
	@echo ""
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Dual-Transport Test Ready (BLE + QUIC)                   ║"
	@echo "╠════════════════════════════════════════════════════════════╣"
	@echo "║  Pi:      $(BLE_TEST_PI) running dual_test_peer (BLE + QUIC)"
	@echo "║  Android: BLE test app installed                          ║"
	@echo "║                                                            ║"
	@echo "║  Android discovers Pi via:                                ║"
	@echo "║    - BLE advertisements (GATT sync)                       ║"
	@echo "║    - mDNS/direct (QUIC platform sync)                    ║"
	@echo "║                                                            ║"
	@echo "║  Launching test with QUIC peer info...                    ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@ANDROID_PASS=false; \
	for attempt in 1 2 3; do \
		echo ""; \
		echo "--- Android attempt $$attempt/3 ---"; \
		adb shell am force-stop com.defenseunicorns.peat.test 2>/dev/null || true; \
		adb logcat -c 2>/dev/null || true; \
		if [ $$attempt -gt 1 ]; then \
			echo "Resetting Pi BLE adapter and restarting dual_test_peer..."; \
			ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'pkill -x dual_test_peer 2>/dev/null || true; sleep 1; bluetoothctl power off 2>/dev/null; sleep 1; bluetoothctl power on 2>/dev/null; sleep 1; nohup ~/dual_test_peer > ~/dual_test_peer.log 2>&1 &'; \
			sleep 3; \
		fi; \
		echo "Toggling Android BLE..."; \
		adb shell cmd bluetooth_manager disable 2>/dev/null || true; \
		sleep 2; \
		adb shell cmd bluetooth_manager enable 2>/dev/null || true; \
		adb shell cmd bluetooth_manager wait-for-state:STATE_ON 2>/dev/null; \
		sleep 5; \
		adb shell am start -n com.defenseunicorns.peat.test/.MainActivity \
			--es quic_node_id "$(QUIC_NODE_ID)" \
			--es quic_address "$(BLE_TEST_PI_IP):$(IROH_TEST_PORT)" \
			--ez auto_run true; \
		echo "Waiting for result (up to 90s)..."; \
		for i in $$(seq 1 90); do \
			if adb logcat -d -s HiveTest 2>/dev/null | grep -q "^.*RESULT:"; then \
				break; \
			fi; \
			sleep 1; \
		done; \
		if adb logcat -d -s HiveTest 2>/dev/null | grep -q "RESULT:.*PASSED"; then \
			ANDROID_PASS=true; \
			break; \
		fi; \
		echo "Attempt $$attempt failed, checking error..."; \
		adb logcat -d -s HiveTest 2>/dev/null | grep -E "Phase 5|FAIL" || true; \
		if [ $$attempt -lt 3 ]; then \
			echo "Retrying after BLE reset..."; \
		fi; \
	done; \
	echo ""; \
	echo "╔════════════════════════════════════════════════════════════╗"; \
	echo "║  Android Results                                          ║"; \
	echo "╚════════════════════════════════════════════════════════════╝"; \
	adb logcat -d -s HiveTest 2>/dev/null | grep -E "Phase|RESULT|PEAT |Run:|Build:|====| " || echo "  (no output captured)"
	@echo ""
	@echo "Waiting for Pi dual_test_peer to finish (up to 30s)..."
	@for i in $$(seq 1 30); do \
		if ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'test ! -e /proc/$$(pgrep -x dual_test_peer 2>/dev/null || echo 0)/status 2>/dev/null'; then \
			break; \
		fi; \
		sleep 1; \
	done
	@echo ""
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Pi Results                                               ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'cat ~/dual_test_peer.log' 2>/dev/null | grep -E "PEER_NODE_ID|Received|PASSED|FAILED" || echo "  (no output captured)"
	@echo ""
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Final Verdict                                            ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@ANDROID_RESULT=$$(adb logcat -d -s HiveTest 2>/dev/null | grep "RESULT:.*PASSED" || true); \
	PI_RESULT=$$(ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'grep "Test PASSED" ~/dual_test_peer.log 2>/dev/null' || true); \
	if [ -n "$$ANDROID_RESULT" ] && [ -n "$$PI_RESULT" ]; then \
		echo "  ✓ BOTH SIDES PASSED — dual-transport verified"; \
	else \
		echo "  ✗ TEST FAILED"; \
		[ -z "$$ANDROID_RESULT" ] && echo "    Android: FAILED"; \
		[ -z "$$PI_RESULT" ] && echo "    Pi:      FAILED"; \
		exit 1; \
	fi

# Show dual_test_peer logs from Pi
dual-test-peer-logs:
	ssh $(BLE_TEST_PI_USER)@$(BLE_TEST_PI) 'tail -f ~/dual_test_peer.log'

# ============================================
# Functional Test Suite (all hardware tests)
# ============================================
# Orchestrates all functional/hardware tests from one entry point.
# Tests: rpi-rpi BLE (peat-btle), rpi-android dual-transport (peat),
#        k8s cluster (peat-mesh)

functional-suite:
	@./scripts/functional-suite.sh

functional-ble:
	@./scripts/functional-suite.sh --ble-only

functional-android:
	@./scripts/functional-suite.sh --android-only

functional-k8s:
	@./scripts/functional-suite.sh --k8s-only

# ============================================
# Legacy E-Series Tests (kept for compatibility)
# ============================================

e11-modes:
	@echo "Running E11 mode testing (legacy)..."
	@cd peat-sim && ./test-all-modes-report.sh
