#!/bin/bash
# Phase 0 Quick Start — runs crane agent locally (no Docker needed)
#
# Prerequisites:
#   pip install mcp anthropic openai
#   export ANTHROPIC_API_KEY=sk-...
#
# Usage:
#   ./run-phase0.sh                          # Claude API (default)
#   ./run-phase0.sh --provider ollama        # Local Ollama
#   ./run-phase0.sh --max-cycles 5           # Quick test (5 cycles)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Add source paths
export PYTHONPATH="${SCRIPT_DIR}/bridge/src:${SCRIPT_DIR}/agent/src:${PYTHONPATH}"

echo "============================================"
echo "  HIVE Port Operations — Phase 0"
echo "  Single Crane Agent Proof of Concept"
echo "============================================"
echo ""
echo "This runs a Gantry Crane agent that:"
echo "  1. OBSERVES HIVE state via MCP resources"
echo "  2. ORIENTS using LLM reasoning"
echo "  3. DECIDES on an action (lift, wait, report)"
echo "  4. ACTS via MCP tools (updates HIVE state)"
echo "  5. Repeats on simulation clock"
echo ""

# Default args
ARGS="--node-id crane-1 --persona gantry-crane"

# Pass through any CLI args
ARGS="${ARGS} $@"

# Set defaults if not overridden
if [[ ! "$*" == *"--max-cycles"* ]]; then
    ARGS="${ARGS} --max-cycles 10"
fi

if [[ ! "$*" == *"--time-compression"* ]]; then
    ARGS="${ARGS} --time-compression 600"  # 10x faster for demo (1 real sec = 10 sim min)
fi

echo "Running: python -m port_agent.main ${ARGS}"
echo ""

python -m port_agent.main ${ARGS}
