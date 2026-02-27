#!/bin/bash
# lab5-report.sh - Generate unified Lab 5 test results report
#
# Reads results from:
#   /tmp/lab5-recovery-results.jsonl  - Recovery time measurements
#   /tmp/lab5-consistency-results.jsonl - Data integrity verification
#   /tmp/lab5-chaos-events.jsonl - Chaos event history
#
# Success criteria from Epic #471:
#   - Node loss detection: < 5s
#   - Node loss recovery: < 10s
#   - 30s blackout recovery: < 60s after restoration
#   - Data integrity: No split-brain or corruption
#
# Usage: ./lab5-report.sh [--json]

set -euo pipefail

RECOVERY_RESULTS="/tmp/lab5-recovery-results.jsonl"
CONSISTENCY_RESULTS="/tmp/lab5-consistency-results.jsonl"
CHAOS_EVENTS="/tmp/lab5-chaos-events.jsonl"
JSON_OUTPUT="${1:-}"

# Targets from Epic #471
TARGET_KNOCKOUT_RECOVERY_MS=10000
TARGET_BLACKOUT_RECOVERY_MS=60000
TARGET_DETECTION_MS=5000

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 5 Chaos Engineering Test Results                      ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Generated: $(date '+%Y-%m-%d %H:%M:%S')"
echo ""

TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Helper to format pass/fail
result_badge() {
    local RESULT="$1"
    if [ "$RESULT" = "PASS" ] || [ "$RESULT" = "CONSISTENT" ]; then
        echo "[PASS]"
    else
        echo "[FAIL]"
    fi
}

# Section: Chaos Events Summary
echo "=== Chaos Events ==="
if [ -f "$CHAOS_EVENTS" ]; then
    INJECT_COUNT=$(grep -c '"type":"inject"' "$CHAOS_EVENTS" 2>/dev/null || echo "0")
    RECOVER_COUNT=$(grep -c '"type":"recover"' "$CHAOS_EVENTS" 2>/dev/null || echo "0")
    echo "  Inject events: $INJECT_COUNT"
    echo "  Recover events: $RECOVER_COUNT"

    # Show last few events
    echo ""
    echo "  Recent events:"
    tail -5 "$CHAOS_EVENTS" 2>/dev/null | while read -r line; do
        TYPE=$(echo "$line" | grep -oP '"type":"\K[^"]+' || echo "?")
        MODE=$(echo "$line" | grep -oP '"mode":"\K[^"]+' || echo "?")
        TS=$(echo "$line" | grep -oP '"timestamp_ms":\K[0-9]+' || echo "0")
        if [ "$TS" -gt 0 ]; then
            TIME=$(date -d "@$((TS / 1000))" '+%H:%M:%S' 2>/dev/null || echo "?")
            echo "    $TIME - $TYPE ($MODE)"
        fi
    done
else
    echo "  No chaos events recorded"
fi
echo ""

# Section: Recovery Time Results
echo "=== Recovery Time Measurements ==="
if [ -f "$RECOVERY_RESULTS" ]; then
    # Process each recovery result
    while IFS= read -r line; do
        MODE=$(echo "$line" | grep -oP '"mode":"\K[^"]+' || echo "unknown")
        RECOVERY_MS=$(echo "$line" | grep -oP '"recovery_time_ms":\K[0-9]+' || echo "0")
        TARGET_MS=$(echo "$line" | grep -oP '"target_ms":\K[0-9]+' || echo "60000")
        RESULT=$(echo "$line" | grep -oP '"result":"\K[^"]+' || echo "UNKNOWN")

        RECOVERY_S=$(awk "BEGIN {printf \"%.2f\", $RECOVERY_MS / 1000}")
        TARGET_S=$(awk "BEGIN {printf \"%.1f\", $TARGET_MS / 1000}")

        BADGE=$(result_badge "$RESULT")

        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        if [ "$RESULT" = "PASS" ]; then
            PASSED_TESTS=$((PASSED_TESTS + 1))
        else
            FAILED_TESTS=$((FAILED_TESTS + 1))
        fi

        printf "  %-20s %8ss  %-6s < %ss\n" "$MODE:" "$RECOVERY_S" "$BADGE" "$TARGET_S"
    done < "$RECOVERY_RESULTS"
else
    echo "  No recovery measurements recorded"
    echo "  Run chaos tests with 'make lab5-recover' then 'make lab5-recovery-time'"
fi
echo ""

# Section: Data Integrity Results
echo "=== Data Integrity Verification ==="
if [ -f "$CONSISTENCY_RESULTS" ]; then
    # Get the latest result
    LATEST=$(tail -1 "$CONSISTENCY_RESULTS")
    RESULT=$(echo "$LATEST" | grep -oP '"result":"\K[^"]+' || echo "UNKNOWN")
    BADGE=$(result_badge "$RESULT")

    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    if [ "$RESULT" = "CONSISTENT" ]; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi

    echo "  Status: $RESULT $BADGE"

    # Show details if available
    SQUAD_PASS=$(echo "$LATEST" | grep -oP '"squad_leaders":\{"pass":\K[0-9]+' || echo "?")
    SQUAD_TOTAL=$(echo "$LATEST" | grep -oP '"squad_leaders":\{"pass":[0-9]+,"total":\K[0-9]+' || echo "?")
    PLATOON_PASS=$(echo "$LATEST" | grep -oP '"platoon_leaders":\{"pass":\K[0-9]+' || echo "?")
    PLATOON_TOTAL=$(echo "$LATEST" | grep -oP '"platoon_leaders":\{"pass":[0-9]+,"total":\K[0-9]+' || echo "?")

    if [ "$SQUAD_PASS" != "?" ]; then
        echo "    Squad leaders:   $SQUAD_PASS/$SQUAD_TOTAL aggregating"
        echo "    Platoon leaders: $PLATOON_PASS/$PLATOON_TOTAL aggregating"
    fi
else
    echo "  No consistency checks recorded"
    echo "  Run 'make lab5-verify-consistency' after chaos tests"
fi
echo ""

# Section: Summary
echo "============================================"
echo "SUMMARY"
echo "============================================"
if [ $TOTAL_TESTS -eq 0 ]; then
    echo "  No tests recorded yet."
    echo ""
    echo "  To run Lab 5 tests:"
    echo "    1. Deploy Lab 4: make lab4-96"
    echo "    2. Run chaos: make lab5-blackout DURATION=30"
    echo "    3. Recover: make lab5-recover"
    echo "    4. Measure: make lab5-recovery-time"
    echo "    5. Verify: make lab5-verify-consistency"
    echo "    6. Report: make lab5-report"
else
    echo "  Tests Run:    $TOTAL_TESTS"
    echo "  Passed:       $PASSED_TESTS"
    echo "  Failed:       $FAILED_TESTS"
    echo ""

    if [ $FAILED_TESTS -eq 0 ]; then
        echo "  Overall: ALL TESTS PASSED"
        EXIT_CODE=0
    else
        echo "  Overall: $FAILED_TESTS TEST(S) FAILED"
        EXIT_CODE=1
    fi
fi
echo "============================================"

# Optional JSON output
if [ "$JSON_OUTPUT" = "--json" ]; then
    echo ""
    echo "=== JSON Output ==="
    cat << EOF
{
  "timestamp": "$(date -Iseconds)",
  "total_tests": $TOTAL_TESTS,
  "passed": $PASSED_TESTS,
  "failed": $FAILED_TESTS,
  "result": "$([ $FAILED_TESTS -eq 0 ] && echo "PASS" || echo "FAIL")"
}
EOF
fi

exit ${EXIT_CODE:-0}
