#!/bin/bash
# measure-recovery.sh - Measure recovery time after chaos events
#
# Reads chaos event timestamps from /tmp/lab5-chaos-events.jsonl
# Monitors commander logs for AggregationCompleted events
# Calculates recovery_time = first_aggregation_after_chaos - chaos_recover_timestamp
#
# Usage: ./measure-recovery.sh [--target <ms>] [--chaos-mode <mode>]
#   --target: Target recovery time in ms (default: 60000 for blackout, 10000 for others)
#   --chaos-mode: Filter by chaos mode (blackout, partition, knockout, etc.)

set -euo pipefail

CHAOS_EVENTS_FILE="/tmp/lab5-chaos-events.jsonl"
LAB_FILTER="clab-lab4"
TARGET_MS=""
CHAOS_MODE=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --target) TARGET_MS="$2"; shift 2 ;;
        --chaos-mode) CHAOS_MODE="$2"; shift 2 ;;
        *) shift ;;
    esac
done

# Check if chaos events file exists
if [ ! -f "$CHAOS_EVENTS_FILE" ]; then
    echo "ERROR: No chaos events file found at $CHAOS_EVENTS_FILE"
    echo "Run a chaos test first (e.g., make lab5-blackout)"
    exit 1
fi

# Find the commander container (docker filter with multiple --filter name= is OR, so use grep)
COMMANDER=$(docker ps --filter "name=${LAB_FILTER}" --format '{{.Names}}' | grep "commander" | head -1)
if [ -z "$COMMANDER" ]; then
    echo "ERROR: No commander container found"
    exit 1
fi

echo "Measuring recovery time..."
echo "Commander: $COMMANDER"
echo "Chaos events file: $CHAOS_EVENTS_FILE"
echo ""

# Get the last recover event from chaos events file
if [ -n "$CHAOS_MODE" ]; then
    RECOVER_EVENT=$(grep '"type":"recover"' "$CHAOS_EVENTS_FILE" | grep "\"mode\":\"$CHAOS_MODE\"" | tail -1 || echo "")
else
    RECOVER_EVENT=$(grep '"type":"recover"' "$CHAOS_EVENTS_FILE" | tail -1 || echo "")
fi

if [ -z "$RECOVER_EVENT" ]; then
    echo "ERROR: No recover event found in chaos events file"
    echo "Make sure you ran 'make lab5-recover' after chaos injection"
    exit 1
fi

# Extract timestamp from recover event
RECOVER_TIMESTAMP_MS=$(echo "$RECOVER_EVENT" | grep -oP '"timestamp_ms":\K[0-9]+')
RECOVER_MODE=$(echo "$RECOVER_EVENT" | grep -oP '"mode":"\K[^"]+')

echo "Last recovery event:"
echo "  Mode: $RECOVER_MODE"
echo "  Timestamp: $RECOVER_TIMESTAMP_MS ms ($(date -d @$((RECOVER_TIMESTAMP_MS / 1000)) '+%H:%M:%S'))"
echo ""

# Set default target based on mode if not specified
if [ -z "$TARGET_MS" ]; then
    case "$RECOVER_MODE" in
        blackout|intermittent) TARGET_MS=60000 ;;
        partition) TARGET_MS=60000 ;;
        *) TARGET_MS=10000 ;;
    esac
fi

# Get commander logs and find first AggregationCompleted after recover timestamp
# The logs have format: METRICS: {"event_type":"AggregationCompleted",...,"timestamp_us":...}
echo "Searching for first aggregation after recovery..."

# Get all aggregation events from commander logs
AGGREGATION_EVENTS=$(docker logs "$COMMANDER" 2>&1 | grep "AggregationCompleted" | grep '"tier":"company"' || echo "")

if [ -z "$AGGREGATION_EVENTS" ]; then
    echo "WARNING: No company-tier AggregationCompleted events found in commander logs"
    echo "This might be expected if the system hasn't fully recovered yet"
    exit 1
