#!/bin/bash
# process-lab4-metrics.sh - Extract and analyze metrics from experiment logs
#
# Works on whatever data exists - designed for post-mortem analysis even after
# partial runs or OOM crashes.
#
# Usage:
#   ./process-lab4-metrics.sh /work/peat-sim-results/lab4-automerge-384n-1gbps-20241217-120000
#   ./process-lab4-metrics.sh --latest   # Process most recent results directory

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Parse arguments
if [[ $# -eq 0 ]]; then
    echo "Usage: $0 <results-dir> | --latest"
    exit 1
fi

if [[ "$1" == "--latest" ]]; then
    RESULTS_DIR=$(find /work/peat-sim-results -maxdepth 1 -type d -name "lab4-*" -printf '%T@ %p\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-)
    if [[ -z "$RESULTS_DIR" ]]; then
        echo "ERROR: No lab4 results directories found"
        exit 1
    fi
else
    RESULTS_DIR="$1"
fi

if [[ ! -d "$RESULTS_DIR" ]]; then
    echo "ERROR: Directory not found: $RESULTS_DIR"
    exit 1
fi

echo "========================================"
echo "Processing Lab 4 Metrics"
echo "========================================"
echo "Results directory: $RESULTS_DIR"
echo ""

# Output files
METRICS_RAW="${RESULTS_DIR}/metrics-raw.jsonl"
METRICS_CSV="${RESULTS_DIR}/metrics-summary.csv"
REPORT_FILE="${RESULTS_DIR}/analysis-report.txt"

# Read state if available
STATE_FILE="${RESULTS_DIR}/state.json"
if [[ -f "$STATE_FILE" ]]; then
    echo "Experiment state:"
    cat "$STATE_FILE" | jq -r 'to_entries | .[] | "  \(.key): \(.value)"' 2>/dev/null || cat "$STATE_FILE"
    echo ""
fi

# Find all log directories (snapshots)
LOG_DIR="${RESULTS_DIR}/logs"
if [[ ! -d "$LOG_DIR" ]]; then
    echo "ERROR: No logs directory found at $LOG_DIR"
    exit 1
fi

SNAPSHOT_COUNT=$(find "$LOG_DIR" -maxdepth 1 -type d | wc -l)
TOTAL_LOGS=$(find "$LOG_DIR" -name "*.log" 2>/dev/null | wc -l)

echo "Log inventory:"
echo "  Snapshots: $((SNAPSHOT_COUNT - 1))"  # -1 for the logs dir itself
echo "  Total log files: $TOTAL_LOGS"

if [[ $TOTAL_LOGS -eq 0 ]]; then
    echo ""
    echo "ERROR: No log files found"
    exit 1
fi

# List snapshots
echo "  Snapshot directories:"
find "$LOG_DIR" -maxdepth 1 -type d -name "*-*" | sort | while read -r dir; do
    count=$(find "$dir" -name "*.log" | wc -l)
    echo "    $(basename "$dir"): $count logs"
done
echo ""

# ============================================================================
# PHASE 1: Extract all METRICS lines
# ============================================================================
echo "Phase 1: Extracting metrics..."

# Use the LATEST snapshot for each container (final > stream > initial)
# This ensures we get the most complete data
> "$METRICS_RAW"

# Find all unique container names across all snapshots
CONTAINERS=$(find "$LOG_DIR" -name "*.log" -exec basename {} .log \; | sort -u)
CONTAINER_COUNT=$(echo "$CONTAINERS" | wc -l)
echo "  Found $CONTAINER_COUNT unique containers"

# For each container, use the log from the latest snapshot
for container in $CONTAINERS; do
    # Find all logs for this container, prefer final > stream > initial
    LOG_FILE=""
    for pattern in "final-*" "stream-*" "initial-*" "emergency-*"; do
        found=$(find "$LOG_DIR" -path "*/$pattern/$container.log" 2>/dev/null | sort -r | head -1)
        if [[ -n "$found" && -s "$found" ]]; then
            LOG_FILE="$found"
            break
        fi
    done

    # If no snapshot logs, check for direct logs in logs/
    if [[ -z "$LOG_FILE" && -f "${LOG_DIR}/${container}.log" ]]; then
        LOG_FILE="${LOG_DIR}/${container}.log"
    fi

    if [[ -n "$LOG_FILE" && -s "$LOG_FILE" ]]; then
        grep 'METRICS:' "$LOG_FILE" 2>/dev/null | sed 's/.*METRICS: //' >> "$METRICS_RAW" || true
    fi
done

METRICS_COUNT=$(wc -l < "$METRICS_RAW")
echo "  Extracted $METRICS_COUNT metric events"
echo ""

if [[ $METRICS_COUNT -eq 0 ]]; then
    echo "WARNING: No metrics found in logs"
    echo "This could mean:"
    echo "  - Containers didn't run long enough to emit metrics"
    echo "  - Logs were collected before metrics were emitted"
    echo "  - Different log format than expected"
    echo ""
    echo "Sample log content:"
    find "$LOG_DIR" -name "*.log" -size +0 | head -3 | while read -r f; do
        echo "  $f:"
        head -5 "$f" | sed 's/^/    /'
    done
    exit 1
fi

# ============================================================================
# PHASE 2: Analyze metrics
# ============================================================================
echo "Phase 2: Analyzing metrics..."

# Create analysis report
cat > "$REPORT_FILE" << EOF
Lab 4 Metrics Analysis Report
==============================
Generated: $(date -Iseconds)
Results: $RESULTS_DIR

EOF

# Extract experiment parameters from state or directory name
if [[ -f "$STATE_FILE" ]]; then
    NODE_COUNT=$(jq -r '.node_count // "unknown"' "$STATE_FILE")
    BACKEND=$(jq -r '.backend // "unknown"' "$STATE_FILE")
    BANDWIDTH=$(jq -r '.bandwidth // "unknown"' "$STATE_FILE")
else
    # Try to parse from directory name: lab4-automerge-384n-1gbps-TIMESTAMP
    DIRNAME=$(basename "$RESULTS_DIR")
    BACKEND=$(echo "$DIRNAME" | sed -n 's/lab4-\([^-]*\)-.*/\1/p')
    NODE_COUNT=$(echo "$DIRNAME" | sed -n 's/.*-\([0-9]*\)n-.*/\1/p')
    BANDWIDTH=$(echo "$DIRNAME" | sed -n 's/.*n-\([^-]*\)-.*/\1/p')
fi

cat >> "$REPORT_FILE" << EOF
Experiment Parameters:
  Nodes: $NODE_COUNT
  Backend: $BACKEND
  Bandwidth: $BANDWIDTH
  Total metrics: $METRICS_COUNT

EOF

# Count event types
echo "Event type distribution:" >> "$REPORT_FILE"
jq -r '.event_type' "$METRICS_RAW" 2>/dev/null | sort | uniq -c | sort -rn | head -20 >> "$REPORT_FILE" || {
    echo "  (Could not parse event types)" >> "$REPORT_FILE"
}
echo "" >> "$REPORT_FILE"

# Extract latencies by event type
echo "Latency Analysis:" >> "$REPORT_FILE"

# DocumentReceived latencies
DOC_RECEIVED_COUNT=$(grep -c '"event_type":"DocumentReceived"' "$METRICS_RAW" 2>/dev/null || echo 0)
if [[ $DOC_RECEIVED_COUNT -gt 0 ]]; then
    echo "" >> "$REPORT_FILE"
    echo "DocumentReceived events: $DOC_RECEIVED_COUNT" >> "$REPORT_FILE"

    # Extract latencies
    grep '"event_type":"DocumentReceived"' "$METRICS_RAW" | \
        jq -r '.latency_ms // empty' 2>/dev/null | \
        sort -n > /tmp/doc_lat_$$.txt || true

    if [[ -s /tmp/doc_lat_$$.txt ]]; then
        P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) printf "%.2f", a[int(c*0.5)]; else print "N/A"}' /tmp/doc_lat_$$.txt)
        P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) printf "%.2f", a[int(c*0.95)]; else print "N/A"}' /tmp/doc_lat_$$.txt)
        P99=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) printf "%.2f", a[int(c*0.99)]; else print "N/A"}' /tmp/doc_lat_$$.txt)
        MIN=$(head -1 /tmp/doc_lat_$$.txt)
        MAX=$(tail -1 /tmp/doc_lat_$$.txt)
        AVG=$(awk '{sum+=$1; c++} END{if(c>0) printf "%.2f", sum/c; else print "N/A"}' /tmp/doc_lat_$$.txt)

        echo "  Min: ${MIN}ms  Avg: ${AVG}ms  P50: ${P50}ms  P95: ${P95}ms  P99: ${P99}ms  Max: ${MAX}ms" >> "$REPORT_FILE"
    fi
    rm -f /tmp/doc_lat_$$.txt
