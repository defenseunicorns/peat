#!/bin/bash
################################################################################
# Connection Churn Validation Test
#
# Quick validation test to verify connection churn infrastructure works
# correctly before running full test matrix.
#
# Tests:
#   - 12-node P2P mesh with connection churn enabled
#   - Validates churn injection, recovery, and metrics collection
#
# Expected outcomes:
#   - Churn events logged with timestamps
#   - Metrics show latency spikes during outages
#   - Network recovers after churn events
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

echo "================================================================================"
echo "Connection Churn Validation Test"
echo "================================================================================"
echo ""

cd /home/kit/Code/revolve/cap/cap-sim

# Source environment variables for Ditto credentials
log_info "Loading Ditto credentials from .env"
set -a
source .env
set +a

# Test configuration
TOPOLOGY="topologies/squad-12node-p2p-limited-mesh.yaml"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="../labs/e12-comprehensive-empirical-validation/scripts/churn-validation-${TIMESTAMP}"
LOG_DIR="${RESULTS_DIR}/12node-p2p-with-churn"

log_info "Test: 12-node P2P mesh with connection churn"
log_info "Topology: ${TOPOLOGY}"
log_info "Results: ${RESULTS_DIR}"

# Check topology exists
if [ ! -f "${TOPOLOGY}" ]; then
    log_error "Topology file not found: ${TOPOLOGY}"
    exit 1
fi

# Destroy any existing deployment
make sim-destroy 2>/dev/null || true
sleep 2

# Clean storage
log_info "Ensuring clean slate..."
rm -rf /tmp/cap_sim_* 2>/dev/null || true
for container in $(docker ps -a --filter "name=clab-" --format "{{.Names}}" 2>/dev/null); do
    docker exec "$container" rm -rf /tmp/cap_sim_* 2>/dev/null || true
done
log_success "Storage cleaned"

# Deploy topology
log_info "Deploying 12-node P2P mesh..."
if ! containerlab deploy -t "${TOPOLOGY}"; then
    log_error "Deployment failed"
    exit 1
fi
log_success "Deployment complete"

# Warmup period
log_info "Warm-up: 30 seconds..."
sleep 30

# Start connection churn
mkdir -p "${LOG_DIR}"
CHURN_LOG="${LOG_DIR}/connection-churn.log"
log_info "Starting connection churn injection..."
log_info "Churn config: 60s duration, 10s interval, 5-15s outages"
../labs/e12-comprehensive-empirical-validation/scripts/inject-connection-churn.sh 60 10 "${CHURN_LOG}" &
CHURN_PID=$!
log_success "Connection churn started (PID: ${CHURN_PID})"

# Data collection period
log_info "Collecting data with active connection churn: 60 seconds..."
sleep 60

# Stop churn
log_info "Stopping connection churn..."
kill ${CHURN_PID} 2>/dev/null || true
wait ${CHURN_PID} 2>/dev/null || true
log_success "Connection churn stopped"

# Collect logs
log_info "Collecting logs..."
for container in $(docker ps --format '{{.Names}}' | grep clab); do
    docker logs "$container" > "${LOG_DIR}/${container}.log" 2>&1 || true
done

# Teardown
log_info "Tearing down..."
containerlab destroy -t "${TOPOLOGY}"
sleep 2

echo ""
echo "================================================================================"
log_success "Connection Churn Validation Test Complete"
echo "================================================================================"
echo ""
echo "Results: ${RESULTS_DIR}"
echo ""
echo "Analysis steps:"
echo "  1. Review churn events: cat ${CHURN_LOG}"
echo "  2. Check container logs for errors: ls ${LOG_DIR}/*.log"
echo "  3. Analyze latency metrics during churn:"
echo "     grep DocumentReceived ${LOG_DIR}/*.log | jq -r '.latency_ms' | sort -n"
echo ""
echo "Expected observations:"
echo "  - Connection churn log shows DROP/RESTORE events"
echo "  - Metrics show increased latency during outages"
echo "  - Network recovers after each churn event"
echo ""
