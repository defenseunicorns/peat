#!/bin/bash
################################################################################
# E13v3 Mode 4 Hierarchical Test Matrix - CAP Protocol Performance
#
# Tests CAP Hierarchical Mode 4 with realistic connection topology (strict
# hierarchical connections, not forced full-mesh) at multiple scales.
#
# Test Matrix:
#   - 12 nodes (squad): 1-level hierarchy (squad leader → soldiers)
#   - 24 nodes (platoon): 2-level hierarchy (platoon → squad leaders → soldiers)
#   - 48 nodes (company): 2-level hierarchy (battalion HQ → platoon leaders → soldiers)
#   - 96 nodes (battalion): 2-level hierarchy (battalion HQ → platoon leaders → soldiers)
#
# Each test:
#   - 30s warmup
#   - 60s data collection
#   - Mode 4 hierarchical aggregation
#   - 1Gbps bandwidth
#   - Strict hierarchical connections (NOT forced full-mesh)
#   - Optional: Connection churn injection (simulates tactical radio failures)
#
# This completes the architectural comparison:
#   E13v2: Pure P2P mesh (writer/reader mode)
#   E13v3: Mode 4 hierarchical (with corrected topology)
#
# Usage:
#   ./run-e13v3-mode4-hierarchical.sh              # Normal test (no churn)
#   ./run-e13v3-mode4-hierarchical.sh --with-churn # Enable connection churn
################################################################################

set -euo pipefail

# Parse command line arguments
ENABLE_CHURN=false
if [ "${1:-}" = "--with-churn" ]; then
    ENABLE_CHURN=true
fi

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="e13v3-mode4-hierarchical-${TIMESTAMP}"
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

echo "================================================================================"
echo "E13v3: Mode 4 Hierarchical Test Matrix - CAP Protocol Performance"
echo "================================================================================"
echo ""
echo "Testing CAP Hierarchical Mode 4 at 12, 24, 48, 96 nodes"
echo "Using strict hierarchical connections (NOT forced full-mesh)"
if [ "${ENABLE_CHURN}" = true ]; then
    echo "Connection churn: ENABLED (simulating tactical radio failures)"
else
    echo "Connection churn: DISABLED"
fi
echo "Results: ${RESULTS_DIR}"
echo ""

cd /home/kit/Code/revolve/cap/cap-sim

# Source environment variables for Ditto credentials
log_info "Loading Ditto credentials from .env"
set -a
source .env
set +a

# Define test matrix
declare -a SCALES=("12" "24" "48" "96")
declare -a NAMES=("squad" "platoon" "company" "battalion")
declare -a TOPOLOGIES=(
    "topologies/squad-12node-client-server-mode4.yaml"
    "topologies/platoon-24node-client-server-mode4.yaml"
    "topologies/battalion-48node-client-server-mode4.yaml"
    "topologies/battalion-96node-client-server-mode4.yaml"
)

