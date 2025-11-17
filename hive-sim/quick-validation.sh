#!/bin/bash
# Quick validation test for hive-sim with DittoStore -summary fix
# Tests 24-node Mode 4 hierarchical topology

set -e

RESULTS_DIR="quick-validation-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo "=== Quick Validation: 24-node Mode 4 Hierarchical ===" | tee "$RESULTS_DIR/test.log"
echo "Testing DittoStore -summary suffix fix" | tee -a "$RESULTS_DIR/test.log"
echo "Start time: $(date)" | tee -a "$RESULTS_DIR/test.log"

# Load environment variables from parent directory
if [ -f ../.env ]; then
    export $(grep -v '^#' ../.env | xargs)
elif [ -f .env ]; then
    export $(grep -v '^#' .env | xargs)
fi

# Check required env vars
if [ -z "$DITTO_APP_ID" ] || [ -z "$DITTO_OFFLINE_TOKEN" ] || [ -z "$DITTO_SHARED_KEY" ]; then
    echo "ERROR: Missing required Ditto environment variables" | tee -a "$RESULTS_DIR/test.log"
    exit 1
fi

# Clean up any existing deployments
echo "Cleaning up existing deployments..." | tee -a "$RESULTS_DIR/test.log"
containerlab destroy --all --cleanup 2>&1 | head -10 | tee -a "$RESULTS_DIR/test.log"
sleep 5

# Deploy 24-node hierarchical topology
TOPOLOGY="topologies/platoon-24node-mesh-mode4.yaml"
echo "Deploying topology: $TOPOLOGY" | tee -a "$RESULTS_DIR/test.log"

containerlab deploy -t "$TOPOLOGY" 2>&1 | tee -a "$RESULTS_DIR/test.log"

# Wait for warmup
WARMUP=30
echo "Warmup period: ${WARMUP}s..." | tee -a "$RESULTS_DIR/test.log"
sleep $WARMUP

# Observation period
OBSERVE=60
echo "Observation period: ${OBSERVE}s..." | tee -a "$RESULTS_DIR/test.log"
sleep $OBSERVE

# Collect logs from all containers
echo "Collecting container logs..." | tee -a "$RESULTS_DIR/test.log"
for container in $(docker ps --filter "name=clab-hive-" --format "{{.Names}}"); do
    echo "Collecting logs from $container" | tee -a "$RESULTS_DIR/test.log"
    docker logs "$container" 2>&1 > "$RESULTS_DIR/${container}.log"
done

# Aggregate metrics
echo "Aggregating metrics..." | tee -a "$RESULTS_DIR/test.log"
cat "$RESULTS_DIR"/clab-*.log | grep -E '^\{.*\}$' > "$RESULTS_DIR/all-metrics.jsonl" || echo "No JSONL metrics found"

# Quick analysis
if [ -f "$RESULTS_DIR/all-metrics.jsonl" ]; then
    echo "" | tee -a "$RESULTS_DIR/test.log"
    echo "=== Event Type Counts ===" | tee -a "$RESULTS_DIR/test.log"
    jq -r '.event_type' "$RESULTS_DIR/all-metrics.jsonl" 2>/dev/null | sort | uniq -c | tee -a "$RESULTS_DIR/test.log"

    echo "" | tee -a "$RESULTS_DIR/test.log"
    echo "=== Summary Document Check ===" | tee -a "$RESULTS_DIR/test.log"

    # Check for SquadSummaryCreated events
    SQUAD_SUMMARY_COUNT=$(grep -c '"event_type":"SquadSummaryCreated"' "$RESULTS_DIR/all-metrics.jsonl" || echo "0")
    echo "SquadSummaryCreated events: $SQUAD_SUMMARY_COUNT" | tee -a "$RESULTS_DIR/test.log"

    # Check for PlatoonSummaryCreated events
    PLATOON_SUMMARY_COUNT=$(grep -c '"event_type":"PlatoonSummaryCreated"' "$RESULTS_DIR/all-metrics.jsonl" || echo "0")
    echo "PlatoonSummaryCreated events: $PLATOON_SUMMARY_COUNT" | tee -a "$RESULTS_DIR/test.log"

    # Check for DocumentReceived events with -summary suffix
    SUMMARY_DOC_COUNT=$(jq -r 'select(.event_type=="DocumentReceived") | .doc_id' "$RESULTS_DIR/all-metrics.jsonl" 2>/dev/null | grep -c "\-summary" || echo "0")
    echo "DocumentReceived events with -summary suffix: $SUMMARY_DOC_COUNT" | tee -a "$RESULTS_DIR/test.log"

    echo "" | tee -a "$RESULTS_DIR/test.log"
    if [ "$SQUAD_SUMMARY_COUNT" -gt 0 ] && [ "$SUMMARY_DOC_COUNT" -gt 0 ]; then
        echo "✅ SUCCESS: Summary documents are being created correctly!" | tee -a "$RESULTS_DIR/test.log"
    else
        echo "❌ FAIL: Summary documents NOT being created" | tee -a "$RESULTS_DIR/test.log"
        echo "  Expected SquadSummaryCreated > 0 and DocumentReceived with -summary > 0" | tee -a "$RESULTS_DIR/test.log"
    fi
fi

# Cleanup
echo "" | tee -a "$RESULTS_DIR/test.log"
echo "Cleaning up..." | tee -a "$RESULTS_DIR/test.log"
containerlab destroy --all --cleanup 2>&1 | head -10 | tee -a "$RESULTS_DIR/test.log"

echo "" | tee -a "$RESULTS_DIR/test.log"
echo "End time: $(date)" | tee -a "$RESULTS_DIR/test.log"
echo "Results saved to: $RESULTS_DIR" | tee -a "$RESULTS_DIR/test.log"
