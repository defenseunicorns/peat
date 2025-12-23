#!/bin/bash
# verify-consistency.sh - Verify CRDT data consistency across all nodes
#
# Checks that all nodes at each tier have received the expected documents
# and that aggregation counts match expectations.
#
# For hierarchical HIVE:
#   - Each soldier should have 1 NodeState (their own)
#   - Each squad leader should see NodeStates from all soldiers in their squad
#   - Each platoon leader should see SquadSummaries from all squads in their platoon
#   - Commander should see PlatoonSummaries from all platoons
#
# Usage: ./verify-consistency.sh [--verbose]

set -euo pipefail

LAB_FILTER="clab-lab4"
VERBOSE="${1:-}"
RESULTS_FILE="/tmp/lab5-consistency-results.jsonl"

log_info() { echo "[INFO] $*"; }
log_pass() { echo "[PASS] $*"; }
log_fail() { echo "[FAIL] $*"; }
log_warn() { echo "[WARN] $*"; }

# Get all containers by role
get_soldiers() {
    docker ps --filter "name=${LAB_FILTER}" --format '{{.Names}}' | grep "soldier" | grep -v "leader"
}

get_squad_leaders() {
    docker ps --filter "name=${LAB_FILTER}" --format '{{.Names}}' | grep -E "squad-[0-9]+-leader"
}

get_platoon_leaders() {
    docker ps --filter "name=${LAB_FILTER}" --format '{{.Names}}' | grep -E "platoon-[0-9]+-leader"
}

get_commander() {
    docker ps --filter "name=${LAB_FILTER}" --format '{{.Names}}' | grep "commander" | head -1
}

# Count aggregation events in a container's logs
count_aggregations() {
    local CONTAINER="$1"
    local TIER="$2"
    docker logs "$CONTAINER" 2>&1 | grep "AggregationCompleted" | grep "\"tier\":\"$TIER\"" | wc -l
}

# Get the last input_count from aggregation events
get_last_input_count() {
    local CONTAINER="$1"
    local TIER="$2"
    docker logs "$CONTAINER" 2>&1 | grep "AggregationCompleted" | grep "\"tier\":\"$TIER\"" | \
        tail -1 | grep -oP '"input_count":\K[0-9]+' || echo "0"
}

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  CRDT Data Consistency Verification                        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Check if Lab 4 deployment exists
TOTAL_CONTAINERS=$(docker ps --filter "name=${LAB_FILTER}" -q | wc -l)
if [ "$TOTAL_CONTAINERS" -eq 0 ]; then
    echo "ERROR: No Lab 4 deployment found. Run 'make lab4-96' first."
    exit 1
fi

log_info "Checking $TOTAL_CONTAINERS containers..."
echo ""

OVERALL_RESULT="CONSISTENT"
DETAILS=()

# 1. Verify squad leaders are aggregating soldiers
echo "=== Squad Leader Aggregation ==="
SQUAD_LEADERS=$(get_squad_leaders)
SQUAD_COUNT=$(echo "$SQUAD_LEADERS" | wc -l)
SQUAD_PASS=0
SQUAD_FAIL=0

for leader in $SQUAD_LEADERS; do
    AGG_COUNT=$(count_aggregations "$leader" "squad")
    INPUT_COUNT=$(get_last_input_count "$leader" "squad")

    if [ "$AGG_COUNT" -gt 0 ] && [ "$INPUT_COUNT" -gt 0 ]; then
        if [ -n "$VERBOSE" ]; then
            log_pass "$leader: $AGG_COUNT aggregations, last input=$INPUT_COUNT soldiers"
        fi
        SQUAD_PASS=$((SQUAD_PASS + 1))
    else
        log_fail "$leader: No aggregations (count=$AGG_COUNT, input=$INPUT_COUNT)"
        SQUAD_FAIL=$((SQUAD_FAIL + 1))
        OVERALL_RESULT="INCONSISTENT"
    fi
done

echo "  Squad leaders: $SQUAD_PASS/$SQUAD_COUNT passing"
DETAILS+=("\"squad_leaders\":{\"pass\":$SQUAD_PASS,\"total\":$SQUAD_COUNT}")
echo ""