fi

# CRDTUpsert latencies (tier-specific)
for tier in "soldier" "squad_leader" "platoon_leader" "company_commander"; do
    TIER_COUNT=$(grep '"event_type":"CRDTUpsert"' "$METRICS_RAW" 2>/dev/null | grep -c "\"tier\":\"$tier\"" 2>/dev/null || true)
    TIER_COUNT=${TIER_COUNT:-0}
    if [[ "$TIER_COUNT" -gt 0 ]]; then
        echo "" >> "$REPORT_FILE"
        echo "CRDTUpsert ($tier): $TIER_COUNT events" >> "$REPORT_FILE"

        grep '"event_type":"CRDTUpsert"' "$METRICS_RAW" | \
            grep "\"tier\":\"$tier\"" | \
            jq -r '.latency_ms // empty' 2>/dev/null | \
            sort -n > /tmp/tier_lat_$$.txt || true

        if [[ -s /tmp/tier_lat_$$.txt ]]; then
            P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) printf "%.2f", a[int(c*0.5)]; else print "N/A"}' /tmp/tier_lat_$$.txt)
            P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) printf "%.2f", a[int(c*0.95)]; else print "N/A"}' /tmp/tier_lat_$$.txt)
            echo "  P50: ${P50}ms  P95: ${P95}ms" >> "$REPORT_FILE"
        fi
        rm -f /tmp/tier_lat_$$.txt
    fi