fi

# Find first aggregation event after recover timestamp
FIRST_AGGREGATION_AFTER_RECOVER=""
FIRST_AGGREGATION_TIMESTAMP_US=""

while IFS= read -r line; do
    # Extract timestamp_us from the JSON
    TIMESTAMP_US=$(echo "$line" | grep -oP '"timestamp_us":\K[0-9]+' || echo "")
    if [ -z "$TIMESTAMP_US" ]; then
        continue
    fi

    # Convert to ms for comparison
    TIMESTAMP_MS=$((TIMESTAMP_US / 1000))

    if [ "$TIMESTAMP_MS" -gt "$RECOVER_TIMESTAMP_MS" ]; then
        FIRST_AGGREGATION_AFTER_RECOVER="$line"
        FIRST_AGGREGATION_TIMESTAMP_US="$TIMESTAMP_US"
        break
    fi
done <<< "$AGGREGATION_EVENTS"

if [ -z "$FIRST_AGGREGATION_AFTER_RECOVER" ]; then
    echo "WARNING: No aggregation event found after recovery timestamp"
    echo "The system may still be recovering..."

    # Get the last aggregation timestamp
    LAST_AGG=$(echo "$AGGREGATION_EVENTS" | tail -1)
    LAST_TIMESTAMP_US=$(echo "$LAST_AGG" | grep -oP '"timestamp_us":\K[0-9]+' || echo "0")
    LAST_TIMESTAMP_MS=$((LAST_TIMESTAMP_US / 1000))

    echo "Last aggregation was at: $(date -d @$((LAST_TIMESTAMP_MS / 1000)) '+%H:%M:%S')"
    echo "Recovery was at: $(date -d @$((RECOVER_TIMESTAMP_MS / 1000)) '+%H:%M:%S')"
    exit 1
fi

# Calculate recovery time
FIRST_AGG_TIMESTAMP_MS=$((FIRST_AGGREGATION_TIMESTAMP_US / 1000))
RECOVERY_TIME_MS=$((FIRST_AGG_TIMESTAMP_MS - RECOVER_TIMESTAMP_MS))
RECOVERY_TIME_S=$(awk "BEGIN {printf \"%.2f\", $RECOVERY_TIME_MS / 1000}")

echo "First aggregation after recovery:"
echo "  Timestamp: $FIRST_AGG_TIMESTAMP_MS ms ($(date -d @$((FIRST_AGG_TIMESTAMP_MS / 1000)) '+%H:%M:%S'))"
echo ""

# Determine pass/fail
if [ "$RECOVERY_TIME_MS" -le "$TARGET_MS" ]; then
    RESULT="PASS"
    RESULT_SYMBOL="[PASS]"
else
    RESULT="FAIL"
    RESULT_SYMBOL="[FAIL]"
fi

TARGET_S=$(awk "BEGIN {printf \"%.1f\", $TARGET_MS / 1000}")

echo "============================================"
echo "RECOVERY TIME MEASUREMENT"
echo "============================================"
echo "Chaos Mode:      $RECOVER_MODE"
echo "Recovery Time:   ${RECOVERY_TIME_S}s  $RESULT_SYMBOL < ${TARGET_S}s"
echo "============================================"

# Write result to file for lab5-report.sh
RESULTS_FILE="/tmp/lab5-recovery-results.jsonl"
RESULT_JSON="{\"test\":\"recovery_time\",\"mode\":\"$RECOVER_MODE\",\"recovery_time_ms\":$RECOVERY_TIME_MS,\"target_ms\":$TARGET_MS,\"result\":\"$RESULT\",\"timestamp_ms\":$(date +%s%3N)}"
echo "$RESULT_JSON" >> "$RESULTS_FILE"

if [ "$RESULT" = "PASS" ]; then
    exit 0
else
    exit 1
fi
