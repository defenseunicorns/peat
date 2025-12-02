#!/bin/bash
################################################################################
# E13 Standalone Validation - Connection-Limited Mesh
#
# Simple direct test execution without E12 harness complexity
################################################################################

set -euo pipefail

CYAN='\033[0;36m'
GREEN='\033[0;32m'
NC='\033[0m'

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="e13-standalone-${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

log_info() {
    echo -e "${CYAN}→ $1${NC}"
}

log_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

echo "================================================================================"
echo "E13 Standalone Validation: Connection-Limited Mesh"
echo "================================================================================"
echo ""
echo "Testing connection-limited mesh (3-5 peers/node) vs forced full-mesh"
echo "Results: ${RESULTS_DIR}"
echo ""

# Test 1: CAP Mesh Limited 24-node @ 1Gbps
log_info "[1/2] Testing CAP Mesh Limited 24-node @ 1Gbps"
log_info "Topology: platoon-24node-limited-mesh.yaml (~80 connections)"

cd /home/kit/Code/revolve/cap/cap-sim

# Source environment variables for Ditto credentials
log_info "Loading Ditto credentials from .env"
set -a
source .env
set +a

make sim-destroy || true
sleep 2

# Deploy limited mesh
containerlab deploy -t topologies/platoon-24node-limited-mesh.yaml

log_info "Warm-up: 30 seconds..."
sleep 30

log_info "Collecting data: 60 seconds..."
sleep 60

# Collect logs
log_info "Collecting logs..."
mkdir -p "../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/cap-mesh-limited-24node-1gbps"
for container in $(docker ps --format '{{.Names}}' | grep clab); do
    docker logs "$container" > "../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/cap-mesh-limited-24node-1gbps/${container}.log" 2>&1 || true
done

# Teardown
containerlab destroy -t topologies/platoon-24node-limited-mesh.yaml
sleep 5

log_success "Test 1 complete"
echo ""

# Test 2: CAP Mesh Full 24-node @ 1Gbps (E12 forced full-mesh for comparison)
log_info "[2/2] Testing CAP Mesh Full 24-node @ 1Gbps (E12 baseline)"
log_info "Topology: platoon-24node-dynamic-mesh.yaml (~552 connections)"

make sim-destroy || true
sleep 2

# Deploy full mesh
containerlab deploy -t topologies/platoon-24node-dynamic-mesh.yaml

log_info "Warm-up: 30 seconds..."
sleep 30

log_info "Collecting data: 60 seconds..."
sleep 60

# Collect logs
log_info "Collecting logs..."
mkdir -p "../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/cap-mesh-full-24node-1gbps"
for container in $(docker ps --format '{{.Names}}' | grep clab); do
    docker logs "$container" > "../labs/e12-comprehensive-empirical-validation/scripts/${RESULTS_DIR}/cap-mesh-full-24node-1gbps/${container}.log" 2>&1 || true
done

# Teardown
containerlab destroy -t topologies/platoon-24node-dynamic-mesh.yaml

log_success "Test 2 complete"
echo ""

echo "================================================================================"
log_success "E13 Standalone Validation Complete"
echo "================================================================================"
echo ""
echo "Results saved to: ${RESULTS_DIR}"
echo ""
echo "Next: Manually analyze logs to compare latencies:"
echo "  grep 'DocumentReceived' ${RESULTS_DIR}/*/\*.log | grep latency_ms"
echo ""
