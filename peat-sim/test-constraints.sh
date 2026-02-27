#!/bin/bash
# Test network constraints with ContainerLab
#
# This script:
# 1. Deploys constrained topology
# 2. Applies network impairments (56 Kbps, 50ms latency, 1% loss)
# 3. Waits for sync to complete
# 4. Collects timing data

set -e

cd "$(dirname "$0")"

echo "===== E8.1 Network Constraint Validation Test ====="
echo ""

# Load environment variables
set -a
source ../.env
set +a

# Deploy topology
echo "[1/5] Deploying topology..."
containerlab deploy -t topologies/poc-2node-constrained.yaml

echo ""
echo "[2/5] Waiting for containers to start (3s)..."
sleep 3

# Apply network constraints to both nodes
echo ""
echo "[3/5] Applying network constraints..."
echo "  - Bandwidth: 56 Kbps (tactical radio)"
echo "  - Latency: 50ms + 10ms jitter"
echo "  - Packet Loss: 1%"

containerlab tools netem set -n clab-cap-poc-2node-constrained-node1 -i eth0 \
  --delay 50ms --jitter 10ms --loss 1 --rate 56

containerlab tools netem set -n clab-cap-poc-2node-constrained-node2 -i eth0 \
  --delay 50ms --jitter 10ms --loss 1 --rate 56

echo ""
echo "[4/5] Waiting for sync to complete (30s)..."
echo "  Monitoring logs for sync completion..."

# Wait and monitor
START_TIME=$(date +%s)
MAX_WAIT=30
COMPLETED=false

for i in $(seq 1 $MAX_WAIT); do
  sleep 1
  # Check if node2 completed
  if docker logs clab-cap-poc-2node-constrained-node2 2>&1 | grep -q "POC SUCCESS"; then
    END_TIME=$(date +%s)
    ELAPSED=$((END_TIME - START_TIME))
    echo ""
    echo "✓ Sync completed in ${ELAPSED} seconds"
    COMPLETED=true
    break
  fi
  printf "."
done

echo ""

if [ "$COMPLETED" = false ]; then
  echo "✗ Sync did not complete within ${MAX_WAIT} seconds"
  echo ""
  echo "Node1 logs:"
  docker logs clab-cap-poc-2node-constrained-node1 2>&1 | tail -10
  echo ""
  echo "Node2 logs:"
  docker logs clab-cap-poc-2node-constrained-node2 2>&1 | tail -10

  # Clean up
  containerlab destroy -t topologies/poc-2node-constrained.yaml
  exit 1
fi

# Extract detailed timing from logs
echo ""
echo "[5/5] Collecting results..."
echo ""
echo "===== Node1 (Writer) Logs ====="
docker logs clab-cap-poc-2node-constrained-node1 2>&1 | grep -v "INFO\|ERROR" | tail -15

echo ""
echo "===== Node2 (Reader) Logs ====="
docker logs clab-cap-poc-2node-constrained-node2 2>&1 | grep -v "INFO\|ERROR" | tail -15

echo ""
echo "===== Applied Constraints ====="
echo "Node1:"
containerlab tools netem show -n clab-cap-poc-2node-constrained-node1
echo ""
echo "Node2:"
containerlab tools netem show -n clab-cap-poc-2node-constrained-node2

echo ""
echo "===== Test Complete ====="
echo "Result: ✓ SUCCESS"
echo "Sync Time with Constraints: ${ELAPSED} seconds"
echo ""
echo "Comparison:"
echo "  - Baseline (unconstrained): ~1-2 seconds"
echo "  - With constraints: ${ELAPSED} seconds"
if [ $ELAPSED -gt 3 ]; then
  echo "  → Network constraints ARE affecting Ditto traffic ✓"
else
  echo "  → WARNING: Sync time similar to baseline (constraints may not be working)"
fi

echo ""
echo "Clean up with: containerlab destroy -t topologies/poc-2node-constrained.yaml"
