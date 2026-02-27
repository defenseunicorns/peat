#!/bin/bash
# Monitor Lab 3b test suite progress

LOG_FILE="/tmp/lab3b-full-suite.log"

if [ ! -f "$LOG_FILE" ]; then
    echo "❌ Log file not found: $LOG_FILE"
    exit 1
fi

echo "════════════════════════════════════════════════════════════"
echo "Lab 3b Test Suite Progress Monitor"
echo "════════════════════════════════════════════════════════════"
echo ""

# Extract current test number
CURRENT_TEST=$(grep -E "\[[0-9]+/24\]" "$LOG_FILE" | tail -1 | grep -oE "\[[0-9]+/24\]" | grep -oE "[0-9]+" | head -1)
if [ -z "$CURRENT_TEST" ]; then
    CURRENT_TEST=0
fi

TOTAL_TESTS=24
PCT_COMPLETE=$((CURRENT_TEST * 100 / TOTAL_TESTS))

echo "Progress: Test $CURRENT_TEST of $TOTAL_TESTS ($PCT_COMPLETE% complete)"
echo ""

# Show recent activity
echo "Recent activity:"
tail -10 "$LOG_FILE" | grep -E "peat-flat-mesh|Running for|✅|❌" | tail -5
echo ""

# Estimate remaining time
if [ "$CURRENT_TEST" -gt 0 ]; then
    # ~140s per test (120s run + ~20s overhead)
    REMAINING_TESTS=$((TOTAL_TESTS - CURRENT_TEST))
    REMAINING_SECONDS=$((REMAINING_TESTS * 140))
    REMAINING_MINUTES=$((REMAINING_SECONDS / 60))

    echo "Estimated time remaining: ~$REMAINING_MINUTES minutes"
fi

echo ""
echo "Monitor continuously:"
echo "  watch -n 10 ./monitor-lab3b-progress.sh"
echo ""
echo "View full log:"
echo "  tail -f /tmp/lab3b-full-suite.log"
