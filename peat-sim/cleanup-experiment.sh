#!/bin/bash
# cleanup-experiment.sh - Standalone cleanup for lab experiments
#
# Use this after OOM or any other failure to clean up orphaned resources.
# Safe to run multiple times.
#
# Usage:
#   ./cleanup-experiment.sh              # Clean all lab4 resources
#   ./cleanup-experiment.sh --all        # Clean ALL containerlab resources
#   ./cleanup-experiment.sh --topo FILE  # Clean specific topology

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

CLEAN_ALL=false
TOPO_FILE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --all) CLEAN_ALL=true; shift ;;
        --topo) TOPO_FILE="$2"; shift 2 ;;
        -h|--help)
            echo "Usage: $0 [--all] [--topo FILE]"
            echo ""
            echo "Options:"
            echo "  --all       Clean ALL containerlab resources (not just lab4)"
            echo "  --topo FILE Destroy specific topology file"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "========================================"
echo "Experiment Cleanup"
echo "========================================"
echo ""

# Function to count resources
count_containers() {
    docker ps -aq --filter "name=$1" 2>/dev/null | wc -l
}

count_networks() {
    docker network ls --format '{{.Name}}' 2>/dev/null | grep -E "$1" | wc -l || echo 0
}

# If specific topology file provided, try graceful destroy first
if [[ -n "$TOPO_FILE" && -f "$TOPO_FILE" ]]; then
    echo "Destroying topology: $TOPO_FILE"
    timeout 60 containerlab destroy -t "$TOPO_FILE" --cleanup 2>/dev/null || {
        echo "Graceful destroy failed, will force cleanup"
    }
    echo ""
fi

# Show current state
echo "Current state:"
if $CLEAN_ALL; then
    CONTAINER_PATTERN="clab-"
    NETWORK_PATTERN="^clab-"
else
    CONTAINER_PATTERN="clab-lab4-"
    NETWORK_PATTERN="^lab4-"
fi

CONTAINERS=$(count_containers "$CONTAINER_PATTERN")
NETWORKS=$(count_networks "$NETWORK_PATTERN")
echo "  Containers matching '$CONTAINER_PATTERN': $CONTAINERS"
echo "  Networks matching '$NETWORK_PATTERN': $NETWORKS"
echo ""

# Don't exit early - still need to check for orphaned processes

# Stop and remove containers
if [[ $CONTAINERS -gt 0 ]]; then
    echo "Stopping containers..."
    docker ps -q --filter "name=$CONTAINER_PATTERN" | xargs -r docker stop --time 5 2>/dev/null || true

    echo "Removing containers..."
    docker ps -aq --filter "name=$CONTAINER_PATTERN" | xargs -r docker rm -f 2>/dev/null || true

    REMAINING=$(count_containers "$CONTAINER_PATTERN")
    echo "  Containers remaining: $REMAINING"
fi

# Remove networks
if [[ $NETWORKS -gt 0 ]]; then
    echo "Removing networks..."
    docker network ls --format '{{.Name}}' | grep -E "$NETWORK_PATTERN" | xargs -r docker network rm 2>/dev/null || true

    REMAINING=$(count_networks "$NETWORK_PATTERN")
    echo "  Networks remaining: $REMAINING"
fi

# Prune any dangling resources
echo ""
echo "Pruning dangling resources..."
docker network prune -f 2>/dev/null || true

# Final state
echo ""
echo "Final state:"
CONTAINERS=$(count_containers "$CONTAINER_PATTERN")
NETWORKS=$(count_networks "$NETWORK_PATTERN")
echo "  Containers: $CONTAINERS"
echo "  Networks: $NETWORKS"

if [[ $CONTAINERS -eq 0 && $NETWORKS -eq 0 ]]; then
    echo ""
    echo "Cleanup complete."
else
    echo ""
    echo "WARNING: Some resources remain. You may need to run with --all or check manually."
    echo ""
    echo "Remaining containers:"
    docker ps -a --filter "name=$CONTAINER_PATTERN" --format "  {{.Names}}: {{.Status}}"
    echo ""
    echo "Remaining networks:"
    docker network ls --format '{{.Name}}' | grep -E "$NETWORK_PATTERN" | sed 's/^/  /'
fi

# Check for orphaned peat-sim processes
echo ""
echo "Checking for orphaned peat-sim processes..."
ORPHAN_COUNT=$(pgrep -f "/usr/local/bin/peat-sim" 2>/dev/null | wc -l || echo 0)

if [[ $ORPHAN_COUNT -gt 0 ]]; then
    echo "  Found $ORPHAN_COUNT orphaned peat-sim processes"
    echo "  Process details:"
    pgrep -a -f "/usr/local/bin/peat-sim" 2>/dev/null | head -5 | while read pid cmd; do
        echo "    PID $pid: $(echo $cmd | cut -c1-80)..."
    done

    # Try to kill them (may need sudo for root-owned processes)
    if pkill -f "/usr/local/bin/peat-sim" 2>/dev/null; then
        sleep 1
        REMAINING=$(pgrep -f "/usr/local/bin/peat-sim" 2>/dev/null | wc -l || echo 0)
        if [[ $REMAINING -gt 0 ]]; then
            echo "  Graceful kill didn't work, trying SIGKILL..."
            pkill -9 -f "/usr/local/bin/peat-sim" 2>/dev/null || true
            sleep 1
            REMAINING=$(pgrep -f "/usr/local/bin/peat-sim" 2>/dev/null | wc -l || echo 0)
        fi
        echo "  Killed $(($ORPHAN_COUNT - $REMAINING)) processes, $REMAINING remaining"

        if [[ $REMAINING -gt 0 ]]; then
            echo ""
            echo "WARNING: $REMAINING processes still running (may be owned by root)"
            echo "  Run with sudo to kill root-owned processes:"
            echo "    sudo pkill -9 -f '/usr/local/bin/peat-sim'"
        fi
    else
        echo ""
        echo "WARNING: Could not kill processes (may be owned by root)"
        echo "  Run with sudo to kill root-owned processes:"
        echo "    sudo pkill -9 -f '/usr/local/bin/peat-sim'"
    fi
else
    echo "  No orphaned processes found"
fi

# Show memory reclaimed
echo ""
echo "System memory:"
free -h | grep "^Mem:"
