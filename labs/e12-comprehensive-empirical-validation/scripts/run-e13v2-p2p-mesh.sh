#!/bin/bash
################################################################################
# E13v2 Pure P2P Mesh Validation
#
# Fair test of Ditto CRDT mesh performance WITHOUT Mode 4 overhead
#
# Test Comparison:
#   Test 1: Pure P2P with connection limits (3-4 peers/node, ~75 connections)
#   Test 2: Pure P2P with forced full-mesh (n-1 peers/node, ~552 connections)
#
# Expected: Connection-limited mesh should show MUCH better latency than
#           E12's forced full-mesh, demonstrating Ditto's true mesh capabilities
################################################################################

set -euo pipefail

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="e13v2-p2p-mesh-${TIMESTAMP}"
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

echo "================================================================================"
echo "E13v2: Pure P2P Mesh Validation (NO Mode 4)"
echo "================================================================================"
echo ""
echo "Testing Ditto's native CRDT mesh convergence"
echo "Comparison: Connection-limited (3-4 peers) vs Forced full-mesh (n-1 peers)"
echo "Results: ${RESULTS_DIR}"
echo ""

cd /home/kit/Code/revolve/cap/cap-sim

# Source environment variables for Ditto credentials
log_info "Loading Ditto credentials from .env"
set -a
source .env
set +a

# Test 1: Pure P2P with Connection Limits (3-4 peers/node)
log_info "[1/2] Testing Pure P2P with Connection Limits"
log_info "Topology: platoon-24node-p2p-limited-mesh.yaml (~75 connections)"
log_info "Mode: Pure P2P (writer/reader), NO Mode 4 aggregation"

make sim-destroy || true
sleep 2

# Deploy connection-limited P2P mesh
containerlab deploy -t topologies/platoon-24node-p2p-limited-mesh.yaml

log_info "Warm-up: 30 seconds..."
sleep 30

log_info "Collecting data: 60 seconds..."
sleep 60

# Collect logs
log_info "Collecting logs..."
mkdir -p "../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/p2p-limited-24node-1gbps"
for container in $(docker ps --format '{{.Names}}' | grep clab); do
    docker logs "$container" > "../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/p2p-limited-24node-1gbps/${container}.log" 2>&1 || true
done

# Teardown
containerlab destroy -t topologies/platoon-24node-p2p-limited-mesh.yaml
sleep 5

log_success "Test 1 complete"
echo ""

# Test 2: Pure P2P with Forced Full-Mesh (n-1 peers/node)
log_info "[2/2] Testing Pure P2P with Forced Full-Mesh (E12 topology)"
log_info "Topology: platoon-24node-dynamic-mesh.yaml (~552 connections)"
log_info "Mode: Pure P2P (writer/reader), NO Mode 4 aggregation"

make sim-destroy || true
sleep 2

# Deploy forced full-mesh
containerlab deploy -t topologies/platoon-24node-dynamic-mesh.yaml

log_info "Warm-up: 30 seconds..."
sleep 30

log_info "Collecting data: 60 seconds..."
sleep 60

# Collect logs
log_info "Collecting logs..."
mkdir -p "../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/p2p-full-24node-1gbps"
for container in $(docker ps --format '{{.Names}}' | grep clab); do
    docker logs "$container" > "../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/p2p-full-24node-1gbps/${container}.log" 2>&1 || true
done

# Teardown
containerlab destroy -t topologies/platoon-24node-dynamic-mesh.yaml

log_success "Test 2 complete"
echo ""

echo "================================================================================"
log_success "E13v2 Pure P2P Mesh Validation Complete"
echo "================================================================================"
echo ""
echo "Results saved to: ${RESULTS_DIR}"
echo ""
echo "Analysis:"
echo "  This tests Ditto's native CRDT mesh WITHOUT Mode 4 overhead"
echo "  Expected: Connection-limited mesh shows better latency than forced full-mesh"
echo "  Expected: Both should be MUCH better than E13v1's Mode 4 results (P90=16s)"
echo ""
echo "Next: Analyze results with:"
echo "  cd /home/kit/Code/revolve/cap/labs/e12-comprehensive-empirical-validation/scripts"
echo "  python3 analyze-e13v2.py ${RESULTS_DIR}"
echo ""
