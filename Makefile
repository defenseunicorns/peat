.PHONY: help clean clean-ditto build test test-e2e fmt clippy check all pre-commit ci
.PHONY: sim-build sim-deploy-poc sim-deploy-squad sim-destroy sim-logs sim-clean
.PHONY: docs-presentation

# Default target
help:
	@echo "CAP Protocol Development Makefile"
	@echo ""
	@echo "Development:"
	@echo "  help         - Show this help message"
	@echo "  clean        - Remove build artifacts and Ditto directories"
	@echo "  clean-ditto  - Remove Ditto persistence directories only"
	@echo "  build        - Build all crates"
	@echo "  test         - Run all tests serially (due to Ditto resource usage)"
	@echo "  test-e2e     - Run E2E integration tests for Squad Formation"
	@echo "  fmt          - Format all code with cargo fmt"
	@echo "  clippy       - Run clippy linter"
	@echo "  check        - Run fmt + clippy + test"
	@echo "  pre-commit   - Run all checks before committing (fmt + clippy + test)"
	@echo "  ci           - Run full CI pipeline (fmt check + clippy + test)"
	@echo "  all          - Clean, build, and run all checks"
	@echo ""
	@echo "Documentation:"
	@echo "  docs-presentation - Build HTML presentation from markdown"
	@echo ""
	@echo "Network Simulation (E8 - requires Linux with ContainerLab):"
	@echo "  sim-build                  - Build cap-sim-node Docker image"
	@echo "  sim-deploy-poc             - Deploy 2-node POC topology"
	@echo "  sim-deploy-squad-simple    - Deploy squad (Mode 1: Client-Server)"
	@echo "  sim-deploy-squad-hierarchical - Deploy squad (Mode 2: Hub-Spoke)"
	@echo "  sim-deploy-squad-dynamic   - Deploy squad (Mode 3: Dynamic Mesh) ⭐"
	@echo "  sim-deploy-squad           - Alias for Mode 3 (recommended)"
	@echo "  sim-logs NODE=x            - Show logs for specific node"
	@echo "  sim-inspect                - Inspect running topologies"
	@echo "  sim-destroy                - Destroy running topology"
	@echo "  sim-clean                  - Destroy all and clean up artifacts"
	@echo ""
	@echo "E8/E11 Testing & Analysis:"
	@echo "  e11-comprehensive-suite    - Test all modes × all bandwidths (16 tests, ~60min) ⭐⭐⭐"
	@echo "  e11-all-modes-report       - Test all modes unconstrained + report"
	@echo "  e11-mode4-bandwidth BW=x   - Test Mode 4 at specific bandwidth (1gbps/100mbps/1mbps/256kbps)"
	@echo "  e8-baseline-comparison     - Run three-way baseline comparison"
	@echo "  e8-performance-tests       - Run full E8 performance test suite"
	@echo "  e8-compare-results DIR=x   - Generate comparison report"

# Clean build artifacts and Ditto directories
clean: clean-ditto
	@echo "Cleaning build artifacts..."
	cargo clean

# Remove Ditto persistence directories
clean-ditto:
	@echo "Removing Ditto persistence directories..."
	@find . -type d -name ".ditto*" -exec rm -rf {} + 2>/dev/null || true
	@rm -rf /tmp/cap-persistence-test-* 2>/dev/null || true
	@echo "Ditto directories cleaned"

# Build all crates
build:
	@echo "Building all crates..."
	cargo build

# Run tests (serial execution due to Ditto resource usage)
# NOTE: Most tests use real Ditto instances and must run serially to avoid FD exhaustion
test: clean-ditto
	@echo "Running tests serially (single-threaded due to Ditto resource usage)..."
	@if [ -f .env ]; then \
		export $$(grep -v '^#' .env | xargs) && cargo test -- --test-threads=1; \
	else \
		cargo test -- --test-threads=1; \
	fi

# Run E2E integration tests
test-e2e: clean-ditto
	@echo "Running E2E integration tests..."
	@if [ ! -f .env ]; then \
		echo "⚠️  Warning: .env file not found. Ditto tests may be skipped."; \
		echo "   Create .env with DITTO_APP_ID, DITTO_OFFLINE_TOKEN, DITTO_SHARED_KEY"; \
	fi
	cd cap-protocol && export $$(grep -v '^#' ../.env | xargs) && cargo test --test squad_formation_e2e -- --nocapture

# Format all code
fmt:
	@echo "Formatting code..."
	cargo fmt --all

