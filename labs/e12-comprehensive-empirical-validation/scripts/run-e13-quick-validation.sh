#!/bin/bash

################################################################################
# E13 Quick Validation - Connection-Limited Mesh
#
# Tests Ditto with realistic mesh topology (3-5 peers/node)
# Compares directly to E12 forced full-mesh results
#
# Test Matrix:
#   - Traditional (baseline):  12, 24 nodes × 1gbps
#   - CAP Mesh Limited (E13): 12, 24 nodes × 1gbps
#   - CAP Mesh Full (E12):    12, 24 nodes × 1gbps (for comparison)
#
# Expected outcome: E13 should show MUCH better latency than E12
################################################################################

set -euo pipefail

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="e13-quick-validation-${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

log_info() {
    echo -e "${CYAN}→ $1${NC}"
}

log_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

log_warn() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

log_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Source the E12 test harness functions
source run-comprehensive-suite.sh

################################################################################
# E13 Quick Validation Tests
################################################################################

echo "================================================================================"
echo "E13 Quick Validation: Connection-Limited Mesh"
echo "================================================================================"
echo ""
echo "Purpose: Fair evaluation of Ditto CRDT+P2P mesh performance"
echo "Issue: E12 forced full-mesh TCP topology (11-23 connections/node)"
echo "Fix: E13 uses connection-limited mesh (3-5 connections/node)"
echo ""
echo "Results directory: ${RESULTS_DIR}"
echo ""

# Test configurations
declare -A E13_TESTS
E13_TESTS["traditional-12node-1gbps"]="../../../cap-sim/topologies/traditional-platoon-12node.yaml"
E13_TESTS["traditional-24node-1gbps"]="../../../cap-sim/topologies/traditional-platoon-24node.yaml"
E13_TESTS["cap-mesh-limited-12node-1gbps"]="../../../cap-sim/topologies/squad-12node-limited-mesh.yaml"
E13_TESTS["cap-mesh-limited-24node-1gbps"]="../../../cap-sim/topologies/platoon-24node-limited-mesh.yaml"
E13_TESTS["cap-mesh-full-12node-1gbps"]="../../../cap-sim/topologies/squad-12node-dynamic-mesh.yaml"
E13_TESTS["cap-mesh-full-24node-1gbps"]="../../../cap-sim/topologies/platoon-24node-dynamic-mesh.yaml"

TOTAL_TESTS=${#E13_TESTS[@]}
CURRENT_TEST=0

log_info "Running ${TOTAL_TESTS} tests..."
echo ""

for test_name in "${!E13_TESTS[@]}"; do
    CURRENT_TEST=$((CURRENT_TEST + 1))
    topology="${E13_TESTS[$test_name]}"

    echo "--------------------------------------------------------------------------------"
    log_info "[$CURRENT_TEST/$TOTAL_TESTS] ${test_name}"
    echo "--------------------------------------------------------------------------------"

    # Parse test name: architecture-scale-bandwidth
    if [[ $test_name =~ ^([^-]+(-[^-]+)*)-([0-9]+node)-([^-]+)$ ]]; then
        architecture="${BASH_REMATCH[1]}"
        scale="${BASH_REMATCH[3]}"
        bandwidth="${BASH_REMATCH[4]}"
    else
        log_error "Failed to parse test name: $test_name"
        continue
    fi

    # Check if topology file exists
    if [[ ! -f "$topology" ]]; then
        log_warn "Topology file not found: $topology"
        log_warn "Skipping test: $test_name"
        echo ""
        continue
    fi

    # Run the test using E12 harness function
    log_info "Starting test with topology: $(basename $topology)"

    # Override RESULTS_DIR for this test
    OLD_RESULTS_DIR="$RESULTS_DIR"
    RESULTS_DIR="$OLD_RESULTS_DIR"

    if run_single_test "$architecture" "$scale" "$bandwidth" "$topology"; then
        log_success "Test completed: $test_name"
    else
        log_error "Test failed: $test_name"
    fi

    RESULTS_DIR="$OLD_RESULTS_DIR"
    echo ""
done

echo "================================================================================"
log_success "E13 Quick Validation Complete"
echo "================================================================================"
echo ""
echo "Results saved to: ${RESULTS_DIR}"
echo ""
echo "Next steps:"
echo "  1. Run analysis: python3 analyze-comprehensive-results.py ${RESULTS_DIR}"
echo "  2. Compare E13 vs E12 latencies"
echo "  3. Verify connection-limited mesh shows better performance"
echo ""
