#!/bin/bash
#
# Quick validation test for battalion-48node-mesh-mode4.yaml topology
# Tests that the new topology produces valid DocumentReceived metrics
#

set -euo pipefail

echo "========================================================"
echo "Battalion 48-Node Mesh-Mode4 Topology Validation"
echo "========================================================"
echo ""

# Change to cap root directory to source .env
cd "$(dirname "$0")/../../../.."
CAP_ROOT=$(pwd)

# Source Ditto credentials from .env file
if [ -f .env ]; then
  echo "→ Loading Ditto credentials from .env..."
  set -a  # Export all variables
  source .env
  set +a  # Stop exporting

  # Verify credentials are set
  if [ -z "${DITTO_APP_ID:-}" ] || [ -z "${DITTO_OFFLINE_TOKEN:-}" ] || [ -z "${DITTO_SHARED_KEY:-}" ]; then
    echo "✗ ERROR: Ditto credentials not found in .env file"
    echo "  Required: DITTO_APP_ID, DITTO_OFFLINE_TOKEN, DITTO_SHARED_KEY"
    exit 1
  fi
  echo "  ✓ Ditto credentials loaded"
else
  echo "✗ ERROR: .env file not found at ${CAP_ROOT}/.env"
  exit 1
fi

# Create validation results directory
cd "${CAP_ROOT}/labs/e12-comprehensive-empirical-validation/scripts"
VALIDATION_DIR="battalion-48node-validation-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$VALIDATION_DIR"

echo ""
echo "→ Deploying 48-node battalion topology with mesh-mode4..."
echo ""

# Deploy topology
containerlab deploy \
  --topo ../../../peat-sim/topologies/battalion-48node-mesh-mode4.yaml \
  --reconfigure

echo ""
echo "→ Waiting 40 seconds for warmup..."
sleep 40

echo "→ Collecting metrics for 90 seconds..."
sleep 90

echo ""
echo "→ Collecting container logs..."

# Collect logs from all containers
for container in $(docker ps --filter "name=clab-cap-battalion-48node-mode4-mesh-" --format "{{.Names}}"); do
  docker logs "$container" > "${VALIDATION_DIR}/${container}.log" 2>&1
done

echo "→ Extracting metrics..."

# Extract all JSONL metrics from logs
cat "${VALIDATION_DIR}"/*.log | grep -E '^\{' | grep '"event_type"' > "${VALIDATION_DIR}/all-metrics.jsonl" || true

echo "→ Analyzing metrics..."
echo ""

# Count event types
echo "Event Type Distribution:"
jq -r '.event_type' "${VALIDATION_DIR}/all-metrics.jsonl" 2>/dev/null | sort | uniq -c || echo "No metrics found"

echo ""
echo "Document Types (first 30):"
jq -r 'select(.event_type=="DocumentReceived") | .doc_id' "${VALIDATION_DIR}/all-metrics.jsonl" 2>/dev/null | head -30 || echo "No DocumentReceived events"

echo ""
echo "Squad Summary Documents:"
jq -r 'select(.event_type=="DocumentReceived" and (.doc_id | contains("squad-") and contains("-summary"))) | .doc_id' "${VALIDATION_DIR}/all-metrics.jsonl" 2>/dev/null | sort | uniq || echo "No squad summary documents"

echo ""
echo "Platoon Summary Documents:"
jq -r 'select(.event_type=="DocumentReceived" and (.doc_id | contains("platoon-") and contains("-summary"))) | .doc_id' "${VALIDATION_DIR}/all-metrics.jsonl" 2>/dev/null | sort | uniq || echo "No platoon summary documents"

echo ""
echo "Battalion Summary Documents:"
jq -r 'select(.event_type=="DocumentReceived" and (.doc_id | contains("battalion-") and contains("-summary"))) | .doc_id' "${VALIDATION_DIR}/all-metrics.jsonl" 2>/dev/null | sort | uniq || echo "No battalion summary documents"

echo ""
echo "Latency Statistics:"
jq -s 'map(select(.event_type=="DocumentReceived") | .latency_ms) | {count: length, min: min, max: max, avg: (add/length)}' "${VALIDATION_DIR}/all-metrics.jsonl" 2>/dev/null || echo "No latency data"

echo ""
echo "→ Destroying topology..."
containerlab destroy --topo ../../../peat-sim/topologies/battalion-48node-mesh-mode4.yaml --cleanup

echo ""
echo "========================================================"
echo "Validation Complete"
echo "========================================================"
echo "Results saved to: ${VALIDATION_DIR}/"
echo ""

# Check for success
doc_received_count=$(jq -r 'select(.event_type=="DocumentReceived")' "${VALIDATION_DIR}/all-metrics.jsonl" 2>/dev/null | wc -l || echo "0")
squad_summary_count=$(jq -r 'select(.event_type=="DocumentReceived" and (.doc_id | contains("squad-") and contains("-summary")))' "${VALIDATION_DIR}/all-metrics.jsonl" 2>/dev/null | wc -l || echo "0")

if [ "$doc_received_count" -gt 0 ]; then
  echo "✓ SUCCESS: Found ${doc_received_count} DocumentReceived events"
  if [ "$squad_summary_count" -gt 0 ]; then
    echo "✓ SUCCESS: Found ${squad_summary_count} squad summary documents"
    echo "✓ Battalion 48-node mesh-mode4 topology is working correctly"
    exit 0
  else
    echo "⚠ WARNING: No squad summary documents found"
    echo "  This may indicate hierarchical aggregation is not working properly"
    exit 1
  fi
else
  echo "✗ FAILURE: No DocumentReceived events found"
  echo "✗ Battalion topology may have issues"
  exit 1
fi
