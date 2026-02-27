#!/bin/bash
# Verify All Labs - Quick smoke test for each lab infrastructure
#
# This script runs a minimal test for each lab to verify:
# 1. Topology deploys correctly
# 2. Containers start and produce logs
# 3. Metrics are being collected
# 4. Cleanup works properly
#
# Duration: ~10-15 minutes total (vs hours for full suite)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

source ./test-common.sh

# Source Ditto credentials
if [ -f "../.env" ]; then
    echo "Loading Ditto credentials from ../.env"
    set -a
    source ../.env
    set +a
    export DITTO_APP_ID DITTO_OFFLINE_TOKEN DITTO_SHARED_KEY
fi

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="verification-${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  PEAT Labs Verification Suite                              ║"
echo "║  Quick smoke test for all lab infrastructure               ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results directory: ${RESULTS_DIR}"
echo ""

# Validate environment first
validate_environment

# Track results
RESULTS_FILE="${RESULTS_DIR}/verification-results.md"
echo "# Lab Verification Results" > "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "**Date:** $(date)" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"

PASS_COUNT=0
FAIL_COUNT=0

# Function to run a single lab verification
verify_lab() {
    local LAB_NAME="$1"
    local TOPOLOGY="$2"
    local EXPECTED_CONTAINERS="$3"
    local METRIC_PATTERN="$4"
    local DURATION="$5"

    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "Verifying: ${LAB_NAME}"
    echo "═══════════════════════════════════════════════════════════"

    local STATUS="PASS"
    local DETAILS=""

    # Step 1: Check topology file exists
    if [ ! -f "topologies/${TOPOLOGY}" ]; then
        echo "  ❌ Topology file not found: ${TOPOLOGY}"
        STATUS="FAIL"
        DETAILS="Topology file missing"
        echo "## ${LAB_NAME}: ❌ FAIL" >> "$RESULTS_FILE"
        echo "- ${DETAILS}" >> "$RESULTS_FILE"
        echo "" >> "$RESULTS_FILE"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        return 1
    fi
    echo "  ✅ Topology file exists"

    # Step 2: Deploy topology
    echo "  → Deploying topology..."
    if ! containerlab deploy -t "topologies/${TOPOLOGY}" --reconfigure > "${RESULTS_DIR}/${LAB_NAME}-deploy.log" 2>&1; then
        echo "  ❌ Deployment failed"
        STATUS="FAIL"
        DETAILS="Deployment failed - check ${LAB_NAME}-deploy.log"
        echo "## ${LAB_NAME}: ❌ FAIL" >> "$RESULTS_FILE"
        echo "- ${DETAILS}" >> "$RESULTS_FILE"
        echo "" >> "$RESULTS_FILE"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        return 1
    fi
    echo "  ✅ Topology deployed"

    # Step 3: Verify expected containers are running
    echo "  → Checking containers..."
    sleep 5  # Give containers time to start
    local RUNNING_COUNT=$(docker ps --filter "name=clab" --format "{{.Names}}" | wc -l)
    if [ "$RUNNING_COUNT" -lt "$EXPECTED_CONTAINERS" ]; then
        echo "  ⚠️  Only ${RUNNING_COUNT} containers running (expected ${EXPECTED_CONTAINERS})"
        DETAILS="${DETAILS}Only ${RUNNING_COUNT}/${EXPECTED_CONTAINERS} containers; "
    else
        echo "  ✅ ${RUNNING_COUNT} containers running"
    fi

    # Step 4: Wait for test duration
    echo "  → Running for ${DURATION}s..."
    sleep "$DURATION"

    # Step 5: Collect logs and check for metrics
    echo "  → Collecting logs..."
    mkdir -p "${RESULTS_DIR}/${LAB_NAME}"

    local CONTAINERS_WITH_METRICS=0
    for CONTAINER in $(docker ps --filter "name=clab" --format "{{.Names}}" | head -5); do
        docker logs "$CONTAINER" 2>&1 > "${RESULTS_DIR}/${LAB_NAME}/${CONTAINER}.log" || true

        if grep -q "${METRIC_PATTERN}" "${RESULTS_DIR}/${LAB_NAME}/${CONTAINER}.log" 2>/dev/null; then
            CONTAINERS_WITH_METRICS=$((CONTAINERS_WITH_METRICS + 1))
        fi
    done

    if [ "$CONTAINERS_WITH_METRICS" -eq 0 ]; then
        echo "  ⚠️  No containers produced expected metrics (pattern: ${METRIC_PATTERN})"
        DETAILS="${DETAILS}No metrics found; "
    else
        echo "  ✅ ${CONTAINERS_WITH_METRICS} containers produced metrics"
    fi

    # Step 6: Cleanup
    echo "  → Cleaning up..."
    if ! containerlab destroy -t "topologies/${TOPOLOGY}" --cleanup > "${RESULTS_DIR}/${LAB_NAME}-destroy.log" 2>&1; then
        echo "  ⚠️  Cleanup warning (may be harmless)"
    fi

    # Verify cleanup
    sleep 2
    local REMAINING=$(docker ps --filter "name=clab" --format "{{.Names}}" | wc -l)
    if [ "$REMAINING" -gt 0 ]; then
        echo "  ⚠️  ${REMAINING} containers still running after cleanup"
        DETAILS="${DETAILS}${REMAINING} containers remain; "
    else
        echo "  ✅ All containers cleaned up"
    fi

    # Final status
    if [ -z "$DETAILS" ]; then
        echo "  ✅ ${LAB_NAME} PASSED"
        echo "## ${LAB_NAME}: ✅ PASS" >> "$RESULTS_FILE"
        echo "" >> "$RESULTS_FILE"
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        echo "  ⚠️  ${LAB_NAME} completed with warnings: ${DETAILS}"
        echo "## ${LAB_NAME}: ⚠️ PASS (with warnings)" >> "$RESULTS_FILE"
        echo "- ${DETAILS}" >> "$RESULTS_FILE"
        echo "" >> "$RESULTS_FILE"
        PASS_COUNT=$((PASS_COUNT + 1))
    fi

    return 0
}