# Run clippy linter
clippy:
	@echo "Running clippy..."
	cargo clippy --all-targets --all-features -- -D warnings

# Quick check: fmt + clippy + test
check: fmt clippy test

# Pre-commit hook: run all checks
pre-commit: clean-ditto
	@echo "Running pre-commit checks..."
	@echo "1. Formatting code..."
	@cargo fmt --all
	@echo "2. Running clippy..."
	@cargo clippy --all-targets --all-features -- -D warnings
	@echo "3. Running tests serially (due to Ditto resource usage)..."
	@if [ -f .env ]; then \
		export $$(grep -v '^#' .env | xargs) && cargo test -- --test-threads=1; \
	else \
		cargo test -- --test-threads=1; \
	fi
	@echo ""
	@echo "✅ All pre-commit checks passed!"

# CI pipeline: check formatting without modifying, then clippy and test
ci: clean-ditto
	@echo "Running CI pipeline..."
	@echo "1. Checking code formatting..."
	@cargo fmt --all -- --check
	@echo "2. Running clippy..."
	@cargo clippy --all-targets --all-features -- -D warnings
	@echo "3. Running tests serially (due to Ditto resource usage)..."
	@if [ -f .env ]; then \
		export $$(grep -v '^#' .env | xargs) && cargo test -- --test-threads=1; \
	else \
		cargo test -- --test-threads=1; \
	fi
	@echo ""
	@echo "✅ CI pipeline passed!"

# Full workflow: clean, build, and check
all: clean build check
	@echo ""
	@echo "✅ All tasks completed successfully!"

# ============================================
# E8 Network Simulation (ContainerLab)
# ============================================

# Build cap-sim-node Docker image
sim-build:
	@echo "Building cap-sim-node Docker image..."
	@docker build -f cap-sim/Dockerfile -t cap-sim-node:latest .

# Deploy 2-node POC
sim-deploy-poc:
	@echo "Deploying 2-node POC topology..."
	@bash -c 'set -a && source .env && set +a && cd cap-sim && containerlab deploy -t topologies/poc-2node.yaml'

# Deploy 12-node squad (Mode 1: Client-Server)
sim-deploy-squad-simple:
	@echo "Deploying 12-node squad (Mode 1: Client-Server)..."
	@bash -c 'set -a && source .env && set +a && cd cap-sim && containerlab deploy -t topologies/squad-12node-client-server.yaml'

# Deploy 12-node squad (Mode 2: Hub-Spoke)
sim-deploy-squad-hierarchical:
	@echo "Deploying 12-node squad (Mode 2: Hub-Spoke - Hierarchical)..."
	@bash -c 'set -a && source .env && set +a && cd cap-sim && containerlab deploy -t topologies/squad-12node-hub-spoke.yaml'

# Deploy 12-node squad (Mode 3: Dynamic Mesh)
sim-deploy-squad-dynamic:
	@echo "Deploying 12-node squad (Mode 3: Dynamic Mesh)..."
	@bash -c 'set -a && source .env && set +a && cd cap-sim && containerlab deploy -t topologies/squad-12node-dynamic-mesh.yaml'

# Deploy 12-node squad (alias for Mode 3 - recommended)
sim-deploy-squad: sim-deploy-squad-dynamic

# Show logs for specific node
sim-logs:
	@if [ -z "$(NODE)" ]; then \
		echo "Usage: make sim-logs NODE=<container-name>"; \
		echo "Example: make sim-logs NODE=clab-cap-squad-12node-soldier-1"; \
		exit 1; \
	fi
	@docker logs -f $(NODE)

# Inspect running topologies
sim-inspect:
	@containerlab inspect --all

# Destroy current topology
sim-destroy:
	@echo "Destroying all ContainerLab topologies..."
	@containerlab destroy --all --cleanup

# Clean up all simulation artifacts
sim-clean: sim-destroy
	@echo "Cleaning up ContainerLab artifacts..."
	@cd cap-sim && rm -rf topologies/clab-* || true
	@echo "✅ Simulation cleanup complete"

# E11 All-Modes Validation: Test all CAP modes and generate comprehensive report
# Tests Modes 1-4 with full experimental validation
# Estimated time: ~5-6 minutes
# E11 Comprehensive Test Suite: All modes × all bandwidths
# Tests Modes 1-4 across 1Gbps, 100Mbps, 1Mbps, 256Kbps
# Total: 16 test runs (~60 minutes)
e11-comprehensive-suite:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  E11 Comprehensive Suite - All Modes × All Bandwidths     ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "This will run 16 tests (4 modes × 4 bandwidths):"
	@echo "  • Modes: 1 (Client-Server), 2 (Hub-Spoke), 3 (Mesh), 4 (Hierarchical)"
	@echo "  • Bandwidths: 1Gbps, 100Mbps, 1Mbps, 256Kbps"
	@echo ""
	@echo "⚠️  WARNING: This will take approximately 60 minutes"
	@echo ""
	@cd cap-sim && ./test-bandwidth-suite.sh