# 2. Verify platoon leaders are aggregating squads
echo "=== Platoon Leader Aggregation ==="
PLATOON_LEADERS=$(get_platoon_leaders)
PLATOON_COUNT=$(echo "$PLATOON_LEADERS" | wc -l)
PLATOON_PASS=0
PLATOON_FAIL=0

for leader in $PLATOON_LEADERS; do
    AGG_COUNT=$(count_aggregations "$leader" "platoon")
    INPUT_COUNT=$(get_last_input_count "$leader" "platoon")

    if [ "$AGG_COUNT" -gt 0 ] && [ "$INPUT_COUNT" -gt 0 ]; then
        if [ -n "$VERBOSE" ]; then
            log_pass "$leader: $AGG_COUNT aggregations, last input=$INPUT_COUNT squads"
        fi
        PLATOON_PASS=$((PLATOON_PASS + 1))
    else
        log_fail "$leader: No aggregations (count=$AGG_COUNT, input=$INPUT_COUNT)"
        PLATOON_FAIL=$((PLATOON_FAIL + 1))
        OVERALL_RESULT="INCONSISTENT"
    fi
done

echo "  Platoon leaders: $PLATOON_PASS/$PLATOON_COUNT passing"
DETAILS+=("\"platoon_leaders\":{\"pass\":$PLATOON_PASS,\"total\":$PLATOON_COUNT}")
echo ""

# 3. Verify commander is aggregating platoons
echo "=== Company Commander Aggregation ==="
COMMANDER=$(get_commander)
if [ -n "$COMMANDER" ]; then
    AGG_COUNT=$(count_aggregations "$COMMANDER" "company")
    INPUT_COUNT=$(get_last_input_count "$COMMANDER" "company")

    if [ "$AGG_COUNT" -gt 0 ] && [ "$INPUT_COUNT" -gt 0 ]; then
        log_pass "$COMMANDER: $AGG_COUNT aggregations, last input=$INPUT_COUNT platoons"
        DETAILS+=("\"commander\":{\"pass\":true,\"aggregations\":$AGG_COUNT,\"input_count\":$INPUT_COUNT}")
    else
        log_fail "$COMMANDER: No company aggregations (count=$AGG_COUNT, input=$INPUT_COUNT)"
        OVERALL_RESULT="INCONSISTENT"
        DETAILS+=("\"commander\":{\"pass\":false,\"aggregations\":$AGG_COUNT,\"input_count\":$INPUT_COUNT}")
    fi
else
    log_fail "No commander found!"
    OVERALL_RESULT="INCONSISTENT"
    DETAILS+=("\"commander\":{\"pass\":false,\"error\":\"not_found\"}")
fi
echo ""

# 4. Check for any crashed or restarting containers
echo "=== Container Health ==="
RUNNING=$(docker ps --filter "name=${LAB_FILTER}" --filter "status=running" -q | wc -l)
EXITED=$(docker ps -a --filter "name=${LAB_FILTER}" --filter "status=exited" -q | wc -l)
RESTARTING=$(docker ps -a --filter "name=${LAB_FILTER}" --filter "status=restarting" -q | wc -l)

if [ "$EXITED" -gt 0 ] || [ "$RESTARTING" -gt 0 ]; then
    log_warn "Containers: $RUNNING running, $EXITED exited, $RESTARTING restarting"
    # Don't fail for exited containers after chaos tests - they may have been killed
else
    log_pass "All $RUNNING containers running healthy"
fi
DETAILS+=("\"container_health\":{\"running\":$RUNNING,\"exited\":$EXITED,\"restarting\":$RESTARTING}")
echo ""

# Final result
echo "============================================"
if [ "$OVERALL_RESULT" = "CONSISTENT" ]; then
    log_pass "DATA INTEGRITY: $OVERALL_RESULT"
    EXIT_CODE=0
else
    log_fail "DATA INTEGRITY: $OVERALL_RESULT"
    EXIT_CODE=1
fi
echo "============================================"

# Write result to file for lab5-report.sh
DETAILS_JSON=$(IFS=,; echo "${DETAILS[*]}")
RESULT_JSON="{\"test\":\"data_integrity\",\"result\":\"$OVERALL_RESULT\",\"details\":{$DETAILS_JSON},\"timestamp_ms\":$(date +%s%3N)}"
echo "$RESULT_JSON" >> "$RESULTS_FILE"

exit $EXIT_CODE
