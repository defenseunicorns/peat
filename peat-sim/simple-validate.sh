#!/usr/bin/env bash
# simple-validate.sh - Quick validation smoke test for peat-sim
#
# Deploys the am24n topology (24 nodes, 26 containers, Automerge backend),
# waits for convergence, checks for errors, and tears down.
#
# Exit 0 on PASS, 1 on FAIL.

set -euo pipefail

TOPOLOGY="topologies/am24n.yaml"
TOPO_NAME="am24n"
EXPECTED_CONTAINERS=26
WAIT_SECONDS=30
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cd "$SCRIPT_DIR"

# Colors (disabled if not a terminal)
if [ -t 1 ]; then
    GREEN='\033[0;32m'
    RED='\033[0;31m'
    YELLOW='\033[1;33m'
    NC='\033[0m'
else
    GREEN='' RED='' YELLOW='' NC=''
fi

pass() { echo -e "${GREEN}[PASS]${NC} $1"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; }
info() { echo -e "${YELLOW}[INFO]${NC} $1"; }

FAILED=0

cleanup() {
    info "Destroying topology..."
    sudo containerlab destroy -t "$TOPOLOGY" --cleanup 2>/dev/null || true
}

# Always clean up on exit
trap cleanup EXIT

# --- Deploy ---
info "Deploying $TOPOLOGY ($EXPECTED_CONTAINERS containers expected)..."
if ! sudo containerlab deploy -t "$TOPOLOGY" --reconfigure; then
    fail "ContainerLab deploy failed"
    exit 1
fi

# --- Wait for convergence ---
info "Waiting ${WAIT_SECONDS}s for nodes to converge..."
sleep "$WAIT_SECONDS"

# --- Check 1: All containers running ---
RUNNING=$(docker ps --filter "name=clab-${TOPO_NAME}" --filter "status=running" -q | wc -l)
if [ "$RUNNING" -ge "$EXPECTED_CONTAINERS" ]; then
    pass "All containers running ($RUNNING/$EXPECTED_CONTAINERS)"
else
    fail "Only $RUNNING/$EXPECTED_CONTAINERS containers running"
    FAILED=1
fi

# --- Check 2: No panics or fatal errors ---
PANIC_COUNT=0
for container in $(docker ps --filter "name=clab-${TOPO_NAME}" --format '{{.Names}}'); do
    PANICS=$(docker logs "$container" 2>&1 | grep -ciE "panic|fatal|segfault|SIGSEGV" || true)
    if [ "$PANICS" -gt 0 ]; then
        fail "Container $container has $PANICS panic/fatal messages"
        docker logs "$container" 2>&1 | grep -iE "panic|fatal|segfault|SIGSEGV" | head -3
        PANIC_COUNT=$((PANIC_COUNT + PANICS))
    fi
done
if [ "$PANIC_COUNT" -eq 0 ]; then
    pass "No panics or fatal errors in any container"
else
    FAILED=1
fi

# --- Check 3: Sync activity detected ---
SYNC_NODES=0
for container in $(docker ps --filter "name=clab-${TOPO_NAME}" --format '{{.Names}}'); do
    HAS_SYNC=$(docker logs "$container" 2>&1 | grep -ciE "sync|merge|replicate|AggregationCompleted|DocumentReceived" || true)
    if [ "$HAS_SYNC" -gt 0 ]; then
        SYNC_NODES=$((SYNC_NODES + 1))
    fi
done
if [ "$SYNC_NODES" -gt 0 ]; then
    pass "Sync activity detected on $SYNC_NODES/$RUNNING nodes"
else
    fail "No sync activity detected on any node"
    FAILED=1
fi

# --- Summary ---
echo ""
if [ "$FAILED" -eq 0 ]; then
    pass "Validation PASSED"
    exit 0
else
    fail "Validation FAILED"
    exit 1
fi