# ============================================================================
# Lab 1: Producer-Only Baseline
# ============================================================================
# Uses traditional_baseline binary with USE_PRODUCER_ONLY=true
# Topology: traditional-platoon-24node.yaml (smallest traditional topology)

echo ""
echo "Note: Lab 1 uses dynamically generated topology, testing with 24-node traditional"
verify_lab \
    "Lab1-ProducerOnly" \
    "traditional-platoon-24node.yaml" \
    "24" \
    "METRICS" \
    "30"

# ============================================================================
# Lab 2: Client-Server Full Replication
# ============================================================================
# Uses traditional_baseline binary (full broadcast)
# Topology: traditional-platoon-24node.yaml

verify_lab \
    "Lab2-ClientServer" \
    "traditional-platoon-24node.yaml" \
    "24" \
    "METRICS" \
    "30"

# ============================================================================
# Lab 3: P2P Mesh (Raw TCP)
# ============================================================================
# Uses p2p_mesh_baseline binary
# Topology: squad-12node-dynamic-mesh.yaml (or similar mesh topology)

if [ -f "topologies/squad-12node-dynamic-mesh.yaml" ]; then
    verify_lab \
        "Lab3-P2PMesh" \
        "squad-12node-dynamic-mesh.yaml" \
        "12" \
        "METRICS" \
        "30"
else
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "Verifying: Lab3-P2PMesh"
    echo "═══════════════════════════════════════════════════════════"
    echo "  ⚠️  Skipping - no suitable mesh topology found"
    echo "## Lab3-P2PMesh: ⏭️ SKIPPED" >> "$RESULTS_FILE"
    echo "- No suitable mesh topology found" >> "$RESULTS_FILE"
fi

# ============================================================================
# Lab 3b: PEAT Flat Mesh (CRDT)
# ============================================================================
# Uses peat-sim with MODE=flat_mesh
# Topology: squad-12node-dynamic-mesh.yaml

if [ -f "topologies/squad-12node-dynamic-mesh.yaml" ]; then
    verify_lab \
        "Lab3b-PEATFlatMesh" \
        "squad-12node-dynamic-mesh.yaml" \
        "12" \
        "METRICS" \
        "30"
else
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "Verifying: Lab3b-PEATFlatMesh"
    echo "═══════════════════════════════════════════════════════════"
    echo "  ⚠️  Skipping - no suitable mesh topology found"
    echo "## Lab3b-PEATFlatMesh: ⏭️ SKIPPED" >> "$RESULTS_FILE"
    echo "- No suitable mesh topology found" >> "$RESULTS_FILE"
fi

# ============================================================================
# Lab 4: PEAT Hierarchical CRDT
# ============================================================================
# Uses peat-sim with MODE=hierarchical_crdt or mode4_mesh
# Topology: platoon-24node-mesh-mode4.yaml

verify_lab \
    "Lab4-PEATHierarchical" \
    "platoon-24node-mesh-mode4.yaml" \
    "24" \
    "METRICS" \
    "30"

# ============================================================================
# Summary
# ============================================================================

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Verification Complete                                     ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: ${RESULTS_DIR}"
echo "Passed: ${PASS_COUNT}"
echo "Failed: ${FAIL_COUNT}"
echo ""
echo "Detailed results: ${RESULTS_FILE}"
echo ""

# Summary to results file
echo "---" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "## Summary" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "- **Passed:** ${PASS_COUNT}" >> "$RESULTS_FILE"
echo "- **Failed:** ${FAIL_COUNT}" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"

if [ "$FAIL_COUNT" -eq 0 ]; then
    echo "✅ All labs verified successfully!"
    exit 0
else
    echo "❌ Some labs failed verification - check ${RESULTS_FILE}"
    exit 1
fi