# E11 Mode 4 Bandwidth Test: Test Mode 4 at specific bandwidth
# Usage: make e11-mode4-bandwidth BW=256kbps
# Options: 1gbps, 100mbps, 1mbps, 256kbps
e11-mode4-bandwidth:
	@if [ -z "$(BW)" ]; then \
		echo "Error: BW parameter required"; \
		echo "Usage: make e11-mode4-bandwidth BW=256kbps"; \
		echo "Options: 1gbps, 100mbps, 1mbps, 256kbps"; \
		exit 1; \
	fi
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║    E11 Mode 4 Bandwidth Test - $(BW)                      ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@cd cap-sim && ./test-mode4-bandwidth.sh $(BW)

# E11 All-Modes Validation: Test all modes unconstrained
e11-all-modes-report:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║    E11 All-Modes Validation - Unconstrained Report        ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "This will test all CAP modes (unconstrained) and generate a report:"
	@echo "  • Mode 1: Client-Server (12 nodes)"
	@echo "  • Mode 2: Hub-Spoke (12 nodes)"
	@echo "  • Mode 3: Dynamic Mesh (12 nodes)"
	@echo "  • Mode 4: Hierarchical Aggregation (24 nodes)"
	@echo ""
	@echo "Estimated time: ~5-6 minutes"
	@echo ""
	@cd cap-sim && ./test-all-modes-report.sh

# E8 Performance Test Suite (Three-Way Comparison with Bandwidth Constraints)
# Runs 32 tests across 3 configurations: Traditional IoT, CAP Full, CAP Differential
# Estimated time: 30-35 minutes
e8-performance-tests:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║   E8 Performance Test Suite - Three-Way Comparison        ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "This will run 32 tests and take approximately 30-35 minutes"
	@echo "Tests: Traditional (8) + CAP Full (12) + CAP Differential (12)"
	@echo "Bandwidths: 100Mbps, 10Mbps, 1Mbps, 256Kbps"
	@echo ""
	@cd cap-sim && ./run-e8-performance-suite.sh

# Compare E8 performance results and generate analysis report
e8-compare-results:
	@if [ -z "$(DIR)" ]; then \
		echo "Usage: make e8-compare-results DIR=<results-directory>"; \
		echo "Example: make e8-compare-results DIR=cap-sim/e8-performance-results-20251107-140000"; \
		exit 1; \
	fi
	@echo "Generating three-way comparison report for $(DIR)..."
	@echo "TODO: Implement comparison script"

# Three-Way Baseline Comparison: Traditional IoT vs CAP Full vs CAP Differential
# Tests identical topologies (2-node, 12-node client-server, 12-node hub-spoke)
# Estimated time: ~5 minutes
e8-baseline-comparison:
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Three-Way Baseline Comparison                            ║"
	@echo "║  Traditional IoT vs CAP Full vs CAP Differential          ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "This will run the complete baseline comparison matrix:"
	@echo "  1. Traditional IoT Baseline (NO CRDT, periodic full messages)"
	@echo "  2. CAP Full Replication (CRDT without filtering)"
	@echo "  3. CAP Differential Filtering (CRDT + capability filtering)"
	@echo ""
	@echo "Tests: 3 architectures × 3 topologies = 9 test scenarios"
	@echo "Estimated time: ~5 minutes"
	@echo ""
	@cd cap-sim && ./run-baseline-comparison.sh

# ============================================
# Documentation
# ============================================

# Build HTML presentation from markdown
docs-presentation:
	@echo "Building HTML presentation..."
	@if ! command -v marp &> /dev/null; then \
		echo "❌ Error: marp-cli not found"; \
		echo "Install with: npm install -g @marp-team/marp-cli"; \
		exit 1; \
	fi
	@marp docs/CAP_PROTOCOL_TECHNOLOGY_DEEPDIVE.md --html --allow-local-files -o docs/CAP_PROTOCOL_TECHNOLOGY_DEEPDIVE.html
	@echo "✅ Presentation built: docs/CAP_PROTOCOL_TECHNOLOGY_DEEPDIVE.html"