done

# Aggregation events
AGG_COUNT=$(grep -c '"event_type":"AggregationCompleted"' "$METRICS_RAW" 2>/dev/null || echo 0)
if [[ $AGG_COUNT -gt 0 ]]; then
    echo "" >> "$REPORT_FILE"
    echo "Aggregation events: $AGG_COUNT" >> "$REPORT_FILE"

    # Get input counts
    grep '"event_type":"AggregationCompleted"' "$METRICS_RAW" | \
        jq -r '.input_count // empty' 2>/dev/null | \
        sort -n | uniq -c | head -5 >> "$REPORT_FILE" || true
fi

echo "" >> "$REPORT_FILE"

# ============================================================================
# PHASE 3: Generate CSV summary
# ============================================================================
echo "Phase 3: Generating CSV summary..."

# Header
echo "NodeCount,Backend,Bandwidth,TotalMetrics,DocReceived_P50_ms,DocReceived_P95_ms,Status" > "$METRICS_CSV"

# Calculate values
DOC_P50="N/A"
DOC_P95="N/A"
STATUS="PARTIAL"

if [[ $DOC_RECEIVED_COUNT -gt 0 ]]; then
    grep '"event_type":"DocumentReceived"' "$METRICS_RAW" | \
        jq -r '.latency_ms // empty' 2>/dev/null | \
        sort -n > /tmp/csv_lat_$$.txt || true

    if [[ -s /tmp/csv_lat_$$.txt ]]; then
        DOC_P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) printf "%.2f", a[int(c*0.5)]; else print "N/A"}' /tmp/csv_lat_$$.txt)
        DOC_P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) printf "%.2f", a[int(c*0.95)]; else print "N/A"}' /tmp/csv_lat_$$.txt)
        STATUS="COMPLETE"
    fi
    rm -f /tmp/csv_lat_$$.txt
fi

echo "${NODE_COUNT},${BACKEND},${BANDWIDTH},${METRICS_COUNT},${DOC_P50},${DOC_P95},${STATUS}" >> "$METRICS_CSV"

# ============================================================================
# SUMMARY
# ============================================================================
echo ""
echo "========================================"
echo "Analysis Complete"
echo "========================================"
echo ""
echo "Output files:"
echo "  Raw metrics: $METRICS_RAW"
echo "  CSV summary: $METRICS_CSV"
echo "  Report: $REPORT_FILE"
echo ""
echo "Quick summary:"
cat "$METRICS_CSV"
echo ""
echo "Report preview:"
head -30 "$REPORT_FILE"
