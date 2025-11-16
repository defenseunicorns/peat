#!/bin/bash
################################################################################
# E14: Mode 4 Clean vs Dirty Network Comparison
#
# Compares CAP Mode 4 hierarchical performance under two network conditions:
#   1. Clean: Stable connections, 1Gbps bandwidth
#   2. Dirty: Connection churn enabled (random drops every 10s)
#
# Test: 24-node platoon (2-level hierarchy)
#   - Platoon leader → Squad leaders → Soldiers
#   - 30s warmup + 60s data collection per condition
#   - Same topology, only difference is connection stability
#
# This quantifies the impact of tactical radio link instability on
# hierarchical state aggregation performance.
################################################################################

set -euo pipefail

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

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

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="e14-mode4-clean-vs-dirty-${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

echo "================================================================================"
echo "E14: Mode 4 Clean vs Dirty Network Comparison"
echo "================================================================================"
echo ""
echo "Test: 24-node platoon (Mode 4 hierarchical)"
echo "Conditions:"
echo "  1. Clean network: Stable connections, 1Gbps bandwidth"
echo "  2. Dirty network: Connection churn (random drops every ~10s)"
echo ""
echo "Results: ${RESULTS_DIR}"
echo ""

cd /home/kit/Code/revolve/hive/hive-sim

# Source environment variables
log_info "Loading Ditto credentials from .env"
set -a
source .env
set +a

TOPOLOGY="topologies/platoon-24node-client-server-mode4.yaml"

# Verify topology exists
if [ ! -f "${TOPOLOGY}" ]; then
    log_error "Topology not found: ${TOPOLOGY}"
    exit 1
fi

################################################################################
# Test 1: Clean Network (No Churn)
################################################################################

log_info "========================================"
log_info "Test 1: CLEAN NETWORK (stable connections)"
log_info "========================================"

# Cleanup
make sim-destroy 2>/dev/null || true
sleep 2

log_info "Ensuring clean slate..."
rm -rf /tmp/cap_sim_* 2>/dev/null || true
for container in $(docker ps -a --filter "name=clab-" --format "{{.Names}}" 2>/dev/null); do
    docker exec "$container" rm -rf /tmp/cap_sim_* 2>/dev/null || true
done
log_success "Storage cleaned"

# Deploy
log_info "Deploying 24-node Mode 4 hierarchical topology..."
if ! containerlab deploy -t "${TOPOLOGY}"; then
    log_error "Deployment failed"
    exit 1
fi
log_success "Deployment complete"

# Warmup
log_info "Warm-up: 30 seconds..."
sleep 30

# Data collection
log_info "Collecting data (CLEAN network): 60 seconds..."
sleep 60

# Collect logs
log_info "Collecting logs..."
CLEAN_LOG_DIR="../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/clean-network"
mkdir -p "${CLEAN_LOG_DIR}"

for container in $(docker ps --format '{{.Names}}' | grep clab); do
    docker logs "$container" > "${CLEAN_LOG_DIR}/${container}.log" 2>&1 || true
done

# Teardown
log_info "Tearing down clean network test..."
containerlab destroy -t "${TOPOLOGY}"
sleep 5

log_success "Clean network test complete"
echo ""

################################################################################
# Test 2: Dirty Network (With Churn)
################################################################################

log_info "========================================"
log_info "Test 2: DIRTY NETWORK (connection churn)"
log_info "========================================"

# Cleanup
make sim-destroy 2>/dev/null || true
sleep 2

log_info "Ensuring clean slate..."
rm -rf /tmp/cap_sim_* 2>/dev/null || true
for container in $(docker ps -a --filter "name=clab-" --format "{{.Names}}" 2>/dev/null); do
    docker exec "$container" rm -rf /tmp/cap_sim_* 2>/dev/null || true
done
log_success "Storage cleaned"

# Deploy
log_info "Deploying 24-node Mode 4 hierarchical topology..."
if ! containerlab deploy -t "${TOPOLOGY}"; then
    log_error "Deployment failed"
    exit 1
fi
log_success "Deployment complete"

# Warmup
log_info "Warm-up: 30 seconds..."
sleep 30

# Start connection churn
DIRTY_LOG_DIR="../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/dirty-network"
mkdir -p "${DIRTY_LOG_DIR}"
CHURN_LOG="${DIRTY_LOG_DIR}/connection-churn.log"

log_info "Starting connection churn injection..."
log_info "Churn: Random drops every ~10s, 5-15s outages"
../labs/e12-comprehensive-empirical-validation/scripts/inject-connection-churn.sh 60 10 "${CHURN_LOG}" &
CHURN_PID=$!
log_success "Connection churn started (PID: ${CHURN_PID})"

# Data collection
log_info "Collecting data (DIRTY network with churn): 60 seconds..."
sleep 60

# Stop churn
log_info "Stopping connection churn..."
kill ${CHURN_PID} 2>/dev/null || true
wait ${CHURN_PID} 2>/dev/null || true
log_success "Connection churn stopped"

# Collect logs
log_info "Collecting logs..."
for container in $(docker ps --format '{{.Names}}' | grep clab); do
    docker logs "$container" > "${DIRTY_LOG_DIR}/${container}.log" 2>&1 || true
done

# Teardown
log_info "Tearing down dirty network test..."
containerlab destroy -t "${TOPOLOGY}"
sleep 2

log_success "Dirty network test complete"
echo ""

################################################################################
# Summary
################################################################################

echo "================================================================================"
log_success "E14 Mode 4 Clean vs Dirty Network Comparison Complete"
echo "================================================================================"
echo ""
echo "Results saved to: ${RESULTS_DIR}"
echo ""
echo "Test conditions:"
echo "  Clean network: ${CLEAN_LOG_DIR}"
echo "  Dirty network: ${DIRTY_LOG_DIR}"
echo "  Churn log: ${CHURN_LOG}"
echo ""
echo "Analysis steps:"
echo "  1. Compare latency distributions:"
echo "     grep DocumentReceived ${CLEAN_LOG_DIR}/*.log | jq -r '.latency_ms' | sort -n > clean-latencies.txt"
echo "     grep DocumentReceived ${DIRTY_LOG_DIR}/*.log | jq -r '.latency_ms' | sort -n > dirty-latencies.txt"
echo ""
echo "  2. Review churn events:"
echo "     cat ${CHURN_LOG}"
echo ""
echo "  3. Calculate statistics:"
echo "     python3 -c 'import numpy as np; data=np.loadtxt(\"clean-latencies.txt\"); print(f\"Clean - P50: {np.percentile(data,50):.1f}ms, P90: {np.percentile(data,90):.1f}ms, P99: {np.percentile(data,99):.1f}ms\")'"
echo "     python3 -c 'import numpy as np; data=np.loadtxt(\"dirty-latencies.txt\"); print(f\"Dirty - P50: {np.percentile(data,50):.1f}ms, P90: {np.percentile(data,90):.1f}ms, P99: {np.percentile(data,99):.1f}ms\")'"
echo ""
echo "Expected findings:"
echo "  - Clean network: Low, consistent latencies (7-40ms)"
echo "  - Dirty network: Higher P90/P99 due to recovery after churn events"
echo "  - Quantifies Mode 4 resilience to connection instability"
echo ""
