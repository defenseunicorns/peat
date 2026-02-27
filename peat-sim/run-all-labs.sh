#!/bin/bash
# Run All Labs - Complete Empirical Validation Suite
#
# Runs all 5 labs back-to-back for Epic #132 validation
# Estimated runtime: 6-8 hours total
#
# Labs:
#   Lab 1: Producer-Only (32 tests) - ~2 hours
#   Lab 2: Client-Server (32 tests) - ~2 hours
#   Lab 3b: PEAT Flat Mesh (24 tests) - ~1 hour
#   Lab 4: PEAT Hierarchical (20 tests) - ~1 hour

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Source Ditto credentials
if [ -f "../.env" ]; then
    echo "Loading Ditto credentials from ../.env"
    set -a
    source ../.env
    set +a
    export DITTO_APP_ID DITTO_OFFLINE_TOKEN DITTO_SHARED_KEY
else
    echo "ERROR: ../.env not found - Ditto credentials required"
    exit 1
fi

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
MASTER_LOG="all-labs-${TIMESTAMP}.log"
RESULTS_SUMMARY="all-labs-summary-${TIMESTAMP}.md"

echo "╔════════════════════════════════════════════════════════════════════╗"
echo "║  PEAT Labs - Complete Empirical Validation Suite                   ║"
echo "║  Epic #132: Distribution Architecture Boundary Validation          ║"
echo "╚════════════════════════════════════════════════════════════════════╝"
echo ""
echo "Started: $(date)"
echo "Master log: ${MASTER_LOG}"
echo ""

# Initialize summary
cat > "$RESULTS_SUMMARY" << EOF
# Epic #132: Complete Lab Results

**Run Started:** $(date)
**Host:** $(hostname)

## Lab Execution Summary

EOF

# Track timing
START_TIME=$(date +%s)

run_lab() {
    local LAB_NAME="$1"
    local LAB_SCRIPT="$2"
    local LAB_NUM="$3"
    local TOTAL_LABS="$4"

    echo ""
    echo "╔════════════════════════════════════════════════════════════════════╗"
    echo "║  [${LAB_NUM}/${TOTAL_LABS}] ${LAB_NAME}"
    echo "║  Started: $(date +%H:%M:%S)"
    echo "╚════════════════════════════════════════════════════════════════════╝"
    echo ""

    local LAB_START=$(date +%s)

    if [ -f "./${LAB_SCRIPT}" ]; then
        echo "Running ${LAB_SCRIPT}..."
        if ./"${LAB_SCRIPT}" 2>&1 | tee -a "$MASTER_LOG"; then
            local LAB_END=$(date +%s)
            local LAB_DURATION=$((LAB_END - LAB_START))
            local LAB_MINS=$((LAB_DURATION / 60))
            echo ""
            echo "✅ ${LAB_NAME} completed in ${LAB_MINS} minutes"
            echo "| ${LAB_NAME} | ✅ PASS | ${LAB_MINS} min |" >> "$RESULTS_SUMMARY"
        else
            local LAB_END=$(date +%s)
            local LAB_DURATION=$((LAB_END - LAB_START))
            local LAB_MINS=$((LAB_DURATION / 60))
            echo ""
            echo "❌ ${LAB_NAME} failed after ${LAB_MINS} minutes"
            echo "| ${LAB_NAME} | ❌ FAIL | ${LAB_MINS} min |" >> "$RESULTS_SUMMARY"
        fi
    else
        echo "⚠️  Script not found: ${LAB_SCRIPT}"
        echo "| ${LAB_NAME} | ⏭️ SKIP | N/A |" >> "$RESULTS_SUMMARY"
    fi
}

# Add table header to summary
echo "| Lab | Status | Duration |" >> "$RESULTS_SUMMARY"
echo "|-----|--------|----------|" >> "$RESULTS_SUMMARY"

# ============================================================================
# Lab 1: Producer-Only Baseline
# ============================================================================
run_lab "Lab 1: Producer-Only Baseline" "test-producer-only.sh" 1 4

# ============================================================================
# Lab 2: Client-Server Full Replication
# ============================================================================
run_lab "Lab 2: Client-Server Full Replication" "test-traditional-baseline.sh" 2 4

# ============================================================================
# Lab 3b: PEAT Flat Mesh (CRDT)
# ============================================================================
run_lab "Lab 3b: PEAT Flat Mesh CRDT" "test-lab3b-peat-mesh.sh" 3 4

# ============================================================================
# Lab 4: PEAT Hierarchical CRDT
# ============================================================================
run_lab "Lab 4: PEAT Hierarchical CRDT" "test-lab4-hierarchical-peat-crdt.sh" 4 4

# ============================================================================
# Summary
# ============================================================================

END_TIME=$(date +%s)
TOTAL_DURATION=$((END_TIME - START_TIME))
TOTAL_HOURS=$((TOTAL_DURATION / 3600))
TOTAL_MINS=$(((TOTAL_DURATION % 3600) / 60))

echo ""
echo "╔════════════════════════════════════════════════════════════════════╗"
echo "║  All Labs Complete                                                 ║"
echo "╚════════════════════════════════════════════════════════════════════╝"
echo ""
echo "Completed: $(date)"
echo "Total runtime: ${TOTAL_HOURS}h ${TOTAL_MINS}m"
echo ""
echo "Results:"
echo "  - Master log: ${MASTER_LOG}"
echo "  - Summary: ${RESULTS_SUMMARY}"
echo ""

# Find result directories
echo "Result directories:"
ls -d *-202511* 2>/dev/null | head -10 || echo "  (check for timestamped directories)"

# Append to summary
cat >> "$RESULTS_SUMMARY" << EOF

---

## Execution Details

**Run Completed:** $(date)
**Total Runtime:** ${TOTAL_HOURS}h ${TOTAL_MINS}m

## Result Directories

$(ls -d *-202511* 2>/dev/null || echo "Check for timestamped directories")

## Next Steps

1. Run analysis scripts on each result directory
2. Generate cross-lab comparison report
3. Update Epic #132 with findings

EOF

echo ""
echo "✅ Epic #132 empirical validation suite complete!"