# Track test results
declare -a TEST_STATUS=()
TESTS_TOTAL=${#SCALES[@]}
TESTS_PASSED=0
TESTS_FAILED=0

# Run tests
for i in "${!SCALES[@]}"; do
    SCALE="${SCALES[$i]}"
    NAME="${NAMES[$i]}"
    TOPOLOGY="${TOPOLOGIES[$i]}"
    TEST_NUM=$((i + 1))

    log_info "[${TEST_NUM}/${TESTS_TOTAL}] Testing ${NAME} (${SCALE} nodes, Mode 4)"
    log_info "Topology: ${TOPOLOGY}"

    # Check topology file exists
    if [ ! -f "${TOPOLOGY}" ]; then
        log_error "Topology file not found: ${TOPOLOGY}"
        TEST_STATUS+=("FAILED")
        TESTS_FAILED=$((TESTS_FAILED + 1))
        continue
    fi

    # Destroy any existing deployment
    make sim-destroy 2>/dev/null || true
    sleep 2

    # Clean slate: Remove all Ditto storage from previous test runs
    log_info "Ensuring clean slate for test run..."
    rm -rf /tmp/cap_sim_* 2>/dev/null || true

    # Also clean any persisted storage in containers (if they exist)
    for container in $(docker ps -a --filter "name=clab-" --format "{{.Names}}" 2>/dev/null); do
        docker exec "$container" rm -rf /tmp/cap_sim_* 2>/dev/null || true
    done

    log_success "Storage cleaned"

    # Deploy topology
    log_info "Deploying ${SCALE}-node Mode 4 hierarchical topology..."
    if ! containerlab deploy -t "${TOPOLOGY}"; then
        log_error "Deployment failed for ${NAME}"
        TEST_STATUS+=("FAILED")
        TESTS_FAILED=$((TESTS_FAILED + 1))
        continue
    fi

    # Warmup period
    log_info "Warm-up: 30 seconds..."
    sleep 30

    # Start connection churn injection if enabled
    CHURN_PID=""
    if [ "${ENABLE_CHURN}" = true ]; then
        log_info "Starting connection churn injection..."
        CHURN_LOG="${LOG_DIR}/connection-churn.log"
        ../labs/e12-comprehensive-empirical-validation/scripts/inject-connection-churn.sh 60 10 "${CHURN_LOG}" &
        CHURN_PID=$!
        log_success "Connection churn started (PID: ${CHURN_PID})"
    fi

    # Data collection period
    log_info "Collecting data: 60 seconds..."
    sleep 60

    # Stop connection churn if running
    if [ -n "${CHURN_PID}" ]; then
        log_info "Stopping connection churn..."
        kill ${CHURN_PID} 2>/dev/null || true
        wait ${CHURN_PID} 2>/dev/null || true
        log_success "Connection churn stopped"
    fi

    # Collect logs
    log_info "Collecting logs..."
    LOG_DIR="../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/mode4-${SCALE}node-1gbps"
    mkdir -p "${LOG_DIR}"

    for container in $(docker ps --format '{{.Names}}' | grep clab); do
        docker logs "$container" > "${LOG_DIR}/${container}.log" 2>&1 || true
    done

    # Teardown
    log_info "Tearing down..."
    containerlab destroy -t "${TOPOLOGY}"
    sleep 5

    log_success "Test ${TEST_NUM} complete: ${NAME} (${SCALE} nodes)"
    TEST_STATUS+=("PASSED")
    TESTS_PASSED=$((TESTS_PASSED + 1))
    echo ""
done

echo "================================================================================"
log_success "E13v3 Mode 4 Hierarchical Test Matrix Complete"
echo "================================================================================"
echo ""
echo "Results saved to: ${RESULTS_DIR}"
echo ""
echo "Test Summary:"
echo "  Total: ${TESTS_TOTAL}"
echo "  Passed: ${TESTS_PASSED}"
echo "  Failed: ${TESTS_FAILED}"
echo ""

# Show per-test status
for i in "${!SCALES[@]}"; do
    SCALE="${SCALES[$i]}"
    NAME="${NAMES[$i]}"
    STATUS="${TEST_STATUS[$i]}"

    if [ "${STATUS}" == "PASSED" ]; then
        echo -e "${GREEN}  ✓ ${NAME} (${SCALE} nodes): ${STATUS}${NC}"
    else
        echo -e "${RED}  ✗ ${NAME} (${SCALE} nodes): ${STATUS}${NC}"
    fi
done
echo ""

echo "Next Steps:"
echo "  1. Analyze results with steady-state filtering"
echo "  2. Compare Mode 4 hierarchical vs pure P2P mesh (E13v2)"
echo "  3. Measure hierarchical aggregation overhead"
echo "  4. Validate 5-second aggregation delay per hop"
echo ""
echo "Analysis command:"
echo "  cd /home/kit/Code/revolve/cap/labs/e12-comprehensive-empirical-validation/scripts"
echo "  python3 analyze-e13v3.py ${RESULTS_DIR}"
echo ""

# Exit with appropriate status
if [ ${TESTS_FAILED} -eq 0 ]; then
    exit 0
else
    exit 1
fi
