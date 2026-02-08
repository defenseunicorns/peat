#!/bin/bash
# Phase 1a — Multi-Agent HIVE Simulation
#
# Runs 2 gantry cranes + 1 hold aggregator sharing a single HIVE state store.
# All agents run as asyncio tasks in one Python process.
#
# Usage:
#   ./run-phase1a.sh                          # Default (dry-run, 15 cycles)
#   ./run-phase1a.sh --max-cycles 10          # Shorter run
#   ./run-phase1a.sh --provider anthropic     # Use Claude API

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Add source paths
export PYTHONPATH="${SCRIPT_DIR}/bridge/src:${SCRIPT_DIR}/agent/src:${PYTHONPATH}"

echo "============================================"
echo "  HIVE Port Operations — Phase 1a"
echo "  Multi-Agent Simulation (2 cranes + 1 agg)"
echo "============================================"
echo ""
echo "This runs 3 agents sharing one HIVE state store:"
echo "  crane-1:    Gantry crane (processes containers)"
echo "  crane-2:    Gantry crane (processes containers)"
echo "  hold-agg-3: Hold aggregator (team summaries)"
echo ""
echo "Watch for:"
echo "  - Queue contention (failed claims)"
echo "  - Queue splitting (both cranes active)"
echo "  - Hazmat escalation from both cranes"
echo "  - Aggregator summaries with computed rate"
echo ""

# Default args
ARGS="--mode multi --agents 2c1a --provider dry-run"

# Pass through any CLI args
ARGS="${ARGS} $@"

# Set defaults if not overridden
if [[ ! "$*" == *"--max-cycles"* ]]; then
    ARGS="${ARGS} --max-cycles 15"
fi

if [[ ! "$*" == *"--time-compression"* ]]; then
    ARGS="${ARGS} --time-compression 600"
fi

echo "Running: python -m port_agent.main ${ARGS}"
echo ""

python -m port_agent.main ${ARGS}
