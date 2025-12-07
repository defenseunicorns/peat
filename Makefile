.PHONY: help clean clean-ditto build test test-unit test-integration test-e2e test-fast fmt clippy check pre-commit ci

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
	@cargo clippy --all-targets --all-features --workspace --exclude hive-ffi -- -D warnings
	@$(MAKE) test-unit
	@echo "✅ Pre-commit checks passed!"

ci: clean-ditto
	@echo "Running CI pipeline..."
	@cargo fmt --all -- --check
	@cargo clippy --all-targets --all-features --workspace --exclude hive-ffi -- -D warnings
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
# Legacy E-Series Tests (kept for compatibility)
# ============================================

e11-modes:
	@echo "Running E11 mode testing (legacy)..."
	@cd hive-sim && ./test-all-modes-report.sh

e12-comprehensive:
	@echo "Running E12 comprehensive validation (legacy)..."
	@cd labs/e12-comprehensive-empirical-validation/scripts && ./run-comprehensive-suite.sh
