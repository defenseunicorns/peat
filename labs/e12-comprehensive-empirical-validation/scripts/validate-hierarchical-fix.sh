#!/bin/bash
set -e

echo "=== CAP Hierarchical Fix Validation ==="
echo "Testing: 24-node hierarchical topology with fix"
echo ""

# Source environment from cap-sim directory
if [ -f ../../../cap-sim/.env ]; then
    set -a
    source ../../../cap-sim/.env
    set +a
else
    echo "ERROR: No .env file found in cap-sim directory"
    exit 1
fi

VALIDATION_DIR="validation-hierarchical-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$VALIDATION_DIR"

echo "Results will be saved to: $VALIDATION_DIR"
echo ""

# Deploy topology
echo "Step 1: Deploying CAP Hierarchical topology..."
TOPOLOGY="../../../cap-sim/topologies/platoon-24node-client-server-mode4.yaml"

containerlab deploy -t "$TOPOLOGY" --reconfigure 2>&1 | tee "$VALIDATION_DIR/deploy.log"

echo ""
echo "Step 2: Waiting 20 seconds for test to complete..."
sleep 20

echo ""
echo "Step 3: Collecting logs and metrics..."

# Collect all-metrics.jsonl from each node
for container in $(docker ps --filter "name=clab-cap-platoon" --format "{{.Names}}"); do
    node_name=$(echo "$container" | sed 's/clab-cap-platoon-mode4-client-server-//')
    echo "  Collecting metrics from $node_name..."
    docker exec "$container" cat /data/all-metrics.jsonl > "$VALIDATION_DIR/${node_name}-metrics.jsonl" 2>/dev/null || true
    docker logs "$container" > "$VALIDATION_DIR/${node_name}.log" 2>&1
done

echo ""
echo "Step 4: Analyzing results..."

# Count event types
echo ""
echo "=== Event Type Summary ==="
echo ""

echo "DocumentReceived events:"
grep -h '"event_type":"DocumentReceived"' "$VALIDATION_DIR"/*-metrics.jsonl 2>/dev/null | wc -l

echo "DocumentInserted events:"
grep -h '"event_type":"DocumentInserted"' "$VALIDATION_DIR"/*-metrics.jsonl 2>/dev/null | wc -l

echo "MessageSent events:"
grep -h '"event_type":"MessageSent"' "$VALIDATION_DIR"/*-metrics.jsonl 2>/dev/null | wc -l

echo ""
echo "=== Sample DocumentReceived Events ==="
grep -h '"event_type":"DocumentReceived"' "$VALIDATION_DIR"/*-metrics.jsonl 2>/dev/null | head -3 | jq .

echo ""
echo "=== Reader Node Status ==="
echo "Checking if readers received documents..."
echo ""

# Check a few reader nodes
for node in alpha-soldier-1 bravo-soldier-1 charlie-soldier-1; do
    echo "  $node:"
    if grep -q "Test document received" "$VALIDATION_DIR/${node}.log" 2>/dev/null; then
        echo "    ✓ Test document received"
        latency=$(grep "Test document received" "$VALIDATION_DIR/${node}.log" | grep -oP 'latency: \K[0-9.]+')
        echo "    ✓ Latency: ${latency}ms"
    else
        echo "    ✗ Test document NOT received"
    fi
done

echo ""
echo "Step 5: Cleaning up..."
containerlab destroy -t "$TOPOLOGY" --cleanup 2>&1 > /dev/null

echo ""
echo "=== Validation Complete ==="
echo "Results saved to: $VALIDATION_DIR"
echo ""

# Summary
RECEIVED_COUNT=$(grep -h '"event_type":"DocumentReceived"' "$VALIDATION_DIR"/*-metrics.jsonl 2>/dev/null | wc -l)

if [ "$RECEIVED_COUNT" -gt 0 ]; then
    echo "✓ SUCCESS: Fix verified - readers are receiving documents!"
    echo "  DocumentReceived events logged: $RECEIVED_COUNT"
    exit 0
else
    echo "✗ FAILURE: No DocumentReceived events found"
    echo "  Check logs in $VALIDATION_DIR for details"
    exit 1
fi
