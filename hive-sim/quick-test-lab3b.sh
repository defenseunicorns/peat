#!/bin/bash
# Quick Lab 3b Validation Test
# Tests flat mesh with HIVE CRDT on 5 nodes to validate implementation

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

source ./test-common.sh

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 3b Quick Validation: 5-Node Flat Mesh with CRDT     ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Validate environment
validate_environment

# Test parameters
NODE_COUNT=5
BANDWIDTH="1gbps"
TEST_DURATION=120  # 2 minutes
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="lab3b-validation-${TIMESTAMP}"
mkdir -p "${RESULTS_DIR}"

echo "Parameters:"
echo "  Nodes: ${NODE_COUNT}"
echo "  Bandwidth: ${BANDWIDTH}"
echo "  Duration: ${TEST_DURATION}s"
echo "  Results: ${RESULTS_DIR}"
echo ""

# Generate topology
TOPO_FILE="${RESULTS_DIR}/topology.yaml"
echo "[1/4] Generating flat mesh topology..."
python3 generate-flat-mesh-hive-topology.py ${NODE_COUNT} ${BANDWIDTH} ${TOPO_FILE}

# Deploy
echo "[2/4] Deploying with containerlab..."
containerlab deploy -t "${TOPO_FILE}" --reconfigure

# Wait for initialization
echo "[3/4] Running test for ${TEST_DURATION}s..."
echo "      (Nodes publishing updates and syncing via CRDT)"
sleep ${TEST_DURATION}

# Collect logs
echo "[4/4] Collecting logs..."
LOG_DIR="${RESULTS_DIR}/logs"
mkdir -p "${LOG_DIR}"

for i in $(seq 1 ${NODE_COUNT}); do
    CONTAINER="clab-hive-flat-mesh-${NODE_COUNT}n-${BANDWIDTH}-peer-${i}"
    echo "      Collecting from ${CONTAINER}..."
    docker logs "${CONTAINER}" 2>&1 > "${LOG_DIR}/peer-${i}.log"
done

# Cleanup
echo ""
echo "Cleaning up..."
containerlab destroy -t "${TOPO_FILE}" --cleanup

# Quick analysis
echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Quick Analysis                                           ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Check for flat mesh mode initialization
echo "Checking flat mesh mode initialization:"
for i in $(seq 1 ${NODE_COUNT}); do
    LOG="${LOG_DIR}/peer-${i}.log"
    if grep -q "FLAT MESH MODE" "${LOG}" 2>/dev/null; then
        echo "  ✅ peer-${i}: Flat mesh mode started"
    else
        echo "  ❌ peer-${i}: Mode not detected"
    fi
done

echo ""
echo "Checking CRDT document publishing:"
for i in $(seq 1 ${NODE_COUNT}); do
    LOG="${LOG_DIR}/peer-${i}.log"
    UPDATES=$(grep -c "Published state update" "${LOG}" 2>/dev/null || echo "0")
    echo "  peer-${i}: ${UPDATES} updates published"
done

echo ""
echo "Checking FlatMeshCoordinator:"
for i in $(seq 1 ${NODE_COUNT}); do
    LOG="${LOG_DIR}/peer-${i}.log"
    if grep -q "Initialized as flat mesh peer" "${LOG}" 2>/dev/null; then
        LEVEL=$(grep "Initialized as flat mesh peer" "${LOG}" | head -1 | grep -o "level: [^)]*" || echo "unknown")
        echo "  ✅ peer-${i}: Coordinator initialized (${LEVEL})"
    else
        echo "  ⚠️  peer-${i}: Coordinator status unclear"
    fi
done

echo ""
echo "Results saved to: ${RESULTS_DIR}"
echo ""
echo "Review full logs:"
echo "  ls -la ${LOG_DIR}/"
echo "  cat ${LOG_DIR}/peer-1.log | grep -E 'FLAT MESH|Published|Current role'"
echo ""

if grep -q "FLAT MESH MODE" "${LOG_DIR}/peer-1.log" 2>/dev/null; then
    echo "✅ Lab 3b validation PASSED - Flat mesh mode is working!"
    exit 0
else
    echo "❌ Lab 3b validation FAILED - Check logs for errors"
    exit 1
fi
